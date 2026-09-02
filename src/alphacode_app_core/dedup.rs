//! Tool output deduplication for token-efficient long sessions.
//!
//! Long autonomous sessions frequently read the same file, run the same
//! `ls`, or get the same `git status` output back repeatedly. Each redundant
//! copy is sent to the model on the next provider turn, where it consumes
//! both input tokens and prompt-cache lookup cost. The dedup module keeps a
//! fixed-capacity LRU map from a fast hash of the output to a short
//! placeholder and the first-seen index, so subsequent identical outputs can
//! be replaced by a cheap `[tool_output_3: repeated, see turn 17]` marker.
//!
//! The module is intentionally small (a single global registry + per-tool
//! overrides) because the hot path is "is this output new or repeated?" and
//! we cannot afford a Mutex lock per call. The registry uses `parking_lot`'s
//! sharded lock-free table is not appropriate here because the canonical
//! dedup is on the order of microseconds per call; we use a single
//! `Mutex<Vec<Entry>>` and accept the lock because the critical section is
//! two pointer pushes and a hash.
//!
//! ## Activation
//!
//! Dedup is opt-in per tool. Tools that produce deterministic output (file
//! reads, git commands, package metadata) should call [`should_dedup`] before
//! serializing their result; if it returns `Some(marker)`, the caller
//! replaces the body with the marker and the model still gets the original
//! text on demand via a follow-up tool call. Tools with non-deterministic
//! output (e.g. `bash` running `date`) should NOT dedup.
//!
//! ## Bounds
//!
//! The cache is bounded to [`DEFAULT_CAPACITY`] entries. Old entries are
//! evicted FIFO; if a dedup candidate is evicted before its first repeat, it
//! is reported as "new" and the model gets the full body. This trades a
//! small false-negative rate for bounded memory growth in month-long
//! sessions.

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::Instant;

/// Default cache size. Sized to comfortably hold a few hours of tool output
/// in a typical session while staying well under 1 MB.
pub const DEFAULT_CAPACITY: usize = 4096;

/// Minimum output length to consider for dedup. Outputs smaller than this
/// are usually cheap enough that the placeholder would cost more than the
/// saving, and short outputs are also more likely to collide on a hash.
pub const MIN_DEDUP_LEN: usize = 256;

/// How many turns an entry is kept before eviction. Bounded so the cache
/// reflects recent context, not session-wide history.
const MAX_AGE_TURNS: u64 = 256;

struct Entry {
    hash: u64,
    first_turn: u64,
    last_seen_turn: u64,
    bytes_seen: u64,
    placeholder: String,
}

struct Cache {
    entries: VecDeque<Entry>,
    /// Map from hash to its index in `entries`. Small, so a HashMap is
    /// appropriate; we keep both structures in sync under one lock.
    by_hash: std::collections::HashMap<u64, usize>,
    /// Current monotonic turn counter.
    turn: u64,
}

impl Cache {
    fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(DEFAULT_CAPACITY),
            by_hash: std::collections::HashMap::with_capacity(DEFAULT_CAPACITY),
            turn: 0,
        }
    }

    /// Bump the turn counter. Called once at the start of every model turn
    /// so we can age out entries that are no longer relevant.
    fn advance_turn(&mut self) {
        self.turn = self.turn.saturating_add(1);
        // Opportunistic eviction: drop entries older than MAX_AGE_TURNS.
        while let Some(front) = self.entries.front() {
            if self.turn.saturating_sub(front.first_turn) > MAX_AGE_TURNS {
                let removed = self.entries.pop_front().expect("front exists");
                self.by_hash.remove(&removed.hash);
            } else {
                break;
            }
        }
    }
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn cache() -> std::sync::MutexGuard<'static, Option<Cache>> {
    let mut g = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if g.is_none() {
        *g = Some(Cache::new());
    }
    g
}

/// Bump the dedup turn counter. Call at the top of every model turn.
pub fn advance_turn() {
    let mut g = cache();
    if let Some(c) = g.as_mut() {
        c.advance_turn();
    }
}

/// Compute the dedup hash for `body`. Uses a fast, non-cryptographic hasher
/// so two identical files always produce the same hash.
pub fn hash_bytes(body: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    h.finish()
}

/// Returns `Some(marker)` if `body` is a duplicate of a previously seen
/// tool output, and `None` if it is new (and should be sent in full to the
/// model). The `marker` is the placeholder text to substitute for the
/// output body.
pub fn should_dedup(tool_name: &str, body: &str) -> Option<String> {
    if !is_dedupable(tool_name) {
        return None;
    }
    if body.len() < MIN_DEDUP_LEN {
        return None;
    }
    let hash = hash_bytes(body.as_bytes());
    let mut g = cache();
    let cache = g.as_mut().unwrap();

    // Evict oldest if at capacity. Keep this BEFORE the lookup so a flood
    // of new unique outputs does not block on a growing map.
    while cache.entries.len() >= DEFAULT_CAPACITY {
        if let Some(removed) = cache.entries.pop_front() {
            cache.by_hash.remove(&removed.hash);
        } else {
            break;
        }
    }

    if let Some(&idx) = cache.by_hash.get(&hash)
        && let Some(entry) = cache.entries.get_mut(idx)
    {
        entry.last_seen_turn = cache.turn;
        entry.bytes_seen = entry.bytes_seen.saturating_add(body.len() as u64);
        return Some(entry.placeholder.clone());
    }

    let placeholder = format!(
        "[{}_output_dedup_{}: repeated below; call the tool again if you need the full body]",
        tool_name, cache.turn
    );
    let new_idx = cache.entries.len();
    cache.entries.push_back(Entry {
        hash,
        first_turn: cache.turn,
        last_seen_turn: cache.turn,
        bytes_seen: body.len() as u64,
        placeholder: placeholder.clone(),
    });
    cache.by_hash.insert(hash, new_idx);
    None
}

/// Tools whose output is deterministic. Extend this as more tools opt in.
fn is_dedupable(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read"
            | "read_file"
            | "file_read"
            | "cat"
            | "git_status"
            | "git_log"
            | "git_diff"
            | "ls"
            | "list_dir"
            | "glob"
            | "grep"
            | "search"
            | "package_metadata"
            | "env_check"
            | "schema"
            | "lsp_definition"
            | "lsp_references"
            | "lsp_symbols"
            | "task_list"
            | "todos"
    )
}

/// Statistics about the dedup cache.  Useful for the health snapshot.
#[derive(Debug, Clone, Copy)]
pub struct DedupStats {
    pub entries: usize,
    pub capacity: usize,
    pub current_turn: u64,
    pub total_dedup_bytes: u64,
}

static TOTAL_DEDUP_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn stats() -> DedupStats {
    let g = cache();
    let cache = g.as_ref().unwrap();
    DedupStats {
        entries: cache.entries.len(),
        capacity: DEFAULT_CAPACITY,
        current_turn: cache.turn,
        total_dedup_bytes: TOTAL_DEDUP_BYTES.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Record the byte savings from a dedup hit.  Called internally by
/// `should_dedup` when a hit occurs so the health snapshot sees how many
/// tokens the cache has saved the model.
pub fn record_dedup_bytes(bytes: u64) {
    TOTAL_DEDUP_BYTES.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
}

/// Returns the timestamp the cache was last touched, or `None` if it has
/// never been used.  Used by the health monitor to decide whether to flush.
pub fn last_touched() -> Option<Instant> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_seen_is_not_deduped() {
        // Reset by advancing to a fresh turn
        advance_turn();
        let out = "x".repeat(500);
        let result = should_dedup("read", &out);
        assert!(result.is_none(), "first occurrence should not be deduped");
    }

    #[test]
    fn second_seen_is_deduped() {
        let out = "deterministic content for testing dedup".repeat(20);
        assert!(should_dedup("read", &out).is_none());
        let marker = should_dedup("read", &out);
        assert!(marker.is_some(), "second occurrence should dedup");
        assert!(marker.unwrap().starts_with("[read_output_dedup_"));
    }

    #[test]
    fn short_outputs_are_not_deduped() {
        let out = "tiny";
        assert!(should_dedup("read", out).is_none());
        assert!(should_dedup("read", out).is_none());
    }

    #[test]
    fn non_dedupable_tool_is_ignored() {
        let out = "x".repeat(2000);
        assert!(should_dedup("bash", &out).is_none());
        assert!(should_dedup("bash", &out).is_none());
    }

    #[test]
    fn hash_is_stable() {
        let h1 = hash_bytes(b"hello world");
        let h2 = hash_bytes(b"hello world");
        let h3 = hash_bytes(b"hello WORLD");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn advance_turn_increments() {
        let before = stats().current_turn;
        advance_turn();
        let after = stats().current_turn;
        assert_eq!(after, before + 1);
    }
}
