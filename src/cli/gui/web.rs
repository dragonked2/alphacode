//! AlphaCode Desktop & Web GUI host — `alphacode --gui` / `alphacode gui [--web]`
//!
//! Serves the React `ui/dist` assets (embedded at build time when present, with
//! fallback to `ui/dist` on disk) on a random localhost port and launches the
//! native WebView window (or default browser with `--web`). A lightweight
//! WebSocket endpoint on the same port bridges the JSON-line `gui.rs` backend
//! protocol so the UI can send actions and receive streamed replies.
//!
//! Architecture (zero new heavy deps — reuses tokio + tokio-tungstenite):
//!
//!   client   ←HTTP GET /→   static asset handler (embedded ui/dist or disk fallback)
//!   client   ←WS /ws→       bridge relay ↔ `alphacode gui --bridge` JSON-line protocol

use anyhow::{Context, Result, bail};
use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command as ProcCommand;

include!(concat!(env!("OUT_DIR"), "/gui_embedded_assets.rs"));

// ── MIME type helper ─────────────────────────────────────────────────────────

fn mime_for_ext(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "json" => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn lookup_embedded_asset_gz(rel: &str) -> Option<&'static [u8]> {
    EMBEDDED_UI_ASSETS_GZIP
        .binary_search_by_key(&rel, |(path, _)| path)
        .ok()
        .map(|idx| EMBEDDED_UI_ASSETS_GZIP[idx].1)
}

fn accepts_gzip(request: &str) -> bool {
    for line in request.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("accept-encoding") {
            return value.split(',').any(|part| {
                part.split(';')
                    .next()
                    .is_some_and(|enc| enc.trim().eq_ignore_ascii_case("gzip"))
            });
        }
    }
    false
}

fn decode_gzip_asset(gz: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(gz);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).ok()?;
    Some(out)
}

fn embedded_asset_payload(
    gz: &'static [u8],
    client_accepts_gzip: bool,
) -> Option<(Cow<'static, [u8]>, Option<&'static str>)> {
    if client_accepts_gzip {
        Some((Cow::Borrowed(gz), Some("gzip")))
    } else {
        decode_gzip_asset(gz).map(|raw| (Cow::Owned(raw), None))
    }
}

/// Non-empty marker string when the React frontend bundle was embedded at build time.
pub(crate) fn embedded_ui_bundle_marker() -> &'static str {
    EMBEDDED_UI_BUNDLE_MARKER
}

/// Sanitize a request URL path into a safe relative asset path (POSIX separators).
/// Rejects path traversal (`..`), backslashes, and NUL bytes.
fn sanitize_rel_path(raw_path: &str) -> Option<String> {
    let path_only = raw_path.split(['?', '#']).next().unwrap_or("/");
    if path_only.contains('\0') || path_only.contains('\\') {
        return None;
    }

    let mut clean_segments = Vec::new();
    for segment in path_only.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        let lower = segment.to_ascii_lowercase();
        if segment == ".."
            || lower.contains("%2e")
            || lower.contains("%2f")
            || lower.contains("%5c")
        {
            return None;
        }
        clean_segments.push(segment);
    }

    if clean_segments.is_empty() {
        Some("index.html".to_string())
    } else {
        Some(clean_segments.join("/"))
    }
}

fn has_file_extension(rel: &str) -> bool {
    rel.rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.') && !name.starts_with('.'))
}

// ── Static file response ──────────────────────────────────────────────────────

fn http_response(
    status: u16,
    reason: &str,
    content_type: &str,
    content_encoding: Option<&str>,
    body: &[u8],
    include_body: bool,
) -> Vec<u8> {
    let encoding_header = content_encoding
        .map(|enc| format!("Content-Encoding: {enc}\r\nVary: Accept-Encoding\r\n"))
        .unwrap_or_else(|| "Vary: Accept-Encoding\r\n".to_string());
    let marker = embedded_ui_bundle_marker();
    let marker_header = if marker.is_empty() {
        String::new()
    } else {
        format!("X-AlphaCode-UI: {marker}\r\n")
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         {encoding_header}\
         {marker_header}\
         Content-Length: {}\r\n\
         Cache-Control: no-cache\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Referrer-Policy: no-referrer\r\n\
         \r\n",
        body.len()
    );
    let mut out = header.into_bytes();
    if include_body {
        out.extend_from_slice(body);
    }
    out
}

async fn read_disk_asset(dist: &Path, rel: &str) -> Option<Vec<u8>> {
    let canon_root = tokio::fs::canonicalize(dist).await.ok()?;
    let candidate = dist.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let canon_file = tokio::fs::canonicalize(&candidate).await.ok()?;
    if !canon_file.starts_with(&canon_root) {
        return None;
    }
    let meta = tokio::fs::metadata(&canon_file).await.ok()?;
    if !meta.is_file() {
        return None;
    }
    tokio::fs::read(&canon_file).await.ok()
}

// ── Serve one HTTP connection ─────────────────────────────────────────────────

async fn serve_http(mut stream: TcpStream, disk_dist: Option<Arc<PathBuf>>) -> Result<()> {
    use tokio::io::AsyncReadExt;

    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = std::str::from_utf8(&buf[..n]).unwrap_or("");
    let first_line = request.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let client_accepts_gzip = accepts_gzip(request);

    if !matches!(method, "GET" | "HEAD") {
        let resp = http_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            None,
            b"Method Not Allowed",
            true,
        );
        stream.write_all(&resp).await?;
        return Ok(());
    }
    let include_body = method == "GET";

    let Some(rel) = sanitize_rel_path(raw_path) else {
        let resp = http_response(
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            None,
            b"Bad Request",
            include_body,
        );
        stream.write_all(&resp).await?;
        return Ok(());
    };

    let (status, reason, ct, encoding, body): (u16, &str, &str, Option<&str>, Cow<'static, [u8]>) =
        if let Some(gz) = lookup_embedded_asset_gz(&rel)
            && let Some((payload, enc)) = embedded_asset_payload(gz, client_accepts_gzip)
        {
            let ext = Path::new(&rel)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            (200, "OK", mime_for_ext(ext), enc, payload)
        } else if let Some(dist) = disk_dist.as_deref()
            && let Some(bytes) = read_disk_asset(dist, &rel).await
        {
            let ext = Path::new(&rel)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            (200, "OK", mime_for_ext(ext), None, Cow::Owned(bytes))
        } else if !has_file_extension(&rel) || rel == "index.html" {
            // SPA fallback: serve index.html for client-side routes
            if let Some(gz) = lookup_embedded_asset_gz("index.html")
                && let Some((payload, enc)) = embedded_asset_payload(gz, client_accepts_gzip)
            {
                (200, "OK", "text/html; charset=utf-8", enc, payload)
            } else if let Some(dist) = disk_dist.as_deref()
                && let Some(bytes) = read_disk_asset(dist, "index.html").await
            {
                (
                    200,
                    "OK",
                    "text/html; charset=utf-8",
                    None,
                    Cow::Owned(bytes),
                )
            } else {
                (
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    None,
                    Cow::Borrowed(b"404"),
                )
            }
        } else {
            (
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                None,
                Cow::Borrowed(b"404"),
            )
        };

    let resp = http_response(status, reason, ct, encoding, &body, include_body);
    stream.write_all(&resp).await?;
    Ok(())
}

// ── CSWSH Origin validation ───────────────────────────────────────────────────

/// Validate that a browser `Origin` header matches our loopback GUI server.
fn is_trusted_ws_origin(origin: &str, bound_port: u16) -> bool {
    let origin = origin.trim();
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("HTTP://"))
    else {
        return false;
    };

    if rest.is_empty() || rest.contains(['/', '@', '?', '#']) {
        return false;
    }

    let (host, port_str) = if let Some(after_v6) = rest.strip_prefix("[::1]") {
        let Some(port) = after_v6.strip_prefix(':') else {
            return false;
        };
        ("[::1]", port)
    } else if let Some((h, p)) = rest.split_once(':') {
        (h, p)
    } else {
        return false;
    };

    let host_ok = host.eq_ignore_ascii_case("127.0.0.1")
        || host.eq_ignore_ascii_case("localhost")
        || host == "[::1]";
    if !host_ok {
        return false;
    }

    let Ok(port) = port_str.parse::<u16>() else {
        return false;
    };

    port == bound_port || (cfg!(debug_assertions) && port == 5173)
}

// ── WebSocket bridge relay ────────────────────────────────────────────────────
//
// Each WS connection spawns a fresh `alphacode gui --bridge` child and
// relays its stdin/stdout through the socket.

// `tokio_tungstenite::accept_hdr_async` fixes the `Result<Response, ErrorResponse>`
// callback signature, and `ErrorResponse` is `http::Response<Option<String>>`.
#[allow(clippy::result_large_err)]
fn verify_ws_handshake(
    req: &tokio_tungstenite::tungstenite::handshake::server::Request,
    resp: tokio_tungstenite::tungstenite::handshake::server::Response,
    bound_port: u16,
) -> std::result::Result<
    tokio_tungstenite::tungstenite::handshake::server::Response,
    tokio_tungstenite::tungstenite::handshake::server::ErrorResponse,
> {
    use tokio_tungstenite::tungstenite::handshake::server::ErrorResponse;

    if req.uri().path() != "/ws" {
        let mut err = ErrorResponse::new(Some("Not Found".to_string()));
        *err.status_mut() = http::StatusCode::NOT_FOUND;
        return Err(err);
    }
    if let Some(origin_hdr) = req.headers().get(http::header::ORIGIN) {
        let origin_val = origin_hdr.to_str().unwrap_or("");
        if !is_trusted_ws_origin(origin_val, bound_port) {
            eprintln!("Rejected WebSocket connection from untrusted origin: {origin_val}");
            let mut err = ErrorResponse::new(Some("Forbidden origin".to_string()));
            *err.status_mut() = http::StatusCode::FORBIDDEN;
            return Err(err);
        }
    }
    Ok(resp)
}

fn configure_bridge_command(bridge_exe: &Path, no_update: bool, open_browser: bool) -> ProcCommand {
    let mut cmd = ProcCommand::new(bridge_exe);
    cmd.args(["gui", "--bridge"])
        .args(if no_update {
            &["--no-update"][..]
        } else {
            &[][..]
        })
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(if open_browser {
            std::process::Stdio::inherit()
        } else {
            std::process::Stdio::null()
        });

    #[cfg(windows)]
    if !open_browser {
        // Prevent a console window popup in native desktop mode.
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd
}

#[allow(clippy::result_large_err)]
async fn ws_bridge(
    stream: TcpStream,
    exe: Arc<PathBuf>,
    no_update: bool,
    bound_port: u16,
    open_browser: bool,
) -> Result<()> {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

    let ws = tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &Request, resp: Response| -> std::result::Result<Response, ErrorResponse> {
            verify_ws_handshake(req, resp, bound_port)
        },
    )
    .await?;
    let (mut tx, mut rx) = ws.split();

    // Find CLI binary or use current exe
    let mut bridge_exe = exe.as_ref().clone();
    if let Some(parent) = exe.parent() {
        let candidates = [
            parent.join("alphacode.exe"),
            parent.join("AlphaCode.exe"),
            parent.join("alphacode"),
        ];
        for candidate in candidates {
            if candidate.exists() {
                bridge_exe = candidate;
                break;
            }
        }
    }

    let mut cmd = configure_bridge_command(&bridge_exe, no_update, open_browser);

    let mut child = cmd.spawn().context("failed to spawn gui --bridge child")?;

    let child_stdin = child.stdin.take().context("bridge child has no stdin")?;
    let child_stdout = child.stdout.take().context("bridge child has no stdout")?;

    let mut child_in = tokio::io::BufWriter::new(child_stdin);
    let mut child_out = BufReader::new(child_stdout);

    // Relay child stdout → WebSocket (one JSON line at a time)
    let (kill_tx, mut kill_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match child_out.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    if tx
                        .send(Message::Text(line.trim_end().to_owned().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        drop(kill_tx);
    });

    // Relay WebSocket → child stdin
    loop {
        tokio::select! {
            msg = rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        child_in.write_all(text.as_bytes()).await?;
                        child_in.write_all(b"\n").await?;
                        child_in.flush().await?;
                    }
                    Some(Ok(Message::Binary(bin))) => {
                        child_in.write_all(&bin).await?;
                        child_in.write_all(b"\n").await?;
                        child_in.flush().await?;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            _ = &mut kill_rx => break,
        }
    }

    child.kill().await.ok();
    Ok(())
}

// ── Accept loop — peek at the first bytes to route HTTP vs WS ────────────────

fn is_ws_upgrade_preview(header_preview: &str) -> bool {
    let first_line = header_preview.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");
    let is_ws_path = path == "/ws" || path.starts_with("/ws?");
    let has_upgrade = header_preview
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("upgrade: websocket"));
    is_ws_path && has_upgrade
}

async fn accept_loop(
    listener: TcpListener,
    disk_dist: Option<Arc<PathBuf>>,
    exe: Arc<PathBuf>,
    no_update: bool,
    bound_port: u16,
    open_browser: bool,
) {
    loop {
        let Ok((stream, _addr)) = listener.accept().await else {
            continue;
        };

        // Peek to detect WebSocket upgrade on `/ws`
        let mut peek_buf = [0u8; 4096];
        let n = match stream.peek(&mut peek_buf).await {
            Ok(n) => n,
            Err(_) => continue,
        };
        let header_preview = std::str::from_utf8(&peek_buf[..n]).unwrap_or("");

        if is_ws_upgrade_preview(header_preview) {
            let exe2 = Arc::clone(&exe);
            tokio::spawn(async move {
                ws_bridge(stream, exe2, no_update, bound_port, open_browser)
                    .await
                    .ok();
            });
        } else {
            let dist2 = disk_dist.clone();
            tokio::spawn(async move {
                serve_http(stream, dist2).await.ok();
            });
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Find the UI dist directory relative to the running executable or repository.
/// Checks:
///   1. `<exe_dir>/ui/dist`                     (installed alongside binary)
///   2. `<cwd>/src/cli/gui/ui/dist`             (standard repo architecture)
///   3. `<exe_dir>/../../src/cli/gui/ui/dist`   (cargo target/debug dev layout)
///   4. `<cwd>/ui/dist`                         (fallback)
fn find_dist(exe: &Path) -> Option<PathBuf> {
    let candidates: &[fn(&Path) -> PathBuf] = &[
        |e| e.parent().unwrap_or(e).join("ui").join("dist"),
        |_| {
            std::env::current_dir()
                .unwrap_or_default()
                .join("src")
                .join("cli")
                .join("gui")
                .join("ui")
                .join("dist")
        },
        |e| {
            e.parent()
                .and_then(|p| p.parent())
                .unwrap_or(e)
                .join("src")
                .join("cli")
                .join("gui")
                .join("ui")
                .join("dist")
        },
        |_| {
            std::env::current_dir()
                .unwrap_or_default()
                .join("ui")
                .join("dist")
        },
    ];
    for f in candidates {
        let p = f(exe);
        if p.join("index.html").is_file() {
            return Some(p);
        }
    }
    None
}

const PREFERRED_GUI_PORTS: std::ops::RangeInclusive<u16> = 19731..=19741;

async fn bind_gui_listener() -> Result<TcpListener> {
    for port in PREFERRED_GUI_PORTS {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(listener);
        }
    }
    TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind localhost port")
}

pub async fn run(no_update: bool, open_browser: bool) -> Result<()> {
    if !open_browser {
        crate::cli::gui::desktop::hide_console_if_standalone();
    }

    let exe = std::env::current_exe().context("cannot determine executable path")?;
    let disk_dist = find_dist(&exe).map(Arc::new);

    if EMBEDDED_UI_ASSETS_GZIP.is_empty() && disk_dist.is_none() {
        bail!(
            "ui/dist not found — run `npm ci && npm run build` inside `src/cli/gui/ui/` first.\n\
             Searched relative to: {}",
            exe.display()
        );
    }

    // Prefer a stable loopback port so WebView2 keeps localStorage/origin state across restarts.
    let listener = bind_gui_listener().await?;
    let addr: SocketAddr = listener.local_addr()?;
    let bound_port = addr.port();
    let url = format!("http://127.0.0.1:{bound_port}");

    println!("AlphaCode Desktop GUI → {url}");
    if !EMBEDDED_UI_ASSETS_GZIP.is_empty() {
        println!(
            "Serving UI: embedded gzip ({} asset(s), {})",
            EMBEDDED_UI_ASSETS_GZIP.len(),
            embedded_ui_bundle_marker()
        );
    } else if let Some(ref dist) = disk_dist {
        println!("Serving UI: {}", dist.display());
    }

    let exe = Arc::new(exe);

    // Spawn HTTP & WS bridge server in background
    let dist_clone = disk_dist.clone();
    let exe_clone = Arc::clone(&exe);
    tokio::spawn(async move {
        accept_loop(
            listener,
            dist_clone,
            exe_clone,
            no_update,
            bound_port,
            open_browser,
        )
        .await;
    });

    if open_browser {
        println!("Opening Web GUI in default browser...");
        if let Err(e) = open::that(&url) {
            eprintln!("Could not open browser automatically: {e}");
            eprintln!("Open manually: {url}");
        }
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    } else {
        println!("Launching AlphaCode Native Desktop Window...");
        let url_clone = url.clone();
        tokio::task::block_in_place(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::cli::gui::desktop::run_desktop_window(&url_clone)
            }));
            match res {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    eprintln!(
                        "Native window error ({err:#}), trying standalone desktop window mode..."
                    );
                    if !crate::cli::gui::desktop::launch_app_mode_window(&url_clone) {
                        eprintln!("Falling back to system browser...");
                        let _ = open::that(&url_clone);
                    }
                }
                Err(_) => {
                    eprintln!("Native window unexpected fault, launching app mode window...");
                    if !crate::cli::gui::desktop::launch_app_mode_window(&url_clone) {
                        eprintln!("Falling back to system browser...");
                        let _ = open::that(&url_clone);
                    }
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_ws_origin_enforces_exact_loopback_host_and_port() {
        assert!(is_trusted_ws_origin("http://127.0.0.1:43210", 43210));
        assert!(is_trusted_ws_origin("http://localhost:43210", 43210));
        assert!(is_trusted_ws_origin("http://[::1]:43210", 43210));

        // Spoofed subdomains / prefixes must be rejected
        assert!(!is_trusted_ws_origin(
            "http://localhost.evil.com:43210",
            43210
        ));
        assert!(!is_trusted_ws_origin(
            "http://127.0.0.1.evil.com:43210",
            43210
        ));
        assert!(!is_trusted_ws_origin("http://localhost:43210/path", 43210));
        assert!(!is_trusted_ws_origin("http://127.0.0.1:9999", 43210));
        assert!(!is_trusted_ws_origin("https://evil.com", 43210));
        assert!(!is_trusted_ws_origin("null", 43210));
        assert!(!is_trusted_ws_origin("tauri://localhost", 43210));
        assert!(!is_trusted_ws_origin("", 43210));
    }

    #[test]
    fn sanitize_rel_path_blocks_traversal() {
        assert_eq!(sanitize_rel_path("/"), Some("index.html".to_string()));
        assert_eq!(
            sanitize_rel_path("/assets/index.js?v=1"),
            Some("assets/index.js".to_string())
        );
        assert_eq!(sanitize_rel_path("/../secret"), None);
        assert_eq!(sanitize_rel_path("/assets/../../secret"), None);
        assert_eq!(sanitize_rel_path("/assets\\index.js"), None);
        assert_eq!(sanitize_rel_path("/%2e%2e/secret"), None);
        assert_eq!(sanitize_rel_path("/bad\0path"), None);
    }

    #[test]
    fn ws_upgrade_preview_matches_only_ws_endpoint() {
        assert!(is_ws_upgrade_preview(
            "GET /ws HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\r\n"
        ));
        assert!(is_ws_upgrade_preview(
            "GET /ws?token=1 HTTP/1.1\r\nupgrade: websocket\r\n\r\n"
        ));
        assert!(!is_ws_upgrade_preview(
            "GET /index.html HTTP/1.1\r\nUpgrade: websocket\r\n\r\n"
        ));
        assert!(!is_ws_upgrade_preview(
            "GET /ws HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
        ));
    }

    #[test]
    fn configure_bridge_command_wires_args_and_runs_cleanly() {
        let exe = Path::new("alphacode.exe");
        let cmd_web = configure_bridge_command(exe, true, true);
        let args_web: Vec<_> = cmd_web
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args_web, vec!["gui", "--bridge", "--no-update"]);

        let cmd_native = configure_bridge_command(exe, false, false);
        let args_native: Vec<_> = cmd_native
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args_native, vec!["gui", "--bridge"]);
    }

    #[test]
    fn accepts_gzip_parses_request_headers() {
        assert!(accepts_gzip(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Encoding: gzip, deflate, br\r\n\r\n"
        ));
        assert!(accepts_gzip(
            "GET / HTTP/1.1\r\naccept-encoding: br, GZIP;q=0.8\r\n\r\n"
        ));
        assert!(!accepts_gzip(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Encoding: identity\r\n\r\n"
        ));
        assert!(!accepts_gzip("GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"));
    }

    #[test]
    fn decode_gzip_asset_round_trips_and_embedded_bundle_decodes() {
        use std::io::Write;
        let sample = b"<!doctype html><html><body>AlphaCode</body></html>";
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(sample).unwrap();
        let gz = encoder.finish().unwrap();
        assert_eq!(decode_gzip_asset(&gz).as_deref(), Some(sample.as_slice()));

        if !EMBEDDED_UI_ASSETS_GZIP.is_empty() {
            assert_eq!(embedded_ui_bundle_marker(), "ALPHACODE_EMBEDDED_GUI_UI_V1");
            let index_gz = lookup_embedded_asset_gz("index.html").expect("embedded index.html");
            let index_raw = decode_gzip_asset(index_gz).expect("valid gzip index.html");
            let html = String::from_utf8(index_raw).expect("UTF-8 index.html");
            assert!(html.to_ascii_lowercase().contains("<!doctype html>"));
        }
    }
}
