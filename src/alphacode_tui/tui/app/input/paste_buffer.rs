use std::{ops::Range, sync::Arc};

use super::PASTE_PLACEHOLDER;

/// Text stays out of the composer and is shared by draft/undo snapshots. Token
/// positions, not insertion order or token counts, identify the owning paste.
#[derive(Clone, Debug, Default)]
pub(in crate::alphacode_tui::tui::app) struct PasteBuffer {
    entries: Vec<PastedText>,
    payload_bytes: usize,
}

#[derive(Clone, Debug)]
struct PastedText {
    start: usize,
    text: Arc<String>,
}

impl PartialEq for PasteBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(&other.entries)
                .all(|(a, b)| a.start == b.start && Arc::ptr_eq(&a.text, &b.text))
    }
}

impl PasteBuffer {
    pub(in crate::alphacode_tui::tui::app) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in crate::alphacode_tui::tui::app) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(in crate::alphacode_tui::tui::app) fn clear(&mut self) {
        self.entries.clear();
        self.payload_bytes = 0;
    }

    pub(in crate::alphacode_tui::tui::app) fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    /// Conservative retained-size estimate. Shared payloads are counted in
    /// each snapshot for the undo budget, but are never copied by a keystroke.
    pub(in crate::alphacode_tui::tui::app) fn retained_bytes(&self) -> usize {
        self.payload_bytes + self.entries.capacity() * std::mem::size_of::<PastedText>()
    }

    pub(in crate::alphacode_tui::tui::app) fn insert(&mut self, start: usize, text: String) {
        self.payload_bytes += text.capacity();
        let index = self.entries.partition_point(|entry| entry.start < start);
        self.entries.insert(
            index,
            PastedText {
                start,
                text: Arc::new(text),
            },
        );
    }

    /// Called at the shared edit boundary before mutating the draft. Editing
    /// inside a token invalidates only that token; surrounding tokens move with
    /// their text. Deletions cannot make a different payload inherit its slot.
    pub(in crate::alphacode_tui::tui::app) fn rebase(
        &mut self,
        range: Range<usize>,
        inserted_bytes: usize,
    ) {
        let removed_bytes = range.end - range.start;
        self.entries.retain_mut(|entry| {
            let end = entry.start + PASTE_PLACEHOLDER.len();
            let overlaps = if range.is_empty() {
                entry.start < range.start && range.start < end
            } else {
                entry.start < range.end && range.start < end
            };
            if overlaps {
                self.payload_bytes -= entry.text.capacity();
                return false;
            }
            if entry.start >= range.end {
                entry.start = entry.start - removed_bytes + inserted_bytes;
            }
            true
        });
    }

    /// Return native echo text only when the entire range is made of owned
    /// paste tokens. Literal lookalikes must never consume a clipboard result.
    pub(in crate::alphacode_tui::tui::app) fn text_in_token_range(
        &self,
        draft: &str,
        range: Range<usize>,
    ) -> Option<String> {
        let mut cursor = range.start;
        let mut result = String::new();
        for entry in self
            .entries
            .iter()
            .filter(|entry| range.contains(&entry.start))
        {
            let end = entry.start + PASTE_PLACEHOLDER.len();
            if entry.start != cursor
                || end > range.end
                || draft.get(entry.start..end) != Some(PASTE_PLACEHOLDER)
            {
                return None;
            }
            result.push_str(&entry.text);
            cursor = end;
        }
        (cursor == range.end && cursor > range.start).then_some(result)
    }

    pub(in crate::alphacode_tui::tui::app) fn expand(&self, draft: &str) -> String {
        self.expand_range(draft, 0..draft.len())
    }

    /// Clipboard operations expand complete owned tokens in the selection,
    /// never literal lookalikes or only part of a token.
    pub(in crate::alphacode_tui::tui::app) fn expand_range(
        &self,
        draft: &str,
        range: Range<usize>,
    ) -> String {
        let mut result = String::new();
        let mut cursor = range.start;
        for entry in &self.entries {
            let end = entry.start + PASTE_PLACEHOLDER.len();
            if entry.start < cursor
                || end > range.end
                || draft.get(entry.start..end) != Some(PASTE_PLACEHOLDER)
            {
                continue;
            }
            result.push_str(&draft[cursor..entry.start]);
            result.push_str(&entry.text);
            cursor = end;
        }
        result.push_str(&draft[cursor..range.end]);
        result
    }
}

#[cfg(test)]
impl PartialEq<Vec<String>> for PasteBuffer {
    fn eq(&self, other: &Vec<String>) -> bool {
        self.entries.len() == other.len()
            && self
                .entries
                .iter()
                .zip(other)
                .all(|(entry, text)| entry.text.as_str() == text)
    }
}

#[cfg(test)]
impl PartialEq<Vec<&str>> for PasteBuffer {
    fn eq(&self, other: &Vec<&str>) -> bool {
        self.entries.len() == other.len()
            && self
                .entries
                .iter()
                .zip(other)
                .all(|(entry, text)| entry.text.as_str() == *text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_share_large_payloads_and_do_not_expand_literals() {
        let mut buffer = PasteBuffer::default();
        buffer.insert(0, "shared payload\n".repeat(100_000));
        let snapshot = buffer.clone();
        assert!(Arc::ptr_eq(
            &buffer.entries[0].text,
            &snapshot.entries[0].text
        ));
        let draft = format!("{PASTE_PLACEHOLDER} literal {PASTE_PLACEHOLDER}");
        assert_eq!(
            buffer.expand(&draft),
            "shared payload\n".repeat(100_000) + " literal " + PASTE_PLACEHOLDER
        );
        buffer.rebase(0..PASTE_PLACEHOLDER.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(snapshot.len(), 1);
    }
}
