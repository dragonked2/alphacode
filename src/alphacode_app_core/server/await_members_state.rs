use crate::alphacode_app_core::protocol::{AwaitedMemberStatus, ServerEvent};
use crate::alphacode_app_core::server::durable_state::{
    hashed_request_key, load_json_state, now_unix_ms, save_json_state, state_dir,
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};

const AWAIT_MEMBERS_DIR: &str = "alphacode-await-members";
const FINAL_STATE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const PENDING_STATE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

static STATE_LOCKS: LazyLock<StdMutex<HashMap<std::path::PathBuf, Weak<StdMutex<()>>>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

fn state_lock(key: &str) -> Arc<StdMutex<()>> {
    let path = super::durable_state::state_path(AWAIT_MEMBERS_DIR, key);
    let mut locks = STATE_LOCKS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(&path).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(StdMutex::new(()));
    locks.insert(path, Arc::downgrade(&lock));
    lock
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedAwaitMembersResult {
    pub completed: bool,
    pub members: Vec<AwaitedMemberStatus>,
    pub summary: String,
    pub resolved_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedAwaitMembersState {
    pub key: String,
    pub session_id: String,
    pub swarm_id: String,
    pub target_status: Vec<String>,
    pub requested_ids: Vec<String>,
    /// Persist the observed cohort separately from the request. Empty
    /// requested_ids still means dynamic whole-swarm watching, including joins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_members: Vec<AwaitedMemberStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub created_at_unix_ms: u64,
    pub deadline_unix_ms: u64,
    /// When true, the wait runs as a detached background watcher that delivers
    /// its result via notify/wake instead of blocking the requesting turn.
    /// Background watchers are auto-resumed at server startup after a reload.
    #[serde(default)]
    pub background: bool,
    /// Surface a completion notification card to attached clients.
    #[serde(default = "default_true")]
    pub notify: bool,
    /// Wake an idle requesting agent on completion (or soft-interrupt if busy).
    #[serde(default = "default_true")]
    pub wake: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_response: Option<PersistedAwaitMembersResult>,
}

fn default_true() -> bool {
    true
}

impl PersistedAwaitMembersState {
    pub fn is_pending(&self) -> bool {
        self.final_response.is_none()
    }

    pub fn remaining_timeout(&self) -> Duration {
        let now = now_unix_ms();
        Duration::from_millis(self.deadline_unix_ms.saturating_sub(now))
    }
}

#[derive(Clone)]
struct AwaitMembersWaiter {
    request_id: u64,
    client_event_tx: mpsc::UnboundedSender<ServerEvent>,
}

#[derive(Clone, Default)]
pub(crate) struct AwaitMembersRuntime {
    active_keys: Arc<RwLock<HashSet<String>>>,
    waiters: Arc<RwLock<HashMap<String, Vec<AwaitMembersWaiter>>>>,
}

impl AwaitMembersRuntime {
    pub(super) async fn add_waiter(
        &self,
        key: &str,
        request_id: u64,
        client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    ) {
        let mut waiters = self.waiters.write().await;
        waiters
            .entry(key.to_string())
            .or_default()
            .push(AwaitMembersWaiter {
                request_id,
                client_event_tx: client_event_tx.clone(),
            });
    }

    pub(super) async fn mark_active_if_new(&self, key: &str) -> bool {
        let mut active = self.active_keys.write().await;
        active.insert(key.to_string())
    }

    pub(super) async fn clear_active(&self, key: &str) {
        self.active_keys.write().await.remove(key);
    }

    pub(super) async fn retain_open_waiters(&self, key: &str) -> usize {
        let mut waiters = self.waiters.write().await;
        let Some(entries) = waiters.get_mut(key) else {
            return 0;
        };
        entries.retain(|waiter| !waiter.client_event_tx.is_closed());
        let remaining = entries.len();
        if remaining == 0 {
            waiters.remove(key);
        }
        remaining
    }

    pub(super) async fn take_waiters(
        &self,
        key: &str,
    ) -> Vec<(u64, mpsc::UnboundedSender<ServerEvent>)> {
        self.waiters
            .write()
            .await
            .remove(key)
            .unwrap_or_default()
            .into_iter()
            .map(|waiter| (waiter.request_id, waiter.client_event_tx))
            .collect()
    }
}

fn is_stale(state: &PersistedAwaitMembersState) -> bool {
    let now = now_unix_ms();
    if let Some(final_response) = &state.final_response {
        now.saturating_sub(final_response.resolved_at_unix_ms) > FINAL_STATE_TTL.as_millis() as u64
    } else {
        now.saturating_sub(state.deadline_unix_ms) > PENDING_STATE_TTL.as_millis() as u64
    }
}

pub(super) fn request_key(
    session_id: &str,
    swarm_id: &str,
    requested_ids: &[String],
    target_status: &[String],
    mode: Option<&str>,
) -> String {
    let mut requested = requested_ids.to_vec();
    requested.sort();

    let mut target = target_status.to_vec();
    target.sort();

    hashed_request_key(
        session_id,
        "await_members",
        &[
            swarm_id.to_string(),
            requested.join("\u{1f}"),
            target.join("\u{1f}"),
            mode.unwrap_or("all").to_string(),
        ],
    )
}

pub(super) fn load_state(key: &str) -> Option<PersistedAwaitMembersState> {
    let lock = state_lock(key);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    load_state_unlocked(key)
}

fn load_state_unlocked(key: &str) -> Option<PersistedAwaitMembersState> {
    load_json_state(AWAIT_MEMBERS_DIR, key, is_stale)
}

pub(super) fn save_state(state: &PersistedAwaitMembersState) {
    let lock = state_lock(&state.key);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    let mut next = state.clone();
    if let Some(current) = load_state_unlocked(&state.key) {
        if current.created_at_unix_ms != state.created_at_unix_ms {
            crate::logging::warn("Ignoring stale await-state preference update");
            return;
        }
        // Generic saves update preferences/final response; observation writes
        // use record_observations, which knows which payload is newest.
        next.observed_members = merge_observed_members(
            &state.observed_members,
            &current.observed_members,
            &state.target_status,
        );
        if current.final_response.is_some() {
            // A preference update captured before finalization cannot reopen
            // a request whose outcome has already committed.
            next.final_response = current.final_response;
        }
    }
    save_state_unlocked(&next);
}

fn save_state_unlocked(state: &PersistedAwaitMembersState) {
    save_json_state(AWAIT_MEMBERS_DIR, &state.key, state, "await_members state");
}

/// Keep every observed member. Missing unfinished work becomes unknown;
/// previously observed matching outcomes remain historical evidence until a
/// new live observation supersedes them.
pub(super) fn merge_observed_members(
    previous: &[AwaitedMemberStatus],
    current: &[AwaitedMemberStatus],
    target: &[String],
) -> Vec<AwaitedMemberStatus> {
    let mut members: HashMap<String, AwaitedMemberStatus> = previous
        .iter()
        .map(|member| {
            let mut fallback = member.clone();
            if !fallback.done {
                fallback.status = "unknown".into();
                fallback.done = target.iter().any(|status| status == "unknown");
            }
            (fallback.session_id.clone(), fallback)
        })
        .collect();
    for member in current {
        if member.status == "unknown"
            && members
                .get(&member.session_id)
                .is_some_and(|previous| previous.done)
        {
            continue;
        }
        members.insert(member.session_id.clone(), member.clone());
    }
    let mut members: Vec<_> = members.into_values().collect();
    members.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    members
}

/// Merge observations into the latest pending record, preserving delivery
/// preferences changed by another request while a watcher was running.
pub(super) fn record_observations(
    state: &PersistedAwaitMembersState,
    observations: &[AwaitedMemberStatus],
) -> anyhow::Result<PersistedAwaitMembersState> {
    let lock = state_lock(&state.key);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    let current = load_state_unlocked(&state.key);
    if let Some(current) = &current {
        anyhow::ensure!(
            current.created_at_unix_ms == state.created_at_unix_ms,
            "Await request changed while its watcher was running"
        );
    }
    let missing = current.is_none();
    let mut next = current.unwrap_or_else(|| state.clone());
    if next.is_pending() {
        let observed =
            merge_observed_members(&next.observed_members, observations, &next.target_status);
        if missing || observed != next.observed_members {
            next.observed_members = observed;
            let path = super::durable_state::state_path(AWAIT_MEMBERS_DIR, &next.key);
            crate::storage::write_json_fast(&path, &next)
                .context("Unable to persist swarm member observations")?;
        }
    }
    Ok(next)
}

#[expect(
    clippy::too_many_arguments,
    reason = "pending await state mirrors persisted fields and existing call sites"
)]
pub(super) fn ensure_pending_state(
    key: &str,
    session_id: &str,
    swarm_id: &str,
    requested_ids: &[String],
    target_status: &[String],
    mode: Option<&str>,
    deadline_unix_ms: u64,
    background: bool,
    notify: bool,
    wake: bool,
) -> PersistedAwaitMembersState {
    let lock = state_lock(key);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(existing) = load_state_unlocked(key).filter(PersistedAwaitMembersState::is_pending)
    {
        return existing;
    }

    let state = PersistedAwaitMembersState {
        key: key.to_string(),
        session_id: session_id.to_string(),
        swarm_id: swarm_id.to_string(),
        target_status: target_status.to_vec(),
        requested_ids: requested_ids.to_vec(),
        observed_members: Vec::new(),
        mode: mode.map(str::to_string),
        created_at_unix_ms: now_unix_ms(),
        deadline_unix_ms,
        background,
        notify,
        wake,
        final_response: None,
    };
    save_state_unlocked(&state);
    state
}

pub(super) fn persist_final_response(
    state: &PersistedAwaitMembersState,
    completed: bool,
    members: Vec<AwaitedMemberStatus>,
    summary: String,
) -> (PersistedAwaitMembersState, bool) {
    let lock = state_lock(&state.key);
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    let mut next = load_state_unlocked(&state.key)
        .filter(|current| current.created_at_unix_ms == state.created_at_unix_ms)
        .unwrap_or_else(|| state.clone());
    if next.final_response.is_some() {
        return (next, false);
    }
    next.observed_members =
        merge_observed_members(&next.observed_members, &members, &next.target_status);
    next.final_response = Some(PersistedAwaitMembersResult {
        completed,
        members,
        summary,
        resolved_at_unix_ms: now_unix_ms(),
    });
    save_state_unlocked(&next);
    (next, true)
}

pub fn pending_await_members_for_session(session_id: &str) -> Vec<PersistedAwaitMembersState> {
    let mut pending: Vec<PersistedAwaitMembersState> = all_pending_await_members()
        .into_iter()
        .filter(|state| state.session_id == session_id)
        .collect();
    pending.sort_by_key(|state| state.deadline_unix_ms);
    pending
}

/// Load every still-pending await state across all sessions, pruning stale
/// files as a side effect. Used both for per-session lookups and for resuming
/// backgrounded watchers after a server reload.
pub(super) fn all_pending_await_members() -> Vec<PersistedAwaitMembersState> {
    let now = now_unix_ms();
    all_pending_await_members_including_expired()
        .into_iter()
        .filter(|state| state.deadline_unix_ms > now)
        .collect()
}

/// Like [`all_pending_await_members`], but also returns pending states whose
/// deadline has already passed (still within the pending TTL). Startup resume
/// uses this so background awaits that expired while the server was down can
/// be finalized with a timeout instead of silently dropping the promised
/// notify/wake.
pub(super) fn all_pending_await_members_including_expired() -> Vec<PersistedAwaitMembersState> {
    let dir = state_dir(AWAIT_MEMBERS_DIR);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut pending = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Some(key) = path.file_stem().and_then(|key| key.to_str()) else {
            continue;
        };
        let lock = state_lock(key);
        let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());

        let Ok(state) = crate::storage::read_json::<PersistedAwaitMembersState>(&path) else {
            continue;
        };
        if state.key != key {
            continue;
        }
        if is_stale(&state) {
            let _ = std::fs::remove_file(path);
            continue;
        }
        if state.is_pending() {
            pending.push(state);
        }
    }

    pending
}
