//! Desktop transcript projection for the companion GUI.
//!
//! The companion previously showed only the last assistant message. A real
//! desktop workspace needs the conversation: user prompts, assistant
//! responses, and a compact trace of tool activity. This module renders a
//! bounded, presentation-only projection of a live session.
//!
//! It is intentionally read-only and allocation-bounded: the desktop polls
//! roughly once per second while expanded, so every field is truncated and the
//! message count is capped.

use crate::session::Session;
use serde_json::{Value, json};

/// Most recent conversation messages rendered into the desktop transcript.
const MAX_TRANSCRIPT_MESSAGES: usize = 40;
/// Maximum characters per rendered message body.
const MAX_MESSAGE_CHARS: usize = 4_000;
/// Maximum characters for the whole transcript payload.
const MAX_TRANSCRIPT_CHARS: usize = 24_000;
/// Tool-call labels shown per message before the remainder is dropped.
const MAX_TOOL_LABELS: usize = 6;

/// Render a bounded transcript for the desktop workspace.
///
/// Output shape:
/// `{"message_count":4,"messages":[{"role":"user","text":"...","tools":["read"]}]}`
pub(super) fn desktop_transcript(session: &Session) -> String {
    let messages = crate::session::render_messages(session);
    let total = messages.len();
    let start = total.saturating_sub(MAX_TRANSCRIPT_MESSAGES);

    let mut out: Vec<Value> = Vec::with_capacity(total - start);
    let mut budget = MAX_TRANSCRIPT_CHARS;
    for message in messages.into_iter().skip(start) {
        // `render_messages` yields "system" for synthetic UI notices.
        let role = match message.role.as_str() {
            "user" => "user",
            "assistant" => "assistant",
            _ => "system",
        };
        let text = crate::util::truncate_str(&message.content, MAX_MESSAGE_CHARS).to_string();
        budget = budget.saturating_sub(text.len());
        if budget == 0 {
            break;
        }
        let mut tools = message.tool_calls;
        tools.truncate(MAX_TOOL_LABELS);
        out.push(json!({
            "role": role,
            "text": text,
            "tools": tools,
        }));
    }

    json!({
        "message_count": total,
        "messages": out,
    })
    .to_string()
}
