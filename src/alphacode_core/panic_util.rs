//! Helpers for turning a panic into a recoverable error.
//!
//! Two things live here:
//!
//! * [`panic_payload_to_string`] — render a panic payload as text without
//!   assuming it is a `&str` or a `String`.
//! * [`suppress_panic_output`] — silence the *default* panic hook for the
//!   current thread while a block of third-party code runs, so a recoverable
//!   failure does not also spray a scary backtrace over the TUI.
//!
//! # Why the suppression is thread-scoped
//!
//! The obvious way to silence a panic is to swap the process-global panic
//! hook:
//!
//! ```ignore
//! let prev = panic::take_hook();
//! panic::set_hook(Box::new(|_| {}));
//! let result = panic::catch_unwind(work);
//! panic::set_hook(prev);
//! ```
//!
//! That is racy in two ways, and both bite in a multi-threaded TUI:
//!
//! 1. The hook is **process-global**. Any panic raised by *another* thread
//!    while the mute hook is installed is silently swallowed — the user sees
//!    no crash output for a failure that has nothing to do with the muted
//!    code, and the real error is lost.
//! 2. `take_hook` / `set_hook` is a read-modify-write on shared state with
//!    no locking. Two threads muting at the same time can restore each
//!    other's hooks in the wrong order, leaving the process with a stale hook
//!    (or no hook) permanently.
//!
//! [`suppress_panic_output`] avoids both problems: it installs **one**
//! process-wide hook exactly once, and that hook consults a **thread-local**
//! flag to decide whether the panic it is reporting belongs to a muted
//! thread. Panics on other threads keep their normal output.

use std::cell::Cell;
use std::panic::{self, PanicHookInfo};
use std::sync::OnceLock;

// Whether the current thread is running inside a `suppress_panic_output`
// scope.
thread_local! {
    static PANIC_OUTPUT_SUPPRESSED: Cell<bool> = const { Cell::new(false) };
}
/// Installed exactly once per process; see the module docs.
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

/// Install the process-wide hook that honours [`suppress_panic_output`].
///
/// Safe to call repeatedly: the hook is installed only on the first call, and
/// every later call is a no-op. The previous (default) hook is preserved and
/// still runs for panics raised outside a suppression scope.
fn install_suppression_aware_hook() {
    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
            if PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()) {
                return;
            }
            previous(info);
        }));
    });
}

/// Run `work` with the default panic hook silenced for this thread only.
///
/// The panic is still caught by the caller's `catch_unwind`; this only stops
/// the default hook from printing a backtrace that would corrupt the TUI
/// frame. Panics on other threads are unaffected.
///
/// The suppression is restored even if `work` panics, so a nested scope
/// cannot leave the thread muted.
///
/// ```ignore
/// let result = {
///     let _guard = suppress_panic_output();
///     panic::catch_unwind(work)
/// };
/// ```
pub fn suppress_panic_output() -> PanicOutputGuard {
    install_suppression_aware_hook();
    let previously_suppressed = PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.replace(true));
    PanicOutputGuard {
        restore: Some(previously_suppressed),
    }
}

/// RAII guard returned by [`suppress_panic_output`].
pub struct PanicOutputGuard {
    /// `Some(previous_value)` while the guard is alive; `None` after the
    /// guard has been consumed by `Drop`.
    restore: Option<bool>,
}

impl Drop for PanicOutputGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.restore.take() {
            PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.set(previous));
        }
    }
}

pub fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::panic_payload_to_string;
    use super::{PANIC_OUTPUT_SUPPRESSED, suppress_panic_output};

    #[test]
    fn panic_payload_to_string_handles_common_payloads() {
        let str_payload: &(dyn std::any::Any + Send) = &"borrowed panic";
        let string_payload: &(dyn std::any::Any + Send) = &String::from("owned panic");
        let unknown_payload: &(dyn std::any::Any + Send) = &42usize;

        assert_eq!(panic_payload_to_string(str_payload), "borrowed panic");
        assert_eq!(panic_payload_to_string(string_payload), "owned panic");
        assert_eq!(
            panic_payload_to_string(unknown_payload),
            "unknown panic payload"
        );
    }

    #[test]
    fn suppression_is_scoped_to_the_current_thread() {
        assert!(!PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()));
        {
            let _guard = suppress_panic_output();
            assert!(PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()));
        }
        assert!(!PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()));
    }

    #[test]
    fn nested_suppression_restores_the_outer_state() {
        {
            let _outer = suppress_panic_output();
            {
                let _inner = suppress_panic_output();
                assert!(PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()));
            }
            // The inner guard must not un-mute the outer scope.
            assert!(PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()));
        }
        assert!(!PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()));
    }

    #[test]
    fn suppression_does_not_leak_to_other_threads() {
        let _guard = suppress_panic_output();
        let other_thread_suppressed =
            std::thread::spawn(|| PANIC_OUTPUT_SUPPRESSED.with(|flag| flag.get()))
                .join()
                .expect("probe thread");
        assert!(
            !other_thread_suppressed,
            "suppression must not leak to other threads"
        );
    }
}
