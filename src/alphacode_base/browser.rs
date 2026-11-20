use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::storage;

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

/// These actions can validate a request without touching the active tab.
/// Tab creation is omitted because it accepts empty parameters.
const TAB_ACTION_PROBES: &[&str] = &[
    "evaluate",
    "getActiveTab",
    "setActiveTab",
    "closeTab",
    "listFrames",
    "navigate",
    "reload",
    "back",
    "forward",
    "getContent",
    "getInteractables",
    "click",
    "hover",
    "type",
    "fillForm",
    "drag",
    "waitFor",
    "scroll",
    "uploadFile",
    "screenshot",
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

#[cfg(unix)]
fn is_session_alive(name: &str) -> bool {
    use std::os::unix::fs::FileTypeExt;

    // These paths belong to the browser CLI. It uses XDG_RUNTIME_DIR or /tmp,
    // independently of the Alphacode daemon's runtime or storage directory.
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let pid_path = dir.join(format!("browser-session-{name}.pid"));
    let socket_path = dir.join(format!("browser-session-{name}.sock"));
    std::fs::read_to_string(pid_path)
        .ok()
        .and_then(|pid| pid.trim().parse().ok())
        .is_some_and(crate::platform::is_process_running)
        && std::fs::symlink_metadata(socket_path)
            .is_ok_and(|metadata| metadata.file_type().is_socket())
}

#[cfg(not(unix))]
pub async fn ensure_browser_session(_session_id: &str) -> Option<String> {
    // The browser CLI uses Unix sockets for session routing.
    None
}

#[cfg(unix)]
pub async fn ensure_browser_session(session_id: &str) -> Option<String> {
    static START_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let session_name = sanitize_session_name(session_id);
    if session_name.is_empty() {
        return None;
    }

    if is_session_alive(&session_name) {
        return Some(session_name);
    }

    let _guard = START_LOCK.lock().await;
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
    if browser_supports_bind_window(&bin).await
        && let Some(name) = spawn_browser_session(&bin, &session_name, true).await
    {
        return Some(name);
    }
    spawn_browser_session(&bin, &session_name, false).await
}

#[cfg(unix)]
async fn browser_supports_bind_window(bin: &std::path::Path) -> bool {
    run_browser_cli_capped(bin, &["session", "start", "--help"], Duration::from_secs(2))
        .await
        .ok()
        .flatten()
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains("--bind-window")
                || String::from_utf8_lossy(&output.stderr).contains("--bind-window")
        })
}

#[cfg(unix)]
async fn spawn_browser_session(
    bin: &std::path::Path,
    session_name: &str,
    bind_window: bool,
) -> Option<String> {
    let mut args = vec!["session", "start", session_name];
    if bind_window {
        args.push("--bind-window");
    }
    let mut child = tokio::process::Command::new(bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .inspect_err(|error| {
            tracing::debug!(session_name, %error, "Could not start browser session");
        })
        .ok()?;

    let ready = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if is_session_alive(session_name) {
                return Ok(true);
            }
            if let Some(status) = child.try_wait()? {
                tracing::debug!(session_name, %status, "Browser session process exited");
                return Ok::<_, std::io::Error>(false);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if matches!(ready, Ok(Ok(true))) {
        // Reap the helper after a Firefox disconnect, and kill it if the
        // owning async runtime shuts down first.
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        return Some(session_name.to_string());
    }
    tracing::debug!(session_name, ?ready, "Browser session did not become ready");
    let _ = child.kill().await;
    None
}

#[cfg(unix)]
fn sanitize_session_name(session_id: &str) -> String {
    session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
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
    static SETUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = SETUP_LOCK.lock().await;
    let mut log = String::new();

    std::fs::create_dir_all(browser_dir())?;

    let initial_status = inspect_browser_status().await?;
    if initial_status.ready && initial_status.setup_complete {
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
    if !browser_binary_path().exists() || !host_binary_path().exists() {
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
    let final_status = inspect_browser_status().await?;
    if final_status.responding {
        log.push_str("connected!\n");
        if !final_status.compatible {
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
        }
    } else {
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

    if final_status.ready {
        mark_setup_complete().context("Failed to record browser setup completion")?;
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
    let missing: Vec<_> = [
        (get_host_asset_name(), host_binary_path()),
        (get_platform_asset_name(), browser_binary_path()),
    ]
    .into_iter()
    .filter(|(_, path)| !path.exists())
    .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let client = crate::alphacode_provider_core::shared_http_client();

    let mut request = client
        .get(GITHUB_API_LATEST)
        .timeout(Duration::from_secs(30))
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

    // Repair just the missing components. An existing native host can be
    // running and locked against replacement by Windows.
    let downloads = missing.iter().map(|(name, _)| {
        let client = &client;
        let available_assets = &available_assets;
        async move {
            let url = assets
                .iter()
                .find(|asset| asset["name"].as_str() == Some(name.as_str()))
                .and_then(|asset| asset["browser_download_url"].as_str())
                .with_context(|| {
                    format!(
                        "Browser bridge release has no download for '{name}'. Available assets: {}. \
                         Copy a compatible binary into {} and rerun `alphacode browser setup`.",
                        available_assets(),
                        browser_dir().display(),
                    )
                })?;
            download_bridge_asset(client, url, name).await
        }
    });
    // Validate every download before installing anything.
    let files = futures::future::try_join_all(downloads).await?;
    for ((_, path), bytes) in missing.iter().zip(files) {
        if !path.exists() {
            write_file_atomically(path, &bytes, true)?;
        }
    }

    Ok(())
}

async fn download_bridge_asset(
    client: &reqwest::Client,
    url: &str,
    asset_name: &str,
) -> Result<bytes::Bytes> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("Failed to download browser bridge asset '{asset_name}'"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("Failed to read browser bridge asset '{asset_name}'"))?;
    anyhow::ensure!(
        !bytes.is_empty(),
        "Browser bridge asset '{asset_name}' is empty"
    );
    Ok(bytes)
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
    use std::io::Write;

    let parent = path
        .parent()
        .context("Target file has no parent directory")?;
    std::fs::create_dir_all(parent)?;

    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .context("Failed to create temporary browser bridge file")?;
    temporary
        .write_all(bytes)
        .context("Failed to write temporary browser bridge file")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if executable {
            temporary
                .as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o755))
                .context("Failed to make browser bridge file executable")?;
        }
    }

    let temporary = temporary.into_temp_path();
    rename_with_retry(&temporary, path)
        .with_context(|| format!("Failed to move {} into place", path.display()))
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

    write_file_atomically(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
        false,
    )?;

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
    let output = crate::alphacode_base::platform::helper_command("reg")
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
    crate::alphacode_base::platform::hide_helper_console(cmd.as_std_mut());
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

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
        Some(output) if output.status.success() => Ok(is_pong(&output.stdout)),
        _ => Ok(false),
    }
}

fn is_pong(stdout: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(stdout)
        .is_ok_and(|value| value.get("pong").and_then(serde_json::Value::as_bool) == Some(true))
        || std::str::from_utf8(stdout).is_ok_and(|text| text.trim() == "pong")
}

/// Fast liveness check for a browser action.
///
/// Keep normal interaction to one bounded ping and reserve compatibility
/// probes for diagnostics.
pub async fn browser_bridge_responding() -> Result<bool> {
    check_browser_ping().await
}

fn bridge_action_probes() -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::json;

    TAB_ACTION_PROBES
        .iter()
        .map(|&action| (action, json!({ "tabId": -1 })))
        .chain([
            ("listTabs", json!({ "windowId": -1 })),
            ("listCookies", json!({ "url": "https://example.invalid" })),
            ("setCookie", json!({})),
            ("removeCookie", json!({})),
        ])
        .collect()
}

fn parse_bridge_probe_results(
    stdout: &[u8],
    actions: &[(&str, serde_json::Value)],
) -> Result<Vec<String>> {
    let response: serde_json::Value =
        serde_json::from_slice(stdout).context("Invalid browser bridge capability response")?;
    let results = response["results"]
        .as_array()
        .filter(|results| results.len() == actions.len())
        .context("Incomplete browser bridge capability response")?;

    let mut missing = Vec::new();
    for (result, (action, _)) in results.iter().zip(actions) {
        anyhow::ensure!(
            result["action"].as_str() == Some(action),
            "Unexpected browser bridge capability result for '{action}'"
        );
        match result["ok"].as_bool() {
            Some(true) => {}
            Some(false) => {
                let error = result["error"]
                    .as_str()
                    .filter(|error| !error.trim().is_empty())
                    .with_context(|| format!("Browser bridge gave no diagnostic for '{action}'"))?;
                if error
                    .trim()
                    .eq_ignore_ascii_case(&format!("Unknown action: {action}"))
                {
                    missing.push(action.to_string());
                }
            }
            None => anyhow::bail!("Browser bridge gave no capability result for '{action}'"),
        }
    }
    Ok(missing)
}

async fn probe_bridge_missing_actions() -> Result<Vec<String>> {
    let actions = bridge_action_probes();
    let commands: Vec<_> = actions
        .iter()
        .map(|(action, params)| serde_json::json!({ "action": action, "params": params }))
        .collect();
    let payload = serde_json::json!({ "commands": commands, "stopOnError": false }).to_string();
    let output = run_browser_cli_capped(
        &browser_binary_path(),
        &["batch", &payload],
        BRIDGE_PING_TIMEOUT,
    )
    .await?
    .context("Browser bridge capability check timed out")?;
    anyhow::ensure!(
        output.status.success(),
        "Browser bridge capability check failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim(),
    );
    parse_bridge_probe_results(&output.stdout, &actions)
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
        probe_bridge_missing_actions().await?
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

/// Common Firefox locations, including the Microsoft Store execution alias.
#[cfg(target_os = "windows")]
fn windows_firefox_paths() -> impl Iterator<Item = PathBuf> {
    [
        ("ProgramFiles", "Mozilla Firefox"),
        ("ProgramFiles(x86)", "Mozilla Firefox"),
        ("LOCALAPPDATA", "Mozilla Firefox"),
        ("LOCALAPPDATA", "Microsoft/WindowsApps"),
    ]
    .into_iter()
    .filter_map(|(root, directory)| {
        std::env::var_os(root).map(|path| PathBuf::from(path).join(directory).join("firefox.exe"))
    })
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
    // A browser window opens only for a human attached to this terminal;
    // agent/batch invocations of `browser status` get the URL to visit
    // themselves (returned below / logged), never a popping tab.
    if crate::alphacode_base::auth::browser_suppressed(false) {
        return Ok(format!(
            "       Visit the Firefox Add-ons page to install manually: {FIREFOX_ADDON_PAGE_URL}\n"
        ));
    }
    #[cfg(target_os = "windows")]
    {
        for firefox in windows_firefox_paths() {
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

#[cfg(test)]
mod tests {
    use super::{bridge_action_probes, is_pong, parse_bridge_probe_results};
    use serde_json::json;

    #[test]
    fn bridge_ping_requires_a_positive_acknowledgment() {
        assert!(is_pong(br#"{ "pong": true, "version": "1.3.0" }"#));
        assert!(is_pong(b"pong\n"));
        assert!(!is_pong(br#"{ "pong": false }"#));
        assert!(!is_pong(b"ping failed: missing pong"));
    }

    #[test]
    fn bridge_status_does_not_target_the_active_tab() {
        let actions = bridge_action_probes();
        for action in ["scroll", "getContent", "screenshot", "evaluate", "closeTab"] {
            let (_, params) = actions.iter().find(|(name, _)| *name == action).unwrap();
            assert_eq!(*params, json!({ "tabId": -1 }));
        }
    }

    #[test]
    fn bridge_status_distinguishes_unsupported_actions_and_invalid_targets() {
        let actions = [
            ("scroll", json!({ "tabId": -1 })),
            ("drag", json!({ "tabId": -1 })),
        ];
        let response = br#"{ "results": [
            { "action": "scroll", "ok": false, "error": "tabId must be a non-negative integer" },
            { "action": "drag", "ok": false, "error": "Unknown action: drag" }
        ] }"#;
        assert_eq!(
            parse_bridge_probe_results(response, &actions).unwrap(),
            ["drag"]
        );
    }

    #[test]
    fn bridge_status_rejects_incomplete_and_uninformative_responses() {
        let actions = [("scroll", json!({ "tabId": -1 }))];
        for response in [
            br#"{ "results": [] }"#.as_slice(),
            br#"{ "results": [{ "action": "drag", "ok": true }] }"#,
            br#"{ "results": [{ "action": "scroll", "ok": false }] }"#,
        ] {
            assert!(parse_bridge_probe_results(response, &actions).is_err());
        }
    }
}
