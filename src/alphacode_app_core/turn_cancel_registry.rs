//! Process-global registry of cancel signals for actively running turns.
//!
//! Why this exists (issue #428): a cancel (Esc) is delivered through whatever
//! `SessionControlHandle` the receiving connection happens to hold. That
//! handle's stop signal can be a *different* [`InterruptSignal`] instance from
//! the `graceful_shutdown` signal of the agent actually streaming the turn:
//!
//! - re-attach after a reload or disconnect where cleanup removed the
//!   `shutdown_signals` registration,
//! - server-initiated turns (`spawn_tracked_live_turn`, headless recovery,
//!   swarm wake delivery) running on an agent object the connection never
//!   locked,
//! - headless spawns that never registered a shutdown signal,
//! - the lock-free `cancel_only` fallback built while the agent mutex is busy.
//!
//! Firing a stale instance silently does nothing: the client shows
//! "Interrupting..." while the model keeps generating for minutes and every
//! extra Esc only stacks another `Interrupted` event ("Interrupted [x66]").
//!
//! Every turn now registers its own `graceful_shutdown` signal here for the
//! duration of the turn, and `SessionControlHandle::request_cancel` fires
//! every registered signal for the session in addition to its own handle, so
//! cancellation reaches the in-flight provider stream no matter which handle
//! instance received the request.

use crate::alphacode_agent_runtime::InterruptSignal;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
/// All interrupt signals registered for one session's in-flight turns.
type SessionTurnSignals = Vec<(u64, InterruptSignal)>;
static ACTIVE_TURNS: LazyLock<Mutex<HashMap<String, SessionTurnSignals>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// RAII registration for one running turn. Dropping the guard removes the
/// signal from the registry, so signals never outlive the turn that owns them.
///
/// Dropping also resets the signal: a cancel fired through the registry sets
/// the turn's own `graceful_shutdown` flag, and nothing else ever clears that
/// instance (the server's deferred epoch-guarded reset only touches the
/// control handle's signal). Without this, one interrupt would leave the flag
/// permanently set and instantly abort every subsequent turn on the agent.
pub struct ActiveTurnGuard {
    session_id: String,
    token: u64,
    signal: InterruptSignal,
}

/// Register `signal` as the cancel signal for a turn running in `session_id`.
/// Call at turn start; keep the guard alive for the duration of the turn.
pub fn register_active_turn(session_id: &str, signal: InterruptSignal) -> ActiveTurnGuard {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let active = match ACTIVE_TURNS.lock() {
        Ok(mut map) => {
            let entry = map.entry(session_id.to_string()).or_default();
            entry.push((token, signal.clone()));
            entry.len()
        }
        Err(_) => 0,
    };
    crate::logging::info(&format!(
        "TURN_CANCEL_REGISTERED session={} active_turns={}",
        session_id, active
    ));
    ActiveTurnGuard {
        session_id: session_id.to_string(),
        token,
        signal,
    }
}

/// All cancel signals currently registered for turns in `session_id`.
pub fn active_turn_signals(session_id: &str) -> Vec<InterruptSignal> {
    ACTIVE_TURNS
        .lock()
        .ok()
        .and_then(|map| {
            map.get(session_id)
                .map(|entries| entries.iter().map(|(_, signal)| signal.clone()).collect())
        })
        .unwrap_or_default()
}

impl Drop for ActiveTurnGuard {
    fn drop(&mut self) {
        let remaining = match ACTIVE_TURNS.lock() {
            Ok(mut map) => {
                let remaining = if let Some(entries) = map.get_mut(&self.session_id) {
                    entries.retain(|(token, _)| *token != self.token);
                    entries.len()
                } else {
                    0
                };
                if remaining == 0 {
                    map.remove(&self.session_id);
                }
                remaining
            }
            Err(_) => 0,
        };
        // A turn's cancel flag must never outlive the turn: if a cancel fired
        // this signal (possibly through a stale control handle that nothing
        // else ever resets), leaving it set would instantly abort the next
        // turn on this agent.
        self.signal.reset();
        crate::logging::info(&format!(
            "TURN_CANCEL_UNREGISTERED session={} active_turns={}",
            self.session_id, remaining
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_drop_tracks_active_signals() {
        let session_id = "turn_cancel_registry_register_drop";
        assert!(active_turn_signals(session_id).is_empty());

        let signal = InterruptSignal::new();
        let guard = register_active_turn(session_id, signal.clone());
        let registered = active_turn_signals(session_id);
        assert_eq!(registered.len(), 1);
        assert!(registered[0].same_instance(&signal));

        drop(guard);
        assert!(
            active_turn_signals(session_id).is_empty(),
            "dropping the guard must remove the registration"
        );
    }

    #[test]
    fn multiple_turns_for_one_session_are_all_listed() {
        let session_id = "turn_cancel_registry_multiple";
        let first = InterruptSignal::new();
        let second = InterruptSignal::new();
        let _guard_first = register_active_turn(session_id, first.clone());
        let guard_second = register_active_turn(session_id, second.clone());

        let registered = active_turn_signals(session_id);
        assert_eq!(registered.len(), 2);
        assert!(registered.iter().any(|signal| signal.same_instance(&first)));
        assert!(
            registered
                .iter()
                .any(|signal| signal.same_instance(&second))
        );

        drop(guard_second);
        let registered = active_turn_signals(session_id);
        assert_eq!(registered.len(), 1);
        assert!(registered[0].same_instance(&first));
    }
}
