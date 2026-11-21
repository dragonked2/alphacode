//! Server-initiated ("wake") turns for live sessions.
//!
//! Several server paths start a full conversation turn in a session without
//! that session's client sending a message: swarm DM/broadcast wake delivery,
//! background-task completion wakes, scheduled-task delivery, and post-reload
//! resume. Those turns must keep the same bookkeeping as client-initiated
//! turns, otherwise the swarm member status stays "ready/idle" while the agent
//! is actually streaming and attached TUIs never learn the turn finished.
//!
//! This module is the single shared implementation: it marks the member
//! `running` while the turn streams, flips it back to `ready` (with a
//! completion report) or `failed` at the end, and fans out a terminal
//! event so attached clients can settle the externally started turn in their UI.

use super::client_lifecycle::{TurnEventRelay, turn_terminal_event};
use super::{
    SwarmEvent, SwarmMember, truncate_detail, update_member_status,
    update_member_status_with_report,
};
use crate::alphacode_app_core::agent::Agent;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Mutex, RwLock, broadcast};

type SessionAgents = Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>;

/// Swarm bookkeeping handles needed to keep member status accurate around a
/// server-initiated turn.
#[derive(Clone)]
pub(super) struct LiveTurnSwarmContext {
    pub members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    pub swarms_by_id: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    pub event_history: Arc<RwLock<VecDeque<SwarmEvent>>>,
    pub event_counter: Arc<AtomicU64>,
    pub event_tx: broadcast::Sender<SwarmEvent>,
}

impl LiveTurnSwarmContext {
    pub(super) fn new(
        members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
        swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
        event_history: &Arc<RwLock<VecDeque<SwarmEvent>>>,
        event_counter: &Arc<AtomicU64>,
        event_tx: &broadcast::Sender<SwarmEvent>,
    ) -> Self {
        Self {
            members: Arc::clone(members),
            swarms_by_id: Arc::clone(swarms_by_id),
            event_history: Arc::clone(event_history),
            event_counter: Arc::clone(event_counter),
            event_tx: event_tx.clone(),
        }
    }
}

/// Return the live agent for `session_id` when the session has at least one
/// live client attachment and its agent is currently idle (lock not held).
pub(super) async fn idle_live_agent(
    session_id: &str,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<Arc<Mutex<Agent>>> {
    let agent = {
        let guard = sessions.read().await;
        guard.get(session_id).cloned()
    }?;

    let has_live_attachments = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .map(|member| !member.event_txs.is_empty() || !member.event_tx.is_closed())
            .unwrap_or(false)
    };
    if !has_live_attachments {
        return None;
    }

    let is_idle = agent.try_lock().is_ok();
    is_idle.then_some(agent)
}

/// Spawn `message` as a full tracked turn in a live session.
///
/// Update member status and publish one terminal event while the turn owns
/// the agent lock, so the next turn cannot race the previous turn's output.
pub(super) fn spawn_tracked_live_turn(
    session_id: &str,
    agent: Arc<Mutex<Agent>>,
    message: String,
    system_reminder: Option<String>,
    status_detail: Option<String>,
    swarm: LiveTurnSwarmContext,
) {
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let mut agent = agent.lock().await;
        update_member_status(
            &session_id,
            "running",
            status_detail,
            &swarm.members,
            &swarm.swarms_by_id,
            Some(&swarm.event_history),
            Some(&swarm.event_counter),
            Some(&swarm.event_tx),
        )
        .await;
        let relay = TurnEventRelay::new(&swarm.members, &session_id, None);
        let start_message_index = agent.message_count();
        let generation_id = uuid::Uuid::new_v4();
        crate::logging::info(&format!(
            "Starting background generation {generation_id} session={session_id}"
        ));
        let result = relay
            .run(&mut agent, &message, Vec::new(), system_reminder)
            .await;
        let (outcome, terminal_event) = turn_terminal_event(0, &result);
        match result {
            Ok(()) => {
                let completion_report = agent.latest_assistant_text_after(start_message_index);
                update_member_status_with_report(
                    &session_id,
                    "ready",
                    None,
                    completion_report,
                    &swarm.members,
                    &swarm.swarms_by_id,
                    Some(&swarm.event_history),
                    Some(&swarm.event_counter),
                    Some(&swarm.event_tx),
                )
                .await;
            }
            Err(error) => {
                let status = if error.is::<crate::agent::TurnInterrupted>() {
                    "stopped"
                } else {
                    "failed"
                };
                update_member_status(
                    &session_id,
                    status,
                    Some(truncate_detail(&error.to_string(), 120)),
                    &swarm.members,
                    &swarm.swarms_by_id,
                    Some(&swarm.event_history),
                    Some(&swarm.event_counter),
                    Some(&swarm.event_tx),
                )
                .await;
            }
        }
        crate::logging::info(&format!(
            "Finished background generation {generation_id} session={session_id} outcome={outcome}"
        ));
        relay.send(terminal_event).await;
    });
}

/// Run `message` immediately as a tracked turn if the session is live and
/// idle. Returns `true` when the turn was started.
pub(super) async fn run_live_turn_if_idle(
    session_id: &str,
    message: &str,
    system_reminder: Option<String>,
    sessions: &SessionAgents,
    swarm: LiveTurnSwarmContext,
) -> bool {
    let Some(agent) = idle_live_agent(session_id, sessions, &swarm.members).await else {
        return false;
    };
    let detail = Some(truncate_detail(message, 120)).filter(|detail| !detail.is_empty());
    spawn_tracked_live_turn(
        session_id,
        agent,
        message.to_string(),
        system_reminder,
        detail,
        swarm,
    );
    true
}
