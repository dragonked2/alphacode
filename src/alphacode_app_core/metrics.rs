//! Rolling performance metrics for benchmark analysis.
//!
//! Long-running sessions benefit from a compact performance snapshot that
//! surfaces token efficiency, request throughput, and tool success rate so
//! the user (or a benchmark harness) can answer "is this session making
//! progress, or burning tokens?" without parsing the entire log.
//!
//! Data is held in lock-free atomics for the hot path (every tool call,
//! every API token) and gathered into a [`MetricsSnapshot`] on demand.  The
//! snapshot is JSON-serializable so it can be emitted to the health
//! monitor's reporter or piped to an external collector.
//!
//! ## Activation
//!
//! Hooks are scattered across the agent runtime:
//! - `record_api_call(duration, input_tokens, output_tokens, cache_read, cache_creation)`
//! - `record_tool_call(name, duration_ms, success)`
//! - `record_compaction(tokens_saved)`
//!
//! None of these calls take locks, so they are safe to invoke from the
//! streaming agent loop on every event.  The snapshot is built on demand by
//! [`snapshot`] and is the only place that aggregates values across
//! categories.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Per-API-call metrics.  Token counts are summed across calls.
#[derive(Debug, Default)]
struct ApiMetrics {
    calls: AtomicU64,
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cache_read: AtomicU64,
    total_cache_creation: AtomicU64,
    /// Sum of all call durations, in microseconds.  Exposed as average via
    /// the snapshot so a benchmark can read the mean latency without
    /// storing every individual timing.
    total_duration_us: AtomicU64,
    max_duration_us: AtomicU64,
}

/// Per-tool metrics.  Stored as parallel arrays for cache efficiency.
#[derive(Debug, Default)]
struct ToolMetrics {
    calls: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    total_duration_ms: AtomicU64,
    max_duration_ms: AtomicU64,
}

/// Rollup of API and tool activity for the current session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub api_calls: u64,
    pub api_input_tokens: u64,
    pub api_output_tokens: u64,
    pub api_cache_read_tokens: u64,
    pub api_cache_creation_tokens: u64,
    pub api_avg_duration_ms: f64,
    pub api_max_duration_ms: u64,
    /// Tokens per API call (input + output), averaged across all calls.  A
    /// healthy session averages in the low thousands; a runaway session
    /// pushing hundreds of thousands per call is the first signal of an
    /// inefficient prompt or a stuck retry loop.
    pub api_tokens_per_call: f64,
    /// Hit ratio for prompt cache reads vs total input tokens (excluding
    /// cache reads).  A session with cache hits >80% is much cheaper than
    /// one at 0%.
    pub cache_hit_ratio: f64,
    pub tool_calls: u64,
    pub tool_successes: u64,
    pub tool_failures: u64,
    pub tool_success_rate: f64,
    pub tool_avg_duration_ms: f64,
    pub tool_max_duration_ms: u64,
    /// Tokens saved by compaction events.  This is a separate counter
    /// because compaction is opt-in and many sessions do not exercise it.
    pub tokens_saved_by_compaction: u64,
}

static API: ApiMetrics = ApiMetrics {
    calls: AtomicU64::new(0),
    total_input_tokens: AtomicU64::new(0),
    total_output_tokens: AtomicU64::new(0),
    total_cache_read: AtomicU64::new(0),
    total_cache_creation: AtomicU64::new(0),
    total_duration_us: AtomicU64::new(0),
    max_duration_us: AtomicU64::new(0),
};

static TOOL: ToolMetrics = ToolMetrics {
    calls: AtomicU64::new(0),
    successes: AtomicU64::new(0),
    failures: AtomicU64::new(0),
    total_duration_ms: AtomicU64::new(0),
    max_duration_ms: AtomicU64::new(0),
};

static COMPACTION_TOKENS_SAVED: AtomicU64 = AtomicU64::new(0);

/// Record one completed API call.  All counters are atomic so partial
/// updates are safe under concurrent recorders (the streaming agent loop is
/// single-threaded today but tests may drive it from multiple tasks).
pub fn record_api_call(duration: Duration, input_tokens: u64, output_tokens: u64) {
    API.calls.fetch_add(1, Ordering::Relaxed);
    API.total_input_tokens
        .fetch_add(input_tokens, Ordering::Relaxed);
    API.total_output_tokens
        .fetch_add(output_tokens, Ordering::Relaxed);
    let us = duration.as_micros().min(u64::MAX as u128) as u64;
    API.total_duration_us.fetch_add(us, Ordering::Relaxed);
    update_max(&API.max_duration_us, us);
}

/// Record cache hits/creation from an API call.  Kept separate from
/// `record_api_call` because some providers do not emit cache telemetry on
/// every call.
pub fn record_cache_tokens(cache_read: u64, cache_creation: u64) {
    API.total_cache_read
        .fetch_add(cache_read, Ordering::Relaxed);
    API.total_cache_creation
        .fetch_add(cache_creation, Ordering::Relaxed);
}

/// Record a completed tool call.  `success = false` records a failure.
pub fn record_tool_call(duration: Duration, success: bool) {
    TOOL.calls.fetch_add(1, Ordering::Relaxed);
    if success {
        TOOL.successes.fetch_add(1, Ordering::Relaxed);
    } else {
        TOOL.failures.fetch_add(1, Ordering::Relaxed);
    }
    let ms = duration.as_millis().min(u64::MAX as u128) as u64;
    TOOL.total_duration_ms.fetch_add(ms, Ordering::Relaxed);
    update_max(&TOOL.max_duration_ms, ms);
}

/// Record tokens saved by an auto-compaction event.
pub fn record_compaction(tokens_saved: u64) {
    COMPACTION_TOKENS_SAVED.fetch_add(tokens_saved, Ordering::Relaxed);
}

fn update_max(slot: &AtomicU64, value: u64) {
    // Lock-free CAS loop.  The contention is negligible (one update per API
    // call or tool call) but the pattern is correct under arbitrary
    // concurrency from tests.
    let mut current = slot.load(Ordering::Relaxed);
    while value > current {
        match slot.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

/// Snapshot all metrics into an immutable view.
pub fn snapshot() -> MetricsSnapshot {
    let api_calls = API.calls.load(Ordering::Relaxed);
    let total_input = API.total_input_tokens.load(Ordering::Relaxed);
    let total_output = API.total_output_tokens.load(Ordering::Relaxed);
    let cache_read = API.total_cache_read.load(Ordering::Relaxed);
    let cache_creation = API.total_cache_creation.load(Ordering::Relaxed);
    let total_us = API.total_duration_us.load(Ordering::Relaxed);
    let max_us = API.max_duration_us.load(Ordering::Relaxed);

    let tool_calls = TOOL.calls.load(Ordering::Relaxed);
    let tool_successes = TOOL.successes.load(Ordering::Relaxed);
    let tool_failures = TOOL.failures.load(Ordering::Relaxed);
    let total_tool_ms = TOOL.total_duration_ms.load(Ordering::Relaxed);
    let max_tool_ms = TOOL.max_duration_ms.load(Ordering::Relaxed);

    let avg_api_ms = if api_calls == 0 {
        0.0
    } else {
        (total_us as f64) / (api_calls as f64) / 1000.0
    };
    let api_tokens_per_call = if api_calls == 0 {
        0.0
    } else {
        ((total_input + total_output) as f64) / (api_calls as f64)
    };
    let cache_hit_ratio = if total_input == 0 {
        0.0
    } else {
        (cache_read as f64) / ((cache_read + total_input) as f64)
    };
    let tool_success_rate = if tool_calls == 0 {
        1.0
    } else {
        (tool_successes as f64) / (tool_calls as f64)
    };
    let avg_tool_ms = if tool_calls == 0 {
        0.0
    } else {
        (total_tool_ms as f64) / (tool_calls as f64)
    };

    MetricsSnapshot {
        api_calls,
        api_input_tokens: total_input,
        api_output_tokens: total_output,
        api_cache_read_tokens: cache_read,
        api_cache_creation_tokens: cache_creation,
        api_avg_duration_ms: avg_api_ms,
        api_max_duration_ms: max_us / 1000,
        api_tokens_per_call,
        cache_hit_ratio,
        tool_calls,
        tool_successes,
        tool_failures,
        tool_success_rate,
        tool_avg_duration_ms: avg_tool_ms,
        tool_max_duration_ms: max_tool_ms,
        tokens_saved_by_compaction: COMPACTION_TOKENS_SAVED.load(Ordering::Relaxed),
    }
}

/// Reset all counters.  Tests call this between cases.  Production code
/// does not.
pub fn reset() {
    API.calls.store(0, Ordering::Relaxed);
    API.total_input_tokens.store(0, Ordering::Relaxed);
    API.total_output_tokens.store(0, Ordering::Relaxed);
    API.total_cache_read.store(0, Ordering::Relaxed);
    API.total_cache_creation.store(0, Ordering::Relaxed);
    API.total_duration_us.store(0, Ordering::Relaxed);
    API.max_duration_us.store(0, Ordering::Relaxed);
    TOOL.calls.store(0, Ordering::Relaxed);
    TOOL.successes.store(0, Ordering::Relaxed);
    TOOL.failures.store(0, Ordering::Relaxed);
    TOOL.total_duration_ms.store(0, Ordering::Relaxed);
    TOOL.max_duration_ms.store(0, Ordering::Relaxed);
    COMPACTION_TOKENS_SAVED.store(0, Ordering::Relaxed);
}

/// Per-tool aggregate: (calls, failures, total_duration_ms, max_duration_ms).
type ToolAggregate = (u64, u64, u64, u64);

/// Per-tool-name breakdown.  Optional - many tools share a single
/// `TOOL` aggregate.  When a session wants per-tool granularity (e.g. for
/// a benchmark), it can opt into the [`per_tool`] table.
static PER_TOOL: std::sync::LazyLock<std::sync::Mutex<HashMap<String, ToolAggregate>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Record a tool call with its name.  Calls [`record_tool_call`] with the
/// same duration and success flag and additionally updates a per-name map
/// so [`per_tool`] can report per-tool averages.
pub fn record_tool_call_named(name: &str, duration: Duration, success: bool) {
    record_tool_call(duration, success);
    let ms = duration.as_millis().min(u64::MAX as u128) as u64;
    if let Ok(mut g) = PER_TOOL.lock() {
        let entry = g.entry(name.to_string()).or_insert((0, 0, 0, 0));
        entry.0 = entry.0.saturating_add(1);
        if success {
            entry.1 = entry.1.saturating_add(1);
        } else {
            entry.2 = entry.2.saturating_add(1);
        }
        entry.3 = entry.3.saturating_add(ms);
    }
}

/// Per-tool call counts.  Returns (calls, successes, failures, total_ms).
pub fn per_tool() -> HashMap<String, (u64, u64, u64, u64)> {
    PER_TOOL.lock().map(|g| g.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_metrics_accumulate() {
        reset();
        record_api_call(Duration::from_millis(100), 1000, 200);
        record_api_call(Duration::from_millis(300), 1500, 250);
        let s = snapshot();
        assert_eq!(s.api_calls, 2);
        assert_eq!(s.api_input_tokens, 2500);
        assert_eq!(s.api_output_tokens, 450);
        assert_eq!(s.api_max_duration_ms, 300);
        assert!((s.api_avg_duration_ms - 200.0).abs() < 0.001);
    }

    #[test]
    fn cache_hit_ratio_computes() {
        reset();
        record_api_call(Duration::from_millis(100), 1000, 0);
        record_cache_tokens(4000, 0);
        let s = snapshot();
        // ratio = 4000 / (4000 + 1000) = 0.8
        assert!((s.cache_hit_ratio - 0.8).abs() < 0.001);
    }

    #[test]
    fn tool_success_rate() {
        reset();
        record_tool_call(Duration::from_millis(50), true);
        record_tool_call(Duration::from_millis(50), true);
        record_tool_call(Duration::from_millis(50), false);
        let s = snapshot();
        assert_eq!(s.tool_calls, 3);
        assert_eq!(s.tool_successes, 2);
        assert_eq!(s.tool_failures, 1);
        assert!((s.tool_success_rate - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn compaction_savings() {
        reset();
        record_compaction(5000);
        record_compaction(3000);
        let s = snapshot();
        assert_eq!(s.tokens_saved_by_compaction, 8000);
    }

    #[test]
    fn empty_snapshot_is_zero() {
        reset();
        let s = snapshot();
        assert_eq!(s.api_calls, 0);
        assert_eq!(s.tool_calls, 0);
        assert_eq!(s.api_avg_duration_ms, 0.0);
        assert_eq!(s.cache_hit_ratio, 0.0);
    }

    #[test]
    fn per_tool_records() {
        reset();
        record_tool_call_named("bash", Duration::from_millis(50), true);
        record_tool_call_named("bash", Duration::from_millis(70), false);
        record_tool_call_named("read", Duration::from_millis(20), true);
        let per = per_tool();
        let bash = per.get("bash").unwrap();
        assert_eq!(bash.0, 2);
        assert_eq!(bash.1, 1);
        assert_eq!(bash.2, 1);
        assert_eq!(bash.3, 120);
        let read = per.get("read").unwrap();
        assert_eq!(read.0, 1);
    }
}
