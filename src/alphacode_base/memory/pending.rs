use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

type LastInjectedMemorySetBySession = HashMap<String, (HashSet<String>, Instant)>;
type InjectedMemoryIdsBySession = HashMap<String, HashMap<String, Instant>>;

/// Pending memory prompt from background check - ready to inject on next turn.
/// Keyed by session ID so each session gets its own pending memory.
static PENDING_MEMORY: Mutex<Option<HashMap<String, PendingMemory>>> = Mutex::new(None);

/// Signature of the last injected prompt to suppress near-immediate duplicates.
/// Keyed by session ID.
static LAST_INJECTED_PROMPT_SIGNATURE: Mutex<Option<HashMap<String, (String, Instant)>>> =
    Mutex::new(None);

/// Recently injected memory ID sets per session.
/// Used to suppress near-duplicate re-injection even when formatting differs.
static LAST_INJECTED_MEMORY_SET: Mutex<Option<LastInjectedMemorySetBySession>> = Mutex::new(None);

/// Memory IDs that have already been injected into the conversation, with the
/// time they were injected. Used to prevent the same memory from being
/// re-injected on subsequent turns while it is still fresh in the transcript.
/// Keyed by session ID.
static INJECTED_MEMORY_IDS: Mutex<Option<InjectedMemoryIdsBySession>> = Mutex::new(None);

/// Guard to ensure only one memory check runs at a time, per session.
/// Keyed by session ID.
static MEMORY_CHECK_IN_PROGRESS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Suppress repeated identical memory payloads within this many seconds.
const MEMORY_REPEAT_SUPPRESSION_SECS: u64 = 90;
/// Suppress substantially overlapping memory sets for a bit longer.
const MEMORY_SET_REPEAT_SUPPRESSION_SECS: u64 = 180;
/// If a new pending payload overlaps this much with the last injected set,
/// treat it as too similar to surface again immediately.
const MEMORY_SET_OVERLAP_SUPPRESSION_RATIO: f32 = 0.8;
/// How long an injected memory counts as "already known" to a session.
///
/// Injection payloads are ephemeral (not persisted into history), but the
/// model's response that consumed them IS part of the transcript, so
/// re-injecting the same memory shortly afterwards is pure noise. This
/// tracking used to be cleared on every detected topic change, which fires
/// often on real sessions (cosine similarity between consecutive coding turns
/// regularly sits below the topic threshold), so the same memory could be
/// re-injected minutes apart in one transcript. A TTL keeps the dedup stable
/// across topic wobble while still letting genuinely old memories resurface in
/// long sessions once they may have scrolled out of (or been compacted from)
/// the context window.
const INJECTED_MEMORY_TTL_SECS: u64 = 45 * 60;

fn injected_recently(at: &Instant) -> bool {
    at.elapsed().as_secs() < INJECTED_MEMORY_TTL_SECS
}

/// A pending memory result from async checking.
#[derive(Debug, Clone)]
pub struct PendingMemory {
    /// The formatted memory prompt ready for injection.
    pub prompt: String,
    /// Optional UI-focused rendering of the injected memory payload.
    /// This can contain extra display-only metadata that is not sent to the model.
    pub display_prompt: Option<String>,
    /// When this was computed.
    pub computed_at: Instant,
    /// Number of relevant memories found.
    pub count: usize,
    /// IDs of memories included in this prompt (for dedup tracking).
    pub memory_ids: Vec<String>,
}

impl PendingMemory {
    /// Check if this pending memory is still fresh (not too old).
    pub fn is_fresh(&self) -> bool {
        self.computed_at.elapsed().as_secs() < 120
    }
}

fn prompt_signature(prompt: &str) -> String {
    prompt
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

fn memory_set(ids: &[String]) -> HashSet<String> {
    ids.iter().cloned().collect()
}

fn memory_overlap_ratio(left: &HashSet<String>, right: &HashSet<String>) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let intersection = left.intersection(right).count() as f32;
    let baseline = left.len().max(right.len()) as f32;
    intersection / baseline
}

/// Take pending memory if available and fresh for the given session.
pub fn take_pending_memory(session_id: &str) -> Option<PendingMemory> {
    if let Ok(mut guard) = PENDING_MEMORY.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        if let Some(pending) = map.remove(session_id) {
            if !pending.is_fresh() {
                crate::memory_log::log_pending_discarded(session_id, "stale (>120s)");
                return None;
            }

            // If every memory in this payload is still fresh in the session's
            // injected set, the model already knows all of it; do not re-inject
            // just because formatting or ranking shifted slightly.
            if !pending.memory_ids.is_empty()
                && pending
                    .memory_ids
                    .iter()
                    .all(|id| is_memory_injected(session_id, id))
            {
                crate::memory_log::log_pending_discarded(
                    session_id,
                    "all memories already known to session",
                );
                return None;
            }

            let sig = prompt_signature(&pending.prompt);
            if let Ok(mut last_guard) = LAST_INJECTED_PROMPT_SIGNATURE.lock() {
                let sig_map = last_guard.get_or_insert_with(HashMap::new);
                if let Some((last_sig, last_at)) = sig_map.get(session_id)
                    && *last_sig == sig
                    && last_at.elapsed().as_secs() < MEMORY_REPEAT_SUPPRESSION_SECS
                {
                    crate::memory_log::log_pending_discarded(session_id, "duplicate suppressed");
                    return None;
                }
                sig_map.insert(session_id.to_string(), (sig, Instant::now()));
            }

            if !pending.memory_ids.is_empty() {
                let pending_set = memory_set(&pending.memory_ids);
                if let Ok(mut last_guard) = LAST_INJECTED_MEMORY_SET.lock() {
                    let set_map = last_guard.get_or_insert_with(HashMap::new);
                    if let Some((last_set, last_at)) = set_map.get(session_id) {
                        let overlap = memory_overlap_ratio(last_set, &pending_set);
                        if overlap >= MEMORY_SET_OVERLAP_SUPPRESSION_RATIO
                            && last_at.elapsed().as_secs() < MEMORY_SET_REPEAT_SUPPRESSION_SECS
                        {
                            crate::memory_log::log_pending_discarded(
                                session_id,
                                "overlapping memory set suppressed",
                            );
                            return None;
                        }
                    }
                    set_map.insert(session_id.to_string(), (pending_set, Instant::now()));
                }
            }

            if !pending.memory_ids.is_empty() {
                mark_memories_injected(session_id, &pending.memory_ids);
            }

            crate::memory_log::log_pending_consumed(
                session_id,
                pending.count,
                pending.computed_at.elapsed().as_millis() as u64,
                pending.prompt.chars().count(),
            );

            return Some(pending);
        }
    }
    None
}

/// Store a pending memory result for the given session.
pub fn set_pending_memory(session_id: &str, prompt: String, count: usize) {
    set_pending_memory_with_ids(session_id, prompt, count, Vec::new());
}

/// Store a pending memory result with associated memory IDs for dedup tracking.
pub fn set_pending_memory_with_ids(
    session_id: &str,
    prompt: String,
    count: usize,
    memory_ids: Vec<String>,
) {
    set_pending_memory_with_ids_and_display(session_id, prompt, count, memory_ids, None);
}

/// Store a pending memory result with associated memory IDs and optional display-only content.
pub fn set_pending_memory_with_ids_and_display(
    session_id: &str,
    prompt: String,
    count: usize,
    memory_ids: Vec<String>,
    display_prompt: Option<String>,
) {
    crate::memory_log::log_pending_prepared(session_id, &prompt, count, &memory_ids);

    if let Ok(mut guard) = PENDING_MEMORY.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        let new_sig = prompt_signature(&prompt);
        let new_memory_set = memory_set(&memory_ids);

        if let Some(existing) = map.get(session_id)
            && existing.is_fresh()
        {
            let existing_sig = prompt_signature(&existing.prompt);
            let overlap = memory_overlap_ratio(&memory_set(&existing.memory_ids), &new_memory_set);
            if existing_sig == new_sig || overlap >= MEMORY_SET_OVERLAP_SUPPRESSION_RATIO {
                crate::memory_log::log_pending_discarded(
                    session_id,
                    "similar pending payload already queued",
                );
                return;
            }
        }

        map.insert(
            session_id.to_string(),
            PendingMemory {
                prompt,
                display_prompt,
                computed_at: Instant::now(),
                count,
                memory_ids,
            },
        );
    }
}

/// Mark memory IDs as already injected for a session (prevents re-injection on future turns).
pub fn mark_memories_injected(session_id: &str, ids: &[String]) {
    crate::memory_log::log_marked_injected(session_id, ids);
    insert_injected_ids(session_id, ids);
}

/// Mark memory IDs as already KNOWN to a session without them having been
/// injected, e.g. because they were just extracted from this session's own
/// transcript. The conversation already contains this information, so
/// re-injecting it would be a pure echo.
pub fn mark_memories_known(session_id: &str, ids: &[String], reason: &str) {
    if ids.is_empty() {
        return;
    }
    crate::memory_log::log_marked_known(session_id, ids, reason);
    insert_injected_ids(session_id, ids);
}

fn insert_injected_ids(session_id: &str, ids: &[String]) {
    if let Ok(mut guard) = INJECTED_MEMORY_IDS.lock() {
        let outer = guard.get_or_insert_with(HashMap::new);
        let set = outer
            .entry(session_id.to_string())
            .or_insert_with(HashMap::new);
        set.retain(|_, at| injected_recently(at));
        let now = Instant::now();
        for id in ids {
            set.insert(id.clone(), now);
        }
        crate::logging::info(&format!(
            "[{}] Marked {} memory IDs as injected (total tracked: {})",
            session_id,
            ids.len(),
            set.len()
        ));
    }
}

/// Replace injected memory tracking for a session with the provided IDs.
/// Used when restoring persisted session state so the same logical session does
/// not re-inject memories after reload/resume.
pub fn sync_injected_memories(session_id: &str, ids: &[String]) {
    if let Ok(mut guard) = INJECTED_MEMORY_IDS.lock() {
        let outer = guard.get_or_insert_with(HashMap::new);
        if ids.is_empty() {
            outer.remove(session_id);
            return;
        }

        let now = Instant::now();
        outer.insert(
            session_id.to_string(),
            ids.iter().cloned().map(|id| (id, now)).collect(),
        );
    }
}

/// Check if a memory ID has already been injected for a session.
/// An injected ID "expires" after [`INJECTED_MEMORY_TTL_SECS`], at which point
/// the memory may be surfaced again.
pub fn is_memory_injected(session_id: &str, id: &str) -> bool {
    if let Ok(guard) = INJECTED_MEMORY_IDS.lock()
        && let Some(outer) = guard.as_ref()
        && let Some(set) = outer.get(session_id)
        && let Some(at) = set.get(id)
    {
        return injected_recently(at);
    }
    false
}

/// Check if a memory ID has already been injected in ANY session.
/// Used by the singleton memory agent which doesn't track per-session state.
pub fn is_memory_injected_any(id: &str) -> bool {
    if let Ok(guard) = INJECTED_MEMORY_IDS.lock()
        && let Some(outer) = guard.as_ref()
    {
        return outer
            .values()
            .any(|set| set.get(id).is_some_and(injected_recently));
    }
    false
}

/// Clear injected memory tracking for a session (call on session reset or topic change).
pub fn clear_injected_memories(session_id: &str) {
    if let Ok(mut guard) = LAST_INJECTED_PROMPT_SIGNATURE.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(session_id);
    }
    if let Ok(mut guard) = LAST_INJECTED_MEMORY_SET.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(session_id);
    }

    if let Ok(mut guard) = INJECTED_MEMORY_IDS.lock()
        && let Some(outer) = guard.as_mut()
        && let Some(set) = outer.remove(session_id)
        && !set.is_empty()
    {
        crate::logging::info(&format!(
            "[{}] Clearing {} tracked injected memory IDs",
            session_id,
            set.len()
        ));
    }
}

/// Clear all injected memory tracking across all sessions.
pub fn clear_all_injected_memories() {
    if let Ok(mut guard) = LAST_INJECTED_PROMPT_SIGNATURE.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = LAST_INJECTED_MEMORY_SET.lock() {
        *guard = None;
    }

    if let Ok(mut guard) = INJECTED_MEMORY_IDS.lock() {
        if let Some(outer) = guard.as_ref() {
            let total: usize = outer.values().map(|s| s.len()).sum();
            if total > 0 {
                crate::logging::info(&format!(
                    "Clearing {} tracked injected memory IDs across {} sessions",
                    total,
                    outer.len()
                ));
            }
        }
        *guard = None;
    }
}

/// Clear any pending memory result for a session.
pub fn clear_pending_memory(session_id: &str) {
    if let Ok(mut guard) = PENDING_MEMORY.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(session_id);
    }
    if let Ok(mut guard) = LAST_INJECTED_PROMPT_SIGNATURE.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(session_id);
    }
    if let Ok(mut guard) = LAST_INJECTED_MEMORY_SET.lock()
        && let Some(map) = guard.as_mut()
    {
        map.remove(session_id);
    }
    clear_injected_memories(session_id);
}

/// Clear all pending memory state across all sessions.
pub fn clear_all_pending_memory() {
    if let Ok(mut guard) = PENDING_MEMORY.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = LAST_INJECTED_PROMPT_SIGNATURE.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = LAST_INJECTED_MEMORY_SET.lock() {
        *guard = None;
    }
    clear_all_injected_memories();
}

/// Check if there's a pending memory for a specific session.
pub fn has_pending_memory(session_id: &str) -> bool {
    PENDING_MEMORY
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|m| m.contains_key(session_id)))
        .unwrap_or(false)
}

/// Check if there's any pending memory across all sessions.
pub fn has_any_pending_memory() -> bool {
    PENDING_MEMORY
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|m| !m.is_empty()))
        .unwrap_or(false)
}

pub(super) fn begin_memory_check(session_id: &str) -> bool {
    if let Ok(mut guard) = MEMORY_CHECK_IN_PROGRESS.lock() {
        let set = guard.get_or_insert_with(HashSet::new);
        return set.insert(session_id.to_string());
    }
    false
}

pub(super) fn finish_memory_check(session_id: &str) {
    if let Ok(mut guard) = MEMORY_CHECK_IN_PROGRESS.lock()
        && let Some(set) = guard.as_mut()
    {
        set.remove(session_id);
    }
}
