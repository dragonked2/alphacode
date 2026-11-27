//! Small local desktop client. Status reads use existing daemon snapshots;
//! GUI-owned chats use a normal client connection and no terminal UI.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{OnceCell, mpsc, oneshot};

use crate::alphacode_app_core::server::{connect_socket, socket_path};
use crate::alphacode_app_core::transport::{ReadHalf, WriteHalf};
use crate::alphacode_provider_core::{EventStream, Provider};
use crate::protocol::{Request, ServerEvent};
use crate::server::Client;

#[path = "../alphacode_app_core/server/debug_sessions_index.rs"]
mod session_index;

#[path = "gui/db.rs"]
mod db;

#[path = "gui/provider_connections.rs"]
mod provider_connections;

#[path = "gui/web.rs"]
pub mod web;

#[path = "gui/desktop.rs"]
pub mod desktop;

const OWN_CHAT: &str = "__companion__";
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_REPLY_BYTES: usize = 24 * 1024;
const MAX_THINKING_BYTES: usize = 24 * 1024;
const MAX_TOOL_ARGS_BYTES: usize = 16 * 1024;
const MAX_TOOL_OUTPUT_BYTES: usize = 16 * 1024;
const MAX_FILE_READ_BYTES: usize = 512 * 1024;
const MAX_FILE_WRITE_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_SHELL_TIMEOUT_MS: u64 = 30_000;
const MAX_SHELL_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_SHELL_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_SHELL_OUTPUT_BYTES: usize = 256 * 1024;

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

#[derive(Clone, Debug, Default, Serialize)]
struct LiveThinking {
    text: String,
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_secs: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
struct LiveToolCall {
    id: String,
    name: String,
    args_raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
    output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    status: String,
}

#[derive(Default)]
struct OwnedChat {
    session_id: Option<String>,
    busy: bool,
    prompt_pre_recorded: bool,
    turn_seq: u64,
    error: Option<String>,
    detail: String,
    reply: String,
    transcript: String,
    title: Option<String>,
    saved: bool,
    input_request: Option<CommandInput>,
    /// Workspace the next owned turn should subscribe in. Reset by `new_chat`.
    working_dir: Option<String>,
    live_thinking: LiveThinking,
    live_tools: Vec<LiveToolCall>,
    side_pane_images: Vec<Value>,
    generated_images: Vec<Value>,
    turn_started_at: Option<Instant>,
    api_call_started_at: Option<Instant>,
    tokens_per_sec: f64,
    last_context_tokens: usize,
    session_input_tokens: u64,
    session_output_tokens: u64,
    session_cache_read_tokens: u64,
    session_cache_creation_tokens: u64,
}

#[derive(Clone, Serialize)]
struct CommandInput {
    request_id: String,
    prompt: String,
    is_password: bool,
}

#[derive(Default, Deserialize)]
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
    setup_value: Option<String>,
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
    /// Working directory for `new_chat`, `files`, `shell_exec`, etc.
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
    /// Path for `read_file`, `write_file`, `create_file`, `create_dir`, `delete_path`, `rename_path`, `git_action`.
    #[serde(default)]
    path: Option<String>,
    /// Destination path for `rename_path`.
    #[serde(default)]
    new_path: Option<String>,
    /// File contents for `write_file` / `create_file`.
    #[serde(default)]
    content: Option<String>,
    /// Git operation for `git_action`: "stage" | "unstage" | "discard" | "commit".
    #[serde(default)]
    git_op: Option<String>,
    /// Commit message or memory content.
    #[serde(default)]
    message: Option<String>,
    /// Shell or slash command string.
    #[serde(default)]
    command: Option<String>,
    /// Optional shell timeout override in milliseconds.
    #[serde(default)]
    timeout_ms: Option<u64>,
    /// Optional shell output cap override in bytes.
    #[serde(default)]
    max_output_bytes: Option<usize>,
    /// Tool name for `run_tool` / `run_recon`.
    #[serde(default)]
    tool: Option<String>,
    /// Tool arguments for `run_tool` / `run_recon`.
    #[serde(default)]
    args: Option<Value>,
    /// Target domain/URL/host for `run_recon`.
    #[serde(default)]
    target: Option<String>,
    /// Sub-operation for `memory` ("list"|"save"|"delete") or `schedule` ("list"|"cancel"|"pause"|"resume").
    #[serde(default)]
    op: Option<String>,
    /// Memory ID or schedule ID.
    #[serde(default)]
    id: Option<String>,
    /// Memory category.
    #[serde(default)]
    category: Option<String>,
    /// Memory tags.
    #[serde(default)]
    tags: Option<Vec<String>>,
    /// Memory scope ("project"|"global").
    #[serde(default)]
    scope: Option<String>,
    /// Config key for `config_set`.
    #[serde(default)]
    key: Option<String>,
    /// Config value for `config_set`.
    #[serde(default)]
    value: Option<Value>,
    /// Batch config updates for `config_set`.
    #[serde(default)]
    updates: Option<Value>,
    /// Account label for `account_switch`.
    #[serde(default)]
    label: Option<String>,
    /// Message index for `/rewind <n>`.
    #[serde(default)]
    message_index: Option<usize>,
    /// Pin flag for `workspace_pin`.
    #[serde(default)]
    pinned: Option<bool>,
    /// Collapse flag for `workspace_collapse`.
    #[serde(default)]
    collapsed: Option<bool>,
    /// Optional display name for `workspace_add`.
    #[serde(default)]
    name: Option<String>,
}

struct DesktopInput {
    content: String,
    accepted: oneshot::Sender<Result<(), String>>,
}

enum OwnedControl {
    Clear(oneshot::Sender<Result<Value, String>>),
    Compact(oneshot::Sender<Result<Value, String>>),
    Rewind {
        message_index: usize,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    RewindUndo(oneshot::Sender<Result<Value, String>>),
    Split(oneshot::Sender<Result<Value, String>>),
    Transfer(oneshot::Sender<Result<Value, String>>),
    GetSessionTranscript {
        session_id: String,
        reply: oneshot::Sender<Result<Option<String>, String>>,
    },
    SwitchAccount {
        provider: String,
        label: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    SetServiceTier {
        tier: String,
        reply: oneshot::Sender<Result<Option<String>, String>>,
    },
    SetTransport {
        transport: String,
        reply: oneshot::Sender<Result<Option<String>, String>>,
    },
}

struct BridgeSocketClient {
    reader: BufReader<ReadHalf>,
    read_buf: Vec<u8>,
    writer: WriteHalf,
    next_id: u64,
}

impl BridgeSocketClient {
    async fn connect() -> Result<Self> {
        let stream = connect_socket(&socket_path()).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            read_buf: Vec::with_capacity(8192),
            writer,
            next_id: 1,
        })
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn send_request(&mut self, request: &Request) -> Result<()> {
        let mut encoded = serde_json::to_vec(request)?;
        encoded.push(b'\n');
        self.writer.write_all(&encoded).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn send_message(&mut self, content: &str) -> Result<u64> {
        let id = self.alloc_id();
        self.send_request(&Request::Message {
            id,
            content: content.to_string(),
            images: vec![],
            system_reminder: None,
        })
        .await?;
        Ok(id)
    }

    async fn cancel(&mut self) -> Result<u64> {
        let id = self.alloc_id();
        self.send_request(&Request::Cancel { id }).await?;
        Ok(id)
    }

    async fn send_stdin_response(&mut self, request_id: &str, input: &str) -> Result<u64> {
        let id = self.alloc_id();
        self.send_request(&Request::StdinResponse {
            id,
            request_id: request_id.to_string(),
            input: input.to_string(),
        })
        .await?;
        Ok(id)
    }

    async fn subscribe_with_info(
        &mut self,
        working_dir: Option<String>,
        selfdev: Option<bool>,
        target_session_id: Option<String>,
        client_has_local_history: bool,
        allow_session_takeover: bool,
    ) -> Result<u64> {
        let id = self.alloc_id();
        let working_dir = match working_dir
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty() && !user_home_dir_or_install(Path::new(d)))
        {
            Some(dir) => Some(dir),
            None if target_session_id.is_some() => None,
            None => bail!("Open a project folder first before starting a chat"),
        };
        self.send_request(&Request::Subscribe {
            id,
            working_dir,
            selfdev,
            target_session_id,
            client_instance_id: None,
            client_has_local_history,
            allow_session_takeover,
            terminal_env: crate::terminal_launch::snapshot_client_terminal_env(),
        })
        .await?;
        Ok(id)
    }

    /// Cancellation-safe line reader using `fill_buf` + `consume`.
    async fn read_event(&mut self) -> Result<ServerEvent> {
        const MAX_EVENT_BYTES: usize = 64 * 1024 * 1024;
        loop {
            if let Some(pos) = self.read_buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.read_buf.drain(..=pos).collect();
                let trimmed = String::from_utf8_lossy(&line);
                let trimmed = trimmed.trim();
                if trimmed.is_empty() {
                    continue;
                }
                return Ok(serde_json::from_str(trimmed)?);
            }
            let chunk = self.reader.fill_buf().await?;
            if chunk.is_empty() {
                bail!("Server disconnected");
            }
            if self.read_buf.len() + chunk.len() > MAX_EVENT_BYTES {
                bail!("Server event exceeded the 64MB limit");
            }
            self.read_buf.extend_from_slice(chunk);
            let len = chunk.len();
            self.reader.consume(len);
        }
    }
}

struct GuiStubProvider;

#[async_trait::async_trait]
impl Provider for GuiStubProvider {
    fn name(&self) -> &str {
        "gui-local"
    }

    fn model(&self) -> String {
        "unknown".to_string()
    }

    async fn complete(
        &self,
        _messages: &[crate::message::Message],
        _tools: &[crate::message::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        bail!("GuiStubProvider does not run LLM completions")
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(GuiStubProvider)
    }
}

static GUI_TOOL_REGISTRY: OnceCell<Arc<crate::tool::Registry>> = OnceCell::const_new();

async fn gui_tool_registry() -> Arc<crate::tool::Registry> {
    GUI_TOOL_REGISTRY
        .get_or_init(|| async {
            let provider: Arc<dyn Provider> = Arc::new(GuiStubProvider);
            Arc::new(crate::tool::Registry::new(provider).await)
        })
        .await
        .clone()
}

#[derive(Clone, Serialize, Deserialize)]
struct GuiPersistentState {
    #[serde(default)]
    last_working_dir: Option<String>,
    #[serde(default)]
    active_workspace: Option<String>,
    #[serde(default)]
    workspaces: Vec<String>,
    #[serde(default)]
    pinned_workspaces: Vec<String>,
    #[serde(default)]
    collapsed_workspaces: Vec<String>,
    #[serde(default)]
    pinned_chats: Vec<String>,
    #[serde(default)]
    last_session_id: Option<String>,
    #[serde(default = "default_gui_theme")]
    theme: Option<String>,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    font_size: Option<u32>,
    #[serde(default)]
    sidebar_open: Option<bool>,
}

fn default_gui_theme() -> Option<String> {
    Some("dark".to_string())
}

impl Default for GuiPersistentState {
    fn default() -> Self {
        Self {
            last_working_dir: None,
            active_workspace: None,
            workspaces: Vec::new(),
            pinned_workspaces: Vec::new(),
            collapsed_workspaces: Vec::new(),
            pinned_chats: Vec::new(),
            last_session_id: None,
            theme: default_gui_theme(),
            lang: None,
            font_size: None,
            sidebar_open: None,
        }
    }
}

fn is_program_install_dir(path: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(exe_dir) = exe.parent() else {
        return false;
    };
    let Ok(canon_path) = std::fs::canonicalize(path) else {
        return false;
    };
    let Ok(canon_exe_dir) = std::fs::canonicalize(exe_dir) else {
        return false;
    };
    canon_path == canon_exe_dir
}

fn is_user_home_dir(path: &Path) -> bool {
    let mut home_candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("USERPROFILE") {
        home_candidates.push(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("HOME") {
        home_candidates.push(PathBuf::from(p));
    }
    if let Some(p) = dirs::home_dir() {
        home_candidates.push(p);
    }
    let canon_target = std::fs::canonicalize(path).ok();
    let lex_target = lexically_normalize_path(path);
    home_candidates.into_iter().any(|home| {
        if let (Some(ct), Ok(ch)) = (&canon_target, std::fs::canonicalize(&home))
            && ct == &ch
        {
            return true;
        }
        let lex_home = lexically_normalize_path(&home);
        #[cfg(windows)]
        {
            lex_target
                .to_string_lossy()
                .eq_ignore_ascii_case(&lex_home.to_string_lossy())
        }
        #[cfg(not(windows))]
        {
            lex_target == lex_home
        }
    })
}

fn user_home_dir_or_install(path: &Path) -> bool {
    is_program_install_dir(path) || is_user_home_dir(path)
}

#[cfg(test)]
fn load_gui_state() -> GuiPersistentState {
    db::GuiDb::open_default()
        .and_then(|db| db.load_gui_state())
        .unwrap_or_default()
}

fn save_gui_state(state: &GuiPersistentState) -> Result<()> {
    db::GuiDb::open_default()?.save_gui_state(state)
}

fn normalize_existing_dir(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = Path::new(trimmed);
    if !path.is_dir() {
        return None;
    }
    std::fs::canonicalize(path)
        .ok()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            #[cfg(windows)]
            {
                s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
            }
            #[cfg(not(windows))]
            {
                s
            }
        })
        .or_else(|| Some(trimmed.to_string()))
}

fn push_unique_workspace(list: &mut Vec<String>, dir: &str) {
    let key = dir.trim();
    if key.is_empty() {
        return;
    }
    if let Some(pos) = list.iter().position(|item| item.eq_ignore_ascii_case(key)) {
        list.remove(pos);
    }
    list.insert(0, key.to_string());
    if list.len() > 24 {
        list.truncate(24);
    }
}

fn resolve_default_startup_working_dir(gui_state: &GuiPersistentState) -> Option<String> {
    gui_state
        .active_workspace
        .as_deref()
        .or(gui_state.last_working_dir.as_deref())
        .and_then(normalize_existing_dir)
        .filter(|dir| !user_home_dir_or_install(Path::new(dir)))
}

#[derive(Default)]
struct Bridge {
    connections: provider_connections::ProviderConnections,
    last_auth_epoch: u64,
    owned: Arc<Mutex<OwnedChat>>,
    prompt_tx: Option<mpsc::Sender<DesktopInput>>,
    cancel_tx: Option<mpsc::Sender<()>>,
    control_tx: Option<mpsc::Sender<OwnedControl>>,
    sessions: BTreeMap<String, CompanionSession>,
    stored_ids: HashSet<String>,
    activity: Vec<String>,
    models: Vec<String>,
    model_options: Vec<Value>,
    model: String,
    provider: String,
    effort: String,
    runtime_for: Option<String>,
    workflow_requested: bool,
    gui_state: GuiPersistentState,
}

fn extract_thinking_from_stored_message(
    session: &crate::session::Session,
    stored_index: Option<usize>,
) -> Option<String> {
    let idx = stored_index?;
    let msg = session.messages.get(idx)?;
    let mut parts = Vec::new();
    for block in &msg.content {
        match block {
            crate::message::ContentBlock::Reasoning { text }
            | crate::message::ContentBlock::ReasoningTrace { text }
            | crate::message::ContentBlock::AnthropicThinking { thinking: text, .. } => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
            crate::message::ContentBlock::OpenAIReasoning { summary, .. } => {
                for s in summary {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn build_stored_transcript_json(session: &crate::session::Session) -> Value {
    let history = crate::session::render_messages(session);
    let count = history.len();
    let messages: Vec<_> = history
        .into_iter()
        .skip(count.saturating_sub(40))
        .map(|message| {
            let thinking = if message.role == "assistant" {
                extract_thinking_from_stored_message(session, message.stored_index)
                    .map(|t| display_text(&t, MAX_THINKING_BYTES))
            } else {
                None
            };
            let mut row = json!({
                "role": message.role,
                "text": display_text(&message.content, 4000),
                "tools": message.tool_calls,
            });
            if let Some(thinking_text) = thinking
                && !thinking_text.trim().is_empty()
            {
                row["thinking"] = json!(thinking_text);
            }
            row
        })
        .collect();
    json!({"message_count": count, "messages": messages})
}

fn is_safe_session_id(id: &str) -> bool {
    let id = id.trim();
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

async fn load_stored_transcript_response(id: &str) -> Value {
    let stored_id = id.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let session = crate::session::Session::load(&stored_id)?;
        Ok::<_, anyhow::Error>(build_stored_transcript_json(&session))
    })
    .await;
    match result {
        Ok(Ok(transcript)) => json!({"ok": true, "session_id": id, "transcript": transcript}),
        Ok(Err(error)) => {
            json!({"ok": false, "error": format!("Could not load saved chat: {error:#}")})
        }
        Err(error) => {
            json!({"ok": false, "error": format!("Could not load saved chat: {error}")})
        }
    }
}

impl Bridge {
    fn with_saved_state() -> Self {
        let mut gui_state = if let Ok(db) = db::GuiDb::open_default() {
            if let Ok(base) = crate::storage::alphacode_dir() {
                let sessions_dir = base.join("sessions");
                if sessions_dir.is_dir() {
                    let _ = db.sync_sessions_index(&sessions_dir);
                }
            }
            db.load_gui_state().unwrap_or_default()
        } else {
            GuiPersistentState::default()
        };
        gui_state
            .workspaces
            .retain(|w| Path::new(w).is_dir() && !user_home_dir_or_install(Path::new(w)));
        gui_state
            .pinned_workspaces
            .retain(|w| Path::new(w).is_dir() && !user_home_dir_or_install(Path::new(w)));
        gui_state
            .collapsed_workspaces
            .retain(|w| Path::new(w).is_dir() && !user_home_dir_or_install(Path::new(w)));
        let startup_dir = resolve_default_startup_working_dir(&gui_state);
        let bridge = Self {
            gui_state,
            ..Self::default()
        };
        if let Some(dir) = startup_dir {
            bridge
                .owned
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .working_dir = Some(dir);
        }
        bridge
    }

    fn record_workspace_visit(&mut self, dir: &str, update_last: bool) {
        let Some(clean) = normalize_existing_dir(dir) else {
            return;
        };
        if !user_home_dir_or_install(Path::new(&clean)) {
            push_unique_workspace(&mut self.gui_state.workspaces, &clean);
            if let Ok(db) = db::GuiDb::open_default() {
                let _ = db.upsert_workspace(&clean, None, true);
            }
        }
        if update_last && !user_home_dir_or_install(Path::new(&clean)) {
            self.gui_state.last_working_dir = Some(clean.clone());
            self.gui_state.active_workspace = Some(clean);
        }
        let _ = save_gui_state(&self.gui_state);
    }

    fn gui_state_get(&self) -> Value {
        json!({
            "ok": true,
            "action": "gui_state_get",
            "state": self.gui_state,
            "active_working_dir": self.owned.lock().unwrap_or_else(|e| e.into_inner()).working_dir,
        })
    }

    fn workspace_select(&mut self, request: &BridgeRequest) -> Value {
        let raw = request
            .working_dir
            .as_deref()
            .or(request.path.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() {
            return json!({"ok": false, "action": "workspace_select", "error": "Workspace path is required"});
        }
        let Some(clean) = normalize_existing_dir(raw) else {
            return json!({"ok": false, "action": "workspace_select", "error": format!("Workspace directory not found: {raw}")});
        };
        if is_program_install_dir(Path::new(&clean)) {
            return json!({"ok": false, "action": "workspace_select", "error": "Program install directory cannot be used as a workspace"});
        }
        push_unique_workspace(&mut self.gui_state.workspaces, &clean);
        self.gui_state.active_workspace = Some(clean.clone());
        self.gui_state.last_working_dir = Some(clean.clone());
        if let Ok(db) = db::GuiDb::open_default() {
            let _ = db.upsert_workspace(&clean, request.name.as_deref(), true);
        }
        if let Err(err) = save_gui_state(&self.gui_state) {
            return json!({"ok": false, "action": "workspace_select", "error": format!("{err:#}")});
        }
        let mut reset_idle_connection = false;
        {
            let mut owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
            if !owned.busy {
                let changed = owned
                    .working_dir
                    .as_deref()
                    .is_none_or(|prev| !db::workspace_paths_equal(prev, &clean));
                owned.working_dir = Some(clean.clone());
                if changed && owned.transcript.is_empty() && owned.reply.is_empty() {
                    owned.session_id = None;
                    reset_idle_connection = true;
                }
            }
        }
        if reset_idle_connection {
            self.prompt_tx = None;
            self.cancel_tx = None;
            self.control_tx = None;
            self.runtime_for = None;
        }
        json!({
            "ok": true,
            "action": "workspace_select",
            "working_dir": clean,
            "active_workspace": self.gui_state.active_workspace,
            "state": self.gui_state,
        })
    }

    fn workspace_add(&mut self, request: &BridgeRequest) -> Value {
        let raw = request
            .working_dir
            .as_deref()
            .or(request.path.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() {
            return json!({"ok": false, "action": "workspace_add", "error": "Workspace path is required"});
        }
        let Some(clean) = normalize_existing_dir(raw) else {
            return json!({"ok": false, "action": "workspace_add", "error": format!("Workspace directory not found: {raw}")});
        };
        if is_program_install_dir(Path::new(&clean)) {
            return json!({"ok": false, "action": "workspace_add", "error": "Program install directory cannot be added as a workspace"});
        }
        push_unique_workspace(&mut self.gui_state.workspaces, &clean);
        if let Ok(db) = db::GuiDb::open_default() {
            let _ = db.upsert_workspace(&clean, request.name.as_deref(), true);
        }
        if let Err(err) = save_gui_state(&self.gui_state) {
            return json!({"ok": false, "action": "workspace_add", "error": format!("{err:#}")});
        }
        json!({
            "ok": true,
            "action": "workspace_add",
            "working_dir": clean,
            "state": self.gui_state,
        })
    }

    fn workspace_remove(&mut self, request: &BridgeRequest) -> Value {
        let raw = request
            .working_dir
            .as_deref()
            .or(request.path.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() {
            return json!({"ok": false, "action": "workspace_remove", "error": "Workspace path is required"});
        }
        let target = normalize_existing_dir(raw).unwrap_or_else(|| raw.to_string());
        let before_len = self.gui_state.workspaces.len();
        self.gui_state.workspaces.retain(|w| {
            !db::workspace_paths_equal(w, &target) && !db::workspace_paths_equal(w, raw)
        });
        self.gui_state.pinned_workspaces.retain(|w| {
            !db::workspace_paths_equal(w, &target) && !db::workspace_paths_equal(w, raw)
        });
        self.gui_state.collapsed_workspaces.retain(|w| {
            !db::workspace_paths_equal(w, &target) && !db::workspace_paths_equal(w, raw)
        });
        if self.gui_state.active_workspace.as_deref().is_some_and(|w| {
            db::workspace_paths_equal(w, &target) || db::workspace_paths_equal(w, raw)
        }) {
            self.gui_state.active_workspace = None;
        }
        if self.gui_state.last_working_dir.as_deref().is_some_and(|w| {
            db::workspace_paths_equal(w, &target) || db::workspace_paths_equal(w, raw)
        }) {
            self.gui_state.last_working_dir = None;
        }
        {
            let mut owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
            if !owned.busy
                && owned.working_dir.as_deref().is_some_and(|w| {
                    db::workspace_paths_equal(w, &target) || db::workspace_paths_equal(w, raw)
                })
            {
                owned.working_dir = None;
            }
        }
        let db_removed = db::GuiDb::open_default()
            .and_then(|db| db.remove_workspace(&target))
            .unwrap_or(false);
        let _ = save_gui_state(&self.gui_state);
        json!({
            "ok": true,
            "action": "workspace_remove",
            "working_dir": target,
            "removed": db_removed || self.gui_state.workspaces.len() < before_len,
            "state": self.gui_state,
        })
    }

    fn workspace_pin(&mut self, request: &BridgeRequest) -> Value {
        let raw = request
            .working_dir
            .as_deref()
            .or(request.path.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() {
            return json!({"ok": false, "action": "workspace_pin", "error": "Workspace path is required"});
        }
        let Some(clean) = normalize_existing_dir(raw) else {
            return json!({"ok": false, "action": "workspace_pin", "error": format!("Workspace directory not found: {raw}")});
        };
        let requested = request
            .pinned
            .or(request.saved)
            .or_else(|| request.value.as_ref().and_then(Value::as_bool));
        if !self
            .gui_state
            .workspaces
            .iter()
            .any(|w| db::workspace_paths_equal(w, &clean))
        {
            push_unique_workspace(&mut self.gui_state.workspaces, &clean);
        }
        let currently_pinned = self
            .gui_state
            .pinned_workspaces
            .iter()
            .any(|w| db::workspace_paths_equal(w, &clean));
        let next_pinned = requested.unwrap_or(!currently_pinned);
        self.gui_state
            .pinned_workspaces
            .retain(|w| !db::workspace_paths_equal(w, &clean));
        if next_pinned {
            self.gui_state.pinned_workspaces.push(clean.clone());
        }
        if let Ok(db) = db::GuiDb::open_default() {
            let _ = db.set_workspace_pinned(&clean, Some(next_pinned));
        }
        if let Err(err) = save_gui_state(&self.gui_state) {
            return json!({"ok": false, "action": "workspace_pin", "error": format!("{err:#}")});
        }
        json!({
            "ok": true,
            "action": "workspace_pin",
            "working_dir": clean,
            "pinned": next_pinned,
            "state": self.gui_state,
        })
    }

    fn workspace_collapse(&mut self, request: &BridgeRequest) -> Value {
        let raw = request
            .working_dir
            .as_deref()
            .or(request.path.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() {
            return json!({"ok": false, "action": "workspace_collapse", "error": "Workspace path is required"});
        }
        let Some(clean) = normalize_existing_dir(raw) else {
            return json!({"ok": false, "action": "workspace_collapse", "error": format!("Workspace directory not found: {raw}")});
        };
        let requested = request
            .collapsed
            .or(request.saved)
            .or_else(|| request.value.as_ref().and_then(Value::as_bool));
        if !self
            .gui_state
            .workspaces
            .iter()
            .any(|w| db::workspace_paths_equal(w, &clean))
        {
            push_unique_workspace(&mut self.gui_state.workspaces, &clean);
        }
        let currently_collapsed = self
            .gui_state
            .collapsed_workspaces
            .iter()
            .any(|w| db::workspace_paths_equal(w, &clean));
        let next_collapsed = requested.unwrap_or(!currently_collapsed);
        self.gui_state
            .collapsed_workspaces
            .retain(|w| !db::workspace_paths_equal(w, &clean));
        if next_collapsed {
            self.gui_state.collapsed_workspaces.push(clean.clone());
        }
        if let Ok(db) = db::GuiDb::open_default() {
            let _ = db.set_workspace_collapsed(&clean, Some(next_collapsed));
        }
        if let Err(err) = save_gui_state(&self.gui_state) {
            return json!({"ok": false, "action": "workspace_collapse", "error": format!("{err:#}")});
        }
        json!({
            "ok": true,
            "action": "workspace_collapse",
            "working_dir": clean,
            "collapsed": next_collapsed,
            "state": self.gui_state,
        })
    }

    fn gui_state_set(&mut self, request: &BridgeRequest) -> Value {
        let patch = request
            .updates
            .as_ref()
            .or(request.value.as_ref())
            .and_then(Value::as_object);
        let Some(obj) = patch else {
            return json!({"ok": false, "action": "gui_state_set", "error": "Expected state object"});
        };
        if let Some(v) = obj.get("last_working_dir") {
            if v.is_null() {
                self.gui_state.last_working_dir = None;
            } else if let Some(s) = v.as_str()
                && let Some(clean) = normalize_existing_dir(s)
                && !user_home_dir_or_install(Path::new(&clean))
            {
                self.gui_state.last_working_dir = Some(clean.clone());
                self.gui_state.active_workspace = Some(clean.clone());
                push_unique_workspace(&mut self.gui_state.workspaces, &clean);
                let mut owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
                if !owned.busy {
                    owned.working_dir = Some(clean);
                }
            }
        }
        if let Some(v) = obj.get("active_workspace") {
            if v.is_null() {
                self.gui_state.active_workspace = None;
            } else if let Some(s) = v.as_str()
                && let Some(clean) = normalize_existing_dir(s)
                && !user_home_dir_or_install(Path::new(&clean))
            {
                self.gui_state.active_workspace = Some(clean.clone());
                self.gui_state.last_working_dir = Some(clean.clone());
                push_unique_workspace(&mut self.gui_state.workspaces, &clean);
                let mut owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
                if !owned.busy {
                    owned.working_dir = Some(clean);
                }
            }
        }
        if let Some(arr) = obj.get("workspaces").and_then(Value::as_array) {
            self.gui_state.workspaces = arr
                .iter()
                .filter_map(Value::as_str)
                .filter_map(normalize_existing_dir)
                .filter(|w| !user_home_dir_or_install(Path::new(w)))
                .take(24)
                .collect();
        }
        if let Some(arr) = obj.get("pinned_workspaces").and_then(Value::as_array) {
            self.gui_state.pinned_workspaces = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .take(24)
                .collect();
        }
        if let Some(arr) = obj.get("collapsed_workspaces").and_then(Value::as_array) {
            self.gui_state.collapsed_workspaces = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .take(48)
                .collect();
        }
        if let Some(arr) = obj.get("pinned_chats").and_then(Value::as_array) {
            self.gui_state.pinned_chats = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .take(64)
                .collect();
        }
        if let Some(v) = obj.get("last_session_id") {
            self.gui_state.last_session_id = v.as_str().map(str::to_string);
        }
        if let Some(v) = obj.get("theme").and_then(Value::as_str)
            && matches!(v, "dark" | "light")
        {
            self.gui_state.theme = Some(v.to_string());
        }
        if let Some(v) = obj.get("lang").and_then(Value::as_str)
            && matches!(v, "en" | "ar")
        {
            self.gui_state.lang = Some(v.to_string());
        }
        if let Some(v) = obj.get("font_size").and_then(Value::as_u64)
            && (10..=24).contains(&v)
        {
            self.gui_state.font_size = Some(v as u32);
        }
        if let Some(v) = obj.get("sidebar_open").and_then(Value::as_bool) {
            self.gui_state.sidebar_open = Some(v);
        }
        if let Err(err) = save_gui_state(&self.gui_state) {
            return json!({"ok": false, "action": "gui_state_set", "error": format!("{err:#}")});
        }
        json!({"ok": true, "action": "gui_state_set", "state": self.gui_state})
    }

    async fn delete_stored_chat(&mut self, id: &str) -> Result<Value> {
        let id = id.trim();
        if id.is_empty()
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        {
            bail!("Invalid session identifier");
        }
        let owned_id = self
            .owned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .session_id
            .clone();
        if id == OWN_CHAT || owned_id.as_deref() == Some(id) {
            let _ = self.submit_new_chat(None).await;
            if let Some(real_id) = owned_id.as_deref().filter(|sid| *sid != OWN_CHAT) {
                let real_owned = real_id.to_string();
                let removed = tokio::task::spawn_blocking(move || -> Result<bool> {
                    let mut deleted = false;
                    let s_path = crate::session::session_path(&real_owned)?;
                    if s_path.exists() {
                        std::fs::remove_file(&s_path)?;
                        deleted = true;
                    }
                    let j_path = crate::session::session_journal_path(&real_owned)?;
                    if j_path.exists() {
                        let _ = std::fs::remove_file(&j_path);
                    }
                    if let Ok(db) = db::GuiDb::open_default() {
                        let _ = db.delete_chat(&real_owned);
                    }
                    Ok(deleted)
                })
                .await??;
                self.stored_ids.remove(real_id);
                self.gui_state.pinned_chats.retain(|c| c != real_id);
                if self.gui_state.last_session_id.as_deref() == Some(real_id) {
                    self.gui_state.last_session_id = None;
                }
                let _ = save_gui_state(&self.gui_state);
                crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
                return Ok(json!({
                    "ok": true,
                    "action": "delete_chat",
                    "session_id": id,
                    "deleted": removed,
                }));
            }
            return Ok(json!({
                "ok": true,
                "action": "delete_chat",
                "session_id": id,
                "deleted": true,
            }));
        }
        let id_owned = id.to_string();
        let removed = tokio::task::spawn_blocking(move || -> Result<bool> {
            let mut deleted = false;
            let s_path = crate::session::session_path(&id_owned)?;
            if s_path.exists() {
                std::fs::remove_file(&s_path)?;
                deleted = true;
            }
            let j_path = crate::session::session_journal_path(&id_owned)?;
            if j_path.exists() {
                let _ = std::fs::remove_file(&j_path);
            }
            if let Ok(db) = db::GuiDb::open_default() {
                let _ = db.delete_chat(&id_owned);
            }
            Ok(deleted)
        })
        .await??;
        self.stored_ids.remove(id);
        self.gui_state.pinned_chats.retain(|c| c != id);
        if self.gui_state.last_session_id.as_deref() == Some(id) {
            self.gui_state.last_session_id = None;
        }
        let _ = save_gui_state(&self.gui_state);
        crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
        Ok(json!({
            "ok": true,
            "action": "delete_chat",
            "session_id": id,
            "deleted": removed,
        }))
    }

    async fn reveal_path_in_os(&self, raw_path: Option<&str>) -> Value {
        let Some(raw) = raw_path.map(str::trim).filter(|s| !s.is_empty()) else {
            return json!({"ok": false, "action": "reveal_path", "error": "Missing path"});
        };
        let target = PathBuf::from(raw);
        if !target.exists() {
            return json!({"ok": false, "action": "reveal_path", "error": format!("Path not found: {raw}")});
        }
        #[cfg(windows)]
        let res = {
            let mut cmd = std::process::Command::new("explorer.exe");
            if target.is_file() {
                cmd.arg(format!("/select,{}", target.display()));
            } else {
                cmd.arg(&target);
            }
            cmd.spawn()
        };
        #[cfg(target_os = "macos")]
        let res = std::process::Command::new("open").arg(&target).spawn();
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let res = std::process::Command::new("xdg-open").arg(&target).spawn();

        match res {
            Ok(_) => {
                json!({"ok": true, "action": "reveal_path", "path": target.display().to_string()})
            }
            Err(err) => {
                json!({"ok": false, "action": "reveal_path", "error": format!("{err}")})
            }
        }
    }
}

fn display_text(text: &str, limit: usize) -> String {
    crate::message::strip_ansi_escape_sequences(text)
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .take(limit)
        .collect()
}

fn display_text_tail(text: &str, limit: usize) -> String {
    let cleaned: Vec<char> = crate::message::strip_ansi_escape_sequences(text)
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect();
    if cleaned.len() <= limit {
        cleaned.into_iter().collect()
    } else {
        cleaned[cleaned.len() - limit..].iter().collect()
    }
}

fn append_bounded(buf: &mut String, text: &str, max_bytes: usize) {
    buf.push_str(text);
    if buf.len() > max_bytes {
        let mut start = buf.len() - max_bytes;
        while !buf.is_char_boundary(start) {
            start += 1;
        }
        buf.drain(..start);
    }
}

fn append_reply(reply: &mut String, text: &str) {
    append_bounded(reply, text, MAX_REPLY_BYTES);
}

fn compute_tokens_per_sec(output_tokens: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if output_tokens == 0 || secs <= 0.001 {
        return 0.0;
    }
    ((output_tokens as f64 / secs) * 10.0).round() / 10.0
}

fn resolve_context_max(model: &str, provider: &str) -> Option<usize> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    let resolved =
        crate::alphacode_app_core::agent::free_pool_rotation::resolve_free_pool_model_selection(
            trimmed,
        )
        .unwrap_or(trimmed);
    let provider_hint = (!provider.trim().is_empty()).then_some(provider.trim());
    crate::provider::context_limit_for_model_with_provider(resolved, provider_hint).or_else(|| {
        crate::alphacode_provider_metadata::free_pool::free_model_context_limit(resolved)
    })
}

fn compute_session_cost_usd(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> Option<f64> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    let resolved =
        crate::alphacode_app_core::agent::free_pool_rotation::resolve_free_pool_model_selection(
            trimmed,
        )
        .unwrap_or(trimmed);
    let usage = crate::usage_ledger::ModelUsage {
        requests: 1,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    };
    crate::usage_ledger::estimate_cost_usd(resolved, &usage)
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

async fn git_current_branch(directory: &std::path::Path) -> String {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(directory)
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {
            let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !branch.is_empty() {
                branch
            } else {
                "main".to_string()
            }
        }
        _ => "main".to_string(),
    }
}

async fn pick_folder() -> Value {
    #[cfg(windows)]
    {
        let ps_script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = "Select AlphaCode Workspace Folder"
$dialog.ShowNewFolderButton = $true
$topForm = New-Object System.Windows.Forms.Form
$topForm.TopMost = $true
if ($dialog.ShowDialog($topForm) -eq [System.Windows.Forms.DialogResult]::OK) {
    Write-Output $dialog.SelectedPath
}
"#;
        let mut cmd = tokio::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", ps_script]);
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                let chosen = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !chosen.is_empty() && std::path::Path::new(&chosen).is_dir() {
                    json!({"ok": true, "action": "pick_folder", "path": chosen, "cancelled": false})
                } else {
                    json!({"ok": true, "action": "pick_folder", "cancelled": true})
                }
            }
            _ => json!({"ok": true, "action": "pick_folder", "cancelled": true}),
        }
    }
    #[cfg(not(windows))]
    {
        json!({"ok": false, "action": "pick_folder", "error": "Folder picker is only supported on Windows"})
    }
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn canonicalize_workspace_root(workspace: &Path) -> Result<PathBuf> {
    if !workspace.is_dir() {
        bail!("Workspace directory not found: {}", workspace.display());
    }
    workspace.canonicalize().with_context(|| {
        format!(
            "Cannot resolve workspace directory: {}",
            workspace.display()
        )
    })
}

fn validate_requested_rel_or_abs(requested: &str) -> Result<&str> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        bail!("File path is required");
    }
    if trimmed.contains('\0') {
        bail!("Invalid path: NUL byte is not allowed");
    }
    Ok(trimmed)
}

fn resolve_existing_in_workspace(workspace: &Path, requested: &str) -> Result<PathBuf> {
    let requested = validate_requested_rel_or_abs(requested)?;
    let canonical_dir = canonicalize_workspace_root(workspace)?;
    let target_path = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        canonical_dir.join(requested)
    };
    let canonical_target = target_path
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("Path not found: {}", target_path.display()))?;
    if !canonical_target.starts_with(&canonical_dir) {
        bail!("Access denied: path is outside the active workspace");
    }
    Ok(canonical_target)
}

fn resolve_target_in_workspace(workspace: &Path, requested: &str) -> Result<(PathBuf, PathBuf)> {
    let requested = validate_requested_rel_or_abs(requested)?;
    let canonical_dir = canonicalize_workspace_root(workspace)?;
    let raw_target = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        canonical_dir.join(requested)
    };
    let normalized = lexically_normalize_path(&raw_target);
    if !normalized.starts_with(&canonical_dir) {
        bail!("Access denied: path is outside the active workspace");
    }

    // Walk up to the deepest existing ancestor so symlinked directories cannot escape.
    let mut existing = normalized.as_path();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name.to_os_string());
                existing = parent;
            }
            _ => bail!("Access denied: path has no valid workspace ancestor"),
        }
    }
    let canonical_ancestor = existing
        .canonicalize()
        .with_context(|| format!("Cannot resolve ancestor directory: {}", existing.display()))?;
    if !canonical_ancestor.starts_with(&canonical_dir) {
        bail!("Access denied: path resolves outside the active workspace");
    }
    let mut resolved = canonical_ancestor;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    if !resolved.starts_with(&canonical_dir) {
        bail!("Access denied: path is outside the active workspace");
    }
    Ok((canonical_dir, resolved))
}

fn write_file_in_workspace(workspace: &Path, requested: &str, content: &str) -> Result<Value> {
    if content.len() > MAX_FILE_WRITE_BYTES {
        bail!(
            "File content exceeds maximum allowed size of {} bytes",
            MAX_FILE_WRITE_BYTES
        );
    }
    let (canonical_dir, target) = resolve_target_in_workspace(workspace, requested)?;
    if target == canonical_dir || target.is_dir() {
        bail!("Target path is a directory, not a file");
    }
    if let Some(parent) = target.parent()
        && !parent.exists()
    {
        if !parent.starts_with(&canonical_dir) {
            bail!("Access denied: parent directory is outside the active workspace");
        }
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory: {}", parent.display()))?;
    }
    std::fs::write(&target, content.as_bytes())
        .with_context(|| format!("Failed to write file: {}", target.display()))?;
    Ok(json!({
        "ok": true,
        "action": "write_file",
        "path": requested.trim(),
        "bytes_written": content.len(),
    }))
}

fn create_file_in_workspace(workspace: &Path, requested: &str, content: &str) -> Result<Value> {
    if content.len() > MAX_FILE_WRITE_BYTES {
        bail!(
            "File content exceeds maximum allowed size of {} bytes",
            MAX_FILE_WRITE_BYTES
        );
    }
    let (canonical_dir, target) = resolve_target_in_workspace(workspace, requested)?;
    if target == canonical_dir {
        bail!("Cannot overwrite workspace root");
    }
    if target.exists() {
        bail!("Path already exists: {}", requested.trim());
    }
    if let Some(parent) = target.parent()
        && !parent.exists()
    {
        if !parent.starts_with(&canonical_dir) {
            bail!("Access denied: parent directory is outside the active workspace");
        }
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory: {}", parent.display()))?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .with_context(|| format!("Failed to create file: {}", target.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write file: {}", target.display()))?;
    Ok(json!({
        "ok": true,
        "action": "create_file",
        "path": requested.trim(),
        "bytes_written": content.len(),
    }))
}

fn create_dir_in_workspace(workspace: &Path, requested: &str) -> Result<Value> {
    let (canonical_dir, target) = resolve_target_in_workspace(workspace, requested)?;
    if target == canonical_dir {
        bail!("Directory already exists");
    }
    if target.exists() {
        bail!("Path already exists: {}", requested.trim());
    }
    std::fs::create_dir_all(&target)
        .with_context(|| format!("Failed to create directory: {}", target.display()))?;
    Ok(json!({
        "ok": true,
        "action": "create_dir",
        "path": requested.trim(),
    }))
}

fn delete_path_in_workspace(workspace: &Path, requested: &str) -> Result<Value> {
    let canonical_dir = canonicalize_workspace_root(workspace)?;
    let target = resolve_existing_in_workspace(workspace, requested)?;
    if target == canonical_dir {
        bail!("Access denied: cannot delete the workspace root");
    }
    let kind = if target.is_dir() {
        std::fs::remove_dir_all(&target)
            .with_context(|| format!("Failed to delete directory: {}", target.display()))?;
        "dir"
    } else {
        std::fs::remove_file(&target)
            .with_context(|| format!("Failed to delete file: {}", target.display()))?;
        "file"
    };
    Ok(json!({
        "ok": true,
        "action": "delete_path",
        "path": requested.trim(),
        "kind": kind,
    }))
}

fn rename_path_in_workspace(
    workspace: &Path,
    old_requested: &str,
    new_requested: &str,
) -> Result<Value> {
    let canonical_dir = canonicalize_workspace_root(workspace)?;
    let old_target = resolve_existing_in_workspace(workspace, old_requested)?;
    if old_target == canonical_dir {
        bail!("Access denied: cannot rename the workspace root");
    }
    let (_, new_target) = resolve_target_in_workspace(workspace, new_requested)?;
    if new_target == canonical_dir {
        bail!("Access denied: destination cannot be the workspace root");
    }
    if new_target.exists() {
        bail!("Destination already exists: {}", new_requested.trim());
    }
    if let Some(parent) = new_target.parent()
        && !parent.exists()
    {
        if !parent.starts_with(&canonical_dir) {
            bail!("Access denied: destination parent is outside the active workspace");
        }
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create destination parent: {}", parent.display())
        })?;
    }
    std::fs::rename(&old_target, &new_target).with_context(|| {
        format!(
            "Failed to rename {} -> {}",
            old_target.display(),
            new_target.display()
        )
    })?;
    Ok(json!({
        "ok": true,
        "action": "rename_path",
        "path": old_requested.trim(),
        "new_path": new_requested.trim(),
    }))
}

fn cap_utf8_bytes(bytes: &[u8], cap: usize) -> (String, bool) {
    if bytes.len() <= cap {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let mut text = String::from_utf8_lossy(&bytes[..cap]).into_owned();
    text.push_str("\nâ€¦[output truncated]");
    (text, true)
}

async fn shell_exec_in_workspace(
    workspace: &Path,
    command: &str,
    timeout_ms: Option<u64>,
    max_output_bytes: Option<usize>,
) -> Result<Value> {
    let command = command.trim();
    if command.is_empty() {
        bail!("Command is required");
    }
    if command.len() > 32 * 1024 {
        bail!("Command exceeds 32KB limit");
    }
    let canonical_dir = canonicalize_workspace_root(workspace)?;
    let timeout_ms = timeout_ms
        .unwrap_or(DEFAULT_SHELL_TIMEOUT_MS)
        .clamp(500, MAX_SHELL_TIMEOUT_MS);
    let max_output = max_output_bytes
        .unwrap_or(DEFAULT_SHELL_OUTPUT_BYTES)
        .clamp(1024, MAX_SHELL_OUTPUT_BYTES);

    #[cfg(windows)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };

    cmd.current_dir(&canonical_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    crate::alphacode_core::env::strip_internal_env(&mut cmd);

    let started = Instant::now();
    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn shell in {}", canonical_dir.display()))?;

    let mut stdout_pipe = child.stdout.take().context("Missing stdout pipe")?;
    let mut stderr_pipe = child.stderr.take().context("Missing stderr pipe")?;

    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = stdout_pipe.read(&mut chunk).await {
            if n == 0 {
                break;
            }
            if buf.len() <= max_output {
                let take = (max_output + 1).saturating_sub(buf.len()).min(n);
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        buf
    });

    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = stderr_pipe.read(&mut chunk).await {
            if n == 0 {
                break;
            }
            if buf.len() <= max_output {
                let take = (max_output + 1).saturating_sub(buf.len()).min(n);
                buf.extend_from_slice(&chunk[..take]);
            }
        }
        buf
    });

    let wait_res = tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait()).await;
    let (exit_code, timed_out) = match wait_res {
        Ok(Ok(status)) => (status.code().unwrap_or(-1), false),
        Ok(Err(err)) => bail!("Failed waiting for command: {err}"),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            (-1, true)
        }
    };

    let duration_ms = started.elapsed().as_millis() as u64;
    let raw_stdout = stdout_task.await.unwrap_or_default();
    let raw_stderr = stderr_task.await.unwrap_or_default();
    let (stdout, out_trunc) = cap_utf8_bytes(&raw_stdout, max_output);
    let (stderr, err_trunc) = cap_utf8_bytes(&raw_stderr, max_output);

    Ok(json!({
        "ok": !timed_out && exit_code == 0,
        "action": "shell_exec",
        "command": command,
        "working_dir": canonical_dir.display().to_string(),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "duration_ms": duration_ms,
        "timed_out": timed_out,
        "truncated": out_trunc || err_trunc,
    }))
}

fn validate_git_pathspec(workspace: &Path, raw: &str) -> Result<String> {
    let trimmed = validate_requested_rel_or_abs(raw)?;
    if trimmed.starts_with('-') {
        bail!("Invalid git pathspec");
    }
    let (canonical_dir, target) = resolve_target_in_workspace(workspace, trimmed)?;
    let rel = target
        .strip_prefix(&canonical_dir)
        .context("Path is outside workspace")?;
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    if rel_str.is_empty() || rel_str.starts_with('-') {
        bail!("Invalid git pathspec");
    }
    Ok(rel_str)
}

async fn git_action_in_workspace(
    workspace: &Path,
    op: &str,
    path: Option<&str>,
    message: Option<&str>,
) -> Result<Value> {
    let canonical_dir = canonicalize_workspace_root(workspace)?;
    let op = op.trim().to_ascii_lowercase();
    let mut cmd = tokio::process::Command::new("git");
    cmd.current_dir(&canonical_dir);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match op.as_str() {
        "stage" => {
            if let Some(p) = path.map(str::trim).filter(|p| !p.is_empty()) {
                let rel = validate_git_pathspec(&canonical_dir, p)?;
                cmd.args(["add", "--", &rel]);
            } else {
                cmd.args(["add", "-A"]);
            }
        }
        "unstage" => {
            if let Some(p) = path.map(str::trim).filter(|p| !p.is_empty()) {
                let rel = validate_git_pathspec(&canonical_dir, p)?;
                cmd.args(["reset", "-q", "HEAD", "--", &rel]);
            } else {
                cmd.args(["reset", "-q", "HEAD"]);
            }
        }
        "discard" => {
            let p = path
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .context("File path is required for git discard")?;
            let rel = validate_git_pathspec(&canonical_dir, p)?;
            // Check if the path is untracked; if so, remove it safely within workspace.
            let untracked = tokio::process::Command::new("git")
                .args(["ls-files", "--others", "--exclude-standard", "--", &rel])
                .current_dir(&canonical_dir)
                .output()
                .await
                .ok()
                .is_some_and(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty());
            if untracked {
                let _ = delete_path_in_workspace(&canonical_dir, &rel)?;
                let diff_out = tokio::process::Command::new("git")
                    .args(["diff", "HEAD"])
                    .current_dir(&canonical_dir)
                    .output()
                    .await
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .unwrap_or_default();
                let status_out = tokio::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&canonical_dir)
                    .output()
                    .await
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .unwrap_or_default();
                let git_branch = git_current_branch(&canonical_dir).await;
                return Ok(json!({
                    "ok": true,
                    "action": "git_action",
                    "git_op": op,
                    "working_dir": canonical_dir.display().to_string(),
                    "stdout": format!("Removed untracked {rel}"),
                    "stderr": "",
                    "diff": diff_out,
                    "status": status_out,
                    "git_branch": git_branch,
                }));
            }
            cmd.args(["checkout", "--", &rel]);
        }
        "commit" => {
            let msg = message
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .context("Commit message is required")?;
            if msg.len() > 4096 {
                bail!("Commit message must be 4096 characters or fewer");
            }
            cmd.args(["commit", "-m", msg]);
        }
        other => bail!("Unsupported git_op `{other}`; expected stage, unstage, discard, or commit"),
    }

    let out = cmd.output().await.context("Failed to execute git")?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("git {op} failed with exit code {:?}", out.status.code())
        };
        bail!("{detail}");
    }

    let diff_out = tokio::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(&canonical_dir)
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let status_out = tokio::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&canonical_dir)
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let git_branch = git_current_branch(&canonical_dir).await;

    Ok(json!({
        "ok": true,
        "action": "git_action",
        "git_op": op,
        "working_dir": canonical_dir.display().to_string(),
        "stdout": stdout,
        "stderr": stderr,
        "diff": diff_out,
        "status": status_out,
        "git_branch": git_branch,
    }))
}

impl Bridge {
    fn resolve_workspace_dir(&self, custom_dir: Option<&str>, selected: Option<&str>) -> PathBuf {
        custom_dir
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                selected
                    .filter(|id| *id != OWN_CHAT)
                    .and_then(|id| self.sessions.get(id))
                    .and_then(|session| session.working_dir.clone())
                    .map(PathBuf::from)
            })
            .or_else(|| {
                self.owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .working_dir
                    .clone()
                    .map(PathBuf::from)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    fn has_valid_owned_workspace(&self) -> bool {
        self.owned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .working_dir
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .is_some_and(|d| {
                let path = Path::new(d);
                path.is_dir() && !user_home_dir_or_install(path)
            })
    }

    fn ensure_owned_chat(&mut self) {
        if self
            .prompt_tx
            .as_ref()
            .is_some_and(|sender| !sender.is_closed())
        {
            return;
        }
        if !self.has_valid_owned_workspace() {
            return;
        }
        self.start_owned_chat(None);
    }

    fn start_owned_chat(&mut self, client: Option<BridgeSocketClient>) {
        let (tx, rx) = mpsc::channel(1);
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let (control_tx, control_rx) = mpsc::channel(8);
        self.prompt_tx = Some(tx);
        self.cancel_tx = Some(cancel_tx);
        self.control_tx = Some(control_tx);
        let state = Arc::clone(&self.owned);
        tokio::spawn(async move {
            if let Err(error) =
                run_owned_chat(Arc::clone(&state), rx, cancel_rx, control_rx, client).await
            {
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                state.busy = false;
                state.live_thinking.active = false;
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
        if self.is_owned_selected(selected) {
            return None;
        }
        let id = selected.filter(|id| *id != OWN_CHAT)?;
        let value = tokio::time::timeout(
            Duration::from_millis(1200),
            self.desktop_output("history", Some(id), None),
        )
        .await
        .ok()?
        .ok()?;
        // Render the whole conversation, not just the last assistant turn.
        //
        // This used to `.rev().find(assistant)`, so the desktop showed only the
        // most recent answer while the terminal showed every exchange. After a
        // multi-step task ("search subdomains", then "search for bugs") the GUI
        // looked like earlier results had vanished (issue #38). Build the same
        // role-labelled transcript the terminal renders, keeping the most recent
        // messages when the conversation exceeds the reply budget.
        let messages = value.get("messages")?.as_array()?;
        let mut conversation = String::new();
        for message in messages {
            let role = match message.get("role").and_then(Value::as_str) {
                Some("user") => "You",
                Some("assistant") => "AlphaCode",
                _ => continue,
            };
            let Some(body) = message.get("content").and_then(Value::as_str) else {
                continue;
            };
            if body.trim().is_empty() {
                continue;
            }
            append_reply(
                &mut conversation,
                &format!("{role}\n{}\n\n", display_text(body, 8000)),
            );
        }
        (!conversation.trim().is_empty())
            .then(|| display_text(conversation.trim_end(), MAX_REPLY_BYTES))
    }

    async fn refresh_runtime_context(&mut self, selected: Option<&str>) {
        let selected = selected.unwrap_or(OWN_CHAT);
        if self.runtime_for.as_deref() != Some(selected) {
            self.model.clear();
            self.provider.clear();
            self.models.clear();
            self.model_options.clear();
            self.effort.clear();
            self.runtime_for = Some(selected.to_string());
        }
        // Do not keep offering a stale catalog after disconnect/auth changes.
        self.models.clear();
        self.model_options.clear();
        let auth_epoch = provider_connections::auth_completed_epoch();
        let action = if auth_epoch != self.last_auth_epoch {
            "refresh_auth"
        } else {
            "state"
        };
        if let Some(value) = tokio::time::timeout(
            Duration::from_millis(1200),
            self.desktop_output(action, Some(selected), None),
        )
        .await
        .ok()
        .and_then(Result::ok)
        {
            self.last_auth_epoch = auth_epoch;
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
            if self.is_owned_selected(Some(selected)) {
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
            self.model_options = value.get("model_options").and_then(Value::as_array)
                .map(|choices| choices.iter().filter_map(|choice| {
                    let id = choice.get("id")?.as_str()?;
                    if !self.models.iter().any(|model| model == id) { return None; }
                    let id = hidden_pool_models_to_display_names(vec![id.into()]).into_iter().next()?;
                    Some(json!({"id": id, "provider": choice.get("provider").and_then(Value::as_str).unwrap_or(&self.provider)}))
                }).collect()).unwrap_or_else(|| {
                    // An older daemon supplies no connection provenance for
                    // its full catalog. Offer only its active provider/model.
                    self.models.retain(|model| model == &self.model && !self.provider.is_empty());
                    self.models.iter().map(|id| json!({"id": id, "provider": self.provider})).collect()
                });
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
        self.models.retain(|model| {
            self.model_options
                .iter()
                .any(|choice| choice["id"].as_str() == Some(model.as_str()))
        });
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
        let target = if self.is_owned_selected(selected) {
            Some(OWN_CHAT)
        } else {
            selected
        };
        let result = self.desktop_output("model", target, Some(model)).await?;
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
        let target = if self.is_owned_selected(selected) {
            Some(OWN_CHAT)
        } else {
            selected
        };
        let result = self.desktop_output("effort", target, Some(&effort)).await?;
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
        selected
            .map(str::trim)
            .is_none_or(|id| id.is_empty() || id == OWN_CHAT)
    }

    fn is_owned_selected(&self, selected: Option<&str>) -> bool {
        if Self::is_owned(selected) {
            return true;
        }
        let target = selected.map(str::trim).unwrap_or("");
        let owned_id = self
            .owned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .session_id
            .clone();
        !target.is_empty() && owned_id.as_deref() == Some(target)
    }

    /// Cancel the in-flight generation for the selected chat.
    ///
    /// This mirrors the terminal's Esc: it fires the session's cancel signal
    /// through the standard `Cancel` request, so the running turn stops for
    /// real instead of only clearing the desktop's spinner.
    async fn submit_cancel(&mut self, selected: Option<&str>) -> Result<Value> {
        if self.is_owned_selected(selected) {
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
                .detail = "Cancelling the current turnâ€¦".to_string();
            return Ok(json!({"ok": true, "cancelled": true}));
        }

        let result = self.desktop_output("cancel", selected, None).await?;
        Ok(
            json!({"ok": true, "cancelled": result.get("cancelled").and_then(Value::as_bool).unwrap_or(false)}),
        )
    }

    /// Rename the selected chat (works for live sessions AND stored sessions on disk).
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
        let owned_id = self
            .owned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .session_id
            .clone();
        let targets_owned = self.is_owned_selected(selected);
        let is_live = targets_owned || selected.is_some_and(|id| self.sessions.contains_key(id));

        if is_live {
            let target = if targets_owned {
                Some(OWN_CHAT)
            } else {
                selected
            };
            let result = self.desktop_output("rename", target, normalized).await?;
            let display_title = result
                .get("display_title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| normalized.map(str::to_string));
            if targets_owned {
                self.owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .title = display_title.clone();
            }
            let db_target_id = if targets_owned {
                owned_id
            } else {
                selected.map(str::to_string)
            };
            if let Some(sid) = db_target_id.filter(|s| !s.is_empty() && s != OWN_CHAT) {
                let custom = normalized.map(|_| display_title.as_deref().unwrap_or_default());
                if let Ok(db) = db::GuiDb::open_default() {
                    let _ = db.set_chat_title(&sid, display_title.as_deref(), custom);
                }
            }
            crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
            return Ok(json!({"ok": true, "detail": result}));
        }

        let Some(id) = selected.map(str::trim).filter(|id| !id.is_empty()) else {
            bail!("Select a chat to rename");
        };
        let id_owned = id.to_string();
        let next_title = normalized.map(str::to_string);
        let display_title = tokio::task::spawn_blocking(move || -> Result<String> {
            let mut session = crate::session::Session::load(&id_owned)?;
            session.rename_title(next_title);
            let final_title = session.display_title_or_name().to_string();
            session.save()?;
            if let Ok(db) = db::GuiDb::open_default() {
                let _ = db.upsert_chat_from_session(&session);
            }
            Ok(final_title)
        })
        .await??;
        crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
        Ok(json!({"ok": true, "detail": {"display_title": display_title}}))
    }

    /// Bookmark/unbookmark ("archive") the selected chat (works for live sessions AND stored sessions on disk).
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
        let owned_id = self
            .owned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .session_id
            .clone();
        let targets_owned = self.is_owned_selected(selected);
        let is_live = targets_owned || selected.is_some_and(|id| self.sessions.contains_key(id));

        if is_live {
            let target = if targets_owned {
                Some(OWN_CHAT)
            } else {
                selected
            };
            let result = self
                .desktop_output(if saved { "save" } else { "unsave" }, target, label)
                .await?;
            if targets_owned {
                self.owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .saved = saved;
            }
            let db_target_id = if targets_owned {
                owned_id
            } else {
                selected.map(str::to_string)
            };
            if let Some(sid) = db_target_id.filter(|s| !s.is_empty() && s != OWN_CHAT)
                && let Ok(db) = db::GuiDb::open_default()
            {
                let _ = db.set_chat_saved(&sid, saved, label);
            }
            crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
            return Ok(json!({"ok": true, "saved": saved, "detail": result}));
        }

        let Some(id) = selected.map(str::trim).filter(|id| !id.is_empty()) else {
            bail!("Select a chat to bookmark");
        };
        let id_owned = id.to_string();
        let label_owned = label.map(str::to_string);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut session = crate::session::Session::load(&id_owned)?;
            if saved {
                session.mark_saved(label_owned);
            } else {
                session.unmark_saved();
            }
            session.save()?;
            if let Ok(db) = db::GuiDb::open_default() {
                let _ = db.upsert_chat_from_session(&session);
            }
            Ok(())
        })
        .await??;
        crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
        Ok(json!({"ok": true, "saved": saved}))
    }

    /// Canonical on-disk session index, identical to the CLI session picker.
    async fn list_stored_sessions(&mut self, limit: usize) -> Result<Value> {
        let pinned_chats = self.gui_state.pinned_chats.clone();
        let output = tokio::task::spawn_blocking(move || {
            if let Ok(base) = crate::storage::alphacode_dir()
                && let Ok(db) = db::GuiDb::open_default()
            {
                let _ = db.sync_sessions_index(&base.join("sessions"));
            }
            session_index::desktop_session_index()
        })
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
            for row in sessions.iter_mut() {
                if let Some(id) = row.get("id").and_then(Value::as_str).map(str::to_string) {
                    let is_pinned = pinned_chats.iter().any(|p| p == &id);
                    row["pinned"] = json!(is_pinned);
                }
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

    /// Replace the companion conversation with a fresh session.
    async fn submit_new_chat(&mut self, working_dir: Option<&str>) -> Result<Value> {
        let prev_working_dir = {
            let state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
            if state.busy {
                bail!("Finish or cancel the current turn before starting a new chat");
            }
            state.working_dir.clone()
        };
        // Dropping the senders releases the old connection. Give the new worker
        // its own state so a late disconnect cannot overwrite the new chat.
        self.prompt_tx = None;
        self.cancel_tx = None;
        self.control_tx = None;
        let explicit_dir = working_dir
            .map(str::trim)
            .filter(|dir| !dir.is_empty())
            .map(str::to_string);
        let normalized_dir = if let Some(ref dir) = explicit_dir {
            if let Some(existing) = normalize_existing_dir(dir) {
                if user_home_dir_or_install(Path::new(&existing)) {
                    None
                } else {
                    self.record_workspace_visit(&existing, true);
                    Some(existing)
                }
            } else if user_home_dir_or_install(Path::new(dir)) {
                None
            } else {
                Some(dir.clone())
            }
        } else {
            prev_working_dir
                .or_else(|| self.gui_state.active_workspace.clone())
                .or_else(|| self.gui_state.last_working_dir.clone())
                .and_then(|d| normalize_existing_dir(&d))
                .filter(|d| !user_home_dir_or_install(Path::new(d)))
        };
        self.owned = Arc::new(Mutex::new(OwnedChat {
            detail: "New chat ready. Send a prompt to begin.".to_string(),
            working_dir: normalized_dir.clone(),
            ..OwnedChat::default()
        }));
        self.runtime_for = None;
        Ok(json!({
            "ok": true,
            "action": "new_chat",
            "new_chat": true,
            "session_id": OWN_CHAT,
            "working_dir": normalized_dir,
        }))
    }

    async fn resume_stored_chat(&mut self, id: &str) -> Result<Value> {
        let id = id.trim();
        let on_disk = is_safe_session_id(id) && crate::session::session_exists(id);
        if (!self.stored_ids.contains(id) && !on_disk) || self.sessions.contains_key(id) {
            bail!("Select a local AlphaCode chat from saved history to resume it");
        }
        {
            let state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
            if state.busy {
                bail!("Finish or cancel the current desktop turn before resuming another chat");
            }
        }

        let id = id.to_string();
        let (mut restored, transcript_json) = tokio::task::spawn_blocking(move || {
            let session = crate::session::Session::load(&id)?;
            let working_dir = session.working_dir.clone();
            if let Some(path) = &working_dir
                && !std::path::Path::new(path).is_dir()
            {
                bail!("The saved workspace is unavailable: {path}");
            }
            let transcript_json = build_stored_transcript_json(&session);
            let mut transcript = String::new();
            let history = crate::session::render_messages(&session);
            let count = history.len();
            for message in history.into_iter().skip(count.saturating_sub(40)) {
                let role = match message.role.as_str() {
                    "user" => "You",
                    "assistant" => "AlphaCode",
                    _ => "Session",
                };
                let body = display_text(&message.content, 4000);
                if !body.trim().is_empty() {
                    append_reply(&mut transcript, &format!("{role}\n{body}\n\n"));
                }
            }
            Ok::<_, anyhow::Error>((
                OwnedChat {
                    session_id: Some(id),
                    working_dir,
                    title: Some(session.display_title_or_name().to_string()),
                    saved: session.saved,
                    transcript,
                    detail: "Resumed chat. Ready for your next task.".to_string(),
                    ..OwnedChat::default()
                },
                transcript_json,
            ))
        })
        .await
        .context("Could not read the saved chat")??;
        let (client, session_id) =
            connect_owned_chat(restored.working_dir.clone(), restored.session_id.take()).await?;
        restored.session_id = Some(session_id);

        // Keep the previous conversation available until the daemon confirms
        // that the new connection is attached to the requested session.
        self.owned = Arc::new(Mutex::new(restored));
        self.runtime_for = None;
        self.start_owned_chat(Some(client));
        let snapshot = self.snapshot(Some(OWN_CHAT)).await;
        Ok(json!({
            "ok": true,
            "snapshot": snapshot,
            "transcript": transcript_json,
        }))
    }

    /// Tools/skills/MCP servers available to the selected chat, so the desktop
    /// shows the same capabilities and schemas the terminal session has.
    async fn runtime_capabilities(&self, selected: Option<&str>) -> Value {
        let registry = gui_tool_registry().await;
        let definitions = registry.definitions(None).await;
        let def_map: BTreeMap<String, crate::message::ToolDefinition> = definitions
            .into_iter()
            .map(|def| (def.name.clone(), def))
            .collect();

        let live_tools = tokio::time::timeout(
            Duration::from_secs(3),
            self.desktop_output("tools", selected, None),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|output| serde_json::from_value::<Vec<String>>(output).ok());

        let mut tools = live_tools.unwrap_or_else(|| def_map.keys().cloned().collect());
        tools.sort();
        tools.dedup();

        let tool_definitions: Vec<Value> = tools
            .iter()
            .map(|name| {
                let category = if name.starts_with("mcp__") {
                    "mcp"
                } else if matches!(
                    name.as_str(),
                    "subfinder"
                        | "httpx"
                        | "waybackurls"
                        | "gau"
                        | "katana"
                        | "ffuf"
                        | "dnsx"
                        | "nuclei"
                        | "naabu"
                        | "amass"
                        | "assetfinder"
                        | "gobuster"
                        | "sqlmap"
                        | "nmap"
                        | "nikto"
                        | "unfurl"
                        | "meg"
                        | "gf"
                        | "feroxbuster"
                        | "hakrawler"
                        | "gospider"
                        | "dalfox"
                        | "kxss"
                        | "corsy"
                        | "crlfuzz"
                        | "cariddi"
                        | "httprobe"
                        | "anew"
                        | "qsreplace"
                        | "ctf_web_recon"
                ) {
                    "recon"
                } else {
                    match crate::alphacode_usage_types::classify_telemetry_tool_category(name) {
                        crate::alphacode_usage_types::TelemetryToolCategory::ReadSearch => {
                            "read_search"
                        }
                        crate::alphacode_usage_types::TelemetryToolCategory::Write => "write",
                        crate::alphacode_usage_types::TelemetryToolCategory::Shell => "shell",
                        crate::alphacode_usage_types::TelemetryToolCategory::Web => "web",
                        crate::alphacode_usage_types::TelemetryToolCategory::Memory => "memory",
                        crate::alphacode_usage_types::TelemetryToolCategory::Subagent => "subagent",
                        crate::alphacode_usage_types::TelemetryToolCategory::Swarm => "swarm",
                        crate::alphacode_usage_types::TelemetryToolCategory::Email => "email",
                        crate::alphacode_usage_types::TelemetryToolCategory::SidePanel => {
                            "side_panel"
                        }
                        crate::alphacode_usage_types::TelemetryToolCategory::Goal => "goal",
                        crate::alphacode_usage_types::TelemetryToolCategory::Todo => "todo",
                        crate::alphacode_usage_types::TelemetryToolCategory::Mcp => "mcp",
                        crate::alphacode_usage_types::TelemetryToolCategory::Other => "core",
                    }
                };
                if let Some(def) = def_map.get(name) {
                    json!({
                        "name": def.name,
                        "description": def.description,
                        "input_schema": def.input_schema,
                        "category": category,
                    })
                } else {
                    json!({
                        "name": name,
                        "description": "",
                        "input_schema": { "type": "object", "properties": {} },
                        "category": category,
                    })
                }
            })
            .collect();

        let mut connected_mcp: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for tool in &tools {
            if let Some(rest) = tool.strip_prefix("mcp__")
                && let Some((server, tool_name)) = rest.split_once("__")
            {
                connected_mcp
                    .entry(server.to_string())
                    .or_default()
                    .push(tool_name.to_string());
            }
        }
        let servers: Vec<String> = connected_mcp.keys().cloned().collect();

        let directory = self.resolve_workspace_dir(None, selected);
        let dir_for_skills = directory.clone();
        let (skill_names, skill_items) = tokio::task::spawn_blocking(move || {
            let base = crate::skill::SkillRegistry::shared_snapshot();
            let registry = crate::skill::SkillRegistry::effective_for_working_dir(
                &base,
                Some(dir_for_skills.as_path()),
            );
            let mut list = registry.list();
            list.sort_by(|a, b| a.name.cmp(&b.name));
            let names: Vec<String> = list.iter().map(|s| s.name.clone()).collect();
            let items: Vec<Value> = list
                .into_iter()
                .map(|skill| {
                    json!({
                        "name": skill.name,
                        "description": skill.description,
                        "path": skill.path.display().to_string(),
                        "aliases": skill.aliases,
                        "triggers": skill.triggers,
                        "auto_invoke": skill.auto_invoke,
                    })
                })
                .collect();
            (names, items)
        })
        .await
        .unwrap_or_default();

        let dir_for_mcp = directory;
        let mcp_info = tokio::task::spawn_blocking(move || {
            let config = crate::mcp::McpConfig::load_for_dir(Some(dir_for_mcp.as_path()));
            let config_path = crate::storage::alphacode_dir()
                .ok()
                .map(|dir| dir.join("mcp.json"))
                .filter(|path| path.exists())
                .map(|path| path.display().to_string());
            let mut configured: Vec<Value> = config
                .servers
                .iter()
                .map(|(name, cfg)| {
                    let connected_tools = connected_mcp.get(name).cloned().unwrap_or_default();
                    json!({
                        "name": name,
                        "command": cfg.command,
                        "args": cfg.args,
                        "transport": cfg.transport,
                        "url": cfg.url,
                        "enabled": cfg.is_enabled(),
                        "project_local": cfg.is_project_local(),
                        "connected": !connected_tools.is_empty(),
                        "tools": connected_tools,
                    })
                })
                .collect();
            configured.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            json!({
                "servers": servers,
                "configured": configured,
                "config_path": config_path,
            })
        })
        .await
        .unwrap_or_else(|_| json!({"servers": [], "configured": [], "config_path": null}));

        json!({
            "ok": true,
            "action": "capabilities",
            "tools": tools,
            "tool_definitions": tool_definitions,
            "skills": {
                "skills": skill_names,
                "items": skill_items,
            },
            "mcp": mcp_info,
        })
    }

    /// Conversation transcript for the selected chat.
    async fn transcript(&self, selected: Option<&str>) -> Value {
        let Some(id) = selected.filter(|id| *id != OWN_CHAT) else {
            return json!({"ok": false, "error": "Transcript is only available for server sessions"});
        };
        if !self.sessions.contains_key(id)
            && (self.stored_ids.contains(id)
                || (is_safe_session_id(id) && crate::session::session_exists(id)))
        {
            return load_stored_transcript_response(id).await;
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

    async fn diff(&self, selected: Option<&str>) -> Value {
        let directory = self.resolve_workspace_dir(None, selected);

        let diff_out = tokio::process::Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&directory)
            .output()
            .await;

        let status_out = tokio::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&directory)
            .output()
            .await;

        let diff_text = diff_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let status_text = status_out
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let git_branch = git_current_branch(&directory).await;

        json!({
            "ok": true,
            "action": "diff",
            "working_dir": directory.display().to_string(),
            "diff": diff_text,
            "status": status_text,
            "git_branch": git_branch,
        })
    }

    async fn list_files(&self, custom_dir: Option<&str>, selected: Option<&str>) -> Value {
        let directory = self.resolve_workspace_dir(custom_dir, selected);

        if !directory.is_dir() {
            return json!({
                "ok": false,
                "action": "files",
                "error": format!("Directory not found: {}", directory.display()),
                "files": []
            });
        }

        let dir_clone = directory.clone();
        let files = tokio::task::spawn_blocking(move || {
            let mut list = Vec::new();
            let walker = walkdir::WalkDir::new(&dir_clone)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| {
                    let file_name = entry.file_name().to_string_lossy();
                    !(entry.file_type().is_dir()
                        && (file_name == ".git"
                            || file_name == "target"
                            || file_name == "node_modules"
                            || file_name == ".next"
                            || file_name == "dist"
                            || file_name == ".turbo"
                            || file_name == ".cache"
                            || file_name == ".system_generated"))
                });

            for entry in walker.flatten() {
                if entry.file_type().is_file()
                    && let Ok(rel) = entry.path().strip_prefix(&dir_clone)
                {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    list.push(rel_str);
                    if list.len() >= 1000 {
                        break;
                    }
                }
            }
            list
        })
        .await
        .unwrap_or_default();

        json!({
            "ok": true,
            "action": "files",
            "working_dir": directory.display().to_string(),
            "files": files,
        })
    }

    async fn read_file(&self, path: Option<&str>, selected: Option<&str>) -> Value {
        let requested = match path.map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => p,
            None => {
                return json!({
                    "ok": false,
                    "action": "read_file",
                    "error": "File path is required"
                });
            }
        };

        let directory = self.resolve_workspace_dir(None, selected);
        let canonical_target = match resolve_existing_in_workspace(&directory, requested) {
            Ok(path) => path,
            Err(err) => {
                return json!({
                    "ok": false,
                    "action": "read_file",
                    "path": requested,
                    "error": format!("{err:#}")
                });
            }
        };

        if !canonical_target.is_file() {
            return json!({
                "ok": false,
                "action": "read_file",
                "path": requested,
                "error": format!("File not found: {}", canonical_target.display())
            });
        }

        match tokio::fs::File::open(&canonical_target).await {
            Ok(mut file) => {
                use tokio::io::AsyncReadExt;
                let mut buffer = vec![0u8; MAX_FILE_READ_BYTES];
                match file.read(&mut buffer).await {
                    Ok(bytes_read) => {
                        buffer.truncate(bytes_read);
                        let content = String::from_utf8_lossy(&buffer).to_string();
                        json!({
                            "ok": true,
                            "action": "read_file",
                            "path": requested,
                            "content": content
                        })
                    }
                    Err(err) => json!({
                        "ok": false,
                        "action": "read_file",
                        "path": requested,
                        "error": format!("Failed to read file: {err:#}")
                    }),
                }
            }
            Err(err) => json!({
                "ok": false,
                "action": "read_file",
                "path": requested,
                "error": format!("Cannot open file: {err:#}")
            }),
        }
    }

    async fn write_file(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let path = request.path.clone().unwrap_or_default();
        let content = request.content.clone().unwrap_or_default();
        match tokio::task::spawn_blocking(move || {
            write_file_in_workspace(&workspace, &path, &content)
        })
        .await
        {
            Ok(Ok(val)) => val,
            Ok(Err(err)) => {
                json!({"ok": false, "action": "write_file", "error": format!("{err:#}")})
            }
            Err(err) => json!({"ok": false, "action": "write_file", "error": format!("{err}")}),
        }
    }

    async fn create_file(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let path = request.path.clone().unwrap_or_default();
        let content = request.content.clone().unwrap_or_default();
        match tokio::task::spawn_blocking(move || {
            create_file_in_workspace(&workspace, &path, &content)
        })
        .await
        {
            Ok(Ok(val)) => val,
            Ok(Err(err)) => {
                json!({"ok": false, "action": "create_file", "error": format!("{err:#}")})
            }
            Err(err) => json!({"ok": false, "action": "create_file", "error": format!("{err}")}),
        }
    }

    async fn create_dir(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let path = request.path.clone().unwrap_or_default();
        match tokio::task::spawn_blocking(move || create_dir_in_workspace(&workspace, &path)).await
        {
            Ok(Ok(val)) => val,
            Ok(Err(err)) => {
                json!({"ok": false, "action": "create_dir", "error": format!("{err:#}")})
            }
            Err(err) => json!({"ok": false, "action": "create_dir", "error": format!("{err}")}),
        }
    }

    async fn delete_path(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let path = request.path.clone().unwrap_or_default();
        match tokio::task::spawn_blocking(move || delete_path_in_workspace(&workspace, &path)).await
        {
            Ok(Ok(val)) => val,
            Ok(Err(err)) => {
                json!({"ok": false, "action": "delete_path", "error": format!("{err:#}")})
            }
            Err(err) => json!({"ok": false, "action": "delete_path", "error": format!("{err}")}),
        }
    }

    async fn rename_path(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let old_path = request.path.clone().unwrap_or_default();
        let new_path = request.new_path.clone().unwrap_or_default();
        match tokio::task::spawn_blocking(move || {
            rename_path_in_workspace(&workspace, &old_path, &new_path)
        })
        .await
        {
            Ok(Ok(val)) => val,
            Ok(Err(err)) => {
                json!({"ok": false, "action": "rename_path", "error": format!("{err:#}")})
            }
            Err(err) => json!({"ok": false, "action": "rename_path", "error": format!("{err}")}),
        }
    }

    async fn git_action(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let op = request
            .git_op
            .as_deref()
            .or(request.op.as_deref())
            .unwrap_or("");
        let msg = request
            .message
            .as_deref()
            .or(request.title.as_deref())
            .or(request.prompt.as_deref());
        match git_action_in_workspace(&workspace, op, request.path.as_deref(), msg).await {
            Ok(val) => val,
            Err(err) => json!({
                "ok": false,
                "action": "git_action",
                "git_op": op,
                "error": format!("{err:#}"),
            }),
        }
    }

    async fn shell_exec(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let command = request
            .command
            .as_deref()
            .or(request.prompt.as_deref())
            .unwrap_or("");
        match shell_exec_in_workspace(
            &workspace,
            command,
            request.timeout_ms,
            request.max_output_bytes,
        )
        .await
        {
            Ok(val) => val,
            Err(err) => json!({
                "ok": false,
                "action": "shell_exec",
                "error": format!("{err:#}"),
            }),
        }
    }

    async fn doctor(&self) -> Value {
        tokio::task::spawn_blocking(|| {
            let cfg = crate::config::Config::load();
            let auth = crate::alphacode_base::auth::AuthStatus::check();
            let configured_providers = crate::provider_catalog::cli_login_providers()
                .into_iter()
                .filter(|p| {
                    p.target != crate::provider_catalog::LoginProviderTarget::AutoImport
                        && auth.assessment_for_provider(*p).is_configured()
                })
                .count();
            let live = crate::storage::session_presence();
            let self_pid = std::process::id();
            let other_sessions: Vec<u32> = live
                .iter()
                .filter(|p| p.pid != self_pid)
                .map(|p| p.pid)
                .collect();
            let config_path = crate::config::Config::path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unavailable>".to_string());
            let config_valid = crate::config::Config::load_strict().is_ok();

            let checks = vec![
                json!({
                    "label": "version",
                    "status": "ok",
                    "detail": format!("alphacode {}", crate::alphacode_build_meta::version()),
                }),
                json!({
                    "label": "binary",
                    "status": "info",
                    "detail": std::env::current_exe()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "<unknown>".to_string()),
                }),
                json!({
                    "label": "config",
                    "status": if config_valid { "ok" } else { "warn" },
                    "detail": if config_valid {
                        format!("Valid config at {config_path}")
                    } else {
                        format!("Malformed config at {config_path}")
                    },
                }),
                json!({
                    "label": "providers",
                    "status": if configured_providers > 0 { "ok" } else { "warn" },
                    "detail": format!("{configured_providers} configured provider(s)"),
                }),
                json!({
                    "label": "default_model",
                    "status": "info",
                    "detail": cfg.provider.default_model.as_deref().unwrap_or("<automatic>"),
                }),
                json!({
                    "label": "build_type",
                    "status": "ok",
                    "detail": if crate::update::is_release_build() {
                        "release build"
                    } else {
                        "dev/source build"
                    },
                }),
                json!({
                    "label": "other_sessions",
                    "status": if other_sessions.is_empty() { "ok" } else { "info" },
                    "detail": if other_sessions.is_empty() {
                        "none".to_string()
                    } else {
                        format!("{} active session(s): {:?}", other_sessions.len(), other_sessions)
                    },
                }),
            ];

            let recon_reports = crate::alphacode_app_core::bugbounty_doctor::probe();
            let recon_tools: Vec<Value> = recon_reports
                .into_iter()
                .map(|t| {
                    let (present, path) = match t.status {
                        crate::alphacode_app_core::bugbounty_doctor::ToolStatus::Present {
                            path,
                        } => (true, Some(path)),
                        crate::alphacode_app_core::bugbounty_doctor::ToolStatus::Missing => {
                            (false, None)
                        }
                    };
                    json!({
                        "binary": t.binary,
                        "purpose": t.purpose,
                        "present": present,
                        "path": path,
                        "install_hint": t.install_hint,
                    })
                })
                .collect();

            json!({
                "ok": true,
                "action": "doctor",
                "version": crate::alphacode_build_meta::version(),
                "checks": checks,
                "recon_tools": recon_tools,
            })
        })
        .await
        .unwrap_or_else(|err| json!({"ok": false, "action": "doctor", "error": format!("{err}")}))
    }

    async fn usage(&self) -> Value {
        let provider_reports = crate::usage::fetch_all_provider_usage().await;
        let providers: Vec<Value> = provider_reports
            .into_iter()
            .map(|report| {
                let limits: Vec<Value> = report
                    .limits
                    .into_iter()
                    .map(|l| {
                        json!({
                            "name": l.name,
                            "usage_percent": l.usage_percent,
                            "resets_at": l.resets_at,
                        })
                    })
                    .collect();
                let extra_info: Vec<Value> = report
                    .extra_info
                    .into_iter()
                    .map(|(k, v)| json!({ "label": k, "value": v }))
                    .collect();
                json!({
                    "provider_name": report.provider_name,
                    "limits": limits,
                    "extra_info": extra_info,
                    "hard_limit_reached": report.hard_limit_reached,
                    "error": report.error,
                    "last_used_unix_secs": report.last_used_unix_secs,
                })
            })
            .collect();

        let ledger = tokio::task::spawn_blocking(|| {
            crate::usage_ledger::flush();
            let (lifetime_tokens, lifetime_requests) = crate::usage_ledger::totals();
            let since = crate::usage_ledger::first_recorded_day();
            let model_rows: Vec<Value> = crate::usage_ledger::snapshot()
                .into_iter()
                .map(|(model, usage)| {
                    let cost = crate::usage_ledger::estimate_cost_usd(&model, &usage);
                    let display_name = crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(&model)
                        .map(str::to_string)
                        .unwrap_or(model);
                    json!({
                        "model": display_name,
                        "requests": usage.requests,
                        "input_tokens": usage.input_tokens,
                        "output_tokens": usage.output_tokens,
                        "cache_read_tokens": usage.cache_read_tokens,
                        "cache_write_tokens": usage.cache_write_tokens,
                        "total_tokens": usage.total_tokens(),
                        "cost_usd": cost,
                    })
                })
                .collect();
            let daily_rows: Vec<Value> = crate::usage_ledger::daily_usage()
                .into_iter()
                .take(60)
                .map(|day| {
                    json!({
                        "day": day.day,
                        "requests": day.requests,
                        "input_tokens": day.input_tokens,
                        "output_tokens": day.output_tokens,
                        "cache_read_tokens": day.cache_read_tokens,
                        "cache_write_tokens": day.cache_write_tokens,
                        "total_tokens": day.total_tokens(),
                        "cost_usd": day.cost_usd,
                    })
                })
                .collect();
            let lifetime_cost_usd: f64 = daily_rows
                .iter()
                .filter_map(|r| r["cost_usd"].as_f64())
                .sum();
            let any_cost = daily_rows.iter().any(|r| r["cost_usd"].is_f64());
            json!({
                "since": since,
                "total_tokens": lifetime_tokens,
                "total_requests": lifetime_requests,
                "estimated_cost_usd": any_cost.then_some(lifetime_cost_usd),
                "models": model_rows,
                "daily": daily_rows,
            })
        })
        .await
        .unwrap_or_else(|_| json!({}));

        let owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
        let session_cost = compute_session_cost_usd(
            &self.model,
            owned.session_input_tokens,
            owned.session_output_tokens,
            owned.session_cache_read_tokens,
            owned.session_cache_creation_tokens,
        );
        let session = json!({
            "session_id": owned.session_id,
            "model": self.model,
            "provider": self.provider,
            "input_tokens": owned.session_input_tokens,
            "output_tokens": owned.session_output_tokens,
            "cache_read_tokens": owned.session_cache_read_tokens,
            "cache_creation_tokens": owned.session_cache_creation_tokens,
            "total_tokens": owned.session_input_tokens
                .saturating_add(owned.session_output_tokens)
                .saturating_add(owned.session_cache_read_tokens)
                .saturating_add(owned.session_cache_creation_tokens),
            "estimated_cost_usd": session_cost,
            "tokens_per_sec": owned.tokens_per_sec,
            "context_size": owned.last_context_tokens,
            "context_max": resolve_context_max(&self.model, &self.provider),
        });
        drop(owned);

        json!({
            "ok": true,
            "action": "usage",
            "providers": providers,
            "lifetime": ledger,
            "session": session,
        })
    }

    async fn memory(&self, request: &BridgeRequest) -> Value {
        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let op = request
            .op
            .as_deref()
            .unwrap_or("list")
            .trim()
            .to_ascii_lowercase();
        let scope_str = request
            .scope
            .as_deref()
            .unwrap_or("project")
            .trim()
            .to_ascii_lowercase();
        let id = request.id.clone();
        let content = request
            .content
            .clone()
            .or_else(|| request.message.clone())
            .or_else(|| request.prompt.clone());
        let category = request.category.clone();
        let tags = request.tags.clone().unwrap_or_default();

        tokio::task::spawn_blocking(move || {
            let manager = crate::alphacode_base::memory::MemoryManager::new()
                .with_project_dir(&workspace)
                .with_skills(false);

            match op.as_str() {
                "list" => {
                    let scope = match scope_str.as_str() {
                        "global" => crate::alphacode_base::memory::MemoryScope::Global,
                        "all" => crate::alphacode_base::memory::MemoryScope::All,
                        _ => crate::alphacode_base::memory::MemoryScope::Project,
                    };
                    match manager.list_all_scoped(scope) {
                        Ok(entries) => {
                            let rows: Vec<Value> = entries
                                .into_iter()
                                .map(|entry| {
                                    json!({
                                        "id": entry.id,
                                        "category": format!("{:?}", entry.category).to_ascii_lowercase(),
                                        "content": entry.content,
                                        "tags": entry.tags,
                                        "created_at": entry.created_at.to_rfc3339(),
                                        "updated_at": entry.updated_at.to_rfc3339(),
                                        "strength": entry.strength,
                                        "confidence": entry.confidence,
                                        "active": entry.active,
                                    })
                                })
                                .collect();
                            json!({
                                "ok": true,
                                "action": "memory",
                                "op": "list",
                                "scope": scope_str,
                                "working_dir": workspace.display().to_string(),
                                "entries": rows,
                            })
                        }
                        Err(err) => json!({"ok": false, "action": "memory", "error": format!("{err:#}")}),
                    }
                }
                "save" => {
                    let text = content.unwrap_or_default();
                    let text = text.trim();
                    if text.is_empty() {
                        return json!({"ok": false, "action": "memory", "error": "Memory content is required"});
                    }
                    let cat: crate::alphacode_base::memory::MemoryCategory = category
                        .as_deref()
                        .unwrap_or("fact")
                        .parse()
                        .unwrap_or(crate::alphacode_base::memory::MemoryCategory::Fact);
                    let mut entry = crate::alphacode_base::memory::MemoryEntry::new(cat, text);
                    entry.tags = tags;
                    entry.refresh_search_text();
                    entry.source = Some("gui".to_string());
                    let saved_id = if scope_str == "global" {
                        manager.remember_global(entry)
                    } else {
                        manager.remember_project(entry)
                    };
                    match saved_id {
                        Ok(id) => json!({
                            "ok": true,
                            "action": "memory",
                            "op": "save",
                            "scope": scope_str,
                            "id": id,
                        }),
                        Err(err) => json!({"ok": false, "action": "memory", "error": format!("{err:#}")}),
                    }
                }
                "delete" => {
                    let target_id = match id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                        Some(id) => id,
                        None => {
                            return json!({"ok": false, "action": "memory", "error": "Memory id is required"});
                        }
                    };
                    match manager.forget(target_id) {
                        Ok(deleted) => json!({
                            "ok": true,
                            "action": "memory",
                            "op": "delete",
                            "id": target_id,
                            "deleted": deleted,
                        }),
                        Err(err) => json!({"ok": false, "action": "memory", "error": format!("{err:#}")}),
                    }
                }
                other => json!({
                    "ok": false,
                    "action": "memory",
                    "error": format!("Unsupported memory op `{other}`; expected list, save, or delete"),
                }),
            }
        })
        .await
        .unwrap_or_else(|err| json!({"ok": false, "action": "memory", "error": format!("{err}")}))
    }

    async fn config_get(&self) -> Value {
        let cfg = crate::config::Config::load();
        let shaping = match crate::alphacode_render_core::get_shaping_mode() {
            crate::alphacode_render_core::ArabicShapingMode::Auto => "auto",
            crate::alphacode_render_core::ArabicShapingMode::StandardUnicode => "standard",
            crate::alphacode_render_core::ArabicShapingMode::PresentationForms => "presentation",
        };
        let reasoning_display = match cfg.display.reasoning_display() {
            crate::alphacode_config_types::ReasoningDisplayMode::Off => "off",
            crate::alphacode_config_types::ReasoningDisplayMode::Full => "full",
            crate::alphacode_config_types::ReasoningDisplayMode::Current => "current",
        };
        let markdown_spacing = match cfg.display.markdown_spacing {
            crate::alphacode_config_types::MarkdownSpacingMode::Compact => "compact",
            crate::alphacode_config_types::MarkdownSpacingMode::Document => "document",
        };
        let diff_mode = match cfg.display.diff_mode {
            crate::alphacode_config_types::DiffDisplayMode::Off => "off",
            crate::alphacode_config_types::DiffDisplayMode::Inline => "inline",
            crate::alphacode_config_types::DiffDisplayMode::FullInline => "full-inline",
            crate::alphacode_config_types::DiffDisplayMode::Pinned => "pinned",
            crate::alphacode_config_types::DiffDisplayMode::File => "file",
        };
        let fast_mode = cfg.provider.openai_service_tier.as_deref() == Some("priority");
        let transport = cfg
            .provider
            .openai_transport
            .as_deref()
            .unwrap_or("auto")
            .to_string();
        let model_display = cfg.provider.default_model.clone().map(|m| {
            hidden_pool_models_to_display_names(vec![m])
                .into_iter()
                .next()
                .unwrap_or_default()
        });

        json!({
            "ok": true,
            "action": "config_get",
            "config_path": crate::config::Config::path().map(|p| p.display().to_string()),
            "display": {
                "alignment": if cfg.display.centered { "centered" } else { "left" },
                "centered": cfg.display.centered,
                "emoji": cfg.display.emoji,
                "markdown_spacing": markdown_spacing,
                "diff_mode": diff_mode,
                "reasoning_display": reasoning_display,
                "compact_notifications": cfg.display.compact_notifications,
                "tool_call_details": cfg.display.tool_call_details,
                "pin_todos": cfg.display.pin_todos,
                "pin_images": cfg.display.pin_images,
                "show_alphagrep_output": cfg.display.show_alphagrep_output,
                "theme": cfg.display.theme,
                "preset": cfg.display.preset,
                "arabic_shaping_mode": shaping,
                "native_shaping_supported": crate::alphacode_render_core::terminal_supports_native_shaping(),
            },
            "provider": {
                "default_model": model_display,
                "default_provider": cfg.provider.default_provider,
                "fast_mode": fast_mode,
                "openai_service_tier": cfg.provider.openai_service_tier,
                "transport": transport,
                "openai_reasoning_effort": cfg.provider.openai_reasoning_effort,
                "anthropic_reasoning_effort": cfg.provider.anthropic_reasoning_effort,
                "copilot_premium": cfg.provider.copilot_premium,
                "same_provider_account_failover": cfg.provider.same_provider_account_failover,
            },
            "agents": {
                "swarm_model": cfg.agents.swarm_model,
                "memory_model": cfg.agents.memory_model,
                "swarm_spawn_mode": cfg.agents.swarm_spawn_mode.as_str(),
                "swarm_strip_layout": cfg.agents.swarm_strip_layout.as_str(),
                "memory_sidecar_enabled": cfg.agents.memory_sidecar_enabled,
                "swarm_max_concurrent_agents": cfg.agents.swarm_max_concurrent_agents,
                "plan_max_nodes": cfg.agents.plan_max_nodes,
                "autoreview_enabled": cfg.autoreview.enabled,
                "autoreview_model": cfg.autoreview.model,
                "autojudge_enabled": cfg.autojudge.enabled,
                "autojudge_model": cfg.autojudge.model,
            },
        })
    }

    async fn config_set(&mut self, request: &BridgeRequest) -> Value {
        let mut pairs: Vec<(String, Value)> = Vec::new();
        if let Some(key) = request
            .key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            let val = request.value.clone().unwrap_or(Value::Null);
            pairs.push((key.to_string(), val));
        }
        if let Some(obj) = request.updates.as_ref().and_then(Value::as_object) {
            for (k, v) in obj {
                pairs.push((k.clone(), v.clone()));
            }
        }
        if pairs.is_empty() {
            return json!({
                "ok": false,
                "action": "config_set",
                "error": "Provide `key` and `value`, or an `updates` object",
            });
        }

        let mut runtime_tier: Option<String> = None;
        let mut runtime_transport: Option<String> = None;
        let mut runtime_model: Option<String> = None;
        let mut runtime_effort: Option<String> = None;

        let apply_result = crate::config::Config::mutate_config(|cfg| -> Result<()> {
            for (key, val) in &pairs {
                match key.as_str() {
                    "alignment" | "display.alignment" => {
                        let s = val.as_str().unwrap_or("").trim().to_ascii_lowercase();
                        cfg.display.centered = matches!(s.as_str(), "centered" | "center" | "true");
                    }
                    "centered" | "display.centered" => {
                        cfg.display.centered = val.as_bool().unwrap_or(false);
                    }
                    "emoji" | "display.emoji" => {
                        cfg.display.emoji = val.as_bool().unwrap_or(true);
                    }
                    "output_style" | "markdown_spacing" | "display.markdown_spacing" => {
                        let s = val.as_str().unwrap_or("").trim().to_ascii_lowercase();
                        cfg.display.markdown_spacing = match s.as_str() {
                            "document" | "doc" => {
                                crate::alphacode_config_types::MarkdownSpacingMode::Document
                            }
                            _ => crate::alphacode_config_types::MarkdownSpacingMode::Compact,
                        };
                    }
                    "diff_mode" | "display.diff_mode" => {
                        let s = val.as_str().unwrap_or("").trim().to_ascii_lowercase();
                        cfg.display.diff_mode = match s.as_str() {
                            "off" => crate::alphacode_config_types::DiffDisplayMode::Off,
                            "full-inline" | "full_inline" | "full" => {
                                crate::alphacode_config_types::DiffDisplayMode::FullInline
                            }
                            "pinned" => crate::alphacode_config_types::DiffDisplayMode::Pinned,
                            "file" => crate::alphacode_config_types::DiffDisplayMode::File,
                            _ => crate::alphacode_config_types::DiffDisplayMode::Inline,
                        };
                    }
                    "reasoning_display" | "display.reasoning_display" => {
                        if let Some(mode) = val
                            .as_str()
                            .and_then(crate::alphacode_config_types::ReasoningDisplayMode::parse)
                        {
                            cfg.display.set_reasoning_display(mode);
                        }
                    }
                    "compact_notifications" | "display.compact_notifications" => {
                        cfg.display.compact_notifications = val.as_bool().unwrap_or(false);
                    }
                    "tool_call_details" | "display.tool_call_details" => {
                        cfg.display.tool_call_details = val.as_bool().unwrap_or(false);
                    }
                    "pin_todos" | "display.pin_todos" => {
                        cfg.display.pin_todos = val.as_bool().unwrap_or(false);
                    }
                    "pin_images" | "display.pin_images" => {
                        cfg.display.pin_images = val.as_bool().unwrap_or(true);
                    }
                    "show_alphagrep_output" | "display.show_alphagrep_output" => {
                        cfg.display.show_alphagrep_output = val.as_bool().unwrap_or(false);
                    }
                    "theme" | "display.theme" => {
                        cfg.display.theme = val.as_str().unwrap_or("").trim().to_string();
                    }
                    "preset" | "display.preset" => {
                        cfg.display.preset = val.as_str().unwrap_or("").trim().to_string();
                    }
                    "arabic_shaping_mode" | "display.arabic_shaping_mode" => {
                        let s = val.as_str().unwrap_or("auto").trim().to_ascii_lowercase();
                        let mode = match s.as_str() {
                            "standard" | "unicode" | "native" => {
                                crate::alphacode_render_core::ArabicShapingMode::StandardUnicode
                            }
                            "presentation" | "forms" | "legacy" => {
                                crate::alphacode_render_core::ArabicShapingMode::PresentationForms
                            }
                            _ => crate::alphacode_render_core::ArabicShapingMode::Auto,
                        };
                        crate::alphacode_render_core::set_shaping_mode_override(Some(mode));
                    }
                    "fast_mode" | "provider.fast_mode" => {
                        let enabled = val.as_bool().unwrap_or(false);
                        let tier = if enabled { "priority" } else { "off" }.to_string();
                        cfg.provider.openai_service_tier = Some(tier.clone());
                        runtime_tier = Some(tier);
                    }
                    "openai_service_tier" | "provider.openai_service_tier" => {
                        let tier = val.as_str().unwrap_or("off").trim().to_ascii_lowercase();
                        cfg.provider.openai_service_tier = Some(tier.clone());
                        runtime_tier = Some(tier);
                    }
                    "transport"
                    | "openai_transport"
                    | "provider.transport"
                    | "provider.openai_transport" => {
                        let transport = val.as_str().unwrap_or("auto").trim().to_ascii_lowercase();
                        cfg.provider.openai_transport = if transport == "auto" {
                            None
                        } else {
                            Some(transport.clone())
                        };
                        runtime_transport = Some(transport);
                    }
                    "model" | "default_model" | "provider.default_model" => {
                        let raw = val.as_str().unwrap_or("").trim();
                        if raw.is_empty() {
                            cfg.provider.default_model = None;
                        } else {
                            let resolved = crate::alphacode_app_core::agent::free_pool_rotation::resolve_free_pool_model_selection(raw).unwrap_or(raw);
                            cfg.provider.default_model = Some(resolved.to_string());
                            runtime_model = Some(raw.to_string());
                        }
                    }
                    "default_provider" | "provider.default_provider" => {
                        let raw = val.as_str().unwrap_or("").trim();
                        cfg.provider.default_provider = (!raw.is_empty()).then(|| raw.to_string());
                    }
                    "effort" | "openai_reasoning_effort" | "provider.openai_reasoning_effort" => {
                        let effort = val.as_str().unwrap_or("").trim().to_ascii_lowercase();
                        cfg.provider.openai_reasoning_effort =
                            (!effort.is_empty()).then(|| effort.clone());
                        cfg.provider.anthropic_reasoning_effort =
                            (!effort.is_empty()).then(|| effort.clone());
                        if !effort.is_empty() {
                            runtime_effort = Some(effort);
                        }
                    }
                    "anthropic_reasoning_effort" | "provider.anthropic_reasoning_effort" => {
                        let effort = val.as_str().unwrap_or("").trim().to_ascii_lowercase();
                        cfg.provider.anthropic_reasoning_effort =
                            (!effort.is_empty()).then(|| effort.clone());
                    }
                    "swarm_model" | "agents.swarm_model" => {
                        let raw = val.as_str().unwrap_or("").trim();
                        cfg.agents.swarm_model =
                            (!raw.is_empty() && raw != "inherit").then(|| raw.to_string());
                    }
                    "memory_model" | "agents.memory_model" => {
                        let raw = val.as_str().unwrap_or("").trim();
                        cfg.agents.memory_model = (!raw.is_empty()).then(|| raw.to_string());
                    }
                    "swarm_spawn_mode" | "agents.swarm_spawn_mode" => {
                        if let Some(mode) = val
                            .as_str()
                            .and_then(crate::alphacode_config_types::SwarmSpawnMode::parse)
                        {
                            cfg.agents.swarm_spawn_mode = mode;
                        }
                    }
                    "swarm_strip_layout" | "agents.swarm_strip_layout" => {
                        if let Some(layout) = val
                            .as_str()
                            .and_then(crate::alphacode_config_types::SwarmStripLayout::parse)
                        {
                            cfg.agents.swarm_strip_layout = layout;
                        }
                    }
                    "memory_sidecar_enabled" | "agents.memory_sidecar_enabled" => {
                        cfg.agents.memory_sidecar_enabled = val.as_bool().unwrap_or(true);
                    }
                    "autoreview_enabled" | "autoreview.enabled" => {
                        cfg.autoreview.enabled = val.as_bool().unwrap_or(false);
                    }
                    "autoreview_model" | "autoreview.model" => {
                        let raw = val.as_str().unwrap_or("").trim();
                        cfg.autoreview.model = (!raw.is_empty()).then(|| raw.to_string());
                    }
                    "autojudge_enabled" | "autojudge.enabled" => {
                        cfg.autojudge.enabled = val.as_bool().unwrap_or(false);
                    }
                    "autojudge_model" | "autojudge.model" => {
                        let raw = val.as_str().unwrap_or("").trim();
                        cfg.autojudge.model = (!raw.is_empty()).then(|| raw.to_string());
                    }
                    unknown => bail!("Unknown config key `{unknown}`"),
                }
            }
            Ok(())
        });

        match apply_result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                return json!({"ok": false, "action": "config_set", "error": format!("{err:#}")});
            }
            Err(err) => {
                return json!({"ok": false, "action": "config_set", "error": format!("{err:#}")});
            }
        }

        if let Some(model) = runtime_model {
            let _ = self
                .submit_model(request.session_id.as_deref(), &model)
                .await;
        }
        if let Some(effort) = runtime_effort {
            let _ = self
                .submit_effort(request.session_id.as_deref(), &effort)
                .await;
        }
        if let Some(tier) = runtime_tier
            && let Some(tx) = self.control_tx.as_ref().filter(|tx| !tx.is_closed())
        {
            let (reply_tx, reply_rx) = oneshot::channel();
            if tx
                .send(OwnedControl::SetServiceTier {
                    tier,
                    reply: reply_tx,
                })
                .await
                .is_ok()
            {
                let _ = tokio::time::timeout(Duration::from_secs(2), reply_rx).await;
            }
        }
        if let Some(transport) = runtime_transport
            && let Some(tx) = self.control_tx.as_ref().filter(|tx| !tx.is_closed())
        {
            let (reply_tx, reply_rx) = oneshot::channel();
            if tx
                .send(OwnedControl::SetTransport {
                    transport,
                    reply: reply_tx,
                })
                .await
                .is_ok()
            {
                let _ = tokio::time::timeout(Duration::from_secs(2), reply_rx).await;
            }
        }

        let mut updated = self.config_get().await;
        updated["action"] = json!("config_set");
        updated
    }

    async fn copy_session(&self, selected: Option<&str>) -> Value {
        let target_id = match selected.filter(|id| *id != OWN_CHAT) {
            Some(id) => Some(id.to_string()),
            None => self
                .owned
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .session_id
                .clone(),
        };

        if let Some(ref sid) = target_id {
            // 1) Try live export from the owned socket when matching the owned session.
            let is_owned_sid = self
                .owned
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .session_id
                .as_deref()
                == Some(sid.as_str());
            if is_owned_sid && let Some(tx) = self.control_tx.as_ref().filter(|tx| !tx.is_closed())
            {
                let (reply_tx, reply_rx) = oneshot::channel();
                if tx
                    .send(OwnedControl::GetSessionTranscript {
                        session_id: sid.clone(),
                        reply: reply_tx,
                    })
                    .await
                    .is_ok()
                    && let Ok(Ok(Ok(Some(text)))) =
                        tokio::time::timeout(Duration::from_secs(4), reply_rx).await
                {
                    let lines = text.lines().count();
                    return json!({
                        "ok": true,
                        "action": "copy_session",
                        "session_id": sid,
                        "markdown": text,
                        "lines": lines,
                    });
                }
            }

            // 2) Try on-disk Session::load().clipboard_transcript().
            let sid_clone = sid.clone();
            if let Ok(Ok(Some(text))) = tokio::task::spawn_blocking(move || {
                let session = crate::session::Session::load(&sid_clone)?;
                Ok::<_, anyhow::Error>(session.clipboard_transcript())
            })
            .await
            {
                let lines = text.lines().count();
                return json!({
                    "ok": true,
                    "action": "copy_session",
                    "session_id": sid,
                    "markdown": text,
                    "lines": lines,
                });
            }
        }

        // 3) Fallback to current in-memory owned transcript + reply.
        let owned = self.owned.lock().unwrap_or_else(|e| e.into_inner());
        let mut md = String::new();
        let title = owned.title.as_deref().unwrap_or("AlphaCode Session");
        if !owned.transcript.trim().is_empty() || !owned.reply.trim().is_empty() {
            md.push_str(&format!("# {title}\n\n"));
            if let Some(sid) = &owned.session_id {
                md.push_str(&format!("Session: {sid}\n\n"));
            }
            if !owned.transcript.is_empty() {
                md.push_str(&owned.transcript);
            }
            if !owned.reply.is_empty() {
                if !md.ends_with('\n') {
                    md.push('\n');
                }
                md.push_str(&owned.reply);
            }
        }
        let lines = md.lines().count();
        json!({
            "ok": true,
            "action": "copy_session",
            "session_id": owned.session_id,
            "markdown": md,
            "lines": lines,
        })
    }

    async fn run_tool(&self, request: &BridgeRequest, is_recon: bool) -> Value {
        let action_name = if is_recon { "run_recon" } else { "run_tool" };
        let tool_name = match request
            .tool
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(name) => name.to_string(),
            None => {
                return json!({
                    "ok": false,
                    "action": action_name,
                    "error": "Tool name is required",
                });
            }
        };

        let workspace = self.resolve_workspace_dir(
            request.working_dir.as_deref(),
            request.session_id.as_deref(),
        );
        let session_id = self
            .owned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .session_id
            .clone()
            .unwrap_or_else(|| "gui-direct".to_string());

        let mut args = request.args.clone().unwrap_or_else(|| json!({}));
        if !args.is_object() {
            args = json!({});
        }

        let registry = gui_tool_registry().await;
        if is_recon
            && let Some(target) = request
                .target
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
        {
            let defs = registry.definitions(None).await;
            if let Some(def) = defs.iter().find(|d| d.name == tool_name)
                && let Some(props) = def
                    .input_schema
                    .get("properties")
                    .and_then(Value::as_object)
                && let Some(obj) = args.as_object_mut()
            {
                if props.contains_key("domain") && !obj.contains_key("domain") {
                    obj.insert("domain".into(), json!(target));
                } else if props.contains_key("targets") && !obj.contains_key("targets") {
                    obj.insert("targets".into(), json!([target]));
                } else if props.contains_key("url") && !obj.contains_key("url") {
                    obj.insert("url".into(), json!(target));
                } else if props.contains_key("target") && !obj.contains_key("target") {
                    obj.insert("target".into(), json!(target));
                } else if props.contains_key("host") && !obj.contains_key("host") {
                    obj.insert("host".into(), json!(target));
                } else if props.contains_key("hosts") && !obj.contains_key("hosts") {
                    obj.insert("hosts".into(), json!([target]));
                }
            }
        }

        let call_id = format!(
            "gui-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let ctx = crate::alphacode_tool_core::ToolContext {
            session_id,
            message_id: call_id.clone(),
            tool_call_id: call_id,
            working_dir: Some(workspace.clone()),
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: crate::alphacode_tool_core::ToolExecutionMode::Direct,
        };

        let started = Instant::now();
        match registry.execute(&tool_name, args, ctx).await {
            Ok(out) => json!({
                "ok": true,
                "action": action_name,
                "tool": tool_name,
                "title": out.title,
                "output": display_text(&out.output, 32_000),
                "metadata": out.metadata,
                "duration_ms": started.elapsed().as_millis() as u64,
                "working_dir": workspace.display().to_string(),
            }),
            Err(err) => json!({
                "ok": false,
                "action": action_name,
                "tool": tool_name,
                "error": format!("{err:#}"),
                "duration_ms": started.elapsed().as_millis() as u64,
            }),
        }
    }

    async fn schedule_action(&self, request: &BridgeRequest) -> Value {
        let op = request
            .op
            .as_deref()
            .unwrap_or("list")
            .trim()
            .to_ascii_lowercase();
        let target_id = request.id.clone();
        tokio::task::spawn_blocking(move || {
            let mut manager = match crate::alphacode_app_core::ambient::AmbientManager::new() {
                Ok(m) => m,
                Err(err) => {
                    return json!({
                        "ok": false,
                        "action": "schedule",
                        "error": format!("{err:#}"),
                    });
                }
            };
            match op.as_str() {
                "cancel" | "delete" => {
                    let Some(id) = target_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                        return json!({"ok": false, "action": "schedule", "error": "Schedule id is required"});
                    };
                    match manager.cancel_schedule(id) {
                        Ok(removed) => json!({
                            "ok": true,
                            "action": "schedule",
                            "op": "cancel",
                            "id": id,
                            "removed": removed.is_some(),
                        }),
                        Err(err) => json!({"ok": false, "action": "schedule", "error": format!("{err:#}")}),
                    }
                }
                "pause" | "resume" => {
                    let Some(id) = target_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                        return json!({"ok": false, "action": "schedule", "error": "Schedule id is required"});
                    };
                    let paused = op == "pause";
                    match manager.pause_schedule(id, paused) {
                        Ok(updated) => json!({
                            "ok": true,
                            "action": "schedule",
                            "op": op,
                            "id": id,
                            "updated": updated.is_some(),
                        }),
                        Err(err) => json!({"ok": false, "action": "schedule", "error": format!("{err:#}")}),
                    }
                }
                _ => {
                    let items: Vec<Value> = manager
                        .queue()
                        .items()
                        .iter()
                        .map(|item| {
                            json!({
                                "id": item.id,
                                "scheduled_for": item.scheduled_for.to_rfc3339(),
                                "repeat_every_minutes": item.repeat_every_minutes.map(|v| v.get()),
                                "paused": item.paused,
                                "context": item.context,
                                "task_description": item.task_description,
                                "priority": format!("{:?}", item.priority).to_ascii_lowercase(),
                                "working_dir": item.working_dir,
                                "created_by_session": item.created_by_session,
                            })
                        })
                        .collect();
                    json!({
                        "ok": true,
                        "action": "schedule",
                        "op": "list",
                        "enabled": crate::alphacode_app_core::ambient::AmbientManager::is_enabled(),
                        "state": manager.state(),
                        "items": items,
                    })
                }
            }
        })
        .await
        .unwrap_or_else(|err| json!({"ok": false, "action": "schedule", "error": format!("{err}")}))
    }

    fn slash_commands_list(&self) -> Value {
        let mut seen = HashSet::new();
        let mut commands: Vec<Value> = Vec::new();
        for (name, help) in crate::alphacode_tui::tui::app::registered_command_entries() {
            if seen.insert(name.to_string()) {
                commands.push(json!({
                    "name": name,
                    "help": help,
                }));
            }
        }
        for (name, help) in [
            (
                "/doctor",
                "Run local diagnostics and recon tool availability checks",
            ),
            ("/schedule", "List and manage scheduled ambient tasks"),
        ] {
            if seen.insert(name.to_string()) {
                commands.push(json!({
                    "name": name,
                    "help": help,
                }));
            }
        }
        json!({
            "ok": true,
            "action": "slash_commands",
            "commands": commands,
        })
    }

    async fn send_owned_control(
        &mut self,
        build: impl FnOnce(oneshot::Sender<Result<Value, String>>) -> OwnedControl,
    ) -> Result<Value> {
        self.ensure_owned_chat();
        let sender = self
            .control_tx
            .as_ref()
            .filter(|tx| !tx.is_closed())
            .context("AlphaCode GUI session control channel unavailable")?;
        let (tx, rx) = oneshot::channel();
        sender
            .send(build(tx))
            .await
            .context("AlphaCode GUI session closed before control command could be delivered")?;
        let result = tokio::time::timeout(Duration::from_secs(20), rx)
            .await
            .context("Session control command timed out")?
            .context("Session control channel closed")?;
        result.map_err(anyhow::Error::msg)
    }

    async fn execute_slash_command(&mut self, request: &BridgeRequest) -> Value {
        let raw = request
            .command
            .as_deref()
            .or(request.prompt.as_deref())
            .unwrap_or("")
            .trim();
        if raw.is_empty() {
            return json!({
                "ok": false,
                "action": "slash_command",
                "error": "Slash command is required",
            });
        }
        let normalized = if raw.starts_with('/') {
            raw.to_string()
        } else {
            format!("/{raw}")
        };
        let mut parts = normalized.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").to_ascii_lowercase();
        let rest = parts.next().unwrap_or("").trim();

        match cmd.as_str() {
            "/compact" => match self.send_owned_control(OwnedControl::Compact).await {
                Ok(res) => {
                    json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "session", "data": res})
                }
                Err(err) => {
                    json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                }
            },
            "/clear" | "/cls" => match self.send_owned_control(OwnedControl::Clear).await {
                Ok(res) => {
                    json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "session", "data": res})
                }
                Err(err) => {
                    json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                }
            },
            "/rewind" => {
                if rest.eq_ignore_ascii_case("undo") {
                    match self.send_owned_control(OwnedControl::RewindUndo).await {
                        Ok(res) => {
                            json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "session", "data": res})
                        }
                        Err(err) => {
                            json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                        }
                    }
                } else if let Some(idx) = request.message_index.or_else(|| {
                    (!rest.is_empty())
                        .then(|| rest.parse::<usize>().ok())
                        .flatten()
                }) {
                    match self
                        .send_owned_control(|reply| OwnedControl::Rewind {
                            message_index: idx,
                            reply,
                        })
                        .await
                    {
                        Ok(res) => {
                            json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "session", "data": res})
                        }
                        Err(err) => {
                            json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                        }
                    }
                } else {
                    // Bare `/rewind`: list rewindable messages from current session.
                    let sid = self
                        .owned
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .session_id
                        .clone();
                    let messages = if let Some(sid) = sid {
                        tokio::task::spawn_blocking(move || {
                            let session = crate::session::Session::load(&sid)?;
                            let msgs = crate::session::render_messages(&session);
                            Ok::<_, anyhow::Error>(
                                msgs.into_iter()
                                    .enumerate()
                                    .map(|(i, m)| json!({"index": i + 1, "role": m.role, "preview": display_text(&m.content, 200)}))
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "rewind_list", "data": {"messages": messages}})
                }
            }
            "/copy" => {
                let res = self.copy_session(request.session_id.as_deref()).await;
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "copy", "data": res})
            }
            "/context" | "/info" => {
                let snap = self.snapshot(request.session_id.as_deref()).await;
                let caps = self
                    .runtime_capabilities(request.session_id.as_deref())
                    .await;
                json!({
                    "ok": true,
                    "action": "slash_command",
                    "command": cmd,
                    "kind": "info",
                    "data": {
                        "session_id": snap["owned_session_id"],
                        "model": snap["model"],
                        "provider": snap["provider"],
                        "effort": snap["effort"],
                        "git_branch": snap["git_branch"],
                        "token_metrics": snap["token_metrics"],
                        "tools_count": caps["tools"].as_array().map(|a| a.len()).unwrap_or(0),
                        "skills_count": caps["skills"]["skills"].as_array().map(|a| a.len()).unwrap_or(0),
                        "mcp_servers": caps["mcp"]["servers"],
                    },
                })
            }
            "/usage" => {
                let res = self.usage().await;
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "usage", "data": res})
            }
            "/memory" => {
                let mut sub = BridgeRequest {
                    action: "memory".into(),
                    provider: None,
                    auth_method: None,
                    api_key: None,
                    api_base: None,
                    setup_value: None,
                    request_id: request.request_id.clone(),
                    session_id: request.session_id.clone(),
                    prompt: None,
                    model: None,
                    effort: None,
                    title: None,
                    working_dir: request.working_dir.clone(),
                    saved: None,
                    limit: None,
                    include_workflow: false,
                    path: None,
                    new_path: None,
                    content: None,
                    git_op: None,
                    message: None,
                    command: None,
                    timeout_ms: None,
                    max_output_bytes: None,
                    tool: None,
                    args: None,
                    target: None,
                    op: Some("list".into()),
                    id: None,
                    category: None,
                    tags: None,
                    scope: Some("project".into()),
                    key: None,
                    value: None,
                    updates: None,
                    label: None,
                    ..BridgeRequest::default()
                };
                if let Some(note) = rest.strip_prefix("save ") {
                    sub.op = Some("save".into());
                    sub.content = Some(note.trim().to_string());
                } else if let Some(del_id) = rest.strip_prefix("delete ") {
                    sub.op = Some("delete".into());
                    sub.id = Some(del_id.trim().to_string());
                }
                let res = self.memory(&sub).await;
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "memory", "data": res})
            }
            "/doctor" => {
                let res = self.doctor().await;
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "doctor", "data": res})
            }
            "/config" => {
                let res = if let Some(kv) = rest.strip_prefix("set ")
                    && let Some((k, v)) = kv.split_once(' ')
                {
                    let val: Value = serde_json::from_str(v.trim())
                        .unwrap_or_else(|_| Value::String(v.trim().to_string()));
                    let mut cfg_req = BridgeRequest {
                        action: "config_set".into(),
                        provider: None,
                        auth_method: None,
                        api_key: None,
                        api_base: None,
                        setup_value: None,
                        request_id: request.request_id.clone(),
                        session_id: request.session_id.clone(),
                        prompt: None,
                        model: None,
                        effort: None,
                        title: None,
                        working_dir: request.working_dir.clone(),
                        saved: None,
                        limit: None,
                        include_workflow: false,
                        path: None,
                        new_path: None,
                        content: None,
                        git_op: None,
                        message: None,
                        command: None,
                        timeout_ms: None,
                        max_output_bytes: None,
                        tool: None,
                        args: None,
                        target: None,
                        op: None,
                        id: None,
                        category: None,
                        tags: None,
                        scope: None,
                        key: Some(k.trim().to_string()),
                        value: Some(val),
                        updates: None,
                        label: None,
                        ..BridgeRequest::default()
                    };
                    let _ = &mut cfg_req;
                    self.config_set(&cfg_req).await
                } else {
                    self.config_get().await
                };
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "config", "data": res})
            }
            "/git" => {
                let res = self.diff(request.session_id.as_deref()).await;
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "git", "data": res})
            }
            "/agents" => {
                let cfg = self.config_get().await;
                let workflow = tokio::time::timeout(
                    Duration::from_millis(1200),
                    self.desktop_output("workflow", request.session_id.as_deref(), None),
                )
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(Value::Null);
                json!({
                    "ok": true,
                    "action": "slash_command",
                    "command": cmd,
                    "kind": "agents",
                    "data": {
                        "agents": cfg["agents"],
                        "workflow": workflow,
                    },
                })
            }
            "/skills" => {
                let caps = self
                    .runtime_capabilities(request.session_id.as_deref())
                    .await;
                json!({
                    "ok": caps["ok"],
                    "action": "slash_command",
                    "command": cmd,
                    "kind": "skills",
                    "data": caps["skills"],
                })
            }
            "/schedule" => {
                let mut sched_req = BridgeRequest {
                    action: "schedule".into(),
                    provider: None,
                    auth_method: None,
                    api_key: None,
                    api_base: None,
                    setup_value: None,
                    request_id: request.request_id.clone(),
                    session_id: request.session_id.clone(),
                    prompt: None,
                    model: None,
                    effort: None,
                    title: None,
                    working_dir: request.working_dir.clone(),
                    saved: None,
                    limit: None,
                    include_workflow: false,
                    path: None,
                    new_path: None,
                    content: None,
                    git_op: None,
                    message: None,
                    command: None,
                    timeout_ms: None,
                    max_output_bytes: None,
                    tool: None,
                    args: None,
                    target: None,
                    op: Some("list".into()),
                    id: None,
                    category: None,
                    tags: None,
                    scope: None,
                    key: None,
                    value: None,
                    updates: None,
                    label: None,
                    ..BridgeRequest::default()
                };
                if let Some(id) = rest.strip_prefix("cancel ") {
                    sched_req.op = Some("cancel".into());
                    sched_req.id = Some(id.trim().to_string());
                } else if let Some(id) = rest.strip_prefix("pause ") {
                    sched_req.op = Some("pause".into());
                    sched_req.id = Some(id.trim().to_string());
                } else if let Some(id) = rest.strip_prefix("resume ") {
                    sched_req.op = Some("resume".into());
                    sched_req.id = Some(id.trim().to_string());
                }
                let res = self.schedule_action(&sched_req).await;
                json!({"ok": res["ok"], "action": "slash_command", "command": cmd, "kind": "schedule", "data": res})
            }
            "/fork" | "/split" => match self.send_owned_control(OwnedControl::Split).await {
                Ok(res) => {
                    json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "session", "data": res})
                }
                Err(err) => {
                    json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                }
            },
            "/transfer" => match self.send_owned_control(OwnedControl::Transfer).await {
                Ok(res) => {
                    json!({"ok": true, "action": "slash_command", "command": cmd, "kind": "session", "data": res})
                }
                Err(err) => {
                    json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                }
            },
            "/model" => {
                if rest.is_empty() {
                    json!({
                        "ok": true,
                        "action": "slash_command",
                        "command": cmd,
                        "kind": "model_picker",
                        "data": {
                            "model": self.model,
                            "provider": self.provider,
                            "models": self.models,
                            "model_options": self.model_options,
                        }
                    })
                } else {
                    match self.submit_model(request.session_id.as_deref(), rest).await {
                        Ok(()) => json!({
                            "ok": true,
                            "action": "slash_command",
                            "command": cmd,
                            "kind": "model",
                            "data": {"model": self.model, "provider": self.provider}
                        }),
                        Err(err) => {
                            json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                        }
                    }
                }
            }
            "/improve" | "/refactor" | "/plan" | "/test" | "/fix" | "/poke" => {
                let prompt = match cmd.as_str() {
                    "/improve" => {
                        let plan_only = rest == "plan" || rest.starts_with("plan ");
                        let focus = rest.strip_prefix("plan ").map(str::trim).or((!plan_only && !rest.is_empty()).then_some(rest));
                        let focus_line = focus.map(|f| format!("\nFocus area: {f}.")).unwrap_or_default();
                        if plan_only {
                            format!("You are entering improvement planning mode for this repository. Inspect the codebase and write a concise ranked todo list using `todo` with the best 3-7 candidate improvements. Do not edit files.{focus_line}")
                        } else {
                            format!("You are entering improvement mode for this repository. Inspect the codebase, write a concise ranked todo list using `todo`, execute the strongest safe improvements, and validate with builds/tests.{focus_line}")
                        }
                    }
                    "/refactor" => {
                        let focus_line = if rest.is_empty() {
                            String::new()
                        } else {
                            format!("\nFocus area: {rest}.")
                        };
                        format!("Run a safe, behavior-preserving refactor loop on this repository. Inspect code smells/duplication, plan with `todo`, make small verified edits, and run tests after each change.{focus_line}")
                    }
                    "/plan" => {
                        let goal_line = if rest.is_empty() {
                            "Goal: produce a concrete, actionable plan for the current task in focus.".to_string()
                        } else {
                            format!("Goal: {rest}")
                        };
                        format!("You are entering planning mode.\n\n{goal_line}\n\nDo NOT implement anything yet or edit files. Read/search freely, then present the plan inside a ```plan ... ``` fenced block with Goal, Scope, Approach, Validation, and Open questions.")
                    }
                    "/test" => {
                        let target = if rest.is_empty() { "the current changes" } else { rest };
                        format!("Run a layered verification pass for {target}. Execute real builds/tests/linters and report exact commands run, outcomes, and any gaps.")
                    }
                    "/fix" => {
                        let target = if rest.is_empty() { "the current issue or interrupted turn" } else { rest };
                        format!("Diagnose and fix {target}. Inspect recent errors/state, identify the root cause, apply the minimal correct fix, and verify with tests.")
                    }
                    "/poke" => {
                        "Continue working on the remaining incomplete todos/tasks in this session and report progress when done.".to_string()
                    }
                    _ => unreachable!(),
                };
                let target_session = request.session_id.as_deref().unwrap_or(OWN_CHAT);
                match self.submit(target_session, &prompt).await {
                    Ok(()) => json!({
                        "ok": true,
                        "action": "slash_command",
                        "command": cmd,
                        "kind": "prompt_dispatched",
                        "data": {"sent": true, "prompt": prompt},
                    }),
                    Err(err) => {
                        json!({"ok": false, "action": "slash_command", "command": cmd, "error": format!("{err:#}")})
                    }
                }
            }
            _ => json!({
                "ok": false,
                "action": "slash_command",
                "command": cmd,
                "error": format!("Unsupported GUI slash command `{cmd}`. Send as a normal chat prompt or use a supported command."),
            }),
        }
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
                    json!({"available": false, "todos": [], "swarm": [], "swarm_plan": null, "detail": "Work data is unavailable for this chat. It may be saved, disconnected, or still starting."})
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
        let (
            own,
            active_dir,
            owned_busy,
            owned_error,
            owned_session_id,
            input_request,
            conversation,
            live_reply,
            owned_turn_seq,
            last_context_tokens,
            tokens_per_sec,
            session_input_tokens,
            session_output_tokens,
            session_cache_read_tokens,
            session_cache_creation_tokens,
            live_thinking,
            live_tools,
            side_pane_images,
            generated_images,
        ) = {
            let owned = self.owned.lock().unwrap_or_else(|error| error.into_inner());
            for sessions in results
                .into_iter()
                .filter_map(|result| result.ok().and_then(Result::ok))
            {
                connected += 1;
                for session in sessions {
                    if visible.contains(&session.id)
                        && owned.session_id.as_ref() != Some(&session.id)
                    {
                        self.sessions.insert(session.id.clone(), session);
                    }
                }
            }
            if unavailable > 0 {
                for (id, mut session) in previous {
                    if visible.contains(&id) && !self.sessions.contains_key(&id) {
                        session.state = "offline".to_string();
                        session.detail =
                            "Reconnecting to this session; its current activity is unknown"
                                .to_string();
                        self.sessions.insert(id, session);
                    }
                }
            }
            let own = CompanionSession {
                id: OWN_CHAT.to_string(),
                title: owned
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
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
                working_dir: owned.working_dir.clone(),
                saved: owned.saved,
                socket: None,
            };
            let active_dir = own.working_dir.as_deref().map(PathBuf::from);
            let live_reply = owned.reply.clone();
            let turn_seq = owned.turn_seq;
            let mut conversation = owned.transcript.clone();
            append_reply(&mut conversation, &live_reply);
            (
                own,
                active_dir,
                owned.busy,
                owned.error.clone(),
                owned.session_id.clone(),
                owned.input_request.clone(),
                conversation,
                live_reply,
                turn_seq,
                owned.last_context_tokens,
                owned.tokens_per_sec,
                owned.session_input_tokens,
                owned.session_output_tokens,
                owned.session_cache_read_tokens,
                owned.session_cache_creation_tokens,
                owned.live_thinking.clone(),
                owned.live_tools.clone(),
                owned.side_pane_images.clone(),
                owned.generated_images.clone(),
            )
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
        let git_branch = match active_dir.as_deref() {
            Some(dir) if dir.is_dir() => git_current_branch(dir).await,
            _ => String::new(),
        };

        let context_size = if last_context_tokens > 0 {
            last_context_tokens
        } else if conversation.is_empty() {
            0
        } else {
            conversation.len().div_ceil(4)
        };
        let context_max = resolve_context_max(&self.model, &self.provider);
        let session_cost_usd = compute_session_cost_usd(
            &self.model,
            session_input_tokens,
            session_output_tokens,
            session_cache_read_tokens,
            session_cache_creation_tokens,
        );
        let (lifetime_tokens, lifetime_requests) = crate::usage_ledger::totals();

        json!({
            "ok": true,
            "action": "status",
            "sessions": sessions,
            "working": working,
            "attention": attention,
            "offline": offline,
            "connected_servers": connected,
            "reply": display_text_tail(&conversation, MAX_REPLY_BYTES),
            "live_reply": display_text_tail(&live_reply, MAX_REPLY_BYTES),
            "owned_turn_seq": owned_turn_seq,
            "owned_busy": owned_busy,
            "owned_error": owned_error,
            "owned_session_id": owned_session_id,
            "input_request": input_request,
            "selected_session_id": selected,
            "selected_reply": selected_reply,
            "activity": self.activity,
            "model": self.model,
            "provider": self.provider,
            "effort": self.effort,
            "models": self.models,
            "model_options": self.model_options,
            "dictation_available": dictation_available,
            "workflow": workflow,
            "provider_connection": self.connections.status(),
            "git_branch": git_branch,
            "live_thinking": live_thinking,
            "live_tools": live_tools,
            "side_pane_images": side_pane_images,
            "generated_images": generated_images,
            "tokens_per_sec": tokens_per_sec,
            "context_size": context_size,
            "context_max": context_max,
            "token_metrics": {
                "tokens_per_sec": tokens_per_sec,
                "context_size": context_size,
                "context_max": context_max,
                "input_tokens": session_input_tokens,
                "output_tokens": session_output_tokens,
                "cache_read_tokens": session_cache_read_tokens,
                "cache_creation_tokens": session_cache_creation_tokens,
                "session_cost_usd": session_cost_usd,
                "lifetime_tokens": lifetime_tokens,
                "lifetime_requests": lifetime_requests,
            }
        })
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
            state.detail = "Sending command inputâ€¦".to_string();
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
        let target_id = session_id.trim();
        if !self.is_owned_selected(Some(target_id))
            && !self.sessions.contains_key(target_id)
            && self.stored_ids.contains(target_id)
        {
            self.resume_stored_chat(target_id).await?;
        }
        if self.is_owned_selected(Some(target_id)) {
            {
                let mut state = self.owned.lock().unwrap_or_else(|error| error.into_inner());
                if state.busy {
                    bail!("AlphaCode chat is still working; wait for its response");
                }
                let has_open_workspace = state
                    .working_dir
                    .as_deref()
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                    .is_some_and(|d| {
                        let path = Path::new(d);
                        path.is_dir() && !user_home_dir_or_install(path)
                    });
                if !has_open_workspace {
                    bail!("Open a project folder first before sending a message");
                }
                // Rotate the previous turn into `transcript` and clear live state
                // BEFORE `Ack` so an in-between `status` poll can never return
                // the previous reply as the active turn's `live_reply`.
                let previous = std::mem::take(&mut state.reply);
                if !previous.is_empty() {
                    append_reply(&mut state.transcript, &previous);
                    append_reply(&mut state.transcript, "\n\n");
                }
                append_reply(
                    &mut state.transcript,
                    &format!("You\n{prompt}\n\nAlphaCode\n"),
                );
                state.prompt_pre_recorded = true;
                state.turn_seq = state.turn_seq.saturating_add(1);
                state.busy = true;
                state.error = None;
                state.live_thinking = LiveThinking::default();
                state.live_tools.clear();
                state.side_pane_images.clear();
                state.generated_images.clear();
                state.turn_started_at = Some(Instant::now());
                state.api_call_started_at = Some(Instant::now());
                state.detail = "Connecting to AlphaCodeâ€¦".to_string();
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
            self.desktop_output("prompt", Some(target_id), Some(prompt)),
        )
        .await
        .context("Delivery could not be confirmed. Check the session before retrying")??;
        Ok(())
    }
}

async fn connect_owned_chat(
    working_dir: Option<String>,
    resume: Option<String>,
) -> Result<(BridgeSocketClient, String)> {
    if resume.is_none() {
        let valid = working_dir
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .is_some_and(|d| {
                let path = Path::new(d);
                path.is_dir() && !user_home_dir_or_install(path)
            });
        if !valid {
            bail!("Open a project folder first before starting a chat");
        }
    }
    if !crate::server_spawn::is_running().await {
        tokio::time::timeout(
            Duration::from_secs(30),
            crate::server_spawn::spawn_default_server(),
        )
        .await
        .context("AlphaCode server startup timed out")??;
    }
    let mut client = BridgeSocketClient::connect()
        .await
        .context("Could not connect to AlphaCode")?;
    let request_id = client
        .subscribe_with_info(working_dir, Some(false), resume.clone(), false, false)
        .await?;
    let session_id = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match client.read_event().await? {
                ServerEvent::SessionId { session_id } | ServerEvent::History { session_id, .. }
                    if resume
                        .as_ref()
                        .is_none_or(|expected| expected == &session_id) =>
                {
                    return Ok::<_, anyhow::Error>(session_id);
                }
                ServerEvent::Error { id, message, .. } if id == request_id => bail!("{message}"),
                _ => {}
            }
        }
    })
    .await
    .context("AlphaCode GUI session startup timed out")??;
    Ok((client, session_id))
}

fn apply_owned_server_event(
    state: &Arc<Mutex<OwnedChat>>,
    event: ServerEvent,
) -> Option<ServerEvent> {
    let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
    match event {
        ServerEvent::TextDelta { text } => {
            if guard.api_call_started_at.is_none() {
                guard.api_call_started_at = Some(Instant::now());
            }
            append_reply(&mut guard.reply, &text);
            guard.detail = "Writing a responseâ€¦".to_string();
            None
        }
        ServerEvent::TextReplace { text } => {
            guard.reply.clear();
            append_reply(&mut guard.reply, &text);
            None
        }
        ServerEvent::ReasoningDelta { text } => {
            if guard.api_call_started_at.is_none() {
                guard.api_call_started_at = Some(Instant::now());
            }
            append_bounded(&mut guard.live_thinking.text, &text, MAX_THINKING_BYTES);
            guard.live_thinking.active = true;
            guard.detail = "Thinkingâ€¦".to_string();
            None
        }
        ServerEvent::ReasoningDone { duration_secs } => {
            guard.live_thinking.active = false;
            guard.live_thinking.duration_secs = duration_secs;
            None
        }
        ServerEvent::ToolStart { id, name } => {
            guard.detail = format!("Preparing {name}");
            if let Some(existing) = guard.live_tools.iter_mut().find(|t| t.id == id) {
                existing.name = name;
                existing.status = "pending".to_string();
            } else {
                guard.live_tools.push(LiveToolCall {
                    id,
                    name,
                    args_raw: String::new(),
                    args: None,
                    output: String::new(),
                    error: None,
                    status: "pending".to_string(),
                });
                if guard.live_tools.len() > 64 {
                    guard.live_tools.remove(0);
                }
            }
            None
        }
        ServerEvent::ToolInput { delta } => {
            if let Some(tool) = guard.live_tools.last_mut() {
                append_bounded(&mut tool.args_raw, &delta, MAX_TOOL_ARGS_BYTES);
                if let Ok(parsed) = serde_json::from_str::<Value>(&tool.args_raw) {
                    tool.args = Some(parsed);
                }
            }
            None
        }
        ServerEvent::ToolExec { id, name } => {
            guard.detail = format!("Running {name}");
            if let Some(existing) = guard.live_tools.iter_mut().find(|t| t.id == id) {
                existing.name = name;
                existing.status = "running".to_string();
                if existing.args.is_none()
                    && !existing.args_raw.is_empty()
                    && let Ok(parsed) = serde_json::from_str::<Value>(&existing.args_raw)
                {
                    existing.args = Some(parsed);
                }
            } else {
                guard.live_tools.push(LiveToolCall {
                    id,
                    name,
                    args_raw: String::new(),
                    args: None,
                    output: String::new(),
                    error: None,
                    status: "running".to_string(),
                });
            }
            None
        }
        ServerEvent::ToolDone {
            id,
            name,
            output,
            error,
        } => {
            let status = if error.is_some() { "error" } else { "done" }.to_string();
            let cleaned_output = display_text(&output, MAX_TOOL_OUTPUT_BYTES);
            if let Some(existing) = guard.live_tools.iter_mut().find(|t| t.id == id) {
                existing.name = name;
                existing.output = cleaned_output;
                existing.error = error;
                existing.status = status;
            } else {
                guard.live_tools.push(LiveToolCall {
                    id,
                    name,
                    args_raw: String::new(),
                    args: None,
                    output: cleaned_output,
                    error,
                    status,
                });
            }
            None
        }
        ServerEvent::SidePaneImages { images, .. } => {
            guard.side_pane_images = images
                .into_iter()
                .filter_map(|img| serde_json::to_value(img).ok())
                .collect();
            None
        }
        ServerEvent::GeneratedImage {
            id,
            path,
            metadata_path,
            output_format,
            revised_prompt,
        } => {
            guard.generated_images.push(json!({
                "id": id,
                "path": path,
                "metadata_path": metadata_path,
                "output_format": output_format,
                "revised_prompt": revised_prompt,
            }));
            if guard.generated_images.len() > 20 {
                guard.generated_images.remove(0);
            }
            None
        }
        ServerEvent::TokenUsage {
            input,
            output,
            cache_read_input,
            cache_creation_input,
        } => {
            let cache_read = cache_read_input.unwrap_or(0);
            let cache_create = cache_creation_input.unwrap_or(0);
            guard.session_input_tokens = guard.session_input_tokens.saturating_add(input);
            guard.session_output_tokens = guard.session_output_tokens.saturating_add(output);
            guard.session_cache_read_tokens =
                guard.session_cache_read_tokens.saturating_add(cache_read);
            guard.session_cache_creation_tokens = guard
                .session_cache_creation_tokens
                .saturating_add(cache_create);
            guard.last_context_tokens = input
                .saturating_add(cache_read)
                .saturating_add(cache_create)
                .saturating_add(output) as usize;
            if let Some(started) = guard.api_call_started_at.or(guard.turn_started_at) {
                let rate = compute_tokens_per_sec(output, started.elapsed());
                if rate > 0.0 {
                    guard.tokens_per_sec = rate;
                }
            }
            guard.api_call_started_at = Some(Instant::now());
            None
        }
        ServerEvent::RetryRollback { attempt, max } => {
            guard.reply.clear();
            guard.error = None;
            guard.live_thinking = LiveThinking::default();
            guard.live_tools.clear();
            guard.api_call_started_at = Some(Instant::now());
            guard.detail = format!("Retrying attempt {attempt}/{max}â€¦");
            None
        }
        ServerEvent::SessionRenamed {
            session_id,
            title,
            display_title,
        } => {
            let matched = guard.session_id.as_deref() == Some(session_id.as_str());
            if matched {
                guard.title = Some(display_title.clone());
                crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
            }
            drop(guard);
            if matched {
                db::sync_session_renamed(&session_id, &display_title, title.as_deref());
            }
            None
        }
        other => Some(other),
    }
}

fn rebuild_transcript_from_history(messages: &[crate::protocol::HistoryMessage]) -> String {
    let mut transcript = String::new();
    let count = messages.len();
    for message in messages.iter().skip(count.saturating_sub(40)) {
        let role = match message.role.as_str() {
            "user" => "You",
            "assistant" => "AlphaCode",
            _ => "Session",
        };
        let body = display_text(&message.content, 4000);
        if !body.trim().is_empty() {
            append_reply(&mut transcript, &format!("{role}\n{body}\n\n"));
        }
    }
    transcript
}

async fn handle_idle_control(
    client: &mut BridgeSocketClient,
    state: &Arc<Mutex<OwnedChat>>,
    control: OwnedControl,
) -> Result<()> {
    match control {
        OwnedControl::Clear(reply) => {
            let id = client.alloc_id();
            if let Err(err) = client.send_request(&Request::Clear { id }).await {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::SessionId { session_id } => {
                            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                            s.session_id = Some(session_id.clone());
                            s.reply.clear();
                            s.transcript.clear();
                            s.live_thinking = LiveThinking::default();
                            s.live_tools.clear();
                            s.side_pane_images.clear();
                            s.generated_images.clear();
                            s.last_context_tokens = 0;
                            s.session_input_tokens = 0;
                            s.session_output_tokens = 0;
                            s.session_cache_read_tokens = 0;
                            s.session_cache_creation_tokens = 0;
                            s.tokens_per_sec = 0.0;
                            s.error = None;
                            s.detail = "Conversation cleared.".to_string();
                            return Ok::<_, anyhow::Error>(
                                json!({"cleared": true, "session_id": session_id}),
                            );
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Clear timed out".to_string()));
                }
            }
        }
        OwnedControl::Compact(reply) => {
            let id = client.alloc_id();
            if let Err(err) = client.send_request(&Request::Compact { id }).await {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::CompactResult {
                            id: cid,
                            message,
                            success,
                        } if cid == id => {
                            return Ok::<_, anyhow::Error>(
                                json!({"success": success, "message": message}),
                            );
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Compaction request timed out".to_string()));
                }
            }
        }
        OwnedControl::Rewind {
            message_index,
            reply,
        } => {
            let id = client.alloc_id();
            if let Err(err) = client
                .send_request(&Request::Rewind { id, message_index })
                .await
            {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::History { id: hid, messages, .. } if hid == id => {
                            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                            s.reply.clear();
                            s.transcript = rebuild_transcript_from_history(&messages);
                            s.detail = format!("Rewound to message {message_index}");
                            return Ok::<_, anyhow::Error>(json!({"rewound": true, "message_index": message_index, "message_count": messages.len()}));
                        }
                        ServerEvent::Error { id: eid, message, .. } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Rewind timed out".to_string()));
                }
            }
        }
        OwnedControl::RewindUndo(reply) => {
            let id = client.alloc_id();
            if let Err(err) = client.send_request(&Request::RewindUndo { id }).await {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::History {
                            id: hid, messages, ..
                        } if hid == id => {
                            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                            s.reply.clear();
                            s.transcript = rebuild_transcript_from_history(&messages);
                            s.detail = "Restored rewound messages".to_string();
                            return Ok::<_, anyhow::Error>(
                                json!({"undone": true, "message_count": messages.len()}),
                            );
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Rewind undo timed out".to_string()));
                }
            }
        }
        OwnedControl::Split(reply) => {
            let id = client.alloc_id();
            if let Err(err) = client.send_request(&Request::Split { id }).await {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::SplitResponse {
                            id: sid,
                            new_session_id,
                            new_session_name,
                        } if sid == id => {
                            return Ok::<_, anyhow::Error>(json!({
                                "forked": true,
                                "new_session_id": new_session_id,
                                "new_session_name": new_session_name,
                            }));
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Fork timed out".to_string()));
                }
            }
        }
        OwnedControl::Transfer(reply) => {
            let id = client.alloc_id();
            if let Err(err) = client.send_request(&Request::Transfer { id }).await {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::Ack { id: aid } if aid == id => {}
                        ServerEvent::SessionId { session_id } => {
                            return Ok::<_, anyhow::Error>(
                                json!({"transferred": true, "session_id": session_id}),
                            );
                        }
                        ServerEvent::Done { id: did } if did == id => {
                            return Ok::<_, anyhow::Error>(json!({"transferred": true}));
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Transfer timed out".to_string()));
                }
            }
        }
        OwnedControl::GetSessionTranscript { session_id, reply } => {
            let id = client.alloc_id();
            if let Err(err) = client
                .send_request(&Request::GetSessionTranscript { id, session_id })
                .await
            {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::SessionTranscript { id: tid, text, .. } if tid == id => {
                            return Ok::<_, anyhow::Error>(text);
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("Transcript export timed out".to_string()));
                }
            }
        }
        OwnedControl::SwitchAccount {
            provider,
            label,
            reply,
        } => {
            let id = client.alloc_id();
            let req = if provider == "claude" {
                Request::SwitchAnthropicAccount { id, label }
            } else {
                Request::SwitchOpenAiAccount { id, label }
            };
            if let Err(err) = client.send_request(&req).await {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let _ = reply.send(Ok(()));
        }
        OwnedControl::SetServiceTier { tier, reply } => {
            let id = client.alloc_id();
            if let Err(err) = client
                .send_request(&Request::SetServiceTier {
                    id,
                    service_tier: tier,
                })
                .await
            {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(4), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::ServiceTierChanged {
                            id: sid,
                            service_tier,
                            error,
                        } if sid == id => {
                            if let Some(err) = error {
                                bail!(err);
                            }
                            return Ok::<_, anyhow::Error>(service_tier);
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("SetServiceTier timed out".to_string()));
                }
            }
        }
        OwnedControl::SetTransport { transport, reply } => {
            let id = client.alloc_id();
            if let Err(err) = client
                .send_request(&Request::SetTransport { id, transport })
                .await
            {
                let _ = reply.send(Err(format!("{err:#}")));
                return Err(err);
            }
            let outcome = tokio::time::timeout(Duration::from_secs(4), async {
                loop {
                    match client.read_event().await? {
                        ServerEvent::TransportChanged {
                            id: tid,
                            transport,
                            error,
                        } if tid == id => {
                            if let Some(err) = error {
                                bail!(err);
                            }
                            return Ok::<_, anyhow::Error>(transport);
                        }
                        ServerEvent::Error {
                            id: eid, message, ..
                        } if eid == id => bail!(message),
                        other => {
                            let _ = apply_owned_server_event(state, other);
                        }
                    }
                }
            })
            .await;
            match outcome {
                Ok(Ok(val)) => {
                    let _ = reply.send(Ok(val));
                }
                Ok(Err(err)) => {
                    let _ = reply.send(Err(format!("{err:#}")));
                }
                Err(_) => {
                    let _ = reply.send(Err("SetTransport timed out".to_string()));
                }
            }
        }
    }
    Ok(())
}

async fn run_owned_chat(
    state: Arc<Mutex<OwnedChat>>,
    mut prompts: mpsc::Receiver<DesktopInput>,
    mut cancels: mpsc::Receiver<()>,
    mut controls: mpsc::Receiver<OwnedControl>,
    client: Option<BridgeSocketClient>,
) -> Result<()> {
    let mut client = if let Some(client) = client {
        client
    } else {
        let (working_dir, resume) = {
            let state = state.lock().unwrap_or_else(|error| error.into_inner());
            (state.working_dir.clone(), state.session_id.clone())
        };
        let (client, session_id) = connect_owned_chat(working_dir, resume).await?;
        let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
        state.session_id = Some(session_id);
        state.error = None;
        if !state.busy {
            state.detail = "Connected. Ready for your next task.".to_string();
        }
        client
    };

    loop {
        let prompt = tokio::select! {
            maybe_prompt = prompts.recv() => match maybe_prompt {
                Some(p) => p,
                None => break,
            },
            Some(control) = controls.recv() => {
                handle_idle_control(&mut client, &state, control).await?;
                continue;
            }
            event = client.read_event() => {
                if let Some(ServerEvent::SessionCloseRequested { reason }) = apply_owned_server_event(&state, event?) {
                    bail!("{reason}");
                }
                continue;
            }
        };

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
                    other => {
                        let _ = apply_owned_server_event(&state, other);
                    }
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
            if !std::mem::take(&mut state.prompt_pre_recorded) {
                let previous = std::mem::take(&mut state.reply);
                if !previous.is_empty() {
                    append_reply(&mut state.transcript, &previous);
                    append_reply(&mut state.transcript, "\n\n");
                }
                append_reply(
                    &mut state.transcript,
                    &format!("You\n{}\n\nAlphaCode\n", prompt.content),
                );
                state.turn_seq = state.turn_seq.saturating_add(1);
                state.live_thinking = LiveThinking::default();
                state.live_tools.clear();
                state.side_pane_images.clear();
                state.generated_images.clear();
            }
            state.error = None;
            state.turn_started_at = Some(Instant::now());
            state.api_call_started_at = Some(Instant::now());
        }
        {
            if !crate::alphacode_app_core::server::auto_title::is_greeting_or_command_for_auto_title(
                &prompt.content,
            ) && let Some(smart_title) = session_index::smart_title_from_text(&prompt.content)
            {
                let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
                let is_default = guard.title.as_deref().is_none_or(|t| {
                    let tr = t.trim();
                    tr.is_empty()
                        || tr.eq_ignore_ascii_case("New Conversation")
                        || tr.eq_ignore_ascii_case("AlphaCode chat")
                        || tr.eq_ignore_ascii_case("AlphaCode Companion")
                        || tr.eq_ignore_ascii_case("New Chat")
                });
                if is_default {
                    // Use the prompt snippet as an immediate in-memory label while
                    // the turn streams; the server emits `SessionRenamed` with the
                    // AI-generated title once the turn finishes.
                    guard.title = Some(smart_title);
                }
            }
        }
        let _ = prompt.accepted.send(Ok(()));
        loop {
            let event = tokio::select! {
                event = client.read_event() => event?,
                Some(()) = cancels.recv() => {
                    client.cancel().await?;
                    continue;
                }
                Some(control) = controls.recv() => {
                    match control {
                        OwnedControl::Clear(r)
                        | OwnedControl::Compact(r)
                        | OwnedControl::Rewind { reply: r, .. }
                        | OwnedControl::RewindUndo(r)
                        | OwnedControl::Split(r)
                        | OwnedControl::Transfer(r) => {
                            let _ = r.send(Err("Session is busy; wait for the turn to finish".to_string()));
                        }
                        OwnedControl::GetSessionTranscript { reply, .. } => {
                            let _ = reply.send(Err("Session is busy; wait for the turn to finish".to_string()));
                        }
                        OwnedControl::SwitchAccount { reply, .. } => {
                            let _ = reply.send(Err("Session is busy; wait for the turn to finish".to_string()));
                        }
                        OwnedControl::SetServiceTier { reply, .. }
                        | OwnedControl::SetTransport { reply, .. } => {
                            let _ = reply.send(Err("Session is busy; wait for the turn to finish".to_string()));
                        }
                    }
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
                state.detail = "Continuing the commandâ€¦".to_string();
                continue;
            }
            if let Some(remaining) = apply_owned_server_event(&state, event) {
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                match remaining {
                    ServerEvent::Done { id } if id == request_id => {
                        state.busy = false;
                        state.live_thinking.active = false;
                        state.input_request = None;
                        state.error = None;
                        state.detail = "Response ready".to_string();
                        state.turn_started_at = None;
                        state.api_call_started_at = None;
                        let finished_sid = state.session_id.clone();
                        drop(state);
                        if let Some(sid) = finished_sid {
                            tokio::task::spawn_blocking(move || {
                                db::sync_session_by_id(&sid);
                            });
                        }
                        break;
                    }
                    ServerEvent::Interrupted => {
                        state.busy = false;
                        state.live_thinking.active = false;
                        state.input_request = None;
                        state.error = None;
                        state.detail = "Interrupted".to_string();
                        state.turn_started_at = None;
                        state.api_call_started_at = None;
                        break;
                    }
                    ServerEvent::ProviderGuardrail { message, .. } => {
                        state.busy = false;
                        state.live_thinking.active = false;
                        state.input_request = None;
                        state.error = Some(message);
                        state.turn_started_at = None;
                        state.api_call_started_at = None;
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
                            state.live_thinking.active = false;
                            state.input_request = None;
                            state.error = Some(message);
                            state.turn_started_at = None;
                            state.api_call_started_at = None;
                            break;
                        }
                    }
                    ServerEvent::SessionCloseRequested { reason } => bail!("{reason}"),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

impl Bridge {
    async fn handle_bridge_request(&mut self, request: BridgeRequest) -> Value {
        match request.action.as_str() {
            "providers" => self.connections.providers(),
            "provider_connect" => {
                match self.connections.start(
                    request.provider.as_deref().unwrap_or_default(),
                    request.auth_method.as_deref().unwrap_or_default(),
                    request.api_key.unwrap_or_default(),
                    request.api_base.as_deref(),
                    request.setup_value.as_deref().unwrap_or_default(),
                ) {
                    Ok(()) => {
                        json!({"ok": true, "action": "provider_connect", "connection": self.connections.status()})
                    }
                    Err(error) => {
                        json!({"ok": false, "action": "provider_connect", "error": format!("{error:#}")})
                    }
                }
            }
            "provider_input" => {
                match self
                    .connections
                    .send_input(request.prompt.as_deref().unwrap_or_default())
                    .await
                {
                    Ok(()) => {
                        json!({"ok": true, "action": "provider_input", "connection": self.connections.status()})
                    }
                    Err(error) => {
                        json!({"ok": false, "action": "provider_input", "error": format!("{error:#}")})
                    }
                }
            }
            "provider_cancel" => {
                self.connections.cancel().await;
                json!({"ok": true, "action": "provider_cancel", "connection": self.connections.status()})
            }
            "provider_logout" => {
                let prov = request.provider.as_deref().unwrap_or_default();
                let label = request.label.as_deref().or(request.prompt.as_deref());
                match self.connections.logout(prov, label) {
                    Ok(payload) => payload,
                    Err(error) => {
                        json!({"ok": false, "action": "provider_logout", "error": format!("{error:#}")})
                    }
                }
            }
            "account_list" => self.connections.accounts(),
            "account_switch" => {
                let prov = request.provider.as_deref().unwrap_or_default();
                let label = request
                    .label
                    .as_deref()
                    .or(request.prompt.as_deref())
                    .unwrap_or_default();
                match self.connections.switch_account(prov, label) {
                    Ok((canonical, payload)) => {
                        provider_connections::bump_auth_completed_epoch();
                        if let Some(tx) = self.control_tx.as_ref().filter(|tx| !tx.is_closed()) {
                            let (reply_tx, reply_rx) = oneshot::channel();
                            if tx
                                .send(OwnedControl::SwitchAccount {
                                    provider: canonical,
                                    label: label.trim().to_string(),
                                    reply: reply_tx,
                                })
                                .await
                                .is_ok()
                            {
                                let _ =
                                    tokio::time::timeout(Duration::from_secs(2), reply_rx).await;
                            }
                        }
                        payload
                    }
                    Err(error) => {
                        json!({"ok": false, "action": "account_switch", "error": format!("{error:#}")})
                    }
                }
            }
            "status" => {
                self.workflow_requested = request.include_workflow;
                self.snapshot(request.session_id.as_deref()).await
            }
            "reconnect" => {
                self.owned
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .error = None;
                self.ensure_owned_chat();
                json!({"ok": true, "action": "reconnect", "reconnecting": true})
            }
            "dictate" => match self.submit_dictate().await {
                Ok(value) => value,
                Err(error) => {
                    json!({"ok": false, "action": "dictate", "error": format!("{error:#}")})
                }
            },
            "input" => {
                match self
                    .submit_input(
                        request.request_id.as_deref().unwrap_or(""),
                        request.prompt.as_deref().unwrap_or(""),
                    )
                    .await
                {
                    Ok(()) => json!({"ok": true, "action": "input", "sent": true}),
                    Err(error) => {
                        json!({"ok": false, "action": "input", "error": format!("{error:#}")})
                    }
                }
            }
            "prompt" => {
                match self
                    .submit(
                        request.session_id.as_deref().unwrap_or(OWN_CHAT),
                        request.prompt.as_deref().unwrap_or(""),
                    )
                    .await
                {
                    Ok(()) => json!({"ok": true, "action": "prompt", "sent": true}),
                    Err(error) => {
                        json!({"ok": false, "action": "prompt", "error": format!("{error:#}")})
                    }
                }
            }
            "model" => {
                match self
                    .submit_model(
                        request.session_id.as_deref(),
                        request.model.as_deref().unwrap_or(""),
                    )
                    .await
                {
                    Ok(()) => {
                        json!({"ok": true, "action": "model", "model": self.model, "provider": self.provider})
                    }
                    Err(error) => {
                        json!({"ok": false, "action": "model", "error": format!("{error:#}")})
                    }
                }
            }
            "effort" => {
                match self
                    .submit_effort(
                        request.session_id.as_deref(),
                        request.effort.as_deref().unwrap_or(""),
                    )
                    .await
                {
                    Ok(()) => json!({"ok": true, "action": "effort", "effort": self.effort}),
                    Err(error) => {
                        json!({"ok": false, "action": "effort", "error": format!("{error:#}")})
                    }
                }
            }
            "cancel" => match self.submit_cancel(request.session_id.as_deref()).await {
                Ok(value) => value,
                Err(error) => {
                    json!({"ok": false, "action": "cancel", "error": format!("{error:#}")})
                }
            },
            "rename" => {
                match self
                    .submit_rename(request.session_id.as_deref(), request.title.as_deref())
                    .await
                {
                    Ok(value) => value,
                    Err(error) => {
                        json!({"ok": false, "action": "rename", "error": format!("{error:#}")})
                    }
                }
            }
            "archive" => {
                let saved = request.saved.unwrap_or(true);
                match self
                    .submit_archive(
                        request.session_id.as_deref(),
                        request.title.as_deref(),
                        saved,
                    )
                    .await
                {
                    Ok(value) => value,
                    Err(error) => {
                        json!({"ok": false, "action": "archive", "error": format!("{error:#}")})
                    }
                }
            }
            "sessions" => {
                match self
                    .list_stored_sessions(request.limit.unwrap_or(0) as usize)
                    .await
                {
                    // Wrapped under `stored` so it can never be confused with
                    // the live `sessions` array returned by `status`.
                    Ok(value) => json!({"ok": true, "action": "sessions", "stored": value}),
                    Err(error) => {
                        json!({"ok": false, "action": "sessions", "error": format!("{error:#}")})
                    }
                }
            }
            "new_chat" => match self.submit_new_chat(request.working_dir.as_deref()).await {
                Ok(mut value) => {
                    if self.has_valid_owned_workspace() {
                        self.ensure_owned_chat();
                    }
                    value["snapshot"] = self.snapshot(Some(OWN_CHAT)).await;
                    value
                }
                Err(error) => {
                    json!({"ok": false, "action": "new_chat", "error": format!("{error:#}")})
                }
            },
            "resume_chat" => {
                let target_id = request.session_id.as_deref().unwrap_or("");
                match self.resume_stored_chat(target_id).await {
                    Ok(value) => {
                        let resumed_dir = self
                            .owned
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .working_dir
                            .clone();
                        if let Some(dir) = resumed_dir {
                            self.record_workspace_visit(&dir, true);
                        }
                        self.gui_state.last_session_id = Some(target_id.to_string());
                        let _ = save_gui_state(&self.gui_state);
                        value
                    }
                    Err(error) => {
                        json!({"ok": false, "action": "resume_chat", "error": format!("{error:#}")})
                    }
                }
            }
            "delete_chat" => {
                match self
                    .delete_stored_chat(request.session_id.as_deref().unwrap_or(""))
                    .await
                {
                    Ok(value) => value,
                    Err(error) => {
                        json!({"ok": false, "action": "delete_chat", "error": format!("{error:#}")})
                    }
                }
            }
            "gui_state_get" => self.gui_state_get(),
            "gui_state_set" => self.gui_state_set(&request),
            "workspace_select" => self.workspace_select(&request),
            "workspace_add" | "workspaces_add" => self.workspace_add(&request),
            "workspace_remove" => self.workspace_remove(&request),
            "workspace_pin" => self.workspace_pin(&request),
            "workspace_collapse" => self.workspace_collapse(&request),
            "reveal_path" => {
                self.reveal_path_in_os(request.path.as_deref().or(request.working_dir.as_deref()))
                    .await
            }
            "capabilities" => {
                self.runtime_capabilities(request.session_id.as_deref())
                    .await
            }
            "transcript" => self.transcript(request.session_id.as_deref()).await,
            "diff" => self.diff(request.session_id.as_deref()).await,
            "files" => {
                self.list_files(
                    request.working_dir.as_deref(),
                    request.session_id.as_deref(),
                )
                .await
            }
            "pick_folder" => pick_folder().await,
            "read_file" => {
                self.read_file(request.path.as_deref(), request.session_id.as_deref())
                    .await
            }
            "write_file" => self.write_file(&request).await,
            "create_file" => self.create_file(&request).await,
            "create_dir" => self.create_dir(&request).await,
            "delete_path" => self.delete_path(&request).await,
            "rename_path" => self.rename_path(&request).await,
            "git_action" => self.git_action(&request).await,
            "shell_exec" => self.shell_exec(&request).await,
            "slash_commands" => self.slash_commands_list(),
            "slash_command" => self.execute_slash_command(&request).await,
            "doctor" => self.doctor().await,
            "usage" => self.usage().await,
            "memory" => self.memory(&request).await,
            "config_get" => self.config_get().await,
            "config_set" => self.config_set(&request).await,
            "copy_session" => self.copy_session(request.session_id.as_deref()).await,
            "run_tool" => self.run_tool(&request, false).await,
            "run_recon" => self.run_tool(&request, true).await,
            "schedule" => self.schedule_action(&request).await,
            _ => json!({"ok": false, "error": "Unknown or unsupported bridge action"}),
        }
    }
}

pub(crate) async fn run(once: bool, preview: bool, bridge: bool, no_update: bool) -> Result<()> {
    if once {
        println!("{}", Bridge::default().snapshot(None).await);
        return Ok(());
    }
    if bridge {
        let mut state = Bridge::with_saved_state();
        state.ensure_owned_chat();
        let live_sessions: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();
        let writer_handle = tokio::spawn(async move {
            let mut stdout = tokio::io::stdout();
            while let Some(response) = out_rx.recv().await {
                if let Ok(mut encoded) = serde_json::to_vec(&response) {
                    encoded.push(b'\n');
                    if stdout.write_all(&encoded).await.is_err() {
                        break;
                    }
                    if stdout.flush().await.is_err() {
                        break;
                    }
                }
            }
        });

        let (work_tx, mut work_rx) = mpsc::unbounded_channel::<(BridgeRequest, Option<String>)>();
        let out_tx_worker = out_tx.clone();
        let live_sessions_worker = Arc::clone(&live_sessions);
        let worker_handle = tokio::spawn(async move {
            while let Some((request, req_id_echo)) = work_rx.recv().await {
                let mut response = state.handle_bridge_request(request).await;
                {
                    let mut live = live_sessions_worker
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    *live = state.sessions.keys().cloned().collect();
                }
                if let Some(req_id) = req_id_echo
                    && response.is_object()
                    && response.get("request_id").is_none()
                {
                    response["request_id"] = json!(req_id);
                }
                if out_tx_worker.send(response).is_err() {
                    break;
                }
            }
        });

        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Some(line) = lines.next_line().await? {
            let parsed = serde_json::from_str::<BridgeRequest>(&line);
            let req_id_echo = parsed
                .as_ref()
                .ok()
                .and_then(|r| r.request_id.clone())
                .filter(|id| !id.trim().is_empty());
            match parsed {
                Ok(request)
                    if request.action == "transcript"
                        && request
                            .session_id
                            .as_deref()
                            .map(str::trim)
                            .is_some_and(|id| {
                                id != OWN_CHAT
                                    && is_safe_session_id(id)
                                    && !live_sessions
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .contains(id)
                                    && crate::session::session_exists(id)
                            }) =>
                {
                    // Load stored session transcript immediately from disk without
                    // waiting behind `resume_chat` connecting to the daemon.
                    let id = request
                        .session_id
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    let out_tx_fast = out_tx.clone();
                    tokio::spawn(async move {
                        let mut response = load_stored_transcript_response(&id).await;
                        if let Some(req_id) = req_id_echo
                            && response.is_object()
                            && response.get("request_id").is_none()
                        {
                            response["request_id"] = json!(req_id);
                        }
                        let _ = out_tx_fast.send(response);
                    });
                }
                Ok(request) => {
                    if work_tx.send((request, req_id_echo)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let mut response =
                        json!({"ok": false, "error": "Unknown or unsupported bridge action"});
                    if let Some(req_id) = req_id_echo {
                        response["request_id"] = json!(req_id);
                    }
                    if out_tx.send(response).is_err() {
                        break;
                    }
                }
            }
        }
        drop(work_tx);
        let _ = worker_handle.await;
        drop(out_tx);
        let _ = writer_handle.await;
        return Ok(());
    }
    launch(preview, no_update)
}

#[cfg(windows)]
fn launch(preview: bool, no_update: bool) -> Result<()> {
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
    if no_update {
        command.env("ALPHACODE_NO_AUTO_UPDATE", "1");
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
/// packer/dropper heuristics score â€” BitDefender (`Gen:Variant.Barys`, resold as
/// ALYac/Arcibit/eScan/Emsisoft/GData/VIPRE) flagged the embedded layout â€” so
/// the default build carries no second executable in its data section.
///
/// Resolution order:
/// 1. `ALPHACODE_DESKTOP_HOST` â€” explicit override for packaging/tests.
/// 2. The versioned release host, when its CLI exactly matches the running CLI.
/// 3. Beside the running executable â€” the release archive and `cargo build`
///    layouts, and the directory the installer writes.
/// 4. `~/.alphacode/gui/` â€” where older builds unpacked the embedded copy, so
///    an install upgraded in place keeps finding its host.
///
/// Only the legacy `embedded-gui-host` feature still writes an extracted copy,
/// into the directory from (4).
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

    // Older updaters copied only the new CLI to the launcher, leaving the
    // previous GUI beside it. Prefer the release's companion host when the
    // versioned CLI is byte-for-byte identical to the running executable.
    if let (Ok(exe), Ok(versioned)) = (
        std::env::current_exe(),
        crate::alphacode_build_support::version_binary_path(crate::alphacode_build_meta::semver()),
    ) && let Some(host) = matching_versioned_desktop_host(&exe, &versioned)
    {
        return Ok(host);
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

#[cfg(windows)]
fn matching_versioned_desktop_host(
    exe: &std::path::Path,
    versioned: &std::path::Path,
) -> Option<PathBuf> {
    use std::io::Read;
    let host = versioned.parent()?.join(DESKTOP_HOST_FILENAME);
    if !host.is_file() {
        return None;
    }
    let mut left = std::fs::File::open(exe).ok()?;
    let mut right = std::fs::File::open(versioned).ok()?;
    if left.metadata().ok()?.len() != right.metadata().ok()?.len() {
        return None;
    }
    let mut a = [0_u8; 64 * 1024];
    let mut b = [0_u8; 64 * 1024];
    loop {
        let count = left.read(&mut a).ok()?;
        right.read_exact(&mut b[..count]).ok()?;
        if a[..count] != b[..count] {
            return None;
        }
        if count == 0 {
            return Some(host);
        }
    }
}

#[cfg(not(windows))]
fn launch(_preview: bool, _no_update: bool) -> Result<()> {
    bail!(
        "The AlphaCode GUI currently supports Windows. Use `alphacode gui --once` for status on this platform"
    )
}

/// Replace free-pool model ids with the product display name.
///
/// The Alphax Free gateway is an internal implementation detail. A user picked
/// "Alphax Free"; which upstream concrete id happened to serve the turn â€” or
/// appears in the picker â€” is ours to hide, not theirs to reason about. The TUI
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

    #[cfg(windows)]
    #[test]
    fn versioned_host_is_used_only_for_identical_cli() {
        let temp = tempfile::tempdir().unwrap();
        let launcher = temp.path().join("launcher.exe");
        let version_dir = temp.path().join("version");
        std::fs::create_dir_all(&version_dir).unwrap();
        let versioned = version_dir.join("alphacode.exe");
        let host = version_dir.join(DESKTOP_HOST_FILENAME);
        std::fs::write(&launcher, b"same binary").unwrap();
        std::fs::write(&versioned, b"same binary").unwrap();
        std::fs::write(&host, b"matching host").unwrap();
        assert_eq!(
            matching_versioned_desktop_host(&launcher, &versioned),
            Some(host)
        );
        std::fs::write(&versioned, b"different!!").unwrap();
        assert_eq!(matching_versioned_desktop_host(&launcher, &versioned), None);
    }

    /// The resolver looks for `DESKTOP_HOST_FILENAME` beside `alphacode.exe`.
    /// That name also appears in `build.rs` (what gets compiled and staged) and
    /// in `scripts/install.ps1` (what gets copied). If the three drift apart,
    /// `alphacode gui` reports "host not found" for every packaged user while
    /// the build stays green â€” so this test reads the other two files and fails
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

    /// The GUI host is its own PE, so it needs its own structural checks.
    ///
    /// The 1.1.21 host (`AlphaCode.Desktop.exe`, sha256 `f61b747a…`) was
    /// reported malicious by 11 engines and tagged `overlay`: `build.rs`
    /// appended a compiled VERSIONINFO `.res` after the last section, which left
    /// a PE overlay *and* made Windows read ProductName/CompanyName/
    /// LegalCopyright as empty. The guard that catches that shape —
    /// `scripts/verify-pe-metadata.py` — plus the AV-trigger scan now have to run
    /// against the host, not only the CLI. This test fails if either wiring is
    /// dropped, so the check cannot silently regress to covering one binary.
    #[cfg(windows)]
    #[test]
    fn release_guards_cover_the_gui_host_too() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(
            root.join("scripts").join("verify-pe-metadata.py").is_file(),
            "scripts/verify-pe-metadata.py is missing; nothing enforces the \
             zero-overlay and complete-VERSIONINFO contract"
        );

        for entry in [
            root.join(".github").join("workflows").join("release.yml"),
            root.join("scripts").join("release-build.sh"),
        ] {
            let text = std::fs::read_to_string(&entry)
                .unwrap_or_else(|_| panic!("{} is part of the source tree", entry.display()));

            // `release.yml` names the host file literally; `release-build.sh`
            // assigns it to a shell variable (HOST=.../AlphaCode.Desktop.exe)
            // and passes "$HOST". Both are valid as long as the variable really
            // resolves to the host, so collect the variable names whose
            // assignment mentions the filename and accept either form.
            let mut host_tokens = vec![DESKTOP_HOST_FILENAME.to_string()];
            for line in text.lines() {
                let Some((name, value)) = line.split_once('=') else {
                    continue;
                };
                let name = name.trim();
                if value.contains(DESKTOP_HOST_FILENAME)
                    && !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                {
                    host_tokens.push(format!("${{{name}}}"));
                    host_tokens.push(format!("${name}"));
                }
            }

            for script in ["verify-pe-metadata.py", "verify-no-av-triggers.py"] {
                // Each guard must be invoked against the host, not just the CLI:
                // the same line has to name both the script and the host file.
                let covered = text.lines().any(|line| {
                    line.contains(script) && host_tokens.iter().any(|token| line.contains(token))
                });
                assert!(
                    covered,
                    "{} runs {} on alphacode.exe but not on {} (tokens: {:?})",
                    entry.display(),
                    script,
                    DESKTOP_HOST_FILENAME,
                    host_tokens
                );
            }
        }
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

        // Feeding the shown label back â€” which is what the picker does on a
        // selection â€” must resolve to a routable id, not to the label itself.
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
        let mut reply = "ç•Œ".repeat(MAX_REPLY_BYTES);
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

    #[test]
    fn workspace_path_sandbox_blocks_traversal_on_write_create_delete_and_rename() {
        let outer = tempfile::tempdir().unwrap();
        let workspace = outer.path().join("workspace");
        let outside = outer.path().join("outside");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.txt");
        std::fs::write(&secret, "top secret").unwrap();

        // Direct parent traversal & nested non-existent ancestor traversal
        assert!(write_file_in_workspace(&workspace, "../outside/secret.txt", "pwn").is_err());
        assert!(
            write_file_in_workspace(&workspace, "sub/missing/../../../outside/pwn.txt", "pwn")
                .is_err()
        );
        assert!(create_file_in_workspace(&workspace, "../outside/new.txt", "pwn").is_err());
        assert!(create_dir_in_workspace(&workspace, "../outside/evil_dir").is_err());
        assert!(delete_path_in_workspace(&workspace, "../outside/secret.txt").is_err());
        assert!(delete_path_in_workspace(&workspace, ".").is_err());
        assert!(
            rename_path_in_workspace(&workspace, "../outside/secret.txt", "stolen.txt").is_err()
        );
        // Valid inside-workspace file operations succeed and respect boundaries
        let created =
            create_file_in_workspace(&workspace, "src/main.rs", "fn main() {}\n").unwrap();
        assert_eq!(created["ok"], true);
        // Creating an already existing file fails
        assert!(create_file_in_workspace(&workspace, "src/main.rs", "dup").is_err());
        // Cannot move inside-workspace file outside workspace
        assert!(
            rename_path_in_workspace(&workspace, "src/main.rs", "../outside/escaped.rs").is_err()
        );
        assert!(workspace.join("src/main.rs").is_file());
        // Valid write, rename, create_dir, and delete
        let written =
            write_file_in_workspace(&workspace, "src/main.rs", "fn main() { 1; }\n").unwrap();
        assert_eq!(written["ok"], true);
        let dir_res = create_dir_in_workspace(&workspace, "nested/dir").unwrap();
        assert_eq!(dir_res["ok"], true);
        assert!(workspace.join("nested/dir").is_dir());
        let renamed =
            rename_path_in_workspace(&workspace, "src/main.rs", "nested/dir/app.rs").unwrap();
        assert_eq!(renamed["ok"], true);
        assert!(!workspace.join("src/main.rs").exists());
        assert!(workspace.join("nested/dir/app.rs").is_file());
        let deleted = delete_path_in_workspace(&workspace, "nested/dir/app.rs").unwrap();
        assert_eq!(deleted["kind"], "file");
        assert!(!workspace.join("nested/dir/app.rs").exists());

        // Absolute outside path & NUL byte are blocked
        assert!(
            write_file_in_workspace(&workspace, &secret.to_string_lossy(), "overwrite").is_err()
        );
        assert!(write_file_in_workspace(&workspace, "bad\0name.txt", "x").is_err());
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "top secret");
    }

    #[tokio::test]
    async fn shell_exec_respects_workspace_bounds_output_cap_and_timeout() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path();

        assert!(
            shell_exec_in_workspace(workspace, "   ", None, None)
                .await
                .is_err()
        );
        assert!(
            shell_exec_in_workspace(workspace, &"a".repeat(32 * 1024 + 1), None, None)
                .await
                .is_err()
        );

        let res = shell_exec_in_workspace(workspace, "echo hello-bridge", None, None)
            .await
            .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["exit_code"], 0);
        assert!(
            res["stdout"]
                .as_str()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains("hello-bridge")
        );

        // Output cap truncation
        #[cfg(windows)]
        let spam_cmd = "1..400 | ForEach-Object { '0123456789ABCDEF' }";
        #[cfg(not(windows))]
        let spam_cmd = "i=0; while [ $i -lt 400 ]; do echo 0123456789ABCDEF; i=$((i+1)); done";
        let capped = shell_exec_in_workspace(workspace, spam_cmd, Some(10_000), Some(1024))
            .await
            .unwrap();
        assert_eq!(capped["truncated"], true);
        assert!(capped["stdout"].as_str().unwrap().len() <= 1100);

        // Timeout enforcement (clamped floor 500ms)
        #[cfg(windows)]
        let sleep_cmd = "Start-Sleep -Seconds 3";
        #[cfg(not(windows))]
        let sleep_cmd = "sleep 3";
        let timed_out = shell_exec_in_workspace(workspace, sleep_cmd, Some(500), None)
            .await
            .unwrap();
        assert_eq!(timed_out["timed_out"], true);
        assert_eq!(timed_out["ok"], false);
    }

    #[test]
    fn real_metrics_and_structured_live_events_are_tracked_without_fakes() {
        assert_eq!(compute_tokens_per_sec(0, Duration::from_secs(2)), 0.0);
        assert_eq!(compute_tokens_per_sec(100, Duration::from_millis(0)), 0.0);
        let rate = compute_tokens_per_sec(125, Duration::from_millis(2500));
        assert!((rate - 50.0).abs() < 0.01);

        assert_eq!(
            resolve_context_max("claude-opus-4-8", "claude"),
            Some(1_000_000)
        );
        assert_eq!(resolve_context_max("gpt-5.4", "openai"), Some(1_000_000));
        assert_eq!(resolve_context_max("", ""), None);

        let state = Arc::new(Mutex::new(OwnedChat {
            turn_started_at: Some(Instant::now() - Duration::from_secs(2)),
            api_call_started_at: Some(Instant::now() - Duration::from_secs(2)),
            ..OwnedChat::default()
        }));

        apply_owned_server_event(
            &state,
            ServerEvent::ReasoningDelta {
                text: "Checking files".into(),
            },
        );
        assert!(state.lock().unwrap().live_thinking.active);
        assert_eq!(state.lock().unwrap().live_thinking.text, "Checking files");

        apply_owned_server_event(
            &state,
            ServerEvent::ReasoningDone {
                duration_secs: Some(1.2),
            },
        );
        assert!(!state.lock().unwrap().live_thinking.active);
        assert_eq!(state.lock().unwrap().live_thinking.duration_secs, Some(1.2));

        apply_owned_server_event(
            &state,
            ServerEvent::ToolStart {
                id: "call_1".into(),
                name: "read".into(),
            },
        );
        apply_owned_server_event(
            &state,
            ServerEvent::ToolInput {
                delta: "{\"path\":\"src/lib.rs\"}".into(),
            },
        );
        apply_owned_server_event(
            &state,
            ServerEvent::ToolExec {
                id: "call_1".into(),
                name: "read".into(),
            },
        );
        assert_eq!(state.lock().unwrap().live_tools[0].status, "running");
        assert_eq!(
            state.lock().unwrap().live_tools[0]
                .args
                .as_ref()
                .and_then(|v| v.get("path"))
                .and_then(Value::as_str),
            Some("src/lib.rs")
        );

        apply_owned_server_event(
            &state,
            ServerEvent::ToolDone {
                id: "call_1".into(),
                name: "read".into(),
                output: "pub fn run()".into(),
                error: None,
            },
        );
        assert_eq!(state.lock().unwrap().live_tools[0].status, "done");
        assert_eq!(state.lock().unwrap().live_tools[0].output, "pub fn run()");

        apply_owned_server_event(
            &state,
            ServerEvent::GeneratedImage {
                id: "img_1".into(),
                path: "/tmp/out.png".into(),
                metadata_path: None,
                output_format: "png".into(),
                revised_prompt: Some("diagram".into()),
            },
        );
        assert_eq!(state.lock().unwrap().generated_images.len(), 1);

        apply_owned_server_event(
            &state,
            ServerEvent::TokenUsage {
                input: 1200,
                output: 100,
                cache_read_input: Some(300),
                cache_creation_input: None,
            },
        );
        {
            let guard = state.lock().unwrap();
            assert_eq!(guard.session_input_tokens, 1200);
            assert_eq!(guard.session_output_tokens, 100);
            assert_eq!(guard.session_cache_read_tokens, 300);
            assert_eq!(guard.last_context_tokens, 1600);
            assert!(guard.tokens_per_sec > 0.0);
        }
    }

    #[test]
    fn git_pathspec_rejects_flag_injection_and_slash_commands_list_populated() {
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_git_pathspec(temp.path(), "--exec=evil").is_err());
        assert!(validate_git_pathspec(temp.path(), "../outside").is_err());
        let bridge = Bridge::default();
        let list = bridge.slash_commands_list();
        assert_eq!(list["ok"], true);
        let cmds = list["commands"].as_array().unwrap();
        assert!(cmds.iter().any(|c| c["name"] == "/compact"));
        assert!(cmds.iter().any(|c| c["name"] == "/doctor"));
    }

    #[tokio::test]
    async fn snapshot_exposes_owned_error_when_set() {
        let mut bridge = Bridge::default();
        {
            let mut state = bridge.owned.lock().unwrap();
            state.error = Some("HTTP 400: invalid request".to_string());
        }
        let snap = bridge.snapshot(Some(OWN_CHAT)).await;
        assert_eq!(snap["owned_error"], "HTTP 400: invalid request");
        assert_eq!(snap["sessions"][0]["state"], "attention");

        {
            let mut state = bridge.owned.lock().unwrap();
            state.error = None;
        }
        let cleared = bridge.snapshot(Some(OWN_CHAT)).await;
        assert!(cleared["owned_error"].is_null());
    }

    #[tokio::test]
    async fn gui_state_persists_preferences_workspaces_and_deletes_stored_chat() {
        let _lock = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("my_project");
        std::fs::create_dir_all(&workspace).unwrap();
        let prev = std::env::var_os("ALPHACODE_HOME");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        let mut bridge = Bridge::default();
        let ws_str = workspace.to_string_lossy().into_owned();
        let req = BridgeRequest {
            action: "gui_state_set".into(),
            provider: None,
            auth_method: None,
            api_key: None,
            api_base: None,
            setup_value: None,
            request_id: None,
            session_id: None,
            prompt: None,
            model: None,
            effort: None,
            title: None,
            working_dir: None,
            saved: None,
            limit: None,
            include_workflow: false,
            path: None,
            new_path: None,
            content: None,
            git_op: None,
            message: None,
            command: None,
            timeout_ms: None,
            max_output_bytes: None,
            tool: None,
            args: None,
            target: None,
            op: None,
            id: None,
            category: None,
            tags: None,
            scope: None,
            key: None,
            value: None,
            updates: Some(json!({
                "last_working_dir": ws_str,
                "theme": "light",
                "lang": "ar",
                "font_size": 16,
                "sidebar_open": false,
                "pinned_workspaces": [workspace.to_string_lossy()],
            })),
            ..BridgeRequest::default()
        };
        let saved_resp = bridge.gui_state_set(&req);
        assert_eq!(saved_resp["ok"], true);
        let reloaded = load_gui_state();
        assert_eq!(reloaded.theme.as_deref(), Some("light"));
        assert_eq!(reloaded.lang.as_deref(), Some("ar"));
        assert_eq!(reloaded.font_size, Some(16));
        assert_eq!(reloaded.sidebar_open, Some(false));
        assert!(resolve_default_startup_working_dir(&reloaded).is_some());

        let fake_session_id = "test_delete_session_123";
        let spath = crate::session::session_path(fake_session_id).unwrap();
        std::fs::create_dir_all(spath.parent().unwrap()).unwrap();
        std::fs::write(&spath, "{}").unwrap();
        assert!(spath.exists());
        let del_resp = bridge.delete_stored_chat(fake_session_id).await.unwrap();
        assert_eq!(del_resp["ok"], true);
        assert_eq!(del_resp["deleted"], true);
        assert!(!spath.exists());

        let mut sess = crate::session::Session::create(None, Some("Thinking Test".into()));
        sess.add_message(
            crate::message::Role::User,
            vec![crate::message::ContentBlock::Text {
                text: "Solve 2+2".into(),
                cache_control: None,
            }],
        );
        sess.add_message(
            crate::message::Role::Assistant,
            vec![
                crate::message::ContentBlock::ReasoningTrace {
                    text: "Step 1: add 2 and 2".into(),
                },
                crate::message::ContentBlock::Text {
                    text: "4".into(),
                    cache_control: None,
                },
            ],
        );
        let transcript = build_stored_transcript_json(&sess);
        let msgs = transcript["messages"].as_array().unwrap();
        let last = msgs.last().unwrap();
        assert_eq!(last["text"], "4");
        assert_eq!(last["thinking"], "Step 1: add 2 and 2");

        sess.save().unwrap();
        let sess_id = sess.id.clone();
        bridge
            .submit_rename(Some(&sess_id), Some("Smart Renamed Chat"))
            .await
            .unwrap();
        bridge
            .submit_archive(Some(&sess_id), None, true)
            .await
            .unwrap();
        let reloaded_sess = crate::session::Session::load(&sess_id).unwrap();
        assert_eq!(reloaded_sess.display_title_or_name(), "Smart Renamed Chat");
        assert!(reloaded_sess.saved);
        let _ = bridge.delete_stored_chat(&sess_id).await;

        if let Some(prev) = prev {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", prev);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }

    #[tokio::test]
    async fn owned_turns_rotate_reply_immediately_and_match_resumed_session_id() {
        let temp_ws = tempfile::tempdir().unwrap();
        let mut bridge = Bridge::default();
        let (prompt_tx, mut prompt_rx) = mpsc::channel(4);
        bridge.prompt_tx = Some(prompt_tx);
        {
            let mut state = bridge.owned.lock().unwrap();
            state.session_id = Some("resumed-session-uuid".to_string());
            state.working_dir = Some(temp_ws.path().to_string_lossy().into_owned());
            state.reply = "First turn answer".to_string();
            state.transcript = "You\nFirst prompt\n\nAlphaCode\n".to_string();
        }
        assert!(bridge.is_owned_selected(Some(OWN_CHAT)));
        assert!(bridge.is_owned_selected(Some("resumed-session-uuid")));
        assert!(!bridge.is_owned_selected(Some("other-session-uuid")));

        let responder = tokio::spawn(async move {
            if let Some(input) = prompt_rx.recv().await {
                assert_eq!(input.content, "Second prompt");
                let _ = input.accepted.send(Ok(()));
            }
        });

        bridge
            .submit("resumed-session-uuid", "Second prompt")
            .await
            .unwrap();
        responder.await.unwrap();

        let state = bridge.owned.lock().unwrap();
        assert!(state.busy);
        assert!(state.reply.is_empty());
        assert_eq!(state.turn_seq, 1);
        assert!(state.prompt_pre_recorded);
        assert!(state.transcript.contains("First turn answer"));
        assert!(
            state
                .transcript
                .ends_with("You\nSecond prompt\n\nAlphaCode\n")
        );
        assert_eq!(display_text_tail("abcdef", 3), "def");
    }

    #[tokio::test]
    async fn clean_startup_never_auto_inserts_home_or_install_dir_and_defaults_dark_theme() {
        let _lock = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let prev_home = std::env::var_os("ALPHACODE_HOME");
        let prev_cwd = std::env::current_dir().unwrap();
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        // Move cwd to USERPROFILE / HOME if available
        let home_dir = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        if let Some(ref home) = home_dir {
            let _ = std::env::set_current_dir(home);
        }

        let bridge = Bridge::with_saved_state();
        assert!(bridge.gui_state.workspaces.is_empty());
        assert_eq!(bridge.gui_state.theme.as_deref(), Some("dark"));
        if home_dir.is_some() {
            assert!(bridge.owned.lock().unwrap().working_dir.is_none());
        }
        let state_resp = bridge.gui_state_get();
        assert_eq!(state_resp["ok"], true);
        assert_eq!(state_resp["state"]["theme"], "dark");
        assert_eq!(state_resp["state"]["workspaces"], json!([]));

        let _ = std::env::set_current_dir(prev_cwd);
        if let Some(prev) = prev_home {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", prev);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }

    #[tokio::test]
    async fn workspace_actions_and_new_chat_turn_isolation_work_with_sqlite() {
        let _lock = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let prev_home = std::env::var_os("ALPHACODE_HOME");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        let ws_a = temp.path().join("project_a");
        let ws_b = temp.path().join("project_b");
        std::fs::create_dir_all(&ws_a).unwrap();
        std::fs::create_dir_all(&ws_b).unwrap();
        let ws_a_str = normalize_existing_dir(&ws_a.to_string_lossy()).unwrap();
        let ws_b_str = normalize_existing_dir(&ws_b.to_string_lossy()).unwrap();

        let mut bridge = Bridge::with_saved_state();

        // Add workspaces
        let add_a = bridge.workspace_add(&BridgeRequest {
            action: "workspace_add".into(),
            working_dir: Some(ws_a_str.clone()),
            ..BridgeRequest::default()
        });
        assert_eq!(add_a["ok"], true);
        let add_b = bridge.workspace_add(&BridgeRequest {
            action: "workspace_add".into(),
            working_dir: Some(ws_b_str.clone()),
            ..BridgeRequest::default()
        });
        assert_eq!(add_b["ok"], true);

        // Select workspace_a without creating or resetting a chat
        {
            let mut owned = bridge.owned.lock().unwrap();
            owned.session_id = Some("existing-sess".into());
            owned.transcript = "keep me".into();
        }
        let sel = bridge.workspace_select(&BridgeRequest {
            action: "workspace_select".into(),
            working_dir: Some(ws_a_str.clone()),
            ..BridgeRequest::default()
        });
        assert_eq!(sel["ok"], true);
        assert_eq!(sel["active_workspace"], ws_a_str);
        assert_eq!(
            bridge.owned.lock().unwrap().session_id.as_deref(),
            Some("existing-sess")
        );
        assert_eq!(bridge.owned.lock().unwrap().transcript, "keep me");

        // Pin & collapse
        let pin = bridge.workspace_pin(&BridgeRequest {
            action: "workspace_pin".into(),
            working_dir: Some(ws_a_str.clone()),
            pinned: Some(true),
            ..BridgeRequest::default()
        });
        assert_eq!(pin["ok"], true);
        assert_eq!(pin["pinned"], true);

        let col = bridge.workspace_collapse(&BridgeRequest {
            action: "workspace_collapse".into(),
            working_dir: Some(ws_b_str.clone()),
            collapsed: Some(true),
            ..BridgeRequest::default()
        });
        assert_eq!(col["ok"], true);
        assert_eq!(col["collapsed"], true);

        // Remove workspace_b
        let rem = bridge.workspace_remove(&BridgeRequest {
            action: "workspace_remove".into(),
            working_dir: Some(ws_b_str.clone()),
            ..BridgeRequest::default()
        });
        assert_eq!(rem["ok"], true);
        assert_eq!(rem["removed"], true);
        assert!(!bridge.gui_state.workspaces.iter().any(|w| w == &ws_b_str));

        // new_chat returns __companion__ and preserves active workspace when working_dir is None
        let nc = bridge.submit_new_chat(None).await.unwrap();
        assert_eq!(nc["ok"], true);
        assert_eq!(nc["action"], "new_chat");
        assert_eq!(nc["new_chat"], true);
        assert_eq!(nc["session_id"], OWN_CHAT);
        assert_eq!(nc["working_dir"], ws_a_str);

        // Turn isolation: new_chat -> submit -> reply -> next submit -> snapshot live_reply
        let (prompt_tx, mut prompt_rx) = mpsc::channel(4);
        bridge.prompt_tx = Some(prompt_tx);
        let owned_ref = Arc::clone(&bridge.owned);
        let responder = tokio::spawn(async move {
            let p1 = prompt_rx.recv().await.unwrap();
            assert_eq!(p1.content, "Turn 1");
            let _ = p1.accepted.send(Ok(()));
            {
                let mut g = owned_ref.lock().unwrap();
                g.reply = "Answer 1".into();
                g.busy = false;
            }
            let p2 = prompt_rx.recv().await.unwrap();
            assert_eq!(p2.content, "Turn 2");
            let _ = p2.accepted.send(Ok(()));
        });

        bridge.submit(OWN_CHAT, "Turn 1").await.unwrap();
        tokio::task::yield_now().await;
        while bridge.owned.lock().unwrap().busy {
            tokio::task::yield_now().await;
        }
        let snap1 = bridge.snapshot(Some(OWN_CHAT)).await;
        assert_eq!(snap1["live_reply"], "Answer 1");

        bridge.submit(OWN_CHAT, "Turn 2").await.unwrap();
        let snap2 = bridge.snapshot(Some(OWN_CHAT)).await;
        // live_reply must be current turn only (empty before Turn 2 streams), never "Answer 1"
        assert_eq!(snap2["live_reply"], "");
        assert!(snap2["reply"].as_str().unwrap().contains("Answer 1"));
        responder.await.unwrap();

        if let Some(prev) = prev_home {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", prev);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }

    #[tokio::test]
    async fn submit_requires_open_workspace_folder_and_new_chat_keeps_none_without_workspace() {
        let mut bridge = Bridge::default();
        // No workspace opened yet -> new_chat(None) keeps working_dir as None
        let nc = bridge.submit_new_chat(None).await.unwrap();
        assert!(nc["working_dir"].is_null());
        assert!(bridge.owned.lock().unwrap().working_dir.is_none());

        // submit without open workspace fails with exact message
        let err_none = bridge.submit(OWN_CHAT, "Fix bug").await.unwrap_err();
        assert!(
            err_none
                .to_string()
                .contains("Open a project folder first before sending a message"),
            "unexpected error: {err_none:#}"
        );

        // Non-existent directory also rejected
        bridge.owned.lock().unwrap().working_dir = Some("Z:/does_not_exist_12345".into());
        let err_missing = bridge.submit(OWN_CHAT, "Fix bug").await.unwrap_err();
        assert!(
            err_missing
                .to_string()
                .contains("Open a project folder first before sending a message"),
            "unexpected error: {err_missing:#}"
        );

        // ensure_owned_chat and new_chat without a valid workspace must not spawn a daemon session
        bridge.owned.lock().unwrap().working_dir = None;
        bridge.ensure_owned_chat();
        assert!(bridge.prompt_tx.is_none());
        let nc_resp = bridge
            .handle_bridge_request(BridgeRequest {
                action: "new_chat".into(),
                ..BridgeRequest::default()
            })
            .await;
        assert_eq!(nc_resp["ok"], true);
        assert!(nc_resp["working_dir"].is_null());
        assert!(bridge.prompt_tx.is_none());
        assert!(connect_owned_chat(None, None).await.is_err());
    }

    #[tokio::test]
    async fn transcript_loads_stored_session_fast_from_disk_and_session_renamed_syncs_db() {
        let _lock = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let prev_home = std::env::var_os("ALPHACODE_HOME");
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", temp.path());

        let ws = temp.path().join("demo_ws");
        std::fs::create_dir_all(&ws).unwrap();
        let ws_norm = db::normalize_workspace_path(&ws.to_string_lossy()).unwrap();

        let mut sess =
            crate::session::Session::create_with_id("sess_fast_load_1".to_string(), None, None);
        sess.working_dir = Some(ws_norm.clone());
        sess.add_message(
            crate::message::Role::User,
            vec![crate::message::ContentBlock::Text {
                text: "Explain WAL mode".into(),
                cache_control: None,
            }],
        );
        sess.add_message(
            crate::message::Role::Assistant,
            vec![crate::message::ContentBlock::Text {
                text: "WAL keeps readers non-blocking.".into(),
                cache_control: None,
            }],
        );
        sess.save().unwrap();

        // Even before `sessions` or `resume_chat`, `transcript` loads directly from disk
        let mut bridge = Bridge::default();
        let tr = bridge.transcript(Some("sess_fast_load_1")).await;
        assert_eq!(tr["ok"], true);
        assert_eq!(tr["transcript"]["message_count"], 2);
        assert_eq!(
            tr["transcript"]["messages"][1]["text"],
            "WAL keeps readers non-blocking."
        );

        // SessionRenamed updates owned.title, snapshot sessions[0].title, and alphacode.db
        {
            let mut owned = bridge.owned.lock().unwrap();
            owned.session_id = Some("sess_fast_load_1".into());
        }
        apply_owned_server_event(
            &bridge.owned,
            ServerEvent::SessionRenamed {
                session_id: "sess_fast_load_1".into(),
                title: None,
                display_title: "SQLite WAL Mode".into(),
            },
        );
        assert_eq!(
            bridge.owned.lock().unwrap().title.as_deref(),
            Some("SQLite WAL Mode")
        );
        let snap = bridge.snapshot(Some(OWN_CHAT)).await;
        assert_eq!(snap["sessions"][0]["title"], "SQLite WAL Mode");

        let db = db::GuiDb::open_default().unwrap();
        let row = db.get_chat("sess_fast_load_1").unwrap().unwrap();
        assert_eq!(row.title.as_deref(), Some("SQLite WAL Mode"));
        assert_eq!(row.workspace_path.as_deref(), Some(ws_norm.as_str()));

        if let Some(prev) = prev_home {
            crate::alphacode_core::env::set_var("ALPHACODE_HOME", prev);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_HOME");
        }
    }
}
