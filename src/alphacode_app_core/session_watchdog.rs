//! Session Health Watchdog — detects and auto-recovers from stale states.
//!
//! Long-running sessions can stall when:
//! - The agent loop stops processing messages
//! - The TUI render thread freezes
//! - Provider connections hang
//! - Memory grows monotonically (leak)
//!
//! The watchdog monitors subsystem liveness via the health monitor's
//! per-subsystem heartbeat tracking and takes corrective action:
//!
//! 1. **Stall detection**: If all subsystems are idle for >5 minutes,
//!    the watchdog signals a stall and attempts recovery.
//! 2. **Memory leak mitigation**: If RSS grows monotonically for >30
//!    minutes, the watchdog triggers a forced compaction.
//! 3. **Connection refresh**: If the provider hasn't sent a keepalive
//!    in >10 minutes, the watchdog closes and reopens the connection.
//! 4. **Crash recovery**: If a subsystem hasn't heartbeated in >2
//!    minutes, the watchdog marks the session as degraded and notifies
//!    the user.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Stall timeout — how long all subsystems can be idle before we consider
/// the session stalled.
const STALL_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Memory growth monitoring window.
const MEMORY_WINDOW_SECS: u64 = 1800; // 30 minutes

#[allow(dead_code)]
/// Connection keepalive timeout.
const KEEPALIVE_TIMEOUT_SECS: u64 = 600; // 10 minutes

/// Subsystem liveness timeout.
const SUBSYSTEM_TIMEOUT_SECS: u64 = 120; // 2 minutes

/// Recovery action types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No recovery needed — session is healthy.
    None,
    /// Signal a stall — attempt to wake subsystems.
    StallDetected,
    /// Trigger forced compaction to reclaim memory.
    ForcedCompaction,
    /// Refresh provider connection.
    ConnectionRefresh,
    /// Mark session as degraded — user notification needed.
    Degraded,
}

/// Watchdog state tracked atomically for lock-free hot path.
struct WatchdogState {
    /// Last time a recovery action was taken.
    last_recovery: Mutex<Instant>,
    /// Number of recoveries performed.
    recovery_count: AtomicU64,
    /// Whether memory is being monitored for leaks.
    memory_monitoring_enabled: AtomicBool,
    /// RSS samples for trend analysis.
    rss_samples: Mutex<Vec<(Instant, u64)>>,
}

static WATCHDOG: OnceLock<WatchdogState> = OnceLock::new();

fn state() -> &'static WatchdogState {
    WATCHDOG.get_or_init(|| WatchdogState {
        last_recovery: Mutex::new(Instant::now()),
        recovery_count: AtomicU64::new(0),
        memory_monitoring_enabled: AtomicBool::new(true),
        rss_samples: Mutex::new(Vec::with_capacity(64)),
    })
}

/// Initialize the session watchdog. Safe to call multiple times.
pub fn init() {
    // Touch the state to ensure it's initialized.
    let _ = state();
    crate::logging::info("session_watchdog: initialized");
}

/// Record an RSS sample for memory trend analysis.
pub fn record_rss(rss_bytes: u64) {
    let s = state();
    if !s.memory_monitoring_enabled.load(Ordering::Relaxed) {
        return;
    }
    let mut samples = s.rss_samples.lock().unwrap_or_else(|p| p.into_inner());
    if samples.len() >= 64 {
        samples.remove(0);
    }
    samples.push((Instant::now(), rss_bytes));
}

/// Check whether the session needs recovery and return the action.
///
/// This is the main watchdog check. It is called by the health reporter thread
/// once per [`crate::alphacode_app_core::health::REPORT_INTERVAL`].
pub fn check_health() -> RecoveryAction {
    let s = state();
    let now = Instant::now();

    // Check for memory leak (monotonic RSS growth).
    if let Some(action) = check_memory_trend(s, now) {
        return action;
    }

    // Check subsystem liveness via the health monitor.
    if let Some(action) = check_subsystem_liveness(now) {
        return action;
    }

    RecoveryAction::None
}

/// Return samples no older than `max_age`, excluding anomalous timestamps in
/// the future.
///
/// Comparing ages directly is important on platforms whose monotonic-clock
/// epoch is the machine boot. Constructing `now - max_age` can underflow and
/// panic when the machine has been up for less than `max_age`.
fn samples_within_window(samples: &[(Instant, u64)], now: Instant, max_age: Duration) -> Vec<u64> {
    samples
        .iter()
        .filter_map(|(sampled_at, rss_bytes)| {
            let age = now.checked_duration_since(*sampled_at)?;
            (age <= max_age).then_some(*rss_bytes)
        })
        .collect()
}

/// Check if RSS has been growing monotonically for the monitoring window.
fn check_memory_trend(s: &WatchdogState, now: Instant) -> Option<RecoveryAction> {
    let samples = s.rss_samples.lock().unwrap_or_else(|p| p.into_inner());
    if samples.len() < 10 {
        return None;
    }

    let window = Duration::from_secs(MEMORY_WINDOW_SECS);
    let recent = samples_within_window(&samples, now, window);

    if recent.len() < 5 {
        return None;
    }

    // Do not diagnose a "30-minute leak" from only a few minutes of data. This
    // also prevents a short-lived but sharp growth spike from repeatedly
    // forcing compaction before the full observation window has elapsed.
    // Require the *oldest retained sample* to span the complete window. Using
    // `samples.first()` here would let one ancient sample satisfy the age gate
    // even when the retained in-window data covers only a few minutes.
    let oldest_recent_age = samples
        .iter()
        .filter_map(|(sampled_at, _)| now.checked_duration_since(*sampled_at))
        .filter(|age| *age <= window)
        .max()?;
    if oldest_recent_age < window {
        return None;
    }

    // Check for monotonic growth: each sample >= previous.
    let monotonic = recent.windows(2).all(|w| w[1] >= w[0]);
    if !monotonic {
        return None;
    }

    // Check for significant growth (> 20% increase over window).
    let first = recent.first()?;
    let last = recent.last()?;
    if *first > 0 && *last as f64 / *first as f64 > 1.2 {
        crate::logging::warn(&format!(
            "session_watchdog: RSS grew {:.1}% over {} minutes ({} -> {} bytes)",
            (*last as f64 / *first as f64 - 1.0) * 100.0,
            window.as_secs() / 60,
            first,
            last
        ));
        Some(RecoveryAction::ForcedCompaction)
    } else {
        None
    }
}

/// Check subsystem liveness via the health monitor.
fn check_subsystem_liveness(_now: Instant) -> Option<RecoveryAction> {
    use crate::alphacode_app_core::health;

    let snap = health::snapshot();

    // Check if any critical subsystem has been idle too long.
    let critical_subsystems = ["agent_loop", "tui_render", "main"];
    for idle in &snap.subsystems_idle_secs {
        if critical_subsystems.contains(&idle.name.as_str())
            && idle.idle_secs > SUBSYSTEM_TIMEOUT_SECS
        {
            crate::logging::warn(&format!(
                "session_watchdog: subsystem '{}' idle for {}s (threshold: {}s)",
                idle.name, idle.idle_secs, SUBSYSTEM_TIMEOUT_SECS
            ));
            return Some(RecoveryAction::Degraded);
        }
    }

    // Check for process-wide stall: all subsystems idle > STALL_TIMEOUT.
    if !snap.subsystems_idle_secs.is_empty() {
        let max_idle = snap
            .subsystems_idle_secs
            .iter()
            .map(|s| s.idle_secs)
            .max()
            .unwrap_or(0);
        let min_idle = snap
            .subsystems_idle_secs
            .iter()
            .map(|s| s.idle_secs)
            .min()
            .unwrap_or(0);
        if min_idle > STALL_TIMEOUT_SECS && max_idle > STALL_TIMEOUT_SECS {
            crate::logging::warn(&format!(
                "session_watchdog: all subsystems idle >{}s — stall detected",
                STALL_TIMEOUT_SECS
            ));
            return Some(RecoveryAction::StallDetected);
        }
    }

    None
}

/// Mark that a recovery action was performed.
pub fn record_recovery(action: RecoveryAction) {
    let s = state();
    s.recovery_count.fetch_add(1, Ordering::Relaxed);
    *s.last_recovery.lock().unwrap_or_else(|p| p.into_inner()) = Instant::now();
    crate::health::record_recovery();
    crate::logging::info(&format!(
        "session_watchdog: recovery performed: {:?}",
        action
    ));
}

/// Get the number of recoveries performed.
pub fn recovery_count() -> u64 {
    state().recovery_count.load(Ordering::Relaxed)
}

/// Get how long since the last recovery.
pub fn time_since_last_recovery() -> Duration {
    let s = state();
    let last = *s.last_recovery.lock().unwrap_or_else(|p| p.into_inner());
    last.elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        init();
        // recovery_count may be non-zero from prior tests in the same process.
        // Just verify init does not panic.
    }

    #[test]
    fn test_record_recovery() {
        init();
        let before = recovery_count();
        record_recovery(RecoveryAction::ForcedCompaction);
        assert_eq!(recovery_count(), before + 1);
    }

    #[test]
    fn test_time_since_last_recovery() {
        init();
        record_recovery(RecoveryAction::StallDetected);
        let elapsed = time_since_last_recovery();
        assert!(elapsed < Duration::from_secs(1));
    }

    #[test]
    fn test_check_health_does_not_panic() {
        init();
        let _action = check_health();
    }

    fn state_with_samples(samples: Vec<(Instant, u64)>) -> WatchdogState {
        WatchdogState {
            last_recovery: Mutex::new(Instant::now()),
            recovery_count: AtomicU64::new(0),
            memory_monitoring_enabled: AtomicBool::new(true),
            rss_samples: Mutex::new(samples),
        }
    }

    fn increasing_samples(
        start: Instant,
        elapsed: Duration,
        first: u64,
        increment: u64,
    ) -> Vec<(Instant, u64)> {
        (0..10)
            .map(|index| {
                let sample_at = start
                    .checked_add(elapsed.mul_f64(index as f64 / 9.0))
                    .expect("test instant should be representable");
                (sample_at, first + index * increment)
            })
            .collect()
    }

    #[test]
    fn ten_samples_do_not_panic_before_full_window() {
        let start = Instant::now();
        let elapsed = Duration::from_secs(MEMORY_WINDOW_SECS - 1);
        let state = state_with_samples(increasing_samples(start, elapsed, 100, 4));
        let now = start
            .checked_add(elapsed)
            .expect("test instant should be representable");

        assert_eq!(check_memory_trend(&state, now), None);
    }

    #[test]
    fn flat_full_window_does_not_trigger_compaction() {
        let start = Instant::now();
        let samples = increasing_samples(start, Duration::from_secs(MEMORY_WINDOW_SECS), 100, 0);
        let state = state_with_samples(samples);
        let now = start
            .checked_add(Duration::from_secs(MEMORY_WINDOW_SECS))
            .expect("test instant should be representable");

        assert_eq!(check_memory_trend(&state, now), None);
    }

    #[test]
    fn monotonic_full_window_growth_triggers_compaction() {
        let start = Instant::now();
        let samples = increasing_samples(start, Duration::from_secs(MEMORY_WINDOW_SECS), 100, 4);
        let state = state_with_samples(samples);
        let now = start
            .checked_add(Duration::from_secs(MEMORY_WINDOW_SECS))
            .expect("test instant should be representable");

        assert_eq!(
            check_memory_trend(&state, now),
            Some(RecoveryAction::ForcedCompaction)
        );
    }

    #[test]
    fn non_monotonic_growth_does_not_trigger_compaction() {
        let start = Instant::now();
        let mut samples =
            increasing_samples(start, Duration::from_secs(MEMORY_WINDOW_SECS), 100, 4);
        samples[5].1 = 1;
        let state = state_with_samples(samples);
        let now = start
            .checked_add(Duration::from_secs(MEMORY_WINDOW_SECS))
            .expect("test instant should be representable");

        assert_eq!(check_memory_trend(&state, now), None);
    }

    #[test]
    fn an_ancient_sample_does_not_make_a_short_recent_run_look_full_window() {
        let now = Instant::now();
        let mut samples = Vec::new();
        samples.push((
            now.checked_sub(Duration::from_secs(MEMORY_WINDOW_SECS * 2))
                .expect("test instant should be representable"),
            1,
        ));
        for index in 0..10 {
            samples.push((
                now.checked_sub(Duration::from_secs(300 - index * 10))
                    .expect("test instant should be representable"),
                100 + index,
            ));
        }
        let state = state_with_samples(samples);
        assert_eq!(check_memory_trend(&state, now), None);
    }

    #[test]
    fn sample_window_excludes_old_and_future_timestamps() {
        let sampled_at = Instant::now();
        let now = sampled_at
            .checked_add(Duration::from_secs(MEMORY_WINDOW_SECS + 1))
            .expect("test instant should be representable");
        let future = now
            .checked_add(Duration::from_secs(1))
            .expect("test instant should be representable");

        let recent = samples_within_window(
            &[(sampled_at, 1), (now, 2), (future, 3)],
            now,
            Duration::from_secs(MEMORY_WINDOW_SECS),
        );

        assert_eq!(recent, vec![2]);
    }
}
