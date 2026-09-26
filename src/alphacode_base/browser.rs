use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::{platform, storage};

const GITHUB_API_LATEST: &str =
    "https://api.github.com/repos/1jehuang/firefox-agent-bridge/releases/latest";

// Centralized browser-bridge identity (see ChatGPT review + XPI verification).
//
// The bundled `AlphaCode-Browser-Agent-1.6.0.xpi` ("AlphaCode Browser Agent")
// declares:
//   gecko.id = "alpha-agent@alpha-agent.local"
//   background.js NATIVE_HOSTS = ["alpha_agent", "firefox_agent_bridge"]
// Firefox enforces `allowed_extensions` before launching the native host, so
// the whitelist MUST contain the canonical ID or the bridge can never connect.
// Old IDs are kept for backward compat during the transition — do NOT drop
// them until the old XPIs are fully retired.
pub const BROWSER_EXTENSION_ID: &str = "alpha-agent@alpha-agent.local";
pub const NATIVE_HOST_NAME_CANONICAL: &str = "alpha_agent";
/// Legacy host name kept for wire compat: the XPI falls back to it if
/// `alpha_agent` is not registered, and existing installs already reference it.
pub const NATIVE_HOST_NAME_LEGACY: &str = "firefox_agent_bridge";
const EXTENSION_ID_LISTED: &str = "browser-agent-bridge@alphacode.dev";
// Previous extension ID kept as fallback so existing installs keep working.
const EXTENSION_ID_LISTED_LEGACY: &str = "browser-agent-bridge@1jehuang.github.io";
const EXTENSION_ID_LOCAL: &str = "alphacode-browser-bridge@local";
// The ID of the XPI compiled into this binary (see EMBEDDED_XPI). This is the
// canonical extension ID for this build; the manifest whitelist must include
// it or Firefox refuses the native host connection.
const EXTENSION_ID_EMBEDDED: &str = BROWSER_EXTENSION_ID;

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

pub const EMBEDDED_XPI_FILENAME: &str = "AlphaCode-Browser-Agent-1.6.0.xpi";

/// Path used for the extension bundled with this binary.
///
/// Keep the version in the filename: Firefox can keep an XPI mapped for the
/// lifetime of the process on Windows, so overwriting the historical
/// `browser-agent-bridge.xpi` can fail with ERROR_USER_MAPPED_FILE. A new
/// versioned path makes setup refreshable without requiring Firefox to be
/// closed first.
pub fn xpi_path() -> PathBuf {
    browser_dir().join(EMBEDDED_XPI_FILENAME)
}

/// The Firefox extension, compiled into the binary from the repository-root
/// `AlphaCode-Browser-Agent-1.6.0.xpi`. Every rebuild picks up the current file,
/// so `browser setup` never needs to download the extension and works offline.
/// (The native CLI + host binaries are separate programs from an external
/// release and cannot be embedded — only the XPI lives in this repo.)
///
/// `build.rs` emits `cargo:rerun-if-changed` for the XPI so any update to
/// the file forces a rebuild and refreshes these bytes.
const EMBEDDED_XPI: &[u8] = include_bytes!("../../AlphaCode-Browser-Agent-1.6.0.xpi");

/// Raw bytes of the embedded Firefox extension. Exposed so diagnostics,
/// tests, and the release guard can verify the bridge was compiled in.
pub fn embedded_xpi_bytes() -> &'static [u8] {
    EMBEDDED_XPI
}

/// Length of the embedded XPI in bytes. Used by `browser status` diagnostics
/// and to keep `EMBEDDED_XPI` referenced even in builds that skip setup.
pub fn embedded_xpi_len() -> usize {
    EMBEDDED_XPI.len()
}

/// Install the embedded extension to the browser dir when it is missing or
/// differs (a rebuild with a new XPI refreshes the installed copy).
/// Returns `true` when it wrote the file.
fn install_embedded_xpi() -> Result<bool> {
    let path = xpi_path();
    let current = std::fs::read(&path).ok();
    if current.as_deref() == Some(EMBEDDED_XPI) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_file_atomically(&path, EMBEDDED_XPI, false)?;
    Ok(true)
}

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

    // Step 0 (offline-first): install the XPI compiled into this binary via
    // `include_bytes!("../../AlphaCode-Browser-Agent-1.6.0.xpi")`. This guarantees the
    // extension is available even with no network, and refreshes the installed
    // copy whenever a rebuild bundles a new XPI. It also keeps
    // `EMBEDDED_XPI` / `install_embedded_xpi` referenced so `cargo check`
    // does not report them as dead code.
    match install_embedded_xpi() {
        Ok(true) => log.push_str(&format!(
            "[0/3] Embedded extension ({} bytes, compiled in)... installed to {}\n",
            embedded_xpi_len(),
            xpi_path().display()
        )),
        Ok(false) => log.push_str("[0/3] Embedded extension... already up to date\n"),
        Err(e) => log.push_str(&format!(
            "[0/3] Embedded extension... failed to install: {}\n",
            e
        )),
    }

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

    // Step 1: Check/download native CLI + host binaries.
    // The XPI is already handled in step 0 from the bytes compiled into this
    // binary, so a network failure here must not abort setup — the extension
    // can still be installed manually and the manifest step below can still
    // run. This matters on Windows where no `browser-windows-x64.exe` /
    // `host-windows-x64.exe` release assets exist yet.
    if !browser_binary_path().exists()
        || !host_binary_path().exists()
        || (initial_status.responding && !initial_status.compatible)
    {
        log.push_str("[1/3] Downloading browser bridge binaries... ");
        match download_browser_binary().await {
            Ok(()) => {
                log.push_str("done\n");
                // `download_browser_binary` may have replaced the XPI with the
                // release copy; restore the compiled-in build so the installed
                // file always matches this binary.
                if let Ok(true) = install_embedded_xpi() {
                    log.push_str("       Restored embedded extension to match this build.\n");
                }
            }
            Err(e) => {
                log.push_str(&format!("failed: {}\n", e));
                log.push_str("       Continuing with the embedded extension; native binaries remain missing.\n");
                // Best-effort: make sure the embedded XPI is on disk even when
                // the download failed before it could save anything.
                let _ = install_embedded_xpi();
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
                log.push_str("       Existing extension is missing required actions. Opening Firefox install/update prompt...\n");
                match install_extension().await {
                    Ok(msg) => {
                        log.push_str(&msg);
                        log.push_str("       Waiting for extension update to become ready... ");
                        match wait_for_ready(15).await {
                            Ok(true) => {
                                log.push_str("ready!\n");
                                mark_setup_complete().ok();
                            }
                            Ok(false) => {
                                log.push_str("timed out\n");
                            }
                            Err(e) => {
                                log.push_str(&format!("error: {}\n", e));
                            }
                        }
                    }
                    Err(e) => {
                        log.push_str(&format!("       Could not auto-update extension: {}\n", e));
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

                match install_extension().await {
                    Ok(msg) => {
                        log.push_str(&msg);
                        // Check again after install attempt
                        log.push_str("       Waiting for extension connection... ");
                        match wait_for_ping(15).await {
                            Ok(true) => {
                                log.push_str("connected!\n");
                                mark_setup_complete().ok();
                            }
                            Ok(false) => {
                                log.push_str("timed out\n");
                                log.push_str(
                                    "       Extension not detected. You can retry with: alphacode browser setup\n",
                                );
                                log.push_str(
                                    "       Or manually install: Firefox > about:addons > Install from file > ",
                                );
                                log.push_str(&xpi_path().to_string_lossy());
                                log.push('\n');
                            }
                            Err(e) => {
                                log.push_str(&format!("error: {}\n", e));
                            }
                        }
                    }
                    Err(e) => {
                        log.push_str(&format!("       Could not auto-install extension: {}\n", e));
                        log.push_str(
                            "       Manually install: Firefox > about:addons > Install from file > ",
                        );
                        log.push_str(&xpi_path().to_string_lossy());
                        log.push('\n');
                    }
                }
            } else {
                log.push_str(
                    "       Existing browser setup was already completed, so setup will not reopen the extension installer.\n",
                );
                log.push_str(
                    "       Make sure Firefox is running with the Browser Agent Bridge extension enabled, then re-run `alphacode browser status`.\n",
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
        log.push_str(
            "Use `alphacode browser status` to re-check readiness after any manual Firefox step.\n",
        );
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
        // Still leave the compiled-in extension on disk so manual install works.
        let _ = install_embedded_xpi();
        anyhow::bail!(
            "Browser bridge GitHub release not found (HTTP 404). \
             The release repository 'dragonked2/alphacode' has no browser assets for this platform. \
             The embedded Firefox extension ({} bytes) was saved to {} — install it via Firefox > about:addons > Install from file. \
             For native binaries, manually install the browser bridge binaries into ~/.alphacode/browser/",
            embedded_xpi_len(),
            xpi_path().display()
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

    // Find the XPI
    let xpi_asset = assets.iter().find(|a| {
        a["name"]
            .as_str()
            .map(|n| n.ends_with(".xpi"))
            .unwrap_or(false)
    });

    // Find the host binary
    let host_asset_name = get_host_asset_name();
    let host_asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&host_asset_name));
    let host_missing = host_asset.is_none();

    // When the release publishes no binaries for this platform (currently the
    // case for Windows: no `browser-windows-x64.exe` / `host-windows-x64.exe`
    // assets), fail with the exact expected names plus the full asset list so
    // the user can confirm the gap instead of guessing. The compiled-in XPI
    // is installed first so the Firefox extension can still be installed
    // manually even when the native binaries are unavailable.
    if browser_missing || host_missing {
        // Prefer the XPI compiled into this binary (offline, always matches
        // this build). Fall back to the release XPI only if the embedded
        // install fails.
        let embedded_ok = install_embedded_xpi().unwrap_or(false) || xpi_path().exists();
        if !embedded_ok
            && let Some(xpi) = xpi_asset
            && let Some(xpi_url) = xpi["browser_download_url"].as_str()
            && let Ok(response) = client.get(xpi_url).send().await
            && let Ok(xpi_bytes) = response.bytes().await
        {
            let _ = write_file_atomically(&xpi_path(), &xpi_bytes, false);
        }
        let mut missing = Vec::new();
        if browser_missing {
            missing.push(format!("browser CLI '{}'", asset_name));
        }
        if host_missing {
            missing.push(format!("native host '{}'", host_asset_name));
        }
        anyhow::bail!(
            "Browser bridge has no {} for this platform ({}-{}). Expected release \
             asset(s): {}. Available release assets: {}. The Firefox extension (XPI, {} bytes, compiled into this binary) \
             was saved to {} — install it via Firefox > about:addons > Install from \
             file. For the missing CLI binaries, build them from source or copy \
             compatible binaries into {} and re-run `alphacode browser setup`.",
            missing.join(" and "),
            std::env::consts::OS,
            std::env::consts::ARCH,
            missing.join(", "),
            available_assets(),
            embedded_xpi_len(),
            xpi_path().display(),
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

    // Install the XPI compiled into this binary as the source of truth.
    // Older releases may not publish an XPI asset at all; the embedded copy
    // always matches this build, so prefer it and only fall back to the
    // release download when the embedded install fails.
    if install_embedded_xpi().is_err()
        && let Some(xpi) = xpi_asset
        && let Some(xpi_url) = xpi["browser_download_url"].as_str()
    {
        let xpi_bytes = client
            .get(xpi_url)
            .send()
            .await?
            .bytes()
            .await
            .context("Failed to download XPI")?;
        write_file_atomically(&xpi_path(), &xpi_bytes, false)?;
    }

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

#[allow(unused_variables)]
fn write_file_atomically(path: &PathBuf, bytes: &[u8], executable: bool) -> Result<()> {
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

    std::fs::write(&tmp_path, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(mode))?;
    }

    std::fs::rename(&tmp_path, path)?;
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
                entry.as_str() == Some(EXTENSION_ID_EMBEDDED)
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
            EXTENSION_ID_EMBEDDED,
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

/// Silent, offline-first asset install: embedded XPI + both native-host
/// manifests (`alpha_agent` + `firefox_agent_bridge`). No network, no Firefox
/// prompt, no 15s ping wait. Safe to call on every startup / status check —
/// it only writes when files are missing or stale. This is what lets the
/// browser work without the user ever typing `alphacode browser setup`.
pub fn ensure_browser_assets_installed_silent() -> String {
    let mut log = String::new();
    if std::fs::create_dir_all(browser_dir()).is_err() {
        return log;
    }
    match install_embedded_xpi() {
        Ok(true) => log.push_str("embedded extension installed; "),
        Ok(false) => {}
        Err(e) => log.push_str(&format!("embedded extension failed: {}; ", e)),
    }
    // Install/update both host manifests; ignore errors here (setup will
    // surface them with full context).
    match install_native_host_manifest() {
        Ok(true) => log.push_str("native host installed; "),
        Ok(false) => {}
        Err(e) => log.push_str(&format!("native host skipped: {}; ", e)),
    }
    // Zero-click extension paths (sideload + policy). Best-effort: no admin =
    // no write, and the manual prompt in `install_extension` stays fallback.
    if !install_extension_sideload_sync().is_empty() {
        log.push_str("extension sideloaded; ");
    }
    if !install_extension_policy_sync().is_empty() {
        log.push_str("extension policy written; ");
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

async fn wait_for_ping(timeout_secs: u64) -> Result<bool> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        if let Ok(true) = check_browser_ping().await {
            return Ok(true);
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Ok(false)
}

async fn wait_for_ready(timeout_secs: u64) -> Result<bool> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        if let Ok(status) = ensure_browser_ready_noninteractive().await
            && status.ready
        {
            return Ok(true);
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Ok(false)
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

/// Firefox install roots we know how to sideload/policy into.
///
/// Keep this to well-known locations only — no new dependencies, no registry
/// reads. Missing dirs are skipped silently.
fn firefox_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
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
    }
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/Firefox.app/Contents/Resources");
        if app.is_dir() {
            dirs.push(app);
        }
    }
    #[cfg(target_os = "linux")]
    {
        for candidate in [
            PathBuf::from("/usr/lib/firefox"),
            PathBuf::from("/usr/lib64/firefox"),
            PathBuf::from("/opt/firefox"),
            PathBuf::from("/snap/firefox/current/usr/lib/firefox"),
        ] {
            if candidate.is_dir() {
                dirs.push(candidate);
            }
        }
    }
    dirs
}

fn sideload_filename() -> String {
    format!("{}.xpi", BROWSER_EXTENSION_ID)
}

/// Best-effort zero-click sideload: copy the embedded XPI to
/// `<firefox>/distribution/extensions/<id>.xpi` so Firefox picks it up on next
/// start without the file-picker. Returns paths written. Failures (e.g. no
/// admin for `C:\Program Files`) are silently skipped — the manual prompt
/// below remains the fallback.
fn install_extension_sideload_sync() -> Vec<String> {
    let mut written = Vec::new();
    for root in firefox_install_dirs() {
        let dest_dir = root.join("distribution").join("extensions");
        if std::fs::create_dir_all(&dest_dir).is_err() {
            continue;
        }
        let dest = dest_dir.join(sideload_filename());
        let current = std::fs::read(&dest).ok();
        if current.as_deref() == Some(embedded_xpi_bytes()) {
            continue;
        }
        if write_file_atomically(&dest, embedded_xpi_bytes(), false).is_ok() {
            written.push(dest.display().to_string());
        }
    }
    written
}

/// Merge our force-install entry into an existing (or empty) policies object,
/// preserving any admin-configured keys.
fn merge_extension_policy(
    existing: Option<serde_json::Value>,
    install_url: &str,
) -> serde_json::Value {
    let mut root = existing.unwrap_or_else(|| serde_json::json!({}));
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let policies = root
        .as_object_mut()
        .expect("checked is_object")
        .entry("policies")
        .or_insert_with(|| serde_json::json!({}));
    if !policies.is_object() {
        *policies = serde_json::json!({});
    }
    let settings = policies
        .as_object_mut()
        .expect("checked is_object")
        .entry("ExtensionSettings")
        .or_insert_with(|| serde_json::json!({}));
    if !settings.is_object() {
        *settings = serde_json::json!({});
    }
    settings.as_object_mut().expect("checked is_object").insert(
        BROWSER_EXTENSION_ID.to_string(),
        serde_json::json!({
            "installation_mode": "force_installed",
            "install_url": install_url,
            // Helps unsigned builds on ESR/Developer/Nightly; Release
            // still enforces signing regardless of policy.
            "temporarily_allow_weak_signatures": true,
        }),
    );
    root
}

/// Best-effort enterprise-policy install: write/merge
/// `<firefox>/distribution/policies.json` with a `force_installed` entry for
/// the canonical extension ID. Returns policies written. Like sideload, admin
/// rights may be needed — failures are skipped, manual prompt stays fallback.
fn install_extension_policy_sync() -> Vec<String> {
    let mut written = Vec::new();
    let install_url = url::Url::from_file_path(xpi_path())
        .map(|u| u.to_string())
        .unwrap_or_default();
    if install_url.is_empty() {
        return written;
    }
    for root in firefox_install_dirs() {
        let dist = root.join("distribution");
        if std::fs::create_dir_all(&dist).is_err() {
            continue;
        }
        let path = dist.join("policies.json");
        let existing = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let merged = merge_extension_policy(existing, &install_url);
        let text = serde_json::to_string_pretty(&merged).unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        if std::fs::write(&path, format!("{text}\n")).is_ok() {
            written.push(path.display().to_string());
        }
    }
    written
}

async fn install_extension() -> Result<String> {
    let xpi = xpi_path();
    let mut msg = String::new();

    if !xpi.exists() {
        return Err(anyhow::anyhow!("XPI file not found at {}", xpi.display()));
    }

    // Zero-click attempts first: sideload + enterprise policy. Either one
    // removes the file-picker step — Firefox installs on next start.
    for path in install_extension_sideload_sync() {
        msg.push_str(&format!("       Sideloaded extension to {path}\n"));
    }
    for path in install_extension_policy_sync() {
        msg.push_str(&format!("       Wrote enterprise policy to {path}\n"));
    }
    if msg.contains("Sideloaded") || msg.contains("enterprise policy") {
        msg.push_str("       Restart Firefox to pick up the force-installed extension.\n");
        msg.push_str("       Note: Firefox Release still requires a signed XPI; this unsigned build installs cleanly on ESR/Developer/Nightly (or after AMO signing).\n");
    }

    // Manual fallback: open Firefox with the XPI to trigger install prompt
    let xpi_url = url::Url::from_file_path(&xpi)
        .map_err(|_| anyhow::anyhow!("Could not convert XPI path to file URL: {}", xpi.display()))?
        .to_string();

    #[cfg(target_os = "linux")]
    {
        let _ = tokio::process::Command::new("xdg-open")
            .arg(&xpi_url)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        // macOS has no default handler for `.xpi` files, so a plain `open <url>`
        // fails with kLSApplicationNotFoundErr. Open the XPI directly with
        // Firefox, which knows how to install extensions. Try the app name first,
        // then fall back to the bundle id (covers Firefox installed under a
        // non-default name or when it is not the default browser).
        let opened = tokio::process::Command::new("open")
            .args(["-a", "Firefox", &xpi_url])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if !opened {
            let opened_by_id = tokio::process::Command::new("open")
                .args(["-b", "org.mozilla.firefox", &xpi_url])
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if !opened_by_id {
                // Last resort: let Launch Services pick a handler. This likely
                // fails for `.xpi`, but keeps the previous behavior as a fallback.
                let _ = tokio::process::Command::new("open").arg(&xpi_url).spawn();
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = tokio::process::Command::new("cmd")
            .args(["/C", "start", "", &xpi_url])
            .spawn();
    }

    msg.push_str("       Opened Firefox with extension install prompt.\n");
    msg.push_str("       Click \"Add\" when prompted to install the extension.\n");

    Ok(msg)
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
    use super::*;
    use std::io::{Cursor, Read};

    fn embedded_xpi_text(name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(EMBEDDED_XPI))
            .expect("embedded XPI must be a valid ZIP archive");
        let mut file = archive
            .by_name(name)
            .unwrap_or_else(|error| panic!("embedded XPI is missing {name}: {error}"));
        let mut text = String::new();
        file.read_to_string(&mut text)
            .expect("embedded XPI text must be UTF-8");
        text
    }

    #[test]
    fn installed_xpi_uses_versioned_path() {
        assert_eq!(
            xpi_path().file_name().and_then(|name| name.to_str()),
            Some(EMBEDDED_XPI_FILENAME)
        );
    }

    #[test]
    fn embedded_xpi_uses_function_body_eval_with_return_and_await_support() {
        let content = embedded_xpi_text("content.js");
        assert!(
            content.contains("new AsyncFunction(body).call(window)"),
            "browser eval must execute inside an async function body"
        );
        assert!(
            !content.contains("globalThis['eval'](code)"),
            "the bridge 1.4.1 raw-eval path must not return"
        );
    }

    #[test]
    fn embedded_xpi_has_bounded_native_transfer_accounting() {
        let background = embedded_xpi_text("background.js");
        assert!(background.contains("receivedBytes"));
        assert!(background.contains("params = injectTransferData(params)"));
        assert!(background.contains("data.length > MAX_NATIVE_CHUNK_SIZE"));
        assert!(!background.contains("MAX_NATIVE_CHUNK_SIZE * 2"));
    }

    #[test]
    fn embedded_xpi_version_and_identity_match_the_binary() {
        let manifest: serde_json::Value = serde_json::from_str(&embedded_xpi_text("manifest.json"))
            .expect("embedded manifest must be valid JSON");
        assert_eq!(manifest["version"], "1.6.1");
        assert_eq!(
            manifest["browser_specific_settings"]["gecko"]["id"],
            BROWSER_EXTENSION_ID
        );
    }
}
