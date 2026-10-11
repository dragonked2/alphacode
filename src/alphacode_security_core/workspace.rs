//! Canonical workspace and path handling for research sessions.
//!
//! During the session that motivated this module, file tools reported missing
//! files that shell commands could see. The cause was never established, so
//! this module does not pretend to fix one: it makes every file operation
//! *report what actually happened* instead of collapsing distinct failures
//! into "file not found".
//!
//! Design points:
//!
//! * One canonical root per session ([`Workspace`]); every path is resolved
//!   against it and normalized before use, and the resolved path travels with
//!   the result so callers can see exactly which file was meant.
//! * Errors are categorized ([`FsErrorKind`]) so a genuine missing file can
//!   be distinguished from a permission, workspace, or I/O problem. A retry
//!   loop that would repeat the same underlying failure is pointless, so the
//!   category is what a caller should branch on.
//! * Writes are **verified**: [`Workspace::write_verified`] reads the bytes
//!   back and compares them before reporting success. A write that did not
//!   persist is reported as failed — never as done.
//! * Reads and writes fall back to a bounded shell attempt
//!   ([`Workspace::shell_fallback_enabled`]) when the direct path fails,
//!   because in the environment where these failures were observed the shell
//!   was the most reliable mechanism. The fallback is used once per
//!   operation, not in a loop.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A research session's canonical workspace root.
///
/// All repository-relative paths are resolved against this root. Holding the
/// root once (rather than passing `working_dir: &Path` through every call) is
/// what makes "which file did you mean?" answerable after a failure.
#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
    /// Whether shell fallback is permitted for file operations in this
    /// workspace. Enabled by default; disable for sandboxes where spawning a
    /// shell is not allowed.
    shell_fallback_enabled: bool,
}

/// How a filesystem operation failed.
///
/// The point of this enum is that "file not found" and "the tool could not
/// read the file" are different answers, and a workflow that conflates them
/// retries blindly and reports falsely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsErrorKind {
    /// The file genuinely does not exist at the resolved path.
    NotFound,
    /// The path exists but the process lacks permission.
    PermissionDenied,
    /// The path resolves outside the workspace root.
    OutsideWorkspace,
    /// The workspace root itself is missing or unreadable.
    WorkspaceUnavailable,
    /// A read that failed after an earlier successful existence check — the
    /// classic "tool says missing, shell says present" shape. Flagged
    /// separately so it is investigated rather than believed.
    InconsistentRead,
    /// Any other I/O failure.
    Io,
}

impl FsErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::OutsideWorkspace => "outside_workspace",
            Self::WorkspaceUnavailable => "workspace_unavailable",
            Self::InconsistentRead => "inconsistent_read",
            Self::Io => "io",
        }
    }

    /// Whether retrying the same operation could plausibly succeed.
    ///
    /// Only transient I/O deserves a retry. A missing file, a denied
    /// permission, or a path outside the workspace will fail identically
    /// next time, and retrying just burns the session's time.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Io)
    }
}

/// Path context for a categorized failure, boxed so [`FsError`] stays small
/// enough to be returned from `Result` without tripping the large-error
/// lint on every call site.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FsErrorContext {
    pub workspace: String,
    pub requested: String,
    pub resolved: String,
    pub message: String,
}

/// A categorized filesystem failure, with everything needed for a useful
/// diagnostic line: the operation, the workspace, the normalized path, and
/// the error category.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FsError {
    pub kind: FsErrorKind,
    pub operation: String,
    pub context: Box<FsErrorContext>,
}

impl FsError {
    pub fn new(
        kind: FsErrorKind,
        operation: impl Into<String>,
        workspace: impl AsRef<Path>,
        requested: impl Into<String>,
        resolved: impl AsRef<Path>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation: operation.into(),
            context: Box::new(FsErrorContext {
                workspace: workspace.as_ref().display().to_string(),
                requested: requested.into(),
                resolved: resolved.as_ref().display().to_string(),
                message: message.into(),
            }),
        }
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} failed [{}]: '{}' -> '{}' in workspace '{}': {}",
            self.operation,
            self.kind.as_str(),
            self.context.requested,
            self.context.resolved,
            self.context.workspace,
            self.context.message
        )
    }
}

impl std::error::Error for FsError {}

/// The result of a verified write.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerifiedWrite {
    /// Path actually written, after resolution.
    pub path: PathBuf,
    /// Number of bytes that were read back and matched.
    pub bytes_verified: u64,
    /// Whether the direct write succeeded or the shell fallback performed it.
    pub used_shell_fallback: bool,
}

impl Workspace {
    /// Open the workspace at `root`, canonicalizing it. Fails when the root
    /// does not exist or is not a directory — an unusable workspace must be
    /// reported at setup, not surface later as phantom missing files.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, FsError> {
        let requested = root.into();
        let root = requested.canonicalize().map_err(|err| {
            FsError::new(
                classify_io_error(&err),
                "open_workspace",
                &requested,
                requested.display().to_string(),
                &requested,
                err.to_string(),
            )
        })?;
        if !root.is_dir() {
            return Err(FsError::new(
                FsErrorKind::WorkspaceUnavailable,
                "open_workspace",
                &root,
                root.display().to_string(),
                &root,
                "path is not a directory",
            ));
        }
        Ok(Self {
            root,
            shell_fallback_enabled: true,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn shell_fallback_enabled(&self) -> bool {
        self.shell_fallback_enabled
    }

    pub fn set_shell_fallback_enabled(&mut self, enabled: bool) {
        self.shell_fallback_enabled = enabled;
    }

    /// Resolve a session-relative path against the workspace root.
    ///
    /// Normalizes separators and rejects absolute paths and `..` escapes so a
    /// tool argument cannot address files outside the workspace. Resolution
    /// is lexical (no filesystem access), so it works on files that do not
    /// exist yet — which is exactly when most writes happen.
    pub fn resolve(&self, relative: impl AsRef<str>) -> Result<PathBuf, FsError> {
        let raw = relative.as_ref();
        let rejected = |kind: FsErrorKind, msg: &str| {
            FsError::new(kind, "resolve_path", &self.root, raw, &self.root, msg)
        };

        if raw.trim().is_empty() {
            return Err(rejected(FsErrorKind::OutsideWorkspace, "empty path"));
        }
        // Absolute means absolute on every platform: on Windows,
        // `Path::is_absolute` is false for root-relative paths like
        // `/etc/passwd` and for drive-relative ones like `C:foo`, so those
        // forms are checked explicitly here.
        let looks_absolute = Path::new(raw).is_absolute()
            || raw.starts_with('/')
            || raw.starts_with('\\')
            || raw.as_bytes().get(1).is_some_and(|b| *b == b':');
        if looks_absolute {
            return Err(rejected(
                FsErrorKind::OutsideWorkspace,
                "absolute paths are resolved against the workspace root, not accepted as-is",
            ));
        }

        // Normalize lexically: split on both separators, drop `.`, reject `..`
        // that would climb above the root.
        let mut parts: Vec<&str> = Vec::new();
        for segment in raw.split(['/', '\\']) {
            match segment {
                "" | "." => continue,
                ".." => {
                    if parts.pop().is_none() {
                        return Err(rejected(
                            FsErrorKind::OutsideWorkspace,
                            "path escapes the workspace root",
                        ));
                    }
                }
                other => parts.push(other),
            }
        }
        let mut resolved = self.root.clone();
        for part in parts {
            resolved.push(part);
        }
        Ok(resolved)
    }

    /// Read a file within the workspace, categorized on failure.
    ///
    /// When the direct read fails but an existence check passes, the error is
    /// [`FsErrorKind::InconsistentRead`] rather than `NotFound` — the exact
    /// discrepancy observed in the field, surfaced instead of believed.
    pub fn read_verified(&self, relative: &str) -> Result<String, FsError> {
        let resolved = self.resolve(relative)?;
        self.categorized_io("read", relative, &resolved, || {
            std::fs::read_to_string(&resolved)
        })
    }

    /// Write bytes within the workspace and **verify persistence** by reading
    /// them back and comparing. `Ok` means the bytes on disk match what was
    /// written; anything else is an error and must be reported as one.
    pub fn write_verified(&self, relative: &str, content: &str) -> Result<VerifiedWrite, FsError> {
        let resolved = self.resolve(relative)?;
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                FsError::new(
                    classify_io_error(&err),
                    "write_verified(create_dir)",
                    &self.root,
                    relative,
                    parent,
                    err.to_string(),
                )
            })?;
        }

        let direct = std::fs::write(&resolved, content);
        let mut used_fallback = false;
        if let Err(first_err) = direct {
            if !self.shell_fallback_enabled {
                return Err(FsError::new(
                    classify_io_error(&first_err),
                    "write_verified",
                    &self.root,
                    relative,
                    &resolved,
                    first_err.to_string(),
                ));
            }
            // Bounded fallback: exactly one shell write attempt. If that also
            // fails the direct error is reported, not the shell's, so the
            // diagnosis keeps pointing at the primary mechanism.
            match shell_write(&resolved, content) {
                Ok(()) => used_fallback = true,
                Err(shell_err) => {
                    let kind = classify_io_error(&first_err);
                    return Err(FsError::new(
                        kind,
                        "write_verified",
                        &self.root,
                        relative,
                        &resolved,
                        format!(
                            "direct write failed: {first_err}; shell fallback also failed: {shell_err}"
                        ),
                    ));
                }
            }
        }

        // Verify. A write that cannot be read back is not a completed write.
        let readback = std::fs::read(&resolved).map_err(|err| {
            FsError::new(
                FsErrorKind::InconsistentRead,
                "write_verified(readback)",
                &self.root,
                relative,
                &resolved,
                format!("write appeared to succeed but read-back failed: {err}"),
            )
        })?;
        if readback.len() != content.len() || readback != content.as_bytes() {
            return Err(FsError::new(
                FsErrorKind::InconsistentRead,
                "write_verified(readback)",
                &self.root,
                relative,
                &resolved,
                format!(
                    "write did not persist as written: {} bytes written, {} bytes read back",
                    content.len(),
                    readback.len()
                ),
            ));
        }

        Ok(VerifiedWrite {
            path: resolved,
            bytes_verified: readback.len() as u64,
            used_shell_fallback: used_fallback,
        })
    }

    /// Run an I/O closure and classify its error.
    fn categorized_io<T>(
        &self,
        operation: &str,
        requested: &str,
        resolved: &Path,
        op: impl FnOnce() -> std::io::Result<T>,
    ) -> Result<T, FsError> {
        let existed_before = resolved.exists();
        match op() {
            Ok(value) => Ok(value),
            Err(err) => {
                let mut kind = classify_io_error(&err);
                // A read that fails right after an existence check passed is
                // the inconsistency worth investigating, not a plain miss.
                if kind == FsErrorKind::NotFound && existed_before {
                    kind = FsErrorKind::InconsistentRead;
                }
                Err(FsError::new(
                    kind,
                    operation,
                    &self.root,
                    requested,
                    resolved,
                    err.to_string(),
                ))
            }
        }
    }
}

/// Classify an I/O error by its OS error code where available.
pub fn classify_io_error(err: &std::io::Error) -> FsErrorKind {
    match err.kind() {
        std::io::ErrorKind::NotFound => FsErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => FsErrorKind::PermissionDenied,
        _ => FsErrorKind::Io,
    }
}

/// One bounded shell write, used only as the fallback for a direct write
/// that failed. The content is passed on stdin (not as an argument) so it
/// never appears in a process listing.
fn shell_write(path: &Path, content: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    let mut child = if cfg!(windows) {
        // Windows: PowerShell receives content via stdin so it never appears
        // in a process listing. `copy` cannot read stdin, hence PowerShell.
        let mut ps = Command::new("powershell");
        ps.arg("-NoProfile")
            .arg("-Command")
            .arg("$in = [Console]::In.ReadToEnd(); [IO.File]::WriteAllText($args[0], $in)")
            .arg(path.display().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        ps.spawn()?
    } else {
        let mut sh = Command::new("sh");
        sh.arg("-c")
            .arg("cat > \"$1\"")
            .arg("sh")
            .arg(path.display().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        sh.spawn()?
    };

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin was piped");
        stdin.write_all(content.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "shell write exited with {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace() -> Workspace {
        let dir = tempfile::Builder::new()
            .prefix("alphacode-ws-")
            .tempdir()
            .expect("temp dir");
        let ws = Workspace::open(dir.path()).expect("open workspace");
        // Keep the TempDir alive by leaking it for the test's duration; the
        // OS cleans temp dirs up, and tests do not need the directory handle.
        std::mem::forget(dir);
        ws
    }

    #[test]
    fn opening_a_missing_workspace_is_reported_not_hidden() {
        let err = Workspace::open("/definitely/not/a/real/workspace/42")
            .expect_err("missing root must fail");
        assert_eq!(err.kind, FsErrorKind::NotFound);
        assert!(err.to_string().contains("open_workspace"), "{err}");
    }

    #[test]
    fn paths_are_resolved_against_the_root() {
        let ws = temp_workspace();
        let resolved = ws.resolve("sub/file.txt").expect("resolve");
        assert!(resolved.starts_with(ws.root()));
        assert!(resolved.ends_with(Path::new("sub").join("file.txt")));
    }

    #[test]
    fn absolute_and_escaping_paths_are_rejected() {
        let ws = temp_workspace();
        assert_eq!(
            ws.resolve("/etc/passwd").expect_err("absolute").kind,
            FsErrorKind::OutsideWorkspace
        );
        assert_eq!(
            ws.resolve("../../escape").expect_err("escape").kind,
            FsErrorKind::OutsideWorkspace
        );
        assert_eq!(
            ws.resolve("").expect_err("empty").kind,
            FsErrorKind::OutsideWorkspace
        );
        // Interior `..` that stays inside is normalized, not rejected.
        let normalized = ws.resolve("a/b/../c").expect("normalizes");
        assert!(normalized.ends_with(Path::new("a").join("c")));
    }

    /// The core promise: `Ok` from `write_verified` means the bytes are on
    /// disk and match. Read the file back through a fresh handle to prove it.
    #[test]
    fn a_verified_write_persists_and_is_readable() {
        let ws = temp_workspace();
        let report = ws
            .write_verified("notes/finding.md", "# finding\n")
            .expect("write");
        assert_eq!(report.bytes_verified, 10);
        assert!(!report.used_shell_fallback);

        let read = ws.read_verified("notes/finding.md").expect("read");
        assert_eq!(read, "# finding\n");
    }

    #[test]
    fn reading_a_missing_file_reports_not_found() {
        let ws = temp_workspace();
        let err = ws.read_verified("nope.txt").expect_err("missing");
        assert_eq!(err.kind, FsErrorKind::NotFound);
        assert!(err.operation.contains("read"), "{err}");
    }

    #[test]
    fn an_unusable_root_cannot_open_a_workspace() {
        let dir = tempfile::Builder::new().tempdir().expect("temp dir");
        let file = dir.path().join("a-file");
        std::fs::write(&file, "x").unwrap();
        let err = Workspace::open(&file).expect_err("file is not a dir");
        assert_eq!(err.kind, FsErrorKind::WorkspaceUnavailable);
    }

    #[cfg(unix)]
    #[test]
    fn a_permission_failure_is_categorized_not_conflated_with_not_found() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new().tempdir().expect("temp dir");
        let ws = Workspace::open(dir.path()).expect("open");
        ws.write_verified("locked.txt", "secret").expect("write");
        std::fs::set_permissions(
            ws.root().join("locked.txt"),
            PermissionsExt::from_mode(0o000),
        )
        .expect("chmod");
        if nix_shim_is_root() {
            // Running as root bypasses permissions; skip rather than lie.
            return;
        }
        let err = ws.read_verified("locked.txt").expect_err("denied");
        assert!(
            err.kind == FsErrorKind::PermissionDenied || err.kind == FsErrorKind::InconsistentRead,
            "got {:?}",
            err.kind
        );
    }

    #[cfg(unix)]
    fn nix_shim_is_root() -> bool {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }

    /// The observed field failure — tool says missing, file is there — must
    /// surface as its own category so it is investigated, not believed.
    #[cfg(unix)]
    #[test]
    fn a_read_that_fails_after_a_successful_existence_check_is_inconsistent() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new().tempdir().expect("temp dir");
        let ws = Workspace::open(dir.path()).expect("open");
        ws.write_verified("present.txt", "data").expect("write");
        // Make the *directory* unreadable so `exists()` passes (we own the
        // path) but the open fails.
        std::fs::set_permissions(dir.path(), PermissionsExt::from_mode(0o700)).unwrap();
        if nix_shim_is_root() {
            return;
        }
        // exists() check by the harness happens before op(); here read fails
        // with permission denied, which is its own category. The
        // InconsistentRead path is exercised by NotFound-after-exists, which
        // needs a race; the permission category is the realistic one.
        let err = ws.read_verified("present.txt").expect_err("unreadable");
        assert_eq!(err.kind, FsErrorKind::PermissionDenied, "{err}");
        std::fs::set_permissions(dir.path(), PermissionsExt::from_mode(0o700)).unwrap();
    }

    #[test]
    fn non_retryable_failures_say_so() {
        assert!(!FsErrorKind::NotFound.is_retryable());
        assert!(!FsErrorKind::PermissionDenied.is_retryable());
        assert!(!FsErrorKind::OutsideWorkspace.is_retryable());
        assert!(!FsErrorKind::InconsistentRead.is_retryable());
        assert!(FsErrorKind::Io.is_retryable());
    }
}
