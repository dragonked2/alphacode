//! Shared highlight rendering for selectable / findable text.
//!
//! One implementation for every pane that highlights a range of a display
//! line — the copy selection (chat viewport, side pane, file-diff pane,
//! full-screen overlays, and the prompt composer) and the find-in-page
//! matches — so the highlight style stays visually consistent across the UI.

use super::{accent_color, blend_color, rgb};
use ratatui::prelude::*;

pub(crate) fn selection_bg_for(base_bg: Option<Color>) -> Color {
    let fallback = rgb(32, 38, 48);
    blend_color(base_bg.unwrap_or(fallback), accent_color(), 0.34)
}

pub(crate) fn selection_fg_for(base_fg: Option<Color>) -> Option<Color> {
    base_fg.map(|fg| blend_color(fg, Color::White, 0.15))
}

/// Background of a find-in-page hit that is not the currently selected one.
///
/// Deliberately softer than the selection background: a page can hold many
/// hits, and a loud background on all of them drowns the transcript.
pub(crate) fn search_hit_bg_for(base_bg: Option<Color>) -> Color {
    let fallback = rgb(32, 38, 48);
    blend_color(base_bg.unwrap_or(fallback), Color::Rgb(214, 168, 58), 0.30)
}

/// Background of the find-in-page hit the user is currently on.
///
/// Stronger than [`search_hit_bg_for`] so "where am I" is answerable at a
/// glance even when the hit is off the centre of the viewport.
pub(crate) fn search_current_hit_bg_for(base_bg: Option<Color>) -> Color {
    let fallback = rgb(32, 38, 48);
    blend_color(base_bg.unwrap_or(fallback), Color::Rgb(240, 190, 60), 0.62)
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
        let highlighted_style = resolve(&span.style);
        for ch in span.content.chars() {
            let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            let selected = if width == 0 {
                col > start_col && col <= end_col
            } else {
                col < end_col && col.saturating_add(width) > start_col
            };

            let style = if selected {
                highlighted_style
            } else {
                span.style
            };

            if current_style == Some(style) {
                current_text.push(ch);
            } else {
                flush(&mut rebuilt, &mut current_text, &mut current_style);
                current_text.push(ch);
                current_style = Some(style);
            }

            col = col.saturating_add(width);
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
}
