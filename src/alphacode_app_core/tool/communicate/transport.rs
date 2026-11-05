use crate::alphacode_app_core::protocol::{Request, ServerEvent};
use anyhow::Result;
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
struct EventHeader<'a> {
    #[serde(rename = "type", default, borrow)]
    kind: Cow<'a, str>,
    #[serde(default)]
    id: Option<u64>,
}
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn request_type_from_json(json: &str) -> String {
    serde_json::from_str::<EventHeader<'_>>(json)
        .map(|header| header.kind.into_owned())
        .unwrap_or_else(|_| "unknown".to_string())
}

pub(super) async fn send_request(request: Request) -> Result<ServerEvent> {
    send_request_with_timeout(request, None).await
}

pub(super) async fn send_request_with_timeout(
    request: Request,
    timeout: Option<std::time::Duration>,
) -> Result<ServerEvent> {
    let path = crate::alphacode_app_core::server::socket_path();
    let stream = crate::alphacode_app_core::server::connect_socket(&path).await?;
    let (reader, mut writer) = stream.into_split();

    let request_id = request.id();
    let deadline =
        tokio::time::Instant::now() + timeout.unwrap_or(std::time::Duration::from_secs(30));

    let json = serde_json::to_string(&request)? + "\n";
    let request_type = request_type_from_json(&json);
    writer.write_all(json.as_bytes()).await?;

    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Read lines until we find the terminal response for our request ID.
    // Skip: ack events, notification events, swarm_status broadcasts, etc.
    // Terminal events: done, error, comm_spawn_response, comm_await_members_response,
    //                  and any other typed response with matching id.
    loop {
        line.clear();
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            crate::logging::warn(&format!(
                "[tool:communicate] request timed out type={} id={} socket={} after waiting for response",
                request_type,
                request_id,
                path.display()
            ));
            anyhow::bail!("Timed out waiting for response")
        }
        let n = tokio::time::timeout(remaining, reader.read_line(&mut line)).await??;
        if n == 0 {
            crate::logging::warn(&format!(
                "[tool:communicate] connection closed before response type={} id={} socket={}",
                request_type,
                request_id,
                path.display()
            ));
            return Err(anyhow::anyhow!(
                "Connection closed before receiving response"
            ));
        }

        // Borrow only routing fields. Broadcasts can contain large transcripts or
        // swarm snapshots; constructing a Value tree for discarded events wastes
        // allocations on every coordination request.
        let header: EventHeader<'_> = serde_json::from_str(line.trim()).map_err(|err| {
            crate::logging::warn(&format!(
                "[tool:communicate] failed to parse response type={} id={} socket={} error={} payload={}",
                request_type,
                request_id,
                path.display(),
                err,
                crate::util::truncate_str(line.trim(), 240)
            ));
            err
        })?;

        let event_type = header.kind.as_ref();
        let event_id = header.id;

        if event_type != "ack" && event_id != Some(request_id) {
            continue;
        }

        match event_type {
            // Skip ack — not a response
            "ack" => continue,
            // Skip broadcast/async events that are not tied to our request
            "swarm_status"
            | "swarm_plan"
            | "swarm_plan_proposal"
            | "swarm_event"
            | "notification"
            | "soft_interrupt_injected"
            | "session"
            | "session_id"
            | "history"
            | "mcp_status"
            | "memory_injected"
            | "compaction"
            | "connection_type"
            | "connection_phase"
            | "status_detail"
            | "upstream_provider"
            | "reloading"
            | "reload_progress"
            | "available_models_updated"
            | "side_panel_state"
            | "transcript"
            | "interrupted" => continue,
            // Terminal responses and typed request responses with matching ids.
            _ => return Ok(serde_json::from_str(line.trim())?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_header_ignores_broadcast_payload_and_accepts_escaped_types() {
        let header: EventHeader<'_> =
            serde_json::from_str(r#"{"type":"swarm_status","members":[{"nested":{"id":42}}]}"#)
                .unwrap();
        assert_eq!(header.kind, "swarm_status");
        assert_eq!(header.id, None);
        assert!(matches!(header.kind, Cow::Borrowed(_)));
        let header: EventHeader<'_> =
            serde_json::from_str(r#"{"type":"do\u006ee","id":7,"message":"ok"}"#).unwrap();
        assert_eq!(header.kind, "done");
        assert_eq!(header.id, Some(7));
    }
}
