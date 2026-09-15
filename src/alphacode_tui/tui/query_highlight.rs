//! Query-match highlighting shared by the `/model` browser and the `/login`
//! picker.
//!
//! Both pickers let the user type to filter a list, but neither showed *why* a
//! row survived the filter: `/login` matches on provider id, display name,
//! aliases, auth kind, status, and detail text, yet rendered only the display
//! name, and the model browser filtered on the model name without marking the
//! characters that matched. This module supplies the two primitives they need:
//!
//! - [`matched_char_indices`] — which characters of a haystack matched a query
//!   (case-insensitive substring first, then the subsequence fallback both
//!   pickers already use for typos).
//! - [`highlight_within`] — the haystack rendered as ratatui spans, with the
//!   matched characters styled differently and the match kept *visible* when
//!   the text is wider than the column it lives in (the window slides so the
//!   matched characters are on screen instead of scrolled out of view).
//!
//! Everything is char-index based and ASCII-case-insensitive on purpose:
//! `str::to_lowercase` can change a string's character count (`İ` becomes two
//! chars), which would desynchronise the indices we hand back to callers that
//! slice the original text.

use ratatui::prelude::*;
use unicode_width::UnicodeWidthChar;

/// Character indices of `text` that match `query`, or `None` when the query is
/// blank or does not match at all.
///
/// The query is split on whitespace and every term must match — the same
/// contract as the pickers' own `matches_filter`, so a highlighted row is
/// always a row the filter kept. Each term is matched as a contiguous
/// substring when possible, and otherwise as a subsequence (`haik` matches
/// `claude-haiku-3-5`), which mirrors the picker filters exactly.
pub fn matched_char_indices(text: &str, query: &str) -> Option<Vec<usize>> {
    let terms: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|term| term.chars().collect())
        .collect();
    if terms.is_empty() {
        return None;
    }

    let hay: Vec<char> = text.chars().collect();
    let mut matched: Vec<usize> = Vec::new();

    for term in terms {
        let indices = match_term(&hay, &term)?;
        for idx in indices {
            if !matched.contains(&idx) {
                matched.push(idx);
            }
        }
    }

    matched.sort_unstable();
    (!matched.is_empty()).then_some(matched)
}

/// Match a single query term against the haystack.
fn match_term(hay: &[char], term: &[char]) -> Option<Vec<usize>> {
    if term.is_empty() {
        return None;
    }

    // Prefer a contiguous, case-insensitive substring: it gives the tight,
    // readable highlight a user expects from a substring filter.
    if term.len() <= hay.len() {
        'windows: for start in 0..=hay.len() - term.len() {
            for (offset, needle_char) in term.iter().enumerate() {
                if !hay[start + offset].eq_ignore_ascii_case(needle_char) {
                    continue 'windows;
                }
            }
            return Some((start..start + term.len()).collect());
        }
    }

    // Subsequence fallback for typos and abbreviations.
    let mut out = Vec::with_capacity(term.len());
    let mut cursor = 0usize;
    for needle_char in term {
        let mut found = false;
        while cursor < hay.len() {
            let candidate = hay[cursor];
            cursor += 1;
            if candidate.eq_ignore_ascii_case(needle_char) {
                out.push(cursor - 1);
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(out)
}

/// Split `text` into runs of matched / unmatched characters.
///
/// Returns a single span when there is no query or no match, so callers can use
/// this unconditionally.
pub fn highlight(text: &str, query: &str, base: Style, hit: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let matched = matched_char_indices(text, query);
    spans_for_range(text, matched.as_deref(), 0, chars.len(), base, hit)
}

/// Like [`highlight`], but never returns more than `budget` display columns.
///
/// When `text` is wider than `budget` the returned window slides so the first
/// matched character stays on screen (a third of the budget is kept to its
/// left for context); a `…` marks each clipped end. This is what keeps
/// `openrouter/anthropic/claude-sonnet-4.5` legible in a 34-column list — the
/// generic "truncate and lose the highlight" behaviour hid the very characters
/// the user typed.
pub fn highlight_within(
    text: &str,
    query: &str,
    budget: usize,
    base: Style,
    hit: Style,
) -> Vec<Span<'static>> {
    if budget == 0 {
        return Vec::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let widths: Vec<usize> = chars.iter().map(|c| c.width().unwrap_or(0)).collect();
    let total: usize = widths.iter().sum();
    let matched = matched_char_indices(text, query);

    if total <= budget {
        return spans_for_range(text, matched.as_deref(), 0, chars.len(), base, hit);
    } // Something has to be clipped, so one column per ellipsis is reserved.
    // The anchor (first matched character) must always be visible.
    let anchor = matched
        .as_ref()
        .and_then(|m| m.first().copied())
        .unwrap_or(0);

    // Budget for actual content: subtract one col for a possible lead
    // ellipsis and one for a possible tail ellipsis.
    let content_budget = budget.saturating_sub(2);
    // Place the anchor one-third into the visible window so there's context
    // on both sides when possible.
    let lead_cols = content_budget / 3;
    // Walk backwards from anchor to find the start column that fits lead_cols.
    let mut start = anchor;
    let mut used_before = 0usize;
    while start > 0 && used_before + widths[start - 1] <= lead_cols {
        start -= 1;
        used_before += widths[start];
    }

    // Walk forwards to fill the rest of content_budget.
    let mut end = start;
    let mut used = 0usize;
    while end < chars.len() && used + widths[end] <= content_budget {
        used += widths[end];
        end += 1;
    }

    // Ensure the anchor is included even when wide chars compress the window.
    if anchor >= end && anchor < chars.len() {
        end = anchor + 1;
    }

    let lead_ellipsis = start > 0;
    let tail_ellipsis = end < chars.len();

    let mut spans = Vec::new();
    if lead_ellipsis {
        spans.push(Span::styled("…".to_string(), base));
    }
    spans.extend(spans_for_range(
        text,
        matched.as_deref(),
        start,
        end,
        base,
        hit,
    ));
    if tail_ellipsis {
        spans.push(Span::styled("…".to_string(), base));
    }
    spans
}

/// Render characters `start..end` of `text` as spans, styling every character
/// whose index is in `matched`.
fn spans_for_range(
    text: &str,
    matched: Option<&[usize]>,
    start: usize,
    end: usize,
    base: Style,
    hit: Style,
) -> Vec<Span<'static>> {
    let end = end.min(text.chars().count());
    if start >= end {
        return Vec::new();
    }

    // Byte offsets let us slice the original text without allocating per char,
    // which keeps multi-byte characters intact.
    let mut byte_offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    byte_offsets.push(text.len());

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run_start = start;
    let mut run_matched = is_matched(matched, start);

    for idx in start + 1..end {
        let current = is_matched(matched, idx);
        if current != run_matched {
            spans.push(Span::styled(
                text[byte_offsets[run_start]..byte_offsets[idx]].to_string(),
                if run_matched { hit } else { base },
            ));
            run_start = idx;
            run_matched = current;
        }
    }
    spans.push(Span::styled(
        text[byte_offsets[run_start]..byte_offsets[end]].to_string(),
        if run_matched { hit } else { base },
    ));
    spans
}

fn is_matched(matched: Option<&[usize]>, idx: usize) -> bool {
    matched.is_some_and(|indices| indices.binary_search(&idx).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn highlighted_text(spans: &[Span<'static>]) -> String {
        spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::BOLD))
            .map(|s| s.content.as_ref())
            .collect()
    }

    fn base() -> Style {
        Style::default()
    }

    fn hit() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    #[test]
    fn blank_query_matches_nothing() {
        assert!(matched_char_indices("claude-haiku", "").is_none());
        assert!(matched_char_indices("claude-haiku", "   ").is_none());
    }

    #[test]
    fn substring_match_is_case_insensitive() {
        let indices = matched_char_indices("Claude-Haiku-3-5", "haiku").expect("should match");
        assert_eq!(indices, vec![7, 8, 9, 10, 11]);
    }

    #[test]
    fn subsequence_fallback_matches_typos() {
        // `haik` is a substring of `haiku`; `cld` is not, so it exercises the
        // non-adjacent path.
        assert!(matched_char_indices("claude-3-5", "cld").is_some());
        assert!(matched_char_indices("claude-3-5", "zzz").is_none());
    }

    #[test]
    fn multi_term_query_requires_every_term() {
        assert!(matched_char_indices("claude sonnet 4.5", "claude 4").is_some());
        assert!(matched_char_indices("claude sonnet 4.5", "claude opus").is_none());
    }

    #[test]
    fn unmatched_query_returns_none_so_callers_can_explain_why() {
        assert!(matched_char_indices("claude-haiku", "gpt").is_none());
    }

    #[test]
    fn highlight_splits_matched_and_unmatched_runs() {
        let spans = highlight("claude-haiku", "haiku", base(), hit());
        assert_eq!(plain(&spans), "claude-haiku");
        assert_eq!(highlighted_text(&spans), "haiku");
    }

    #[test]
    fn highlight_without_query_is_a_single_span() {
        let spans = highlight("claude-haiku", "", base(), hit());
        assert_eq!(spans.len(), 1);
        assert_eq!(plain(&spans), "claude-haiku");
    }

    #[test]
    fn subsequence_highlight_marks_only_consumed_characters() {
        let spans = highlight("claude-3-5", "cld", base(), hit());
        assert_eq!(plain(&spans), "claude-3-5");
        assert_eq!(highlighted_text(&spans), "cld");
    }

    #[test]
    fn short_text_is_never_clipped() {
        let spans = highlight_within("gpt-5", "gpt", 20, base(), hit());
        assert_eq!(plain(&spans), "gpt-5");
        assert_eq!(highlighted_text(&spans), "gpt");
    }

    #[test]
    fn window_slides_to_keep_the_match_visible() {
        let name = "openrouter/anthropic/claude-sonnet-4.5";
        let spans = highlight_within(name, "sonnet", 24, base(), hit());
        let text = plain(&spans);
        assert!(
            text.contains("sonnet"),
            "window must keep the match on screen, got {text:?}"
        );
        assert!(
            text.starts_with('…'),
            "clipped head should be marked: {text:?}"
        );
        assert!(
            spans.iter().map(|s| s.width()).sum::<usize>() <= 24,
            "window must respect the budget"
        );
    }

    #[test]
    fn window_marks_a_clipped_tail() {
        let spans = highlight_within("claude-haiku-3-5-20241022", "claude", 12, base(), hit());
        let text = plain(&spans);
        assert!(
            text.ends_with('…'),
            "clipped tail should be marked: {text:?}"
        );
        assert!(text.starts_with("claude"));
    }

    #[test]
    fn budget_of_zero_renders_nothing() {
        assert!(highlight_within("claude", "cla", 0, base(), hit()).is_empty());
    }

    #[test]
    fn wide_characters_respect_the_column_budget() {
        // CJK glyphs are two columns wide each; four fit in eight columns.
        let spans = highlight_within("模型一二三四五六七八", "四", 7, base(), hit());
        let width: usize = spans.iter().map(|s| s.width()).sum();
        assert!(width <= 7, "wide chars overflowed the budget: {width}");
        assert!(plain(&spans).contains('四'));
    }

    #[test]
    fn unmatched_long_text_is_still_truncated_cleanly() {
        let spans = highlight_within("claude-haiku-3-5", "zzz", 8, base(), hit());
        assert!(highlighted_text(&spans).is_empty());
        assert!(spans.iter().map(|s| s.width()).sum::<usize>() <= 8);
    }
}
