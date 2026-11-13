//! Filesystem primitives for the project-memory store.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_POLL: Duration = Duration::from_millis(5);

/// Lock a stable sidecar, not STATE.json: atomic replacement changes the state
/// file's identity. Each transaction opens its own handle so clones and
/// independently constructed managers participate in the same OS lock.
/// Never unlink this file; closing the handle releases ownership after a crash.
pub(super) fn acquire_state_lock(root: &Path) -> Result<File> {
    let path = root.join(".memory-state.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("opening project-state lock {}", path.display()))?;
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) => {
                let remaining = LOCK_WAIT.saturating_sub(started.elapsed());
                anyhow::ensure!(
                    !remaining.is_zero(),
                    "Project state is busy: lock acquisition timed out for {}",
                    path.display()
                );
                std::thread::sleep(LOCK_POLL.min(remaining));
            }
            Err(TryLockError::Error(error)) => {
                return Err(error)
                    .with_context(|| format!("locking project state {}", path.display()));
            }
        }
    }
}

/// Initialize only a missing file. A concurrent creator must never be
/// truncated, even if it does not participate in our state lock protocol.
pub(super) fn initialize_file(path: &Path, content: &[u8]) -> Result<()> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(content)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error).with_context(|| format!("initializing {}", path.display())),
    }
}

/// Stage a complete replacement alongside its destination before publishing.
/// This ensures readers see a complete old/new file; transaction locking is
/// separate so a read-modify-write sequence can hold ownership throughout.
pub(crate) fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("memory file has no parent directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(content)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}
