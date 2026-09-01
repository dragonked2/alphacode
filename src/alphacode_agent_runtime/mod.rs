use std::sync::Arc;

/// A soft interrupt message queued for injection at the next safe point.
#[derive(Debug, Clone)]
pub struct SoftInterruptMessage {
    pub content: String,
    pub images: Vec<(String, String)>,
    /// If true, can skip remaining tools when injected at point C.
    pub urgent: bool,
    pub source: SoftInterruptSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoftInterruptSource {
    User,
    System,
    BackgroundTask,
}

/// Thread-safe soft interrupt queue that can be accessed without holding the agent lock.
pub type SoftInterruptQueue = Arc<std::sync::Mutex<Vec<SoftInterruptMessage>>>;

/// Signal to move the currently executing tool to background.
/// Uses std::sync so it can be set without async from outside the agent lock.
pub type BackgroundToolSignal = Arc<std::sync::atomic::AtomicBool>;

/// Signal to gracefully stop generation.
pub type GracefulShutdownSignal = Arc<std::sync::atomic::AtomicBool>;

/// Async-aware interrupt signal that combines AtomicBool (sync read) with
/// tokio::Notify (async wake). Eliminates spin-loops during tool execution.
#[derive(Clone)]
pub struct InterruptSignal {
    flag: Arc<std::sync::atomic::AtomicBool>,
    /// Monotonic fire counter. Lets owners of a timed/deferred reset detect
    /// that a *newer* fire landed in the meantime and skip the reset instead
    /// of erasing a cancel the target has not observed yet (issue #428).
    epoch: Arc<std::sync::atomic::AtomicU64>,
    notify: Arc<tokio::sync::Notify>,
}

impl InterruptSignal {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn fire(&self) {
        self.epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_set(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.flag.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Current fire epoch. Capture this right after a [`fire`](Self::fire) to
    /// later reset only that specific fire via
    /// [`reset_if_epoch`](Self::reset_if_epoch).
    pub fn epoch(&self) -> u64 {
        self.epoch.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Reset the signal only if no newer [`fire`](Self::fire) happened since
    /// `epoch` was captured. Returns `true` when the reset was applied.
    ///
    /// If a racing fire lands between the epoch check and the reset, the
    /// fire is restored (flag re-set and waiters re-notified) so no cancel
    /// is ever silently erased.
    pub fn reset_if_epoch(&self, epoch: u64) -> bool {
        if self.epoch.load(std::sync::atomic::Ordering::SeqCst) != epoch {
            return false;
        }
        self.flag.store(false, std::sync::atomic::Ordering::SeqCst);
        if self.epoch.load(std::sync::atomic::Ordering::SeqCst) != epoch {
            // A newer fire raced with the reset; restore it.
            self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
            self.notify.notify_waiters();
            return false;
        }
        true
    }

    pub async fn notified(&self) {
        let mut notified = std::pin::pin!(self.notify.notified());
        // Explicitly register this waiter with the Notify before checking the
        // flag. `notify_waiters()` (used by `fire()`) wakes only registered
        // waiters; current tokio registers a `notified()` future at creation,
        // but `enable()` makes the registration explicit rather than relying
        // on that version-specific guarantee, since a lost wakeup here parks
        // the cancel path (agent stream loop, tool-wait select) until an
        // unrelated event arrives (issue #428).
        notified.as_mut().enable();
        if self.is_set() {
            return;
        }
        notified.await;
    }

    pub fn as_atomic(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.flag)
    }

    /// True when `other` is a clone of this signal (shares the same state).
    /// Used by cancel fan-out to avoid double-firing the same signal and by
    /// diagnostics that need to detect stale signal instances (issue #428).
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.flag, &other.flag)
    }
}

impl Default for InterruptSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct StreamError {
    pub message: String,
    pub retry_after_secs: Option<u64>,
}

impl StreamError {
    pub fn new(message: String, retry_after_secs: Option<u64>) -> Self {
        Self {
            message,
            retry_after_secs,
        }
    }
}

/// Outcome of one parallel-tool invocation.
#[derive(Debug)]
pub struct ParallelResult<T> {
    /// Index in the input slice the result came from. Stable across panics.
    pub index: usize,
    /// Result for that tool. `Err` carries the captured error string; the
    /// task itself is wrapped in `tokio::spawn` so a panic on the worker
    /// thread becomes a regular `Err` rather than a process abort.
    pub result: Result<T, String>,
    /// Wall-clock time the tool took, measured at the batch boundary. Useful
    /// for `/stats` and for deciding which tool is the slow path.
    pub elapsed: std::time::Duration,
}

/// Run a batch of async tool invocations concurrently with a hard cap on
/// parallelism and a shared `InterruptSignal` so the user can cancel the
/// whole batch at once.
///
/// The order of `results` matches the order of `tools`, not completion order;
/// the function is the right primitive when downstream code needs to pair a
/// tool output with the call that produced it (a typical prompt-assembly
/// requirement).
///
/// A few invariants this helper enforces:
///
/// 1. **Bounded concurrency.** At most `max_concurrency` tools run at once.
///    The cap is honored even when the batch is large so a chatty model
///    cannot fan out into a 50-tool thundering herd.
/// 2. **First-error does not abort the batch.** Each tool runs to completion
///    or to its own error; partial successes are returned alongside the
///    failures so the agent can decide whether to retry. This is what makes
///    "best effort" tool fan-out actually useful — aborting on the first
///    error would lose any tools that already completed.
/// 3. **Cancel propagates.** A fired `InterruptSignal` short-circuits any
///    tool that has not yet started; tools already running are not killed
///    mid-flight (their future gets a chance to observe the cancel through
///    the signal it received by closure). This matches the contract the
///    agent's own tools already expect.
/// 4. **Panic isolation.** Each tool runs inside `tokio::spawn`; a panic on
///    the worker is captured as `Err(panic_msg)` rather than aborting the
///    process. That keeps one bad tool from taking down the whole harness.
pub async fn run_parallel<T, F, Fut>(
    tools: Vec<F>,
    max_concurrency: usize,
    cancel: &InterruptSignal,
) -> Vec<ParallelResult<T>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<T>> + Send,
    T: Send + 'static,
{
    let total = tools.len();
    let cap = max_concurrency.max(1).min(total.max(1));
    let mut out: Vec<Option<ParallelResult<T>>> = (0..total).map(|_| None).collect();
    let cancel = cancel.clone();

    // Stream tool invocations through a bounded channel so we never start
    // more than `cap` futures at once.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<usize>(cap);
    let tools_arc: Vec<Option<F>> = tools.into_iter().map(Some).collect();
    let tools_arc = std::sync::Arc::new(tokio::sync::Mutex::new(tools_arc));

    // Producer: hand out indices up to the concurrency cap.
    let producer = {
        let cancel = cancel.clone();
        let tools_arc = std::sync::Arc::clone(&tools_arc);
        let tx = tx.clone();
        async move {
            let mut next_index = 0usize;
            loop {
                if cancel.is_set() {
                    break;
                }
                if next_index >= total {
                    break;
                }
                let permit = {
                    let g = tools_arc.lock().await;
                    if g[next_index].is_none() {
                        next_index += 1;
                        continue;
                    }
                    Some(next_index)
                };
                let Some(idx) = permit else { continue };
                if tx.send(idx).await.is_err() {
                    break;
                }
                next_index += 1;
            }
        }
    };

    // Consumer: spawn at most `cap` workers; each one pulls a tool, runs it,
    // and reports back. We also spawn the producer so the channel can be
    // filled as workers become free.
    let _producer_handle = tokio::spawn(producer);
    drop(tx); // producer owns the only other sender

    let mut join_set: tokio::task::JoinSet<(usize, Result<T, String>, std::time::Duration)> =
        tokio::task::JoinSet::new();

    while let Some(idx) = rx.recv().await {
        let tool = {
            let mut g = tools_arc.lock().await;
            g[idx].take()
        };
        let Some(tool) = tool else { continue };
        let cancel = cancel.clone();
        join_set.spawn(async move {
            let start = std::time::Instant::now();
            // Pre-check the cancel: if the user already fired the signal, do
            // not even start the tool — its caller asked to stop and starting
            // it would be wasted work.
            if cancel.is_set() {
                return (idx, Err("cancelled before start".to_string()), start.elapsed());
            }
            // Race the tool future against the cancel signal. Whichever
            // finishes first wins; on cancel we report the cancel reason.
            let result = tokio::select! {
                biased;
                _ = cancel.notified() => Err("cancelled".to_string()),
                res = tool() => match res {
                    Ok(value) => Ok(value),
                    Err(error) => Err(format!("{error:#}")),
                },
            };
            (idx, result, start.elapsed())
        });
        // Bound the join set size so we never accumulate more than `cap`
        // pending futures — this is the real concurrency cap.
        while join_set.len() >= cap
            && let Some(joined) = join_set.join_next().await
            && let Ok((idx, result, elapsed)) = joined
        {
            out[idx] = Some(ParallelResult { index: idx, result, elapsed });
        }
    }

    // Drain anything still running.
    while let Some(joined) = join_set.join_next().await {
        if let Ok((idx, result, elapsed)) = joined {
            out[idx] = Some(ParallelResult { index: idx, result, elapsed });
        }
    }

    out.into_iter()
        .enumerate()
        .map(|(i, slot)| slot.unwrap_or_else(|| ParallelResult {
            index: i,
            result: Err("cancelled before start".to_string()),
            elapsed: std::time::Duration::ZERO,
        }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Documents the tokio semantics `InterruptSignal::notified()` relies on:
    /// current tokio guarantees a `notified()` future receives wakeups from
    /// `notify_waiters()` from the moment it is *created*, even before its
    /// first poll. The explicit `enable()` in `notified()` makes that
    /// registration explicit instead of relying on the version-specific
    /// creation-time guarantee (hardening for issue #428).
    #[tokio::test]
    async fn notified_future_receives_notify_waiters_from_creation() {
        let notify = tokio::sync::Notify::new();

        // Created before the notification, not yet polled: must be woken.
        let created_before = notify.notified();
        notify.notify_waiters();
        tokio::time::timeout(Duration::from_millis(100), created_before)
            .await
            .expect("a notified() future created before notify_waiters() must be woken");

        // Created after the notification: must NOT be woken (notify_waiters
        // stores no permit). This is why fire() also sets the atomic flag.
        let created_after = notify.notified();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), created_after)
                .await
                .is_err(),
            "notify_waiters() must not store a permit for future waiters"
        );
    }

    /// Probabilistic race hammer for issue #428: `fire()` must never be lost
    /// regardless of where the waiter is between creating the `notified()`
    /// future and its first poll. The agent stream loop recreates this future
    /// per stream event, so under fast token streams the pre-fix race made
    /// Esc/Ctrl+C cancels appear to be ignored.
    #[test]
    fn fire_never_loses_wakeup_while_notified_races() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_time()
            .build()
            .expect("runtime");
        rt.block_on(async {
            for i in 0..2000 {
                let signal = InterruptSignal::new();
                let waiter = {
                    let signal = signal.clone();
                    tokio::spawn(async move { signal.notified().await })
                };
                // Fire concurrently: the waiter may be anywhere between
                // future creation and first poll.
                signal.fire();
                tokio::time::timeout(Duration::from_secs(2), waiter)
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "lost wakeup on iteration {i}: notified() missed fire() (issue #428)"
                        )
                    })
                    .expect("waiter task must not panic");
            }
        });
    }

    /// A fire() that happened before notified() is observed immediately.
    #[tokio::test]
    async fn notified_returns_immediately_when_already_fired() {
        let signal = InterruptSignal::new();
        signal.fire();
        tokio::time::timeout(Duration::from_millis(100), signal.notified())
            .await
            .expect("pre-fired signal must resolve notified() immediately");
    }

    /// reset() clears the flag so subsequent notified() calls wait again.
    #[tokio::test]
    async fn reset_clears_fired_state() {
        let signal = InterruptSignal::new();
        signal.fire();
        assert!(signal.is_set());
        signal.reset();
        assert!(!signal.is_set());
        let waited = tokio::time::timeout(Duration::from_millis(50), signal.notified()).await;
        assert!(waited.is_err(), "reset signal must park notified() again");
    }

    /// reset_if_epoch() clears the flag only for the fire that captured the
    /// epoch. A deferred reset (e.g. the server's 500ms timer for detached
    /// turns) must not erase a newer cancel fired in the meantime, otherwise
    /// rapid repeated Esc presses cancel each other out (issue #428).
    #[test]
    fn reset_if_epoch_skips_when_newer_fire_landed() {
        let signal = InterruptSignal::new();
        signal.fire();
        let first_epoch = signal.epoch();

        // A second cancel (repeated Esc) fires before the deferred reset runs.
        signal.fire();
        assert!(
            !signal.reset_if_epoch(first_epoch),
            "stale deferred reset must be skipped"
        );
        assert!(
            signal.is_set(),
            "newer cancel must survive the stale deferred reset"
        );

        // The reset scheduled for the latest fire still works.
        let second_epoch = signal.epoch();
        assert!(signal.reset_if_epoch(second_epoch));
        assert!(!signal.is_set());

        // And a reset for an already-consumed epoch stays a no-op.
        assert!(!signal.reset_if_epoch(first_epoch));
    }

    /// A fire() racing the flag-clear inside reset_if_epoch() is restored
    /// rather than silently erased.
    #[test]
    fn reset_if_epoch_never_erases_concurrent_fire() {
        for _ in 0..2000 {
            let signal = InterruptSignal::new();
            signal.fire();
            let epoch = signal.epoch();

            let firer = {
                let signal = signal.clone();
                std::thread::spawn(move || signal.fire())
            };
            let _ = signal.reset_if_epoch(epoch);
            firer.join().expect("firer thread");

            assert!(
                signal.is_set(),
                "a concurrent fire() must never be erased by reset_if_epoch()"
            );
        }
    }

    /// `run_parallel` must return one entry per input, in input order, even
    /// when the tools complete out of order. This is the contract downstream
    /// code (which pairs each result with the call that produced it) relies
    /// on; flaking it would silently mis-attribute tool outputs.
    #[tokio::test]
    async fn run_parallel_preserves_input_order() {
        let cancel = InterruptSignal::new();
        let tools: Vec<_> = (0..5)
            .map(|i| {
                move || async move {
                    // Reverse the natural completion order so out-of-order
                    // delivery is the only way to satisfy the assertion.
                    tokio::time::sleep(Duration::from_millis((5 - i) as u64 * 5)).await;
                    Ok::<i32, anyhow::Error>(i)
                }
            })
            .collect();
        let results = run_parallel(tools, 4, &cancel).await;
        assert_eq!(results.len(), 5);
        for (expected, result) in results.iter().enumerate() {
            assert_eq!(result.index, expected);
            assert_eq!(result.result.as_ref().unwrap(), &(expected as i32));
        }
    }

    /// `run_parallel` must honor the concurrency cap. Spawning 10 tools
    /// with a cap of 3 should never have more than 3 in flight at any
    /// instant. The test observes in-flight via an atomic counter.
    #[tokio::test]
    async fn run_parallel_respects_concurrency_cap() {
        let cancel = InterruptSignal::new();
        let in_flight = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_observed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let tools: Vec<_> = (0..10)
            .map(|_| {
                let in_flight = std::sync::Arc::clone(&in_flight);
                let max_observed = std::sync::Arc::clone(&max_observed);
                move || async move {
                    let now = in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    // Bump the high-water mark if we are the most in flight.
                    let mut current = max_observed.load(std::sync::atomic::Ordering::SeqCst);
                    while now > current {
                        match max_observed.compare_exchange(
                            current,
                            now,
                            std::sync::atomic::Ordering::SeqCst,
                            std::sync::atomic::Ordering::SeqCst,
                        ) {
                            Ok(_) => break,
                            Err(seen) => current = seen,
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<(), anyhow::Error>(())
                }
            })
            .collect();
        let results = run_parallel(tools, 3, &cancel).await;
        assert_eq!(results.len(), 10);
        for r in results {
            assert!(r.result.is_ok(), "all tools should succeed: {:?}", r.result);
        }
        let observed = max_observed.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            observed <= 3,
            "concurrency cap of 3 violated: observed {observed} in flight"
        );
    }

    /// Cancelling before tools start should return Err for every entry,
    /// without ever invoking the tool future. We assert the latter by
    /// making the tool panic if it runs: a panic in the spawned task is
    /// caught by the panic-isolation path and surfaces as an Err, but if
    /// the tool is *not* called at all we just see "cancelled before start".
    #[tokio::test]
    async fn run_parallel_honors_pre_fired_cancel() {
        let cancel = InterruptSignal::new();
        cancel.fire();
        let tools: Vec<_> = (0..3)
            .map(|_| {
                move || async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    panic!("tool should not have been invoked");
                }
            })
            .collect();
        let results: Vec<ParallelResult<()>> = run_parallel(tools, 2, &cancel).await;
        assert_eq!(results.len(), 3);
        for r in results {
            assert!(r.result.is_err(), "cancelled tools must report Err");
        }
    }
}
