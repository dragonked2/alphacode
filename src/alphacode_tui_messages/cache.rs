use crate::alphacode_config_types::{DiagramDisplayMode, DiffDisplayMode};
use crate::alphacode_tui_messages::DisplayMessage;
use ratatui::layout::Alignment;
use ratatui::text::{Line, Span};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MessageCacheKey {
    width: u16,
    diff_mode: DiffDisplayMode,
    message_hash: u64,
    content_len: usize,
    diagram_mode: DiagramDisplayMode,
    centered: bool,
    mermaid_epoch: u64,
    mermaid_aspect_bucket: Option<u16>,
    show_alphagrep_output: bool,
    tool_call_details: bool,
}

#[derive(Default)]
struct MessageCacheState {
    entries: HashMap<MessageCacheKey, Arc<Vec<Line<'static>>>>,
    order: VecDeque<MessageCacheKey>,
    /// Memoized message cache keys, keyed by the message's cheap shape token.
    ///
    /// `DisplayMessage::stable_cache_hash` is O(payload): it walks the whole
    /// message body and the whole tool-input JSON tree. The message cache
    /// consults that hash on **every** lookup, including hits, so the key
    /// cost could exceed the render it was meant to avoid. This table maps a
    /// cheap shape token (buffer addresses + lengths) to the previously
    /// computed full hash, so a repeated lookup of an unchanged message costs
    /// O(1) instead of O(payload).
    ///
    /// The table is bounded by the same limit as the render cache and shares
    /// its eviction order, so it cannot grow without limit.
    hash_memos: HashMap<u64, u64>,
    /// Insertion order of the memo keys, for bounded eviction.
    hash_memo_order: VecDeque<u64>,
}

impl MessageCacheState {
    #[allow(dead_code)]
    fn get(&self, key: &MessageCacheKey) -> Option<Vec<Line<'static>>> {
        self.entries.get(key).map(|arc| arc.as_ref().clone())
    }

    fn get_shared(&self, key: &MessageCacheKey) -> Option<Arc<Vec<Line<'static>>>> {
        self.entries.get(key).cloned()
    }

    #[allow(dead_code)]
    fn insert(&mut self, key: MessageCacheKey, lines: Vec<Line<'static>>) {
        let arc = Arc::new(lines);
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.entries.entry(key.clone())
        {
            entry.insert(arc);
            return;
        }

        self.entries.insert(key.clone(), arc);
        self.order.push_back(key);

        while self.order.len() > MESSAGE_CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    /// Look up a memoized message cache key by its cheap shape token.
    fn hash_memo_get(&self, shape_tag: u64) -> Option<u64> {
        self.hash_memos.get(&shape_tag).copied()
    }

    /// Memoize a message cache key under its cheap shape token.
    fn hash_memo_insert(&mut self, shape_tag: u64, value: u64) {
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.hash_memos.entry(shape_tag)
        {
            entry.insert(value);
            return;
        }
        self.hash_memos.insert(shape_tag, value);
        self.hash_memo_order.push_back(shape_tag);
        while self.hash_memo_order.len() > MESSAGE_CACHE_LIMIT {
            if let Some(oldest) = self.hash_memo_order.pop_front() {
                self.hash_memos.remove(&oldest);
            }
        }
    }

    fn insert_shared(&mut self, key: MessageCacheKey, lines: Arc<Vec<Line<'static>>>) {
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.entries.entry(key.clone())
        {
            entry.insert(lines);
            return;
        }

        self.entries.insert(key.clone(), lines);
        self.order.push_back(key);

        while self.order.len() > MESSAGE_CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

static MESSAGE_CACHE: OnceLock<Mutex<MessageCacheState>> = OnceLock::new();

fn message_cache() -> &'static Mutex<MessageCacheState> {
    MESSAGE_CACHE.get_or_init(|| Mutex::new(MessageCacheState::default()))
}

const MESSAGE_CACHE_LIMIT: usize = 2048;

/// Pack a message shape token into a single `u64` lookup key.
///
/// The token mixes the buffer addresses and lengths that feed
/// `stable_cache_hash`. A change to any of them invalidates the memo, so a
/// mutated message is re-hashed. A 0 token is remapped to 1 so it is never
/// confused with "no memo".
fn shape_tag(shape: &crate::alphacode_tui_messages::MessageShape) -> u64 {
    use std::hash::Hash;
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    shape.role_ptr.hash(&mut hasher);
    shape.role_len.hash(&mut hasher);
    shape.content_ptr.hash(&mut hasher);
    shape.content_len.hash(&mut hasher);
    shape.content_fingerprint.hash(&mut hasher);
    shape.tool_calls_ptr.hash(&mut hasher);
    shape.tool_calls_len.hash(&mut hasher);
    shape.title_ptr.hash(&mut hasher);
    shape.title_len.hash(&mut hasher);
    shape.tool_input_ptr.hash(&mut hasher);
    shape.tool_input_present.hash(&mut hasher);
    let tag = hasher.finish();
    if tag == 0 { 1 } else { tag }
}

/// Memoized [`DisplayMessage::stable_cache_hash`].
///
/// `stable_cache_hash` is O(payload): it walks the whole message body and the
/// whole tool-input JSON tree. The body builder needs that hash for every
/// rendered message (boundary table, aux cache) and again for the whole
/// transcript prefix and suffix when deciding what can be reused — several
/// passes over the entire transcript on every rebuild.
///
/// The memo table turns a repeated lookup of an unchanged message into an
/// O(1) hit, which is what makes the per-rebuild cost proportional to what
/// actually changed instead of to the size of the conversation.
pub fn memoized_stable_cache_hash(msg: &DisplayMessage) -> u64 {
    if cfg!(test) {
        return msg.stable_cache_hash();
    }

    let shape = msg.cache_shape();
    let shape_tag = shape_tag(&shape);
    {
        let cache = match message_cache().lock() {
            Ok(c) => c,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(value) = cache.hash_memo_get(shape_tag) {
            return value;
        }
    }

    let value = msg.stable_cache_hash();
    let mut cache = match message_cache().lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    cache.hash_memo_insert(shape_tag, value);
    value
}

/// Runtime-sensitive inputs that affect message rendering but are not intrinsic to a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MessageCacheContext {
    pub diagram_mode: DiagramDisplayMode,
    pub centered: bool,
    pub mermaid_epoch: u64,
    pub mermaid_aspect_bucket: Option<u16>,
    pub show_alphagrep_output: bool,
    pub tool_call_details: bool,
}

pub fn left_pad_lines_for_centered_mode(lines: &mut [Line<'static>], width: u16) {
    let max_line_width = lines.iter().map(Line::width).max().unwrap_or(0);
    let pad = (width as usize).saturating_sub(max_line_width) / 2;
    if pad == 0 {
        return;
    }

    // Pre-allocate the pad string once and use String::clone (heap alloc)
    // for each line. Unavoidable given Span<'static> ownership, but
    // concentrated into one allocation pattern for the allocator.
    let pad_str = " ".repeat(pad);
    for line in lines {
        line.spans.insert(0, Span::raw(pad_str.clone()));
        line.alignment = Some(Alignment::Left);
    }
}

pub fn centered_wrap_width(width: u16, centered: bool, centered_max_width: usize) -> usize {
    let width = width as usize;
    if centered {
        width.min(centered_max_width).max(1)
    } else {
        width.max(1)
    }
}

pub fn get_cached_message_lines<F>(
    msg: &DisplayMessage,
    width: u16,
    diff_mode: DiffDisplayMode,
    context: MessageCacheContext,
    render: F,
) -> Vec<Line<'static>>
where
    F: FnOnce(&DisplayMessage, u16, DiffDisplayMode) -> Vec<Line<'static>>,
{
    get_cached_message_lines_shared(msg, width, diff_mode, context, render)
        .as_ref()
        .clone()
}

/// Zero-copy variant: returns `Arc` so hot per-frame hits share the cached
/// `Vec<Line>` without deep-cloning every `Span`/`String`. Prefer this in
/// render loops; only clone to `Vec` at the final draw boundary if needed.
pub fn get_cached_message_lines_shared<F>(
    msg: &DisplayMessage,
    width: u16,
    diff_mode: DiffDisplayMode,
    context: MessageCacheContext,
    render: F,
) -> Arc<Vec<Line<'static>>>
where
    F: FnOnce(&DisplayMessage, u16, DiffDisplayMode) -> Vec<Line<'static>>,
{
    if cfg!(test) {
        return Arc::new(render(msg, width, diff_mode));
    }

    // Resolve the message's cache key through the memo table.
    //
    // `stable_cache_hash` is O(payload); the memo makes a repeated lookup of
    // an unchanged message O(1). The shape token is derived from buffer
    // addresses, lengths and a content fingerprint, so an in-place mutation of
    // a public field is detected and the full hash recomputed.
    let message_hash = memoized_stable_cache_hash(msg);

    let key = MessageCacheKey {
        width,
        diff_mode,
        message_hash,
        content_len: msg.content.len(),
        diagram_mode: context.diagram_mode,
        centered: context.centered,
        mermaid_epoch: context.mermaid_epoch,
        mermaid_aspect_bucket: context.mermaid_aspect_bucket,
        show_alphagrep_output: context.show_alphagrep_output,
        tool_call_details: context.tool_call_details,
    };

    // Fast path: shared hit with a single lock, no deep clone.
    {
        let cache = match message_cache().lock() {
            Ok(c) => c,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(shared) = cache.get_shared(&key) {
            return shared;
        }
    }

    let lines = Arc::new(render(msg, width, diff_mode));
    let mut cache = match message_cache().lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    // Re-check under lock: another thread may have inserted while rendering.
    if let Some(shared) = cache.get_shared(&key) {
        return shared;
    }
    cache.insert_shared(key, Arc::clone(&lines));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_wrap_width_caps_centered_width() {
        assert_eq!(centered_wrap_width(120, true, 96), 96);
        assert_eq!(centered_wrap_width(80, true, 96), 80);
        assert_eq!(centered_wrap_width(120, false, 96), 120);
    }

    #[test]
    fn left_pad_lines_aligns_to_centered_block() {
        let mut lines = vec![Line::from("abc")];
        left_pad_lines_for_centered_mode(&mut lines, 9);
        assert_eq!(lines[0].to_string(), "   abc");
        assert_eq!(lines[0].alignment, Some(Alignment::Left));
    }
}
