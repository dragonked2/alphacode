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
//! Malformed calls (a missing required argument, an empty argument object) are
//! counted in a third, separate tier. They used to be *excluded* entirely, on
//! the theory that a schema mistake is correctable and must never lock a model
//! out. That exemption had no bound at all: the observed symptom was the same
//! `write` call, with an empty argument object, failing eight times in a row and
//! the model re-sending it byte-identically each time. Because the streak is
//! keyed by (tool, input), excluding malformed calls bought nothing — a model
//! that genuinely corrects the call sends *different* input and so starts from
//! zero regardless. Counting them costs a corrected retry nothing and bounds the
//! loop. The limit is higher than the runtime tier, since a typo is a real
//! possibility.
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

/// Identical *malformed* calls allowed before the next one is refused.
///
/// Higher than [`IDENTICAL_FAILURE_LIMIT`] because a mistyped argument is a
/// normal thing for a model to do once. Low enough to stop the observed loop:
/// eight identical empty `write` calls, each burning a turn and re-reporting the
/// same "missing field `file_path`".
pub const MALFORMED_CALL_LIMIT: u32 = 3;

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
    /// Malformed calls, tracked separately so a mistyped argument does not eat
    /// into the runtime-failure budget (and vice versa). They share `last_error`
    /// and `order` so the refusal can still quote what went wrong and eviction
    /// stays bounded across both tiers.
    malformed: HashMap<CallKey, u32>,
    /// Last error message per key, so the refusal can remind the model what
    /// actually failed instead of a generic "change the arguments".
    last_error: HashMap<CallKey, String>,
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
    if !state.failures.contains_key(key) && !state.malformed.contains_key(key) {
        while state.failures.len() + state.malformed.len() >= MAX_ENTRIES {
            let Some(oldest) = state.order.pop_front() else {
                break;
            };
            state.failures.remove(&oldest);
            state.malformed.remove(&oldest);
            state.last_error.remove(&oldest);
        }
        state.order.push_back(key.clone());
    }
    // Safety: we just ensured the key exists above (insert or already present).
    // Using `or_insert` via the entry API to avoid a fallible get_mut after
    // insertion, which would be fragile under mutex poisoning.
    state.failures.entry(key.clone()).or_insert(0)
}

/// Same bookkeeping for the malformed tier.
fn malformed_entry_for<'a>(state: &'a mut State, key: &CallKey) -> &'a mut u32 {
    if !state.failures.contains_key(key) && !state.malformed.contains_key(key) {
        while state.failures.len() + state.malformed.len() >= MAX_ENTRIES {
            let Some(oldest) = state.order.pop_front() else {
                break;
            };
            state.failures.remove(&oldest);
            state.malformed.remove(&oldest);
            state.last_error.remove(&oldest);
        }
        state.order.push_back(key.clone());
    }
    state.malformed.entry(key.clone()).or_insert(0)
}

fn bounded_error(message: &str) -> String {
    let message = message.trim();
    if message.len() <= 500 {
        return message.to_string();
    }
    let mut end = 500;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &message[..end])
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
    record_failure_with_error(session, name, known, input, None);
}

/// Record a failure together with its error message, so a later refusal can
/// quote what actually went wrong and how to fix it.
pub fn record_failure_with_error(
    session: &str,
    name: &str,
    known: bool,
    input: &Value,
    error: Option<&str>,
) {
    if session.is_empty() {
        return;
    }
    let mut state = state();
    let key = key_for(session, name, known, input);
    let count = entry_for(&mut state, &key);
    *count = count.saturating_add(1);
    if let Some(message) = error {
        // Bound stored length: error chains can be long; the refusal only
        // quotes the head.
        state.last_error.insert(key, bounded_error(message));
    }
}

/// How many times this exact call has already been rejected as malformed.
///
/// Tracked separately from [`prior_failures`] so a mistyped argument does not
/// consume the runtime-failure budget, and so the caller can apply
/// [`MALFORMED_CALL_LIMIT`] instead.
pub fn prior_malformed(session: &str, name: &str, input: &Value) -> u32 {
    if session.is_empty() {
        return 0;
    }
    let key = key_for(session, name, true, input);
    state().malformed.get(&key).copied().unwrap_or(0)
}

/// Record a malformed call: valid tool, invalid arguments.
///
/// This used to call [`clear_failure`] instead, which is why an identical empty
/// `write` call could be re-sent without limit.
pub fn record_malformed_with_error(session: &str, name: &str, input: &Value, error: Option<&str>) {
    if session.is_empty() {
        return;
    }
    let mut state = state();
    let key = key_for(session, name, true, input);
    let count = malformed_entry_for(&mut state, &key);
    *count = count.saturating_add(1);
    if let Some(message) = error {
        state.last_error.insert(key, bounded_error(message));
    }
}

/// The last recorded error for this exact call, if any.
pub fn last_error(session: &str, name: &str, known: bool, input: &Value) -> Option<String> {
    if session.is_empty() {
        return None;
    }
    let key = key_for(session, name, known, input);
    state().last_error.get(&key).cloned()
}

/// Record that this call succeeded, clearing any failure streak for it.
pub fn record_success(session: &str, name: &str, input: &Value) {
    if session.is_empty() {
        return;
    }
    let mut state = state();
    let key = key_for(session, name, true, input);
    // Both removals unconditional. Written as
    // `failures.remove(..).is_some() || malformed.remove(..).is_some()` the
    // second removal is short-circuited away whenever the first hit, so a
    // malformed streak survived a success.
    state.failures.remove(&key);
    state.malformed.remove(&key);
    state.last_error.remove(&key);
    state.order.retain(|existing| existing != &key);
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
    // Gather from both tiers: a key can be present only in `failures`, only in
    // `malformed`, or in both, and dropping only the first would leave a
    // session's malformed streaks behind for the next session with the same id.
    let removed: Vec<CallKey> = state
        .failures
        .keys()
        .chain(state.malformed.keys())
        .filter(|key| key.session() == session)
        .cloned()
        .collect();
    for key in &removed {
        state.failures.remove(key);
        state.malformed.remove(key);
        // Stored error text is dropped too. Leaving it behind both leaks one
        // session's messages into a later session that reuses the id and grows
        // the map without bound, since nothing else ever evicts `last_error`.
        state.last_error.remove(key);
    }
    let live: std::collections::HashSet<CallKey> = state
        .failures
        .keys()
        .chain(state.malformed.keys())
        .cloned()
        .collect();
    state.order.retain(|key| live.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unicode_errors_are_truncated_safely_and_cleared_on_success() {
        let session = "test-guard-unicode-error";
        let input = json!({"unicode": true});
        let message = format!("{}é-tail", "a".repeat(499));
        for malformed in [false, true] {
            if malformed {
                record_malformed_with_error(session, "bash", &input, Some(&message));
            } else {
                record_failure_with_error(session, "bash", true, &input, Some(&message));
            }
            assert_eq!(
                last_error(session, "bash", true, &input),
                Some(format!("{}…", "a".repeat(499)))
            );
            record_success(session, "bash", &input);
            assert_eq!(last_error(session, "bash", true, &input), None);
        }
        clear_session(session);
    }

    #[test]
    fn eviction_removes_error_text_in_both_tiers() {
        for malformed in [false, true] {
            let mut state = State::default();
            for index in 0..=MAX_ENTRIES {
                let key = key_for("eviction", "tool", true, &json!({"index": index}));
                if malformed {
                    *malformed_entry_for(&mut state, &key) += 1;
                } else {
                    *entry_for(&mut state, &key) += 1;
                }
                state.last_error.insert(key, "error".to_string());
            }
            assert_eq!(state.last_error.len(), MAX_ENTRIES);
            let oldest = key_for("eviction", "tool", true, &json!({"index": 0}));
            assert!(!state.last_error.contains_key(&oldest));
        }
    }

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

    /// A model that mistypes an argument must be able to fix it and carry on. The
    /// old code relied on `clear_failure` for this; it is gone, because the
    /// property it provided falls out of the keying -- a corrected call has a
    /// different input and therefore a different streak.
    #[test]
    fn validation_failure_can_be_corrected_without_repeat_guard_lockout() {
        let session = "test-guard-validation-correction";
        let invalid = json!({});
        let corrected = json!({"command": "echo ok"});
        record_malformed_with_error(session, "bash", &invalid, Some("missing field `command`"));
        assert_eq!(prior_malformed(session, "bash", &invalid), 1);
        assert_eq!(
            prior_malformed(session, "bash", &corrected),
            0,
            "the corrected call must be allowed through"
        );
        clear_session(session);
    }

    /// Regression: an identical malformed call used to have no bound at all,
    /// because validation errors called `clear_failure` and so erased their own
    /// streak. The observed symptom was the same empty-argument `write` failing
    /// eight times in a row, each turn re-reporting "missing field
    /// `file_path`".
    #[test]
    fn identical_malformed_calls_are_bounded() {
        let session = "test-guard-malformed-bound";
        let empty = json!({});
        assert_eq!(prior_malformed(session, "write", &empty), 0);
        for expected in 1..=MALFORMED_CALL_LIMIT {
            record_malformed_with_error(
                session,
                "write",
                &empty,
                Some("missing field `file_path`"),
            );
            assert_eq!(prior_malformed(session, "write", &empty), expected);
            // The limit is what the caller compares against, so reaching it
            // must be observable.
            assert!(
                prior_malformed(session, "write", &empty) < MALFORMED_CALL_LIMIT
                    || expected == MALFORMED_CALL_LIMIT
            );
        }
        assert_eq!(
            prior_malformed(session, "write", &empty),
            MALFORMED_CALL_LIMIT,
            "the guard must fire at the documented limit"
        );
        clear_session(session);
        assert_eq!(prior_malformed(session, "write", &empty), 0);
    }

    /// The point of counting malformed calls rather than exempting them: a
    /// model that actually fixes the call must still be allowed through, with no
    /// clearing needed, because the streak is keyed on the input.
    #[test]
    fn a_corrected_malformed_call_starts_from_zero() {
        let session = "test-guard-malformed-corrected";
        let broken = json!({"file_path": "a.rs"});
        let fixed = json!({"file_path": "a.rs", "content": "x"});
        for _ in 0..MALFORMED_CALL_LIMIT {
            record_malformed_with_error(session, "write", &broken, Some("missing field `content`"));
        }
        assert_eq!(
            prior_malformed(session, "write", &broken),
            MALFORMED_CALL_LIMIT
        );
        // Same tool, corrected arguments: a different key, so unaffected.
        assert_eq!(
            prior_malformed(session, "write", &fixed),
            0,
            "a corrected call must not inherit the broken call's streak"
        );
        clear_session(session);
    }

    /// The two tiers must not eat into each other's budget: a couple of typos
    /// followed by a real execution failure should still hit the runtime limit
    /// on schedule, and vice versa.
    #[test]
    fn malformed_and_runtime_tiers_have_separate_budgets() {
        let session = "test-guard-tier-budgets";
        let input = json!({"command": "boom"});
        record_malformed_with_error(session, "bash", &input, Some("missing field `command`"));
        assert_eq!(prior_failures(session, "bash", true, &input), 0);
        assert_eq!(prior_malformed(session, "bash", &input), 1);
        // A malformed streak must not pre-satisfy the runtime limit.
        record_failure(session, "bash", true, &input);
        assert_eq!(prior_failures(session, "bash", true, &input), 1);
        assert_eq!(prior_malformed(session, "bash", &input), 1);
        // Success clears both.
        record_success(session, "bash", &input);
        assert_eq!(prior_failures(session, "bash", true, &input), 0);
        assert_eq!(prior_malformed(session, "bash", &input), 0);
        clear_session(session);
    }

    /// The last malformed error is kept so the refusal can quote what actually
    /// went wrong instead of a bare "change your arguments".
    #[test]
    fn malformed_errors_are_quoted_for_refusals() {
        let session = "test-guard-malformed-quote";
        let input = json!({});
        assert_eq!(last_error(session, "write", true, &input), None);
        record_malformed_with_error(session, "write", &input, Some("missing field `file_path`"));
        assert_eq!(
            last_error(session, "write", true, &input).as_deref(),
            Some("missing field `file_path`")
        );
        clear_session(session);
        assert_eq!(last_error(session, "write", true, &input), None);
    }

    #[test]
    fn limits_are_the_documented_values() {
        assert_eq!(IDENTICAL_FAILURE_LIMIT, 2);
        assert_eq!(UNKNOWN_NAME_LIMIT, 1);
        assert_eq!(MALFORMED_CALL_LIMIT, 3);
    }

    #[test]
    fn last_error_is_quoted_for_refusal_messages() {
        let session = "test-guard-last-error";
        let input = json!({"command": "false"});
        assert_eq!(last_error(session, "bash", true, &input), None);
        record_failure_with_error(session, "bash", true, &input, Some("exit code 1"));
        assert_eq!(
            last_error(session, "bash", true, &input).as_deref(),
            Some("exit code 1")
        );
        record_success(session, "bash", &input);
        clear_session(session);
    }
}
