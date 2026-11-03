//! User-requested full-session clipboard snapshots. Rendering and viewport
//! state never determine which retained messages are exported.
use super::App;
use super::helpers::ClipboardWriteResult;
use crate::alphacode_tui::tui::backend::RemoteConnection;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

pub(super) struct PendingSessionCopy {
    pub request_id: u64,
    pub session_id: String,
    pub started: Instant,
    pub live_text: String,
}

type ClipboardCopyResult = Result<(ClipboardWriteResult, usize), String>;

pub(super) struct ClipboardCopyJob {
    session_id: String,
    result: Receiver<ClipboardCopyResult>,
    saved_snapshot: bool,
}

fn current_session_id(app: &App) -> &str {
    if app.is_remote {
        app.remote_session_id.as_deref().unwrap_or(&app.session.id)
    } else {
        &app.session.id
    }
}

fn is_copy_command(input: &str) -> bool {
    input.split_whitespace().next() == Some("/copy")
}

fn with_live_text(text: Option<String>, live_text: &str) -> Option<String> {
    let live_text = crate::message::strip_ansi_escape_sequences(live_text);
    if live_text.is_empty() {
        return text;
    }
    let mut text = text.unwrap_or_default();
    // A response may have been committed between requesting and receiving the
    // snapshot. Do not append the same live tail a second time.
    if !text.trim_end().ends_with(live_text.trim_end()) {
        text.push_str("\n## Assistant (in-progress snapshot)\n\n");
        text.push_str(&live_text);
        text.push('\n');
    }
    Some(text)
}

fn copy_busy(app: &mut App) -> bool {
    if app.pending_session_copy.is_some() || app.clipboard_copy_job.is_some() {
        app.set_status_notice("Session copy already in progress");
        true
    } else {
        false
    }
}

pub(super) fn handle_copy_command(app: &mut App, input: &str) -> bool {
    if !is_copy_command(input) {
        return false;
    }
    if input != "/copy" {
        app.set_status_notice("Usage: /copy — copy the entire session");
        return true;
    }
    if copy_busy(app) {
        return true;
    }
    if app.is_remote {
        app.set_status_notice(
            "Reconnect to copy the full server session; visible history may be incomplete",
        );
        return true;
    }
    let text = with_live_text(app.session.clipboard_transcript(), app.streaming_text());
    let session_id = app.session.id.clone();
    start_clipboard_copy(app, session_id, text);
    true
}

pub(super) async fn handle_remote_copy_command(
    app: &mut App,
    remote: &mut RemoteConnection,
    input: &str,
) -> bool {
    if !is_copy_command(input) {
        return false;
    }
    if input != "/copy" {
        app.set_status_notice("Usage: /copy — copy the entire session");
        return true;
    }
    if copy_busy(app) {
        return true;
    }
    let Some(session_id) = app.remote_session_id.clone() else {
        app.set_status_notice("Wait for the session to finish connecting, then run /copy");
        return true;
    };
    match remote.request_session_transcript(&session_id).await {
        Ok(request_id) => {
            app.pending_session_copy = Some(PendingSessionCopy {
                request_id,
                session_id,
                started: Instant::now(),
                live_text: app.streaming_text().to_owned(),
            });
            app.set_status_notice("Preparing the full session for clipboard...");
        }
        Err(error) => app.set_status_notice(format!("Could not request session copy: {error}")),
    }
    true
}

pub(super) fn receive_session_transcript(
    app: &mut App,
    id: u64,
    session_id: String,
    text: Option<String>,
) {
    if !app
        .pending_session_copy
        .as_ref()
        .is_some_and(|pending| pending.request_id == id && pending.session_id == session_id)
    {
        return;
    }
    let Some(pending) = app.pending_session_copy.take() else {
        return;
    };
    if current_session_id(app) != session_id {
        return;
    }
    start_clipboard_copy(app, session_id, with_live_text(text, &pending.live_text));
}

fn start_clipboard_copy(app: &mut App, session_id: String, text: Option<String>) {
    start_clipboard_job(app, session_id, text, None);
}

fn unsupported_transcript_request(message: &str) -> bool {
    message.contains("unknown variant `get_session_transcript`")
}

pub(super) fn handle_session_copy_error(app: &mut App, id: u64, message: &str) -> bool {
    let Some(pending) = app.pending_session_copy.as_ref() else {
        return false;
    };
    let legacy = unsupported_transcript_request(message);
    // Older servers report JSON decode failures with id=0 because the request
    // could not be decoded. Do not consume unrelated errors on the connection.
    if id != pending.request_id && !(id == 0 && legacy) {
        return false;
    }
    let pending = app
        .pending_session_copy
        .take()
        .expect("checked pending copy");
    if current_session_id(app) != pending.session_id {
        return true;
    }
    if legacy {
        start_clipboard_job(app, pending.session_id, None, Some(pending.live_text));
    } else {
        app.set_status_notice(format!("Could not copy this session: {message}"));
    }
    true
}

fn start_clipboard_job(
    app: &mut App,
    session_id: String,
    text: Option<String>,
    saved_live: Option<String>,
) {
    if saved_live.is_none() && text.as_ref().is_none_or(|text| text.is_empty()) {
        app.set_status_notice("Nothing in this session to copy");
        return;
    }
    let saved_snapshot = saved_live.is_some();
    let worker_session = session_id.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    // Disk recovery and native clipboard I/O stay off the socket/render loop.
    match std::thread::Builder::new().name("session-clipboard".into()).spawn(move || {
        let result = (|| {
            let text = if let Some(live) = saved_live {
                let session = crate::session::Session::load_for_remote_startup(&worker_session)
                    .map_err(|_| "This older server cannot export a live transcript and no saved session is available locally. Restart the daemon after upgrading, then run /copy.".to_string())?;
                with_live_text(session.clipboard_transcript(), &live)
            } else { text };
            let text = text.filter(|text| !text.is_empty()).ok_or_else(|| "Nothing saved in this session to copy".to_string())?;
            Ok((super::helpers::copy_to_clipboard_status(&text), text.lines().count()))
        })();
        let _ = tx.send(result);
    }) {
        Ok(_) => {
            app.clipboard_copy_job = Some(ClipboardCopyJob { session_id, result: rx, saved_snapshot });
            app.set_status_notice(if saved_snapshot { "Older daemon: preparing the saved session snapshot..." } else { "Copying the full session..." });
        }
        Err(error) => app.set_status_notice(format!("Could not start session copy: {error}")),
    }
}

pub(super) fn poll_session_copy(app: &mut App) -> bool {
    let expired = app
        .pending_session_copy
        .as_ref()
        .is_some_and(|pending| pending.started.elapsed() >= Duration::from_secs(30));
    if expired {
        app.pending_session_copy = None;
        app.set_status_notice("Session copy timed out; check the connection and run /copy again");
        return true;
    }
    let result = app
        .clipboard_copy_job
        .as_ref()
        .map(|job| job.result.try_recv());
    let Some(result) = result else {
        return false;
    };
    match result {
        Err(TryRecvError::Empty) => false,
        result => {
            let Some(job) = app.clipboard_copy_job.take() else {
                return false;
            };
            if current_session_id(app) == job.session_id {
                match result {
                    Ok(Ok((ClipboardWriteResult::Copied | ClipboardWriteResult::Forwarded, lines))) if job.saved_snapshot => app.set_status_notice(format!("Copied saved session snapshot · {lines} lines; restart the older daemon for live full export")),
                    Ok(Err(error)) => app.set_status_notice(error),
                    Ok(Ok((ClipboardWriteResult::Copied, lines))) => app.set_status_notice(format!(
                        "Copied entire session to clipboard · {lines} lines"
                    )),
                    Ok(Ok((ClipboardWriteResult::Forwarded, lines))) => app.set_status_notice(format!("Full session sent to terminal clipboard · {lines} lines; terminal confirmation unavailable")),
                    _ => app.set_status_notice(
                        "Could not copy the session; clipboard unavailable — run /copy to retry",
                    ),
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_legacy_export_variant_triggers_saved_snapshot_recovery() {
        assert!(super::unsupported_transcript_request(
            "Invalid request: unknown variant `get_session_transcript`, expected `message`"
        ));
        assert!(!super::unsupported_transcript_request(
            "Session changed; run /copy again"
        ));
        assert!(!super::unsupported_transcript_request(
            "unknown variant `message`"
        ));
    }
}
