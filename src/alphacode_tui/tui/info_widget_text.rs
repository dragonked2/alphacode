//! Truncation helpers for info-widget rows.
//!
//! Every function here is measured in **terminal columns**, not `char`s. The
//! two differ for CJK, emoji, and box-drawing content, and the widgets budget
//! their rows in columns — so a `char`-counting truncator silently overflows
//! the panel and ratatui clips the row mid-glyph. The invariant each function
//! upholds is: the returned string's display width is never greater than the
//! budget it was given.

use unicode_width::UnicodeWidthStr;

#[cfg(test)]
use ratatui::text::Span;

/// The ellipsis used when there is room for it. Three columns.
const ELLIPSIS: &str = "...";
/// The single-column ellipsis, for budgets too tight for `ELLIPSIS`.
const NARROW_ELLIPSIS: &str = "…";

/// Longest prefix of `s` that fits in `max_width` columns.
pub(super) fn truncate_width(s: &str, max_width: usize) -> &str {
    if s.width() <= max_width {
        return s;
    }
    let mut used = 0usize;
    for (index, ch) in s.char_indices() {
        let ch_width = ch.to_string().width();
        if used + ch_width > max_width {
            return &s[..index];
        }
        used += ch_width;
    }
    s
}

/// Truncate to `max_len` columns, preferring to cut at a word boundary.
///
/// Falls back to a mid-word cut when the nearest boundary would discard more
/// than half the available room, since a two-character label is less useful
/// than a clipped word.
pub(super) fn truncate_smart(s: &str, max_len: usize) -> String {
    if s.width() <= max_len {
        return s.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    // No room for content plus an ellipsis: signal the elision alone rather
    // than returning a wider string than was asked for.
    if max_len < ELLIPSIS.width() + 1 {
        return NARROW_ELLIPSIS.to_string();
    }

    let target = max_len - ELLIPSIS.width();
    let prefix = truncate_width(s, target);

    if let Some(pos) = prefix.rfind(' ') {
        let before = &prefix[..pos];
        if before.width() > target / 2 {
            return format!("{before}{ELLIPSIS}");
        }
    }
    format!("{prefix}{ELLIPSIS}")
}

/// Longest prefix of `s` containing at most `max_chars` `char`s.
///
/// Prefer [`truncate_width`] for anything sized against a panel; this remains
/// for callers that genuinely count characters rather than columns.
pub(super) fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Total display width of a rendered row.
///
/// Widgets reserve room for their suffixes before truncating the content, and
/// a reservation that disagrees with what is actually pushed lets the row
/// overflow. This is how those two are held to the same number.
#[cfg(test)]
pub(super) fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

/// Truncate to `max_width` columns using the single-column `…`.
pub(super) fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if s.width() <= max_width {
        return s.to_string();
    }
    if max_width == 1 {
        return NARROW_ELLIPSIS.to_string();
    }
    format!(
        "{}{NARROW_ELLIPSIS}",
        truncate_width(s, max_width - NARROW_ELLIPSIS.width())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant the whole module exists for. A row that exceeds its
    /// budget is clipped by ratatui, which can cut a wide glyph in half.
    #[test]
    fn nothing_ever_exceeds_its_budget() {
        let samples = [
            "",
            "a",
            "hello world",
            "a much longer sentence that will certainly need truncating",
            "日本語のテキストはとても幅が広いです",
            "mixed 日本語 and latin text",
            "🎉🎉🎉 emoji run 🎉🎉🎉",
            "   leading and trailing   ",
            "no-spaces-at-all-in-this-one-single-long-token",
        ];
        for sample in samples {
            for budget in 0..=40usize {
                for rendered in [
                    truncate_smart(sample, budget),
                    truncate_with_ellipsis(sample, budget),
                    truncate_width(sample, budget).to_string(),
                ] {
                    assert!(
                        rendered.width() <= budget,
                        "{sample:?} at budget {budget} rendered {} columns: {rendered:?}",
                        rendered.width()
                    );
                }
            }
        }
    }

    /// The bug: a 3-column `"..."` was returned for budgets of 0, 1, and 2,
    /// so the narrowest panels were exactly the ones that overflowed.
    #[test]
    fn a_budget_too_small_for_an_ellipsis_does_not_render_one() {
        assert_eq!(truncate_smart("hello", 0), "");
        assert_eq!(truncate_smart("hello", 1), "…");
        assert_eq!(truncate_smart("hello", 2), "…");
        assert_eq!(truncate_smart("hello", 3), "…");
    }

    #[test]
    fn text_that_already_fits_is_returned_untouched() {
        assert_eq!(truncate_smart("hello", 5), "hello");
        assert_eq!(truncate_smart("hello", 99), "hello");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
        assert_eq!(truncate_width("hello", 5), "hello");
    }

    #[test]
    fn truncation_prefers_a_word_boundary() {
        assert_eq!(truncate_smart("hello brave world", 14), "hello brave...");
    }

    /// Cutting at the boundary here would leave "a...", which says less than a
    /// clipped word does.
    #[test]
    fn a_boundary_that_discards_too_much_is_ignored() {
        let out = truncate_smart("a verylongsingletoken", 12);
        assert_eq!(out, "a verylon...");
    }

    /// Width, not `char` count: each of these is two columns wide, so only
    /// three of them fit in the seven columns left after the ellipsis.
    #[test]
    fn wide_characters_are_measured_in_columns() {
        let out = truncate_smart("日本語のテキスト", 10);
        assert_eq!(out, "日本語...");
        assert_eq!(
            out.width(),
            9,
            "never splits a wide glyph to hit the budget"
        );
    }

    /// A wide glyph that would straddle the boundary is dropped rather than
    /// half-rendered, so the result can come in a column under budget.
    #[test]
    fn a_wide_glyph_is_never_split() {
        assert_eq!(truncate_width("日本語", 3), "日");
        assert_eq!(truncate_width("日本語", 1), "");
    }

    #[test]
    fn the_narrow_ellipsis_leaves_room_for_content() {
        assert_eq!(truncate_with_ellipsis("hello", 3), "he…");
        assert_eq!(truncate_with_ellipsis("hello", 1), "…");
        assert_eq!(truncate_with_ellipsis("hello", 0), "");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(truncate_smart("", 0), "");
        assert_eq!(truncate_smart("", 10), "");
        assert_eq!(truncate_with_ellipsis("", 10), "");
    }

    /// Truncation is monotone: a wider panel never shows less.
    #[test]
    fn a_wider_budget_never_shows_less() {
        let sample = "a reasonably long sentence for checking monotonicity";
        for budget in 1..40usize {
            let narrow = truncate_smart(sample, budget).width();
            let wide = truncate_smart(sample, budget + 1).width();
            assert!(
                wide >= narrow,
                "budget {budget} rendered {narrow} columns but {} rendered {wide}",
                budget + 1
            );
        }
    }

    #[test]
    fn truncate_chars_still_counts_characters() {
        assert_eq!(truncate_chars("日本語", 2), "日本");
        assert_eq!(truncate_chars("hello", 99), "hello");
    }
}
