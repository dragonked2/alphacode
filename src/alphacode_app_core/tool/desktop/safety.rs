//! Safety system for desktop control operations.
//!
//! Provides:
//! * Action timeouts with cancellation
//! * Input-state cleanup on failure (release held keys/buttons)
//! * Emergency stop capability
//! * Retry limits

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Global emergency stop flag. When set, all desktop operations abort immediately.
static EMERGENCY_STOP: AtomicBool = AtomicBool::new(false);

/// Maximum number of retries for transient failures.
#[allow(dead_code)]
pub const MAX_RETRIES: usize = 2;

/// Maximum action duration (hard cap).
pub const MAX_ACTION_DURATION: Duration = Duration::from_secs(30);

/// Set the emergency stop flag.
#[allow(dead_code)]
pub fn emergency_stop() {
    EMERGENCY_STOP.store(true, Ordering::SeqCst);
    // Also release any held input state
    release_all_input();
}

/// Clear the emergency stop flag.
#[allow(dead_code)]
pub fn clear_emergency_stop() {
    EMERGENCY_STOP.store(false, Ordering::SeqCst);
}

/// Check if emergency stop is active.
pub fn is_emergency_stopped() -> bool {
    EMERGENCY_STOP.load(Ordering::SeqCst)
}

/// Execute a closure with a timeout and cancellation support.
///
/// Returns `Err` if:
/// * The operation exceeds `timeout`
/// * Emergency stop is active
/// * The operation itself fails
pub fn with_timeout<F, T>(timeout: Duration, f: F) -> anyhow::Result<T>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    // Clamp to maximum
    let effective_timeout = timeout.min(MAX_ACTION_DURATION);

    // Check emergency stop before starting
    if is_emergency_stopped() {
        anyhow::bail!(
            "EmergencyStop: Desktop control operations are stopped. Run desktop action='clear_emergency_stop' to resume."
        );
    }

    let result = std::thread::scope(|s| {
        let handle = s.spawn(|| {
            // Check emergency stop periodically within the operation
            f()
        });

        let start = std::time::Instant::now();
        loop {
            if start.elapsed() >= effective_timeout {
                // Timeout: release input and return error
                release_all_input();
                return Err(anyhow::anyhow!(
                    "Timeout: Desktop action exceeded {:.1}s limit.",
                    effective_timeout.as_secs_f64()
                ));
            }

            if is_emergency_stopped() {
                release_all_input();
                return Err(anyhow::anyhow!(
                    "EmergencyStop: Desktop control operations are stopped."
                ));
            }

            if handle.is_finished() {
                return handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("Desktop action panicked")));
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    });

    // On failure, ensure input state is clean
    if result.is_err() {
        release_all_input();
    }

    result
}

/// Release all held input state (keys, mouse buttons).
///
/// This is called on error/cancellation to prevent the agent from leaving
/// modifier keys or mouse buttons artificially held.
fn release_all_input() {
    if let Ok(sim) = xa11y::input_sim() {
        // Release common modifier keys (best-effort, ignore errors)
        let _ = sim.keyboard().up(xa11y::Key::Ctrl);
        let _ = sim.keyboard().up(xa11y::Key::Shift);
        let _ = sim.keyboard().up(xa11y::Key::Alt);
        let _ = sim.keyboard().up(xa11y::Key::Meta);

        // Release mouse buttons (best-effort)
        let _ = sim.mouse().up(xa11y::MouseButton::Left);
        let _ = sim.mouse().up(xa11y::MouseButton::Right);
        let _ = sim.mouse().up(xa11y::MouseButton::Middle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_returns_error() {
        clear_emergency_stop();
        let result = with_timeout(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(10));
            Ok(())
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Timeout"));
    }

    #[test]
    fn successful_operation_passes_through() {
        clear_emergency_stop();
        let result = with_timeout(Duration::from_secs(5), || Ok(42));
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn max_retries_constant_is_sane() {
        const _: () = assert!(MAX_RETRIES > 0, "MAX_RETRIES must be positive");
        const _: () = assert!(MAX_RETRIES <= 5, "MAX_RETRIES must be at most 5");
    }
}
