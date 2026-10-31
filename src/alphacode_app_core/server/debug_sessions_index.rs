//! Persisted-session index for the desktop companion.
//!
//! The companion GUI previously listed only sessions that happened to be live
//! on a connected daemon, so its sidebar did not match what the terminal's
//! session picker shows. This module projects the *same* on-disk session store
//! the TUI picker reads (`SessionInfo` from `alphacode-tui-session-picker`) into
//! a compact, presentation-only JSON payload.
//!
//! Design constraints:
//! - Reuse the canonical loader (`load_sessions`) so titles, saved/bookmark
//!   state, source badges, and debug/canary filtering match the CLI exactly.
//!   No second, drifting notion of "a session".
//! - Stay bounded: the desktop polls this about once a second, so the payload
//!   is capped and every string is truncated.
//! - Never block: the loader hits disk, so it runs in `spawn_blocking` and the
//!   caller applies a timeout.

use crate::alphacode_tui_session_picker::{SessionInfo, SessionSource};
use serde_json::{Value, json};

/// Maximum sessions included in one desktop index payload.
const MAX_INDEX_SESSIONS: usize = 200;
/// Maximum characters for a session title.
const MAX_TITLE_CHARS: usize = 120;
/// Maximum characters for a session detail line.
const MAX_DETAIL_CHARS: usize = 200;
/// Maximum characters for a saved/bookmark label.
const MAX_LABEL_CHARS: usize = 60;

fn truncate(value: &str, limit: usize) -> String {
    crate::util::truncate_str(value, limit).to_string()
}

/// Collapse a session into the desktop sidebar's row shape.
///
/// Fields mirror what `gui.ps1` renders: `id`, `title`, `detail`, plus the
/// metadata a desktop workspace needs to label the chat (state, source, model,
/// saved bookmark, working directory, last activity).
fn session_row(info: &SessionInfo) -> Value {
    let title = truncate(&info.title, MAX_TITLE_CHARS);
    let label = info
        .save_label
        .as_deref()
        .map(|label| truncate(label, MAX_LABEL_CHARS));
    // This path is also used for workspace actions, so it must remain exact.
    let branch = info.working_dir.clone();
    let detail = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(badge) = info.source.badge() {
            parts.push(badge.to_string());
        }
        if info.saved {
            parts.push(match &label {
                Some(label) => format!("★ {label}"),
                None => "★ saved".to_string(),
            });
        }
        parts.push(format!("{} message(s)", info.message_count));
        if let Some(model) = info.model.as_deref().filter(|model| !model.is_empty()) {
            // A free-pool member is an internal gateway id; the desktop must see
            // the product label instead, matching the model picker and the TUI.
            let model =
                crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(model)
                    .unwrap_or(model);
            parts.push(truncate(model, MAX_DETAIL_CHARS / 2));
        }
        truncate(&parts.join(" · "), MAX_DETAIL_CHARS)
    };
    let mut status = format!("{:?}", info.status);
    if status.len() > MAX_LABEL_CHARS {
        status = truncate(&status, MAX_LABEL_CHARS);
    }
    json!({
        "id": info.id,
        "title": title,
        "detail": detail,
        "status": status,
        "saved": info.saved,
        "save_label": label,
        "source": source_name(info.source),
        "working_dir": branch,
        "model": info.model.as_deref().map(|model| truncate(model, MAX_LABEL_CHARS)),
        "is_debug": info.is_debug,
        "last_active_at": info.last_active_at.map(|at| at.to_rfc3339()),
        "last_message_time": info.last_message_time.to_rfc3339(),
    })
}

fn source_name(source: SessionSource) -> &'static str {
    match source {
        SessionSource::Alphacode => "alphacode",
        SessionSource::ClaudeCode => "claude-code",
        SessionSource::Codex => "codex",
        SessionSource::Pi => "pi",
        SessionSource::OpenCode => "opencode",
        SessionSource::Cursor => "cursor",
    }
}

/// Build the bounded desktop session index payload.
///
/// Runs the canonical (disk-backed) session loader, so this must be called from
/// a blocking context.
pub(super) fn desktop_session_index() -> String {
    // Drop any cached scan first. The desktop poll interval (15s) is far longer
    // than the picker's 5s cache TTL, so a stale entry would mean the sidebar
    // kept showing a title/archive state the user just changed through the
    // terminal. The scan is already bounded, so refresh it every time.
    crate::alphacode_tui::tui::session_picker::invalidate_session_list_cache();
    let sessions = crate::alphacode_tui::tui::session_picker::load_sessions().unwrap_or_default();
    let total = sessions.len();

    let mut rows: Vec<Value> = Vec::with_capacity(total.min(MAX_INDEX_SESSIONS));
    let mut saved = 0usize;
    for info in sessions.iter().take(MAX_INDEX_SESSIONS) {
        if info.saved {
            saved += 1;
        }
        rows.push(session_row(info));
    }

    json!({
        "ok": true,
        "session_count": total,
        "returned": rows.len(),
        "saved_count": saved,
        "truncated": total > rows.len(),
        "sessions": rows,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names_are_stable() {
        assert_eq!(source_name(SessionSource::Alphacode), "alphacode");
        assert_eq!(source_name(SessionSource::Codex), "codex");
        assert_eq!(source_name(SessionSource::ClaudeCode), "claude-code");
    }

    #[test]
    fn truncate_respects_limit() {
        let long = "x".repeat(500);
        assert!(truncate(&long, MAX_TITLE_CHARS).chars().count() <= MAX_TITLE_CHARS);
    }

    /// The sidebar row's detail line must not name an internal free-pool
    /// gateway id; it carries the same product label the model picker shows.
    #[test]
    fn session_detail_line_uses_the_product_label_for_pool_models() {
        use crate::alphacode_app_core::agent::free_pool_rotation::effective_free_pool;

        let alias = effective_free_pool()
            .into_iter()
            .next()
            .expect("the free pool has a curated floor");

        let now = chrono::Utc::now();
        let info = SessionInfo {
            id: "test".into(),
            parent_id: None,
            short_name: "test".into(),
            icon: String::new(),
            title: "test".into(),
            message_count: 0,
            user_message_count: 0,
            assistant_message_count: 0,
            created_at: now,
            last_message_time: now,
            last_active_at: None,
            working_dir: None,
            model: Some(alias.clone()),
            provider_key: None,
            is_canary: false,
            is_debug: false,
            saved: false,
            save_label: None,
            status: Default::default(),
            needs_catchup: false,
            estimated_tokens: 0,
            first_user_prompt: None,
            messages_preview: Vec::new(),
            search_index: String::new(),
            server_name: None,
            server_icon: None,
            source: SessionSource::Alphacode,
            resume_target: crate::alphacode_session_types::ResumeTarget::AlphacodeSession {
                session_id: "test".into(),
            },
            external_path: None,
        };

        let row = session_row(&info);
        let detail = row
            .get("detail")
            .and_then(Value::as_str)
            .expect("a detail line");

        assert!(
            !detail.contains(&alias),
            "the raw pool id `{alias}` reached the desktop sidebar: {detail}"
        );
        assert!(
            detail.contains("Alphax Free"),
            "the product label should appear instead: {detail}"
        );
    }
}
