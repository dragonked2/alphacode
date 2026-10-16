mod paths;
mod platform_support;
mod source_state;
mod storage_helpers;

pub use paths::{
    SELFDEV_CARGO_PROFILE, binary_name, binary_stem, client_update_candidate,
    current_binary_build_time_string, current_binary_built_at, find_dev_binary,
    find_repo_in_ancestors, get_repo_dir, is_alphacode_repo, launcher_binary_path, launcher_dir,
    preferred_reload_candidate, release_binary_path, resolve_binary_payload, run_selfdev_build,
    selfdev_binary_path, selfdev_build_command, selfdev_build_command_for_target,
    shared_server_update_candidate, update_launcher_symlink_to_current,
    update_launcher_symlink_to_stable, update_reload_candidate, version_matches_installed_channel,
};
pub use source_state::{
    current_build_info, current_git_diff, current_git_hash, current_git_hash_full,
    current_source_state, ensure_source_state_matches, get_commit_message, is_working_tree_dirty,
    repo_build_version, repo_scope_key, worktree_scope_key,
};
pub use storage_helpers::{
    build_log_path, build_progress_path, builds_dir, canary_binary_path, clear_build_progress,
    clear_migration_context, current_binary_path, current_version_file, load_migration_context,
    manifest_path, migration_context_path, read_build_progress, read_current_version,
    read_shared_server_version, read_stable_version, save_migration_context,
    shared_server_binary_path, shared_server_version_file, stable_binary_path, stable_version_file,
    version_binary_path, write_build_progress,
};

use crate::alphacode_storage as storage;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::time::{Duration, Instant};

pub use crate::alphacode_selfdev_types::{
    BinaryChoice, BinaryVersionReport, BuildInfo, CanaryStatus, CrashInfo, DevBinarySourceMetadata,
    MigrationContext, PendingActivation, PublishedBuild, SelfDevBuildCommand, SelfDevBuildTarget,
    SourceState,
};

/// Manifest tracking build versions and their status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildManifest {
    /// Current stable build hash (known good)
    pub stable: Option<String>,
    /// Current canary build hash (being tested)
    pub canary: Option<String>,
    /// Session ID testing the canary build
    pub canary_session: Option<String>,
    /// Status of canary testing
    pub canary_status: Option<CanaryStatus>,
    /// History of recent builds
    #[serde(default)]
    pub history: Vec<BuildInfo>,
    /// Last crash information (if canary crashed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_crash: Option<CrashInfo>,
    /// Pending activation being validated across reload/resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_activation: Option<PendingActivation>,
}

impl BuildManifest {
    /// Load manifest from disk
    pub fn load() -> Result<Self> {
        let path = manifest_path()?;
        if path.exists() {
            storage::read_json(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Save manifest to disk
    pub fn save(&self) -> Result<()> {
        let path = manifest_path()?;
        storage::write_json(&path, self)
    }

    /// Check if we should use stable or canary for a given session
    pub fn binary_for_session(&self, session_id: &str) -> BinaryChoice {
        // If this session is the canary tester, use canary
        if let Some(ref canary_session) = self.canary_session
            && canary_session == session_id
            && let Some(ref canary) = self.canary
        {
            return BinaryChoice::Canary(canary.clone());
        }
        // Otherwise use stable
        if let Some(ref stable) = self.stable {
            BinaryChoice::Stable(stable.clone())
        } else {
            BinaryChoice::Current
        }
    }

    /// Start canary testing for a session
    pub fn start_canary(&mut self, hash: &str, session_id: &str) -> Result<()> {
        self.canary = Some(hash.to_string());
        self.canary_session = Some(session_id.to_string());
        self.canary_status = Some(CanaryStatus::Testing);
        self.save()
    }

    /// Mark canary as passed
    pub fn mark_canary_passed(&mut self) -> Result<()> {
        self.canary_status = Some(CanaryStatus::Passed);
        self.save()
    }

    /// Mark canary as failed
    pub fn mark_canary_failed(&mut self) -> Result<()> {
        self.canary_status = Some(CanaryStatus::Failed);
        self.save()
    }

    /// Record a crash
    pub fn record_crash(
        &mut self,
        hash: &str,
        exit_code: i32,
        stderr: &str,
        diff: Option<String>,
    ) -> Result<()> {
        self.last_crash = Some(CrashInfo {
            build_hash: hash.to_string(),
            exit_code,
            stderr: stderr.chars().take(4096).collect(), // Truncate
            crashed_at: Utc::now(),
            diff,
        });
        self.canary_status = Some(CanaryStatus::Failed);
        self.save()
    }

    /// Clear crash info after it's been handled
    pub fn clear_crash(&mut self) -> Result<()> {
        self.last_crash = None;
        self.save()
    }

    pub fn set_pending_activation(&mut self, activation: PendingActivation) -> Result<()> {
        self.pending_activation = Some(activation);
        self.save()
    }

    pub fn clear_pending_activation(&mut self) -> Result<()> {
        self.pending_activation = None;
        self.save()
    }

    /// Add build to history
    pub fn add_to_history(&mut self, info: BuildInfo) -> Result<()> {
        // Keep last 20 builds
        self.history.insert(0, info);
        self.history.truncate(20);
        self.save()
    }
}

pub fn complete_pending_activation_for_session(session_id: &str) -> Result<Option<String>> {
    let mut manifest = BuildManifest::load()?;
    let Some(pending) = manifest.pending_activation.clone() else {
        return Ok(None);
    };
    if pending.session_id != session_id {
        return Ok(None);
    }

    manifest.canary = Some(pending.new_version.clone());
    manifest.canary_session = Some(session_id.to_string());
    manifest.canary_status = Some(CanaryStatus::Passed);
    manifest.pending_activation = None;
    manifest.last_crash = None;
    manifest.save()?;
    Ok(Some(pending.new_version))
}

pub fn rollback_pending_activation_for_session(session_id: &str) -> Result<Option<String>> {
    let mut manifest = BuildManifest::load()?;
    let Some(pending) = manifest.pending_activation.clone() else {
        return Ok(None);
    };
    if pending.session_id != session_id {
        return Ok(None);
    }

    if let Some(previous) = pending.previous_current_version.as_deref() {
        update_current_symlink(previous)?;
        update_launcher_symlink_to_current()?;
    }
    if let Some(previous) = pending.previous_shared_server_version.as_deref() {
        update_shared_server_symlink(previous)?;
    }
    manifest.canary_status = Some(CanaryStatus::Failed);
    manifest.pending_activation = None;
    manifest.save()?;
    Ok(Some(pending.new_version))
}

/// Validate a version string that is about to be used as a single path
/// component under `builds/versions/`.
///
/// Versions reach us from the GitHub releases API (`tag_name`) and from
/// `main-<sha>` labels, so they are untrusted input for path purposes. Without
/// this check a tag such as `v1.0.0/../../../../Startup` clears the semver gate
/// — `version_is_newer` only reads the first three dot-separated tokens and
/// coerces parse failures to `0`, so `"1"`, `"0"`, `"0/../../../../Startup"`
/// compares as `(1, 0, 0)` and beats any installed `0.x.y` — and then steers
/// the `fs::copy` of the downloaded archive to an attacker-chosen destination,
/// after which `stable`, `current` and the PATH launcher get repointed at it.
///
/// A legitimate version is digits and dots with an optional pre-release or
/// build suffix, so allow exactly that and nothing else.
pub fn safe_version_component(raw: &str) -> Result<String> {
    let v = raw.trim().trim_start_matches('v');
    if v.is_empty() {
        anyhow::bail!("release has an empty version (from {raw:?})");
    }
    // Real versions are far shorter; this stops a hostile tag from producing an
    // absurdly long path component in the first place.
    if v.len() > 64 {
        anyhow::bail!("release version {raw:?} is implausibly long");
    }
    if !v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
    {
        anyhow::bail!("release version {raw:?} contains characters that are not path-safe");
    }
    // `..`, an empty segment, or a leading/trailing dot are never valid, and are
    // exactly the shapes a traversal attempt takes.
    if v.contains("..") || v.split('.').any(str::is_empty) {
        anyhow::bail!("release version {raw:?} is not a single plain path component");
    }
    Ok(v.to_string())
}

/// Install a binary at a specific immutable version path.
pub fn install_binary_at_version(source: &std::path::Path, version: &str) -> Result<PathBuf> {
    if !source.exists() {
        anyhow::bail!("Binary not found at {:?}", source);
    }

    // Choke point: every install path funnels through here, so validating the
    // component once here covers callers that pass `tag_name` or a `main-<sha>`
    // label straight through.
    let version = safe_version_component(version)?;
    let dest_dir = builds_dir()?.join("versions").join(&version);
    storage::ensure_dir(&dest_dir)?;

    let dest = dest_dir.join(binary_name());

    // Rename any existing file aside rather than removing it: the destination
    // is frequently the binary of a running AlphaCode instance (or its
    // just-spawned replacement), and on Windows deleting a loaded executable
    // fails with "Access is denied. (os error 5)". Renaming the directory
    // entry always works; the new file then takes the stable path.
    let aside = platform_support::move_file_aside(&dest)?;

    // Prefer hard link (instant, zero I/O) over copy (71MB+ binary).
    // Falls back to copy if hard link fails (e.g. cross-filesystem).
    let install_result = (|| -> std::io::Result<()> {
        if std::fs::hard_link(source, &dest).is_err() {
            std::fs::copy(source, &dest)?;
        }
        platform_support::set_permissions_executable(&dest)
    })();

    match install_result {
        Ok(()) => {
            if let Some(aside) = &aside {
                platform_support::discard_swapped_aside(aside);
            }
            platform_support::prune_swapped_asides(&dest_dir);
            Ok(dest)
        }
        Err(error) => {
            if let Some(aside) = &aside {
                let _ = platform_support::restore_file_aside(aside, &dest);
            }
            Err(anyhow::Error::from(error).context(format!(
                "Failed to install {} at version path",
                source.display()
            )))
        }
    }
}

/// Install `source` at `dest`, replacing any existing file even when it is
/// locked by a running process.
///
/// Used by the archive-extraction install path, whose files land directly in
/// `builds/versions/<version>/`. Plain `remove_file` + `copy` aborted updates
/// with "Access is denied. (os error 5)" whenever the destination was the
/// image of a running AlphaCode instance.
pub fn replace_file_with(source: &Path, dest: &Path) -> Result<()> {
    let aside = platform_support::move_file_aside(dest)?;
    match std::fs::copy(source, dest) {
        Ok(_) => {
            if let Some(aside) = &aside {
                platform_support::discard_swapped_aside(aside);
            }
            platform_support::prune_swapped_asides(dest.parent().unwrap_or_else(|| Path::new(".")));
            Ok(())
        }
        Err(error) => {
            if let Some(aside) = &aside {
                let _ = platform_support::restore_file_aside(aside, dest);
            }
            Err(anyhow::Error::from(error).context(format!("Failed to install {}", dest.display())))
        }
    }
}

fn binary_source_metadata_path(binary: &Path) -> PathBuf {
    let file_name = binary
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| binary_stem().to_string());
    binary.with_file_name(format!("{file_name}.source.json"))
}

pub fn write_dev_binary_source_metadata(binary: &Path, source: &SourceState) -> Result<PathBuf> {
    let path = binary_source_metadata_path(binary);
    storage::write_json(&path, &DevBinarySourceMetadata::from(source))?;
    Ok(path)
}

pub fn write_current_dev_binary_source_metadata(
    repo_dir: &Path,
    source: &SourceState,
) -> Result<PathBuf> {
    let binary = find_dev_binary(repo_dir)
        .ok_or_else(|| anyhow::anyhow!("Binary not found in target/selfdev or target/release"))?;
    write_dev_binary_source_metadata(&binary, source)
}

fn read_binary_version_report(binary: &Path) -> Result<BinaryVersionReport> {
    let output = Command::new(binary)
        .args(["version", "--json"])
        .env("ALPHACODE_NON_INTERACTIVE", "1")
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "Binary smoke test failed for {} with exit code {:?}: {}",
            binary.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice(&output.stdout).map_err(|err| {
        anyhow::anyhow!(
            "Binary smoke test for {} returned invalid JSON: {}",
            binary.display(),
            err
        )
    })
}

pub fn smoke_test_binary(binary: &Path) -> Result<()> {
    let report = read_binary_version_report(binary)?;
    if report.version.as_deref().unwrap_or_default().is_empty() {
        anyhow::bail!(
            "Binary smoke test for {} returned JSON without a version field",
            binary.display()
        );
    }
    Ok(())
}

fn validate_binary_version_matches_source_report(
    report: &BinaryVersionReport,
    binary: &Path,
    source: &SourceState,
) -> Result<()> {
    let git_hash = report.git_hash.as_deref().unwrap_or_default();
    if git_hash.is_empty() {
        anyhow::bail!(
            "Binary {} version report did not include git_hash; rebuild before publishing {}",
            binary.display(),
            source.version_label
        );
    }
    if git_hash != source.short_hash {
        anyhow::bail!(
            "Refusing to publish {} as {}: binary was built from git hash {}, but source state is {}",
            binary.display(),
            source.version_label,
            git_hash,
            source.short_hash
        );
    }
    Ok(())
}

fn dirty_status_paths(repo_dir: &Path) -> Result<Vec<(PathBuf, bool)>> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(repo_dir)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git status failed while validating dirty build freshness with status {:?}",
            output.status.code()
        );
    }

    let mut entries = output.stdout.split(|byte| *byte == 0).peekable();
    let mut paths = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.is_empty() || entry.len() < 4 {
            continue;
        }
        let x = entry[0];
        let y = entry[1];
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        let deleted = x == b'D' || y == b'D';
        paths.push((PathBuf::from(path), deleted));

        if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C') {
            let _ = entries.next();
        }
    }

    Ok(paths)
}

fn validate_dirty_binary_freshness_without_metadata(
    repo_dir: &Path,
    binary: &Path,
    source: &SourceState,
) -> Result<()> {
    if !source.dirty {
        return Ok(());
    }

    let binary_mtime = std::fs::metadata(binary)
        .and_then(|metadata| metadata.modified())
        .map_err(|err| {
            anyhow::anyhow!(
                "Could not read binary modification time for {}: {}",
                binary.display(),
                err
            )
        })?;
    let dirty_paths = dirty_status_paths(repo_dir)?;
    let mut unverifiable = Vec::new();
    let mut newer_than_binary = Vec::new();

    for (relative, deleted) in dirty_paths {
        if deleted {
            unverifiable.push(relative.display().to_string());
            continue;
        }
        let path = repo_dir.join(&relative);
        let modified = match std::fs::metadata(&path).and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(_) => {
                unverifiable.push(relative.display().to_string());
                continue;
            }
        };
        if modified > binary_mtime {
            newer_than_binary.push(relative.display().to_string());
        }
    }

    if !unverifiable.is_empty() {
        anyhow::bail!(
            "Refusing to publish dirty build {} without source metadata: these changed paths cannot be checked against the binary timestamp: {}",
            source.version_label,
            unverifiable.join(", ")
        );
    }
    if !newer_than_binary.is_empty() {
        anyhow::bail!(
            "Refusing to publish stale dirty build {}: changed paths are newer than {}: {}",
            source.version_label,
            binary.display(),
            newer_than_binary.join(", ")
        );
    }

    Ok(())
}

fn validate_dev_binary_source_metadata(binary: &Path, source: &SourceState) -> Result<bool> {
    let path = binary_source_metadata_path(binary);
    if !path.exists() {
        return Ok(false);
    }

    let metadata: DevBinarySourceMetadata = storage::read_json(&path)?;
    if metadata.source_fingerprint != source.fingerprint
        || metadata.version_label != source.version_label
        || metadata.short_hash != source.short_hash
        || metadata.full_hash != source.full_hash
        || metadata.dirty != source.dirty
    {
        anyhow::bail!(
            "Refusing to publish {} as {}: source metadata at {} was for {} ({})",
            binary.display(),
            source.version_label,
            path.display(),
            metadata.version_label,
            metadata.source_fingerprint
        );
    }
    Ok(true)
}

fn validate_dev_binary_matches_source(
    repo_dir: &Path,
    binary: &Path,
    source: &SourceState,
) -> Result<()> {
    let report = read_binary_version_report(binary)?;
    if report.version.as_deref().unwrap_or_default().is_empty() {
        anyhow::bail!(
            "Binary smoke test for {} returned JSON without a version field",
            binary.display()
        );
    }
    validate_binary_version_matches_source_report(&report, binary, source)?;
    if !validate_dev_binary_source_metadata(binary, source)? {
        validate_dirty_binary_freshness_without_metadata(repo_dir, binary, source)?;
    }
    Ok(())
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeTestReplyKind {
    Ack,
    Pong,
}

#[cfg(unix)]
fn smoke_test_server_request(
    stream: &mut BufReader<std::os::unix::net::UnixStream>,
    request: &serde_json::Value,
    expected_reply_kind: SmokeTestReplyKind,
    expected_reply_id: u64,
) -> Result<()> {
    let payload = serde_json::to_string(request)? + "\n";
    stream.get_mut().write_all(payload.as_bytes())?;
    stream.get_mut().flush()?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut line = String::new();
        let bytes = stream.read_line(&mut line)?;
        if bytes == 0 {
            anyhow::bail!(
                "server closed the smoke-test socket before sending {:?} {}",
                expected_reply_kind,
                expected_reply_id
            );
        }
        let value: serde_json::Value = serde_json::from_str(line.trim()).map_err(|err| {
            anyhow::anyhow!("server smoke test returned invalid JSON line: {}", err)
        })?;
        let reply_type = value.get("type").and_then(|t| t.as_str());
        let reply_id = value.get("id").and_then(|id| id.as_u64());
        let kind_matches = match expected_reply_kind {
            SmokeTestReplyKind::Ack => reply_type == Some("ack"),
            SmokeTestReplyKind::Pong => reply_type == Some("pong"),
        };
        if kind_matches && reply_id == Some(expected_reply_id) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for {:?} {} during server smoke test",
                expected_reply_kind,
                expected_reply_id
            );
        }
    }
}

#[cfg(unix)]
fn smoke_test_server_connect(
    path: &Path,
) -> std::io::Result<BufReader<std::os::unix::net::UnixStream>> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(BufReader::new(stream))
}

#[cfg(unix)]
fn smoke_test_server_protocol(path: &Path, working_dir: &str) -> Result<()> {
    // The server handles an initial Ping on a dedicated lightweight-control
    // connection and closes it after replying, so the subscribed-client probe
    // must use a fresh socket.
    {
        let mut stream = smoke_test_server_connect(path)?;
        smoke_test_server_request(
            &mut stream,
            &serde_json::json!({
                "type": "ping",
                "id": 1
            }),
            SmokeTestReplyKind::Pong,
            1,
        )?;
    }

    let mut stream = smoke_test_server_connect(path)?;
    smoke_test_server_request(
        &mut stream,
        &serde_json::json!({
            "type": "subscribe",
            "id": 2,
            "working_dir": working_dir
        }),
        SmokeTestReplyKind::Ack,
        2,
    )?;
    Ok(())
}

#[cfg(unix)]
pub fn smoke_test_server_binary(binary: &Path) -> Result<()> {
    use std::fs::File;
    use std::process::Stdio;
    use std::thread;

    smoke_test_binary(binary)?;

    let temp = tempfile::tempdir()?;
    let runtime_dir = temp.path().join("runtime");
    storage::ensure_dir(&runtime_dir)?;
    let socket_path = temp.path().join("alphacode-smoke.sock");
    let stderr_path = temp.path().join("alphacode-smoke.stderr.log");
    let stderr = File::create(&stderr_path)?;

    let mut child = Command::new(binary)
        .arg("serve")
        .arg("--socket")
        .arg(&socket_path)
        .env("ALPHACODE_NON_INTERACTIVE", "1")
        .env("ALPHACODE_RUNTIME_DIR", &runtime_dir)
        .env("ALPHACODE_GATEWAY_ENABLED", "0")
        .env("ALPHACODE_TEMP_SERVER", "1")
        .env("ALPHACODE_SERVER_OWNER_PID", std::process::id().to_string())
        .env("ALPHACODE_TEMP_SERVER_IDLE_SECS", "300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()?;

    let result = (|| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait()? {
                let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
                anyhow::bail!(
                    "server smoke test process exited early with status {:?}: {}",
                    status.code(),
                    stderr.trim()
                );
            }

            match smoke_test_server_connect(&socket_path) {
                Ok(_) => {
                    smoke_test_server_protocol(&socket_path, env!("CARGO_MANIFEST_DIR"))?;
                    return Ok(());
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::ConnectionRefused
                            | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if Instant::now() >= deadline {
                        let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
                        anyhow::bail!(
                            "timed out waiting for server smoke test socket {}: {}",
                            socket_path.display(),
                            stderr.trim()
                        );
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err.into()),
            }
        }
    })();

    let _ = child.kill();
    let shutdown_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= shutdown_deadline {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    result
}

#[cfg(not(unix))]
pub fn smoke_test_server_binary(binary: &Path) -> Result<()> {
    smoke_test_binary(binary)
}

fn update_channel_symlink(channel: &str, version: &str) -> Result<PathBuf> {
    let channel_dir = builds_dir()?.join(channel);
    storage::ensure_dir(&channel_dir)?;

    let link_path = channel_dir.join(binary_name());
    let target = version_binary_path(version)?;
    if !target.exists() {
        anyhow::bail!("Version binary not found at {:?}", target);
    }

    let temp = channel_dir.join(format!(
        ".{}-{}-{}",
        binary_stem(),
        channel,
        std::process::id()
    ));
    crate::alphacode_build_support::platform_support::atomic_symlink_swap(
        &target, &link_path, &temp,
    )?;

    Ok(link_path)
}

/// Update stable symlink to point to a version and publish stable-version marker.
pub fn update_stable_symlink(version: &str) -> Result<PathBuf> {
    let stable_link = update_channel_symlink("stable", version)?;
    std::fs::write(stable_version_file()?, version)?;
    Ok(stable_link)
}

/// Update current symlink to point to a version and publish current-version marker.
pub fn update_current_symlink(version: &str) -> Result<PathBuf> {
    let current_link = update_channel_symlink("current", version)?;
    std::fs::write(current_version_file()?, version)?;
    Ok(current_link)
}

/// Update the shared server symlink to point to a version and publish the
/// shared-server-version marker.
pub fn update_shared_server_symlink(version: &str) -> Result<PathBuf> {
    let shared_link = update_channel_symlink("shared-server", version)?;
    std::fs::write(shared_server_version_file()?, version)?;
    Ok(shared_link)
}

pub fn publish_local_current_build_for_source(
    repo_dir: &Path,
    source: &SourceState,
) -> Result<PublishedBuild> {
    let binary = find_dev_binary(repo_dir)
        .ok_or_else(|| anyhow::anyhow!("Binary not found in target/selfdev or target/release"))?;
    if !binary.exists() {
        anyhow::bail!("Binary not found at {:?}", binary);
    }

    validate_dev_binary_matches_source(repo_dir, &binary, source)?;
    let previous_current_version = read_current_version()?;
    let versioned_path = install_binary_at_version(&binary, &source.version_label)?;
    let installed_report = read_binary_version_report(&versioned_path)?;
    if installed_report
        .version
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        anyhow::bail!(
            "Binary smoke test for {} returned JSON without a version field",
            versioned_path.display()
        );
    }
    validate_binary_version_matches_source_report(&installed_report, &versioned_path, source)?;
    let current_link = update_current_symlink(&source.version_label)?;
    let launcher_link = update_launcher_symlink_to_current()?;

    Ok(PublishedBuild {
        version: source.version_label.clone(),
        source_fingerprint: source.fingerprint.clone(),
        versioned_path,
        current_link,
        launcher_link,
        previous_current_version,
    })
}

/// Install the local release binary into immutable versions and make it the active `current`
/// build + launcher, while keeping `stable` untouched.
pub fn publish_local_current_build(repo_dir: &std::path::Path) -> Result<PathBuf> {
    let source = current_source_state(repo_dir)?;
    Ok(publish_local_current_build_for_source(repo_dir, &source)?.versioned_path)
}

/// Promote an already installed immutable version onto the shared server channel.
pub fn promote_version_to_shared_server(version: &str) -> Result<Option<String>> {
    let previous = read_shared_server_version()?;
    update_shared_server_symlink(version)?;
    Ok(previous)
}

/// Returns true when the `shared-server` channel is merely tracking the
/// `stable` channel rather than pinned to a deliberately-promoted build (e.g. a
/// local self-dev binary).
///
/// Updates only advance `current`/`stable`, so the long-lived daemon's reload
/// target (`shared-server`) can drift behind an update. When the channel was
/// just following stable we want updates to carry it forward automatically;
/// when it was explicitly promoted to a self-dev build we must leave it alone
/// so an update never silently wipes that build out from under a force reload.
///
/// A never-promoted (missing/empty) shared-server marker counts as "tracking":
/// there is no deliberate build to protect, so it is safe for updates to begin
/// populating the channel.
pub fn shared_server_tracks_stable() -> Result<bool> {
    let shared = read_shared_server_version()?;
    let shared = shared.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(shared) = shared else {
        return Ok(true);
    };
    let stable = read_stable_version()?;
    let stable = stable.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(stable == Some(shared))
}

/// Advance the `shared-server` channel to `version`, but only when it is
/// currently tracking `stable` (see [`shared_server_tracks_stable`]). Returns
/// `Ok(true)` when the channel was advanced.
///
/// Callers in the update path MUST invoke this *before* moving the `stable`
/// marker, otherwise the pre-update comparison would always disagree.
pub fn advance_shared_server_if_tracking_stable(version: &str) -> Result<bool> {
    if shared_server_tracks_stable()? {
        update_shared_server_symlink(version)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Outcome of [`repair_stale_shared_server_channel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedServerRepair {
    /// The `shared-server` channel was repointed at the installed `stable`
    /// release because stable was strictly newer on disk.
    Repaired {
        previous: Option<String>,
        repaired_to: String,
    },
    /// Nothing to do: shared-server is already at/newer than stable, or there is
    /// no usable stable target.
    AlreadyCurrent,
}

/// Drag a *stale* `shared-server` channel forward to the installed `stable`
/// release so a long-lived daemon can actually reload into a newer binary.
///
/// This is the client-side counterpart to [`advance_shared_server_if_tracking_stable`].
/// Updates advance `stable` but only advance `shared-server` *during the install
/// path*; a client that is already on the newest release (so `/update` is a
/// no-op) never re-runs that install path, leaving a long-lived older daemon
/// pinned to its old `shared-server` binary forever. A newer client that detects
/// an older server calls this to repoint `shared-server` -> `stable` before
/// asking the server to reload, so the forced reload has a strictly-newer target
/// to exec into instead of re-execing the same old binary (the "current client,
/// stale server" report).
///
/// Safety: we only repair when the `stable` binary is *strictly newer by mtime*
/// than the current `shared-server` binary. That preserves a deliberately-pinned
/// self-dev `shared-server` build whenever it is at least as fresh as stable (the
/// case the pin exists to protect), and never downgrades the channel.
pub fn repair_stale_shared_server_channel() -> Result<SharedServerRepair> {
    let stable_version = read_stable_version()?;
    let Some(stable_version) = stable_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(SharedServerRepair::AlreadyCurrent);
    };

    let stable_binary = stable_binary_path()?;
    if !stable_binary.exists() {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }

    // If shared-server already resolves to the same version marker, there is
    // nothing to repair.
    let previous = read_shared_server_version()?;
    if previous.as_deref().map(str::trim).filter(|s| !s.is_empty()) == Some(stable_version) {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }
    if previous
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some_and(|previous| !is_release_channel_marker(previous))
    {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }

    // Only repair when stable is strictly newer than the current shared-server
    // binary on disk. This never downgrades, and it preserves a self-dev pin
    // that is fresher than stable.
    let shared_binary = shared_server_binary_path()?;
    if !shared_server_binary_is_strictly_older_than(&shared_binary, &stable_binary) {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }

    update_shared_server_symlink(stable_version)?;
    Ok(SharedServerRepair::Repaired {
        previous: previous
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        repaired_to: stable_version.to_string(),
    })
}

fn is_release_channel_marker(marker: &str) -> bool {
    let marker = marker.trim();
    let marker = marker.strip_prefix('v').unwrap_or(marker);
    marker.starts_with("main-")
        || marker
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

/// True when `shared` exists and is strictly older (by mtime) than `stable`, or
/// when `shared` is missing entirely (nothing to protect). Any mtime
/// uncertainty on an existing shared binary is treated as "not older" so we
/// never repair away an unverifiable (possibly newer) pinned build.
///
/// Both paths are resolved through [`resolve_binary_payload`] so release
/// installs (wrapper script + `.bin` payload) compare the payloads that
/// actually run instead of the tiny wrapper scripts, whose mtimes carry no
/// version information.
pub(super) fn shared_server_binary_is_strictly_older_than(
    shared: &std::path::Path,
    stable: &std::path::Path,
) -> bool {
    let mtime = |p: &std::path::Path| {
        std::fs::metadata(resolve_binary_payload(p))
            .ok()
            .and_then(|m| m.modified().ok())
    };
    let stable_mtime = match mtime(stable) {
        Some(m) => m,
        None => return false,
    };
    if !shared.exists() {
        // No deliberate pin on disk; safe to point the channel at stable.
        return true;
    }
    match mtime(shared) {
        Some(shared_mtime) => shared_mtime < stable_mtime,
        None => false,
    }
}

/// Install release binary into immutable versions, promote it to stable, and also make it the
/// active current/launcher build.
pub fn install_local_release(repo_dir: &std::path::Path) -> Result<PathBuf> {
    let source = release_binary_path(repo_dir);
    if !source.exists() {
        anyhow::bail!("Binary not found at {:?}", source);
    }

    let version = repo_build_version(repo_dir)?;

    let versioned = install_binary_at_version(&source, &version)?;
    update_stable_symlink(&version)?;
    update_current_symlink(&version)?;
    update_shared_server_symlink(&version)?;
    update_launcher_symlink_to_current()?;

    Ok(versioned)
}

/// Copy binary to versioned location
pub fn install_version(repo_dir: &std::path::Path, hash: &str) -> Result<PathBuf> {
    let source = release_binary_path(repo_dir);
    install_binary_at_version(&source, hash)
}

/// Update canary symlink to point to a version
pub fn update_canary_symlink(hash: &str) -> Result<()> {
    let _ = update_channel_symlink("canary", hash)?;
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod safe_version_component_tests {
    use super::safe_version_component;

    /// Ordinary tags must pass through unchanged, minus the `v` prefix.
    #[test]
    fn accepts_ordinary_versions() {
        for raw in [
            "v1.0.64",
            "1.0.64",
            "v1.2.3-rc.1",
            "v1.0.0+build.5",
            "v0.1.0_alpha",
            "main-abc1234",
        ] {
            let v = safe_version_component(raw)
                .unwrap_or_else(|e| panic!("expected {raw:?} to be accepted: {e}"));
            assert!(!v.contains('\\') && !v.contains('/'), "{raw:?} -> {v:?}");
            assert!(
                !v.starts_with('v'),
                "the `v` prefix should be stripped: {v:?}"
            );
        }
    }

    /// Anything that could escape `builds/versions/` must be rejected. The tag
    /// comes off the GitHub releases API, and `version_is_newer` only reads the
    /// first three dot-separated tokens (coercing parse failures to 0), so
    /// `v1.0.0/../../../../Startup` compares as `(1, 0, 0)` and beats any
    /// installed `0.x.y`.
    #[test]
    fn rejects_traversal_and_path_separators() {
        for raw in [
            "v1.0.0/../../../../Startup",
            "v1.0.0\\..\\..\\Windows",
            "../../../etc",
            "..",
            ".",
            "v1..2",
            ".hidden",
            "trailing.",
            "v1.0.0/Windows/System32",
        ] {
            assert!(
                safe_version_component(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }

    /// No separators at all may appear, and the length is bounded so a hostile
    /// tag cannot produce an absurd path component.
    #[test]
    fn rejects_unsafe_characters_and_overlong_input() {
        for raw in [
            "v1.0.0; rm -rf ~",
            "v1.0.0|cat",
            "v1.0.0$(id)",
            "v1.0.0`id`",
            "v1.0.0:stream",
            "",
            "v",
        ] {
            assert!(
                safe_version_component(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
        let long = format!("v{}", "1".repeat(200));
        assert!(safe_version_component(&long).is_err());
    }

    /// Surrounding whitespace is trimmed rather than rejected: a tag with a
    /// stray trailing space should still install, and trimming it is what makes
    /// the space harmless.
    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            safe_version_component("  v1.0.64  ").unwrap(),
            "1.0.64",
            "whitespace should be trimmed, not treated as path-unsafe"
        );
    }
}
