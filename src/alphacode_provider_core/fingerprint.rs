use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
struct ProviderInputSnapshot {
    request_hash: u64,
    item_hashes: Vec<u64>,
    item_hashes_hash: u64,
    system_hash: Option<u64>,
    tools_hash: Option<u64>,
    captured_at: Instant,
}

static PROVIDER_INPUT_BASELINES: LazyLock<Mutex<HashMap<String, ProviderInputSnapshot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn stable_hash_str(value: &str) -> u64 {
    let digest = Sha256::digest(value.as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

pub fn stable_hash_json<T: Serialize + ?Sized>(value: &T) -> u64 {
    let encoded = serde_json::to_string(value).unwrap_or_default();
    stable_hash_str(&encoded)
}

/// Hash and encoded length of `value`, from a single serialization.
///
/// This exists because the obvious spelling of the two answers we need is
/// `stable_hash_json(v)` plus `stable_json_len(v)`, and each of those runs its
/// own `serde_json::to_string`. On this code path `v` is a whole provider
/// request — the message array plus every tool schema — so asking for both
/// answers serialized it twice, on every API call, for every provider, purely
/// to fill two diagnostic fields. Both answers come from the same bytes.
///
/// Falls back exactly as the pair did: a serialization error hashes the empty
/// string and reports a length of zero.
fn hash_and_len<T: Serialize + ?Sized>(value: &T) -> (u64, usize) {
    match serde_json::to_string(value) {
        Ok(encoded) => {
            let len = encoded.len();
            (stable_hash_str(&encoded), len)
        }
        Err(_) => (stable_hash_str(""), 0),
    }
}

fn item_hashes(items: &[Value]) -> Vec<u64> {
    items.iter().map(stable_hash_json).collect()
}

/// Fold per-item digests into one digest.
///
/// Replaces `stable_hash_json(items)` on the hot path: that call re-serialized
/// the entire message array a second time (every byte the per-item digests had
/// already covered). Folding the digests gives the same "any item edit changes
/// the value" property for change detection, at O(items) instead of O(bytes).
///
/// The fold is order-sensitive, so a reordered message array is still detected.
fn fold_item_hashes(item_hashes: &[u64]) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"alphacode.provider.input.v1");
    for hash in item_hashes {
        hasher.update(hash.to_be_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

fn prefix_matches(current: &[u64], previous: &[u64]) -> bool {
    if previous.len() > current.len() {
        return false;
    }
    current[..previous.len()] == *previous
}

fn common_prefix_len(current: &[u64], previous: &[u64]) -> usize {
    current
        .iter()
        .zip(previous.iter())
        .take_while(|(current, previous)| current == previous)
        .count()
}

/// Whether the provider canonical-input fingerprint should be computed.
///
/// The fingerprint is a **diagnostic**: it hashes the entire provider request
/// (message array + every tool schema) to detect prompt-cache prefix drift.
/// Computing it costs ~20x the wire payload in bytes read and ~3.6x the wire
/// serialization time, on **every** provider request, retry included.
///
/// It is therefore off by default and only enabled when the operator asks for
/// it with `ALPHACODE_TRACE`. Call sites must consult this gate *before*
/// building the canonical payload, so the expensive `serde_json::to_value` /
/// `json!` work is skipped too — gating only the final `log_...` call still
/// pays for the payload construction.
///
/// Enabled by `ALPHACODE_TRACE` (the same switch that turns on debug logging)
/// or by the explicit `ALPHACODE_PROVIDER_FINGERPRINT=1` override, so the
/// diagnostic can be turned on without enabling all debug logging.
#[inline]
pub fn fingerprint_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        if matches!(
            std::env::var("ALPHACODE_PROVIDER_FINGERPRINT").as_deref(),
            Ok("1" | "true" | "TRUE" | "yes" | "YES")
        ) {
            return true;
        }
        crate::alphacode_logging::trace_enabled()
    })
}

/// Log a privacy-preserving fingerprint of the provider-specific prompt payload.
///
/// `payload` should be the prompt/cache-relevant request shape after provider-specific
/// normalization, not the high-level Alphacode message list. Do not include volatile transport
/// IDs unless they are intentionally part of the cache key. `items` should be the ordered
/// provider-visible message/content array so prefix drift can be diagnosed by index.
///
/// This is a no-op unless [`fingerprint_enabled`] returns `true`. Callers
/// should gate the *construction* of `payload` / `items` on the same check —
/// building them is the dominant cost.
#[allow(clippy::too_many_arguments)]
pub fn log_provider_canonical_input(
    provider: &str,
    model: &str,
    format: &str,
    payload: &Value,
    items: &[Value],
    system: Option<&Value>,
    tools: Option<&Value>,
    tool_count: Option<usize>,
    extra_fields: &[(&str, String)],
) {
    if !fingerprint_enabled() {
        return;
    }
    let (request_hash, request_json_chars) = hash_and_len(payload);
    let item_hashes = item_hashes(items);
    let item_hashes_hash = stable_hash_json(&item_hashes);
    // The whole-array hash re-encodes every item that `item_hashes` already
    // covers. Folding the per-item digests into a single digest gives the
    // same change-detection property (any item edit changes the fold)
    // without a second full serialization of the message array.
    let input_hash = fold_item_hashes(&item_hashes);
    let (system_hash, system_json_chars) = match system {
        Some(system) => {
            let (hash, len) = hash_and_len(system);
            (Some(hash), Some(len))
        }
        None => (None, None),
    };
    let (tools_hash, tools_json_chars) = match tools {
        Some(tools) => {
            let (hash, len) = hash_and_len(tools);
            (Some(hash), Some(len))
        }
        None => (None, None),
    };
    let first_item_hash = item_hashes.first().copied();
    let last_item_hash = item_hashes.last().copied();

    let log_context = crate::alphacode_logging::current_context_snapshot();
    let session_key = log_context.session.as_deref().unwrap_or("no-session");
    let key = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        session_key, provider, model, format
    );
    let snapshot = ProviderInputSnapshot {
        request_hash,
        item_hashes: item_hashes.clone(),
        item_hashes_hash,
        system_hash,
        tools_hash,
        captured_at: Instant::now(),
    };

    let previous = PROVIDER_INPUT_BASELINES
        .lock()
        .map(|mut baselines| baselines.insert(key, snapshot))
        .ok()
        .flatten();

    let previous_age_secs = previous
        .as_ref()
        .map(|previous| previous.captured_at.elapsed().as_secs());
    let request_changed = previous
        .as_ref()
        .map(|previous| previous.request_hash != request_hash);
    let item_hashes_changed = previous
        .as_ref()
        .map(|previous| previous.item_hashes_hash != item_hashes_hash);
    let prefix_matches = previous
        .as_ref()
        .map(|previous| prefix_matches(&item_hashes, &previous.item_hashes));
    let common_prefix_items = previous
        .as_ref()
        .map(|previous| common_prefix_len(&item_hashes, &previous.item_hashes));
    let first_changed_item_index = common_prefix_items
        .zip(previous.as_ref().map(|previous| previous.item_hashes.len()))
        .and_then(|(common, previous_len)| (common < previous_len).then_some(common));
    let previous_item_count = previous.as_ref().map(|previous| previous.item_hashes.len());
    let system_changed = previous
        .as_ref()
        .map(|previous| previous.system_hash != system_hash);
    let tools_changed = previous
        .as_ref()
        .map(|previous| previous.tools_hash != tools_hash);

    let mut extras = String::new();
    for (key, value) in extra_fields {
        if !key.is_empty() && !value.is_empty() {
            extras.push(' ');
            extras.push_str(key);
            extras.push('=');
            extras.push_str(value);
        }
    }

    crate::alphacode_logging::info(&format!(
        "PROVIDER_CANONICAL_INPUT: provider={} model={} format={} request_hash={} request_json_chars={} \
         input_hash={} item_count={} previous_item_count={:?} item_hashes_hash={} first_item_hash={:?} last_item_hash={:?} \
         previous_age_secs={:?} prefix_matches={:?} common_prefix_items={:?} first_changed_item_index={:?} \
         request_changed={:?} item_hashes_changed={:?} system_hash={:?} system_json_chars={:?} system_changed={:?} \
         tools_hash={:?} tools_json_chars={:?} tool_count={:?} tools_changed={:?}{}",
        provider,
        model,
        format,
        request_hash,
        request_json_chars,
        input_hash,
        items.len(),
        previous_item_count,
        item_hashes_hash,
        first_item_hash,
        last_item_hash,
        previous_age_secs,
        prefix_matches,
        common_prefix_items,
        first_changed_item_index,
        request_changed,
        item_hashes_changed,
        system_hash,
        system_json_chars,
        system_changed,
        tools_hash,
        tools_json_chars,
        tool_count,
        tools_changed,
        extras,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefix_matching_allows_append_only_growth() {
        assert!(prefix_matches(&[1, 2, 3], &[1, 2]));
    }

    #[test]
    fn prefix_matching_detects_changed_prefix() {
        assert!(!prefix_matches(&[1, 9, 3], &[1, 2]));
        assert_eq!(common_prefix_len(&[1, 9, 3], &[1, 2]), 1);
    }

    #[test]
    fn json_hashes_are_content_sensitive() {
        assert_ne!(
            stable_hash_json(&json!({"a": 1})),
            stable_hash_json(&json!({"a": 2}))
        );
    }

    /// Phase 0.2 regression gate: the fingerprint must not be computed when
    /// the log level is off. The audit measured the fingerprint path reading
    /// 20x the wire payload on *every* provider request; this test fails if
    /// that ever becomes unconditional again.
    #[test]
    fn fingerprint_is_gated_off_by_default() {
        // The gate is a process-wide OnceLock, so this asserts the *default*
        // state only when the env override is absent.
        if std::env::var_os("ALPHACODE_PROVIDER_FINGERPRINT").is_some()
            || crate::alphacode_logging::trace_enabled()
        {
            return;
        }
        assert!(!fingerprint_enabled());
    }

    /// The fold must preserve the change-detection property the whole-array
    /// hash provided: any item edit changes the value, and reordering the
    /// array changes the value too.
    #[test]
    fn folded_item_hashes_detect_edits_and_reordering() {
        let items = vec![json!({"a": 1}), json!({"b": 2})];
        let base = fold_item_hashes(&item_hashes(&items));

        let edited = vec![json!({"a": 1}), json!({"b": 3})];
        assert_ne!(base, fold_item_hashes(&item_hashes(&edited)));

        let reordered = vec![json!({"b": 2}), json!({"a": 1})];
        assert_ne!(base, fold_item_hashes(&item_hashes(&reordered)));

        // Same content folds to the same value.
        assert_eq!(base, fold_item_hashes(&item_hashes(&items)));
    }
}
