//! Harness API bridge: exposes the stable versioned harness API on its own
//! Unix socket and translates to the internal (legacy) alphacode protocol.
//!
//! Architecture (milestone 2 of docs/HARNESS_API_AND_DESKTOP_REWRITE.md):
//! - Listens on `~/.alphacode/alphacode-api.sock` (or `ALPHACODE_API_SOCKET`).
//! - For each API client, dials the legacy daemon socket (`ALPHACODE_SOCKET` or
//!   `~/.alphacode/alphacode.sock`) and speaks `subscribe`/`message`/... on its
//!   behalf.
//! - Translation is JSON-to-JSON so this crate does not depend on the heavy
//!   internal protocol types and cannot be broken by additive internal
//!   changes.
//!
//! This keeps the daemon untouched while the API surface stabilizes. Once
//! proven, the same translation can move in-process behind a `hello` sniff on
//! the main socket.

pub mod background_progress;
pub mod translate;

use crate::alphacode_harness_api::{API_VERSION_MAJOR, ApiEvent, ErrorCode, ServerFrame};
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

// Socket paths live in `alphacode-harness-api` so clients and the bridge can never
// resolve different directories (they once did, and the desktop app could not
// connect as a result).
pub use crate::alphacode_harness_api::{api_socket_path, legacy_socket_path};

/// Maximum number of retry attempts when connecting to the legacy daemon.
const MAX_CONNECT_RETRIES: u32 = 3;

/// Delay between connection retry attempts.
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Run the bridge accept loop forever.
pub async fn run_bridge(api_socket: PathBuf, legacy_socket: PathBuf) -> Result<()> {
    let _ = std::fs::remove_file(&api_socket);
    if let Some(parent) = api_socket.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let listener = UnixListener::bind(&api_socket)
        .with_context(|| format!("bind API socket {}", api_socket.display()))?;
    eprintln!(
        "harness API bridge: listening on {} -> {}",
        api_socket.display(),
        legacy_socket.display()
    );
    loop {
        let (stream, _) = listener.accept().await?;
        let legacy = legacy_socket.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_api_client(stream, legacy).await {
                eprintln!("harness API bridge: client ended: {error:#}");
            }
        });
    }
}

/// Connect to the legacy daemon with retry logic for transient failures.
async fn connect_legacy_with_retry(legacy_socket: &PathBuf) -> Result<UnixStream> {
    let mut last_err = None;

    for attempt in 0..MAX_CONNECT_RETRIES {
        match UnixStream::connect(legacy_socket).await {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                last_err = Some(e);
                if attempt < MAX_CONNECT_RETRIES - 1 {
                    tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::other("connect failed without an error")))
        .context(format!(
            "connect legacy socket {} after {} retries",
            legacy_socket.display(),
            MAX_CONNECT_RETRIES
        ))
}

async fn handle_api_client(stream: UnixStream, legacy_socket: PathBuf) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).lines();

    // 1. Handshake: first frame must be hello with a compatible version.
    let Some(line) = reader.next_line().await? else {
        return Ok(());
    };
    let hello: Value = serde_json::from_str(line.trim()).context("parse hello")?;
    let reply_to = hello["id"].as_u64().unwrap_or(0);
    let compatible = hello["req"] == "hello"
        && hello["min_version"].as_u64().unwrap_or(0) <= u64::from(API_VERSION_MAJOR)
        && hello["max_version"].as_u64().unwrap_or(0) >= u64::from(API_VERSION_MAJOR);
    if !compatible {
        let frame = ServerFrame::reply(
            reply_to,
            ApiEvent::Error {
                code: ErrorCode::UnsupportedVersion,
                message: format!("bridge speaks API v{API_VERSION_MAJOR}; send hello first"),
            },
        );
        write_json_line(&mut write_half, &frame).await?;
        return Ok(());
    }
    let hello_ok = ServerFrame::reply(
        reply_to,
        ApiEvent::HelloOk {
            version: API_VERSION_MAJOR,
            server: format!("alphacode-harness-api-bridge/{}", env!("CARGO_PKG_VERSION")),
            capabilities: vec!["sessions".into(), "streaming".into()],
        },
    );
    write_json_line(&mut write_half, &hello_ok).await?;

    // 2. Dial the legacy daemon for this client with retry logic.
    let legacy = connect_legacy_with_retry(&legacy_socket).await?;
    let (legacy_read, mut legacy_write) = legacy.into_split();
    let mut legacy_reader = BufReader::new(legacy_read).lines();

    let mut state = translate::BridgeState::default();

    // 3. Pump both directions in one select loop so translation state stays
    //    single-threaded. `Lines::next_line` retains partial frames when the
    //    opposite direction wins the select.
    loop {
        tokio::select! {
            line = reader.next_line() => {
                let Some(api_line) = line? else { return Ok(()); };
                if api_line.trim().is_empty() { continue; }
                let request: Value = match serde_json::from_str(api_line.trim()) {
                    Ok(value) => value,
                    Err(error) => {
                        let frame = ServerFrame::event(ApiEvent::Error {
                            code: ErrorCode::InvalidRequest,
                            message: error.to_string(),
                        });
                        write_json_line(&mut write_half, &frame).await?;
                        continue;
                    }
                };
                for out in state.api_request_to_legacy(&request) {
                    match out {
                        translate::Outbound::Legacy(value) => {
                            write_json_line(&mut legacy_write, &value).await?;
                        }
                        translate::Outbound::Reply(frame) => {
                            write_json_line(&mut write_half, &frame).await?;
                        }
                    }
                }
            }
            line = legacy_reader.next_line() => {
                let Some(legacy_line) = line? else {
                    let frame = ServerFrame::event(ApiEvent::Error {
                        code: ErrorCode::Internal,
                        message: "daemon connection closed".into(),
                    });
                    write_json_line(&mut write_half, &frame).await?;
                    return Ok(());
                };
                if legacy_line.trim().is_empty() { continue; }
                let event: Value = match serde_json::from_str(legacy_line.trim()) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                for frame in state.legacy_event_to_api(&event) {
                    write_json_line(&mut write_half, &frame).await?;
                }
            }
        }
    }
}

async fn write_json_line<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: ?Sized + serde::Serialize,
{
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn bridge_preserves_partial_frames_in_both_directions() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("daemon.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let (client, bridge) = UnixStream::pair().unwrap();
            let bridge = tokio::spawn(handle_api_client(bridge, path));
            let (client_read, mut client_write) = client.into_split();
            let mut client_lines = BufReader::new(client_read).lines();
            write_json_line(
                &mut client_write,
                &json!({
                    "v": API_VERSION_MAJOR, "id": 1, "req": "hello",
                    "min_version": API_VERSION_MAJOR, "max_version": API_VERSION_MAJOR
                }),
            )
            .await
            .unwrap();
            let hello: ServerFrame =
                serde_json::from_str(&client_lines.next_line().await.unwrap().unwrap()).unwrap();
            assert!(matches!(hello.event, ApiEvent::HelloOk { .. }));

            let (daemon, _) = listener.accept().await.unwrap();
            let (daemon_read, mut daemon_write) = daemon.into_split();
            let mut daemon_lines = BufReader::new(daemon_read).lines();

            client_write.write_all(b"{\"v\":1,\"id\":2,").await.unwrap();
            tokio::task::yield_now().await;
            write_json_line(
                &mut daemon_write,
                &json!({"type": "text_delta", "text": "ready"}),
            )
            .await
            .unwrap();
            let event: ServerFrame =
                serde_json::from_str(&client_lines.next_line().await.unwrap().unwrap()).unwrap();
            assert!(matches!(event.event, ApiEvent::TextDelta { text, .. } if text == "ready"));
            client_write
                .write_all(b"\"req\":\"ping\"}\n")
                .await
                .unwrap();
            let ping: Value =
                serde_json::from_str(&daemon_lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(ping["type"], "ping");

            daemon_write
                .write_all(b"{\"type\":\"pong\",")
                .await
                .unwrap();
            tokio::task::yield_now().await;
            write_json_line(&mut client_write, &json!({"v": 1, "id": 3, "req": "ping"}))
                .await
                .unwrap();
            let second_ping: Value =
                serde_json::from_str(&daemon_lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(second_ping["type"], "ping");
            daemon_write
                .write_all(format!("\"id\":{}}}\n", ping["id"]).as_bytes())
                .await
                .unwrap();
            let pong: ServerFrame =
                serde_json::from_str(&client_lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(pong, ServerFrame::reply(2, ApiEvent::Pong));

            client_write.shutdown().await.unwrap();
            bridge.await.unwrap().unwrap();
        })
        .await
        .expect("bridge round trip timed out");
    }
}
