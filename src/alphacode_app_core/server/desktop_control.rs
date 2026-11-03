//! Desktop operations over the regular local transport. They do not attach a
//! second terminal client or alter the terminal composer's unsent draft.

use super::{ClientConnectionInfo, SessionAgents, SessionInterruptQueues, SwarmEvent, SwarmMember};
use crate::alphacode_agent_runtime::{InterruptSignal, SoftInterruptSource};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, atomic::AtomicU64};
use tokio::sync::{RwLock, broadcast};

/// Desktop pickers offer only routes that can be used with configured auth.
/// Route-less single-provider runtimes already own their authenticated catalog.
fn desktop_model_options(
    catalog: crate::alphacode_provider_core::ModelCatalogSnapshot,
) -> Vec<String> {
    let allowed: HashSet<&str> = catalog
        .model_routes
        .iter()
        .filter(|route| route.available)
        .map(|route| route.model.as_str())
        .collect();
    let mut seen = HashSet::new();
    catalog
        .available_models
        .into_iter()
        .filter(|model| {
            !model.trim().is_empty()
                && (catalog.model_routes.is_empty() || allowed.contains(model.as_str()))
                && seen.insert(model.clone())
        })
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "desktop control shares existing server state"
)]
pub(super) async fn execute(
    action: &str,
    session_id: Option<&str>,
    value: Option<&str>,
    sessions: &SessionAgents,
    connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    plans: &Arc<RwLock<HashMap<String, super::VersionedPlan>>>,
    history: &Arc<RwLock<VecDeque<SwarmEvent>>>,
    counter: &Arc<AtomicU64>,
    events: &broadcast::Sender<SwarmEvent>,
    queues: &SessionInterruptQueues,
    signals: &Arc<RwLock<HashMap<String, InterruptSignal>>>,
) -> Result<Value> {
    if action == "sessions" {
        let connected: Vec<_> = connections.read().await.values().cloned().collect();
        // Snapshot each map independently: session transitions acquire these
        // locks in other orders and must not be blocked by a desktop poll.
        let agents = sessions.read().await.clone();
        let members = members.read().await.clone();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        for connection in connected {
            let id = connection.session_id;
            if !seen.insert(id.clone()) {
                continue;
            }
            let member = members.get(&id);
            let agent = agents.get(&id).and_then(|agent| agent.try_lock().ok());
            let (saved, save_label) = agent
                .as_ref()
                .map(|agent| agent.session_bookmark())
                .unwrap_or_default();
            let working_dir = agent
                .as_ref()
                .and_then(|agent| agent.working_dir().map(str::to_string))
                .or_else(|| {
                    member.and_then(|member| {
                        member
                            .working_dir
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned())
                    })
                });
            rows.push(json!({
                "session_id": id,
                "friendly_name": agent.as_ref().map(|agent| agent.session_display_title()).or_else(|| member.and_then(|member| member.friendly_name.clone())),
                "is_processing": connection.is_processing || member.is_some_and(|member| member.status == "running"),
                "status": member.map(|member| member.status.clone()),
                "detail": member.and_then(|member| member.detail.clone()),
                "working_dir": working_dir,
                "saved": saved,
                "save_label": save_label,
            }));
        }
        return Ok(json!(rows));
    }

    let id = session_id
        .filter(|id| !id.is_empty())
        .context("Choose a live chat first")?;
    let connected = connections
        .read()
        .await
        .values()
        .any(|connection| connection.session_id == id);
    if !connected {
        bail!("This chat is no longer connected")
    }
    // Read-only work snapshots must remain available while the agent is busy.
    // Snapshot each map independently; never await disk I/O under a map lock.
    if action == "workflow" {
        let (swarm_id, swarm) = {
            let members = members.read().await;
            let swarm_id = members.get(id).and_then(|member| member.swarm_id.clone());
            let mut rows: Vec<_> = members.values()
                .filter(|member| member.session_id == id || swarm_id.as_ref().is_some_and(|swarm| member.swarm_id.as_ref() == Some(swarm)))
                .map(|member| json!({"session_id": member.session_id, "name": member.friendly_name.as_deref().unwrap_or(&member.session_id), "status": member.status, "role": member.role, "detail": member.detail}))
                .collect();
            rows.sort_by(|a, b| a["session_id"].as_str().cmp(&b["session_id"].as_str()));
            rows.truncate(200);
            (swarm_id, rows)
        };
        let plan = {
            let plans = plans.read().await;
            swarm_id.as_ref().and_then(|id| plans.get(id)).cloned()
        };
        let session_id = id.to_string();
        let (todos, intention) = tokio::task::spawn_blocking(move || {
            let todos = crate::todo::load_todos(&session_id)?;
            let intention = crate::todo::load_plan(&session_id)?.user_intention;
            Ok::<_, anyhow::Error>((todos, intention))
        })
        .await??;
        let todo_count = todos.len();
        let completed = todos
            .iter()
            .filter(|todo| todo.status == "completed")
            .count();
        let todos: Vec<_> = todos
            .into_iter()
            .take(300)
            .map(|todo| {
                json!({
                    "id": todo.id, "content": crate::util::truncate_str(&todo.content, 2000),
                    "status": todo.status, "group": todo.group, "blocked_by": todo.blocked_by,
                    "assigned_to": todo.assigned_to,
                })
            })
            .collect();
        let swarm_plan = plan.map(|plan| {
            let graph = crate::plan::summarize_plan_graph(&plan.items);
            json!({"version": plan.version, "item_count": plan.items.len(),
                "items": plan.items.iter().take(300).map(|item| json!({"id": item.id, "content": crate::util::truncate_str(&item.content, 2000), "status": item.status, "assigned_to": item.assigned_to})).collect::<Vec<_>>(),
                "ready": graph.ready_ids.len(), "active": graph.active_ids.len(), "completed": graph.completed_ids.len(),
                "failed": graph.failed_ids.len(), "blocked": graph.blocked_ids.len(), "cycles": graph.cycle_ids.len(),
            })
        });
        return Ok(
            json!({"available": true, "session_id": id, "todos": todos, "todo_count": todo_count,
            "completed": completed, "intention": intention, "swarm": swarm, "swarm_plan": swarm_plan}),
        );
    }
    let agent = sessions
        .read()
        .await
        .get(id)
        .cloned()
        .context("This chat is no longer available")?;
    match action {
        "prompt" => {
            let prompt = value.unwrap_or("");
            if prompt.trim().is_empty() || prompt.len() > 64 * 1024 {
                bail!("Enter a prompt of up to 65,536 bytes")
            }
            let swarm = super::live_turn::LiveTurnSwarmContext::new(
                members, swarms, history, counter, events,
            );
            if let Some(agent) = super::live_turn::idle_live_agent(id, sessions, members).await {
                super::fanout_session_event(
                    members,
                    id,
                    crate::protocol::ServerEvent::ExternalUserPrompt {
                        session_id: id.to_string(),
                        content: prompt.to_string(),
                    },
                )
                .await;
                super::live_turn::spawn_tracked_live_turn(
                    id,
                    agent,
                    prompt.to_string(),
                    None,
                    Some(super::truncate_detail(prompt, 120)),
                    swarm,
                )
                .await;
                return Ok(json!({"sent": true, "queued": false}));
            }
            if super::queue_soft_interrupt_for_session(
                id,
                prompt.to_string(),
                false,
                SoftInterruptSource::User,
                queues,
                sessions,
            )
            .await
            {
                return Ok(json!({"sent": true, "queued": true}));
            }
            bail!("Could not deliver the prompt to this chat")
        }
        "cancel" => {
            let signal = signals
                .read()
                .await
                .get(id)
                .cloned()
                .context("This chat cannot be cancelled")?;
            signal.fire();
            for active in crate::turn_cancel_registry::active_turn_signals(id) {
                if !active.same_instance(&signal) {
                    active.fire();
                }
            }
            Ok(json!({"cancelled": true}))
        }
        "history" | "state" | "model" | "effort" | "rename" | "save" | "unsave" | "tools" => {
            let mut agent = agent
                .try_lock()
                .map_err(|_| anyhow::anyhow!("Chat is working; try again when it is ready"))?;
            match action {
                "history" => {
                    let messages = agent.get_history();
                    let count = messages.len();
                    let recent: Vec<_> = messages
                        .into_iter()
                        .skip(count.saturating_sub(40))
                        .map(|mut message| {
                            message.content =
                                crate::util::truncate_str(&message.content, 4_000).to_string();
                            if let Some(tools) = &mut message.tool_calls {
                                tools.truncate(6);
                            }
                            message.tool_data = None;
                            message
                        })
                        .collect();
                    Ok(json!({"message_count": count, "messages": recent}))
                }
                "state" => {
                    let (saved, label) = agent.session_bookmark();
                    Ok(
                        json!({"model": agent.provider_model(), "provider": agent.provider_name(), "models": desktop_model_options(agent.model_catalog_snapshot()), "effort": agent.current_reasoning_effort(), "title": agent.session_display_title(), "saved": saved, "save_label": label}),
                    )
                }
                "tools" => Ok(json!(agent.tool_names().await)),
                "model" => {
                    let model = value
                        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
                        .context("Choose a model")?;
                    agent.set_model(model)?;
                    Ok(json!({"model": agent.provider_model(), "provider": agent.provider_name()}))
                }
                "effort" => {
                    let effort = value
                        .filter(|value| value.len() <= 32)
                        .context("Choose a reasoning effort")?;
                    let effort = agent.set_reasoning_effort(effort)?;
                    Ok(json!({"effort": effort}))
                }
                "rename" => {
                    let title = value.map(str::trim).filter(|title| !title.is_empty());
                    if title.is_some_and(|title| title.chars().count() > 120) {
                        bail!("Chat titles must be 120 characters or fewer")
                    }
                    let display_title = agent.rename_session_title(title.map(str::to_string))?;
                    crate::session_list_cache::invalidate();
                    drop(agent);
                    super::fanout_session_event(
                        members,
                        id,
                        crate::protocol::ServerEvent::SessionRenamed {
                            session_id: id.to_string(),
                            title: title.map(str::to_string),
                            display_title: display_title.clone(),
                        },
                    )
                    .await;
                    Ok(json!({"display_title": display_title}))
                }
                "save" | "unsave" => {
                    let saved = action == "save";
                    let label = value.map(str::trim).filter(|label| !label.is_empty());
                    if label.is_some_and(|label| label.chars().count() > 60) {
                        bail!("Saved-chat labels must be 60 characters or fewer")
                    }
                    agent.set_session_bookmark(saved, label.map(str::to_string))?;
                    let (_, label) = agent.session_bookmark();
                    crate::session_list_cache::invalidate();
                    drop(agent);
                    super::fanout_session_event(
                        members,
                        id,
                        crate::protocol::ServerEvent::SessionBookmark {
                            session_id: id.to_string(),
                            saved,
                            label: label.clone(),
                        },
                    )
                    .await;
                    Ok(json!({"saved": saved, "label": label}))
                }
                _ => unreachable!(),
            }
        }
        _ => bail!("Unknown desktop action"),
    }
}

#[cfg(test)]
mod tests {
    use super::desktop_model_options;
    use crate::alphacode_provider_core::{ModelCatalogSnapshot, ModelRoute};

    #[test]
    fn desktop_models_require_an_available_route_without_hiding_shared_models() {
        let route = |model: &str, available| ModelRoute {
            model: model.into(),
            provider: "test".into(),
            api_method: "test".into(),
            available,
            detail: String::new(),
            cheapness: None,
        };
        let catalog = ModelCatalogSnapshot::new(
            None,
            Some("logged-out-model".into()),
            [
                "logged-out-model",
                "shared",
                "local-model",
                "shared",
                "unknown",
                "",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            vec![
                route("logged-out-model", false),
                route("shared", false),
                route("shared", true),
                route("local-model", true),
            ],
        );
        assert_eq!(desktop_model_options(catalog), ["shared", "local-model"]);
    }

    #[test]
    fn desktop_models_do_not_fall_back_to_unavailable_catalogs() {
        let catalog = ModelCatalogSnapshot::new(
            None,
            None,
            vec!["unavailable".into()],
            vec![ModelRoute {
                model: "unavailable".into(),
                provider: "signed out".into(),
                api_method: "test".into(),
                available: false,
                detail: String::new(),
                cheapness: None,
            }],
        );
        assert!(desktop_model_options(catalog).is_empty());
    }

    #[test]
    fn desktop_models_preserve_route_less_runtime_and_local_model_names() {
        let catalog = ModelCatalogSnapshot::new(
            None,
            None,
            vec![
                "lmstudio/community-model".into(),
                "lmstudio/community-model".into(),
            ],
            vec![],
        );
        assert_eq!(desktop_model_options(catalog), ["lmstudio/community-model"]);
    }
}
