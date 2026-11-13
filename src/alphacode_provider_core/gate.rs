//! Adaptive per-origin gate shared across the process for outbound provider requests.
//!
//! # Why this exists
//!
//! A swarm runs many agent loops in one process, and every loop issues its own
//! provider HTTP requests. `agents.swarm_max_concurrent_agents` bounds how many
//! *workers* exist, but nothing bounded how many *requests* they put on the wire
//! at once. With 32 live workers that is 32+ simultaneous requests — which the
//! upstream answers with HTTP 429, after which each worker retries independently
//! and the retry storm re-trips the same limit.
//!
//! This module bounds provider load regardless of worker count while staying out
//! of the way when there is no contention:
//!
//!   1. **Adaptive concurrency** — at most `effective_max` requests may be
//!      awaiting response headers at once. `effective_max` starts at the
//!      configured cap and *halves on sustained 429s*, then climbs back one step
//!      at a time as requests succeed (AIMD). It tunes itself to whatever the
//!      account actually tolerates instead of a hard-coded number.
//!   2. **Contention-only pacing** — request starts are spaced by `min_interval`
//!      **while another request is in flight or after an observed capacity limit**. A lone session
//!      issues its requests one at a time (seconds apart), so it never waits: the
//!      pacing cost is paid exclusively by concurrent bursts, which is exactly
//!      where burst limits bite.
//!   3. **Swarm-wide cooldown** — when any request observes a 429, every *new*
//!      request start pauses briefly, so the retry wave backs off together
//!      instead of re-tripping the limit it just hit.
//!
//! Rolling rate budgets and cross-process cooldowns are additionally enforced
//! by `request_budget` at the shared HTTP transport boundary.
//!
//! The permit covers only the wait for response headers (the request
//! *initiation*), not the streaming body: long legitimate streams must not hold
//! a concurrency slot for minutes, and 429s are about burst request rate, not
//! stream duration.
//!
//! # Tuning
//!
//! | Env var | Default | Meaning |
//! |---|---|---|
//! | `ALPHACODE_PROVIDER_MAX_INFLIGHT` | `2` | Max concurrent request initiations |
//! | `ALPHACODE_PROVIDER_MIN_INTERVAL_MS` | `150` | Min spacing between starts *under contention* |

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

/// Minimum spacing applied between request starts while another request is in
/// flight. Only paid under contention — see the module docs.
const DEFAULT_MIN_INTERVAL_MS: u64 = 150;

/// How long all new request starts pause after any request observes a 429.
/// Long enough to break a retry storm, short enough not to stall recovery.
const RATE_LIMIT_COOLDOWN_MS: u64 = 5_000;

/// Consecutive 429s (with no success between them) required before concurrency
/// is halved. Avoids collapsing to 1 off a single blip.
const BACKOFF_THRESHOLD: u32 = 2;

/// Consecutive successes required before concurrency climbs back by one step.
const RECOVERY_THRESHOLD: u32 = 8;

/// Upper bound on the configured cap, so a bad env value cannot remove the
/// guard entirely.
const MAX_CONFIGURED_INFLIGHT: usize = 64;

struct State {
    /// Requests currently awaiting response headers.
    in_flight: usize,
    /// Current adaptive cap. Never exceeds `configured_max`, never below 1.
    effective_max: usize,
    /// Start of the last admitted request (for contention pacing).
    last_start: Instant,
    /// New request starts are held until this deadline (set on a 429).
    cooldown_until: Instant,
    /// Consecutive 429s since the last success.
    consecutive_limits: u32,
    /// Consecutive successes since the last 429.
    consecutive_ok: u32,
    /// Lifetime diagnostics.
    total_rate_limit_hits: u64,
}

struct Gate {
    configured_max: usize,
    min_interval: Duration,
    state: Mutex<State>,
    /// Woken when a slot frees or the concurrency cap grows.
    notify: Notify,
}

/// RAII guard held while awaiting response headers. Dropping it frees the slot.
pub struct GatePermit {
    gate: Arc<Gate>,
}

impl Drop for GatePermit {
    fn drop(&mut self) {
        {
            let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
            state.in_flight = state.in_flight.saturating_sub(1);
        }
        self.gate.notify.notify_waiters();
    }
}

static GATE: OnceLock<Arc<Gate>> = OnceLock::new();
static ORIGIN_GATES: OnceLock<Mutex<HashMap<url::Origin, Arc<Gate>>>> = OnceLock::new();

/// Default concurrency when the env var is unset.
///
/// Kept modest: most accounts allow a small handful of concurrent streams, and
/// the adaptive controller raises throughput back toward the configured ceiling
/// as successes accumulate, so starting low costs little and avoids the initial
/// 429.
fn default_max_in_flight() -> usize {
    2
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(MAX_CONFIGURED_INFLIGHT))
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn new_gate() -> Gate {
    let configured_max = env_usize("ALPHACODE_PROVIDER_MAX_INFLIGHT", default_max_in_flight());
    let min_interval = Duration::from_millis(env_u64(
        "ALPHACODE_PROVIDER_MIN_INTERVAL_MS",
        DEFAULT_MIN_INTERVAL_MS,
    ));
    let now = Instant::now();
    Gate {
        configured_max,
        min_interval,
        state: Mutex::new(State {
            in_flight: 0,
            effective_max: configured_max,
            last_start: now.checked_sub(min_interval).unwrap_or(now),
            cooldown_until: now,
            consecutive_limits: 0,
            consecutive_ok: 0,
            total_rate_limit_hits: 0,
        }),
        notify: Notify::new(),
    }
}

fn gate() -> Arc<Gate> {
    GATE.get_or_init(|| Arc::new(new_gate())).clone()
}

fn origin_gate(origin: &url::Origin) -> Arc<Gate> {
    let mut gates = ORIGIN_GATES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(gate) = gates.get(origin) {
        return gate.clone();
    }
    // Reclaim idle origins before using the bounded overflow gate. Keep
    // active cooldowns so moving to another path cannot erase a rate limit.
    if gates.len() >= 256 {
        let now = Instant::now();
        gates.retain(|_, gate| {
            Arc::strong_count(gate) > 1
                || gate
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cooldown_until
                    > now
        });
        if gates.len() >= 256 {
            return gate();
        }
    }
    let gate = Arc::new(new_gate());
    gates.insert(origin.clone(), gate.clone());
    gate
}

/// Isolate admission/cooldowns by endpoint origin, so switching providers works.
pub async fn acquire_for_origin(origin: &url::Origin) -> anyhow::Result<GatePermit> {
    acquire_on(origin_gate(origin)).await
}

pub async fn note_rate_limited_for_origin(origin: &url::Origin, hint: Option<Duration>) {
    note_limit(&origin_gate(origin), hint);
}

pub fn note_success_for_origin(origin: &url::Origin) {
    note_ok(&origin_gate(origin));
}

/// The configured concurrency ceiling (diagnostics / status surfaces).
pub fn max_in_flight() -> usize {
    gate().configured_max
}

/// The current adaptive cap (diagnostics / status surfaces).
pub fn effective_max_in_flight() -> usize {
    let gate = gate();
    let state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
    state.effective_max
}

/// Acquire permission to start one provider request.
///
/// The returned guard must be held until response headers arrive (or the attempt
/// fails), then dropped. It must **not** be held for the streaming body.
pub async fn acquire() -> anyhow::Result<GatePermit> {
    acquire_on(gate()).await
}

async fn acquire_on(gate: Arc<Gate>) -> anyhow::Result<GatePermit> {
    loop {
        // 1. Honor any active 429 cooldown before competing for a slot.
        let cooldown = {
            let state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
            state
                .cooldown_until
                .saturating_duration_since(Instant::now())
        };
        if !cooldown.is_zero() {
            if cooldown > super::retry_after::MAX_AUTOMATIC_WAIT {
                return Err(super::request_budget::deferred_error(cooldown));
            }
            tokio::time::sleep(cooldown).await;
            continue;
        }

        // 2. Try to claim a slot. Pacing is applied only under contention so a
        //    lone session never waits.
        let pace_delay = {
            let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
            let remaining_cooldown = state
                .cooldown_until
                .saturating_duration_since(Instant::now());
            if !remaining_cooldown.is_zero() {
                Some(remaining_cooldown)
            } else if state.in_flight < state.effective_max {
                let contended = state.in_flight > 0 || state.total_rate_limit_hits > 0;
                let since = state.last_start.elapsed();
                if contended && since < gate.min_interval {
                    Some(gate.min_interval - since)
                } else {
                    state.in_flight += 1;
                    state.last_start = Instant::now();
                    return Ok(GatePermit { gate: gate.clone() });
                }
            } else {
                None
            }
        };

        // 3. No slot (or pacing): wait, then retry the whole decision.
        match pace_delay {
            Some(delay) if delay > super::retry_after::MAX_AUTOMATIC_WAIT => {
                return Err(super::request_budget::deferred_error(delay));
            }
            Some(delay) => tokio::time::sleep(delay).await,
            // Bounded wait so a missed wake cannot stall a worker indefinitely.
            None => {
                let _ =
                    tokio::time::timeout(Duration::from_millis(100), gate.notify.notified()).await;
            }
        }
    }
}

/// Record that a request observed HTTP 429.
///
/// Pauses every new request start for [`RATE_LIMIT_COOLDOWN_MS`] and, after
/// [`BACKOFF_THRESHOLD`] consecutive limits, halves the adaptive concurrency so
/// the swarm stops offering more parallelism than the account will take.
pub async fn note_rate_limited() {
    note_limit(&gate(), None);
}

fn note_limit(gate: &Gate, hint: Option<Duration>) {
    {
        let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
        let fallback_ms = RATE_LIMIT_COOLDOWN_MS
            .saturating_mul(1_u64 << state.consecutive_limits.min(4))
            .min(60_000);
        let delay = hint
            .unwrap_or(Duration::from_millis(fallback_ms))
            .min(super::retry_after::MAX_RETRY_AFTER);
        let candidate = Instant::now() + delay;
        if candidate > state.cooldown_until {
            state.cooldown_until = candidate;
        }
        state.total_rate_limit_hits += 1;
        state.consecutive_ok = 0;
        state.consecutive_limits = state.consecutive_limits.saturating_add(1);
        if state.consecutive_limits >= BACKOFF_THRESHOLD && state.effective_max > 1 {
            state.effective_max = (state.effective_max / 2).max(1);
        }
    }
    gate.notify.notify_waiters();
}

/// Record that a request completed without a rate limit.
///
/// Climbing back is deliberately slow (one step per [`RECOVERY_THRESHOLD`]
/// successes) so a swarm cannot oscillate into the limit it just backed off from.
pub fn note_success() {
    note_ok(&gate());
}

fn note_ok(gate: &Gate) {
    let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
    state.consecutive_limits = 0;
    state.consecutive_ok = state.consecutive_ok.saturating_add(1);
    if state.consecutive_ok >= RECOVERY_THRESHOLD && state.effective_max < gate.configured_max {
        state.effective_max += 1;
        state.consecutive_ok = 0;
        drop(state);
        gate.notify.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an isolated gate so tests never contend on (or drain) the shared
    /// process-global pool while other tests run in parallel.
    fn local_gate(configured_max: usize, min_interval_ms: u64) -> Gate {
        let min_interval = Duration::from_millis(min_interval_ms);
        let now = Instant::now();
        Gate {
            configured_max,
            min_interval,
            state: Mutex::new(State {
                in_flight: 0,
                effective_max: configured_max,
                last_start: now.checked_sub(min_interval).unwrap_or(now),
                cooldown_until: now,
                consecutive_limits: 0,
                consecutive_ok: 0,
                total_rate_limit_hits: 0,
            }),
            notify: Notify::new(),
        }
    }

    /// Exercise the same decision logic `acquire` uses, against a local gate.
    fn try_claim(gate: &Gate) -> Option<Duration> {
        let mut state = gate.state.lock().unwrap();
        if state.in_flight < state.effective_max {
            let contended = state.in_flight > 0 || state.total_rate_limit_hits > 0;
            let since = state.last_start.elapsed();
            if contended && since < gate.min_interval {
                Some(gate.min_interval - since)
            } else {
                state.in_flight += 1;
                state.last_start = Instant::now();
                Some(Duration::ZERO)
            }
        } else {
            None
        }
    }

    #[test]
    fn lone_session_is_never_paced() {
        // A single in-flight request must not trigger pacing: in_flight == 0.
        let gate = local_gate(4, 200);
        assert_eq!(try_claim(&gate), Some(Duration::ZERO));
        assert_eq!(gate.state.lock().unwrap().in_flight, 1);
    }

    #[test]
    fn pacing_only_engages_under_contention() {
        let gate = local_gate(4, 200);
        assert_eq!(try_claim(&gate), Some(Duration::ZERO)); // first, no wait
        // Second start while the first is in flight is paced.
        let delay = try_claim(&gate).expect("slot available");
        assert!(
            delay > Duration::ZERO,
            "second concurrent start was not paced"
        );
    }

    #[test]
    fn concurrency_is_capped() {
        let gate = local_gate(2, 0);
        assert!(try_claim(&gate).is_some());
        assert!(try_claim(&gate).is_some());
        assert!(try_claim(&gate).is_none(), "third start exceeded the cap");
    }

    #[test]
    fn backoff_halves_then_recovers() {
        let gate = local_gate(8, 0);
        // Simulate BACKOFF_THRESHOLD consecutive limits with no success between.
        for _ in 0..BACKOFF_THRESHOLD {
            let mut state = gate.state.lock().unwrap();
            state.consecutive_ok = 0;
            state.consecutive_limits += 1;
            if state.consecutive_limits >= BACKOFF_THRESHOLD && state.effective_max > 1 {
                state.effective_max = (state.effective_max / 2).max(1);
            }
        }
        assert_eq!(gate.state.lock().unwrap().effective_max, 4);
        // Then a run of successes must grow it back, one step at a time.
        {
            let mut state = gate.state.lock().unwrap();
            state.consecutive_limits = 0;
            for _ in 0..RECOVERY_THRESHOLD {
                state.consecutive_ok += 1;
            }
            if state.consecutive_ok >= RECOVERY_THRESHOLD
                && state.effective_max < gate.configured_max
            {
                state.effective_max += 1;
            }
        }
        assert_eq!(gate.state.lock().unwrap().effective_max, 5);
    }

    #[test]
    fn effective_never_exceeds_configured_or_drops_below_one() {
        let gate = local_gate(3, 0);
        {
            let mut state = gate.state.lock().unwrap();
            for _ in 0..10 {
                if state.effective_max > 1 {
                    state.effective_max = (state.effective_max / 2).max(1);
                }
            }
        }
        assert_eq!(gate.state.lock().unwrap().effective_max, 1);
    }

    #[test]
    fn global_gate_is_configured() {
        let max = max_in_flight();
        assert!((1..=MAX_CONFIGURED_INFLIGHT).contains(&max));
        assert!(effective_max_in_flight() <= max);
        assert!(effective_max_in_flight() >= 1);
    }
    #[tokio::test]
    async fn actual_admission_waits_for_permit_release_and_survives_cancellation() {
        let gate = Arc::new(local_gate(1, 0));
        let first = acquire_on(gate.clone()).await.unwrap();
        let waiting = tokio::spawn(acquire_on(gate.clone()));
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        waiting.abort();
        let _ = waiting.await;
        assert_eq!(gate.state.lock().unwrap().in_flight, 1);
        drop(first);
        let next = tokio::time::timeout(Duration::from_secs(1), acquire_on(gate.clone()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(gate.state.lock().unwrap().in_flight, 1);
        drop(next);
        assert_eq!(gate.state.lock().unwrap().in_flight, 0);
    }

    #[test]
    fn missing_retry_hint_backs_off_progressively_and_reduces_concurrency() {
        let gate = local_gate(8, 0);
        note_limit(&gate, None);
        let first = gate.state.lock().unwrap().cooldown_until;
        assert!(first.saturating_duration_since(Instant::now()) >= Duration::from_secs(4));
        note_limit(&gate, None);
        let second = gate.state.lock().unwrap().cooldown_until;
        assert!(second > first + Duration::from_secs(4));
        assert_eq!(gate.state.lock().unwrap().effective_max, 4);
        for _ in 0..RECOVERY_THRESHOLD {
            note_ok(&gate);
        }
        assert_eq!(gate.state.lock().unwrap().effective_max, 5);
        // Successful peers must not clear a cooldown already promised to others.
        assert_eq!(gate.state.lock().unwrap().cooldown_until, second);
    }

    #[tokio::test]
    async fn actual_admission_honors_explicit_hint_even_without_contention() {
        let gate = Arc::new(local_gate(2, 0));
        note_limit(&gate, Some(Duration::from_millis(40)));
        let before = Instant::now();
        let _permit = acquire_on(gate).await.unwrap();
        assert!(before.elapsed() >= Duration::from_millis(35));
    }

    #[test]
    fn capacity_cooldowns_are_isolated_by_origin() {
        let first = url::Url::parse("https://rate-limited-gate-fixture.invalid")
            .unwrap()
            .origin();
        let second = url::Url::parse("https://healthy-gate-fixture.invalid")
            .unwrap()
            .origin();
        let first_gate = origin_gate(&first);
        let second_gate = origin_gate(&second);
        note_limit(&first_gate, Some(Duration::from_secs(30)));
        assert!(first_gate.state.lock().unwrap().cooldown_until > Instant::now());
        assert!(second_gate.state.lock().unwrap().cooldown_until <= Instant::now());
        assert!(!Arc::ptr_eq(&first_gate, &second_gate));
    }

    #[tokio::test]
    async fn long_cooldown_stops_admission_without_shortening_deadline() {
        let gate = Arc::new(local_gate(2, 0));
        note_limit(&gate, Some(Duration::from_secs(3541)));
        let error = acquire_on(gate.clone()).await.err().unwrap();
        assert!(super::super::retry_after::requires_manual_retry(&error));
        assert!(
            gate.state
                .lock()
                .unwrap()
                .cooldown_until
                .duration_since(Instant::now())
                > Duration::from_secs(3540)
        );
        assert_eq!(gate.state.lock().unwrap().in_flight, 0);
    }
}
