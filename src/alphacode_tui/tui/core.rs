//! Shared TUI state and logic used across TUI runtime paths.
//!
//! This module contains the common display state, input handling,
//! and helper methods used by both local and remote TUI modes.
use super::DisplayMessage;
use unicode_segmentation::UnicodeSegmentation;

/// Find the byte offset of the previous extended grapheme boundary before
/// `pos`. This keeps editing operations atomic for combining marks, emoji
/// modifiers, and joined emoji sequences.
pub fn prev_grapheme_boundary(s: &str, pos: usize) -> usize {
    let pos = pos.min(s.len());
    s.grapheme_indices(true)
        .take_while(|(start, _)| *start < pos)
        .map(|(start, _)| start)
        .last()
        .unwrap_or(0)
}

/// Find the byte offset of the next extended grapheme boundary after `pos`.
/// Returns `s.len()` if already at or past the end.
pub fn next_grapheme_boundary(s: &str, pos: usize) -> usize {
    let pos = pos.min(s.len());
    s.grapheme_indices(true)
        .find_map(|(start, grapheme)| {
            (start >= pos || start + grapheme.len() > pos).then_some(start + grapheme.len())
        })
        .unwrap_or(s.len())
}

/// Convert a byte offset in a string to a character index.
/// Needed when the renderer works in character space but `cursor_pos` is byte-based.
#[cfg(test)]
pub fn byte_offset_to_char_index(s: &str, byte_offset: usize) -> usize {
    s[..byte_offset.min(s.len())].chars().count()
}

/// Convert a byte offset to an extended grapheme-cluster index. An offset
/// inside a cluster maps to the index before that cluster.
pub fn byte_offset_to_grapheme_index(s: &str, byte_offset: usize) -> usize {
    let byte_offset = byte_offset.min(s.len());
    s.grapheme_indices(true)
        .take_while(|(start, grapheme)| *start + grapheme.len() <= byte_offset)
        .count()
}

/// Convert an extended grapheme-cluster index to a byte offset. Returns
/// `s.len()` when the requested index is at or beyond the end.
pub fn grapheme_index_to_byte_offset(s: &str, grapheme_index: usize) -> usize {
    if grapheme_index == 0 {
        return 0;
    }

    s.grapheme_indices(true)
        .nth(grapheme_index)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

// ========== DisplayMessage Helpers ==========

pub(crate) trait DisplayMessageRoleExt {
    /// Return the role that should be used for rendering.
    ///
    /// Background-task notifications are persisted/injected through a few older
    /// paths that can lose the dedicated `background_task` display role and come
    /// back as plain `user`/`system` markdown. Detect the canonical notification
    /// shape so those messages still render as the rounded background-task card.
    fn effective_role(&self) -> &str;
}

impl DisplayMessageRoleExt for DisplayMessage {
    fn effective_role(&self) -> &str {
        if self.role != "background_task"
            && self.role != "tool"
            && is_background_task_notification_content(&self.content)
        {
            "background_task"
        } else {
            self.role.as_str()
        }
    }
}

fn is_background_task_notification_content(content: &str) -> bool {
    crate::message::parse_background_task_notification_markdown(content).is_some()
        || crate::message::parse_background_task_progress_notification_markdown(content).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tui_messages::DisplayMessage;

    #[test]
    fn test_display_message_helpers() {
        let msg = DisplayMessage::error("something went wrong");
        assert_eq!(msg.role, "error");
        assert_eq!(msg.content, "something went wrong");

        let msg = DisplayMessage::user("hello").with_title("greeting");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.title, Some("greeting".to_string()));
    }

    #[test]
    fn test_byte_offset_to_char_index() {
        assert_eq!(byte_offset_to_char_index("hello", 0), 0);
        assert_eq!(byte_offset_to_char_index("hello", 3), 3);
        assert_eq!(byte_offset_to_char_index("hello", 5), 5);

        // Korean: each char is 3 bytes
        assert_eq!(byte_offset_to_char_index("한글", 0), 0);
        assert_eq!(byte_offset_to_char_index("한글", 3), 1);
        assert_eq!(byte_offset_to_char_index("한글", 6), 2);

        // Mixed
        assert_eq!(byte_offset_to_char_index("a한b", 0), 0);
        assert_eq!(byte_offset_to_char_index("a한b", 1), 1);
        assert_eq!(byte_offset_to_char_index("a한b", 4), 2);
        assert_eq!(byte_offset_to_char_index("a한b", 5), 3);
    }

    #[test]
    fn grapheme_boundary_helpers_keep_clusters_atomic() {
        let s = "한글test";
        // "한" is bytes 0..3, "글" is bytes 3..6, "test" is bytes 6..10.
        assert_eq!(prev_grapheme_boundary(s, 3), 0);
        assert_eq!(prev_grapheme_boundary(s, 6), 3);
        assert_eq!(prev_grapheme_boundary(s, 7), 6);
        assert_eq!(prev_grapheme_boundary(s, 0), 0);

        assert_eq!(next_grapheme_boundary(s, 0), 3);
        assert_eq!(next_grapheme_boundary(s, 3), 6);
        assert_eq!(next_grapheme_boundary(s, 6), 7);
        assert_eq!(next_grapheme_boundary(s, 9), 10);
    }

    #[test]
    fn grapheme_boundaries_keep_combining_and_zwj_sequences_atomic() {
        let text = "a👨‍👩‍👧‍👦e\u{301}z";
        let family_start = "a".len();
        let combining_start = family_start + "👨‍👩‍👧‍👦".len();
        let z_start = combining_start + "e\u{301}".len();

        assert_eq!(prev_grapheme_boundary(text, text.len()), z_start);
        assert_eq!(prev_grapheme_boundary(text, z_start), combining_start);
        assert_eq!(prev_grapheme_boundary(text, combining_start), family_start);
        assert_eq!(next_grapheme_boundary(text, family_start), combining_start);
        assert_eq!(next_grapheme_boundary(text, combining_start), z_start);
        assert_eq!(next_grapheme_boundary(text, z_start), text.len());

        // Even a stale byte position inside a cluster resolves to its outer edge.
        assert_eq!(prev_grapheme_boundary(text, family_start + 5), family_start);
        assert_eq!(
            next_grapheme_boundary(text, family_start + 5),
            combining_start
        );
    }

    #[test]
    fn grapheme_index_conversions_round_trip_cluster_boundaries() {
        let text = "a👩🏽‍💻e\u{301}";
        let offsets: Vec<usize> = std::iter::once(0)
            .chain(
                text.grapheme_indices(true)
                    .map(|(start, grapheme)| start + grapheme.len()),
            )
            .collect();

        for (index, offset) in offsets.iter().copied().enumerate() {
            assert_eq!(byte_offset_to_grapheme_index(text, offset), index);
            assert_eq!(grapheme_index_to_byte_offset(text, index), offset);
        }
    }

    #[test]
    fn byte_offset_inside_grapheme_maps_before_it() {
        let text = "a👩‍💻z";
        let inside_emoji = "a👩".len();
        assert_eq!(byte_offset_to_grapheme_index(text, inside_emoji), 1);
        assert_eq!(grapheme_index_to_byte_offset(text, 2), "a👩‍💻".len());
    }
}
