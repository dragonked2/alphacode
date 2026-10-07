#![cfg_attr(test, allow(clippy::items_after_test_module))]

pub use crate::alphacode_storage::*;

use anyhow::Result;
use serde::de::DeserializeOwned;
use std::path::Path;

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    crate::alphacode_storage::read_json_with_recovery_handler(path, |event| match event {
        crate::alphacode_storage::StorageRecoveryEvent::CorruptPrimary { path, error } => {
            crate::logging::warn(&format!(
                "Corrupt JSON at {}, trying backup: {}",
                path.display(),
                error
            ));
        }
        crate::alphacode_storage::StorageRecoveryEvent::RecoveredFromBackup { backup_path } => {
            crate::logging::info(&format!("Recovered from backup: {}", backup_path.display()));
        }
    })
}

#[cfg(any(test, feature = "test-support"))]
use std::sync::{Mutex, MutexGuard, OnceLock};

#[cfg(any(test, feature = "test-support"))]
pub fn test_env_lock() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// Whether *this* thread currently holds the crate-wide test-env lock.
    ///
    /// `test_env_lock` is a non-reentrant `Mutex`, so a helper that runs inside a
    /// scope already holding it cannot take it again. That is what makes a bare
    /// `try_lock` miss ambiguous: it means either "we already hold it" (proceed -
    /// no other thread can be in that window) or "another thread holds it" (wait).
    /// Conflating the two let a helper repoint `ALPHACODE_HOME` at a shared
    /// scratch home *while another test was still using its scoped one*, which
    /// showed up as sporadic save/restore and telemetry-state failures that only
    /// appeared in a full parallel run.
    static ENV_LOCK_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the calling thread already holds [`lock_test_env`].
///
/// When true the caller must not take the lock again; use this to pick between
/// "proceed unprotected" and "wait for the other thread".
#[cfg(any(test, feature = "test-support"))]
pub fn test_env_lock_held_by_current_thread() -> bool {
    ENV_LOCK_HELD.with(|held| held.get())
}

/// Guard for [`lock_test_env`] that records lock ownership on this thread.
#[cfg(any(test, feature = "test-support"))]
pub struct TestEnvGuard {
    _guard: MutexGuard<'static, ()>,
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        ENV_LOCK_HELD.with(|held| held.set(false));
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn lock_test_env() -> TestEnvGuard {
    let guard = test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ENV_LOCK_HELD.with(|held| held.set(true));
    TestEnvGuard { _guard: guard }
}

/// Poll the crate-wide test-env lock for up to `limit`, returning it if acquired.
///
/// Callers reach the env lock from both sides of the test lock order: app
/// factories take env-then-render, but render tests hold the render-state lock
/// for their whole body and only then build an app. A *blocking* acquisition
/// from the second group inverts the order against a test that holds this mutex
/// and is itself waiting for the render lock, and the pair deadlocks. This
/// therefore sleeps and retries instead of waiting on the mutex, so it cannot
/// take part in such a cycle - but it still waits, because giving up leaves the
/// caller without an `ALPHACODE_HOME` and that resolves to the real
/// `~/.alphacode` rather than a test directory.
#[cfg(any(test, feature = "test-support"))]
pub fn try_test_env_lock_for(limit: std::time::Duration) -> Option<MutexGuard<'static, ()>> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Ok(guard) = test_env_lock().try_lock() {
            return Some(guard);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_micros(250));
    }
}

#[cfg(test)]
mod tests;
