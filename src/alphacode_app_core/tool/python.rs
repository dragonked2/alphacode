//! Python interpreter discovery shared by tools that run small Python helpers.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Find a usable Python interpreter on PATH or in common per-user install
/// locations. Windows installs from python.org commonly live under
/// `%LOCALAPPDATA%\Programs\Python\Python3xx` without adding the directory
/// to PATH, so checking command names alone misses a working installation.
pub async fn find_python() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os("PYTHON").filter(|value| !value.is_empty()) {
        candidates.push(configured);
    }

    #[cfg(windows)]
    candidates.extend(windows_install_candidates());

    candidates.extend([OsString::from("python3"), OsString::from("python")]);
    #[cfg(windows)]
    candidates.push(OsString::from("py"));

    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if interpreter_works(&candidate).await {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

/// Return a discovered replacement only when a command-line alias is missing
/// or unusable; this preserves a working `python`/`python3` selected by the
/// user's PATH.
#[cfg(windows)]
pub async fn replacement_for_missing_command(name: &str) -> Option<PathBuf> {
    if interpreter_works(name).await {
        None
    } else {
        find_python().await
    }
}

async fn interpreter_works(candidate: impl AsRef<std::ffi::OsStr>) -> bool {
    let mut command = tokio::process::Command::new(candidate);
    command
        .arg("--version")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    tokio::time::timeout(PROBE_TIMEOUT, command.status())
        .await
        .is_ok_and(|result| result.is_ok_and(|status| status.success()))
}

#[cfg(windows)]
fn windows_install_candidates() -> Vec<OsString> {
    let mut roots = Vec::new();
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        roots.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("Python"),
        );
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        roots.push(
            PathBuf::from(profile)
                .join("AppData")
                .join("Local")
                .join("Programs")
                .join("Python"),
        );
    }

    let mut installs = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_python_install = entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("python3");
            if is_python_install && path.is_dir() {
                installs.push(path);
            }
        }
    }

    installs.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| {
                name.to_ascii_lowercase()
                    .strip_prefix("python3")
                    .map(str::to_string)
            })
            .and_then(|version| version.parse::<u32>().ok())
            .unwrap_or_default()
    });
    installs.reverse();
    installs
        .into_iter()
        .map(|path| path.join("python.exe").into_os_string())
        .collect()
}
