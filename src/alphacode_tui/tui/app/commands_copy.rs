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

pub(super) struct ClipboardCopyJob {
    session_id: String,
    result: Receiver<(ClipboardWriteResult, usize)>,
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
    let Some(text) = text.filter(|text| !text.is_empty()) else {
        app.set_status_notice("Nothing in this session to copy");
        return;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    // Native clipboard/helper I/O can block. Keep it off the event/render loop.
    match std::thread::Builder::new()
        .name("session-clipboard".to_string())
        .spawn(move || {
            let lines = text.lines().count();
            let copied = super::helpers::copy_to_clipboard_status(&text);
            let _ = tx.send((copied, lines));
        }) {
        Ok(_) => {
            app.clipboard_copy_job = Some(ClipboardCopyJob {
                session_id,
                result: rx,
            });
            app.set_status_notice("Copying the full session...");
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
                    Ok((ClipboardWriteResult::Copied, lines)) => app.set_status_notice(format!(
                        "Copied entire session to clipboard · {lines} lines"
                    )),
                    Ok((ClipboardWriteResult::Forwarded, lines)) => app.set_status_notice(format!("Full session sent to terminal clipboard · {lines} lines; terminal confirmation unavailable")),
                    _ => app.set_status_notice(
                        "Could not copy the session; clipboard unavailable — run /copy to retry",
                    ),
                }
            }
            true
        }
    }
}
