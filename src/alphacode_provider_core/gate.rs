//! Adaptive process-global gate for outbound provider requests.
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
//!      **only while another request is already in flight**. A lone session
//!      issues its requests one at a time (seconds apart), so it never waits: the
//!      pacing cost is paid exclusively by concurrent bursts, which is exactly
//!      where burst limits bite.
//!   3. **Swarm-wide cooldown** — when any request observes a 429, every *new*
//!      request start pauses briefly, so the retry wave backs off together
//!      instead of re-tripping the limit it just hit.
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
//! | `ALPHACODE_PROVIDER_MAX_INFLIGHT` | hardware-derived | Max concurrent request initiations |
//! | `ALPHACODE_PROVIDER_MIN_INTERVAL_MS` | `150` | Min spacing between starts *under contention* |

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

/// Minimum spacing applied between request starts while another request is in
/// flight. Only paid under contention — see the module docs.
const DEFAULT_MIN_INTERVAL_MS: u64 = 150;

/// How long all new request starts pause after any request observes a 429.
/// Long enough to break a retry storm, short enough not to stall recovery.
const RATE_LIMIT_COOLDOWN_MS: u64 = 2_000;

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
    gate: &'static Gate,
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

static GATE: OnceLock<Gate> = OnceLock::new();

/// Default concurrency when the env var is unset.
///
/// Kept modest: most accounts allow a small handful of concurrent streams, and
/// the adaptive controller raises throughput back toward the configured ceiling
/// as successes accumulate, so starting low costs little and avoids the initial
/// 429.
fn default_max_in_flight() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    cores.clamp(2, 8)
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

fn gate() -> &'static Gate {
    GATE.get_or_init(|| {
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
    })
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
pub async fn acquire() -> GatePermit {
    let gate = gate();
    loop {
        // 1. Honor any active 429 cooldown before competing for a slot.
        let cooldown = {
            let state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
            state
                .cooldown_until
                .saturating_duration_since(Instant::now())
        };
        if !cooldown.is_zero() {
            tokio::time::sleep(cooldown).await;
            continue;
        }

        // 2. Try to claim a slot. Pacing is applied only under contention so a
        //    lone session never waits.
        let pace_delay = {
            let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.in_flight < state.effective_max {
                let contended = state.in_flight > 0;
                let since = state.last_start.elapsed();
                if contended && since < gate.min_interval {
                    Some(gate.min_interval - since)
                } else {
                    state.in_flight += 1;
                    state.last_start = Instant::now();
                    return GatePermit { gate };
                }
            } else {
                None
            }
        };

        // 3. No slot (or pacing): wait, then retry the whole decision.
        match pace_delay {
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
    let gate = gate();
    {
        let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
        let candidate = Instant::now() + Duration::from_millis(RATE_LIMIT_COOLDOWN_MS);
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
    let gate = gate();
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
            let contended = state.in_flight > 0;
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
}
