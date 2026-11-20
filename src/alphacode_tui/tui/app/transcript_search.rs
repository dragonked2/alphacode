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
use crate::alphacode_tui::tui::core::{next_grapheme_boundary, prev_grapheme_boundary};
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
        let state = self.transcript_search.get_or_insert_with(Default::default);
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
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        state.pending_step = state.pending_step.saturating_add(delta);
        self.apply_pending_transcript_search_step();
    }

    /// Apply a deferred step once the renderer has indexed the current query.
    pub(super) fn apply_pending_transcript_search_step(&mut self) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let delta = state.pending_step;
        if delta == 0 {
            return;
        }
        let index = state.index.get_mut();
        if index.signature.query != state.query
            || index.signature.case_sensitive != state.case_sensitive
        {
            self.request_full_repaint();
            return;
        }

        state.pending_step = 0;
        let total = index.matches.len();
        if total > 0 {
            let selected = state.selected.min(total - 1) as i64;
            let total = total as i64;
            let next = (selected + delta.rem_euclid(total)).rem_euclid(total) as usize;
            state.selected = next;
            let hit = index.matches[next];
            self.scroll_to_transcript_match(hit);
        }
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
            KeyCode::Char('f' | 'F') if ctrl && !alt => {
                let delta = if modifiers.contains(KeyModifiers::SHIFT) {
                    -1
                } else {
                    1
                };
                self.step_transcript_search(delta);
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
    pub(super) fn insert_transcript_search_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        state.query.insert_str(cursor, text);
        state.query_cursor = snap_query_cursor(&state.query, cursor + text.len());
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Delete the grapheme before the query cursor.
    fn delete_transcript_search_char(&mut self) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        if cursor == 0 {
            return;
        }
        let previous = prev_grapheme_boundary(&state.query, cursor);
        state.query.replace_range(previous..cursor, "");
        state.query_cursor = snap_query_cursor(&state.query, previous);
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Delete the grapheme at the query cursor.
    fn delete_transcript_search_char_forward(&mut self) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let cursor = state.query_cursor.min(state.query.len());
        if cursor >= state.query.len() {
            return;
        }
        let next = next_grapheme_boundary(&state.query, cursor);
        state.query.replace_range(cursor..next, "");
        state.query_cursor = snap_query_cursor(&state.query, cursor);
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
            let previous = prev_grapheme_boundary(&state.query, start);
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
            let previous = prev_grapheme_boundary(&state.query, start);
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
        state.query_cursor = snap_query_cursor(&state.query, start);
        state.selected = 0;
        self.invalidate_transcript_search_index();
    }

    /// Move the query cursor by one displayed grapheme.
    fn move_transcript_search_cursor(&mut self, delta: i64) {
        let Some(state) = self.transcript_search.as_mut() else {
            return;
        };
        let len = state.query.len();
        let cursor = state.query_cursor.min(len);
        let next = if delta < 0 {
            prev_grapheme_boundary(&state.query, cursor)
        } else {
            next_grapheme_boundary(&state.query, cursor)
        };
        state.query_cursor = next;
    }

    /// Discard the hits and any queued navigation for the previous query.
    fn invalidate_transcript_search_index(&mut self) {
        if let Some(state) = self.transcript_search.as_mut() {
            state.pending_step = 0;
            let index = state.index.get_mut();
            index.signature = Default::default();
            index.matches.clear();
        }
    }

    /// Render snapshot of the find-in-page overlay.
    pub(crate) fn transcript_search_view(&self) -> Option<TranscriptSearchView> {
        let state = self.transcript_search.as_ref()?;
        let index = state.index.borrow();
        let selected = state.selected.min(index.matches.len().saturating_sub(1));
        let current_line = index.matches.get(selected).map(|hit| hit.abs_line);
        Some(TranscriptSearchView {
            query: state.query.clone(),
            case_sensitive: state.case_sensitive,
            selected,
            match_count: index.matches.len(),
            current_line,
        })
    }
}

/// An edit can join the graphemes on either side of the caret.
fn snap_query_cursor(text: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    next_grapheme_boundary(text, prev_grapheme_boundary(text, cursor))
}
