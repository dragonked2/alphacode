//! Path classification: the safety-critical core.
//!
//! Correctness here matters more than anywhere else in the crate, because the
//! catastrophic tier is the one protection that cannot be talked around. It is
//! therefore written to be **absolute and simple**: a small set of paths that
//! may never be recursively destroyed, compared after normalization, with no
//! dependence on correctly understanding the surrounding command.

use super::{RiskContext, RiskFinding, RiskLevel};
use std::path::{Component, Path, PathBuf};

/// Credential stores. Protected *recursively*: destroying a single private key
/// inside `~/.ssh` is as damaging as destroying the directory, so containment
/// matters here and exact-match alone would leave an obvious hole.
const PROTECTED_CREDENTIAL_SUBPATHS: &[&str] = &[".ssh", ".gnupg", ".aws", ".kube", ".docker"];

/// Directories whose *wholesale* destruction is unacceptable, but whose
/// individual files are legitimately edited and removed all the time. Matched
/// exactly, so `~/.config` is protected while `~/.config/app/stale.toml` is not.
const PROTECTED_HOME_SUBPATHS: &[&str] = &[
    ".config",
    ".alphacode",
    ".claude",
    ".local",
    ".local/share",
    "Documents",
    "Desktop",
];

/// Absolute system paths that must never be recursively destroyed.
const PROTECTED_SYSTEM_PATHS: &[&str] = &[
    "/",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/lib",
    "/lib64",
    "/opt",
    "/proc",
    "/root",
    "/sbin",
    "/srv",
    "/sys",
    "/usr",
    "/var",
    "/Applications",
    "/System",
    "/Library",
    "/Users",
    "/home",
];

/// System paths where the *contents* are as critical as the directory itself,
/// so deleting a single file inside them is also unacceptable.
const SYSTEM_PATHS_PROTECTED_RECURSIVELY: &[&str] = &[
    "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/sbin", "/sys", "/usr",
    "/var/lib", "/System", "/Library",
];

/// Windows system roots, in the several spellings a shell can produce.
///
/// These are matched as *prefixes* (see [`matches_protected`]), so each entry
/// means "this directory and everything under it". `c:/users` is deliberately
/// kept out of this list and handled by [`WINDOWS_USERS_ROOT`] below, which
/// exempts the working directory.
///
/// The set is applied to *every* drive letter, not just `c:` — see
/// [`is_windows_system_root`].
const PROTECTED_WINDOWS_PATHS: &[&str] = &[
    "c:/",
    "c:/windows",
    "c:/windows/system32",
    "c:/program files",
    "c:/program files (x86)",
    "c:/programdata",
];

/// `C:\Users` — protected as a prefix, *except* inside the working directory.
///
/// A prefix rule here is over-broad on a default Windows install: the standard
/// layout is `C:\Users\<name>\...\<repo>`, so `rm -f main.rs` and `rm -rf
/// target` inside the repository the agent was working in both start with
/// `c:/users` and were classified catastrophic. That made the agent unable to
/// delete a single file in its own project. A project directory is not a system
/// path, so the working directory is exempted; every other profile, plus
/// `Public` and `Default`, stays protected, and the user's own home directory
/// and the credential stores beneath it are covered separately further down.
/// This mirrors how the POSIX lists deliberately exclude `/Users` and `/home`
/// from their recursive set.
const WINDOWS_USERS_ROOT: &str = "c:/users";

/// If `comparable` begins with a single-letter drive root (`d:/`), return the
/// remainder with no leading slash; otherwise `None`.
fn drive_relative_tail(comparable: &str) -> Option<&str> {
    let bytes = comparable.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return None;
    }
    let rest = &comparable[2..];
    Some(rest.strip_prefix('/').unwrap_or(rest))
}

/// Whether `comparable` is a Windows system root on *any* drive.
///
/// The literal `c:`-rooted lists are not drive-agnostic, which meant `rm -rf
/// D:\` and `format d:` were only "outside the working directory" — and so were
/// allowed — while `rm -rf C:\` was catastrophic. A drive letter is just the
/// first path component, so recognise the shape instead of enumerating letters.
fn is_windows_system_root(comparable: &str) -> bool {
    matches!(
        drive_relative_tail(comparable),
        Some(
            "" | "windows"
                | "windows/system32"
                | "program files"
                | "program files (x86)"
                | "programdata"
        )
    )
}

/// Windows paths whose *contents* are as sensitive as the directory.
const WINDOWS_PATHS_PROTECTED_RECURSIVELY: &[&str] = &[
    "c:/windows",
    "c:/program files",
    "c:/program files (x86)",
    "c:/programdata",
];

/// The set of paths this policy protects, exposed for testing and docs.
pub struct ProtectedPaths;

impl ProtectedPaths {
    pub fn home_subpaths() -> &'static [&'static str] {
        PROTECTED_HOME_SUBPATHS
    }
    pub fn credential_subpaths() -> &'static [&'static str] {
        PROTECTED_CREDENTIAL_SUBPATHS
    }
    pub fn system_paths() -> &'static [&'static str] {
        PROTECTED_SYSTEM_PATHS
    }
    pub fn recursive_system_paths() -> &'static [&'static str] {
        SYSTEM_PATHS_PROTECTED_RECURSIVELY
    }
}

/// Resolve `~`, `$HOME`, and relative paths into something comparable.
///
/// This is lexical only: it never touches the filesystem, so it is safe to call
/// on paths that do not exist and cannot be slowed down by a hostile argument.
pub fn expand(raw: &str, ctx: &RiskContext) -> PathBuf {
    let mut text = raw.to_string();

    if let Some(home) = &ctx.home_dir {
        let home_str = home.to_string_lossy().to_string();
        // Order matters: replace the longer forms first.
        //
        // Only the home-indirection variables are resolved. These are the
        // spellings under which every supported shell names the user's home
        // directory, and all of them reach the same place:
        //
        //   $HOME / ${HOME}          POSIX sh, Git Bash
        //   $USERPROFILE             Git Bash, MSYS on Windows
        //   ${USERPROFILE}           braced form of the above
        //   $env:USERPROFILE         PowerShell
        //   %USERPROFILE%            cmd.exe
        //
        // Resolving them here is what lets `rm -rf $USERPROFILE` be *checked*
        // rather than waved through as an opaque substitution — the expanded
        // form lands in `is_catastrophic_target` and is denied.
        for var in [
            "${HOME}",
            "$HOME",
            "${USERPROFILE}",
            "$USERPROFILE",
            "$env:USERPROFILE",
            "%USERPROFILE%",
        ] {
            text = text.replace(var, &home_str);
        }
        if text == "~" {
            text = home_str.clone();
        } else if let Some(rest) = text.strip_prefix("~/") {
            text = format!("{home_str}/{rest}");
        }
    }

    // Parse explicit Windows drive paths independently of the host OS. The
    // assessor runs on Linux/macOS too, where `Path::is_absolute()` treats
    // `C:\Windows` as a relative filename. Joining it to the workspace then
    // hid protected Windows targets from policy checks and made cross-platform
    // command review unreliable.
    if has_windows_drive_prefix(&text) {
        return normalize(Path::new(&text.replace('\\', "/")));
    }

    let path = PathBuf::from(&text);
    if path.is_absolute() {
        return normalize(&path);
    }
    // On Windows a rooted path with no drive (`/etc/passwd`, `/c/Users/me`,
    // `/dev/sda`) is NOT `is_absolute()`, so it used to be joined onto the
    // working directory — which made every POSIX-style path look like an
    // ordinary in-project file and sail through as merely `Confirm`. The
    // bash tool schema explicitly tells the model to prefer forward slashes,
    // and Git Bash / MSYS / WSL shells all address the system drive that way,
    // so these spellings are exactly what we must understand.
    if text.starts_with('/') {
        // `/c/Users/...` is the Git-Bash mount of the system drive; map it to
        // `C:/Users/...` so it compares against the protected Windows set.
        let rest = text.trim_start_matches('/');
        let mut chars = rest.chars();
        if let (Some(drive), Some('/')) = (chars.next(), chars.next())
            && drive.is_ascii_alphabetic()
        {
            let mapped = format!("{drive}:/{}", &rest[drive.len_utf8() + 1..]);
            return normalize(&PathBuf::from(mapped));
        }
        // Any other rooted path is treated as absolute at the filesystem root.
        return normalize(&PathBuf::from(if cfg!(windows) {
            format!("\\{text}")
        } else {
            text.clone()
        }));
    }
    match &ctx.working_dir {
        Some(cwd) => normalize(&cwd.join(path)),
        None => path,
    }
}

/// Lexically remove `.` and `..` so `/home/u/../..` is seen as `/`.
///
/// Without this, `rm -rf ~/../..` would walk straight past the protected-path
/// check. Note this is intentionally *not* symlink-aware; see the crate docs on
/// defense in depth.
fn normalize(path: &Path) -> PathBuf {
    let path_text = path.to_string_lossy();
    let windows_path;
    let path = if has_windows_drive_prefix(&path_text) {
        windows_path = PathBuf::from(path_text.replace('\\', "/"));
        windows_path.as_path()
    } else {
        path
    };
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        return PathBuf::from("/");
    }
    out
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Reduce a path to a canonical, comparable, lowercase form.
///
/// Three separate problems are solved here:
/// * **Separator** — `C:\Windows` and `C:/Windows` must compare equal.
/// * **Case** — Windows and macOS filesystems are case-insensitive, so
///   `C:/windows` and `C:/Windows` are the same directory.
/// * **Drive-letter mounting** — Git Bash, MSYS and WSL-style shells address
///   the system drive as `/c/...`. Without mapping that onto `c:/...`, the
///   protected-path list never matches and `rm -rf /c/Users/<me>` is allowed
///   on the one platform where it deletes the whole home directory.
fn normalize_for_compare(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/");
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    // `normalize()` on Unix drops the trailing slash from `/c/`, leaving the
    // two-character spelling `/c`. Recover the Git Bash drive-root alias
    // before the general `/c/...` conversion below so `rm -rf /c/` receives
    // the same protection as `rm -rf /c/Users/...`.
    if s.len() == 2 && s.as_bytes()[0] == b'/' && s.as_bytes()[1].is_ascii_alphabetic() {
        s = format!("{}:/", &s[1..2]);
    }
    // `/c/Users/...` -> `c:/Users/...`
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b'/' {
            s = format!("{}:{}", &s[1..2], &s[2..]);
        }
    }
    if s.len() == 2 && s.ends_with(':') {
        // Bare drive root: `c:` -> `c:/`
        s.push('/');
    }
    // Strip a trailing slash so `c:/windows/` == `c:/windows`, but keep `c:/`.
    if s.len() > 3 && s.ends_with('/') {
        while s.len() > 3 && s.ends_with('/') {
            s.pop();
        }
    }
    s.to_lowercase()
}

/// Whether `path` is equal to, or inside, the protected entry `prefix`
/// (compared via [`normalize_for_compare`]).
fn matches_protected(path: &str, prefix: &str) -> bool {
    let prefix = normalize_for_compare(Path::new(prefix));
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// Whether destroying this path is categorically unacceptable.
///
/// Exposed separately because this is the single most important predicate in
/// the crate and deserves to be testable in isolation.
pub fn is_catastrophic_target(path: &Path, ctx: &RiskContext) -> bool {
    let path = normalize(path);

    // The three standard device sinks are ordinary, disposable write targets
    // that appear in routine commands. They must be exempted *before* the
    // recursive `/dev` rule below, which would otherwise swallow them and deny
    // every `cmd > /dev/null`.
    if is_safe_device_sink(&path) {
        return false;
    }

    // Windows system roots. Checked with normalized comparison so every
    // spelling of the same directory is caught.
    let comparable = normalize_for_compare(&path);

    // Is the target the working directory, or inside it? Used only to scope the
    // `C:\Users` exemption below.
    let inside_working_dir = ctx.working_dir.as_ref().is_some_and(|dir| {
        let cwd = normalize_for_compare(&normalize(dir));
        comparable == cwd || comparable.starts_with(&format!("{cwd}/"))
    });

    if PROTECTED_WINDOWS_PATHS
        .iter()
        .any(|p| matches_protected(&comparable, p))
        || (matches_protected(&comparable, WINDOWS_USERS_ROOT) && !inside_working_dir)
    {
        return true;
    }
    // The same system roots on every other drive; see `is_windows_system_root`.
    if is_windows_system_root(&comparable) {
        return true;
    }
    if WINDOWS_PATHS_PROTECTED_RECURSIVELY
        .iter()
        .any(|p| matches_protected(&comparable, p))
    {
        return true;
    }

    // Exact system roots, plus anything inside the ones whose contents are as
    // unrecoverable as the directory itself (`/etc/passwd`). `/home` and
    // `/Users` are deliberately not recursive: a user's own project lives
    // under them, and the home directory itself is handled below.
    if PROTECTED_SYSTEM_PATHS.iter().any(|p| path == Path::new(p)) {
        return true;
    }
    if SYSTEM_PATHS_PROTECTED_RECURSIVELY
        .iter()
        .any(|p| path.starts_with(p))
    {
        return true;
    }

    let Some(home) = &ctx.home_dir else {
        return false;
    };
    let home = normalize(home);

    // The home directory itself.
    if path == home {
        return true;
    }
    // Credential stores, including anything inside them.
    if PROTECTED_CREDENTIAL_SUBPATHS
        .iter()
        .any(|sub| path.starts_with(home.join(sub)))
    {
        return true;
    }
    // Config and document roots, but not their individual files.
    PROTECTED_HOME_SUBPATHS
        .iter()
        .any(|sub| path == home.join(sub))
}

/// Classify one resolved target, returning a finding when it is notable.
pub fn classify_target(
    expanded: &Path,
    raw: &str,
    recursive: bool,
    ctx: &RiskContext,
) -> Option<RiskFinding> {
    // The three device sinks are checked against the *raw* text, before
    // expansion. `expand` rewrites a POSIX-looking `/dev/null` into a Windows
    // path (`C:\dev\null`), so the exemption below — which compares against
    // `/dev/null` — was unreachable on Windows. Every `cmd 2>/dev/null` was
    // therefore graded as a write to a path outside the working directory.
    if is_safe_device_sink(Path::new(raw)) {
        return None;
    }

    // Glob and variable expansion we did not perform: we cannot know the
    // footprint, so escalate rather than guess.
    if raw.contains('*') || raw.contains('?') {
        // A glob whose parent directory is protected is catastrophic by
        // construction: a glob can only match entries *inside* its parent, so
        // `~/.ssh/id_*` destroys private keys exactly as `~/.ssh/*` does. The
        // old code additionally required the glob to be the whole final
        // component (`file_name() == "*"`), which meant `rm -rf ~/.ssh/id_*`
        // fell through to the much weaker Confirm tier — and Confirm is
        // allowed to run.
        if let Some(parent_of_glob) = expanded.parent()
            && is_catastrophic_target(parent_of_glob, ctx)
        {
            return Some(RiskFinding {
                level: RiskLevel::Catastrophic,
                reason: "would destroy the contents of a protected directory".to_string(),
                target: Some(raw.to_string()),
            });
        }
        return Some(RiskFinding {
            level: RiskLevel::Confirm,
            reason: "target contains a glob, so the exact set of affected files \
                     is not known before execution"
                .to_string(),
            target: Some(raw.to_string()),
        });
    }

    // Check the resolved path first: `$HOME` expands to a protected path and
    // must be denied outright, not merely queried. Only genuinely unresolvable
    // substitutions fall through to the Confirm tier below.
    if is_catastrophic_target(expanded, ctx) {
        return Some(RiskFinding {
            level: RiskLevel::Catastrophic,
            reason: "targets a protected system or home path that must never be \
                     destroyed"
                .to_string(),
            target: Some(expanded.display().to_string()),
        });
    }

    // `%` is cmd.exe's expansion sigil and must be caught here, or
    // `rm -rf %USERPROFILE%` looks like an ordinary relative filename: it
    // expands to `<cwd>\%USERPROFILE%`, sits inside the working directory, and
    // was classified `Low` and allowed — while the shell deleted the home
    // directory. The finding it produced also named a path that was never
    // touched, so the report was affirmatively wrong, not merely unhelpful.
    if raw.contains('$') || raw.contains('`') || raw.contains('%') {
        return Some(RiskFinding {
            level: RiskLevel::Confirm,
            reason: "target is computed at runtime (variable or command \
                     substitution), so its value cannot be checked in advance"
                .to_string(),
            target: Some(raw.to_string()),
        });
    }

    // Raw device nodes are never a safe write target, with the exception of the
    // three standard sinks: `cmd > /dev/null` is routine and must not cost a
    // reflection turn.
    if expanded.starts_with("/dev") {
        if is_safe_device_sink(expanded) {
            return None;
        }
        return Some(RiskFinding {
            level: RiskLevel::Catastrophic,
            reason: "writes directly to a device node, which can destroy a \
                     filesystem or disk"
                .to_string(),
            target: Some(expanded.display().to_string()),
        });
    }

    let inside_cwd = ctx
        .working_dir
        .as_ref()
        .is_some_and(|cwd| expanded.starts_with(normalize(cwd)));

    if inside_cwd {
        // The routine case: an agent tidying up its own workspace.
        return recursive.then(|| RiskFinding {
            level: RiskLevel::Low,
            reason: "recursive delete inside the working directory".to_string(),
            target: Some(expanded.display().to_string()),
        });
    }

    if is_temp_path(expanded) {
        return None;
    }

    Some(RiskFinding {
        level: RiskLevel::Confirm,
        reason: "targets a path outside the working directory".to_string(),
        target: Some(expanded.display().to_string()),
    })
}

/// The device pseudo-files that are safe to write to. Matched exactly, so a
/// real device like `/dev/sda` — or a lookalike such as `/dev/nullX` — is not
/// covered by the exemption.
fn is_safe_device_sink(path: &Path) -> bool {
    ["/dev/null", "/dev/stdout", "/dev/stderr"]
        .iter()
        .any(|safe| path == Path::new(safe))
}

/// Temp directories are conventionally disposable, so deleting inside them is
/// not worth a reflection turn.
fn is_temp_path(path: &Path) -> bool {
    ["/tmp", "/var/tmp", "/private/tmp"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod paths_tests;
