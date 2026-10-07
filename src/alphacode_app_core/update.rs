use crate::alphacode_app_core::build;
use crate::alphacode_app_core::storage;
use crate::alphacode_update_core::{
    BACKGROUND_UPDATE_THRESHOLD, estimate_release_update_duration, estimate_source_update_duration,
    format_duration_estimate, get_asset_name, is_archive_name, summarize_git_pull_failure,
    update_estimate, verify_asset_checksum_text, version_is_newer,
};
pub use crate::alphacode_update_core::{
    DownloadProgress, GIT_PULL_DIVERGED_SUMMARY, GitHubAsset, GitHubRelease, PreparedUpdate,
    UpdateCheckResult, UpdateEstimate, format_download_progress_bar, summarize_update_error,
    summary_is_divergence,
};
use anyhow::{Context, Result};

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[path = "update_metadata.rs"]
mod update_metadata;
#[path = "update_rate_limit.rs"]
mod update_rate_limit;
pub use update_metadata::UpdateMetadata;
use update_metadata::{record_release_update_duration, record_source_update_duration};
use update_rate_limit::rate_limit_error;
pub use update_rate_limit::{RATE_LIMIT_ERROR_PREFIX, is_rate_limit_error};

const GITHUB_REPO: &str = "dragonked2/alphacode";
/// Minimum gap between *automatic* update checks.
///
/// Every automatic check costs one or two unauthenticated `api.github.com`
/// requests, which share a 60 req/hour per-IP bucket with everything else on
/// the machine (and everything behind the same NAT). A 60s gap meant a user
/// who opens alphacode a few dozen times an hour exhausted the bucket and then saw
/// spurious 403s. Half an hour is far below any realistic release cadence and
/// keeps automatic checks to at most a couple of requests per hour.
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// Time allowed for the initial TCP/TLS connect to the download host.
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Total wall-clock budget for a single download *attempt*.
///
/// This is intentionally a per-attempt budget, not a budget for the whole
/// asset. The old code used a single 120s total timeout for the entire
/// transfer, so on a slow link a multi-megabyte asset could never finish: it
/// was killed mid-stream, the partial bytes were discarded, and every relaunch
/// restarted from zero. We now cap each attempt and resume via HTTP Range, so
/// a slow-but-progressing download completes across several attempts while a
/// genuinely hung connection still gets bounded.
const DOWNLOAD_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(120);
/// How many *consecutive* stalled attempts (attempts that made no forward
/// progress) to tolerate before giving up. Any attempt that downloads new
/// bytes resets this counter, so a slow-but-progressing download keeps
/// resuming via HTTP Range for as long as it needs; only a genuinely stuck
/// connection eventually fails.
const DOWNLOAD_MAX_ATTEMPTS: usize = 10;
const DOWNLOAD_PROGRESS_UPDATE_STEP: u64 = 1_048_576;
pub fn print_centered(msg: &str) {
    let msg = crate::output_style::terminal_text(msg);
    let width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    for line in msg.lines() {
        let visible_len = unicode_display_width(line);
        if visible_len >= width {
            println!("{}", line);
        } else {
            let pad = (width - visible_len) / 2;
            println!("{:>pad$}{}", "", line, pad = pad);
        }
    }
}

fn unicode_display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    let mut w = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        w += UnicodeWidthChar::width(c).unwrap_or(0);
    }
    w
}

pub fn is_release_build() -> bool {
    crate::alphacode_build_meta::is_release_build()
}

fn current_update_semver() -> &'static str {
    crate::alphacode_build_meta::update_semver()
}

fn source_build_root() -> Result<PathBuf> {
    Ok(storage::alphacode_dir()?.join("builds").join("source"))
}

fn source_build_repo_dir() -> Result<PathBuf> {
    Ok(source_build_root()?.join("alphacode"))
}

pub fn should_auto_update() -> bool {
    if std::env::var("ALPHACODE_NO_AUTO_UPDATE").is_ok() {
        return false;
    }

    if !is_release_build() {
        return false;
    }

    if let Ok(exe) = std::env::current_exe()
        && is_inside_git_repo(&exe)
    {
        return false;
    }

    true
}

pub fn run_git_pull_ff_only(repo_dir: &Path, quiet: bool) -> Result<()> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("pull").arg("--ff-only");
    if quiet {
        cmd.arg("-q");
    }
    let output = cmd
        .current_dir(repo_dir)
        .output()
        .context("Failed to run git pull")?;

    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!("{}", summarize_git_pull_failure(&output.stderr));
    }
}

fn is_inside_git_repo(path: &std::path::Path) -> bool {
    let mut dir = if path.is_dir() {
        Some(path)
    } else {
        path.parent()
    };

    while let Some(d) = dir {
        if d.join(".git").exists() {
            return true;
        }
        dir = d.parent();
    }
    false
}

/// Maximum number of retry attempts for update checks.
const UPDATE_CHECK_MAX_RETRIES: u32 = 3;
/// Initial backoff delay for retry attempts.
const UPDATE_CHECK_RETRY_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
/// Maximum backoff delay for retry attempts.
const UPDATE_CHECK_RETRY_BACKOFF_MAX: Duration = Duration::from_secs(8);

pub fn fetch_latest_release_blocking() -> Result<GitHubRelease> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(UPDATE_CHECK_TIMEOUT)
        .user_agent(crate::alphacode_provider_core::with_alphacode_brand(
            "Alphacode updater",
        ))
        .build()?;

    let mut last_error = None;
    for attempt in 0..=UPDATE_CHECK_MAX_RETRIES {
        if attempt > 0 {
            let backoff = UPDATE_CHECK_RETRY_BACKOFF_INITIAL
                .mul_f32(2_f32.powi((attempt - 1) as i32))
                .min(UPDATE_CHECK_RETRY_BACKOFF_MAX);
            std::thread::sleep(backoff);
        }

        match github_api_request(&client, &url).send() {
            Ok(response) => {
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    anyhow::bail!("No releases found");
                }

                if let Some(error) = rate_limit_error(&response) {
                    return Err(error);
                }

                if !response.status().is_success() {
                    let status = response.status();
                    if status.as_u16() >= 500 && attempt < UPDATE_CHECK_MAX_RETRIES {
                        last_error = Some(anyhow::anyhow!(
                            "GitHub API server error: {} (attempt {}/{})",
                            status,
                            attempt + 1,
                            UPDATE_CHECK_MAX_RETRIES + 1
                        ));
                        continue;
                    }
                    anyhow::bail!("GitHub API error: {}", status);
                }

                let release: GitHubRelease =
                    response.json().context("Failed to parse release info")?;
                return Ok(release);
            }
            Err(e) => {
                if attempt < UPDATE_CHECK_MAX_RETRIES {
                    last_error = Some(e.into());
                    continue;
                }
                return Err(e.into());
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Update check failed after retries")))
}

fn github_api_request(
    client: &reqwest::blocking::Client,
    url: &str,
) -> reqwest::blocking::RequestBuilder {
    let request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    // Authenticated requests use the user's 5000 req/h quota instead of the
    // shared unauthenticated 60 req/h per-IP bucket, which other tools on the
    // same machine or NAT can exhaust and cause spurious 403s on update
    // checks. Falls back to unauthenticated when no token is available.
    if let Some(token) = crate::alphacode_base::github::github_public_api_token() {
        request.bearer_auth(token)
    } else {
        request
    }
}

fn latest_main_sha_blocking() -> Result<String> {
    let url = format!("https://api.github.com/repos/{}/commits/main", GITHUB_REPO);
    let client = reqwest::blocking::Client::builder()
        .timeout(UPDATE_CHECK_TIMEOUT)
        .user_agent(crate::alphacode_provider_core::with_alphacode_brand(
            "Alphacode updater",
        ))
        .build()?;

    let response = github_api_request(&client, &url)
        .send()
        .context("Failed to check main branch")?;
    if let Some(error) = rate_limit_error(&response) {
        return Err(error);
    }
    if !response.status().is_success() {
        anyhow::bail!("GitHub API error checking main: {}", response.status());
    }

    let commit: serde_json::Value = response.json().context("Failed to parse commit info")?;
    Ok(commit["sha"]
        .as_str()
        .unwrap_or("")
        .get(..7)
        .unwrap_or("")
        .to_string())
}

fn platform_asset(release: &GitHubRelease) -> Result<&GitHubAsset> {
    let asset_name = get_asset_name();
    release
        .assets
        .iter()
        .find(|a| a.name.starts_with(asset_name) && is_archive_name(&a.name))
        .ok_or_else(|| anyhow::anyhow!("No asset found for platform: {}", asset_name))
}

fn checksum_asset(release: &GitHubRelease) -> Option<&GitHubAsset> {
    release.assets.iter().find(|a| a.name == "SHA256SUMS")
}

/// Maximum attempts to fetch `SHA256SUMS`, including the first.
///
/// The release assets and `SHA256SUMS` do not appear at the same moment.
/// In `release.yml` every `build` job uploads its archive straight to the
/// release, and the separate `release` job merges and uploads `SHA256SUMS`
/// afterwards. So a client can legitimately observe a release that lists
/// `SHA256SUMS` while that very URL is still propagating, and GitHub answers
/// `404` for a short window. The archive download already tolerates far worse
/// (10 attempts with HTTP Range resume); fetching a 571-byte text file with no
/// retry at all meant a single transient 404 aborted the entire update.
const CHECKSUM_MAX_ATTEMPTS: u32 = 6;
/// Initial backoff between `SHA256SUMS` fetch attempts.
const CHECKSUM_RETRY_BACKOFF_INITIAL: Duration = Duration::from_secs(2);
/// Maximum backoff between `SHA256SUMS` fetch attempts.
const CHECKSUM_RETRY_BACKOFF_MAX: Duration = Duration::from_secs(16);

/// Whether a failed `SHA256SUMS` fetch is worth retrying.
///
/// `404` is the interesting one: the asset is listed in the release response,
/// so its absence at the CDN edge is propagation lag, not a downgrade. `403`
/// can be a short-lived rate limit. Both resolve on their own. Any other
/// status is treated as final so a genuine problem still surfaces promptly.
fn is_retryable_checksum_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND
        || status == reqwest::StatusCode::FORBIDDEN
        || status.is_server_error()
}

fn fetch_checksum_manifest(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    let mut last_status: Option<reqwest::StatusCode> = None;
    let mut last_error: Option<reqwest::Error> = None;

    for attempt in 0..CHECKSUM_MAX_ATTEMPTS {
        if attempt > 0 {
            let backoff = CHECKSUM_RETRY_BACKOFF_INITIAL
                .mul_f32(2_f32.powi((attempt - 1) as i32))
                .min(CHECKSUM_RETRY_BACKOFF_MAX);
            crate::logging::info(&format!(
                "Retrying SHA256SUMS download in {}s (attempt {}/{})",
                backoff.as_secs(),
                attempt + 1,
                CHECKSUM_MAX_ATTEMPTS
            ));
            std::thread::sleep(backoff);
        }

        match client.get(url).send() {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return response.text().context("Failed to read SHA256SUMS");
                }
                if !is_retryable_checksum_status(status) {
                    anyhow::bail!("SHA256SUMS download failed: {}", status);
                }
                last_status = Some(status);
            }
            Err(error) => {
                // Transport hiccups (DNS, reset connection) are worth one more go.
                last_error = Some(error);
            }
        }
    }

    // Every attempt was a retryable failure. Stay fail-closed — the whole point
    // of the check is that we never install unverified — but say what happened
    // so the user knows to simply run the update again rather than concluding
    // their install is broken.
    let detail = match (last_status, last_error) {
        (Some(status), _) => format!("still {status} after {CHECKSUM_MAX_ATTEMPTS} attempts"),
        (None, Some(error)) => format!("{error}"),
        _ => "unknown error".to_string(),
    };
    anyhow::bail!(
        "SHA256SUMS download failed: {detail}. The release may still be publishing — \
         wait a minute and run the update again."
    );
}

fn verify_asset_checksum_if_available(
    client: &reqwest::blocking::Client,
    release: &GitHubRelease,
    asset: &GitHubAsset,
    bytes: &[u8],
) -> Result<()> {
    let Some(checksum_asset) = checksum_asset(release) else {
        // Fail closed. A missing SHA256SUMS is reachable with no attacker at
        // all: in release.yml the archives are uploaded by each `build` job,
        // while SHA256SUMS is uploaded by a separate `release` job that
        // `needs: build`. If that job fails, is cancelled, or its
        // create-release step breaks, the archives are already public and every
        // client would silently install them unverified. Anyone who can
        // influence the release response reaches the same downgrade just by
        // omitting the asset, so its absence must be fatal rather than logged.
        //
        // Note this is deliberately *not* retried: the asset being absent from
        // the release payload is a real condition, not propagation lag, and
        // retrying would only delay refusing it.
        anyhow::bail!(
            "Release {} does not publish SHA256SUMS; refusing to install an unverified binary",
            release.tag_name
        );
    };

    let contents = fetch_checksum_manifest(client, &checksum_asset.browser_download_url)?;
    verify_asset_checksum_text(&contents, &asset.name, bytes)?;
    crate::logging::info(&format!("Verified SHA256 checksum for {}", asset.name));
    Ok(())
}

fn synthetic_main_release(latest_sha: &str) -> GitHubRelease {
    GitHubRelease {
        tag_name: format!("main-{}", latest_sha),
        _name: Some(format!("Built from main ({})", latest_sha)),
        _html_url: format!("https://github.com/{}/commit/{}", GITHUB_REPO, latest_sha),
        _published_at: None,
        assets: vec![],
        _target_commitish: latest_sha.to_string(),
    }
}

fn install_main_source_update_blocking(latest_sha: &str) -> Result<PathBuf> {
    use crate::alphacode_app_core::bus::{
        Bus, BusEvent, ClientMaintenanceAction, SessionUpdateStatus,
    };
    let action = ClientMaintenanceAction::Update;
    let sha = latest_sha.to_string();
    let path = build_from_source_with_progress(|phase| {
        let message = format!("{} (main-{})...", phase.label(), sha);
        crate::logging::info(&message);
        Bus::global().publish(BusEvent::SessionUpdateStatus(SessionUpdateStatus::Status {
            session_id: String::new(), // broadcast to all
            action,
            message,
        }));
    })?;
    crate::logging::info(&format!(
        "Main channel: built successfully at {}",
        path.display()
    ));

    let mut metadata = UpdateMetadata::load().unwrap_or_default();
    let channel_version = format!("main-{}", latest_sha);
    build::install_binary_at_version(&path, &channel_version)
        .context("Failed to install built binary")?;
    // Carry the long-lived daemon's reload target forward too, but only when it
    // was tracking stable. A deliberately-promoted self-dev shared-server build
    // is left untouched so the update never silently wipes it out.
    if let Err(error) = build::advance_shared_server_if_tracking_stable(&channel_version) {
        crate::logging::warn(&format!(
            "update: failed to advance shared-server channel to {}: {}",
            channel_version, error
        ));
    }
    build::update_stable_symlink(&channel_version)?;
    build::update_current_symlink(&channel_version)?;
    build::update_launcher_symlink_to_current()?;

    metadata.installed_version = Some(channel_version.clone());
    metadata.installed_from = Some("source".to_string());
    metadata.last_check = SystemTime::now();
    metadata.save()?;

    Ok(path)
}

fn prepare_stable_update_blocking() -> Result<PreparedUpdate> {
    let current_version = crate::alphacode_build_meta::version();
    let current_update_version = current_update_semver();
    let release = fetch_latest_release_blocking()?;
    let release_version = release.tag_name.trim_start_matches('v');

    if release_version == current_update_version.trim_start_matches('v')
        || !version_is_newer(
            release_version,
            current_update_version.trim_start_matches('v'),
        )
    {
        return Ok(PreparedUpdate::None {
            current: current_version.to_string(),
        });
    }

    let Ok(asset) = platform_asset(&release) else {
        return Ok(PreparedUpdate::None {
            current: current_version.to_string(),
        });
    };
    let metadata = UpdateMetadata::load().unwrap_or_default();
    let duration = estimate_release_update_duration(asset._size, metadata.last_release_update_secs);
    let size_mb = asset._size as f64 / (1024.0 * 1024.0);
    let summary = format!(
        "Prebuilt update {} → {} (~{:.0} MB, {}). {}",
        current_version,
        release.tag_name,
        size_mb,
        format_duration_estimate(duration),
        if duration >= BACKGROUND_UPDATE_THRESHOLD {
            "Running in the background and will reload when it is ready."
        } else {
            "This should be quick."
        }
    );

    Ok(PreparedUpdate::Stable {
        release,
        estimate: update_estimate(summary, duration),
    })
}

fn prepare_main_update_blocking() -> Result<PreparedUpdate> {
    let current_hash = crate::alphacode_build_meta::git_hash();
    if current_hash.is_empty() || current_hash == "unknown" {
        crate::logging::info("Main channel: no git hash in binary, skipping update check");
        return Ok(PreparedUpdate::None {
            current: crate::alphacode_build_meta::version().to_string(),
        });
    }

    let latest_sha = latest_main_sha_blocking()?;
    if latest_sha.is_empty() {
        return Ok(PreparedUpdate::None {
            current: current_hash.to_string(),
        });
    }

    let current_short = if current_hash.len() >= 7 {
        &current_hash[..7]
    } else {
        current_hash
    };

    if current_short == latest_sha {
        crate::logging::info(&format!("Main channel: up to date ({})", current_short));
        return Ok(PreparedUpdate::None {
            current: format!("main-{}", current_short),
        });
    }

    crate::logging::info(&format!(
        "Main channel: new commit {} -> {}",
        current_short, latest_sha
    ));

    if has_cargo() {
        let repo_dir = source_build_repo_dir()?;
        let repo_exists = repo_dir.join(".git").exists();
        let has_previous_build = build::release_binary_path(&repo_dir).exists();
        let metadata = UpdateMetadata::load().unwrap_or_default();
        let duration = estimate_source_update_duration(
            repo_exists,
            has_previous_build,
            metadata.last_source_update_secs,
        );
        let action = if repo_exists {
            if has_previous_build {
                "git pull + cargo build with a warm build cache"
            } else {
                "git pull + cargo build"
            }
        } else {
            "initial clone + cargo build"
        };
        let summary = format!(
            "Source update {} → main-{} requires {} ({}). Running in the background and will reload when it is ready.",
            current_short,
            latest_sha,
            action,
            format_duration_estimate(duration)
        );
        return Ok(PreparedUpdate::MainSource {
            latest_sha,
            estimate: update_estimate(summary, duration),
        });
    }

    crate::logging::info("Main channel: cargo not found, falling back to latest release");
    prepare_stable_update_blocking()
}

pub fn prepare_update_blocking() -> Result<PreparedUpdate> {
    let channel = crate::config::config().features.update_channel;
    match channel {
        crate::config::UpdateChannel::Main => prepare_main_update_blocking(),
        crate::config::UpdateChannel::Stable => prepare_stable_update_blocking(),
    }
}

/// Log the full error and return a single short line for the UI.
///
/// Update failures come from many layers and are often multi-line, so the
/// verbose text belongs in the log while the card/notice stay one line.
fn short_update_error(context: &str, error: &anyhow::Error) -> String {
    crate::logging::warn(&format!("update: {}: {:#}", context, error));
    summarize_update_error(&format!("{:#}", error))
}

pub fn spawn_background_session_update(session_id: String) {
    std::thread::spawn(move || {
        use crate::alphacode_app_core::bus::{
            Bus, BusEvent, ClientMaintenanceAction, SessionUpdateStatus,
        };

        let action = ClientMaintenanceAction::Update;

        let publish = |status| Bus::global().publish(BusEvent::SessionUpdateStatus(status));

        match prepare_update_blocking() {
            Ok(PreparedUpdate::None { current }) => {
                publish(SessionUpdateStatus::NoUpdate {
                    session_id,
                    current,
                });
            }
            Ok(PreparedUpdate::Stable { release, estimate }) => {
                publish(SessionUpdateStatus::Status {
                    session_id: session_id.clone(),
                    action,
                    message: estimate.summary,
                });
                publish(SessionUpdateStatus::Status {
                    session_id: session_id.clone(),
                    action,
                    message: format!(
                        "Downloading {} (estimated {})...",
                        release.tag_name,
                        format_duration_estimate(estimate.duration)
                    ),
                });
                let progress_session_id = session_id.clone();
                let progress_version = release.tag_name.clone();
                match download_and_install_blocking_with_progress(&release, |progress| {
                    publish(SessionUpdateStatus::Status {
                        session_id: progress_session_id.clone(),
                        action,
                        message: format!(
                            "{} {}",
                            progress_version,
                            format_download_progress_bar(progress)
                        ),
                    });
                }) {
                    Ok(_) => publish(SessionUpdateStatus::ReadyToReload {
                        session_id,
                        action,
                        version: release.tag_name,
                    }),
                    Err(error) => publish(SessionUpdateStatus::Error {
                        session_id,
                        action,
                        message: short_update_error("update failed", &error),
                    }),
                }
            }
            Ok(PreparedUpdate::MainSource {
                latest_sha,
                estimate,
            }) => {
                publish(SessionUpdateStatus::Status {
                    session_id: session_id.clone(),
                    action,
                    message: format!(
                        "Update {} → main-{} ({})\n\n{}\n\nBuilding in the background. This may take a few minutes on the first run.",
                        crate::alphacode_build_meta::version(),
                        latest_sha,
                        format_duration_estimate(estimate.duration),
                        estimate.summary
                    ),
                });
                match install_main_source_update_blocking(&latest_sha) {
                    Ok(_) => publish(SessionUpdateStatus::ReadyToReload {
                        session_id,
                        action,
                        version: format!("main-{}", latest_sha),
                    }),
                    Err(error) => publish(SessionUpdateStatus::Error {
                        session_id,
                        action,
                        message: short_update_error("update failed", &error),
                    }),
                }
            }
            Err(error) => publish(SessionUpdateStatus::Error {
                session_id,
                action,
                message: short_update_error("update check failed", &error),
            }),
        }
    });
}

pub fn check_for_update_blocking() -> Result<Option<GitHubRelease>> {
    let channel = crate::config::config().features.update_channel;
    match channel {
        crate::config::UpdateChannel::Main => check_for_main_update_blocking(),
        crate::config::UpdateChannel::Stable => check_for_stable_update_blocking(),
    }
}

fn check_for_stable_update_blocking() -> Result<Option<GitHubRelease>> {
    let current_version = current_update_semver();
    let release = fetch_latest_release_blocking()?;

    let release_version = release.tag_name.trim_start_matches('v');
    if release_version == current_version.trim_start_matches('v') {
        return Ok(None);
    }

    if version_is_newer(release_version, current_version.trim_start_matches('v')) {
        let asset_name = get_asset_name();
        let has_asset = release
            .assets
            .iter()
            .any(|a| a.name.starts_with(asset_name) && is_archive_name(&a.name));

        if has_asset {
            return Ok(Some(release));
        }
    }

    Ok(None)
}

/// Check for updates on the main branch (cutting edge channel).
/// Compares the current binary's git hash against the latest commit on main.
/// If a new commit is found:
///   - Tries to build from source if cargo is available
///   - Falls back to latest GitHub Release if not
fn check_for_main_update_blocking() -> Result<Option<GitHubRelease>> {
    let current_hash = crate::alphacode_build_meta::git_hash();
    if current_hash.is_empty() || current_hash == "unknown" {
        crate::logging::info("Main channel: no git hash in binary, skipping update check");
        return Ok(None);
    }

    let latest_sha = latest_main_sha_blocking()?;

    if latest_sha.is_empty() {
        return Ok(None);
    }

    // Compare short hashes
    let current_short = if current_hash.len() >= 7 {
        &current_hash[..7]
    } else {
        current_hash
    };

    if current_short == latest_sha {
        crate::logging::info(&format!("Main channel: up to date ({})", current_short));
        return Ok(None);
    }

    crate::logging::info(&format!(
        "Main channel: new commit {} -> {}",
        current_short, latest_sha
    ));

    // Try to build from source
    if has_cargo() {
        crate::logging::info("Main channel: cargo found, attempting build from source");
        match install_main_source_update_blocking(&latest_sha) {
            Ok(_) => {
                return Ok(Some(synthetic_main_release(&latest_sha)));
            }
            Err(e) => {
                crate::logging::error(&format!("Main channel: build failed: {}", e));
                // Fall through to release fallback
            }
        }
    } else {
        crate::logging::info("Main channel: cargo not found, falling back to latest release");
    }

    // Fallback: use latest stable release if available
    if let Ok(release) = fetch_latest_release_blocking() {
        let asset_name = get_asset_name();
        let has_asset = release
            .assets
            .iter()
            .any(|a| a.name.starts_with(asset_name) && is_archive_name(&a.name));
        if has_asset {
            let release_version = release.tag_name.trim_start_matches('v');
            let current_version = current_update_semver().trim_start_matches('v');
            if version_is_newer(release_version, current_version) {
                return Ok(Some(release));
            }
        }
    }

    Ok(None)
}

/// Check if cargo is available on the system
fn has_cargo() -> bool {
    std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Progress phases for source builds, so the UI can show what's happening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceBuildPhase {
    Cloning,
    Pulling,
    Building,
    Installing,
}

impl SourceBuildPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cloning => "Cloning repository",
            Self::Pulling => "Pulling latest changes",
            Self::Building => "Building from source",
            Self::Installing => "Installing binary",
        }
    }
}

/// Build alphacode from source by cloning/pulling the repo and running cargo build.
/// Calls `on_phase` at each major step so the UI can show progress.
fn build_from_source_with_progress(mut on_phase: impl FnMut(SourceBuildPhase)) -> Result<PathBuf> {
    let started = Instant::now();
    let build_dir = source_build_root()?;
    fs::create_dir_all(&build_dir)?;

    let repo_dir = build_dir.join("alphacode");

    if repo_dir.join(".git").exists() {
        on_phase(SourceBuildPhase::Pulling);
        crate::logging::info("Main channel: pulling latest from main...");
        let output = std::process::Command::new("git")
            .args(["pull", "--ff-only", "origin", "main"])
            .current_dir(&repo_dir)
            .output()
            .context("Failed to run git pull")?;

        if !output.status.success() {
            let summary = summarize_git_pull_failure(&output.stderr);
            crate::logging::warn(&format!("{}, trying reset", summary));
            let output = std::process::Command::new("git")
                .args(["fetch", "origin", "main"])
                .current_dir(&repo_dir)
                .output()
                .context("Failed to run git fetch")?;
            if !output.status.success() {
                anyhow::bail!(
                    "git fetch failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let output = std::process::Command::new("git")
                .args(["reset", "--hard", "origin/main"])
                .current_dir(&repo_dir)
                .output()
                .context("Failed to run git reset")?;
            if !output.status.success() {
                anyhow::bail!(
                    "git reset failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    } else {
        on_phase(SourceBuildPhase::Cloning);
        crate::logging::info("Main channel: cloning repository...");
        let clone_url = format!("https://github.com/{}.git", GITHUB_REPO);
        let output = std::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                "main",
                &clone_url,
                "alphacode",
            ])
            .current_dir(&build_dir)
            .output()
            .context("Failed to run git clone")?;

        if !output.status.success() {
            anyhow::bail!(
                "git clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    on_phase(SourceBuildPhase::Building);
    crate::logging::info("Main channel: building with cargo...");
    let output = std::process::Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&repo_dir)
        .env("ALPHACODE_RELEASE_BUILD", "1")
        .output()
        .context("Failed to run cargo build")?;

    if !output.status.success() {
        anyhow::bail!(
            "cargo build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let binary = build::release_binary_path(&repo_dir);
    if !binary.exists() {
        anyhow::bail!("Built binary not found at {}", binary.display());
    }

    record_source_update_duration(started.elapsed());

    Ok(binary)
}

/// Download an asset into memory, retrying with HTTP Range resume so a slow or
/// flaky connection recovers instead of restarting from zero.
///
/// Returns the full asset bytes plus the best-known total size (for callers
/// that want a final size). Progress callbacks are invoked across retries using
/// the cumulative bytes already on disk, so the UI never appears to go
/// backwards when a stalled connection reconnects.
fn download_asset_with_resume(
    client: &reqwest::blocking::Client,
    download_url: &str,
    total_hint: Option<u64>,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<(Vec<u8>, Option<u64>)> {
    let mut bytes: Vec<u8> =
        Vec::with_capacity(total_hint.unwrap_or_default().min(usize::MAX as u64) as usize);
    let mut total = total_hint;
    let mut next_progress_at = 0_u64;
    let mut last_error: Option<anyhow::Error> = None;
    // Count only *consecutive stalls* (attempts that made no forward progress).
    // A slow-but-advancing download resets this and keeps resuming, so it can
    // take as long as it needs; only a genuinely stuck connection gives up.
    let mut stalls = 0_usize;
    let mut attempt = 0_usize;

    on_progress(DownloadProgress {
        downloaded: 0,
        total,
    });

    while stalls < DOWNLOAD_MAX_ATTEMPTS {
        attempt += 1;
        let resume_from = bytes.len() as u64;
        let mut request = client.get(download_url);
        if resume_from > 0 {
            // Ask the server to continue where we left off.
            request = request.header(reqwest::header::RANGE, format!("bytes={}-", resume_from));
        }

        let response = match request.send() {
            Ok(response) => response,
            Err(err) => {
                last_error = Some(anyhow::anyhow!("Failed to download update: {}", err));
                log_download_retry(attempt, resume_from, &err);
                stalls += 1;
                continue;
            }
        };

        let status = response.status();
        if resume_from > 0 && status == reqwest::StatusCode::OK {
            // Server ignored the Range header and is resending from the start;
            // discard the partial buffer so we don't corrupt the result.
            bytes.clear();
            next_progress_at = 0;
        } else if resume_from > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
            last_error = Some(anyhow::anyhow!(
                "Resume request returned unexpected status {}",
                status
            ));
            log_download_retry_status(attempt, resume_from, status);
            stalls += 1;
            continue;
        } else if !status.is_success() {
            last_error = Some(anyhow::anyhow!("Download failed: {}", status));
            log_download_retry_status(attempt, resume_from, status);
            stalls += 1;
            continue;
        }

        // Establish the total size. For a 206 response, content-length is the
        // remaining bytes, so prefer the original hint / Content-Range total.
        if total.is_none() {
            total = content_range_total(&response).or_else(|| {
                if status == reqwest::StatusCode::PARTIAL_CONTENT {
                    response
                        .content_length()
                        .map(|len| len.saturating_add(bytes.len() as u64))
                } else {
                    response.content_length()
                }
            });
        }

        let before = bytes.len() as u64;
        let read_result = read_response_into(
            response,
            &mut bytes,
            &mut next_progress_at,
            total,
            on_progress,
        );
        let made_progress = bytes.len() as u64 > before;

        match read_result {
            Ok(()) => {
                let downloaded = bytes.len() as u64;
                // If we know the total and fell short, treat as a stall and retry.
                if let Some(total) = total
                    && downloaded < total
                {
                    last_error = Some(anyhow::anyhow!(
                        "Download ended early ({} of {} bytes)",
                        downloaded,
                        total
                    ));
                    log_download_retry_short(attempt, downloaded, total);
                    stalls = if made_progress { 0 } else { stalls + 1 };
                    continue;
                }
                on_progress(DownloadProgress { downloaded, total });
                return Ok((bytes, total));
            }
            Err(err) => {
                let downloaded = bytes.len() as u64;
                crate::logging::warn(&format!(
                    "Update download attempt {} stream error at {} bytes: {}; retrying with resume",
                    attempt, downloaded, err
                ));
                last_error = Some(err);
                stalls = if made_progress { 0 } else { stalls + 1 };
                continue;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        anyhow::anyhow!(
            "Download stalled with no progress after {} attempts",
            DOWNLOAD_MAX_ATTEMPTS
        )
    }))
}

fn read_response_into(
    mut response: reqwest::blocking::Response,
    bytes: &mut Vec<u8>,
    next_progress_at: &mut u64,
    total: Option<u64>,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .context("Failed to read download")?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        let downloaded = bytes.len() as u64;
        if downloaded >= *next_progress_at || total.is_some_and(|total| downloaded >= total) {
            on_progress(DownloadProgress { downloaded, total });
            *next_progress_at = downloaded.saturating_add(DOWNLOAD_PROGRESS_UPDATE_STEP);
        }
    }
    Ok(())
}

fn content_range_total(response: &reqwest::blocking::Response) -> Option<u64> {
    // Content-Range: bytes 200-1023/1024
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('/').next())
        .and_then(|total| total.trim().parse::<u64>().ok())
}

fn log_download_retry(attempt: usize, resume_from: u64, err: &impl std::fmt::Display) {
    crate::logging::warn(&format!(
        "Update download attempt {}/{} failed at {} bytes: {}; retrying with resume",
        attempt, DOWNLOAD_MAX_ATTEMPTS, resume_from, err
    ));
}

fn log_download_retry_status(attempt: usize, resume_from: u64, status: reqwest::StatusCode) {
    crate::logging::warn(&format!(
        "Update download attempt {}/{} got status {} at {} bytes; retrying with resume",
        attempt, DOWNLOAD_MAX_ATTEMPTS, status, resume_from
    ));
}

fn log_download_retry_short(attempt: usize, downloaded: u64, total: u64) {
    crate::logging::warn(&format!(
        "Update download attempt {}/{} ended early ({} of {} bytes); retrying with resume",
        attempt, DOWNLOAD_MAX_ATTEMPTS, downloaded, total
    ));
}

/// Extract a `.zip` release asset into `extract_dir`, keeping only
/// top-level files.
///
/// Windows release assets are published as `.zip` (see `release.yml` and
/// `scripts/install.ps1`); every other platform uses `.tar.gz`.
///
/// # Entry filtering
///
/// `entry.name()` is the raw stored name with no normalisation, and the `zip`
/// crate documents that it points at [`enclosed_name`] for exactly this
/// reason. A separator-only filter rejects `../../evil` and `/etc/shadow`, but
/// an entry named `C:evil.exe` contains no separator at all and slips through
/// — and on Windows `Path::join` *replaces* the accumulated path when the
/// operand carries a drive prefix, so that write would land in the process CWD
/// on drive C:.
///
/// `enclosed_name()` rejects prefix/root components, refuses any `..` that
/// escapes, and rejects NUL bytes. Requiring exactly one `Component::Normal`
/// on top of that preserves the original "top-level files only, no
/// subdirectories" intent.
///
/// # Why the root is canonicalized once, up front
///
/// This used to canonicalize `extract_dir` inside the loop and compare it
/// against `extract_dir.join(file_name).parent()`. That comparison can never
/// succeed on Windows: [`Path::canonicalize`] returns a verbatim `\\?\`-prefixed
/// path, while the left-hand side was built from the non-canonical
/// `extract_dir`. The two spell the same directory differently, so the
/// containment `ensure!` failed for *every* entry and the Windows in-app updater
/// aborted with `zip entry ... resolved outside the extraction directory`
/// before writing a single byte. Since Windows ships `.zip` assets, that broke
/// 100% of Windows self-updates rather than an edge case.
///
/// Joining onto the already-canonical root makes containment structural:
/// `file_name` is a single `Component::Normal`, so
/// `base.join(file_name).parent() == base` holds by construction. The `ensure!`
/// stays as a tripwire against future regressions, and hoisting the
/// `canonicalize` out of the loop also drops a syscall per archive entry.
fn extract_zip_asset_into(bytes: &[u8], extract_dir: &Path) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("Failed to open zip archive")?;
    let base = extract_dir
        .canonicalize()
        .context("Failed to canonicalize extract dir")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let mut components = rel.components();
        let Some(std::path::Component::Normal(file_name)) = components.next() else {
            continue;
        };
        if components.next().is_some() {
            continue;
        }
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.is_empty() || file_name.ends_with(".zip") {
            continue;
        }
        let dest = base.join(file_name);
        // Belt and braces: prove containment after the join and before
        // creating anything on disk. `base` is already canonical, so this
        // compares canonical-to-canonical and is not defeated by the `\\?\`
        // verbatim prefix that `canonicalize` adds on Windows.
        anyhow::ensure!(
            dest.parent() == Some(base.as_path()),
            "zip entry {file_name:?} resolved outside the extraction directory"
        );
        let mut out_file = fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

pub fn download_and_install_blocking_with_progress(
    release: &GitHubRelease,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf> {
    let started = Instant::now();
    let asset_name = get_asset_name();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with(asset_name) && is_archive_name(&a.name))
        .ok_or_else(|| anyhow::anyhow!("No asset found for platform: {}", asset_name))?;

    let download_url = asset.browser_download_url.clone();

    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("alphacode-update-{}", std::process::id()));

    // The `timeout` here applies per request. Since each retry below is a
    // separate request, this acts as a *per-attempt* budget rather than a cap
    // on the whole asset: a slow-but-progressing download resumes via HTTP
    // Range across attempts (up to DOWNLOAD_MAX_ATTEMPTS), so it can complete
    // even when a single attempt would not finish in time. A genuinely hung
    // connection is still bounded by the per-attempt timeout.
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_ATTEMPT_TIMEOUT)
        .user_agent(crate::alphacode_provider_core::with_alphacode_brand(
            "Alphacode updater",
        ))
        .build()?;

    let total_hint = if asset._size > 0 {
        Some(asset._size)
    } else {
        None
    };
    let (bytes, _total) =
        download_asset_with_resume(&client, &download_url, total_hint, &mut on_progress)?;

    verify_asset_checksum_if_available(&client, release, asset, &bytes)?;

    let mut installed_version_dir: Option<PathBuf> = None;
    if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".zip") {
        let extract_dir = temp_path.with_extension("extract");
        if extract_dir.exists() {
            let _ = fs::remove_dir_all(&extract_dir);
        }
        fs::create_dir_all(&extract_dir).context("Failed to create archive extraction dir")?;

        if asset.name.ends_with(".tar.gz") {
            let cursor = std::io::Cursor::new(&bytes);
            let gz = flate2::read::GzDecoder::new(cursor);
            let mut archive = tar::Archive::new(gz);
            for entry in archive.entries()? {
                let mut entry = entry?;
                let entry_path = entry.path()?.into_owned();
                if entry_path.components().count() != 1 {
                    continue;
                }
                let file_name = entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if file_name.is_empty() || file_name.ends_with(".tar.gz") {
                    continue;
                }
                let dest = extract_dir.join(&file_name);
                entry.unpack(&dest)?;
            }
        } else {
            extract_zip_asset_into(&bytes, &extract_dir)?;
        }

        let mut extracted_binary: Option<PathBuf> = None;
        for entry in fs::read_dir(&extract_dir).context("Failed to read extracted archive")? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name_string = name.to_string_lossy();
            if name_string.starts_with("alphacode") && !name_string.ends_with(".bin") {
                extracted_binary = Some(entry.path());
                break;
            }
        }
        let Some(extracted_binary) = extracted_binary else {
            anyhow::bail!(
                "Could not find alphacode binary inside {} archive",
                if asset.name.ends_with(".tar.gz") {
                    "tar.gz"
                } else {
                    "zip"
                }
            );
        };
        crate::platform::set_permissions_executable(&extracted_binary)?;

        // `tag_name` comes straight off the releases API. Validate it before it
        // becomes a path component: see `safe_version_component`.
        let version_owned = build::safe_version_component(&release.tag_name)?;
        let version = version_owned.as_str();
        let dest_dir = build::builds_dir()?.join("versions").join(version);
        fs::create_dir_all(&dest_dir).context("Failed to create version install dir")?;
        let mut installed_files = Vec::new();
        for entry in fs::read_dir(&extract_dir).context("Failed to read extracted archive")? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name_string = name.to_string_lossy();
            let dest_name = if name_string == get_asset_name()
                || name_string == format!("{}.exe", get_asset_name())
            {
                build::binary_name().to_string()
            } else {
                name_string.to_string()
            };
            let dest = dest_dir.join(dest_name);
            if dest.exists() {
                fs::remove_file(&dest)?;
            }
            fs::copy(entry.path(), &dest)
                .with_context(|| format!("Failed to install {}", dest.display()))?;
            if dest
                .file_name()
                .is_some_and(|name| name == build::binary_name())
                || dest.extension().is_some_and(|ext| ext == "bin")
            {
                crate::platform::set_permissions_executable(&dest)?;
            }
            installed_files.push(dest);
        }
        let install_stamp = SystemTime::now();
        for path in &installed_files {
            if let Ok(file) = fs::File::options().write(true).open(path) {
                let _ = file.set_modified(install_stamp);
            }
        }
        let _ = fs::remove_dir_all(&extract_dir);
        installed_version_dir = Some(dest_dir.join(build::binary_name()));
    } else {
        fs::write(&temp_path, &bytes).context("Failed to write temp file")?;
    }

    let version_owned = build::safe_version_component(&release.tag_name)?;
    let version = version_owned.as_str();
    let mut metadata = UpdateMetadata::load().unwrap_or_default();

    let versioned_path = if let Some(versioned_path) = installed_version_dir {
        versioned_path
    } else {
        crate::platform::set_permissions_executable(&temp_path)?;
        let versioned_path = build::install_binary_at_version(&temp_path, version)?;
        let _ = fs::remove_file(&temp_path);
        versioned_path
    };
    if let Err(error) = build::advance_shared_server_if_tracking_stable(version) {
        crate::logging::warn(&format!(
            "update: failed to advance shared-server channel to {}: {}",
            version, error
        ));
    }
    build::update_stable_symlink(version)?;
    build::update_current_symlink(version)?;
    build::update_launcher_symlink_to_current()?;

    metadata.installed_version = Some(release.tag_name.clone());
    metadata.installed_from = Some(asset.browser_download_url.clone());
    metadata.last_check = SystemTime::now();
    metadata.save()?;
    record_release_update_duration(started.elapsed());

    Ok(versioned_path)
}

pub fn check_and_maybe_update(auto_install: bool) -> UpdateCheckResult {
    use crate::alphacode_app_core::bus::{Bus, BusEvent, UpdateStatus};

    if !should_auto_update() {
        return UpdateCheckResult::NoUpdate;
    }

    let metadata = UpdateMetadata::load().unwrap_or_default();
    if !metadata.should_check() {
        return UpdateCheckResult::NoUpdate;
    }

    Bus::global().publish(BusEvent::UpdateStatus(UpdateStatus::Checking));

    match check_for_update_blocking() {
        Ok(Some(release)) => {
            let current = crate::alphacode_build_meta::version().to_string();
            let latest = release.tag_name.clone();

            Bus::global().publish(BusEvent::UpdateStatus(UpdateStatus::Available {
                current: current.clone(),
                latest: latest.clone(),
            }));

            if auto_install {
                let progress_version = latest.clone();
                match download_and_install_blocking_with_progress(&release, |progress| {
                    Bus::global().publish(BusEvent::UpdateStatus(UpdateStatus::Downloading {
                        version: progress_version.clone(),
                        downloaded: progress.downloaded,
                        total: progress.total,
                    }));
                }) {
                    Ok(path) => {
                        Bus::global().publish(BusEvent::UpdateStatus(UpdateStatus::Installed {
                            version: latest.clone(),
                        }));
                        UpdateCheckResult::UpdateInstalled {
                            version: latest,
                            path,
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to install: {}", e);
                        Bus::global()
                            .publish(BusEvent::UpdateStatus(UpdateStatus::Error(msg.clone())));
                        UpdateCheckResult::Error(msg)
                    }
                }
            } else {
                let mut metadata = UpdateMetadata::load().unwrap_or_default();
                metadata.last_check = SystemTime::now();
                let _ = metadata.save();
                UpdateCheckResult::UpdateAvailable {
                    current,
                    latest,
                    _release: release,
                }
            }
        }
        Ok(None) => {
            repair_stale_shared_server_after_no_update();
            Bus::global().publish(BusEvent::UpdateStatus(UpdateStatus::UpToDate));
            let mut metadata = UpdateMetadata::load().unwrap_or_default();
            metadata.last_check = SystemTime::now();
            let _ = metadata.save();
            UpdateCheckResult::NoUpdate
        }
        Err(e) => {
            let msg = short_update_error("update check failed", &e);
            Bus::global().publish(BusEvent::UpdateStatus(UpdateStatus::Error(msg.clone())));
            UpdateCheckResult::Error(msg)
        }
    }
}

fn repair_stale_shared_server_after_no_update() {
    match build::repair_stale_shared_server_channel() {
        Ok(build::SharedServerRepair::Repaired {
            previous,
            repaired_to,
        }) => {
            crate::logging::info(&format!(
                "update: repaired stale shared-server channel {:?} -> {} after no-op update check",
                previous, repaired_to
            ));
        }
        Ok(build::SharedServerRepair::AlreadyCurrent) => {}
        Err(error) => {
            crate::logging::warn(&format!(
                "update: failed to repair stale shared-server channel after no-op update check: {}",
                error
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_update_core::parse_sha256sums;
    use sha2::{Digest, Sha256};

    /// Build an in-memory zip from `(name, contents)` pairs. `zip` only has a
    /// writer behind the `deflate` feature's `ZipWriter`, which this crate
    /// enables, so writing real archives in tests is possible.
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, contents) in entries {
                writer.start_file(*name, options).expect("start_file");
                std::io::Write::write_all(&mut writer, contents).expect("write entry");
            }
            writer.finish().expect("finish zip");
        }
        buffer.into_inner()
    }

    /// Regression: a normal Windows release zip must extract successfully.
    ///
    /// The containment guard used to compare `extract_dir.canonicalize()`
    /// against `extract_dir.join(file_name).parent()`. On Windows
    /// `canonicalize` returns a verbatim `\\?\`-prefixed path while the other
    /// side was built from the non-canonical `extract_dir`, so the two never
    /// compared equal: the guard rejected *every* entry and the in-app updater
    /// failed with "zip entry \"alphacode.exe\" resolved outside the extraction
    /// directory" for all Windows users. This test fails against that code on
    /// Windows and passes with the canonical-root join.
    #[test]
    fn test_extract_zip_asset_extracts_top_level_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = build_zip(&[("alphacode.exe", b"MZ fake binary")]);

        extract_zip_asset_into(&bytes, dir.path())
            .expect("a well-formed Windows release zip must extract");

        let extracted = dir.path().join("alphacode.exe");
        assert!(extracted.is_file(), "alphacode.exe was not written");
        assert_eq!(fs::read(&extracted).expect("read"), b"MZ fake binary");
    }

    /// Multiple top-level files must all land in the extraction root.
    #[test]
    fn test_extract_zip_asset_extracts_every_top_level_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = build_zip(&[
            ("alphacode.exe", b"exe"),
            ("README.md", b"docs"),
            ("LICENSE", b"licence"),
        ]);

        extract_zip_asset_into(&bytes, dir.path()).expect("extract");

        assert_eq!(
            fs::read(dir.path().join("alphacode.exe")).expect("a"),
            b"exe"
        );
        assert_eq!(fs::read(dir.path().join("README.md")).expect("b"), b"docs");
        assert_eq!(fs::read(dir.path().join("LICENSE")).expect("c"), b"licence");
    }

    /// The zip-slip protections must survive the fix: traversal, absolute, and
    /// nested entries are all skipped, and nothing lands outside the root.
    ///
    /// The load-bearing assertion is the *escape* one. Note what this
    /// deliberately does NOT assert: that a top-level entry refuses to
    /// overwrite an existing file in the extraction root. That is not a
    /// property the extractor has or needs. The root is a private temp
    /// directory the updater creates and owns for the duration of one install
    /// (removed and recreated just before extraction), so writing
    /// `alphacode.exe` into it is the intended behaviour, not a hazard.
    /// Conflating "must not escape" with "must not overwrite" made an earlier
    /// version of this test fail against correct code.
    #[test]
    fn test_extract_zip_asset_skips_escaping_and_nested_entries() {
        let outer = tempfile::tempdir().expect("tempdir");
        let extract_dir = outer.path().join("extract");
        fs::create_dir_all(&extract_dir).expect("create extract dir");

        // One level above the root, which is exactly where a successful `..`
        // traversal would land.
        let outside = outer.path().join("outside.txt");
        fs::write(&outside, b"untouched").expect("write sentinel");

        let bytes = build_zip(&[
            ("../outside.txt", b"escaped"),
            ("nested/inner.txt", b"nested"),
            ("/absolute.txt", b"absolute"),
        ]);

        // Hostile entries are skipped, not fatal.
        extract_zip_asset_into(&bytes, &extract_dir).expect("hostile entries are skipped");

        assert_eq!(
            fs::read(&outside).expect("sentinel"),
            b"untouched",
            "a `..` entry escaped the extraction root and clobbered a file outside it"
        );
        assert!(
            !extract_dir.join("nested").exists(),
            "a nested entry created a subdirectory instead of being skipped"
        );
        assert!(
            !extract_dir.join("absolute.txt").exists(),
            "an absolute entry was written instead of being skipped"
        );
        assert!(
            !extract_dir.join("outside.txt").exists(),
            "a `..` entry was flattened into the root instead of being skipped"
        );
    }

    /// Regression: a transient 404 on `SHA256SUMS` must be retried, not fatal.
    ///
    /// The release archives are uploaded by the per-platform `build` jobs while
    /// `SHA256SUMS` is merged and uploaded afterwards by the `release` job, so
    /// there is a window where the release lists `SHA256SUMS` but the CDN has
    /// not propagated it. The fetch previously had no retry at all, so that
    /// window aborted the whole update with
    /// `SHA256SUMS download failed: 404 Not Found` — which is exactly what a
    /// 1.0.65 client hit auto-updating to 1.0.67.
    #[test]
    fn test_checksum_404_is_retryable() {
        assert!(is_retryable_checksum_status(reqwest::StatusCode::NOT_FOUND));
        assert!(is_retryable_checksum_status(reqwest::StatusCode::FORBIDDEN));
        assert!(is_retryable_checksum_status(
            reqwest::StatusCode::BAD_GATEWAY
        ));
        assert!(is_retryable_checksum_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
    }

    /// The retry must not swallow genuine, permanent failures — otherwise a
    /// real problem turns into a 60-second stall before reporting itself.
    #[test]
    fn test_checksum_permanent_status_is_not_retryable() {
        for status in [
            reqwest::StatusCode::BAD_REQUEST,
            reqwest::StatusCode::UNAUTHORIZED,
            reqwest::StatusCode::GONE,
            reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        ] {
            assert!(
                !is_retryable_checksum_status(status),
                "{status} must not be retried"
            );
        }
    }

    #[test]
    fn test_version_is_newer() {
        assert!(version_is_newer("0.1.3", "0.1.2"));
        assert!(version_is_newer("0.2.0", "0.1.9"));
        assert!(version_is_newer("1.0.0", "0.9.9"));
        assert!(!version_is_newer("0.1.2", "0.1.2"));
        assert!(!version_is_newer("0.1.1", "0.1.2"));
        assert!(!version_is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn test_asset_name() {
        let name = get_asset_name();
        assert!(name.starts_with("alphacode-"));
    }

    #[test]
    fn test_is_archive_name() {
        assert!(is_archive_name("alphacode-linux-x86_64.tar.gz"));
        assert!(is_archive_name("alphacode-windows-x86_64.zip"));
        assert!(!is_archive_name("alphacode-windows-x86_64.sha256"));
        assert!(!is_archive_name("alphacode-linux-x86_64.sha256"));
        assert!(!is_archive_name("alphacode-windows-x86_64"));
        assert!(!is_archive_name("SHA256SUMS"));
    }

    /// Regression: `platform_asset` must never match a `.sha256` checksum
    /// sidecar when a real archive exists with the same stem.  Previously
    /// `starts_with(stem)` matched `.sha256` first (alphabetical), causing
    /// `/update` to fail with "SHA256SUMS does not list …" on every
    /// platform.
    #[test]
    fn test_platform_asset_skips_sha256_sidecars() {
        let release = GitHubRelease {
            tag_name: "v1.0.99".into(),
            _name: None,
            _html_url: String::new(),
            _published_at: None,
            assets: vec![
                GitHubAsset {
                    name: format!("{}.sha256", get_asset_name()),
                    browser_download_url: String::new(),
                    _size: 100,
                },
                GitHubAsset {
                    name: format!("{}.zip", get_asset_name()),
                    browser_download_url: String::new(),
                    _size: 1024,
                },
            ],
            _target_commitish: String::new(),
        };
        let asset = platform_asset(&release).expect("should find archive, not .sha256");
        assert!(
            asset.name.ends_with(".zip") || asset.name.ends_with(".tar.gz"),
            "platform_asset must return the archive, got: {}",
            asset.name
        );
    }

    #[test]
    fn test_format_download_progress_bar_known_total() {
        let rendered = format_download_progress_bar(DownloadProgress {
            downloaded: 512,
            total: Some(1024),
        });
        assert!(rendered.contains("50%"));
        assert!(rendered.contains("512 B/1.0 KiB"));
        assert!(rendered.contains('█'));
        assert!(rendered.contains('░'));
    }

    #[test]
    fn test_format_download_progress_bar_unknown_total() {
        let rendered = format_download_progress_bar(DownloadProgress {
            downloaded: 2 * 1024 * 1024,
            total: None,
        });
        assert_eq!(rendered, "Downloading update... 2.0 MiB downloaded");
    }

    #[test]
    fn test_parse_sha256sums_accepts_standard_and_binary_lines() {
        let digest_a = "a".repeat(64);
        let digest_b = "B".repeat(64);
        let digest_b_lower = "b".repeat(64);
        let contents = format!(
            "# generated by release workflow\n{}  alphacode-linux-x86_64.tar.gz\r\n{} *alphacode-windows-x86_64.exe\n",
            digest_a, digest_b
        );
        let parsed = parse_sha256sums(&contents).unwrap();
        assert_eq!(
            parsed
                .get("alphacode-linux-x86_64.tar.gz")
                .map(String::as_str),
            Some(digest_a.as_str())
        );
        assert_eq!(
            parsed
                .get("alphacode-windows-x86_64.exe")
                .map(String::as_str),
            Some(digest_b_lower.as_str())
        );
    }

    #[test]
    fn test_verify_asset_checksum_text_accepts_matching_digest() {
        let bytes = b"hello update";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let contents = format!("{}  alphacode-linux-x86_64.tar.gz\n", digest);
        verify_asset_checksum_text(&contents, "alphacode-linux-x86_64.tar.gz", bytes).unwrap();
    }

    #[test]
    fn test_verify_asset_checksum_text_rejects_mismatch() {
        let wrong = "0".repeat(64);
        let contents = format!("{}  alphacode-linux-x86_64.tar.gz\n", wrong);
        let err = verify_asset_checksum_text(&contents, "alphacode-linux-x86_64.tar.gz", b"actual")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Checksum mismatch"));
    }

    #[test]
    fn test_verify_asset_checksum_text_requires_asset_entry() {
        let digest = "1".repeat(64);
        let contents = format!("{}  other-asset.tar.gz\n", digest);
        let err = verify_asset_checksum_text(&contents, "alphacode-linux-x86_64.tar.gz", b"actual")
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not list"));
    }

    #[test]
    fn test_parse_sha256sums_rejects_invalid_digest() {
        let err = parse_sha256sums("not-a-sha  alphacode-linux-x86_64.tar.gz\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid SHA256 digest"));
    }

    #[test]
    fn test_is_release_build() {
        assert!(!is_release_build());
    }

    #[test]
    fn test_should_auto_update_dev_build() {
        assert!(!should_auto_update());
    }

    #[test]
    fn test_summarize_git_pull_failure_diverged() {
        let stderr = b"hint: You have divergent branches and need to specify how to reconcile them.\nfatal: Need to specify how to reconcile divergent branches.\n";
        assert_eq!(
            summarize_git_pull_failure(stderr),
            crate::alphacode_update_core::GIT_PULL_DIVERGED_SUMMARY
        );
        assert!(crate::alphacode_update_core::summary_is_divergence(
            &summarize_git_pull_failure(stderr)
        ));
    }

    #[test]
    fn test_summarize_git_pull_failure_no_tracking_branch() {
        let stderr = b"There is no tracking information for the current branch.\n";
        assert_eq!(
            summarize_git_pull_failure(stderr),
            "git pull failed: current branch has no upstream tracking branch"
        );
    }

    #[test]
    fn test_summarize_git_pull_failure_uses_first_non_hint_line() {
        let stderr = b"hint: test hint\nfatal: repository not found\n";
        assert_eq!(
            summarize_git_pull_failure(stderr),
            "git pull failed: repository not found"
        );
    }

    #[test]
    fn test_estimate_release_update_duration_uses_size_buckets() {
        assert_eq!(
            estimate_release_update_duration(10 * 1024 * 1024, None),
            Duration::from_secs(10)
        );
        assert_eq!(
            estimate_release_update_duration(40 * 1024 * 1024, None),
            Duration::from_secs(35)
        );
    }

    #[test]
    fn test_estimate_source_update_duration_prefers_history() {
        assert_eq!(
            estimate_source_update_duration(true, true, Some(123.4)),
            Duration::from_secs(123)
        );
    }

    #[test]
    fn test_content_range_total_parses_total() {
        use reqwest::header::{HeaderMap, HeaderValue};
        // Build a Response is awkward; test the parser via a header map directly.
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_RANGE,
            HeaderValue::from_static("bytes 200-1023/1024"),
        );
        let parsed = headers
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit('/').next())
            .and_then(|total| total.trim().parse::<u64>().ok());
        assert_eq!(parsed, Some(1024));
    }

    #[test]
    fn test_content_range_total_unknown_size_is_none() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_RANGE,
            HeaderValue::from_static("bytes 200-1023/*"),
        );
        let parsed = headers
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit('/').next())
            .and_then(|total| total.trim().parse::<u64>().ok());
        assert_eq!(parsed, None);
    }

    /// End-to-end resume test: a tiny HTTP server serves the first half of the
    /// body then drops the connection, and on the resumed Range request serves
    /// the rest. The download must recover and return the full payload.
    #[test]
    fn test_download_asset_with_resume_recovers_from_dropped_connection() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let payload: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let total = payload.len();
        let split = total / 2;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let payload_for_server = payload.clone();
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_server = Arc::clone(&request_count);

        let handle = std::thread::spawn(move || {
            // Serve exactly two connections: first truncated, second resumed.
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let n = request_count_server.fetch_add(1, Ordering::SeqCst);

                // Parse request headers; look for a Range header.
                let mut range_start = 0usize;
                {
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            break;
                        }
                        let trimmed = line.trim_end();
                        if let Some(rest) =
                            trimmed.to_ascii_lowercase().strip_prefix("range: bytes=")
                            && let Some(start) = rest.split('-').next()
                        {
                            range_start = start.trim().parse().unwrap_or(0);
                        }
                        if trimmed.is_empty() {
                            break;
                        }
                    }
                }

                if n == 0 {
                    // First attempt: 200 OK, but only send the first half, then
                    // close mid-stream to simulate a stalled/dropped connection.
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                        total
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&payload_for_server[..split]);
                    let _ = stream.flush();
                    // Drop connection without finishing the body.
                } else {
                    // Resumed attempt: serve 206 with the remaining bytes.
                    let remaining = &payload_for_server[range_start..];
                    let header = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                        remaining.len(),
                        range_start,
                        total - 1,
                        total
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(remaining);
                    let _ = stream.flush();
                }
            }
        });

        let client = reqwest::blocking::Client::builder()
            .build()
            .expect("client");
        let url = format!("http://{}/asset", addr);
        let (bytes, parsed_total) =
            download_asset_with_resume(&client, &url, Some(total as u64), &mut |_| {})
                .expect("download should recover");

        handle.join().ok();

        assert_eq!(bytes, payload, "resumed download must reconstruct payload");
        assert_eq!(parsed_total, Some(total as u64));
        assert_eq!(
            request_count.load(Ordering::SeqCst),
            2,
            "should have made an initial + one resume request"
        );
    }

    /// A connection that keeps dropping but always advances a little must still
    /// complete: forward progress resets the consecutive-stall budget, so the
    /// number of resumes can exceed DOWNLOAD_MAX_ATTEMPTS as long as each one
    /// delivers new bytes.
    #[test]
    fn test_download_asset_with_resume_tolerates_many_progressing_drops() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let payload: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let total = payload.len();
        // Each attempt delivers only this many bytes, then drops, so it takes
        // many more than DOWNLOAD_MAX_ATTEMPTS attempts to finish.
        let chunk = 100usize;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let payload_for_server = payload.clone();
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_server = Arc::clone(&request_count);

        let handle = std::thread::spawn(move || {
            loop {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let n = request_count_server.fetch_add(1, Ordering::SeqCst);

                let mut range_start = 0usize;
                {
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            break;
                        }
                        let trimmed = line.trim_end();
                        if let Some(rest) =
                            trimmed.to_ascii_lowercase().strip_prefix("range: bytes=")
                            && let Some(start) = rest.split('-').next()
                        {
                            range_start = start.trim().parse().unwrap_or(0);
                        }
                        if trimmed.is_empty() {
                            break;
                        }
                    }
                }

                let end = (range_start + chunk).min(total);
                let body = &payload_for_server[range_start..end];
                let header = if n == 0 {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                        total
                    )
                } else {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                        total - range_start,
                        range_start,
                        total - 1,
                        total
                    )
                };
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
                // Drop after a partial chunk unless we've reached the end.
                if end >= total {
                    break;
                }
            }
        });

        let client = reqwest::blocking::Client::builder()
            .build()
            .expect("client");
        let url = format!("http://{}/asset", addr);
        let (bytes, _total) =
            download_asset_with_resume(&client, &url, Some(total as u64), &mut |_| {})
                .expect("progressing download should complete");

        handle.join().ok();

        assert_eq!(bytes, payload);
        assert!(
            request_count.load(Ordering::SeqCst) > DOWNLOAD_MAX_ATTEMPTS,
            "test should require more than the stall budget of resumes"
        );
    }

    /// Mirrors the real #293 shape closely: the caller has *no* size hint
    /// (None), a slow connection drops repeatedly, and the size is learned from
    /// the server (Content-Length on the initial 200, Content-Range on the 206
    /// resumes, exactly like a GitHub release asset). The download must still
    /// complete, learn the correct total, and report progress that never moves
    /// backwards across reconnects (so the UI bar can't appear to regress).
    #[test]
    fn test_download_asset_with_resume_unknown_total_and_monotonic_progress() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let total = payload.len();
        let chunk = 400usize;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let payload_for_server = payload.clone();
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_server = Arc::clone(&request_count);

        let handle = std::thread::spawn(move || {
            loop {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let n = request_count_server.fetch_add(1, Ordering::SeqCst);

                let mut range_start = 0usize;
                {
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            break;
                        }
                        let trimmed = line.trim_end();
                        if let Some(rest) =
                            trimmed.to_ascii_lowercase().strip_prefix("range: bytes=")
                            && let Some(start) = rest.split('-').next()
                        {
                            range_start = start.trim().parse().unwrap_or(0);
                        }
                        if trimmed.is_empty() {
                            break;
                        }
                    }
                }

                let end = (range_start + chunk).min(total);
                let body = &payload_for_server[range_start..end];
                // Like a real GitHub asset: the initial 200 carries the full
                // Content-Length (so a mid-stream drop is detectable), and the
                // 206 resumes carry Content-Range. The *caller* still gets no
                // size hint, so the total must be learned from these headers.
                let header = if n == 0 {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                        total
                    )
                } else {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                        total - range_start,
                        range_start,
                        total - 1,
                        total
                    )
                };
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
                // Drop after a partial chunk unless we've reached the end.
                if end >= total {
                    break;
                }
            }
        });

        let client = reqwest::blocking::Client::builder()
            .build()
            .expect("client");
        let url = format!("http://{}/asset", addr);

        let mut progress_points: Vec<u64> = Vec::new();
        let mut seen_total: Option<u64> = None;
        let (bytes, parsed_total) = download_asset_with_resume(
            &client,
            &url,
            None, // no size hint, like a fresh download with no metadata
            &mut |p| {
                progress_points.push(p.downloaded);
                if p.total.is_some() {
                    seen_total = p.total;
                }
            },
        )
        .expect("download with unknown caller hint should complete");

        handle.join().ok();

        assert_eq!(bytes, payload, "payload must be fully reconstructed");
        assert_eq!(
            parsed_total,
            Some(total as u64),
            "total must be learned from the server headers"
        );
        assert_eq!(seen_total, Some(total as u64));
        assert!(
            progress_points.windows(2).all(|w| w[1] >= w[0]),
            "progress must never go backwards across reconnects: {:?}",
            progress_points
        );
        assert_eq!(
            *progress_points.last().expect("at least one progress point"),
            total as u64,
            "final progress must reach the full size"
        );
    }
}

#[cfg(test)]
mod github_auth_tests {
    use super::*;

    #[test]
    #[ignore = "live network test"]
    fn live_fetch_latest_release_uses_auth() {
        let release = fetch_latest_release_blocking().expect("release fetch should succeed");
        assert!(!release.tag_name.is_empty());
    }
}
