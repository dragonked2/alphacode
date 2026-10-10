use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::{platform, storage};

const GITHUB_API_LATEST: &str =
    "https://api.github.com/repos/1jehuang/firefox-agent-bridge/releases/latest";

// Centralized browser-bridge identity for the Firefox Add-ons release.
// The published AlphaCode Browser Agent declares:
//   gecko.id = "alpha-agent@alpha-agent.local"
//   background.js NATIVE_HOSTS = ["alpha_agent", "firefox_agent_bridge"]
// Firefox enforces `allowed_extensions` before launching the native host, so
// the whitelist MUST contain the canonical ID or the bridge can never connect.
// Old IDs are kept for backward compatibility with existing installations.
pub const BROWSER_EXTENSION_ID: &str = "alpha-agent@alpha-agent.local";
pub const NATIVE_HOST_NAME_CANONICAL: &str = "alpha_agent";
/// Legacy host name kept for wire compat: the XPI falls back to it if
/// `alpha_agent` is not registered, and existing installs already reference it.
pub const NATIVE_HOST_NAME_LEGACY: &str = "firefox_agent_bridge";
const EXTENSION_ID_LISTED: &str = "browser-agent-bridge@alphacode.dev";
// Previous extension ID kept as fallback so existing installs keep working.
const EXTENSION_ID_LISTED_LEGACY: &str = "browser-agent-bridge@1jehuang.github.io";
const EXTENSION_ID_LOCAL: &str = "alphacode-browser-bridge@local";
// The ID published on Firefox Add-ons; it must be whitelisted for native
// messaging or Firefox refuses to launch the host.
const EXTENSION_ID_AMO: &str = BROWSER_EXTENSION_ID;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserStatus {
    pub backend: &'static str,
    pub browser: &'static str,
    pub setup_complete: bool,
    pub binary_installed: bool,
    pub responding: bool,
    pub compatible: bool,
    pub missing_actions: Vec<String>,
    pub ready: bool,
}

/// Every bridge wire action the tool can emit, paired with params chosen to
/// prove the action exists WITHOUT side effects: unknown ids/empty objects
/// make the bridge fail validation (proving the action is implemented)
/// instead of acting.
///
/// Deliberately skipped: `newSession`/`createTab` (opens a tab), `reload`/
/// `back`/`forward` (act on the active tab), and `screenshot` (heavy). Those
/// are long-standing core actions; probing them would disturb the user's
/// session.
const REQUIRED_BRIDGE_ACTION_PROBES: &[(&str, &str)] = &[
    (
        "evaluate",
        r#"{"script":"return await Promise.resolve(1)"}"#,
    ),
    ("listTabs", "{}"),
    ("getActiveTab", "{}"),
    ("setActiveTab", r#"{"tabId":-1}"#),
    ("closeTab", r#"{"tabId":-1}"#),
    ("listFrames", "{}"),
    ("navigate", "{}"),
    ("getContent", "{}"),
    ("getInteractables", "{}"),
    ("click", "{}"),
    ("hover", "{}"),
    ("type", "{}"),
    ("fillForm", "{}"),
    ("drag", "{}"),
    ("waitFor", r#"{"timeoutMs":100}"#),
    ("scroll", r#"{"position":"top"}"#),
    (
        "uploadFile",
        r#"{"selector":"input[type=file]","filePath":"/tmp/alphacode-browser-capability-probe"}"#,
    ),
    ("listCookies", r#"{"url":"https://example.invalid"}"#),
    ("setCookie", "{}"),
    ("removeCookie", "{}"),
];

fn alphacode_dir() -> PathBuf {
    storage::alphacode_dir().unwrap_or_else(|_| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".alphacode")
    })
}

fn browser_dir() -> PathBuf {
    alphacode_dir().join("browser")
}

pub fn browser_binary_path() -> PathBuf {
    let dir = browser_dir();
    #[cfg(windows)]
    {
        dir.join("browser.exe")
    }
    #[cfg(not(windows))]
    {
        dir.join("browser")
    }
}

fn host_binary_path() -> PathBuf {
    let dir = browser_dir();
    #[cfg(windows)]
    {
        dir.join("firefox-agent-bridge-host.exe")
    }
    #[cfg(not(windows))]
    {
        dir.join("firefox-agent-bridge-host")
    }
}

const FIREFOX_ADDON_PAGE_URL: &str =
    "https://addons.mozilla.org/en-US/firefox/addon/alphacode-browser-agent/";
static LAST_FIREFOX_ADDON_PAGE_OPEN: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn setup_marker_path() -> PathBuf {
    browser_dir().join(".setup-complete")
}

fn runtime_dir() -> PathBuf {
    storage::runtime_dir()
}

fn session_socket_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("browser-session-{}.sock", name))
}

fn session_pid_path(name: &str) -> PathBuf {
    runtime_dir().join(format!("browser-session-{}.pid", name))
}

fn is_session_alive(name: &str) -> bool {
    let pid_path = session_pid_path(name);
    if let Ok(pid_str) = std::fs::read_to_string(&pid_path)
        && let Ok(pid) = pid_str.trim().parse::<u32>()
        && platform::is_process_running(pid)
    {
        return session_socket_path(name).exists();
    }
    false
}

pub fn ensure_browser_session(session_id: &str) -> Option<String> {
    let session_name = sanitize_session_name(session_id);

    if is_session_alive(&session_name) {
        return Some(session_name);
    }

    let bin = browser_binary_path();
    if !bin.exists() {
        return None;
    }

    // Bind each agent session to a dedicated browser window when the installed
    // bridge supports it. Older bridge CLIs reject --bind-window, so probe the
    // command surface instead of paying for a known-failing process launch on
    // every browser action.
    if browser_supports_bind_window(&bin)
        && let Some(name) = spawn_browser_session(&bin, &session_name, true)
    {
        return Some(name);
    }
    spawn_browser_session(&bin, &session_name, false)
}

fn browser_supports_bind_window(bin: &std::path::Path) -> bool {
    std::process::Command::new(bin)
        .args(["session", "start", "--help"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains("--bind-window")
                || String::from_utf8_lossy(&output.stderr).contains("--bind-window")
        })
}

fn spawn_browser_session(
    bin: &std::path::Path,
    session_name: &str,
    bind_window: bool,
) -> Option<String> {
    let mut args = vec!["session", "start", session_name];
    if bind_window {
        args.push("--bind-window");
    }
    let result = std::process::Command::new(bin)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();

    match result {
        Ok(mut child) => {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                if session_socket_path(session_name).exists() && is_session_alive(session_name) {
                    let _ = child.stdout.take();
                    return Some(session_name.to_string());
                }
                if let Ok(Some(status)) = child.try_wait() {
                    eprintln!(
                        "[browser] session '{}' exited before startup with status {}{}",
                        session_name,
                        status,
                        if bind_window {
                            " (retrying without --bind-window)"
                        } else {
                            ""
                        }
                    );
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            eprintln!(
                "[browser] session '{}' did not start within 10s",
                session_name
            );
            let _ = child.kill();
            let _ = child.wait();
            None
        }
        Err(e) => {
            eprintln!(
                "[browser] Failed to start browser session '{}': {}",
                session_name, e
            );
            None
        }
    }
}

fn sanitize_session_name(session_id: &str) -> String {
    session_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect()
}

pub fn is_browser_command(command: &str) -> bool {
    let trimmed = command.trim_start();
    trimmed.starts_with("browser ") || trimmed == "browser" || trimmed.starts_with("browser\t")
}

pub fn is_setup_complete() -> bool {
    setup_marker_path().exists() && browser_binary_path().exists() && host_binary_path().exists()
}

fn mark_setup_complete() -> Result<()> {
    let marker = setup_marker_path();
    std::fs::write(&marker, chrono::Utc::now().to_rfc3339())?;
    Ok(())
}
pub fn rewrite_command_with_full_path(command: &str) -> String {
    let bin = browser_binary_path();
    if !bin.exists() {
        return command.to_string();
    }
    let trimmed = command.trim_start();
    if trimmed == "browser" {
        bin.to_string_lossy().to_string()
    } else if let Some(rest) = trimmed.strip_prefix("browser ") {
        format!("{} {}", bin.to_string_lossy(), rest)
    } else if let Some(rest) = trimmed.strip_prefix("browser\t") {
        format!("{} {}", bin.to_string_lossy(), rest)
    } else {
        command.to_string()
    }
}

pub async fn ensure_browser_setup() -> Result<String> {
    let mut log = String::new();

    std::fs::create_dir_all(browser_dir())?;

    let initial_status = ensure_browser_ready_noninteractive().await?;
    if initial_status.ready {
        log.push_str("Browser bridge is already set up and responding.\n");
        log.push_str("No setup action was needed.\n");
        return Ok(log);
    }

    if initial_status.responding && !initial_status.compatible {
        log.push_str("Browser bridge is connected, but the live Firefox extension is out of date for this alphacode build. Attempting repair steps...\n");
        if !initial_status.missing_actions.is_empty() {
            log.push_str(&format!(
                "Missing actions: {}\n",
                initial_status.missing_actions.join(", ")
            ));
        }
    } else if initial_status.binary_installed {
        log.push_str(
            "Browser bridge is installed but not fully ready. Attempting repair steps...\n",
        );
    } else {
        log.push_str("Browser bridge is not installed yet. Starting setup...\n");
    }

    // Step 1: Check/download native CLI + host binaries. Firefox installs the
    // signed extension directly from AMO; Alphacode never sideloads or opens a
    // bundled XPI.
    if !browser_binary_path().exists()
        || !host_binary_path().exists()
        || (initial_status.responding && !initial_status.compatible)
    {
        log.push_str("[1/3] Downloading browser bridge binaries... ");
        match download_browser_binary().await {
            Ok(()) => log.push_str("done\n"),
            Err(e) => {
                log.push_str(&format!("failed: {}\n", e));
                log.push_str("       Continuing setup; native bridge binaries may still need manual installation.\n");
            }
        }
    } else {
        log.push_str("[1/3] Browser CLI... already installed\n");
    }

    // Step 2: Install native messaging host manifest
    log.push_str("[2/3] Native messaging host... ");
    match install_native_host_manifest() {
        Ok(installed) => {
            if installed {
                log.push_str("installed\n");
            } else {
                log.push_str("already configured\n");
            }
        }
        Err(e) => {
            log.push_str(&format!("failed: {}\n", e));
            log.push_str("       You may need to run setup manually.\n");
        }
    }

    // Step 3: Check extension connectivity
    log.push_str("[3/3] Checking Firefox extension... ");
    match check_browser_ping().await {
        Ok(true) => {
            log.push_str("connected!\n");
            if initial_status.responding && !initial_status.compatible {
                log.push_str("       Existing extension is missing required actions. Opening its Firefox Add-ons page...\n");
                match open_browser_addon_page().await {
                    Ok(msg) => {
                        log.push_str(&msg);
                        log.push_str(
                            "       Update or add the extension in Firefox, then rerun `alphacode browser status`.\n",
                        );
                    }
                    Err(e) => {
                        log.push_str(&format!("       Could not open Firefox Add-ons: {}\n", e));
                    }
                }
            } else {
                mark_setup_complete().ok();
            }
        }
        Ok(false) => {
            log.push_str("not connected\n");
            if should_prompt_extension_install(&initial_status) {
                log.push_str("       Firefox extension needs to be installed.\n");

                match open_browser_addon_page().await {
                    Ok(msg) => {
                        log.push_str(&msg);
                        log.push_str(
                            "       After adding the extension, run `alphacode browser status` to verify the connection.\n",
                        );
                    }
                    Err(e) => {
                        log.push_str(&format!("       Could not open Firefox Add-ons: {}\n", e));
                        log.push_str(&format!(
                            "       Open this page in Firefox and select Add to Firefox: {}\n",
                            FIREFOX_ADDON_PAGE_URL
                        ));
                    }
                }
            } else {
                log.push_str(
                    "       Firefox is not responding. Start Firefox with the AlphaCode Browser Agent enabled, then rerun `alphacode browser status`.\n",
                );
            }
        }
        Err(e) => {
            log.push_str(&format!("error: {}\n", e));
            log.push_str("       Make sure Firefox is running.\n");
        }
    }

    let final_status = ensure_browser_ready_noninteractive().await?;
    if final_status.ready {
        log.push_str("\nSetup complete. Browser bridge is ready.\n");
    } else if final_status.responding && !final_status.compatible {
        log.push_str("\nSetup is not complete yet. The Firefox extension is connected, but it is still missing required actions for this alphacode build.\n");
        if !final_status.missing_actions.is_empty() {
            log.push_str(&format!(
                "Missing actions: {}\n",
                final_status.missing_actions.join(", ")
            ));
        }
        log.push_str("Use `alphacode browser status` to verify readiness after updating the extension in Firefox.\n");
    } else if final_status.binary_installed {
        log.push_str("\nSetup is not complete yet. Browser bridge binaries are installed, but the Firefox extension/bridge is not responding.\n");
        log.push_str(&format!(
            "Open the Firefox Add-ons page, select Add to Firefox, then run `alphacode browser status`: {}\n",
            FIREFOX_ADDON_PAGE_URL
        ));
    } else {
        log.push_str("\nSetup is not complete yet. Browser bridge binary is still missing.\n");
    }

    Ok(log)
}

async fn download_browser_binary() -> Result<()> {
    let asset_name = get_platform_asset_name();
    let client = crate::alphacode_provider_core::shared_http_client();

    let mut request = client
        .get(GITHUB_API_LATEST)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");
    // Avoid the shared unauthenticated 60 req/h per-IP GitHub bucket when a
    // token is available (see crate::github).
    if let Some(token) = crate::github::github_public_api_token() {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;
    let status = response.status();
    let release_info: serde_json::Value = if status.is_success() {
        response
            .json()
            .await
            .context("Failed to fetch latest release info")?
    } else if status.as_u16() == 404 {
        anyhow::bail!(
            "Browser bridge GitHub release not found (HTTP 404). The Firefox extension can be installed from Firefox Add-ons: {FIREFOX_ADDON_PAGE_URL}. Native bridge binaries must be installed separately in {}.",
            browser_dir().display()
        );
    } else {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Browser bridge GitHub API returned HTTP {}: {}",
            status,
            body.trim()
        );
    };

    let assets = release_info["assets"]
        .as_array()
        .context("No assets in release")?;
    let available_assets = || {
        assets
            .iter()
            .filter_map(|a| a["name"].as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Find the browser CLI binary
    let browser_asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&asset_name));
    let browser_missing = browser_asset.is_none();

    // Find the host binary
    let host_asset_name = get_host_asset_name();
    let host_asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&host_asset_name));
    let host_missing = host_asset.is_none();

    // Fail with the exact missing native assets. The extension is installed
    // separately by the user from the signed Firefox Add-ons listing.
    if browser_missing || host_missing {
        let mut missing = Vec::new();
        if browser_missing {
            missing.push(format!("browser CLI '{}'", asset_name));
        }
        if host_missing {
            missing.push(format!("native host '{}'", host_asset_name));
        }
        anyhow::bail!(
            "Browser bridge has no {} for this platform ({}-{}). Expected release \
             asset(s): {}. Available release assets: {}. Install the signed Firefox extension from {}. \
             For missing CLI/native host binaries, build them from source or copy compatible binaries into {} \
             and rerun `alphacode browser setup`.",
            missing.join(" and "),
            std::env::consts::OS,
            std::env::consts::ARCH,
            missing.join(", "),
            available_assets(),
            FIREFOX_ADDON_PAGE_URL,
            browser_dir().display()
        );
    }

    let download_url = browser_asset
        .and_then(|a| a["browser_download_url"].as_str())
        .with_context(|| {
            format!(
                "Release asset '{}' has no download URL. Available assets: {}",
                asset_name,
                available_assets()
            )
        })?;

    // Download browser CLI
    let browser_bytes = client
        .get(download_url)
        .send()
        .await?
        .bytes()
        .await
        .context("Failed to download browser binary")?;

    let bin_path = browser_binary_path();
    write_file_atomically(&bin_path, &browser_bytes, true)?;

    // Download host binary
    let host_url = host_asset
        .and_then(|a| a["browser_download_url"].as_str())
        .context("No host download URL")?;
    let host_bytes = client
        .get(host_url)
        .send()
        .await?
        .bytes()
        .await
        .context("Failed to download host binary")?;

    let host_path = host_binary_path();
    write_file_atomically(&host_path, &host_bytes, true)?;

    Ok(())
}

/// `fs::rename` with a bounded retry on Windows transient sharing failures.
///
/// `ERROR_ACCESS_DENIED` (5), `ERROR_SHARING_VIOLATION` (32) and
/// `ERROR_USER_MAPPED_FILE` (33) all mean "some other process still has the
/// destination open". Retry briefly for transient Windows sharing locks;
/// anywhere else a single atomic attempt is correct.
fn rename_with_retry(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        const ATTEMPTS: u32 = 10;
        const BACKOFF: std::time::Duration = std::time::Duration::from_millis(200);
        let mut last = None;
        for attempt in 0..ATTEMPTS {
            match std::fs::rename(from, to) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let transient = matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33));
                    last = Some(error);
                    if !transient || attempt + 1 == ATTEMPTS {
                        break;
                    }
                    std::thread::sleep(BACKOFF);
                }
            }
        }
        Err(last.expect("the loop always runs at least one attempt"))
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
}

#[allow(unused_variables)]
fn write_file_atomically(path: &std::path::Path, bytes: &[u8], executable: bool) -> Result<()> {
    let parent = path
        .parent()
        .context("Target file has no parent directory")?;
    std::fs::create_dir_all(parent)?;

    let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let pid = std::process::id();
    let tmp_path = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("download"),
        pid,
        ts
    ));

    // Every failure path from here on must remove the scratch file. The old
    // `?` chain leaked a `.<name>.tmp-<pid>-<nanos>` into the browser dir on
    // every failure, and `browser setup` runs repeatedly, so those accumulate.
    if let Err(error) = std::fs::write(&tmp_path, bytes) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error).context("Failed to write temporary file");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        if let Err(error) =
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(mode))
        {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(error).context("Failed to set permissions on temporary file");
        }
    }

    if let Err(error) = rename_with_retry(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error).with_context(|| format!("Failed to move {} into place", path.display()));
    }
    Ok(())
}

fn get_platform_asset_name() -> String {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "browser-linux-x64".to_string()
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "browser-linux-arm64".to_string()
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "browser-macos-arm64".to_string()
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "browser-macos-x64".to_string()
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "browser-windows-x64.exe".to_string()
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        format!(
            "browser-{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    }
}

fn get_host_asset_name() -> String {
    let base = get_platform_asset_name();
    base.replace("browser-", "host-")
}

fn manifest_allows_canonical(value: &serde_json::Value) -> bool {
    value
        .get("allowed_extensions")
        .and_then(|v| v.as_array())
        .is_some_and(|list| {
            list.iter().any(|entry| {
                entry.as_str() == Some(EXTENSION_ID_AMO)
                    || entry.as_str() == Some(BROWSER_EXTENSION_ID)
            })
        })
}

fn install_single_host_manifest(
    manifest_dir: &std::path::Path,
    host_name: &str,
    effective_host: &str,
) -> Result<bool> {
    let manifest_path = manifest_dir.join(format!("{}.json", host_name));

    // Reuse only when the existing manifest already points at a real binary
    // AND already whitelists the canonical extension ID. Older setups lack
    // `alpha-agent@alpha-agent.local`, which is exactly the bug that broke
    // the bridge — those must be rewritten, not skipped.
    if manifest_path.exists()
        && let Ok(contents) = std::fs::read_to_string(&manifest_path)
        && let Ok(existing) = serde_json::from_str::<serde_json::Value>(&contents)
        && let Some(existing_path) = existing["path"].as_str()
        && std::path::Path::new(existing_path).exists()
        && manifest_allows_canonical(&existing)
        && existing["name"].as_str() == Some(host_name)
    {
        #[cfg(target_os = "windows")]
        register_windows_native_host_manifest(&manifest_path, host_name)?;
        return Ok(false);
    }

    let manifest = serde_json::json!({
        "name": host_name,
        "description": "AlphaCode Browser Agent native messaging host (managed by alphacode)",
        "path": effective_host,
        "type": "stdio",
        "allowed_extensions": [
            EXTENSION_ID_AMO,
            EXTENSION_ID_LOCAL,
            EXTENSION_ID_LISTED,
            EXTENSION_ID_LISTED_LEGACY,
        ]
    });

    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    #[cfg(target_os = "windows")]
    register_windows_native_host_manifest(&manifest_path, host_name)?;

    Ok(true)
}

fn install_native_host_manifest() -> Result<bool> {
    let manifest_dir = native_messaging_hosts_dir()?;

    let host_path = host_binary_path();
    let browser_bin = browser_binary_path();

    let effective_host = if host_path.exists() {
        host_path.to_string_lossy().to_string()
    } else if browser_bin.exists() {
        return Err(anyhow::anyhow!(
            "Host binary not found at {}. The native messaging host is required for the Firefox extension to communicate with the bridge.",
            host_path.display()
        ));
    } else {
        return Err(anyhow::anyhow!("No browser binaries found"));
    };

    std::fs::create_dir_all(&manifest_dir)?;

    // The XPI tries `alpha_agent` first, then `firefox_agent_bridge`
    // (see background.js NATIVE_HOSTS). Install both manifests pointing at
    // the same host binary so either handshake path works.
    let canonical =
        install_single_host_manifest(&manifest_dir, NATIVE_HOST_NAME_CANONICAL, &effective_host)?;
    let legacy =
        install_single_host_manifest(&manifest_dir, NATIVE_HOST_NAME_LEGACY, &effective_host)?;

    Ok(canonical || legacy)
}

#[cfg(target_os = "windows")]
fn register_windows_native_host_manifest(
    manifest_path: &std::path::Path,
    host_name: &str,
) -> Result<()> {
    let key = format!(r"HKCU\Software\Mozilla\NativeMessagingHosts\{}", host_name);
    let output = std::process::Command::new("reg")
        .args([
            "add",
            &key,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            &manifest_path.to_string_lossy(),
            "/f",
        ])
        .output()
        .context("Failed to register Firefox native messaging host in Windows registry")?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = stderr.trim();
        if details.is_empty() {
            anyhow::bail!(
                "Failed to register Firefox native messaging host in Windows registry: {}",
                stdout.trim()
            );
        }
        anyhow::bail!(
            "Failed to register Firefox native messaging host in Windows registry: {}",
            details
        )
    }
}

fn native_messaging_hosts_dir() -> Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().context("No home directory")?;
        Ok(home.join(".mozilla").join("native-messaging-hosts"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("No home directory")?;
        Ok(home
            .join("Library")
            .join("Application Support")
            .join("Mozilla")
            .join("NativeMessagingHosts"))
    }
    #[cfg(target_os = "windows")]
    {
        // On Windows, native messaging hosts are registered via the Windows Registry
        // We'll write the manifest file to a known location and handle registry separately
        let appdata = dirs::data_dir().context("No app data directory")?;
        Ok(appdata.join("Mozilla").join("NativeMessagingHosts"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(anyhow::anyhow!("Unsupported platform for native messaging"))
    }
}

/// How long to wait for the browser CLI before declaring the bridge dead.
///
/// The CLI round-trips to the Firefox extension over `ws://127.0.0.1:8766`. If
/// the extension is missing, disabled, or Firefox is closed, nothing ever
/// answers and an unbounded `.output().await` hangs `browser status` and
/// `browser setup` for minutes. See #602.
const BRIDGE_PING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Run the browser CLI with a hard timeout, killing the child if it overruns.
///
/// `Ok(None)` means the call timed out, which callers treat as "not
/// responding" so they fail fast instead of hanging.
async fn run_browser_cli_capped(
    bin: &std::path::Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<Option<std::process::Output>> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args).kill_on_drop(true);

    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(output) => Ok(Some(output?)),
        Err(_) => {
            crate::logging::warn(&format!(
                "browser CLI '{}' timed out after {}s; treating the bridge as not responding",
                args.first().copied().unwrap_or("(no action)"),
                timeout.as_secs()
            ));
            Ok(None)
        }
    }
}

async fn check_browser_ping() -> Result<bool> {
    let bin = browser_binary_path();
    if !bin.exists() {
        return Ok(false);
    }

    match run_browser_cli_capped(&bin, &["ping"], BRIDGE_PING_TIMEOUT).await? {
        Some(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).contains("pong"))
        }
        _ => Ok(false),
    }
}

/// Fast liveness check for a browser action.
///
/// Full compatibility inspection deliberately probes every required wire
/// action and is appropriate for the explicit `status` command. Running that
/// suite before *every* browser action adds one CLI process per capability
/// (and serializes all of them), even though the action itself will report if
/// its specific wire action is unsupported. Keep normal interaction to one
/// bounded ping; reserve the full probe for diagnostics.
pub async fn browser_bridge_responding() -> Result<bool> {
    check_browser_ping().await
}

async fn probe_bridge_action_support(action: &str, params_json: &str) -> Result<bool> {
    let bin = browser_binary_path();
    if !bin.exists() {
        return Ok(false);
    }

    let Some(output) =
        run_browser_cli_capped(&bin, &[action, params_json], BRIDGE_PING_TIMEOUT).await?
    else {
        // A dead bridge cannot tell us whether the action exists; report it as
        // unsupported rather than hanging the caller (#602).
        return Ok(false);
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stderr.trim(), stdout.trim())
    };

    Ok(!combined.contains(&format!("Unknown action: {}", action)))
}

async fn probe_bridge_missing_actions() -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for (action, params_json) in REQUIRED_BRIDGE_ACTION_PROBES {
        if !probe_bridge_action_support(action, params_json).await? {
            missing.push((*action).to_string());
        }
    }
    Ok(missing)
}

pub async fn inspect_browser_status() -> Result<BrowserStatus> {
    let binary_installed = browser_binary_path().exists();
    let setup_complete = is_setup_complete();
    let responding = if binary_installed {
        check_browser_ping().await.unwrap_or(false)
    } else {
        false
    };
    let missing_actions = if responding {
        probe_bridge_missing_actions().await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let compatible = responding && missing_actions.is_empty();
    let ready = responding && compatible;

    Ok(BrowserStatus {
        backend: "firefox_agent_bridge",
        browser: "firefox",
        setup_complete,
        binary_installed,
        responding,
        compatible,
        missing_actions,
        ready,
    })
}

/// Silent native-host manifest install for both host names (`alpha_agent` and
/// `firefox_agent_bridge`). The extension is installed by the user from
/// Firefox Add-ons; do not sideload a bundled XPI or rewrite Firefox policies.
pub fn ensure_browser_assets_installed_silent() -> String {
    let mut log = String::new();
    if std::fs::create_dir_all(browser_dir()).is_err() {
        return log;
    }
    // Install/update both host manifests; ignore errors here (setup will
    // surface them with full context).
    match install_native_host_manifest() {
        Ok(true) => log.push_str("native host installed; "),
        Ok(false) => {}
        Err(e) => log.push_str(&format!("native host skipped: {}; ", e)),
    }
    log
}

pub async fn ensure_browser_ready_noninteractive() -> Result<BrowserStatus> {
    // Self-heal file assets on every status check so a fresh machine gets
    // binaries manifests without ever running `browser setup` explicitly.
    // Only file work — no network, no prompt, no wait.
    let _ = ensure_browser_assets_installed_silent();
    let mut status = inspect_browser_status().await?;
    if status.ready && !status.setup_complete {
        mark_setup_complete().ok();
        status.setup_complete = is_setup_complete();
    }
    Ok(status)
}

/// Whether `browser setup` should offer to (re)install the bridge extension.
///
/// Keying only off the persistent `.setup-complete` marker meant that once a
/// past setup succeeded, setup could never recover if the extension later
/// vanished from the live Firefox profile: it just printed "already completed".
/// Also re-prompt when the binary is installed but the bridge is not
/// responding, which is the only signal that a previously-working setup lost
/// its extension. A healthy responding bridge stays inert. See #602.
fn should_prompt_extension_install(status: &BrowserStatus) -> bool {
    if !status.setup_complete {
        return true;
    }
    status.binary_installed && !status.responding
}

/// Common Firefox install roots used to launch the Add-ons page directly.
/// Keep this to well-known locations only; missing directories are skipped.
#[cfg(target_os = "windows")]
fn firefox_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for candidate in [
        std::env::var("ProgramFiles")
            .ok()
            .map(|p| PathBuf::from(p).join("Mozilla Firefox")),
        std::env::var("ProgramFiles(x86)")
            .ok()
            .map(|p| PathBuf::from(p).join("Mozilla Firefox")),
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("Mozilla Firefox")),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_dir() {
            dirs.push(candidate);
        }
    }
    dirs
}

/// Open the supported Firefox Add-ons listing, rate-limited so repeated tool
/// retries do not create a stack of duplicate tabs.
pub async fn open_browser_addon_page() -> Result<String> {
    let now = Instant::now();
    let last_open = LAST_FIREFOX_ADDON_PAGE_OPEN.get_or_init(|| Mutex::new(None));
    {
        let mut last_open = last_open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last_open.is_some_and(|opened| opened.elapsed() < Duration::from_secs(30)) {
            return Ok(format!(
                "       Firefox Add-ons was opened recently. Complete installation there: {FIREFOX_ADDON_PAGE_URL}\n"
            ));
        }
        *last_open = Some(now);
    }

    let result = open_firefox_addon_page_unthrottled().await;
    if result.is_err() {
        let mut last_open = last_open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *last_open == Some(now) {
            *last_open = None;
        }
    }
    result
}

async fn open_firefox_addon_page_unthrottled() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        for root in firefox_install_dirs() {
            let firefox = root.join("firefox.exe");
            if firefox.is_file()
                && tokio::process::Command::new(&firefox)
                    .arg(FIREFOX_ADDON_PAGE_URL)
                    .spawn()
                    .is_ok()
            {
                return Ok(format!(
                    "       Opened Firefox Add-ons. Select Add to Firefox: {FIREFOX_ADDON_PAGE_URL}\n"
                ));
            }
        }
        if tokio::process::Command::new("firefox.exe")
            .arg(FIREFOX_ADDON_PAGE_URL)
            .spawn()
            .is_ok()
        {
            return Ok(format!(
                "       Opened Firefox Add-ons. Select Add to Firefox: {FIREFOX_ADDON_PAGE_URL}\n"
            ));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let opened = tokio::process::Command::new("open")
            .args(["-a", "Firefox", FIREFOX_ADDON_PAGE_URL])
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false);
        if opened {
            return Ok(format!(
                "       Opened Firefox Add-ons. Select Add to Firefox: {FIREFOX_ADDON_PAGE_URL}\n"
            ));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if tokio::process::Command::new("firefox")
            .arg(FIREFOX_ADDON_PAGE_URL)
            .spawn()
            .is_ok()
        {
            return Ok(format!(
                "       Opened Firefox Add-ons. Select Add to Firefox: {FIREFOX_ADDON_PAGE_URL}\n"
            ));
        }
    }

    Err(anyhow::anyhow!(
        "Could not launch Firefox. Open the Add-ons page manually and select Add to Firefox: {FIREFOX_ADDON_PAGE_URL}"
    ))
}

pub async fn run_setup_command() -> Result<()> {
    println!("Browser Automation Setup");
    println!("========================\n");
    println!("Backend: Firefox Agent Bridge\n");

    let log = ensure_browser_setup().await?;
    print!("{}", log);

    if is_setup_complete() {
        println!("\nTip: Import passwords from Chrome/Safari via Firefox Settings > Import Data");
    }

    Ok(())
}
