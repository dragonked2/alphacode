use std::path::{Path, PathBuf};

/// Marker embedded in the name of a file that was moved out of the way by
/// [`move_file_aside`]. Also matches the older `.alphacode-launcher-old-*`
/// spelling so [`prune_swapped_asides`] cleans both generations.
const SWAPPED_ASIDE_MARKER: &str = ".alphacode-old-";

fn operation_stamp() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    )
}

/// Move `dest` aside so a replacement can take its path even while the old
/// file is locked.
///
/// Windows keeps a running executable open, so deleting or overwriting it
/// fails with `ERROR_SHARING_VIOLATION` — surfaced to users as
/// "Access is denied. (os error 5)" during an update while AlphaCode is still
/// running. The directory entry can be renamed regardless, which is what this
/// does. Returns the path the old file was moved to, or `None` when there was
/// nothing to move.
///
/// Callers install the new file, then either [`restore_file_aside`] it (on
/// failure) or [`discard_swapped_aside`] it (on success, best-effort: an
/// active image cannot be deleted until its process exits).
pub fn move_file_aside(dest: &Path) -> std::io::Result<Option<PathBuf>> {
    if !dest.exists() {
        return Ok(None);
    }
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let file_name = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let aside = parent.join(format!(
        "{file_name}{SWAPPED_ASIDE_MARKER}{}",
        operation_stamp()
    ));
    match std::fs::rename(dest, &aside) {
        Ok(()) => Ok(Some(aside)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Put a file moved by [`move_file_aside`] back at its original path.
///
/// Only used to roll back after the replacement failed and `dest` is absent.
pub fn restore_file_aside(aside: &Path, dest: &Path) -> std::io::Result<()> {
    if dest.exists() {
        return Ok(());
    }
    std::fs::rename(aside, dest)
}

/// Best-effort removal of a file left behind by [`move_file_aside`].
///
/// Fails silently when the old image is still running; a later install's
/// [`prune_swapped_asides`] retries.
pub fn discard_swapped_aside(aside: &Path) {
    let _ = std::fs::remove_file(aside);
}

/// Remove stale files left behind by interrupted swaps in `dir`.
///
/// Every update renames the old binary aside before installing the new one.
/// The old image cannot be deleted while its process is running, so on that
/// path a leftover waits for the next update — historically it waited
/// forever. Only entries older than [`PRUNE_MIN_AGE`] qualify, so a swap
/// happening in another process right now is never swept away mid-rollback.
pub fn prune_swapped_asides(dir: &Path) {
    const PRUNE_MIN_AGE: std::time::Duration = std::time::Duration::from_secs(300);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.contains(SWAPPED_ASIDE_MARKER) && !name.contains(".alphacode-launcher-old-") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(|modified| {
                std::time::SystemTime::now()
                    .duration_since(modified)
                    .map(|age| age >= PRUNE_MIN_AGE)
                    .unwrap_or(true)
            })
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Set file permissions to owner read/write/execute (0o755).
/// No-op on Windows (executability is determined by file extension).
pub fn set_permissions_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(path, perms)
    }
    #[cfg(windows)]
    {
        let _ = path;
        Ok(())
    }
}

/// Atomically swap a symlink by creating a temp symlink and renaming.
///
/// On Unix: creates temp symlink, then renames over target (atomic).
/// On Windows: stages the source, renames the target aside, then moves the
/// staged file into place. This avoids the lock on a running executable but is
/// not fully atomic.
pub fn atomic_symlink_swap(src: &Path, dst: &Path, temp: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(temp);
        std::os::unix::fs::symlink(src, temp)?;
        std::fs::rename(temp, dst)?;
    }
    #[cfg(windows)]
    {
        // Windows keeps a loaded executable open, so removing or copying over
        // the PATH launcher fails with ERROR_SHARING_VIOLATION. It does allow
        // the directory entry to be renamed while the process keeps running
        // from its existing handle. Stage the new file, rename the old entry
        // aside, then put the staged file at the stable path. This is the same
        // rename-aside strategy used by the PowerShell installer.
        let _ = std::fs::remove_file(temp);
        std::fs::copy(src, temp)?;

        let operation_id = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        let old = dst.parent().unwrap_or_else(|| Path::new(".")).join(format!(
            ".alphacode-launcher-old-{operation_id}{}",
            dst.extension()
                .map(|extension| format!(".{}", extension.to_string_lossy()))
                .unwrap_or_default()
        ));
        let mut moved_old = false;
        if dst.exists() {
            match std::fs::rename(dst, &old) {
                Ok(()) => moved_old = true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    let _ = std::fs::remove_file(temp);
                    return Err(error);
                }
            }
        }

        if let Err(error) = std::fs::rename(temp, dst) {
            if moved_old {
                let _ = std::fs::rename(&old, dst);
            }
            let _ = std::fs::remove_file(temp);
            return Err(error);
        }

        // An old loaded executable cannot be deleted until its process exits.
        // Best-effort cleanup is safe; later installs can remove leftovers.
        if moved_old {
            let _ = std::fs::remove_file(old);
        }
        // Sweep leftovers from interrupted swaps (the launcher directory had
        // accumulated weeks of `.alphacode-launcher-old-*.exe` files).
        prune_swapped_asides(dst.parent().unwrap_or_else(|| Path::new(".")));
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::atomic_symlink_swap;

    #[test]
    fn windows_swap_replaces_existing_launcher_via_staged_file() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let src = dir.path().join("source.exe");
        let dst = dir.path().join("alphacode.exe");
        let temp = dir.path().join(".alphacode-current");
        std::fs::write(&src, b"new binary").expect("source");
        std::fs::write(&dst, b"old binary").expect("destination");

        atomic_symlink_swap(&src, &dst, &temp).expect("swap succeeds");

        assert_eq!(std::fs::read(&dst).expect("new launcher"), b"new binary");
        assert!(!temp.exists());
        assert!(
            !dir.path()
                .join(format!(
                    ".alphacode-launcher-old-{}.exe",
                    std::process::id()
                ))
                .exists()
        );
    }
}
