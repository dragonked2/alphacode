//! Guard against a model re-issuing a tool call that already failed.
//!
//! A model that gets a tool error back will often re-send the identical call —
//! or re-invent the same unknown tool name — several times in one turn. One
//! observed session burned twelve calls and roughly two minutes on `ls.intent`,
//! `bash.intent`, and `selfdev.intent`, none of which can ever succeed, because
//! nothing in the tool layer stopped it.
//!
//! `Registry::execute` is the one choke point every caller shares (agent loop,
//! TUI client, server client-actions, `batch` subcalls), so the guard lives here
//! and is consulted immediately before dispatch. It only counts *failures*: a
//! tool a caller legitimately polls keeps working.
//!
//! Two tiers, because the two failure shapes need different rules:
//!
//! * **Unknown name** — the name is not in the registry, so retrying it can
//!   never succeed no matter what the arguments are. Keyed by name alone, and
//!   refused on the second attempt.
//! * **Identical input** — a real tool failed with byte-identical input. Keyed
//!   by (tool, input), refused on the third attempt; a success clears the key,
//!   so a transient failure that later works does not inherit a streak.
//!
//! The map is bounded so a month-long session cannot grow it without limit, and
//! [`clear_session`] drops one session's entries when it tears down.

use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

/// Identical (tool, input) failures allowed before the next one is refused
/// without being dispatched. Two failures already means the model is not
/// adapting to the error, so the third attempt is stopped.
pub const IDENTICAL_FAILURE_LIMIT: u32 = 2;

/// Failures for an unknown tool *name* before further calls are refused. The
/// name cannot start resolving by being repeated, so a single retry is the
/// most we allow.
pub const UNKNOWN_NAME_LIMIT: u32 = 1;

/// Cap on tracked calls. Oldest keys are evicted first; a false negative (we
/// forget a streak) only costs one extra tool call.
const MAX_ENTRIES: usize = 512;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum CallKey {
    // NOTE: `session` is part of the key so streaks cannot leak between
    // sessions and `clear_session` can be exact. Session ids are included in
    // `Debug` output only; nothing logs these keys.
    /// A tool name that is not present in the registry.
    UnknownName { session: String, name: String },
    /// A registered tool called with byte-identical input.
    IdenticalInput {
        session: String,
        name: String,
        input: u64,
    },
}

#[derive(Default)]
struct State {
    failures: HashMap<CallKey, u32>,
    /// Insertion order, for bounded eviction.
    order: VecDeque<CallKey>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Stable hash of a canonical input object. `serde_json::Map` is ordered, so
/// two structurally identical inputs always hash the same here.
fn input_hash(input: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    input.to_string().hash(&mut hasher);
    hasher.finish()
}

fn key_for(session: &str, name: &str, known: bool, input: &Value) -> CallKey {
    let session = session.to_string();
    let name = name.trim().to_ascii_lowercase();
    if known {
        CallKey::IdenticalInput {
            session,
            name,
            input: input_hash(input),
        }
    } else {
        CallKey::UnknownName { session, name }
    }
}

fn entry_for<'a>(state: &'a mut State, key: &CallKey) -> &'a mut u32 {
    if !state.failures.contains_key(key) {
        while state.failures.len() >= MAX_ENTRIES {
            let Some(oldest) = state.order.pop_front() else {
                break;
            };
            state.failures.remove(&oldest);
        }
        state.order.push_back(key.clone());
    }
    // Safety: we just ensured the key exists above (insert or already present).
    // Using `or_insert` via the entry API to avoid a fallible get_mut after
    // insertion, which would be fragile under mutex poisoning.
    state.failures.entry(key.clone()).or_insert(0)
}

/// How many times this exact call has already failed in `session`. `known` says
/// whether the already-resolved name exists in the registry.
pub fn prior_failures(session: &str, name: &str, known: bool, input: &Value) -> u32 {
    if session.is_empty() {
        return 0;
    }
    let key = key_for(session, name, known, input);
    state().failures.get(&key).copied().unwrap_or(0)
}

/// Record that this call failed. Safe to call from every failure path.
pub fn record_failure(session: &str, name: &str, known: bool, input: &Value) {
    if session.is_empty() {
        return;
    }
    let mut state = state();
    let key = key_for(session, name, known, input);
    let count = entry_for(&mut state, &key);
    *count = count.saturating_add(1);
}

/// Record that this call succeeded, clearing any failure streak for it.
pub fn record_success(session: &str, name: &str, input: &Value) {
    if session.is_empty() {
        return;
    }
    let mut state = state();
    let key = key_for(session, name, true, input);
    if state.failures.remove(&key).is_some() {
        state.order.retain(|existing| existing != &key);
    }
}

impl CallKey {
    fn session(&self) -> &str {
        match self {
            CallKey::UnknownName { session, .. } | CallKey::IdenticalInput { session, .. } => {
                session
            }
        }
    }
}

/// Drop every tracked failure for a session (called on session teardown).
pub fn clear_session(session: &str) {
    if session.is_empty() {
        return;
    }
    let mut state = state();
    let removed: Vec<CallKey> = state
        .failures
        .keys()
        .filter(|key| key.session() == session)
        .cloned()
        .collect();
    for key in &removed {
        state.failures.remove(key);
    }
    let live: std::collections::HashSet<CallKey> = state.failures.keys().cloned().collect();
    state.order.retain(|key| live.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_name_streak_counts_by_name_only() {
        let session = "test-guard-unknown-name";
        let first = json!({"intent": "List desktop"});
        let second = json!({"intent": "List root"});
        assert_eq!(prior_failures(session, "ls.intent", false, &first), 0);
        record_failure(session, "ls.intent", false, &first);
        assert_eq!(prior_failures(session, "ls.intent", false, &first), 1);
        // Different arguments still count: the *name* cannot start resolving.
        assert_eq!(prior_failures(session, "ls.intent", false, &second), 1);
        clear_session(session);
        assert_eq!(prior_failures(session, "ls.intent", false, &first), 0);
    }

    #[test]
    fn identical_failure_streak_is_per_input_and_cleared_by_success() {
        let session = "test-guard-identical-input";
        let input = json!({"command": "false"});
        let other = json!({"command": "true"});
        record_failure(session, "bash", true, &input);
        assert_eq!(prior_failures(session, "bash", true, &input), 1);
        // A different input is a different call and does not inherit the streak.
        assert_eq!(prior_failures(session, "bash", true, &other), 0);
        record_success(session, "bash", &input);
        assert_eq!(prior_failures(session, "bash", true, &input), 0);
        clear_session(session);
    }

    #[test]
    fn streaks_are_scoped_to_one_session() {
        let a = "test-guard-scope-a";
        let b = "test-guard-scope-b";
        let input = json!({});
        record_failure(a, "ls", false, &input);
        assert_eq!(prior_failures(a, "ls", false, &input), 1);
        assert_eq!(prior_failures(b, "ls", false, &input), 0);
        clear_session(a);
        assert_eq!(prior_failures(a, "ls", false, &input), 0);
        assert_eq!(prior_failures(b, "ls", false, &input), 0);
    }

    #[test]
    fn unknown_and_known_names_do_not_share_a_streak() {
        let session = "test-guard-tiers";
        let input = json!({});
        record_failure(session, "ls", false, &input);
        assert_eq!(prior_failures(session, "ls", false, &input), 1);
        // Once the name resolves it is a different tier with a fresh count.
        assert_eq!(prior_failures(session, "ls", true, &input), 0);
        clear_session(session);
    }

    #[test]
    fn empty_session_is_never_tracked() {
        let input = json!({});
        record_failure("", "ls", false, &input);
        assert_eq!(prior_failures("", "ls", false, &input), 0);
    }

    #[test]
    fn limits_are_the_documented_values() {
        assert_eq!(IDENTICAL_FAILURE_LIMIT, 2);
        assert_eq!(UNKNOWN_NAME_LIMIT, 1);
    }
}
