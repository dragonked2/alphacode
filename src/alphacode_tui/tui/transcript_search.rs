//! Find-in-page over the rendered transcript (`Ctrl+F`).
//!
//! # Why the renderer owns the index
//!
//! The transcript the user actually reads is not the raw message list: it is
//! the *prepared* transcript — markdown rendered, wrapped to the terminal
//! width, prefixed with role decorations, and trimmed. Searching the raw
//! messages would therefore report hits the user cannot see (truncated tool
//! output, collapsed reasoning traces) and miss text that is plainly on
//! screen (a role label, a rendered heading).
//!
//! So the renderer searches the prepared lines it is about to paint, and
//! publishes the resulting index back to the app. The app uses that index to
//! drive next/previous navigation (it needs the absolute line of a hit to
//! scroll to it), and the overlay uses it to show the hit counter.
//!
//! # Data flow
//!
//! ```text
//!   app state (query, selection)  ──▶  renderer (computes hits)
//!          ▲                                    │
//!          └────────────  index  ────────────────┘
//! ```
//!
//! The index is recomputed only when the query, the case sensitivity, or the
//! transcript itself changed, so an idle TUI does not rescan the transcript on
//! every frame.

use std::cell::RefCell;

use crate::alphacode_tui_messages::{PreparedChatFrame, PreparedSectionKind};

/// One hit of the transcript search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptSearchMatch {
    /// Absolute wrapped-line index within the prepared transcript.
    pub abs_line: usize,
    /// Display column where the match starts on that line.
    pub start_col: usize,
    /// Display column where the match ends (exclusive).
    pub end_col: usize,
}

/// Identifies the inputs a set of matches was computed from.
///
/// Two searches with the same signature can reuse the same matches, which is
/// what keeps the scan off the per-frame path.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TranscriptSearchSignature {
    /// The query the matches were computed for.
    pub query: String,
    /// Whether the match was case-sensitive.
    pub case_sensitive: bool,
    /// Number of transcript lines that were scanned.
    pub source_lines: usize,
    /// Number of transcript messages that were scanned.
    pub source_messages: usize,
}

impl TranscriptSearchSignature {
    /// Build the signature for a query against a prepared transcript.
    pub fn for_frame(query: &str, case_sensitive: bool, prepared: &PreparedChatFrame) -> Self {
        let (source_lines, source_messages) = match prepared
            .sections
            .iter()
            .find(|section| section.kind == PreparedSectionKind::Body)
        {
            Some(body) => (
                body.prepared.wrapped_plain_lines.len(),
                body.prepared.message_boundaries.len(),
            ),
            None => (0, 0),
        };
        Self {
            query: query.to_string(),
            case_sensitive,
            source_lines,
            source_messages,
        }
    }
}

/// The renderer-owned index of transcript search hits.
#[derive(Clone, Debug, Default)]
pub struct TranscriptSearchIndex {
    /// The inputs the matches were computed from.
    pub signature: TranscriptSearchSignature,
    /// Hits in document order.
    pub matches: Vec<TranscriptSearchMatch>,
}

impl TranscriptSearchIndex {
    /// Whether the index already answers this exact question.
    pub fn is_current_for(&self, signature: &TranscriptSearchSignature) -> bool {
        &self.signature == signature
    }
}

/// App-owned state for the find-in-page overlay.
#[derive(Debug, Default)]
pub struct TranscriptSearchState {
    /// The query text.
    pub query: String,
    /// Cursor position (byte offset) inside [`Self::query`].
    pub query_cursor: usize,
    /// Whether matching is case-sensitive.
    pub case_sensitive: bool,
    /// Index of the selected hit.
    pub selected: usize,
    /// A step (next/previous) the user asked for while the hit index was
    /// still one frame behind the query. Applied once the renderer refreshes
    /// the index.
    pub pending_step: i64,
    /// Renderer-owned hit index.
    pub index: RefCell<TranscriptSearchIndex>,
}

impl TranscriptSearchState {
    /// Reset the query and the selection, keeping the case-sensitivity mode.
    pub fn reset_query(&mut self) {
        self.query.clear();
        self.query_cursor = 0;
        self.selected = 0;
        self.pending_step = 0;
        self.index.borrow_mut().matches.clear();
    }
}

/// Render snapshot of the find-in-page overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSearchView {
    /// The query text.
    pub query: String,
    /// Whether matching is case-sensitive.
    pub case_sensitive: bool,
    /// Index of the selected hit.
    pub selected: usize,
    /// Total number of hits.
    pub match_count: usize,
    /// Absolute transcript line of the selected hit, when there is one.
    pub current_line: Option<usize>,
}

/// Fold a character for case-insensitive comparison.
///
/// Uses the *first* character of the Unicode lowercase mapping, which keeps
/// the fold symmetric: `İ` and `i` both fold to `i`, so a query never matches
/// one spelling and not the other. Full lowercase mapping can change a
/// character's length (`İ` → `i` + combining dot), which is exactly the kind
/// of asymmetry that makes naive case-insensitive search miss hits.
fn fold_char(ch: char, case_sensitive: bool) -> char {
    if case_sensitive {
        ch
    } else if ch.is_ascii() {
        // ASCII folding is a byte operation; the general Unicode path allocates
        // an iterator and is far too hot for a per-character scan.
        ch.to_ascii_lowercase()
    } else {
        ch.to_lowercase().next().unwrap_or(ch)
    }
}

/// Find every occurrence of `query` in `text`.
///
/// Returns the display-column range of each hit. Overlapping hits are
/// reported (a query of `aa` matches `aaa` twice), matching the behaviour of
/// browser find-in-page.
pub fn find_line_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if query.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let needle: Vec<char> = query.chars().collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.len() < needle.len() {
        return Vec::new();
    }

    // Prefix display widths let a hit's column range be computed in O(1)
    // instead of re-measuring the line prefix for every hit.
    let mut prefix_widths: Vec<usize> = Vec::with_capacity(chars.len() + 1);
    let mut width = 0usize;
    for &(_, ch) in &chars {
        prefix_widths.push(width);
        width += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    prefix_widths.push(width);

    let folded_needle: Vec<char> = needle
        .iter()
        .map(|ch| fold_char(*ch, case_sensitive))
        .collect();
    // Fold the haystack once instead of once per comparison: the scan is
    // O(line × query), and folding is the expensive half.
    let folded: Vec<char> = chars
        .iter()
        .map(|&(_, ch)| fold_char(ch, case_sensitive))
        .collect();

    let mut hits = Vec::new();
    let mut start = 0usize;
    while start + needle.len() <= chars.len() {
        let matched = folded[start..start + needle.len()]
            .iter()
            .enumerate()
            .all(|(offset, ch)| *ch == folded_needle[offset]);
        if matched {
            hits.push((prefix_widths[start], prefix_widths[start + needle.len()]));
        }
        start += 1;
    }
    hits
}

/// Scan the prepared transcript and collect every hit of `query`.
///
/// `base_line` is the absolute line index of the first scanned line, so the
/// returned hit positions are absolute transcript coordinates.
pub fn scan_lines<'a>(
    lines: impl IntoIterator<Item = (usize, &'a str)>,
    query: &str,
    case_sensitive: bool,
) -> Vec<TranscriptSearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (abs_line, text) in lines {
        for (start_col, end_col) in find_line_matches(text, query, case_sensitive) {
            matches.push(TranscriptSearchMatch {
                abs_line,
                start_col,
                end_col,
            });
        }
    }
    matches
}

/// Recompute the render-side index for the active query, if it is stale.
///
/// The renderer calls this once per frame while a search is open; the scan is
/// skipped entirely when neither the query nor the transcript changed, so an
/// idle TUI does not rescan on every frame.
pub fn refresh_index(
    index: &RefCell<TranscriptSearchIndex>,
    query: &str,
    case_sensitive: bool,
    prepared: &PreparedChatFrame,
) {
    let signature = TranscriptSearchSignature::for_frame(query, case_sensitive, prepared);
    {
        let current = index.borrow();
        if current.is_current_for(&signature) {
            return;
        }
    }

    let matches = match prepared
        .sections
        .iter()
        .find(|section| section.kind == PreparedSectionKind::Body)
    {
        Some(body) => {
            let base = body.line_start;
            scan_lines(
                body.prepared
                    .wrapped_plain_lines
                    .iter()
                    .enumerate()
                    .map(|(offset, text)| (base + offset, text.as_str())),
                query,
                case_sensitive,
            )
        }
        None => Vec::new(),
    };

    let mut current = index.borrow_mut();
    current.signature = signature;
    current.matches = matches;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_match_and_reports_display_columns() {
        let hits = find_line_matches("hello world", "world", true);
        assert_eq!(hits, vec![(6, 11)]);
    }

    #[test]
    fn matching_is_case_insensitive_by_default() {
        let hits = find_line_matches("Hello World", "world", false);
        assert_eq!(hits, vec![(6, 11)]);
        assert!(find_line_matches("Hello World", "world", true).is_empty());
    }

    #[test]
    fn reports_every_occurrence_on_a_line() {
        let hits = find_line_matches("aXbXc", "X", true);
        assert_eq!(hits, vec![(1, 2), (3, 4)]);
    }

    #[test]
    fn overlapping_occurrences_are_reported() {
        let hits = find_line_matches("aaa", "aa", true);
        assert_eq!(hits, vec![(0, 2), (1, 3)]);
    }

    #[test]
    fn wide_characters_shift_the_column_range() {
        // "日本" is two double-width cells.
        let hits = find_line_matches("日本ab", "ab", true);
        assert_eq!(hits, vec![(4, 6)]);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(find_line_matches("hello", "", true).is_empty());
        assert!(find_line_matches("", "hello", true).is_empty());
    }

    #[test]
    fn query_longer_than_the_line_does_not_match() {
        assert!(find_line_matches("ab", "abc", true).is_empty());
    }

    #[test]
    fn folding_is_symmetric_for_turkish_i() {
        // "İ" lowercases to "i" plus a combining dot; both sides must fold
        // the same way or the search would match one spelling only.
        assert_eq!(find_line_matches("İstanbul", "istanbul", false).len(), 1);
        assert_eq!(find_line_matches("istanbul", "İstanbul", false).len(), 1);
    }

    #[test]
    fn scan_lines_offsets_absolute_line_numbers() {
        let matches = scan_lines([(10, "alpha"), (11, "beta")].into_iter(), "bet", true);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].abs_line, 11);
        assert_eq!(matches[0].start_col, 0);
        assert_eq!(matches[0].end_col, 3);
    }

    #[test]
    fn signature_changes_when_the_transcript_grows() {
        let empty = PreparedChatFrame::from_sections(Vec::new());
        let signature = TranscriptSearchSignature::for_frame("x", false, &empty);
        assert_eq!(signature.source_lines, 0);
        assert_eq!(signature.source_messages, 0);
        assert_ne!(signature.query, "y");
    }
}
