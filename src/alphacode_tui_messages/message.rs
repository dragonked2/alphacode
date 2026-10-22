use crate::alphacode_message_types::ToolCall;
use crate::alphacode_session_types::RenderedMessage;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A message in the conversation for TUI display.
#[derive(Clone, Debug)]
pub struct DisplayMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub duration_secs: Option<f32>,
    pub title: Option<String>,
    /// Full tool call data for role="tool" messages.
    pub tool_data: Option<ToolCall>,
}

impl DisplayMessage {
    /// Create an error message.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            role: "error".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a background task completion message (dedicated card display).
    pub fn background_task(content: impl Into<String>) -> Self {
        Self {
            role: "background_task".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a display-only usage card. This is shown in the transcript UI but
    /// is not part of provider/model context.
    pub fn usage(content: impl Into<String>) -> Self {
        Self {
            role: "usage".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some("Usage".to_string()),
            tool_data: None,
        }
    }

    /// Create a display-only overnight progress card. This is shown in the
    /// transcript UI but is not part of provider/model context.
    pub fn overnight(content: impl Into<String>) -> Self {
        Self {
            role: "overnight".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some("Overnight".to_string()),
            tool_data: None,
        }
    }

    /// Create a display-only inline todo-list card. The content is either the
    /// legacy JSON array of todo items or an object containing `todos` and
    /// goal-level assessments in `goals`. Shown in the transcript UI but not
    /// part of provider/model context.
    pub fn todos(content: impl Into<String>) -> Self {
        Self {
            role: "todos".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some("Todos".to_string()),
            tool_data: None,
        }
    }

    /// Create a memory injection message (bordered box display).
    pub fn memory(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "memory".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some(title.into()),
            tool_data: None,
        }
    }

    /// Create a swarm notification message (DM/channel/broadcast/shared context).
    pub fn swarm(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "swarm".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some(title.into()),
            tool_data: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create an assistant message with duration.
    pub fn assistant_with_duration(content: impl Into<String>, duration_secs: f32) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: Some(duration_secs),
            title: None,
            tool_data: None,
        }
    }

    /// Create a tool message.
    pub fn tool(content: impl Into<String>, tool_data: ToolCall) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: Some(tool_data),
        }
    }

    /// Create a tool transcript message when the caller only has rendered text.
    pub fn tool_text(content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a display-only metadata transcript message.
    pub fn meta(content: impl Into<String>) -> Self {
        Self {
            role: "meta".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a display-only collapsing reasoning trace ("current" mode). The
    /// content is sentinel-wrapped dim/italic markup; this message height-collapses
    /// toward a one-line summary and is excluded from provider/model context.
    pub fn reasoning(content: impl Into<String>) -> Self {
        Self {
            role: "reasoning".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Convert the shared session renderer output into the TUI transcript model.
    pub fn from_rendered_message(item: RenderedMessage) -> Self {
        Self {
            role: item.role,
            content: item.content,
            tool_calls: item.tool_calls,
            duration_secs: None,
            title: None,
            tool_data: item.tool_data,
        }
    }

    /// Create a tool message with title.
    pub fn tool_with_title(
        content: impl Into<String>,
        tool_data: ToolCall,
        title: impl Into<String>,
    ) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some(title.into()),
            tool_data: Some(tool_data),
        }
    }

    /// Add tool calls to message (builder pattern).
    pub fn with_tool_calls(mut self, tool_calls: Vec<String>) -> Self {
        self.tool_calls = tool_calls;
        self
    }

    /// Add title to message (builder pattern).
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Stable cache key for this message.
    ///
    /// This walks the full message body and the whole tool-input JSON tree, so
    /// it is O(payload). The message cache consults it on **every** lookup,
    /// including hits, so the key cost can dominate the render it is meant to
    /// avoid. Prefer [`DisplayMessage::stable_cache_hash_memoized`], which
    /// reuses a previously computed value when the message has not changed.
    pub fn stable_cache_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.role.hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.tool_calls.hash(&mut hasher);
        self.title.hash(&mut hasher);
        if let Some(tool) = &self.tool_data {
            tool.id.hash(&mut hasher);
            tool.name.hash(&mut hasher);
            hash_json_value(&tool.input, &mut hasher);
        }
        hasher.finish()
    }

    /// Cheap revalidation token for the memoized cache key.
    ///
    /// Address + length detects every realistic mutation: replacing a `String`
    /// reallocates or moves the buffer (different address), and editing it in
    /// place changes the length. The only undetectable case is an in-place
    /// same-length overwrite, which is documented on
    /// [`DisplayMessage::stable_cache_hash_memoized`].
    pub fn cache_shape(&self) -> MessageShape {
        MessageShape {
            role_ptr: self.role.as_ptr() as usize,
            role_len: self.role.len(),
            content_ptr: self.content.as_ptr() as usize,
            content_len: self.content.len(),
            content_fingerprint: content_fingerprint(self.content.as_bytes()),
            tool_calls_ptr: self.tool_calls.as_ptr() as usize,
            tool_calls_len: self.tool_calls.len(),
            title_ptr: self
                .title
                .as_ref()
                .map(|t| t.as_ptr() as usize)
                .unwrap_or(0),
            title_len: self.title.as_ref().map(|t| t.len()).unwrap_or(0),
            // `serde_json::Value` has no stable address, so the tool input is
            // identified by the tool id/name plus the size of its serialized
            // form. A changed tool input changes one of those.
            tool_input_ptr: self
                .tool_data
                .as_ref()
                .map(tool_input_identity)
                .unwrap_or(0),
            tool_input_present: self.tool_data.is_some(),
        }
    }

    /// Stable cache key, memoized in the caller-supplied slot.
    ///
    /// The memo is validated against a cheap *shape token* (buffer addresses
    /// and lengths) rather than trusted blindly: `DisplayMessage` exposes
    /// public fields, so a caller can mutate `content`/`title`/... in place.
    /// When the shape token changes, the memo is discarded and the hash is
    /// recomputed.
    ///
    /// A mutation that keeps every buffer address *and* length identical
    /// (overwriting a string in place with same-length bytes) is the one case
    /// the token cannot see. The message cache is a render cache, so a stale
    /// hit there costs a re-render of identical-looking text, not incorrect
    /// transcript content.
    pub fn stable_cache_hash_memoized(&self, memo: &mut MessageHashMemo) -> u64 {
        let shape = self.cache_shape();
        if let Some(value) = memo.get(&shape) {
            return value;
        }
        let value = self.stable_cache_hash();
        memo.set(&shape, value);
        value
    }
}

/// Cheap identity of the buffers that feed `stable_cache_hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageShape {
    pub(crate) role_ptr: usize,
    pub(crate) role_len: usize,
    pub(crate) content_ptr: usize,
    pub(crate) content_len: usize,
    /// O(1) fingerprint of the content bytes.
    ///
    /// Addresses are reused by the allocator, so they alone cannot identify a
    /// message across lifetimes. The fingerprint makes an accidental
    /// address-and-length collision between two *different* messages
    /// vanishingly unlikely.
    pub(crate) content_fingerprint: u64,
    pub(crate) tool_calls_ptr: usize,
    pub(crate) tool_calls_len: usize,
    pub(crate) title_ptr: usize,
    pub(crate) title_len: usize,
    pub(crate) tool_input_ptr: usize,
    pub(crate) tool_input_present: bool,
}

/// Single-slot memo for a message cache key.
///
/// The caller owns the memo, so it lives exactly as long as the render loop
/// that reuses it — no global side table, no eviction policy, and no leak when
/// a message is dropped.
#[derive(Debug, Default)]
pub struct MessageHashMemo {
    /// Shape token of the memoized value; `None` when the slot is empty.
    shape: Option<MessageShape>,
    /// The memoized hash value.
    value: u64,
}

impl MessageHashMemo {
    /// Read the memoized value if it was computed for the given shape token.
    pub fn get(&self, shape: &MessageShape) -> Option<u64> {
        if self.shape.as_ref() == Some(shape) {
            Some(self.value)
        } else {
            None
        }
    }

    /// Store the hash value for the given shape token.
    pub fn set(&mut self, shape: &MessageShape, value: u64) {
        self.shape = Some(*shape);
        self.value = value;
    }
}

pub fn display_messages_from_rendered_messages(
    messages: impl IntoIterator<Item = RenderedMessage>,
) -> Vec<DisplayMessage> {
    messages
        .into_iter()
        .map(DisplayMessage::from_rendered_message)
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptPreviewLabels<'a> {
    pub user: &'a str,
    pub assistant: &'a str,
    pub system: &'a str,
    pub tool: &'a str,
    pub background_task: &'a str,
    pub meta: &'a str,
}

impl TranscriptPreviewLabels<'static> {
    pub const DESKTOP: Self = Self {
        user: "user",
        assistant: "asst",
        system: "sys",
        tool: "tool",
        background_task: "task",
        meta: "meta",
    };
}

impl<'a> TranscriptPreviewLabels<'a> {
    fn label_for_role(self, role: &str) -> Option<&'a str> {
        match role {
            "user" => Some(self.user),
            "assistant" => Some(self.assistant),
            "system" => Some(self.system),
            "tool" => Some(self.tool),
            "background_task" => Some(self.background_task),
            "meta" => Some(self.meta),
            _ => None,
        }
    }
}

pub fn transcript_preview_line(
    role: &str,
    content: &str,
    char_limit: usize,
    labels: TranscriptPreviewLabels<'_>,
) -> Option<String> {
    let role = labels.label_for_role(role)?;
    let text = normalize_transcript_preview_text(content);
    if text.is_empty() {
        None
    } else {
        Some(format!(
            "{role} {}",
            truncate_transcript_preview(&text, char_limit)
        ))
    }
}

pub fn transcript_preview_lines<'a>(
    messages: impl DoubleEndedIterator<Item = (&'a str, &'a str)>,
    limit: usize,
    char_limit: usize,
    labels: TranscriptPreviewLabels<'_>,
) -> Vec<String> {
    let mut previews = messages
        .rev()
        .filter_map(|(role, content)| transcript_preview_line(role, content, char_limit, labels))
        .take(limit)
        .collect::<Vec<_>>();
    previews.reverse();
    previews
}

pub fn latest_user_transcript_preview<'a>(
    messages: impl DoubleEndedIterator<Item = (&'a str, &'a str)>,
    char_limit: usize,
) -> Option<String> {
    messages.rev().find_map(|(role, content)| {
        (role == "user").then(|| {
            let text = normalize_transcript_preview_text(content);
            (!text.is_empty()).then(|| truncate_transcript_preview(&text, char_limit))
        })?
    })
}

pub fn normalize_transcript_preview_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate_transcript_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() > max_chars {
        format!("{}…", text.chars().take(max_chars).collect::<String>())
    } else {
        text.to_string()
    }
}

/// Cheap content fingerprint used to make the cache-key memo collision-safe.
///
/// The memo is keyed on buffer addresses and lengths. Addresses alone are not
/// a safe identity: when a message is dropped, the allocator routinely hands
/// the very same address to the next message of the same length, which would
/// let an unrelated message inherit the previous message's memoized hash (and
/// therefore its cached render).
///
/// Mixing a content fingerprint into the key closes that hole: two different
/// messages would have to share their length *and* their content bytes to
/// collide.
///
/// Short buffers are fingerprinted **exactly** — hashing at most
/// [`FULL_FINGERPRINT_LIMIT`] bytes is cheap and removes the sampling
/// caveat entirely for the message sizes that dominate a transcript. Longer
/// buffers fall back to sampling three windows, which keeps the cost O(1)
/// while still catching every realistic edit (an append always changes the
/// length and the tail).
pub const FULL_FINGERPRINT_LIMIT: usize = 512;

/// Fingerprint of a byte buffer. See [`FULL_FINGERPRINT_LIMIT`].
pub fn content_fingerprint(bytes: &[u8]) -> u64 {
    const WINDOW: usize = 32;
    let mut hasher = DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    if bytes.is_empty() {
        return hasher.finish();
    }
    if bytes.len() <= FULL_FINGERPRINT_LIMIT {
        bytes.hash(&mut hasher);
        return hasher.finish();
    }
    let starts = [0usize, bytes.len() / 2, bytes.len().saturating_sub(WINDOW)];
    for start in starts {
        let start = start.min(bytes.len());
        let end = (start + WINDOW).min(bytes.len());
        bytes[start..end].hash(&mut hasher);
        (end - start).hash(&mut hasher);
    }
    hasher.finish()
}

/// Cheap identity token for a tool call's input.
///
/// The tool-input JSON tree is the most expensive part of the cache key, so
/// the memo needs a cheap way to notice that it changed. `serde_json::Value`
/// has no stable address, so the token mixes the tool id, the tool name, and
/// the size of the serialized input. A different input produces a different
/// token; an input that changed without changing its serialized size is the
/// same case the memo cannot see for message bodies.
fn tool_input_identity(tool: &ToolCall) -> usize {
    use std::hash::Hash;
    let mut hasher = DefaultHasher::new();
    tool.id.hash(&mut hasher);
    tool.name.hash(&mut hasher);
    json_value_identity(&tool.input, &mut hasher);
    hasher.finish() as usize
}

/// Fold the *addresses* of a JSON value's heap buffers into a hasher.
///
/// `serde_json::Value` is stored inline, so it has no single stable address
/// to compare. Its heap-allocated parts do: a `String`'s buffer, an array's
/// `Vec` buffer, and the nodes of an object's map. Replacing a subtree
/// allocates new buffers, so the folded address changes.
///
/// This walks the tree but never reads the *contents* of a string, so it is
/// far cheaper than the full [`hash_json_value`] for a large tool input while
/// still catching the realistic mutation: swapping the whole input for a
/// different one.
fn json_value_identity(value: &Value, hasher: &mut DefaultHasher) {
    use std::hash::Hash;
    match value {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(b) => {
            1u8.hash(hasher);
            b.hash(hasher);
        }
        Value::Number(n) => {
            2u8.hash(hasher);
            n.hash(hasher);
        }
        Value::String(s) => {
            3u8.hash(hasher);
            (s.as_ptr() as usize).hash(hasher);
        }
        Value::Array(arr) => {
            4u8.hash(hasher);
            (arr.as_ptr() as usize).hash(hasher);
            arr.len().hash(hasher);
            for item in arr {
                json_value_identity(item, hasher);
            }
        }
        Value::Object(map) => {
            5u8.hash(hasher);
            map.len().hash(hasher);
            for (k, v) in map {
                (k.as_ptr() as usize).hash(hasher);
                json_value_identity(v, hasher);
            }
        }
    }
}

fn hash_json_value(value: &Value, hasher: &mut DefaultHasher) {
    match value {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(b) => {
            1u8.hash(hasher);
            b.hash(hasher);
        }
        Value::Number(n) => {
            2u8.hash(hasher);
            n.hash(hasher);
        }
        Value::String(s) => {
            3u8.hash(hasher);
            s.hash(hasher);
        }
        Value::Array(arr) => {
            4u8.hash(hasher);
            arr.len().hash(hasher);
            for item in arr {
                hash_json_value(item, hasher);
            }
        }
        Value::Object(map) => {
            5u8.hash(hasher);
            map.len().hash(hasher);
            for (k, v) in map {
                k.hash(hasher);
                hash_json_value(v, hasher);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message_with_input(input: Value) -> DisplayMessage {
        DisplayMessage {
            role: "tool".to_string(),
            content: "content".to_string(),
            tool_calls: vec!["read".to_string()],
            duration_secs: Some(1.0),
            title: Some("Read".to_string()),
            tool_data: Some(ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                input,
                intent: None,
                thought_signature: None,
            }),
        }
    }

    #[test]
    fn memoized_cache_hash_matches_the_direct_hash() {
        let message = message_with_input(json!({ "file_path": "a.rs" }));
        let mut memo = MessageHashMemo::default();
        let first = message.stable_cache_hash_memoized(&mut memo);
        let second = message.stable_cache_hash_memoized(&mut memo);
        assert_eq!(first, message.stable_cache_hash());
        assert_eq!(first, second, "the memo must return the same value");
    }

    #[test]
    fn memoized_cache_hash_recomputes_after_a_mutation() {
        let mut message = message_with_input(json!({ "file_path": "a.rs" }));
        let mut memo = MessageHashMemo::default();
        let before = message.stable_cache_hash_memoized(&mut memo);

        message.content = "different content".to_string();
        let after = message.stable_cache_hash_memoized(&mut memo);

        assert_ne!(before, after, "a mutated message must not reuse the memo");
        assert_eq!(after, message.stable_cache_hash());
    }

    #[test]
    fn memoized_cache_hash_recomputes_after_a_tool_input_change() {
        let mut message = message_with_input(json!({ "file_path": "a.rs" }));
        let mut memo = MessageHashMemo::default();
        let before = message.stable_cache_hash_memoized(&mut memo);

        message.tool_data.as_mut().unwrap().input = json!({ "file_path": "b.rs" });
        let after = message.stable_cache_hash_memoized(&mut memo);

        assert_ne!(
            before, after,
            "a changed tool input must not reuse the memo"
        );
        assert_eq!(after, message.stable_cache_hash());
    }

    #[test]
    fn stable_cache_hash_includes_tool_input() {
        let first = message_with_input(json!({ "file_path": "a.rs" }));
        let second = message_with_input(json!({ "file_path": "b.rs" }));
        assert_ne!(first.stable_cache_hash(), second.stable_cache_hash());
    }

    #[test]
    fn stable_cache_hash_ignores_duration() {
        let mut first = message_with_input(json!({ "file_path": "a.rs" }));
        let mut second = first.clone();
        first.duration_secs = Some(1.0);
        second.duration_secs = Some(9.0);
        assert_eq!(first.stable_cache_hash(), second.stable_cache_hash());
    }

    #[test]
    fn rendered_messages_convert_to_display_messages() {
        let rendered = RenderedMessage {
            role: "assistant".to_string(),
            content: "done".to_string(),
            tool_calls: vec!["read".to_string()],
            tool_data: None,
            stored_index: None,
        };

        let display = DisplayMessage::from_rendered_message(rendered);
        assert_eq!(display.role, "assistant");
        assert_eq!(display.content, "done");
        assert_eq!(display.tool_calls, ["read"]);
        assert!(display.tool_data.is_none());
    }

    #[test]
    fn transcript_preview_lines_share_desktop_labeling() {
        let messages = [
            ("user", " hello\nworld "),
            ("assistant", "this answer is long"),
            ("tool", "ignored when outside the tail"),
        ];

        assert_eq!(
            transcript_preview_lines(
                messages.iter().copied(),
                2,
                11,
                TranscriptPreviewLabels::DESKTOP
            ),
            vec![
                "asst this answer…".to_string(),
                "tool ignored whe…".to_string()
            ]
        );
        assert_eq!(
            latest_user_transcript_preview(messages.iter().copied(), 20),
            Some("hello world".to_string())
        );
    }
}
