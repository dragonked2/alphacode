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

fn is_placeholder_title(title: &str, id: &str) -> bool {
    let trimmed = title.trim();
    if trimmed.is_empty() || trimmed == id {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "new conversation"
            | "new chat"
            | "alphacode"
            | "alphacode chat"
            | "alphacode companion"
            | "untitled"
            | "__companion__"
    ) {
        return true;
    }
    let hex_part = trimmed.strip_prefix("session_").unwrap_or(trimmed);
    if hex_part.len() == 36
        && hex_part
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == '-')
    {
        return true;
    }
    false
}

fn is_greeting_only(text: &str) -> bool {
    let clean = text
        .trim()
        .trim_matches(|ch: char| ch.is_ascii_punctuation() || matches!(ch, '؟' | '،' | '!' | '.'))
        .to_lowercase();
    matches!(
        clean.as_str(),
        "hi" | "hello"
            | "hey"
            | "yo"
            | "test"
            | "ping"
            | "اهلا"
            | "أهلا"
            | "أهلاً"
            | "هلا"
            | "مرحبا"
            | "مرحباً"
            | "السلام عليكم"
            | "سلام"
            | "ازيك"
            | "ازيكم"
            | "هاي"
            | "صباح الخير"
            | "مساء الخير"
    )
}

pub(crate) fn smart_title_from_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("<system-reminder>")
        || trimmed.starts_with("<session-context>")
    {
        return None;
    }
    let first_line = trimmed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let mut text = first_line.split_whitespace().collect::<Vec<_>>().join(" ");

    for prefix in [
        "من فضلك ",
        "لو سمحت ",
        "ممكن تساعدني في ",
        "ممكن تساعدني ",
        "ممكن ",
        "عاوزك ",
        "عايزك ",
        "عاوز ",
        "عايز ",
        "اريد ",
        "أريد ",
        "please help me ",
        "help me ",
        "can you please ",
        "can you ",
        "could you ",
        "i want you to ",
        "i need to ",
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            if !rest.trim().is_empty() {
                text = rest.trim().to_string();
                break;
            }
        } else {
            let lower = text.to_ascii_lowercase();
            if lower.starts_with(prefix) && text.is_char_boundary(prefix.len()) {
                let rest = text[prefix.len()..].trim();
                if !rest.is_empty() {
                    text = rest.to_string();
                    break;
                }
            }
        }
    }

    let cleaned = text
        .trim_end_matches(['?', '؟', '!', '.', ',', '،', ':', ';'])
        .trim();
    if cleaned.is_empty() {
        return None;
    }

    let max_chars = 46;
    if cleaned.chars().count() <= max_chars {
        return Some(cleaned.to_string());
    }
    let slice: String = cleaned.chars().take(max_chars).collect();
    if let Some((head, _)) = slice.rsplit_once(' ')
        && head.chars().count() >= 14
    {
        return Some(format!("{head}…"));
    }
    Some(format!("{slice}…"))
}

fn derive_smart_title(info: &SessionInfo) -> String {
    if !is_placeholder_title(&info.title, &info.id) {
        return truncate(&info.title, MAX_TITLE_CHARS);
    }
    let user_candidates: Vec<String> = info
        .messages_preview
        .iter()
        .filter(|m| m.role.eq_ignore_ascii_case("user"))
        .filter_map(|m| smart_title_from_text(&m.content))
        .collect();
    if let Some(substantive) = user_candidates.iter().find(|t| !is_greeting_only(t)) {
        return truncate(substantive, MAX_TITLE_CHARS);
    }
    if let Some(first) = user_candidates.into_iter().next() {
        return truncate(&first, MAX_TITLE_CHARS);
    }
    if let Some(first_prompt) = info.first_user_prompt.as_deref()
        && let Some(smart) = smart_title_from_text(first_prompt)
    {
        return truncate(&smart, MAX_TITLE_CHARS);
    }
    if let Some(dir) = info.working_dir.as_deref()
        && let Some(folder) = std::path::Path::new(dir)
            .file_name()
            .and_then(|n| n.to_str())
    {
        return truncate(&format!("{folder} · Chat"), MAX_TITLE_CHARS);
    }
    truncate(&info.title, MAX_TITLE_CHARS)
}

/// Collapse a session into the desktop sidebar's row shape.
///
/// Fields mirror what `gui.ps1` renders: `id`, `title`, `detail`, plus the
/// metadata a desktop workspace needs to label the chat (state, source, model,
/// saved bookmark, working directory, last activity).
fn session_row(info: &SessionInfo) -> Value {
    let title = derive_smart_title(info);
    let label = info
        .save_label
        .as_deref()
        .map(|label| truncate(label, MAX_LABEL_CHARS));
    // This path is also used for workspace actions, so it must remain exact.
    let branch = info.working_dir.clone();
    let preview_line = info.messages_preview.iter().rev().find_map(|msg| {
        let trimmed = msg.content.trim();
        (!trimmed.is_empty()
            && !trimmed.starts_with("<system-reminder>")
            && !trimmed.starts_with("<session-context>"))
        .then(|| {
            truncate(
                &trimmed.split_whitespace().collect::<Vec<_>>().join(" "),
                90,
            )
        })
    });
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
        "preview": preview_line.unwrap_or_else(|| detail.clone()),
        "status": status,
        "saved": info.saved,
        "save_label": label,
        "source": source_name(info.source),
        "working_dir": branch,
        "model": info.model.as_deref().map(|model| truncate(model, MAX_LABEL_CHARS)),
        "is_debug": info.is_debug,
        "message_count": info.message_count,
        "modified": info.last_message_time.timestamp(),
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

    #[test]
    fn smart_title_strips_filler_and_greetings() {
        assert_eq!(
            smart_title_from_text("can you please fix the auth login bug?"),
            Some("fix the auth login bug".to_string())
        );
        assert_eq!(
            smart_title_from_text("ممكن تساعدني في حل مشكلة الجي يو اي؟"),
            Some("حل مشكلة الجي يو اي".to_string())
        );
        assert!(is_greeting_only("اهلا"));
        assert!(is_greeting_only("hi!"));
        assert!(!is_greeting_only("حل مشكلة الشات"));
    }
}
