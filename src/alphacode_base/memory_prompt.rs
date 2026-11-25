//! Bounded, role-aware context for memory retrieval and extraction.
const MEMORY_CONTEXT_MAX_CHARS: usize = 8_000;
const MEMORY_CONTEXT_MAX_MESSAGES: usize = 12;
const MEMORY_CONTEXT_MAX_BLOCK_CHARS: usize = 1_200;
const EXTRACTION_CONTEXT_MAX_MESSAGES: usize = 40;
const EXTRACTION_CONTEXT_MAX_CHARS: usize = 24_000;
const FOCUSED_QUERY_MAX_CHARS: usize = 3_000;

fn truncate_chars(value: &str, max_chars: usize) -> &str {
    value.char_indices().nth(max_chars).map_or(value, |(index, _)| &value[..index])
}

/// Omit runtime reminder blocks, including inline and unterminated reminders.
/// A literal closing marker without an opener is ordinary text.
fn strip_reminders(value: &str, depth: &mut usize) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut result = String::new();
    let mut rest = value;
    while !rest.is_empty() {
        let open = rest.find(OPEN);
        let close = rest.find(CLOSE);
        match (open, close) {
            (Some(i), other) if other.is_none_or(|j| i < j) => {
                if *depth == 0 { result.push_str(&rest[..i]); }
                *depth = depth.saturating_add(1);
                rest = &rest[i + OPEN.len()..];
            }
            (_, Some(i)) => {
                if *depth == 0 { result.push_str(&rest[..i + CLOSE.len()]); }
                else { *depth -= 1; }
                rest = &rest[i + CLOSE.len()..];
            }
            _ => {
                if *depth == 0 { result.push_str(rest); }
                break;
            }
        }
    }
    result
}

/// Conservative defense in depth, not a complete credential detector.
/// Avoid sending obvious credential lines to embedding or judge providers.
fn safe_prose(value: &str) -> String {
    let mut result = String::new();
    let mut private_key = false;
    for line in value.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("-----begin ") && lower.contains("private key-----") {
            private_key = true;
        }
        let sensitive = private_key || [
            "authorization:", "bearer ", "api_key", "api-key", "apikey",
            "access_token", "refresh_token", "password", "secret_key",
            "private_key", "session_token", "cookie:", "set-cookie:",
            "sk-proj-", "ghp_", "github_pat_", "xoxb-", "signpath_api_token",
        ].iter().any(|marker| lower.contains(marker));
        if sensitive { result.push_str("[sensitive line omitted]"); }
        else { result.push_str(line); }
        result.push('\n');
        if lower.contains("-----end ") && lower.contains("private key-----") {
            private_key = false;
        }
    }
    result.trim().to_owned()
}

fn message_prose(message: &crate::message::Message, extraction: bool) -> String {
    use crate::message::ContentBlock;
    let mut out = String::new();
    let mut depth = 0usize;
    // Keep only prose; tool inputs/results may contain credentials, external
    // instructions, or huge logs. A successful tool call is not evidence here.
    for block in &message.content {
        match block {
            ContentBlock::Text { text, .. } => {
                let stripped = strip_reminders(text, &mut depth);
                let safe = safe_prose(&stripped);
                if !safe.is_empty() {
                    if !out.is_empty() { out.push('\n'); }
                    out.push_str(truncate_chars(&safe, MEMORY_CONTEXT_MAX_BLOCK_CHARS));
                }
            }
            ContentBlock::ToolResult { is_error, .. } if extraction && is_error.unwrap_or(false) => {
                if depth == 0 { out.push_str("\n[Tool reported an error; details omitted]"); }
            }
            _ => {}
        }
        if out.chars().count() >= MEMORY_CONTEXT_MAX_CHARS { break; }
    }
    out.trim().to_owned()
}

fn role_name(message: &crate::message::Message) -> &'static str {
    match message.role {
        crate::message::Role::User => "User",
        crate::message::Role::Assistant => "Assistant",
    }
}

fn render_window(messages: &[crate::message::Message], max_messages: usize,
    budget: usize, extraction: bool) -> String {
    let mut chunks = Vec::new();
    let mut remaining = budget;
    for message in messages.iter().rev().take(max_messages) {
        let prose = message_prose(message, extraction);
        if prose.is_empty() { continue; }
        let role = role_name(message);
        let overhead = role.len() + 2 + usize::from(!chunks.is_empty());
        if remaining <= overhead { break; }
        let text = truncate_chars(&prose, remaining - overhead);
        let chunk = format!("{role}:\n{text}");
        remaining -= chunk.chars().count() + usize::from(!chunks.is_empty());
        chunks.push(chunk);
        if remaining == 0 { break; }
    }
    chunks.reverse();
    chunks.join("\n")
}

pub fn format_context_for_relevance(messages: &[crate::message::Message]) -> String {
    render_window(messages, MEMORY_CONTEXT_MAX_MESSAGES, MEMORY_CONTEXT_MAX_CHARS, false)
}

pub fn format_focused_query_for_relevance(messages: &[crate::message::Message]) -> String {
    // Use actual role metadata rather than interpreting role-like text in prose.
    let mut entries: Vec<_> = messages.iter().rev().take(MEMORY_CONTEXT_MAX_MESSAGES)
        .map(|message| (role_name(message), message_prose(message, false)))
        .filter(|(_, text)| !text.is_empty()).collect();
    let latest_user = entries.iter().position(|(role, _)| *role == "User");
    if let Some(index) = latest_user {
        let user = entries.remove(index);
        entries.insert(0, user);
    }
    bounded_query(entries.into_iter().map(|(_, text)| text))
}

fn bounded_query(texts: impl IntoIterator<Item = String>) -> String {
    let mut out = String::new();
    let mut remaining = FOCUSED_QUERY_MAX_CHARS;
    let mut seen = std::collections::HashSet::new();
    for text in texts {
        if text.trim().is_empty() || !seen.insert(text.clone()) { continue; }
        if !out.is_empty() {
            if remaining <= 1 { break; }
            out.push('\n');
            remaining -= 1;
        }
        let piece = truncate_chars(&text, remaining);
        out.push_str(piece);
        remaining -= piece.chars().count();
        if remaining == 0 { break; }
    }
    out
}

/// Compatibility transform for already-formatted role-prefixed context.
/// Structured callers should use format_focused_query_for_relevance instead.
pub fn focus_query_text(raw: &str) -> String {
    let mut depth = 0;
    let stripped = strip_reminders(raw, &mut depth);
    let safe = safe_prose(&stripped);
    let mut entries: Vec<(bool, String)> = Vec::new();
    let mut is_user = false;
    let mut current = String::new();
    for line in safe.lines() {
        let text = line.trim();
        if matches!(text, "User:" | "Assistant:") {
            if !current.is_empty() { entries.push((is_user, std::mem::take(&mut current))); }
            is_user = text == "User:";
            continue;
        }
        if text.is_empty() || ["[Tool:", "[Tool error:", "[Tool result:", "[Result:",
            "[Image]", "[OpenAI native compaction]"].iter().any(|p| text.starts_with(p)) {
            continue;
        }
        if !current.is_empty() { current.push('\n'); }
        current.push_str(text);
    }
    if !current.is_empty() { entries.push((is_user, current)); }
    entries.reverse();
    if let Some(index) = entries.iter().position(|(user, _)| *user) {
        let user = entries.remove(index);
        entries.insert(0, user);
    }
    bounded_query(entries.into_iter().map(|(_, text)| text))
}

pub(crate) fn format_context_for_extraction(messages: &[crate::message::Message]) -> String {
    render_window(messages, EXTRACTION_CONTEXT_MAX_MESSAGES, EXTRACTION_CONTEXT_MAX_CHARS, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focused_query_keeps_whole_latest_user_message_once() {
        let query = focus_query_text("User:\nold\nAssistant:\nanswer\nUser:\nnew question\nsecond line");
        assert!(query.starts_with("new question\nsecond line"));
        assert_eq!(query.matches("new question").count(), 1);
    }
    #[test]
    fn reminders_and_credentials_are_not_restored_by_fallback() {
        assert_eq!(focus_query_text("User:\n<system-reminder>private</system-reminder>"), "");
        let query = focus_query_text("User:\nkeep <system-reminder>hidden</system-reminder> this\nAPI_KEY=abc");
        assert!(!query.contains("hidden"));
        assert!(!query.contains("abc"));
        assert!(query.contains("keep"));
    }
    #[test]
    fn unicode_budget_is_exact() {
        let input = "界".repeat(5_000);
        let query = focus_query_text(&input);
        assert_eq!(query.chars().count(), FOCUSED_QUERY_MAX_CHARS);
    }
}
