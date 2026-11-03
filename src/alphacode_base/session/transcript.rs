//! Plain Markdown export of the original retained conversation, independent
//! of viewport clipping, clear-view state, tool folding and context compaction.
use super::{Session, is_internal_system_reminder_message};
use crate::message::{ContentBlock, Role};

fn section(out: &mut String, label: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    out.push_str("## ");
    out.push_str(label);
    out.push_str("\n\n");
    out.push_str(text);
    if !text.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
}

impl Session {
    /// Export all retained user/assistant text and tool exchanges. Internal
    /// reminders and opaque provider replay state are not conversation text;
    /// image bytes are represented by attachment labels rather than base64.
    pub fn clipboard_transcript(&self) -> Option<String> {
        let mut body = String::new();
        for message in &self.messages {
            if is_internal_system_reminder_message(message) {
                continue;
            }
            let role = match message.display_role {
                Some(super::StoredDisplayRole::System) => "System",
                Some(super::StoredDisplayRole::BackgroundTask) => "Background task",
                None => match message.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                },
            };
            for block in &message.content {
                match block {
                    ContentBlock::Text { text, .. } => section(&mut body, role, text),
                    ContentBlock::Reasoning { text } | ContentBlock::ReasoningTrace { text } => {
                        section(&mut body, "Reasoning (recorded)", text)
                    }
                    ContentBlock::AnthropicThinking { thinking, .. } => {
                        section(&mut body, "Reasoning (recorded)", thinking)
                    }
                    ContentBlock::OpenAIReasoning { summary, .. } => {
                        for text in summary {
                            section(&mut body, "Reasoning summary", text);
                        }
                    }
                    ContentBlock::ToolUse {
                        id, name, input, ..
                    } => {
                        let arguments = serde_json::to_string_pretty(input)
                            .unwrap_or_else(|_| input.to_string());
                        section(&mut body, &format!("Tool call: {name} ({id})"), &arguments);
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        let state = if *is_error == Some(true) {
                            "error"
                        } else {
                            "result"
                        };
                        section(&mut body, &format!("Tool {state}: {tool_use_id}"), content);
                    }
                    ContentBlock::Image { media_type, .. } => section(
                        &mut body,
                        "Attachment",
                        &format!("[Image: {media_type}; binary content omitted]"),
                    ),
                    ContentBlock::OpenAICompaction { .. } => {}
                }
            }
        }
        if body.is_empty() {
            return None;
        }
        let title = self.display_title_or_name();
        let text = format!("# {title}\n\nSession: {}\n\n{body}", self.id);
        Some(crate::message::strip_ansi_escape_sequences(&text))
    }
}
