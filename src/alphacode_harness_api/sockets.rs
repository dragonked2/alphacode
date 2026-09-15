//! Socket path resolution shared by every harness API client and the bridge.
//!
//! This lives in the API crate on purpose. It used to be duplicated in the
//! bridge and in `alphacode-desktop2`, and the two copies disagreed: the bridge
//! resolved `$XDG_RUNTIME_DIR` while the desktop always looked in
//! `~/.alphacode`. The result was a desktop app that could never connect even
//! with a healthy bridge running. One definition, used by both sides, makes
//! that class of bug impossible.
//!
//! The rules match `alphacode-storage::runtime_dir` so the API socket always lands
//! beside the daemon socket it bridges to.

use std::path::PathBuf;

/// Runtime directory holding the daemon and API sockets.
pub fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ALPHACODE_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    if let Ok(dir) = std::env::var("TMPDIR") {
        return PathBuf::from(dir);
    }
    fallback_runtime_dir()
}

fn fallback_runtime_dir() -> PathBuf {
    std::env::temp_dir().join(format!("alphacode-{}", runtime_user_discriminator()))
}

#[cfg(unix)]
fn runtime_user_discriminator() -> String {
    // Read the uid without pulling in libc: the API crate is deliberately
    // dependency-light, and this only needs to disambiguate users in $TMPDIR.
    std::env::var("UID")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .map(sanitize)
        .unwrap_or_else(|| "user".to_string())
}

#[cfg(not(unix))]
fn runtime_user_discriminator() -> String {
    // Windows named pipes are machine-global (unlike per-uid filesystem
    // sockets), so two Windows sessions of the same user (console + RDP, or
    // two service logons) must not resolve the same fallback dir — otherwise
    // a pipe name collision would route one session's API traffic to the
    // other session's daemon. `SESSIONNAME` distinguishes console/RDP/RDS
    // sessions; `ALPHACODE_SESSION_TAG` covers exotic service hosts.
    let raw = std::env::var("SESSIONNAME")
        .or_else(|_| std::env::var("ALPHACODE_SESSION_TAG"))
        .map(|value| format!("{}-", sanitize(value)))
        .unwrap_or_default();
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .map(sanitize)
        .unwrap_or_else(|_| "user".to_string());
    format!("{user}-{raw}main")
}

fn sanitize(raw: String) -> String {
    let out: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .take(64)
        .collect();
    if out.is_empty() {
        "user".to_string()
    } else {
        out
    }
}

/// Path of the versioned harness API socket. `ALPHACODE_API_SOCKET` overrides it.
pub fn api_socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("ALPHACODE_API_SOCKET") {
        return PathBuf::from(custom);
    }
    runtime_dir().join("alphacode-api.sock")
}

/// Path of the internal daemon socket the bridge translates onto.
/// `ALPHACODE_SOCKET` overrides it.
pub fn legacy_socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("ALPHACODE_SOCKET") {
        return PathBuf::from(custom);
    }
    runtime_dir().join("alphacode.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two sockets must always be siblings. A client that resolves one
    /// directory while the bridge resolves another cannot connect at all,
    /// which is exactly the bug this module exists to prevent.
    #[test]
    fn the_api_socket_sits_beside_the_daemon_socket() {
        // Guard against env-dependent divergence by comparing parents rather
        // than absolute paths, since either may be overridden in a session.
        let api = runtime_dir().join("alphacode-api.sock");
        let legacy = runtime_dir().join("alphacode.sock");
        assert_eq!(api.parent(), legacy.parent());
    }

    #[test]
    fn socket_names_are_stable() {
        assert_eq!(
            runtime_dir()
                .join("alphacode-api.sock")
                .file_name()
                .unwrap(),
            "alphacode-api.sock"
        );
    }

    #[test]
    fn sanitize_strips_path_and_shell_characters() {
        assert_eq!(sanitize("../root; rm".into()), "rootrm");
        assert_eq!(sanitize("!!!".into()), "user");
    }

    #[test]
    #[cfg(not(unix))]
    fn windows_discriminator_includes_session_name() {
        // Keep in sync with `alphacode_storage::runtime_user_discriminator`:
        // console + RDP sessions of the same user must resolve different dirs.
        let prev_session = std::env::var("SESSIONNAME");
        let prev_tag = std::env::var("ALPHACODE_SESSION_TAG");
        let prev_user = std::env::var("USERNAME");
        unsafe {
            std::env::set_var("USERNAME", "tlbbe");
            std::env::remove_var("ALPHACODE_SESSION_TAG");
            std::env::set_var("SESSIONNAME", "Console");
            let console = super::runtime_user_discriminator();
            std::env::set_var("SESSIONNAME", "RDP-Tcp#0");
            let rdp = super::runtime_user_discriminator();
            std::env::remove_var("SESSIONNAME");
            std::env::set_var("ALPHACODE_SESSION_TAG", "svc-1");
            let tagged = super::runtime_user_discriminator();
            match prev_session {
                Ok(v) => std::env::set_var("SESSIONNAME", v),
                Err(_) => std::env::remove_var("SESSIONNAME"),
            }
            match prev_tag {
                Ok(v) => std::env::set_var("ALPHACODE_SESSION_TAG", v),
                Err(_) => std::env::remove_var("ALPHACODE_SESSION_TAG"),
            }
            match prev_user {
                Ok(v) => std::env::set_var("USERNAME", v),
                Err(_) => std::env::remove_var("USERNAME"),
            }
            assert_eq!(console, "tlbbe-Console-main");
            assert_eq!(rdp, "tlbbe-RDP-Tcp0-main");
            assert_eq!(tagged, "tlbbe-svc-1-main");
        }
    }
}
