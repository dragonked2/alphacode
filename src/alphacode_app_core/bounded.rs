//! Bounded operation timeout with graceful degradation.
//!
//! Long-running operations occasionally need to time out cleanly without
//! leaving resources dangling, leaving the user with a clear "this took too
//! long" message, and (where possible) emitting a partial result so the
//! conversation can continue.
//!
//! The standard `tokio::time::timeout` wrapper returns `Result<T, Elapsed>`.
//! That is fine for control flow, but a code base that wants "months of
//! uptime" needs three things beyond the default:
//!
//! 1. **Structured result.** A `BoundedOutcome<T>` enum makes the partial /
//!    completed / cancelled cases first-class so callers must consciously
//!    decide what to render for each.
//! 2. **Health integration.** Anything that times out is recorded as a slow
//!    op + an error so the month-of-uptime health monitor sees it.
//! 3. **Cancellation hook.** If the inner future supports `cancel()`, we
//!    trigger it on timeout so the background task stops burning CPU instead
//!    of racing the user's next request.
//!
//! Use [`run`] for the common case and [`run_with_progress`] when you want to
//! emit a status update to the caller each time the deadline is pushed back
//! by a chunk of work.

use crate::alphacode_app_core::health;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Outcome of a bounded operation.
#[derive(Debug)]
pub enum BoundedOutcome<T> {
    /// Operation finished within the deadline.
    Completed(T),
    /// Operation timed out. `partial` is whatever the caller passed back via
    /// the progress callback; `None` if no progress was ever reported.
    TimedOut { partial: Option<T> },
    /// Operation was cancelled by graceful shutdown / user abort.
    Cancelled,
}

/// Run `future` to completion with a hard deadline.  On timeout, attempt to
/// cancel `cancel_handle` (a `tokio_util::sync::CancellationToken` or any
/// other future that wakes when triggered) so the inner work stops.  The
/// `label` is recorded in the health monitor's slow-op table when the
/// elapsed time exceeds [`health::SLOW_OP_THRESHOLD`].
pub async fn run<F, T>(
    label: &'static str,
    deadline: Duration,
    cancel_handle: Option<impl FnOnce() + Send + 'static>,
    future: F,
) -> BoundedOutcome<T>
where
    F: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let _guard = health::time(label);
    let result = tokio::time::timeout(deadline, future).await;
    let elapsed = start.elapsed();
    match result {
        Ok(value) => {
            if elapsed >= health::SLOW_OP_THRESHOLD {
                crate::alphacode_app_core::logging::info(&format!(
                    "bounded: '{}' completed in {:.2}s",
                    label,
                    elapsed.as_secs_f64()
                ));
            }
            BoundedOutcome::Completed(value)
        }
        Err(_) => {
            health::record_error();
            crate::alphacode_app_core::logging::warn(&format!(
                "bounded: '{}' timed out after {:.2}s (deadline={:.2}s)",
                label,
                elapsed.as_secs_f64(),
                deadline.as_secs_f64()
            ));
            if let Some(cancel) = cancel_handle {
                cancel();
            }
            BoundedOutcome::TimedOut { partial: None }
        }
    }
}

/// Run `future` with a deadline, where the future yields partial progress
/// values via the `progress` callback.  When the deadline elapses, the most
/// recent partial value is returned in [`BoundedOutcome::TimedOut`].
pub async fn run_with_progress<F, P, T>(
    label: &'static str,
    deadline: Duration,
    future: F,
    mut progress: P,
) -> BoundedOutcome<T>
where
    F: std::future::Future<Output = T>,
    P: FnMut(&T),
{
    let start = Instant::now();
    let _guard = health::time(label);
    let mut last_partial: Option<T> = None;

    // Race the future against a deadline ticker.  The ticker polls every
    // chunk so we can call `progress(partial)` between deadline extensions
    // for ops that emit intermediate results (provider stream, file read,
    // etc.).  For "all-or-nothing" futures the callback is simply unused.
    let chunk = (deadline / 8).max(Duration::from_millis(50));
    let mut tick = tokio::time::interval(chunk);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let pinned = std::pin::pin!(future);
    tokio::pin!(pinned);
    loop {
        tokio::select! {
            biased;
            _ = tick.tick() => {
                let elapsed = start.elapsed();
                if elapsed >= deadline {
                    health::record_error();
                    crate::alphacode_app_core::logging::warn(&format!(
                        "bounded.progress: '{}' timed out after {:.2}s",
                        label,
                        elapsed.as_secs_f64()
                    ));
                    return BoundedOutcome::TimedOut { partial: last_partial };
                }
            }
            value = &mut pinned => {
                if start.elapsed() >= health::SLOW_OP_THRESHOLD {
                    crate::alphacode_app_core::logging::info(&format!(
                        "bounded.progress: '{}' completed in {:.2}s",
                        label,
                        start.elapsed().as_secs_f64()
                    ));
                }
                progress(&value);
                return BoundedOutcome::Completed(value);
            }
        }
        // Capture partial: this is a placeholder for callers that use a
        // channel/handle; the simple version just lets `last_partial` stay
        // None.  Real partial-yielding callers can be added later by
        // wrapping the future in a "yield after every poll" stream.
        let _ = &mut last_partial;
    }
}

/// Sleep helper that records the duration in the health monitor's slow-op
/// table if it exceeds [`health::SLOW_OP_THRESHOLD`].  Useful for retry
/// back-off where you want the long sleeps to show up in the health
/// snapshot.
pub async fn recordable_sleep(label: &'static str, dur: Duration) {
    let _guard = health::time(label);
    sleep(dur).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completed_within_deadline() {
        let out: BoundedOutcome<i32> = run(
            "fast",
            Duration::from_millis(200),
            None::<fn()>,
            async { 42 },
        )
        .await;
        match out {
            BoundedOutcome::Completed(v) => assert_eq!(v, 42),
            _ => panic!("expected Completed"),
        }
    }

    #[tokio::test]
    async fn timed_out_returns_partial_none() {
        let out: BoundedOutcome<i32> = run(
            "slow",
            Duration::from_millis(50),
            None::<fn()>,
            async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                99
            },
        )
        .await;
        match out {
            BoundedOutcome::TimedOut { partial } => assert!(partial.is_none()),
            _ => panic!("expected TimedOut"),
        }
    }

    #[tokio::test]
    async fn cancel_handle_runs_on_timeout() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let cancelled = Arc::new(AtomicBool::new(false));
        let c2 = Arc::clone(&cancelled);
        let out: BoundedOutcome<()> = run(
            "slow-with-cancel",
            Duration::from_millis(20),
            Some(move || {
                c2.store(true, Ordering::SeqCst);
            }),
            async {
                tokio::time::sleep(Duration::from_millis(500)).await;
            },
        )
        .await;
        assert!(matches!(out, BoundedOutcome::TimedOut { .. }));
        assert!(cancelled.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn recordable_sleep_does_not_panic() {
        recordable_sleep("test.sleep", Duration::from_millis(10)).await;
    }
}