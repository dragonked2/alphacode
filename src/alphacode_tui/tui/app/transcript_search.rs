//! Find-in-page over the transcript (`Ctrl+F`).
//!
//! The app owns the query, the case-sensitivity mode and the selection; the
//! renderer owns the hit index (see
//! [`crate::alphacode_tui::tui::transcript_search`]). This module wires the
//! two together: it opens and closes the overlay, edits the query, and moves
//! the selection — scrolling the viewport to the selected hit so the user
//! actually sees where they are.

use crossterm::event::{KeyCode, KeyModifiers};

use super::App;
use crate::alphacode_tui::tui::transcript_search::{
    TranscriptSearchMatch, TranscriptSearchState, TranscriptSearchView,
};

impl App {
    /// Open the find-in-page overlay.
    ///
    /// Re-opening while a search is already active keeps the query and jumps
    /// straight to the next hit, which is what a second `Ctrl+F` press is
    /// expected to do.
    pub(super) fn open_transcript_search(&mut self) {
        if self.transcript_search.is_some() {
            self.step_transcript_search(1);
            return;
        }
        self.transcript_search = Some(TranscriptSearchState::default());
        self.request_full_repaint();
    }

    /// Open the find-in-page overlay with a pre-filled query.
    ///
    /// Used by the `/find <text>` command so the user can jump straight to a
    /// hit instead of typing the query into the overlay.
    pub(super) fn open_transcript_search_with_query(&mut self, query: &str) {
        self.open_transcript_search();
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        state.query = query.to_string();
        state.query_cursor = state.query.len();
        state.selected = 0;
        self.invalidate_transcript_search_index();
        self.request_full_repaint();
    }

    /// Close the find-in-page overlay, leaving the viewport where the user
    /// navigated to.
    pub(super) fn close_transcript_search(&mut self) {
        if self.transcript_search.take().is_some() {
            self.request_full_repaint();
        }
    }

    /// Whether the find-in-page overlay is open.
    pub(super) fn transcript_search_is_open(&self) -> bool {
        self.transcript_search.is_some()
    }

    /// Move the selection by `delta` hits and scroll the selected hit into
    /// view.
    ///
    /// The hit index is produced by the renderer, so it can be one frame
    /// behind the query the user just typed. When that happens the step is
    /// *deferred* rather than dropped: the renderer refreshes the index during
    /// the next frame and
    /// [`Self::apply_pending_transcript_search_step`] applies the step then.
    /// Typing a query and pressing Enter within the same event batch must
    /// still advance the search.
    fn step_transcript_search(&mut self, delta: i64) {
        let Some(state) = self.transcript_search.as_ref() else {
            return;
        };
        let index = state.index.borrow();
        let fresh = index.signature.query == state.query
            && index.signature.case_sensitive == state.case_sensitive;
        if !fresh {
            drop(index);
            if let Some(state) = self.transcript_search.as_mut() {
                state.pending_step += delta;
            }
            self.request_full_repaint();
            return;
        }
        let matches = index.matches.clone();
        drop(index);
        if matches.is_empty() {
            return;
        }
        let max = matches.len() as i64 - 1;
        let selected = self
            .transcript_search
            .as_ref()
            .map(|state| state.selected)
            .unwrap_or(0);
        let next = (selected as i64 + delta).clamp(0, max) as usize;
        if let Some(state) = self.transcript_search.as_mut() {
            state.selected = next;
        }
        self.scroll_to_transcript_match(matches[next]);
    }

    /// Apply a find-in-page step that was deferred because the renderer had
    /// not caught up with the query yet.
    ///
    /// Called right after a frame is drawn: the draw refreshed the hit index,
    /// so a pending step can be applied to it now.
    pub(super) fn apply_pending_transcript_search_step(&mut self) {
        let Some(state) = self.transcript_search.as_ref() else {
            return;
        };
        let delta = state.pending_step;
        if delta == 0 {
            return;
        }
        let index = state.index.borrow();
        let fresh = index.signature.query == state.query
            && index.signature.case_sensitive == state.case_sensitive;
        if !fresh {
            // Still stale: keep the step pending for the next frame.
            return;
        }
        let matches = index.matches.clone();
        drop(index);

        if let Some(state) = self.transcript_search.as_mut() {
            state.pending_step = 0;
        }
        if matches.is_empty() {
            return;
        }
        let max = matches.len() as i64 - 1;
        let selected = self
            .transcript_search
            .as_ref()
            .map(|state| state.selected)
            .unwrap_or(0);
        let next = (selected as i64 + delta).clamp(0, max) as usize;
        if let Some(state) = self.transcript_search.as_mut() {
            state.selected = next;
        }
        self.scroll_to_transcript_match(matches[next]);
    }

    /// Scroll the transcript so the given hit is at the top of the viewport.
    fn scroll_to_transcript_match(&mut self, hit: TranscriptSearchMatch) {
        self.pending_history_anchor = None;
        self.scroll_offset = hit.abs_line;
        self.auto_scroll_paused = true;
        self.request_full_repaint();
    }

    /// Handle a key press while the find-in-page overlay is open.
    ///
    /// Returns `true` when the key was consumed by the overlay.
    pub(super) fn handle_transcript_search_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        let alt = modifiers.contains(KeyModifiers::ALT);

        match code {
            KeyCode::Esc => {
                self.close_transcript_search();
                return true;
            }
            // Ctrl+C cancels the search rather than quitting the app: the user
            // asked to search, not to exit.
            KeyCode::Char('c') if ctrl => {
                self.close_transcript_search();
                return true;
            }
            KeyCode::Enter => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    self.step_transcript_search(-1);
                } else {
                    self.step_transcript_search(1);
                }
                return true;
            }
            KeyCode::Up => {
                self.step_transcript_search(-1);
                return true;
            }
            KeyCode::Down => {
                self.step_transcript_search(1);
                return true;
            }
            KeyCode::Char('n') if ctrl => {
                self.step_transcript_search(1);
                return true;
            }
            KeyCode::Char('p') if ctrl => {
                self.step_transcript_search(-1);
                return true;
            }
            // Alt+C toggles case sensitivity, the same gesture `less` uses.
            KeyCode::Char('c') if alt => {
                if let Some(state) = self.transcript_search.as_mut() {
                    state.case_sensitive = !state.case_sensitive;
                    state.selected = 0;
                }
                self.invalidate_transcript_search_index();
                return true;
            }
            KeyCode::Char('u') if ctrl => {
                if let Some(state) = self.transcript_search.as_mut() {
                    state.reset_query();
                }
                self.invalidate_transcript_search_index();
                return true;
            }
            KeyCode::Char('w') if ctrl => {
                self.delete_transcript_search_word();
                return true;
            }
            KeyCode::Char('a') if ctrl => {
                if let Some(state) = self.transcript_search.as_mut() {
                    state.query_cursor = 0;
                }
                return true;
            }
            KeyCode::Char('e') if ctrl => {
                if let Some(state) = self.transcript_search.as_mut() {
                    state.query_cursor = state.query.len();
                }
                return true;
            }
            KeyCode::Left => {
                self.move_transcript_search_cursor(-1);
                return true;
            }
            KeyCode::Right => {
                self.move_transcript_search_cursor(1);
                return true;
            }
            KeyCode::Home => {
                if let Some(state) = self.transcript_search.as_mut() {
                    state.query_cursor = 0;
                }
                return true;
            }
            KeyCode::End => {
                if let Some(state) = self.transcript_search.as_mut() {
                    state.query_cursor = state.query.len();
                }
                return true;
            }
            KeyCode::Backspace => {
                self.delete_transcript_search_char();
                return true;
            }
            KeyCode::Delete => {
                self.delete_transcript_search_char_forward();
                return true;
            }
            _ => {}
        }

        if let Some(text) = super::input::text_input_for_key(code, modifiers) {
            self.insert_transcript_search_text(&text);
            return true;
        }
        false
    }

    /// Insert text at the query cursor.
    fn insert_transcript_search_text(&mut self, text: &str) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        state.query.insert_str(cursor, text);
        state.query_cursor = cursor + text.len();
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Delete the character before the query cursor.
    fn delete_transcript_search_char(&mut self) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        if cursor == 0 {
            return;
        }
        let previous = prev_char_boundary(&state.query, cursor);
        state.query.replace_range(previous..cursor, "");
        state.query_cursor = previous;
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Delete the character at the query cursor.
    fn delete_transcript_search_char_forward(&mut self) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        if cursor >= state.query.len() {
            return;
        }
        let next = next_char_boundary(&state.query, cursor);
        state.query.replace_range(cursor..next, "");
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Delete the word before the query cursor.
    fn delete_transcript_search_word(&mut self) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        let mut start = cursor;
        // Skip the whitespace immediately before the cursor.
        while start > 0 {
            let previous = prev_char_boundary(&state.query, start);
            if !state.query[previous..start]
                .chars()
                .all(char::is_whitespace)
            {
                break;
            }
            start = previous;
        }
        // Then skip the word itself.
        while start > 0 {
            let previous = prev_char_boundary(&state.query, start);
            if state.query[previous..start]
                .chars()
                .any(char::is_whitespace)
            {
                break;
            }
            start = previous;
        }
        if start == cursor {
            return;
        }
        state.query.replace_range(start..cursor, "");
        state.query_cursor = start;
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Move the query cursor by `delta` characters.
    ///
    /// The cursor always lands on a character boundary: stepping by bytes
    /// would put it inside a multi-byte character, and the next insertion
    /// would then split that character.
    fn move_transcript_search_cursor(&mut self, delta: i64) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let len = state.query.len();
        let cursor = state.query_cursor.min(len);
        let next = if delta < 0 {
            prev_char_boundary(&state.query, cursor)
        } else {
            next_char_boundary(&state.query, cursor)
        };
        state.query_cursor = next;
    }

    /// Force the renderer to rescan the transcript on the next frame.
    ///
    /// The renderer caches its hit index against the query and the transcript
    /// size, so a query edit has to invalidate that cache explicitly. The
    /// stale hits are dropped too: navigating them would jump to a match of
    /// the *previous* query.
    fn invalidate_transcript_search_index(&mut self) {
        if let Some(state) = self.transcript_search.as_ref() {
            let mut index = state.index.borrow_mut();
            index.signature = Default::default();
            index.matches.clear();
        }
    }

    /// Render snapshot of the find-in-page overlay.
    pub(crate) fn transcript_search_view(&self) -> Option<TranscriptSearchView> {
        let state = self.transcript_search.as_ref()?;
        let index = state.index.borrow();
        let current_line = index.matches.get(state.selected).map(|hit| hit.abs_line);
        Some(TranscriptSearchView {
            query: state.query.clone(),
            case_sensitive: state.case_sensitive,
            selected: state.selected,
            match_count: index.matches.len(),
            current_line,
        })
    }
}

/// The byte offset of the character *before* `offset`.
///
/// This is the "delete backwards" offset: the cursor points at the start of a
/// character, and the character may be several bytes long, so the start of the
/// previous character is not simply `offset - 1`.
fn prev_char_boundary(text: &str, offset: usize) -> usize {
    let mut index = offset.min(text.len());
    if index == 0 {
        return 0;
    }
    // Step back one byte, then keep stepping until the offset is on a
    // character boundary — that is the start of the character before the
    // cursor.
    index -= 1;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Round a byte offset up to the boundary *after* the character at it.
///
/// This is the "delete the character under the cursor" offset: the cursor
/// points at the start of a character, and the character may be several bytes
/// long, so the end boundary is not simply the next byte.
fn next_char_boundary(text: &str, offset: usize) -> usize {
    let len = text.len();
    let mut index = offset.min(len);
    // Never assume the offset is already on a boundary.
    while index < len && !text.is_char_boundary(index) {
        index += 1;
    }
    if index >= len {
        return len;
    }
    let mut next = index + 1;
    while next < len && !text.is_char_boundary(next) {
        next += 1;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::{next_char_boundary, prev_char_boundary};

    #[test]
    fn prev_boundary_points_at_the_start_of_the_previous_char() {
        let text = "aé";
        // The cursor sits after "é" (byte 3), so the previous character starts
        // at byte 1 — not byte 2, which is inside the two-byte "é".
        assert_eq!(prev_char_boundary(text, 3), 1);
        assert_eq!(prev_char_boundary(text, 1), 0);
        assert_eq!(prev_char_boundary(text, 0), 0);
    }

    #[test]
    fn prev_boundary_handles_ascii() {
        assert_eq!(prev_char_boundary("abc", 3), 2);
        assert_eq!(prev_char_boundary("abc", 1), 0);
    }

    #[test]
    fn next_boundary_never_splits_a_char() {
        let text = "aé";
        // "é" is two bytes, so the boundary after it is 3.
        assert_eq!(next_char_boundary(text, 0), 1);
        assert_eq!(next_char_boundary(text, 1), 3);
        assert_eq!(next_char_boundary(text, 3), 3);
    }

    #[test]
    fn next_boundary_lands_on_a_boundary_even_from_a_split_offset() {
        let text = "aé";
        // Offset 2 is inside "é"; the result must still be a boundary.
        let next = next_char_boundary(text, 2);
        assert_eq!(next, 3);
        assert!(text.is_char_boundary(next));
    }
}
