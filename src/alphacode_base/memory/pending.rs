//! Session-local pending memory and delivery tracking.
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const PENDING_TTL: Duration = Duration::from_secs(120);
const KNOWN_TTL: Duration = Duration::from_secs(45 * 60);
const REPEAT_TTL: Duration = Duration::from_secs(90);
const MAX_TRACKED_IDS: usize = 4096;
const MAX_IDLE_SESSIONS: usize = 512;

#[derive(Debug, Clone)]
pub struct PendingMemory {
    pub prompt: String,
    pub display_prompt: Option<String>,
    pub computed_at: Instant,
    pub count: usize,
    pub memory_ids: Vec<String>,
}

impl PendingMemory {
    pub fn is_fresh(&self) -> bool {
        self.computed_at.elapsed() < PENDING_TTL
    }
}

#[derive(Default)]
struct SessionState {
    pending: Option<PendingMemory>,
    known: HashMap<String, Instant>,
    last: Option<(String, Vec<String>, Instant)>,
    running: Option<u64>,
}

#[derive(Default)]
struct State {
    sessions: HashMap<String, SessionState>,
    next_generation: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|error| error.into_inner())
}

fn prune(state: &mut State) {
    state.sessions.retain(|_, session| {
        session.known.retain(|_, at| at.elapsed() < KNOWN_TTL);
        if session.pending.as_ref().is_some_and(|p| !p.is_fresh()) {
            session.pending = None;
        }
        if session.last.as_ref().is_some_and(|(_, _, at)| at.elapsed() >= REPEAT_TTL) {
            session.last = None;
        }
        session.running.is_some() || session.pending.is_some()
            || !session.known.is_empty() || session.last.is_some()
    });
    // Only evict idle tracking, never an active check or a queued result.
    let mut idle: Vec<_> = state.sessions.iter()
        .filter(|(_, s)| s.running.is_none() && s.pending.is_none())
        .map(|(id, s)| {
            let newest = s.known.values().copied()
                .chain(s.last.as_ref().map(|(_, _, at)| *at)).max();
            (newest, id.clone())
        }).collect();
    if idle.len() > MAX_IDLE_SESSIONS {
        idle.sort_unstable();
        let excess = idle.len() - MAX_IDLE_SESSIONS;
        for (_, id) in idle.into_iter().take(excess) {
            state.sessions.remove(&id);
        }
    }
}

fn normalize_ids(ids: Vec<String>) -> Vec<String> {
    let mut ids = ids;
    ids.retain(|id| !id.is_empty());
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn record_known(session: &mut SessionState, ids: &[String]) {
    session.known.retain(|_, at| at.elapsed() < KNOWN_TTL);
    let now = Instant::now();
    for id in ids {
        if !id.is_empty() {
            session.known.insert(id.clone(), now);
        }
    }
    if session.known.len() > MAX_TRACKED_IDS {
        let mut oldest: Vec<_> = session.known.iter()
            .map(|(id, at)| (*at, id.clone())).collect();
        oldest.sort_unstable();
        let excess = oldest.len() - MAX_TRACKED_IDS;
        for (_, id) in oldest.into_iter().take(excess) {
            session.known.remove(&id);
        }
    }
}

fn queue(session: &mut SessionState, prompt: String, count: usize,
    memory_ids: Vec<String>, display_prompt: Option<String>) {
    if prompt.trim().is_empty() {
        return;
    }
    let memory_ids = normalize_ids(memory_ids);
    if session.pending.as_ref().is_some_and(|old| old.is_fresh()
        && old.prompt == prompt && old.memory_ids == memory_ids
        && old.display_prompt == display_prompt) {
        return;
    }
    session.pending = Some(PendingMemory {
        prompt, display_prompt, computed_at: Instant::now(),
        count: if memory_ids.is_empty() { count } else { memory_ids.len() },
        memory_ids,
    });
}

pub fn take_pending_memory(session_id: &str) -> Option<PendingMemory> {
    let mut guard = lock();
    let state = guard.get_or_insert_with(State::default);
    prune(state);
    let session = state.sessions.get_mut(session_id)?;
    let pending = session.pending.take()?;
    if !pending.is_fresh() {
        return None;
    }
    // Do not discard an updated payload merely because its ID set overlaps.
    if session.last.as_ref().is_some_and(|(text, ids, at)| {
        at.elapsed() < REPEAT_TTL && text == &pending.prompt && ids == &pending.memory_ids
    }) {
        return None;
    }
    session.last = Some((pending.prompt.clone(), pending.memory_ids.clone(), Instant::now()));
    record_known(session, &pending.memory_ids);
    Some(pending)
}

pub fn set_pending_memory(session_id: &str, prompt: String, count: usize) {
    set_pending_memory_with_ids(session_id, prompt, count, Vec::new());
}

pub fn set_pending_memory_with_ids(session_id: &str, prompt: String, count: usize, memory_ids: Vec<String>) {
    set_pending_memory_with_ids_and_display(session_id, prompt, count, memory_ids, None);
}

pub fn set_pending_memory_with_ids_and_display(session_id: &str, prompt: String, count: usize,
    memory_ids: Vec<String>, display_prompt: Option<String>) {
    let mut guard = lock();
    let state = guard.get_or_insert_with(State::default);
    prune(state);
    queue(state.sessions.entry(session_id.to_owned()).or_default(),
        prompt, count, memory_ids, display_prompt);
}

/// Queue only if the originating check still owns this session generation.
pub(crate) fn publish_memory_check(session_id: &str, generation: u64, prompt: String,
    count: usize, memory_ids: Vec<String>, display_prompt: Option<String>) -> bool {
    let mut guard = lock();
    let Some(session) = guard.as_mut().and_then(|s| s.sessions.get_mut(session_id)) else {
        return false;
    };
    if session.running != Some(generation) {
        return false;
    }
    queue(session, prompt, count, memory_ids, display_prompt);
    true
}

pub fn mark_memories_injected(session_id: &str, ids: &[String]) {
    if ids.is_empty() { return; }
    let mut guard = lock();
    let state = guard.get_or_insert_with(State::default);
    prune(state);
    record_known(state.sessions.entry(session_id.to_owned()).or_default(), ids);
}

pub fn mark_memories_known(session_id: &str, ids: &[String], _reason: &str) {
    mark_memories_injected(session_id, ids);
}

pub fn sync_injected_memories(session_id: &str, ids: &[String]) {
    let mut guard = lock();
    let state = guard.get_or_insert_with(State::default);
    prune(state);
    let session = state.sessions.entry(session_id.to_owned()).or_default();
    session.known.clear();
    session.last = None;
    record_known(session, ids);
}

pub fn is_memory_injected(session_id: &str, id: &str) -> bool {
    lock().as_ref().and_then(|s| s.sessions.get(session_id))
        .and_then(|s| s.known.get(id)).is_some_and(|at| at.elapsed() < KNOWN_TTL)
}

/// Legacy global query: do not use to filter a session-specific retrieval.
pub fn is_memory_injected_any(id: &str) -> bool {
    lock().as_ref().is_some_and(|s| s.sessions.values().any(|session|
        session.known.get(id).is_some_and(|at| at.elapsed() < KNOWN_TTL)))
}

pub fn clear_injected_memories(session_id: &str) {
    if let Some(session) = lock().as_mut().and_then(|s| s.sessions.get_mut(session_id)) {
        session.known.clear();
        session.last = None;
    }
}

pub fn clear_all_injected_memories() {
    if let Some(state) = lock().as_mut() {
        for session in state.sessions.values_mut() {
            session.known.clear();
            session.last = None;
        }
        prune(state);
    }
}

/// Reset also revokes the generation of any in-flight result.
pub fn clear_pending_memory(session_id: &str) {
    if let Some(state) = lock().as_mut() {
        state.sessions.remove(session_id);
    }
}

pub fn clear_all_pending_memory() {
    if let Some(state) = lock().as_mut() {
        state.sessions.clear();
        // Preserve generation counter across resets.
    }
}

pub fn has_pending_memory(session_id: &str) -> bool {
    lock().as_ref().and_then(|s| s.sessions.get(session_id))
        .and_then(|s| s.pending.as_ref()).is_some_and(PendingMemory::is_fresh)
}

pub fn has_any_pending_memory() -> bool {
    lock().as_ref().is_some_and(|s| s.sessions.values()
        .any(|session| session.pending.as_ref().is_some_and(PendingMemory::is_fresh)))
}

pub(crate) fn begin_memory_check_generation(session_id: &str) -> Option<u64> {
    let mut guard = lock();
    let state = guard.get_or_insert_with(State::default);
    prune(state);
    if state.sessions.get(session_id).is_some_and(|s| s.running.is_some()) {
        return None;
    }
    state.next_generation = state.next_generation.checked_add(1)?;
    let generation = state.next_generation;
    state.sessions.entry(session_id.to_owned()).or_default().running = Some(generation);
    Some(generation)
}

pub(crate) fn finish_memory_check_generation(session_id: &str, generation: u64) {
    if let Some(session) = lock().as_mut().and_then(|s| s.sessions.get_mut(session_id))
        && session.running == Some(generation) {
            session.running = None;
        }
}

/// Check ownership without reviving a revoked generation.
pub(crate) fn memory_check_generation_active(session_id: &str, generation: u64) -> bool {
    lock().as_ref().and_then(|state| state.sessions.get(session_id))
        .is_some_and(|session| session.running == Some(generation))
}

/// Mark extraction IDs as known only while the originating generation is live.
pub(crate) fn mark_memories_known_for_generation(session_id: &str, generation: u64,
    ids: &[String]) -> bool {
    let mut guard = lock();
    let Some(session) = guard.as_mut().and_then(|state| state.sessions.get_mut(session_id)) else {
        return false;
    };
    if session.running != Some(generation) { return false; }
    record_known(session, ids);
    true
}

// Retained for older callers; the generation-aware pair is preferred.
#[allow(dead_code)]
pub(super) fn begin_memory_check(session_id: &str) -> bool {
    begin_memory_check_generation(session_id).is_some()
}
#[allow(dead_code)]
pub(super) fn finish_memory_check(session_id: &str) {
    if let Some(session) = lock().as_mut().and_then(|s| s.sessions.get_mut(session_id)) {
        session.running = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_revokes_old_result_and_old_guard() {
        let sid = "pending-generation-reset-regression";
        clear_pending_memory(sid);
        let old = begin_memory_check_generation(sid).unwrap();
        clear_pending_memory(sid);
        let new = begin_memory_check_generation(sid).unwrap();
        assert_ne!(old, new);
        assert!(!publish_memory_check(sid, old, "old".into(), 1, vec![], None));
        finish_memory_check_generation(sid, old);
        assert!(begin_memory_check_generation(sid).is_none());
        assert!(publish_memory_check(sid, new, "new".into(), 1, vec![], None));
        assert_eq!(take_pending_memory(sid).unwrap().prompt, "new");
        finish_memory_check_generation(sid, new);
        clear_pending_memory(sid);
    }

    #[test]
    fn stale_generation_cannot_mark_memories_known() {
        let sid = "pending-stale-known-regression";
        clear_pending_memory(sid);
        let old = begin_memory_check_generation(sid).unwrap();
        clear_pending_memory(sid);
        let new = begin_memory_check_generation(sid).unwrap();
        let ids = vec!["regression-id".to_owned()];
        assert!(!memory_check_generation_active(sid, old));
        assert!(!mark_memories_known_for_generation(sid, old, &ids));
        assert!(!is_memory_injected(sid, &ids[0]));
        assert!(mark_memories_known_for_generation(sid, new, &ids));
        assert!(is_memory_injected(sid, &ids[0]));
        clear_pending_memory(sid);
    }

    #[test]
    fn changed_payload_with_same_ids_is_not_discarded() {
        let sid = "pending-updated-content-regression";
        clear_pending_memory(sid);
        set_pending_memory_with_ids(sid, "enabled".into(), 1, vec!["id".into()]);
        assert!(take_pending_memory(sid).is_some());
        set_pending_memory_with_ids(sid, "disabled".into(), 1, vec!["id".into()]);
        assert_eq!(take_pending_memory(sid).unwrap().prompt, "disabled");
        assert!(!is_memory_injected("unrelated-session", "id"));
        clear_pending_memory(sid);
    }

    #[test]
    fn expired_payload_is_not_reported_ready() {
        let sid = "pending-expiry-regression";
        clear_pending_memory(sid);
        set_pending_memory(sid, "old".into(), 1);
        {
            let mut guard = lock();
            let pending = guard.as_mut().unwrap().sessions.get_mut(sid).unwrap().pending.as_mut().unwrap();
            pending.computed_at = Instant::now() - PENDING_TTL;
        }
        assert!(!has_pending_memory(sid));
        assert!(take_pending_memory(sid).is_none());
        clear_pending_memory(sid);
    }
}
