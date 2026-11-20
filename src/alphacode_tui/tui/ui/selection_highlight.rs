//! Shared highlight rendering for selectable / findable text.
//!
//! One implementation for every pane that highlights a range of a display
//! line — the copy selection (chat viewport, side pane, file-diff pane,
//! full-screen overlays, and the prompt composer) and the find-in-page
//! matches — so the highlight style stays visually consistent across the UI.

use super::{accent_color, blend_color};
use crate::alphacode_tui::tui::TranscriptSearchMatch;
use crate::alphacode_tui_render::coalesce_line_graphemes;
use crate::alphacode_tui_style::palette::{Role, role_color};
use ratatui::prelude::*;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) fn selection_bg_for(base_bg: Option<Color>) -> Color {
    let fallback = role_color(Role::UserBg);
    blend_color(base_bg.unwrap_or(fallback), accent_color(), 0.34)
}

pub(crate) fn selection_fg_for(base_fg: Option<Color>) -> Option<Color> {
    base_fg
}

/// Background of a find-in-page hit that is not the currently selected one.
///
/// Deliberately softer than the selection background: a page can hold many
/// hits, and a loud background on all of them drowns the transcript.
pub(crate) fn search_hit_bg_for(base_bg: Option<Color>) -> Color {
    let fallback = role_color(Role::UserBg);
    blend_color(base_bg.unwrap_or(fallback), accent_color(), 0.30)
}

/// Background of the find-in-page hit the user is currently on.
///
/// Stronger than [`search_hit_bg_for`] so "where am I" is answerable at a
/// glance even when the hit is off the centre of the viewport.
pub(crate) fn search_current_hit_bg_for(base_bg: Option<Color>) -> Color {
    let fallback = role_color(Role::UserBg);
    blend_color(base_bg.unwrap_or(fallback), accent_color(), 0.62)
}

/// Apply a copy-selection highlight to a single display line between
/// `[start_col, end_col)` (display columns). Zero-width characters inherit the
/// selection of the cell they attach to; wide characters are selected when any
/// of their cells overlap the range.
pub(crate) fn highlight_line_selection(
    line: &Line<'static>,
    start_col: usize,
    end_col: usize,
) -> Line<'static> {
    highlight_line_range(line, start_col, end_col, |style| {
        let mut selected_style = style.bg(selection_bg_for(style.bg));
        if let Some(fg) = selection_fg_for(style.fg) {
            selected_style = selected_style.fg(fg);
        }
        selected_style
    })
}

/// Apply an arbitrary highlight to a display line between `[start_col,
/// end_col)` (display columns).
///
/// `resolve` turns the style of the span being highlighted into the style the
/// highlighted cells should use; it is called once per span so a resolver can
/// look at the original colours.
pub(crate) fn highlight_line_range(
    line: &Line<'static>,
    start_col: usize,
    end_col: usize,
    resolve: impl Fn(&Style) -> Style,
) -> Line<'static> {
    if end_col <= start_col {
        return line.clone();
    }
    highlight_line_cells(
        line,
        |start, end| usize::from(overlaps(start, end, start_col, end_col)),
        |style| [*style, resolve(style)],
    )
}

pub(super) fn highlight_search_hits(
    line: &Line<'static>,
    hits: &[TranscriptSearchMatch],
    selected: Option<TranscriptSearchMatch>,
) -> Line<'static> {
    let mut hits = hits
        .iter()
        .filter(|hit| hit.start_col < hit.end_col)
        .peekable();
    highlight_line_cells(
        line,
        |start, end| {
            if let Some(hit) = selected
                && overlaps(start, end, hit.start_col, hit.end_col)
            {
                return 2;
            }
            while hits
                .peek()
                .is_some_and(|hit| hit.end_col < end && hit.end_col <= start)
            {
                hits.next();
            }
            usize::from(
                hits.peek()
                    .is_some_and(|hit| overlaps(start, end, hit.start_col, hit.end_col)),
            )
        },
        |style| {
            [
                *style,
                style.bg(search_hit_bg_for(style.bg)),
                style
                    .bg(search_current_hit_bg_for(style.bg))
                    .add_modifier(Modifier::UNDERLINED),
            ]
        },
    )
}

fn overlaps(start: usize, end: usize, range_start: usize, range_end: usize) -> bool {
    range_start < range_end
        && if start == end {
            range_start < start && start <= range_end
        } else {
            start < range_end && end > range_start
        }
}

fn highlight_line_cells<const N: usize>(
    line: &Line<'static>,
    mut style_index: impl FnMut(usize, usize) -> usize,
    resolve: impl Fn(&Style) -> [Style; N],
) -> Line<'static> {
    let line = coalesce_line_graphemes(line);
    let mut rebuilt: Vec<Span<'static>> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;
    let mut col = 0usize;

    let flush = |rebuilt: &mut Vec<Span<'static>>, text: &mut String, style: &mut Option<Style>| {
        if !text.is_empty() {
            let span = match style.take() {
                Some(style) => Span::styled(std::mem::take(text), style),
                None => Span::raw(std::mem::take(text)),
            };
            rebuilt.push(span);
        }
    };

    for span in &line.spans {
        let styles = resolve(&span.style);
        for grapheme in span.content.graphemes(true) {
            let width = grapheme.width();
            let end = col.saturating_add(width);
            let style = styles[style_index(col, end)];

            if current_style == Some(style) {
                current_text.push_str(grapheme);
            } else {
                flush(&mut rebuilt, &mut current_text, &mut current_style);
                current_text.push_str(grapheme);
                current_style = Some(style);
            }

            col = end;
        }
    }

    flush(&mut rebuilt, &mut current_text, &mut current_style);

    Line {
        spans: rebuilt,
        style: line.style,
        alignment: line.alignment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_of(text: &str) -> Line<'static> {
        Line::from(Span::raw(text.to_string()))
    }

    #[test]
    fn selection_preserves_dark_text_on_light_themes() {
        let dark = Color::Rgb(32, 38, 48);
        assert_eq!(selection_fg_for(Some(dark)), Some(dark));
        assert_eq!(selection_fg_for(None), None);
    }

    #[test]
    fn range_outside_the_line_is_a_no_op() {
        let line = line_of("hello");
        let highlighted = highlight_line_range(&line, 3, 2, |_| Style::default());
        assert_eq!(highlighted.spans.len(), 1);
        assert_eq!(highlighted.spans[0].content, "hello");
    }

    #[test]
    fn range_highlights_only_the_requested_columns() {
        let line = line_of("hello");
        let highlighted = highlight_line_range(&line, 1, 3, |_| Style::default().bg(Color::Red));
        let red: Vec<&str> = highlighted
            .spans
            .iter()
            .filter(|span| span.style.bg == Some(Color::Red))
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(red, vec!["el"]);
    }

    #[test]
    fn wide_characters_are_highlighted_when_they_overlap_the_range() {
        let line = line_of("日本");
        // The first character occupies columns 0..2.
        let highlighted = highlight_line_range(&line, 1, 2, |_| Style::default().bg(Color::Red));
        assert!(
            highlighted
                .spans
                .iter()
                .any(|span| span.style.bg == Some(Color::Red)),
            "a wide character overlapping the range must be highlighted"
        );
    }

    #[test]
    fn emoji_clusters_do_not_shift_following_highlights() {
        let line = line_of("👩\u{200d}💻x");
        let highlighted = highlight_line_range(&line, 2, 3, |_| Style::default().bg(Color::Red));
        let red: String = highlighted
            .spans
            .iter()
            .filter(|span| span.style.bg == Some(Color::Red))
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(red, "x");
    }
}
