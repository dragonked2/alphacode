//! Documented harness-side KV cache invalidations.
//!
//! Some harness actions legitimately change the cached request prefix
//! mid-session (a config reload that alters the system prompt, a skill
//! reload that changes the skills list, ...). The resend cost is unavoidable
//! for those, but they must not be reported as harness bugs. Code that
//! knowingly invalidates the prefix records the reason here; the TUI's
//! KV-cache miss detector then attributes the miss to the documented cause
//! and surfaces it, instead of raising an unexplained "harness bust" alarm.
//!
//! Keep `record` calls at the site that performs the invalidation so the
//! journal stays truthful. An empty journal around a harness-caused miss is
//! itself signal: it means something changed the prompt without documenting
//! why, which is exactly the bug class the alarm exists for.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

const MAX_ENTRIES: usize = 16;

/// One documented, intentional cache-prefix invalidation.
#[derive(Debug, Clone)]
pub struct DocumentedInvalidation {
    /// When the invalidating action happened.
    pub at: Instant,
    /// Short stable source label, e.g. `"config reload"`.
    pub source: &'static str,
    /// Human-readable specifics, e.g. the config-reload reason string.
    pub detail: String,
}

static JOURNAL: Mutex<VecDeque<DocumentedInvalidation>> = Mutex::new(VecDeque::new());

/// Record an intentional harness-side cache invalidation.
///
/// Call this from the code that performs a prompt-affecting change, at the
/// moment it happens. `detail` should describe what changed well enough to
/// act on it from the log alone.
pub fn record(source: &'static str, detail: impl Into<String>) {
    let entry = DocumentedInvalidation {
        at: Instant::now(),
        source,
        detail: detail.into(),
    };
    crate::logging::info(&format!(
        "CACHE_INVALIDATION_DOCUMENTED source={} detail={}",
        entry.source, entry.detail
    ));
    let mut journal = JOURNAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if journal.len() >= MAX_ENTRIES {
        journal.pop_front();
    }
    journal.push_back(entry);
}

/// Most recent documented invalidation recorded at or after `since`.
///
/// `since` should be the completion time of the previous request (the
/// baseline): an invalidation recorded between the last request and the
/// current one is what explains a miss on the current one.
pub fn most_recent_since(since: Instant) -> Option<DocumentedInvalidation> {
    let journal = JOURNAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    journal
        .iter()
        .rev()
        .find(|entry| entry.at >= since)
        .cloned()
}

/// Clear the journal. Test-support only: process-global state otherwise leaks
/// documented invalidations across unrelated unit tests.
pub fn clear_for_tests() {
    JOURNAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_since_filters_older_entries() {
        let before = Instant::now();
        record("test source", "detail");
        let found = most_recent_since(before).expect("entry recorded after `before`");
        assert_eq!(found.source, "test source");
    }
}
