use anyhow::Result;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

pub const BACKGROUND_UPDATE_THRESHOLD: Duration = Duration::from_secs(15);
const DOWNLOAD_PROGRESS_BAR_WIDTH: usize = 24;

/// Summary emitted when `git pull` cannot reconcile the local and upstream
/// histories on its own (diverged branches, non-fast-forward, unrelated
/// histories). Callers use this to recognize a divergence and offer a merge
/// affordance instead of a generic failure.
pub const GIT_PULL_DIVERGED_SUMMARY: &str =
    "Local and upstream have diverged, so the update could not fast-forward.";

#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct UpdateEstimate {
    pub duration: Duration,
    pub summary: String,
    pub should_background: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(rename = "name")]
    pub _name: Option<String>,
    #[serde(rename = "html_url")]
    pub _html_url: String,
    #[serde(rename = "published_at")]
    pub _published_at: Option<String>,
    pub assets: Vec<GitHubAsset>,
    #[serde(default)]
    #[serde(rename = "target_commitish")]
    pub _target_commitish: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(rename = "size")]
    pub _size: u64,
}

pub enum PreparedUpdate {
    None {
        current: String,
    },
    Stable {
        release: GitHubRelease,
        estimate: UpdateEstimate,
    },
    MainSource {
        latest_sha: String,
        estimate: UpdateEstimate,
    },
}

pub enum UpdateCheckResult {
    NoUpdate,
    UpdateAvailable {
        current: String,
        latest: String,
        _release: GitHubRelease,
    },
    UpdateInstalled {
        version: String,
        path: PathBuf,
    },
    Error(String),
}

pub fn format_duration_estimate(duration: Duration) -> String {
    match duration.as_secs() {
        0..=15 => "under 15s".to_string(),
        16..=45 => "~30s".to_string(),
        46..=90 => "~1 min".to_string(),
        91..=180 => "~2-3 min".to_string(),
        181..=360 => "~3-6 min".to_string(),
        _ => "5+ min".to_string(),
    }
}

pub fn estimate_release_update_duration(
    asset_size_bytes: u64,
    historical_secs: Option<f64>,
) -> Duration {
    if let Some(previous) = historical_secs {
        return Duration::from_secs(previous.max(5.0).round() as u64);
    }

    let size_mb = asset_size_bytes as f64 / (1024.0 * 1024.0);
    let secs = if size_mb <= 15.0 {
        10
    } else if size_mb <= 35.0 {
        20
    } else if size_mb <= 60.0 {
        35
    } else {
        50
    };
    Duration::from_secs(secs)
}

pub fn estimate_source_update_duration(
    repo_exists: bool,
    has_previous_build: bool,
    historical_secs: Option<f64>,
) -> Duration {
    if let Some(previous) = historical_secs {
        return Duration::from_secs(previous.max(20.0).round() as u64);
    }

    let secs = if !repo_exists {
        420
    } else if has_previous_build {
        90
    } else {
        180
    };
    Duration::from_secs(secs)
}

pub fn update_estimate(summary: String, duration: Duration) -> UpdateEstimate {
    UpdateEstimate {
        duration,
        summary,
        should_background: duration >= BACKGROUND_UPDATE_THRESHOLD,
    }
}

/// Stable stem used for release assets, e.g. `alphacode-linux-x86_64`.
///
/// Mirrors the `archive:` field in `.github/workflows/release.yml` and the
/// `ASSET=` lines in `scripts/install.sh` / `scripts/install.ps1`. The release
/// pipeline ships one `.tar.gz` (Linux/macOS) or one `.zip` (Windows) per
/// stem plus a matching `.sha256`. The binary itself (the launcher on Windows)
/// may or may not have a `.exe` suffix inside the archive, so this stem is
/// intentionally extension-free: it is what we use to *find* the asset on
/// GitHub and to recognise the launcher inside the archive.
pub fn get_asset_stem() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "alphacode-linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "alphacode-linux-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "alphacode-macos-x86_64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "alphacode-macos-arm64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "alphacode-windows-x86_64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "alphacode-windows-arm64"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
    )))]
    {
        "alphacode-unknown"
    }
}

/// Archive extension for the current target. Linux/macOS ship `.tar.gz`;
/// Windows ships `.zip`. This is the suffix of the *downloaded archive*,
/// not of the binary inside it.
pub fn get_asset_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "zip"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "tar.gz"
    }
}

/// Full archive filename, e.g. `alphacode-linux-x86_64.tar.gz` or
/// `alphacode-windows-x86_64.zip`. Built from [`get_asset_stem`] +
/// [`get_asset_extension`] so the two cannot drift.
pub fn get_asset_filename() -> String {
    format!("{}.{}", get_asset_stem(), get_asset_extension())
}

/// Backwards-compatible alias for [`get_asset_stem`].
///
/// Older call sites passed this value to `starts_with`/`==`; they were
/// assuming the stem-without-extension form, which is what this function
/// now returns. The original name is preserved so existing imports and
/// tests keep working.
pub fn get_asset_name() -> &'static str {
    get_asset_stem()
}

/// Returns `true` when `name` looks like a release archive (`.tar.gz` or
/// `.zip`) rather than a `.sha256` checksum sidecar.  Used by
/// `platform_asset` and download callers so they never accidentally
/// match a checksum file when looking for the real archive.
pub fn is_archive_name(name: &str) -> bool {
    name.ends_with(".tar.gz") || name.ends_with(".zip")
}

pub fn summarize_git_pull_failure(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let text = stderr.trim();
    if text.is_empty() {
        return "git pull failed".to_string();
    }

    if git_pull_failure_is_divergence(text) {
        return GIT_PULL_DIVERGED_SUMMARY.to_string();
    }

    if text.contains("There is no tracking information for the current branch") {
        return "git pull failed: current branch has no upstream tracking branch".to_string();
    }

    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("hint:"))
        .unwrap_or("git pull failed");
    let line = line.strip_prefix("fatal: ").unwrap_or(line);
    if line.eq_ignore_ascii_case("git pull failed") {
        "git pull failed".to_string()
    } else {
        format!("git pull failed: {}", line)
    }
}

/// Whether `git pull` stderr indicates the local and upstream branches have
/// diverged (and therefore need a manual merge/rebase, not a fast-forward).
pub fn git_pull_failure_is_divergence(stderr: &str) -> bool {
    stderr.contains("Need to specify how to reconcile divergent branches")
        || stderr.contains("Not possible to fast-forward")
        || stderr.contains("refusing to merge unrelated histories")
        || stderr.contains("have diverged")
}

/// Whether a `summarize_git_pull_failure` summary describes a divergence.
pub fn summary_is_divergence(summary: &str) -> bool {
    summary == GIT_PULL_DIVERGED_SUMMARY
}

/// Longest single-line update summary we hand to the UI.
const UPDATE_ERROR_SUMMARY_MAX_CHARS: usize = 72;

/// Condense any update-path error into a single short line fit for a status
/// notice or a one-line card.
///
/// Update errors reach the UI from many layers (reqwest, git, cargo, tar,
/// checksum verification), so raw text is often multi-line, prefixed with
/// redundant "Update failed:" wrappers, and long enough to wrap several times.
/// Users only need to know what went wrong in a few words; the full text stays
/// in the log.
pub fn summarize_update_error(error: &str) -> String {
    let text = error.trim();
    // Divergence is matched verbatim by callers that offer a merge affordance,
    // so never rewrite it.
    if summary_is_divergence(text) || git_pull_failure_is_divergence(text) {
        return GIT_PULL_DIVERGED_SUMMARY.to_string();
    }

    // Strip the wrapper prefixes callers stack on top of each other.
    let mut stripped = text;
    while let Some(next) = ["Update failed:", "Update check failed:", "Check failed:"]
        .iter()
        .find_map(|prefix| stripped.strip_prefix(prefix))
        .map(str::trim_start)
    {
        stripped = next;
    }

    let first_line = stripped
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("unknown error");
    if summary_is_divergence(first_line) {
        return GIT_PULL_DIVERGED_SUMMARY.to_string();
    }
    if let Some(known) = known_update_error_summary(first_line) {
        return known.to_string();
    }

    // Keep one clause: drop any trailing context sentence and punctuation.
    let clause = first_line
        .split_once(". ")
        .map(|(head, _)| head)
        .unwrap_or(first_line)
        .trim_end_matches(['.', ':'])
        .trim();
    let clause = if clause.is_empty() {
        first_line
    } else {
        clause
    };

    if clause.chars().count() <= UPDATE_ERROR_SUMMARY_MAX_CHARS {
        return clause.to_string();
    }
    let truncated: String = clause
        .chars()
        .take(UPDATE_ERROR_SUMMARY_MAX_CHARS - 1)
        .collect();
    format!("{}…", truncated.trim_end())
}

/// Map noisy transport/tooling failures onto short human phrases.
fn known_update_error_summary(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);

    if has("dns") || has("failed to lookup address") || has("name or service not known") {
        return Some("no network connection");
    }
    if has("timed out") || has("timeout") {
        return Some("GitHub timed out");
    }
    if has("connection refused")
        || has("connection reset")
        || has("network is unreachable")
        || has("tcp connect error")
    {
        return Some("could not reach GitHub");
    }
    if has("certificate") || has("tls ") || has("ssl") {
        return Some("TLS error reaching GitHub");
    }
    if has("checksum") {
        return Some("download failed checksum verification");
    }
    if has("permission denied") || has("read-only file system") || has("os error 13") {
        return Some("no permission to install the update");
    }
    if has("no space left") {
        return Some("not enough disk space");
    }
    if has("cargo build failed") {
        return Some("cargo build failed");
    }
    if has("no asset found for platform") {
        return Some("no release build for this platform");
    }
    if has("no releases found") {
        return Some("no releases published yet");
    }
    None
}

pub fn parse_sha256sums(contents: &str) -> Result<HashMap<String, String>> {
    let mut checksums = HashMap::new();
    for (line_idx, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split_whitespace();
        let Some(checksum) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            anyhow::bail!("Invalid SHA256SUMS line {}: missing filename", line_idx + 1);
        };
        if parts.next().is_some() {
            anyhow::bail!(
                "Invalid SHA256SUMS line {}: expected '<sha256>  <filename>'",
                line_idx + 1
            );
        }
        if checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!(
                "Invalid SHA256SUMS line {}: invalid SHA256 digest",
                line_idx + 1
            );
        }

        let name = name.trim_start_matches('*').to_string();
        let previous = checksums.insert(name.clone(), checksum.to_ascii_lowercase());
        if previous.is_some() {
            anyhow::bail!(
                "Invalid SHA256SUMS line {}: duplicate entry for {}",
                line_idx + 1,
                name
            );
        }
    }
    Ok(checksums)
}

pub fn verify_asset_checksum_text(contents: &str, asset_name: &str, bytes: &[u8]) -> Result<()> {
    let checksums = parse_sha256sums(contents)?;
    let actual = format!("{:x}", Sha256::digest(bytes));

    // Try exact match first.
    if let Some(expected) = checksums.get(asset_name) {
        if actual.eq_ignore_ascii_case(expected) {
            return Ok(());
        }
        anyhow::bail!(
            "Checksum mismatch for {}: expected {}, got {}",
            asset_name,
            expected,
            actual
        );
    }

    // Fuzzy match: the SHA256SUMS file may list the asset with a different
    // extension (e.g. .sh instead of .zip) or a slightly different name.
    // Try matching by stripping extensions and comparing the base name.
    let asset_base = asset_name
        .strip_suffix(".zip")
        .or_else(|| asset_name.strip_suffix(".tar.gz"))
        .or_else(|| asset_name.strip_suffix(".exe"))
        .or_else(|| asset_name.strip_suffix(".sha256"))
        .unwrap_or(asset_name);

    for (sum_name, expected) in &checksums {
        let sum_base = sum_name
            .strip_suffix(".zip")
            .or_else(|| sum_name.strip_suffix(".tar.gz"))
            .or_else(|| sum_name.strip_suffix(".exe"))
            .or_else(|| sum_name.strip_suffix(".sh"))
            .or_else(|| sum_name.strip_suffix(".sha256"))
            .unwrap_or(sum_name);

        if asset_base == sum_base && actual.eq_ignore_ascii_case(expected) {
            return Ok(());
        }
    }

    let listed: Vec<&str> = checksums.keys().map(|s| s.as_str()).collect();
    anyhow::bail!(
        "SHA256SUMS does not list {} — found entries: {} (checked {} entries)",
        asset_name,
        listed.join(", "),
        checksums.len()
    )
}

pub fn version_is_newer(release: &str, current: &str) -> bool {
    let parse = |v: &str| -> (u32, u32, u32) {
        let v = v.trim_start_matches('v');
        let parts: Vec<&str> = v.split('.').collect();
        let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };

    let r = parse(release);
    let c = parse(current);
    r > c
}

pub fn format_download_progress_bar(progress: DownloadProgress) -> String {
    let human_downloaded = format_bytes(progress.downloaded);
    let Some(total) = progress.total.filter(|total| *total > 0) else {
        return format!("Downloading update... {} downloaded", human_downloaded);
    };

    let ratio = (progress.downloaded as f64 / total as f64).clamp(0.0, 1.0);
    let filled = (ratio * DOWNLOAD_PROGRESS_BAR_WIDTH as f64).round() as usize;
    let filled = filled.min(DOWNLOAD_PROGRESS_BAR_WIDTH);
    let empty = DOWNLOAD_PROGRESS_BAR_WIDTH.saturating_sub(filled);
    let percent = (ratio * 100.0).round() as u64;
    format!(
        "Downloading update... [{}{}] {:>3}% ({}/{})",
        "█".repeat(filled),
        "░".repeat(empty),
        percent,
        human_downloaded,
        format_bytes(total)
    )
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.1} GiB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_works() {
        assert!(version_is_newer("v0.2.0", "0.1.9"));
        assert!(!version_is_newer("v0.1.0", "0.1.0"));
    }

    /// Every UI surface renders these on one line, so the summary must stay
    /// short and never contain a newline.
    #[test]
    fn summarize_update_error_is_always_one_short_line() {
        let inputs = [
            "Update failed: Update check failed: error sending request for url (https://api.github.com/repos/dragonked2/alphacode/releases/latest)\n\nCaused by:\n    dns error: failed to lookup address information",
            "cargo build failed: error[E0308]: mismatched types\n  --> src/lib.rs:1:1",
            "Checksum mismatch for alphacode-linux-x86_64.tar.gz: expected aaa, got bbb",
            "Failed to install /home/u/.alphacode/builds/versions/0.1.0/alphacode: Permission denied (os error 13)",
            "a very long single clause with no recognizable cause that just keeps going and going well past any sensible terminal width",
            "",
        ];
        for input in inputs {
            let summary = summarize_update_error(input);
            assert!(!summary.contains('\n'), "multi-line summary for {input:?}");
            assert!(!summary.is_empty(), "empty summary for {input:?}");
            assert!(
                summary.chars().count() <= UPDATE_ERROR_SUMMARY_MAX_CHARS,
                "summary too long ({}) for {input:?}: {summary}",
                summary.chars().count()
            );
        }
    }

    #[test]
    fn summarize_update_error_maps_known_causes() {
        assert_eq!(
            summarize_update_error("Update failed: dns error: failed to lookup address"),
            "no network connection"
        );
        assert_eq!(
            summarize_update_error("Check failed: operation timed out"),
            "GitHub timed out"
        );
        assert_eq!(
            summarize_update_error("Checksum mismatch for alphacode: expected a, got b"),
            "download failed checksum verification"
        );
        assert_eq!(
            summarize_update_error("No asset found for platform: alphacode-linux-x86_64"),
            "no release build for this platform"
        );
    }

    #[test]
    fn summarize_update_error_strips_stacked_wrappers() {
        assert_eq!(
            summarize_update_error("Update failed: Update check failed: something odd happened"),
            "something odd happened"
        );
    }

    /// The merge affordance matches the divergence summary verbatim, so
    /// summarizing must not rewrite it.
    #[test]
    fn summarize_update_error_preserves_divergence_summary() {
        assert_eq!(
            summarize_update_error(GIT_PULL_DIVERGED_SUMMARY),
            GIT_PULL_DIVERGED_SUMMARY
        );
        assert_eq!(
            summarize_update_error(&format!("Update failed: {GIT_PULL_DIVERGED_SUMMARY}")),
            GIT_PULL_DIVERGED_SUMMARY
        );
        assert!(summary_is_divergence(&summarize_update_error(
            "fatal: Need to specify how to reconcile divergent branches."
        )));
    }

    #[test]
    fn asset_name_is_supported() {
        assert_ne!(get_asset_name(), "alphacode-unknown");
        assert_ne!(get_asset_stem(), "alphacode-unknown");
    }

    /// The release workflow (`release.yml`) and the install scripts
    /// (`install.sh` / `install.ps1`) all share the same asset naming. If
    /// the Rust side drifts, `/update` silently reports "already up to date"
    /// on every platform it gets wrong. Lock the contract down here.
    #[test]
    fn asset_stem_matches_release_workflow_naming() {
        let stem = get_asset_stem();
        let ext = get_asset_extension();
        let filename = get_asset_filename();

        // Every supported target uses `arm64` (not `aarch64`) and never
        // bundles a `.exe` into the archive stem. The Windows install
        // workflow is .zip, everything else is .tar.gz.
        assert!(
            !stem.contains("aarch64"),
            "stem should use `arm64`, not `aarch64`: {stem}"
        );
        assert!(
            !stem.ends_with(".exe"),
            "stem must not carry a `.exe` suffix: {stem}"
        );
        assert_eq!(filename, format!("{stem}.{ext}"));

        // The actual filenames GitHub publishes for v1.0.9 must all be
        // reachable as `starts_with(stem)` — that is how
        // `platform_asset` recognises them. A mismatch here is the
        // exact reason `/update` ever reports "already up to date" while
        // GitHub has a newer release.
        let expected_assets: &[&str] = &[
            #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
            "alphacode-linux-x86_64.tar.gz",
            #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
            "alphacode-linux-arm64.tar.gz",
            #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
            "alphacode-macos-x86_64.tar.gz",
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            "alphacode-macos-arm64.tar.gz",
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            "alphacode-windows-x86_64.zip",
            #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
            "alphacode-windows-arm64.zip",
        ];
        assert!(
            !expected_assets.is_empty(),
            "no expected asset configured for target_os/target_arch; update the test"
        );
        for asset in expected_assets {
            assert!(
                asset.starts_with(stem),
                "asset {asset} should start with stem {stem}"
            );
        }
    }

    /// `get_asset_filename()` must end in the right archive extension for
    /// the current target. This is the only place the install path
    /// (`.tar.gz` extraction vs `.zip` extraction) makes its branching
    /// decision, so getting it wrong silently swallows every Windows
    /// update.
    #[cfg(target_os = "windows")]
    #[test]
    fn asset_filename_uses_zip_on_windows() {
        assert!(get_asset_filename().ends_with(".zip"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn asset_filename_uses_tar_gz_off_windows() {
        assert!(get_asset_filename().ends_with(".tar.gz"));
    }

    #[test]
    fn progress_bar_known_total() {
        let text = format_download_progress_bar(DownloadProgress {
            downloaded: 512,
            total: Some(1024),
        });
        assert!(text.contains("50%"));
        assert!(text.contains("512 B/1.0 KiB"));
    }

    #[test]
    fn progress_bar_unknown_total() {
        let text = format_download_progress_bar(DownloadProgress {
            downloaded: 2048,
            total: None,
        });
        assert_eq!(text, "Downloading update... 2.0 KiB downloaded");
    }

    #[test]
    fn sha256sums_accepts_standard_and_binary_lines() {
        let checksums = parse_sha256sums(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  alphacode-linux-x86_64\n\
             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *alphacode-macos-arm64\n",
        )
        .unwrap();
        assert_eq!(
            checksums.get("alphacode-linux-x86_64").map(String::as_str),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            checksums.get("alphacode-macos-arm64").map(String::as_str),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn checksum_verification_accepts_matching_digest() {
        let bytes = b"hello world";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let sums = format!("{}  alphacode-linux-x86_64\n", digest);
        verify_asset_checksum_text(&sums, "alphacode-linux-x86_64", bytes).unwrap();
    }

    #[test]
    fn checksum_verification_rejects_mismatch() {
        let sums = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  alphacode-linux-x86_64\n";
        let err = verify_asset_checksum_text(sums, "alphacode-linux-x86_64", b"hello")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Checksum mismatch"));
    }

    #[test]
    fn checksum_verification_requires_asset_entry() {
        let sums = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  other\n";
        let err = verify_asset_checksum_text(sums, "alphacode-linux-x86_64", b"hello")
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not list"));
    }

    #[test]
    fn sha256sums_rejects_invalid_digest() {
        let err = parse_sha256sums("not-a-digest  alphacode\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid SHA256 digest"));
    }

    #[test]
    fn git_pull_failure_summaries_are_stable() {
        assert_eq!(
            summarize_git_pull_failure(
                b"fatal: Need to specify how to reconcile divergent branches\n"
            ),
            GIT_PULL_DIVERGED_SUMMARY
        );
        assert!(summary_is_divergence(&summarize_git_pull_failure(
            b"fatal: Need to specify how to reconcile divergent branches\n"
        )));
        assert_eq!(
            summarize_git_pull_failure(b"hint: ignore me\nfatal: no upstream\n"),
            "git pull failed: no upstream"
        );
        assert!(!summary_is_divergence(&summarize_git_pull_failure(
            b"hint: ignore me\nfatal: no upstream\n"
        )));
    }

    #[test]
    fn update_duration_estimates_are_stable() {
        assert_eq!(
            estimate_release_update_duration(10 * 1024 * 1024, None),
            Duration::from_secs(10)
        );
        assert_eq!(
            estimate_source_update_duration(true, true, None),
            Duration::from_secs(90)
        );
    }
}
