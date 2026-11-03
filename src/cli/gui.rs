//! Small local desktop client. Status reads use existing daemon snapshots;
//! GUI-owned chats use a normal client connection and no terminal UI.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::ServerEvent;
use crate::server::Client;

#[path = "../alphacode_app_core/server/debug_sessions_index.rs"]
mod session_index;

#[path = "gui/provider_connections.rs"]
mod provider_connections;

const OWN_CHAT: &str = "__companion__";
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_REPLY_BYTES: usize = 24 * 1024;

#[derive(Clone, Debug, Serialize)]
struct CompanionSession {
    id: String,
    title: String,
    state: String,
    detail: String,
    saved: bool,
    /// The workspace this live session is attached to.  The desktop uses this
    /// when the user explicitly carries a live workspace into a new companion
    /// chat; keeping it in the bridge row avoids guessing from the title.
    #[serde(skip_serializing_if = "Option::is_none")]
    working_dir: Option<String>,
    #[serde(skip)]
    socket: Option<PathBuf>,
}

#[derive(Default)]
struct OwnedChat {
    session_id: Option<String>,
    busy: bool,
    error: Option<String>,
    detail: String,
    reply: String,
    transcript: String,
    title: Option<String>,
    saved: bool,
    input_request: Option<CommandInput>,
    /// Workspace the next owned turn should subscribe in. Reset by `new_chat`.
    working_dir: Option<String>,
}

#[derive(Clone, Serialize)]
struct CommandInput {
    request_id: String,
    prompt: String,
    is_password: bool,
}

#[derive(Deserialize)]
struct BridgeRequest {
    action: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    auth_method: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    api_base: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    /// Custom title for `rename`, or bookmark label for `save`.
    #[serde(default)]
    title: Option<String>,
    /// Working directory for `new_chat`.
    #[serde(default, alias = "cwd")]
    working_dir: Option<String>,
    /// `archive` with `saved=false` removes the bookmark.
    #[serde(default)]
    saved: Option<bool>,
    /// Maximum rows for `sessions` (0 = server default).
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    include_workflow: bool,
}

struct DesktopInput {
    content: String,
    accepted: oneshot::Sender<Result<(), String>>,
}

#[derive(Default)]
struct Bridge {
    connections: provider_connections::ProviderConnections,
    owned: Arc<Mutex<OwnedChat>>,
    prompt_tx: Option<mpsc::Sender<DesktopInput>>,
    cancel_tx: Option<mpsc::Sender<()>>,
    sessions: BTreeMap<String, CompanionSession>,
    stored_ids: HashSet<String>,
    activity: Vec<String>,
    models: Vec<String>,
    model: String,
    provider: String,
    effort: String,
    runtime_for: Option<String>,
    workflow_requested: bool,
}

fn display_text(text: &str, limit: usize) -> String {
    crate::message::strip_ansi_escape_sequences(text)
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .take(limit)
        .collect()
}

fn append_reply(reply: &mut String, text: &str) {
    reply.push_str(text);
    if reply.len() > MAX_REPLY_BYTES {
        let mut start = reply.len() - MAX_REPLY_BYTES;
        while !reply.is_char_boundary(start) {
            start += 1;
        }
        reply.drain(..start);
    }
}

fn session_state(processing: bool, status: &str) -> &'static str {
    let status = status.trim().to_ascii_lowercase();
    if [
        "error",
        "failed",
        "blocked",
        "waiting",
        "needs_input",
        "cancelled",
        "stopped",
        "timeout",
    ]
    .contains(&status.as_str())
    {
        "attention"
    } else if processing || status == "running" || status == "working" || status == "processing" {
        "working"
    } else {
        "ready"
    }
}

async fn server_sessions(_debug_socket: PathBuf, socket: PathBuf) -> Result<Vec<CompanionSession>> {
    let mut client = Client::connect_with_path(socket.clone()).await?;
    let request_id = client.desktop_command("sessions", None, None).await?;
    loop {
        match client.read_event().await? {
            ServerEvent::DebugResponse { id, ok, output } if id == request_id => {
                if !ok {
                    bail!("{output}");
                }
                let sessions: Vec<Value> = serde_json::from_str(&output)?;
                return Ok(sessions
                    .into_iter()
                    .filter_map(|session| {
                        let id = session.get("session_id")?.as_str()?.to_string();
                        let directory = session
                            .get("working_dir")
                            .and_then(Value::as_str)
                            .and_then(|path| std::path::Path::new(path).file_name())
                            .and_then(|name| name.to_str());
                        let title = session
                            .get("friendly_name")
                            .and_then(Value::as_str)
                            .filter(|name| !name.trim().is_empty())
                            .or(directory)
                            .unwrap_or(&id);
                        Some(CompanionSession {
                            title: display_text(title, 80),
                            state: session_state(
                                session
                                    .get("is_processing")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                                session.get("status").and_then(Value::as_str).unwrap_or(""),
                            )
                            .to_string(),
                            detail: display_text(
                                session
                                    .get("detail")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Ready for a prompt"),
                                240,
                            ),
                            working_dir: session
                                .get("working_dir")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            saved: session
                                .get("saved")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            id,
                            socket: Some(socket.clone()),
                        })
                    })
                    .collect());
            }
            ServerEvent::Error { message, .. } => bail!("{message}"),
            _ => {}
        }
    }
}

impl Bridge {
    fn ensure_owned_chat(&mut self) {
        if self
            .prompt_tx
            .as_ref()
            .is_some_and(|sender| !sender.is_closed())
        {
            return;
        }
        let (tx, rx) = mpsc::channel(1);
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        self.prompt_tx = Some(tx);
        self.cancel_tx = Some(cancel_tx);
        let state = Arc::clone(&self.owned);
        tokio::spawn(async move {
            if let Err(error) = run_owned_chat(Arc::clone(&state), rx, cancel_rx).await {
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                state.busy = false;
                state.input_request = None;
                state.error = Some(format!("{error:#}"));
            }
        });
    }

    async fn submit_dictate(&self) -> Result<Value> {
        let run = crate::dictation::run_configured().await?;
        Ok(
            json!({"ok": true, "text": display_text(&run.text, MAX_PROMPT_BYTES), "mode": format!("{:?}", run.mode).to_lowercase()}),
        )
    }
    async fn desktop_output(
        &self,
        action: &str,
        selected: Option<&str>,
        value: Option<&str>,
    ) -> Result<Value> {
        let owned_id = self
            .owned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .session_id
            .clone();
        let id = match selected {
            Some(id) if id != OWN_CHAT => Some(id.to_string()),
            _ => owned_id,
        };
        let socket = selected
            .filter(|id| *id != OWN_CHAT)
            .and_then(|id| self.sessions.get(id))
            .and_then(|session| session.socket.clone())
            .unwrap_or_else(crate::server::socket_path);
        let mut client = Client::connect_with_path(socket).await?;
        let request_id = client.desktop_command(action, id.as_deref(), value).await?;
        loop {
            match client.read_event().await? {
                ServerEvent::DebugResponse { id, ok, output } if id == request_id => {
                    if !ok {
                        bail!(output);
                    }
                    return serde_json::from_str(&output).context("Invalid desktop response");
                }
                ServerEvent::Error { id, message, .. } if id == request_id => bail!(message),
                _ => {}
            }
        }
    }

    async fn selected_reply(&self, selected: Option<&str>) -> Option<String> {
        let id = selected.filter(|id| *id != OWN_CHAT)?;
        let value = tokio::time::timeout(
            Duration::from_millis(1200),
            self.desktop_output("history", Some(id), None),
        )
        .await
        .ok()?
        .ok()?;
        value
            .get("messages")?
            .as_array()?
            .iter()
            .rev()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
            .and_then(|message| message.get("content").and_then(Value::as_str))
            .map(|text| display_text(text, 8000))
    }

    async fn refresh_runtime_context(&mut self, selected: Option<&str>) {
        let selected = selected.unwrap_or(OWN_CHAT);
        if self.runtime_for.as_deref() != Some(selected) {
            self.model.clear();
            self.provider.clear();
            self.models.clear();
            self.effort.clear();
            self.runtime_for = Some(selected.to_string());
        }
        if let Some(value) = tokio::time::timeout(
            Duration::from_millis(1200),
            self.desktop_output("state", Some(selected), None),
        )
        .await
        .ok()
        .and_then(Result::ok)
        {
            self.model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.provider = value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.effort = value
                .get("effort")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string();
            if Self::is_owned(Some(selected)) {
                let mut owned = self.owned.lock().unwrap_or_else(|error| error.into_inner());
                owned.saved = value.get("saved").and_then(Value::as_bool).unwrap_or(false);
                owned.title = value
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            self.models = value
                .get("models")
                .and_then(Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
        }
        // The list and the *selected* model must use the same mapping. Mapping
        // only the list left the selected row showing the raw internal id the
        // user was never supposed to see, and the picker's fallback (which
        // re-adds the current model when the list omits it) then put that raw
        // id back at the top of the list.
        self.model = hidden_pool_models_to_display_names(vec![std::mem::take(&mut self.model)])
            .into_iter()
            .next()
            .unwrap_or_default();
        self.models = hidden_pool_models_to_display_names(self.models.clone());
    }

    async fn submit_model(&mut self, selected: Option<&str>, model: &str) -> Result<()> {
        let model = model.trim();
        if model.is_empty() || model.len() > 256 {
            bail!("Model name is invalid");
        }
        // The picker shows the product label for the free pool, so a selection
        // arrives as that label rather than as a routable model id. Translate it
        // back before the request leaves the bridge; otherwise the gateway is
        // asked for a model named after our own UI copy and the switch fails.
        let model = crate::alphacode_app_core::agent::free_pool_rotation::resolve_free_pool_model_selection(model)
            .unwrap_or(model);
        let result = self.desktop_output("model", selected, Some(model)).await?;
        // The server answers with the resolved model id, which for a free-pool
        // selection is the raw internal name. Re-apply the display mapping so a
        // successful switch does not flip the picker back to that name.
        self.model = hidden_pool_models_to_display_names(vec![
            result
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(model)
                .to_string(),
        ])
        .into_iter()
        .next()
        .unwrap_or_default();
        Ok(())
    }

    async fn submit_effort(&mut self, selected: Option<&str>, effort: &str) -> Result<()> {
        let effort = effort.trim().to_lowercase();
        if effort.is_empty() || effort.len() > 32 {
            bail!("Reasoning effort is invalid");
        }
        let result = self
            .desktop_output("effort", selected, Some(&effort))
            .await?;
        self.effort = result
            .get("effort")
            .and_then(Value::as_str)
            .unwrap_or(&effort)
            .to_string();
        Ok(())
    }

    /// Build the request payload the debug socket expects for a "session has a
    /// local connection" target, or `None` for the GUI-owned chat.
    fn is_owned(selected: Option<&str>) -> bool {
        selected.is_none_or(|id| id == OWN_CHAT)
    }

    /// Cancel the in-flight generation for the selected chat.
    ///
    /// This mirrors the terminal's Esc: it fires the session's cancel signal
    /// through the standard `Cancel` request, so the running turn stops for
    /// real instead of only clearing the desktop's spinner.
    async fn submit_cancel(&mut self, selected: Option<&str>) -> Result<Value> {
        if Self::is_owned(selected) {
            let busy = self
                .owned
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .busy;
            if !busy {
                return Ok(
                    json!({"ok": true, "cancelled": false, "detail": "No active GUI turn to cancel"}),
                );
            }
            let sender = self
                .cancel_tx
                .as_ref()
                .context("The chat is not connected yet")?;
            sender
                .send(())
                .await
                .context("The chat connection closed before cancellation")?;
            self.owned
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .detail = "Cancelling the current turn…".to_string();
            return Ok(json!({"ok": true, "cancelled": true}));
        }

        let result = self.desktop_output("cancel", selected, None).await?;
        Ok(
            json!({"ok": true, "cancelled": result.get("cancelled").and_then(Value::as_bool).unwrap_or(false)}),
        )
    }

    /// Rename the selected chat. `title: None` clears the custom title.
    async fn submit_rename(
        &mut self,
        selected: Option<&str>,
        title: Option<&str>,
    ) -> Result<Value> {
        let normalized = title.map(str::trim).filter(|title| !title.is_empty());
        if let Some(title) = normalized
            && title.chars().count() > 120
        {
            bail!("Session title must be 120 characters or fewer");
        }
        let result = self.desktop_output("rename", selected, normalized).await?;
        if Self::is_owned(selected) {
            self.owned
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .title = result
                .get("display_title")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
        Ok(json!({"ok": true, "detail": result}))
    }

    /// Bookmark/unbookmark ("archive") the selected chat.
    async fn submit_archive(
        &mut self,
        selected: Option<&str>,
        label: Option<&str>,
        saved: bool,
    ) -> Result<Value> {
        let label = label.map(str::trim).filter(|label| !label.is_empty());
        if label.is_some_and(|label| label.chars().count() > 60) {
            bail!("Saved-chat labels must be 60 characters or fewer");
        }
        let result = self
            .desktop_output(if saved { "save" } else { "unsave" }, selected, label)
            .await?;
        if Self::is_owned(selected) {
            self.owned
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .saved = saved;
        }
        crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
        Ok(json!({"ok": true, "saved": saved, "detail": result}))
    }

    /// Canonical on-disk session index, identical to the CLI session picker.
    async fn list_stored_sessions(&mut self, limit: usize) -> Result<Value> {
        let output = tokio::task::spawn_blocking(session_index::desktop_session_index)
            .await
            .context("Could not load saved chats")?;
        let mut value: Value = serde_json::from_str(&output)?;
        let mut truncated = false;
        if let Some(sessions) = value.get_mut("sessions").and_then(Value::as_array_mut) {
            let total = sessions.len();
            if limit > 0 && total > limit {
                sessions.truncate(limit);
                truncated = true;
            }
            self.stored_ids = sessions
                .iter()
                .filter_map(|row| {
                    let id = row.get("id")?.as_str()?;
                    let native = row.get("source")?.as_str()? == "alphacode";
                    (native
                        && !id.is_empty()
                        && id
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')))
                    .then(|| id.to_string())
                })
                .collect();
        }
        if truncated {
            value["truncated"] = json!(true);
        }
        Ok(value)
    }

    /// Start a brand-new chat.
    ///
    /// The GUI's own chat is the only session it can create and drive, so
    /// "New chat" resets that session instead of spawning a headless session on
    /// the server (which the desktop has no connection to and could never
    /// select). The next prompt subscribes fresh, so the new chat gets a new
    /// session id and an empty transcript, exactly like the terminal's `/new`.
    async fn submit_new_chat(&mut self, working_dir: Option<&str>) -> Result<Value> {
        {
            let state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
            if state.busy {
                bail!("Finish or cancel the current turn before starting a new chat");
            }
        }
        // Dropping the sender closes the prompt channel, so `run_owned_chat`
        // finishes its loop and its client connection is released before the
        // next prompt opens a new one.
        self.prompt_tx = None;
        self.cancel_tx = None;
        let normalized_dir = working_dir
            .map(str::trim)
            .filter(|dir| !dir.is_empty())
            .map(str::to_string);
        let mut state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
        state.session_id = None;
        state.reply.clear();
        state.transcript.clear();
        state.title = None;
        state.saved = false;
        state.error = None;
        state.input_request = None;
        state.detail = "New chat ready. Send a prompt to begin.".to_string();
        state.working_dir = normalized_dir.clone();
        Ok(json!({
            "ok": true,
            "new_chat": true,
            "working_dir": normalized_dir,
        }))
    }

    /// Tools/skills available to the selected chat, so the desktop shows the
    /// same capabilities the terminal session has.
    async fn runtime_capabilities(&self, selected: Option<&str>) -> Value {
        let tools = match tokio::time::timeout(
            Duration::from_secs(3),
            self.desktop_output("tools", selected, None),
        )
        .await
        {
            Ok(Ok(output)) => serde_json::from_value::<Vec<String>>(output).unwrap_or_default(),
            Ok(Err(error)) => return json!({"ok": false, "error": format!("{error:#}")}),
            Err(_) => return json!({"ok": false, "error": "Loading capabilities timed out"}),
        };
        let mut servers: Vec<String> = tools
            .iter()
            .filter_map(|tool| tool.strip_prefix("mcp__"))
            .filter_map(|name| name.split_once("__").map(|(server, _)| server.to_string()))
            .collect();
        servers.sort();
        servers.dedup();
        let directory = selected
            .filter(|id| *id != OWN_CHAT)
            .and_then(|id| self.sessions.get(id))
            .and_then(|session| session.working_dir.clone())
            .or_else(|| {
                self.owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .working_dir
                    .clone()
            })
            .map(PathBuf::from);
        let skills = tokio::task::spawn_blocking(move || {
            let base = crate::skill::SkillRegistry::shared_snapshot();
            let registry =
                crate::skill::SkillRegistry::effective_for_working_dir(&base, directory.as_deref());
            registry
                .list()
                .into_iter()
                .map(|skill| skill.name.clone())
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        json!({"ok": true, "tools": tools, "skills": {"skills": skills}, "mcp": {"servers": servers}})
    }

    /// Conversation transcript for the selected chat.
    async fn transcript(&self, selected: Option<&str>) -> Value {
        let Some(id) = selected.filter(|id| *id != OWN_CHAT) else {
            return json!({"ok": false, "error": "Transcript is only available for server sessions"});
        };
        if !self.sessions.contains_key(id) && self.stored_ids.contains(id) {
            let stored_id = id.to_string();
            let result = tokio::task::spawn_blocking(move || {
                let session = crate::session::Session::load(&stored_id)?;
                let history = crate::session::render_messages(&session);
                let count = history.len();
                let messages: Vec<_> = history.into_iter().skip(count.saturating_sub(40)).map(|message| {
                    json!({"role": message.role, "text": display_text(&message.content, 4000), "tools": message.tool_calls})
                }).collect();
                Ok::<_, anyhow::Error>(json!({"message_count": count, "messages": messages}))
            }).await;
            return match result {
                Ok(Ok(transcript)) => {
                    json!({"ok": true, "session_id": id, "transcript": transcript})
                }
                Ok(Err(error)) => {
                    json!({"ok": false, "error": format!("Could not load saved chat: {error:#}")})
                }
                Err(error) => {
                    json!({"ok": false, "error": format!("Could not load saved chat: {error}")})
                }
            };
        }
        let result = match tokio::time::timeout(
            Duration::from_millis(2000),
            self.desktop_output("history", Some(id), None),
        )
        .await
        {
            Ok(Ok(output)) => {
                match serde_json::from_value::<Vec<crate::protocol::HistoryMessage>>(
                    output.get("messages").cloned().unwrap_or_default(),
                ) {
                    Ok(history) => {
                        let count = history.len();
                        let messages: Vec<_> = history.into_iter().skip(count.saturating_sub(40)).map(|message| {
                        json!({"role": message.role, "text": display_text(&message.content, 4000), "tools": message.tool_calls.unwrap_or_default()})
                    }).collect();
                        json!({"message_count": count, "messages": messages})
                    }
                    Err(_) => json!({"error": "Malformed transcript response"}),
                }
            }
            Ok(Err(error)) => json!({"error": format!("{error:#}")}),
            Err(_) => json!({"error": "Transcript request timed out"}),
        };
        json!({"ok": true, "session_id": id, "transcript": result})
    }

    async fn snapshot(&mut self, selected: Option<&str>) -> Value {
        self.refresh_runtime_context(selected).await;
        let visible: HashSet<String> = crate::session::user_session_presence()
            .into_iter()
            .map(|presence| presence.session_id)
            .collect();
        let registry = crate::registry::ServerRegistry::load()
            .await
            .unwrap_or_default();
        let mut sockets: Vec<_> = registry
            .servers
            .values()
            .map(|server| (server.debug_socket.clone(), server.socket.clone()))
            .collect();
        sockets.push((
            crate::server::debug_socket_path(),
            crate::server::socket_path(),
        ));
        sockets.sort();
        sockets.dedup();
        // A stale daemon must not hold up status from healthy ones.
        let results =
            futures::future::join_all(sockets.into_iter().take(16).map(|(debug, socket)| {
                tokio::time::timeout(Duration::from_millis(1200), server_sessions(debug, socket))
            }))
            .await;
        let selected_reply = self.selected_reply(selected).await;
        let workflow = if self.workflow_requested {
            match tokio::time::timeout(
                Duration::from_millis(1200),
                self.desktop_output("workflow", selected, None),
            )
            .await
            {
                Ok(Ok(workflow)) => workflow,
                _ => {
                    json!({"available": false, "detail": "Work data is unavailable for this chat. It may be saved, disconnected, or still starting."})
                }
            }
        } else {
            Value::Null
        };
        let previous = std::mem::take(&mut self.sessions);
        let mut connected = 0;
        let unavailable = results
            .iter()
            .filter(|result| !matches!(result, Ok(Ok(_))))
            .count();
        let owned = self.owned.lock().unwrap_or_else(|error| error.into_inner());
        for sessions in results
            .into_iter()
            .filter_map(|result| result.ok().and_then(Result::ok))
        {
            connected += 1;
            for session in sessions {
                if visible.contains(&session.id) && owned.session_id.as_ref() != Some(&session.id) {
                    self.sessions.insert(session.id.clone(), session);
                }
            }
        }
        if unavailable > 0 {
            for (id, mut session) in previous {
                if visible.contains(&id) && !self.sessions.contains_key(&id) {
                    session.state = "offline".to_string();
                    session.detail =
                        "Reconnecting to this session; its current activity is unknown".to_string();
                    self.sessions.insert(id, session);
                }
            }
        }
        let own = CompanionSession {
            id: OWN_CHAT.to_string(),
            title: owned
                .title
                .clone()
                .unwrap_or_else(|| "AlphaCode chat".to_string()),
            state: if owned.error.is_some() || owned.input_request.is_some() {
                "attention"
            } else if owned.busy {
                "working"
            } else {
                "ready"
            }
            .to_string(),
            detail: display_text(
                owned
                    .error
                    .as_deref()
                    .unwrap_or(if owned.detail.is_empty() {
                        "Start a background chat here. Your terminal can stay closed."
                    } else {
                        &owned.detail
                    }),
                240,
            ),
            working_dir: owned.working_dir.clone().or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            }),
            saved: owned.saved,
            socket: None,
        };
        let mut sessions: Vec<_> = self.sessions.values().cloned().collect();
        sessions.insert(0, own);
        let working = sessions
            .iter()
            .filter(|session| session.state == "working")
            .count();
        let attention = sessions
            .iter()
            .filter(|session| session.state == "attention")
            .count();
        let offline = sessions
            .iter()
            .filter(|session| session.state == "offline")
            .count();
        let dictation_available = !crate::config::config().dictation.command.trim().is_empty();
        let mut conversation = owned.transcript.clone();
        append_reply(&mut conversation, &owned.reply);
        json!({ "ok": true, "sessions": sessions, "working": working, "attention": attention, "offline": offline,
            "connected_servers": connected, "reply": display_text(&conversation, MAX_REPLY_BYTES), "owned_busy": owned.busy,
            "owned_session_id": owned.session_id.as_deref(), "input_request": owned.input_request,
            "selected_session_id": selected, "selected_reply": selected_reply, "activity": self.activity,
            "model": self.model, "provider": self.provider, "effort": self.effort, "models": self.models,
            "dictation_available": dictation_available, "workflow": workflow,
            "provider_connection": self.connections.status() })
    }

    async fn submit_input(&mut self, request_id: &str, input: &str) -> Result<()> {
        if input.len() > MAX_PROMPT_BYTES {
            bail!("Command input is too long");
        }
        {
            let mut state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
            if state
                .input_request
                .as_ref()
                .is_none_or(|request| request.request_id != request_id)
            {
                bail!("This command-input request has expired; your input was not sent");
            }
            // Consume the request before enqueueing. Duplicate or stale input
            // must never become the next chat prompt, especially passwords.
            state.input_request = None;
            state.detail = "Sending command input…".to_string();
        }
        let sender = self
            .prompt_tx
            .as_ref()
            .filter(|sender| !sender.is_closed())
            .context("Command connection closed; your input was not sent")?;
        let (accepted, confirmation) = oneshot::channel();
        sender
            .send(DesktopInput {
                content: input.to_string(),
                accepted,
            })
            .await
            .context("Command connection closed; your input was not sent")?;
        confirmation
            .await
            .context("Command connection closed before input was confirmed")?
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }

    async fn submit(&mut self, session_id: &str, prompt: &str) -> Result<()> {
        if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            bail!("Enter a prompt between 1 and 65,536 bytes");
        }
        if session_id == OWN_CHAT {
            {
                let mut state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
                if state.busy {
                    bail!("AlphaCode chat is still working; wait for its response");
                }
                state.busy = true;
                state.error = None;
                state.detail = "Connecting to AlphaCode…".to_string();
            }
            self.ensure_owned_chat();
            let (accepted, confirmation) = oneshot::channel();
            let sender = self
                .prompt_tx
                .as_ref()
                .context("AlphaCode GUI connection unavailable")?;
            if sender
                .send(DesktopInput {
                    content: prompt.to_string(),
                    accepted,
                })
                .await
                .is_err()
            {
                self.owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .busy = false;
                bail!("AlphaCode GUI connection closed; your prompt was not sent");
            }
            match confirmation.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    self.owned
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .busy = false;
                    bail!(error)
                }
                Err(_) => {
                    self.owned
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .busy = false;
                    bail!(
                        "Connection closed before prompt delivery could be confirmed; check the chat before retrying"
                    )
                }
            }
            return Ok(());
        }
        tokio::time::timeout(
            Duration::from_secs(8),
            self.desktop_output("prompt", Some(session_id), Some(prompt)),
        )
        .await
        .context("Delivery could not be confirmed. Check the session before retrying")??;
        Ok(())
    }
}

async fn run_owned_chat(
    state: Arc<Mutex<OwnedChat>>,
    mut prompts: mpsc::Receiver<DesktopInput>,
    mut cancels: mpsc::Receiver<()>,
) -> Result<()> {
    if !crate::server_spawn::is_running().await {
        tokio::time::timeout(
            Duration::from_secs(30),
            crate::server_spawn::spawn_default_server(),
        )
        .await
        .context("AlphaCode server startup timed out")??;
    }
    let mut client = Client::connect()
        .await
        .context("Could not connect to AlphaCode")?;
    let resume = state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .session_id
        .clone();
    let working_dir = state
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .working_dir
        .clone();
    client
        .subscribe_with_info(working_dir, Some(false), resume, false, false)
        .await?;
    let session_id = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match client.read_event().await? {
                ServerEvent::SessionId { session_id } | ServerEvent::History { session_id, .. } => {
                    return Ok::<_, anyhow::Error>(session_id);
                }
                ServerEvent::Error { message, .. } => bail!("{message}"),
                _ => {}
            }
        }
    })
    .await
    .context("AlphaCode GUI session startup timed out")??;
    {
        let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
        state.session_id = Some(session_id);
        state.error = None;
        if !state.busy {
            state.detail = "Connected. Ready for your next task.".to_string();
        }
    }
    while let Some(prompt) = prompts.recv().await {
        let request_id = match client.send_message(&prompt.content).await {
            Ok(id) => id,
            Err(error) => {
                let _ = prompt
                    .accepted
                    .send(Err("Could not send the prompt to AlphaCode".to_string()));
                return Err(error);
            }
        };
        let acknowledged = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match client.read_event().await? {
                    ServerEvent::Ack { id } if id == request_id => {
                        return Ok::<(), anyhow::Error>(());
                    }
                    ServerEvent::Error { id, message, .. } if id == request_id => bail!(message),
                    _ => {}
                }
            }
        })
        .await;
        if !matches!(acknowledged, Ok(Ok(()))) {
            let _ = prompt.accepted.send(Err(
                "Delivery could not be confirmed. Check the chat before retrying.".to_string(),
            ));
            bail!("AlphaCode did not acknowledge the prompt");
        }
        {
            let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
            let previous = std::mem::take(&mut state.reply);
            if !previous.is_empty() {
                append_reply(&mut state.transcript, &previous);
                append_reply(&mut state.transcript, "\n\n");
            }
            append_reply(
                &mut state.transcript,
                &format!("You\n{}\n\nAlphaCode\n", prompt.content),
            );
        }
        let _ = prompt.accepted.send(Ok(()));
        loop {
            let event = tokio::select! {
                event = client.read_event() => event?,
                Some(()) = cancels.recv() => {
                    client.cancel().await?;
                    continue;
                }
            };
            if let ServerEvent::StdinRequest {
                request_id,
                prompt,
                is_password,
                ..
            } = event
            {
                {
                    let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                    state.detail = display_text(&prompt, 240);
                    state.input_request = Some(CommandInput {
                        request_id: request_id.clone(),
                        prompt: display_text(&prompt, 240),
                        is_password,
                    });
                }
                let input = tokio::select! {
                    input = prompts.recv() => input.context("AlphaCode GUI closed while command awaited input")?,
                    Some(()) = cancels.recv() => {
                        client.cancel().await?;
                        continue;
                    }
                };
                if let Err(error) = client
                    .send_stdin_response(&request_id, &input.content)
                    .await
                {
                    let _ = input
                        .accepted
                        .send(Err("Could not deliver command input".to_string()));
                    return Err(error);
                }
                let _ = input.accepted.send(Ok(()));
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                state.input_request = None;
                state.detail = "Continuing the command…".to_string();
                continue;
            }
            let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
            match event {
                ServerEvent::TextDelta { text } => {
                    append_reply(&mut state.reply, &text);
                    state.detail = "Writing a response…".to_string();
                }
                ServerEvent::TextReplace { text } => {
                    state.reply.clear();
                    append_reply(&mut state.reply, &text);
                }
                ServerEvent::ToolStart { name, .. } | ServerEvent::ToolExec { name, .. } => {
                    state.detail = format!("Running {name}")
                }
                ServerEvent::Done { id } if id == request_id => {
                    state.busy = false;
                    state.input_request = None;
                    state.detail = "Response ready".to_string();
                    break;
                }
                ServerEvent::Error {
                    message,
                    retry_after_secs,
                    ..
                } => {
                    if retry_after_secs.is_some() {
                        state.detail = format!("Retrying: {message}");
                    } else {
                        state.busy = false;
                        state.input_request = None;
                        state.error = Some(message);
                        break;
                    }
                }
                ServerEvent::SessionCloseRequested { reason } => bail!("{reason}"),
                _ => {}
            }
        }
    }
    Ok(())
}

pub(crate) async fn run(once: bool, preview: bool, bridge: bool) -> Result<()> {
    if once {
        println!("{}", Bridge::default().snapshot(None).await);
        return Ok(());
    }
    if bridge {
        let mut state = Bridge::default();
        state.ensure_owned_chat();
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut stdout = tokio::io::stdout();
        while let Some(line) = lines.next_line().await? {
            let response = match serde_json::from_str::<BridgeRequest>(&line) {
                Ok(request) if request.action == "providers" => state.connections.providers(),
                Ok(request) if request.action == "provider_connect" => {
                    match state.connections.start(
                        request.provider.as_deref().unwrap_or_default(),
                        request.auth_method.as_deref().unwrap_or_default(),
                        request.api_key.unwrap_or_default(),
                        request.api_base.as_deref(),
                    ) {
                        Ok(()) => json!({"ok": true, "connection": state.connections.status()}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "provider_input" => {
                    match state
                        .connections
                        .send_input(request.prompt.as_deref().unwrap_or_default())
                        .await
                    {
                        Ok(()) => json!({"ok": true, "connection": state.connections.status()}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "provider_cancel" => {
                    state.connections.cancel().await;
                    json!({"ok": true, "connection": state.connections.status()})
                }
                Ok(request) if request.action == "status" => {
                    state.workflow_requested = request.include_workflow;
                    state.snapshot(request.session_id.as_deref()).await
                }
                Ok(request) if request.action == "reconnect" => {
                    state
                        .owned
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .error = None;
                    state.ensure_owned_chat();
                    json!({"ok": true, "reconnecting": true})
                }
                Ok(request) if request.action == "dictate" => match state.submit_dictate().await {
                    Ok(value) => value,
                    Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                },
                Ok(request) if request.action == "input" => {
                    match state
                        .submit_input(
                            request.request_id.as_deref().unwrap_or(""),
                            request.prompt.as_deref().unwrap_or(""),
                        )
                        .await
                    {
                        Ok(()) => json!({"ok": true, "sent": true}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "prompt" => {
                    match state
                        .submit(
                            request.session_id.as_deref().unwrap_or(OWN_CHAT),
                            request.prompt.as_deref().unwrap_or(""),
                        )
                        .await
                    {
                        Ok(()) => json!({"ok": true, "sent": true}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "model" => {
                    match state
                        .submit_model(
                            request.session_id.as_deref(),
                            request.model.as_deref().unwrap_or(""),
                        )
                        .await
                    {
                        Ok(()) => {
                            json!({"ok": true, "model": state.model, "provider": state.provider})
                        }
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "effort" => {
                    match state
                        .submit_effort(
                            request.session_id.as_deref(),
                            request.effort.as_deref().unwrap_or(""),
                        )
                        .await
                    {
                        Ok(()) => json!({"ok": true, "effort": state.effort}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "cancel" => {
                    match state.submit_cancel(request.session_id.as_deref()).await {
                        Ok(value) => value,
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "rename" => {
                    match state
                        .submit_rename(request.session_id.as_deref(), request.title.as_deref())
                        .await
                    {
                        Ok(value) => value,
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "archive" => {
                    let saved = request.saved.unwrap_or(true);
                    match state
                        .submit_archive(
                            request.session_id.as_deref(),
                            request.title.as_deref(),
                            saved,
                        )
                        .await
                    {
                        Ok(value) => value,
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "sessions" => {
                    match state
                        .list_stored_sessions(request.limit.unwrap_or(0) as usize)
                        .await
                    {
                        // Wrapped under `stored` so it can never be confused with
                        // the live `sessions` array returned by `status`.
                        Ok(value) => json!({"ok": true, "stored": value}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "new_chat" => {
                    match state.submit_new_chat(request.working_dir.as_deref()).await {
                        Ok(value) => {
                            state.ensure_owned_chat();
                            value
                        }
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
                Ok(request) if request.action == "capabilities" => {
                    state
                        .runtime_capabilities(request.session_id.as_deref())
                        .await
                }
                Ok(request) if request.action == "transcript" => {
                    state.transcript(request.session_id.as_deref()).await
                }
                _ => {
                    json!({"ok": false, "error": "Expected a status, dictate, prompt, input, model, effort, cancel, rename, archive, sessions, new_chat, capabilities, or transcript request"})
                }
            };
            let mut encoded = serde_json::to_vec(&response)?;
            encoded.push(b'\n');
            stdout.write_all(&encoded).await?;
            stdout.flush().await?;
        }
        return Ok(());
    }
    launch(preview)
}

#[cfg(windows)]
fn launch(preview: bool) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let host = resolve_desktop_host()?;
    let mut command = std::process::Command::new(&host);
    command
        .arg("-Executable")
        .arg(std::env::current_exe()?)
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if preview {
        command.arg("-Preview");
    }
    command
        .spawn()
        .context("Could not open the AlphaCode GUI")?;
    Ok(())
}

/// File name of the Windows GUI host, shipped beside `alphacode.exe`.
///
/// Single source of truth for the sibling lookup. `build.rs` compiles
/// `OUT_DIR/AlphaCode.Desktop.exe` and stages it into the cargo profile
/// directory, `scripts/install.ps1` copies it into the bin directory, and the
/// release archive uses `zip -j` so both land at the root together. Changing the
/// name in any one of those places without the others makes `alphacode gui` fail
/// to find its host; [`tests::desktop_host_filename_matches_release_packaging`]
/// pins this value so the drift is caught here.
#[cfg(windows)]
const DESKTOP_HOST_FILENAME: &str = "AlphaCode.Desktop.exe";

/// Locate the Windows GUI host (`AlphaCode.Desktop.exe`).
///
/// The host ships *alongside* `alphacode.exe`, not embedded inside it. A nested
/// PE that is written to disk and launched at runtime is the exact shape generic
/// packer/dropper heuristics score — BitDefender (`Gen:Variant.Barys`, resold as
/// ALYac/Arcibit/eScan/Emsisoft/GData/VIPRE) flagged the embedded layout — so
/// the default build carries no second executable in its data section.
///
/// Resolution order:
/// 1. `ALPHACODE_DESKTOP_HOST` — explicit override for packaging/tests.
/// 2. Beside the running executable — the release archive and `cargo build`
///    layouts, and the directory the installer writes.
/// 3. `~/.alphacode/gui/` — where older builds unpacked the embedded copy, so
///    an install upgraded in place keeps finding its host.
///
/// Only the legacy `embedded-gui-host` feature still writes an extracted copy,
/// into the directory from (3).
#[cfg(windows)]
fn resolve_desktop_host() -> Result<std::path::PathBuf> {
    if let Some(explicit) = std::env::var_os("ALPHACODE_DESKTOP_HOST") {
        let path = std::path::PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "ALPHACODE_DESKTOP_HOST points at {}, which is not a file",
            path.display()
        );
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(directory) = exe.parent()
    {
        let sibling = directory.join(DESKTOP_HOST_FILENAME);
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    let legacy = crate::storage::alphacode_dir()?.join("gui");
    let legacy_host = legacy.join(DESKTOP_HOST_FILENAME);
    if legacy_host.is_file() {
        return Ok(legacy_host);
    }

    // Only builds compiled with the opt-in feature can materialise a host.
    // Everything else must ship the sibling file, and saying so plainly beats
    // writing a PE to disk to produce the same error one step later.
    #[cfg(feature = "embedded-gui-host")]
    {
        let bytes: &[u8] = include_bytes!(env!("ALPHACODE_DESKTOP_HOST_BYTES"));
        if !bytes.is_empty() {
            std::fs::create_dir_all(&legacy)?;
            let temporary = legacy.join(format!("AlphaCode.Desktop.{}.tmp", std::process::id()));
            std::fs::write(&temporary, bytes)?;
            let installed = std::fs::rename(&temporary, &legacy_host)
                .or_else(|_| std::fs::copy(&temporary, &legacy_host).map(|_| ()));
            let _ = std::fs::remove_file(&temporary);
            installed.context(
                "Could not update the desktop host; close any open AlphaCode GUI and try again",
            )?;
            return Ok(legacy_host);
        }
    }

    bail!(
        "The Windows GUI host (AlphaCode.Desktop.exe) was not found next to {}. \
         Reinstall AlphaCode, or run `alphacode gui --once` for status without the GUI. \
         Builds from source need a native Windows build so the host is compiled.",
        std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "alphacode.exe".to_string())
    )
}

#[cfg(not(windows))]
fn launch(_preview: bool) -> Result<()> {
    bail!(
        "The AlphaCode GUI currently supports Windows. Use `alphacode gui --once` for status on this platform"
    )
}

/// Replace free-pool model ids with the product display name.
///
/// The Alphax Free gateway is an internal implementation detail. A user picked
/// "Alphax Free"; which upstream concrete id happened to serve the turn — or
/// appears in the picker — is ours to hide, not theirs to reason about. The TUI
/// applies the same mapping through `free_pool_display_name`; this keeps the
/// desktop snapshot consistent so no internal gateway name reaches the GUI.
fn hidden_pool_models_to_display_names(models: Vec<String>) -> Vec<String> {
    let mut seen = Vec::<String>::new();
    for model in models {
        let label =
            crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(&model)
                .map(str::to_string)
                .unwrap_or(model);
        // Collapsing many pool ids onto one product label would otherwise
        // produce duplicate picker rows.
        if !seen.contains(&label) {
            seen.push(label);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolver looks for `DESKTOP_HOST_FILENAME` beside `alphacode.exe`.
    /// That name also appears in `build.rs` (what gets compiled and staged) and
    /// in `scripts/install.ps1` (what gets copied). If the three drift apart,
    /// `alphacode gui` reports "host not found" for every packaged user while
    /// the build stays green — so this test reads the other two files and fails
    /// on divergence instead of restating the constant.
    #[cfg(windows)]
    #[test]
    fn desktop_host_filename_matches_build_and_installer() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let build_rs = std::fs::read_to_string(root.join("build.rs"))
            .expect("build.rs is part of the source tree");
        assert!(
            build_rs.contains(DESKTOP_HOST_FILENAME),
            "build.rs no longer produces {DESKTOP_HOST_FILENAME}; the GUI host \
             would not be staged beside the binary"
        );

        let installer = std::fs::read_to_string(root.join("scripts").join("install.ps1"))
            .expect("scripts/install.ps1 is part of the source tree");
        assert!(
            installer.contains(DESKTOP_HOST_FILENAME),
            "scripts/install.ps1 no longer installs {DESKTOP_HOST_FILENAME}; \
             an installed CLI would not find its GUI host"
        );

        // The release archive must include it too, or the packaged user hits the
        // same missing-host path.
        let workflow =
            std::fs::read_to_string(root.join(".github").join("workflows").join("release.yml"))
                .expect("release.yml is part of the source tree");
        assert!(
            workflow.contains(DESKTOP_HOST_FILENAME),
            "release.yml does not package {DESKTOP_HOST_FILENAME}"
        );
    }

    /// Acceptance criterion from the GUI audit: no internal gateway name may
    /// reach the desktop surfaces.
    ///
    /// The concrete pool ids are deliberately obfuscated in the binary, so the
    /// test drives the real resolver rather than hard-coding a name: whatever
    /// the pool currently contains must present as the product label.
    #[test]
    fn gui_model_list_never_exposes_internal_gateway_names() {
        use crate::alphacode_app_core::agent::free_pool_rotation::effective_free_pool;

        let pool = effective_free_pool();
        assert!(!pool.is_empty(), "the curated free pool must not be empty");

        let mut models: Vec<String> = vec![
            "openai/gpt-5.5".to_string(),
            "anthropic/claude-sonnet-4".to_string(),
        ];
        models.extend(pool.iter().cloned());

        let display = hidden_pool_models_to_display_names(models);
        let joined = display.join(",");

        // No raw pool member may survive into the picker.
        for member in &pool {
            assert!(
                !display.contains(member),
                "internal pool id `{member}` reached the GUI: {joined}"
            );
        }
        let label =
            crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(&pool[0])
                .expect("a pool member must resolve to a display name");
        assert!(!label.is_empty());
        assert!(
            display.contains(&label.to_string()) || pool.len() > 1,
            "pool members should surface under the product label: {joined}"
        );

        // Real models keep their honest names.
        assert!(display.contains(&"openai/gpt-5.5".to_string()));
        // Pool members collapse to one product row, not duplicates.
        assert!(
            display.iter().filter(|m| m.as_str() == label).count() <= 1,
            "pool members must collapse to a single product label: {joined}"
        );
    }

    #[test]
    fn gui_selected_model_is_mapped_and_round_trips() {
        use crate::alphacode_app_core::agent::free_pool_rotation::{
            effective_free_pool, resolve_free_pool_model_selection,
        };

        let pool = effective_free_pool();
        let alias = pool.first().cloned().expect("pool has a floor");

        // The selected model goes through the same mapping as the list, so the
        // picker's current row never shows the raw internal id.
        let selected = hidden_pool_models_to_display_names(vec![alias.clone()]);
        let label = selected.first().cloned().expect("one label");
        assert_ne!(label, alias, "the raw pool id must not be the shown label");

        // Feeding the shown label back — which is what the picker does on a
        // selection — must resolve to a routable id, not to the label itself.
        let resolved = resolve_free_pool_model_selection(&label)
            .expect("the product label must resolve back to a model id");
        assert_eq!(resolved, alias.as_str());
    }

    #[test]
    fn gui_model_list_preserves_order_and_deduplicates() {
        let display = hidden_pool_models_to_display_names(vec![
            "b/model".to_string(),
            "a/model".to_string(),
            "b/model".to_string(),
        ]);
        assert_eq!(display, vec!["b/model".to_string(), "a/model".to_string()]);
    }

    #[test]
    fn gui_distinguishes_busy_attention_and_idle() {
        assert_eq!(session_state(true, "running"), "working");
        assert_eq!(session_state(false, "waiting"), "attention");
        assert_eq!(session_state(false, "ready"), "ready");
        assert_eq!(session_state(true, "error"), "attention");
    }

    #[test]
    fn bounded_reply_keeps_unicode_and_latest_result() {
        let mut reply = "界".repeat(MAX_REPLY_BYTES);
        append_reply(&mut reply, "\nLatest result");
        assert!(reply.len() <= MAX_REPLY_BYTES);
        assert!(reply.ends_with("Latest result"));
        assert_eq!(display_text("\x1b[31mfailed\x1b[0m\x07", 100), "failed");
    }

    #[tokio::test]
    async fn rename_rejects_overlong_titles_before_any_io() {
        let mut bridge = Bridge::default();
        // An over-long title is rejected locally, so no socket is ever opened.
        assert!(
            bridge
                .submit_rename(Some("missing-session"), Some(&"t".repeat(121)))
                .await
                .is_err()
        );
        // A 120-character title passes validation and then fails for a real
        // reason: the target session does not exist.
        assert!(
            bridge
                .submit_rename(Some("missing-session"), Some(&"t".repeat(120)))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn new_chat_resets_the_owned_chat_and_rejects_busy() {
        let mut bridge = Bridge::default();
        {
            let mut state = bridge.owned.lock().unwrap();
            state.session_id = Some("session_stale".to_string());
            state.reply = "previous answer".to_string();
            state.transcript = "previous chat".to_string();
            state.error = Some("previous failure".to_string());
        }
        let value = bridge.submit_new_chat(Some("C:/work")).await.unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["new_chat"], true);
        assert_eq!(value["working_dir"], "C:/work");
        {
            let state = bridge.owned.lock().unwrap();
            assert!(state.session_id.is_none());
            assert!(state.reply.is_empty());
            assert!(state.transcript.is_empty());
            assert!(state.error.is_none());
            assert_eq!(state.working_dir.as_deref(), Some("C:/work"));
        }
        // A running turn must not be silently discarded by "New chat". The
        // guard is dropped at the end of the block above so the lock is not
        // held across the await below (clippy::await_holding_lock).
        bridge.owned.lock().unwrap().busy = true;
        assert!(bridge.submit_new_chat(None).await.is_err());
    }

    #[tokio::test]
    async fn archive_requires_a_live_or_owned_session_and_a_short_label() {
        let mut bridge = Bridge::default();
        // No owned session yet, so there is nothing to archive.
        assert!(bridge.submit_archive(None, None, true).await.is_err());
        assert!(
            bridge
                .submit_archive(Some("missing-session"), Some(&"l".repeat(61)), true)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn cancel_without_an_owned_session_reports_nothing_to_cancel() {
        let mut bridge = Bridge::default();
        let value = bridge.submit_cancel(Some(OWN_CHAT)).await.unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["cancelled"], false);
    }

    #[tokio::test]
    async fn transcript_is_only_available_for_server_sessions() {
        let bridge = Bridge::default();
        let value = bridge.transcript(Some(OWN_CHAT)).await;
        assert_eq!(value["ok"], false);
        assert_eq!(
            value["error"],
            "Transcript is only available for server sessions"
        );
    }

    #[tokio::test]
    async fn gui_rejects_empty_oversized_and_unknown_targets_without_connecting() {
        let mut bridge = Bridge::default();
        assert!(bridge.submit(OWN_CHAT, " ").await.is_err());
        assert!(
            bridge
                .submit(OWN_CHAT, &"x".repeat(MAX_PROMPT_BYTES + 1))
                .await
                .is_err()
        );
        assert!(bridge.submit("missing-session", "hello").await.is_err());
        assert!(!bridge.owned.lock().unwrap().busy);
    }

    #[tokio::test]
    async fn command_input_accepts_empty_line_without_replacing_chat_draft_or_reply() {
        let mut bridge = Bridge::default();
        let (tx, mut rx) = mpsc::channel(1);
        bridge.prompt_tx = Some(tx);
        {
            let mut state = bridge.owned.lock().unwrap();
            state.busy = true;
            state.reply = "Previous response".into();
            state.input_request = Some(CommandInput {
                request_id: "input-1".into(),
                prompt: "Press Enter".into(),
                is_password: false,
            });
        }
        assert!(
            bridge
                .submit_input("expired", "do not reinterpret as a prompt")
                .await
                .is_err()
        );
        let receiver = tokio::spawn(async move {
            let input: DesktopInput = rx.recv().await.unwrap();
            assert_eq!(input.content, "");
            let _ = input.accepted.send(Ok(()));
            assert!(rx.try_recv().is_err());
        });
        bridge.submit_input("input-1", "").await.unwrap();
        receiver.await.unwrap();
        assert!(
            bridge
                .submit_input("input-1", "do not duplicate")
                .await
                .is_err()
        );
        assert_eq!(bridge.owned.lock().unwrap().reply, "Previous response");
    }

    #[tokio::test]
    async fn gui_reports_missing_external_sessions() {
        let mut bridge = Bridge::default();
        assert!(bridge.submit("other-session", "hello").await.is_err());
    }
}
