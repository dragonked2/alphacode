//! The find-in-page overlay (`Ctrl+F`).
//!
//! A compact bar that floats just above the composer, mirroring the command
//! palette so the two overlays feel like the same family of surfaces: it
//! never reflows the transcript, it floats over it.
//!
//! The bar shows the query, the hit counter, and the keys that drive it. It
//! uses a second row for shortcuts when there is room.

use ratatui::{prelude::*, widgets::Paragraph};

use super::super::TuiState;
use super::super::brand_ux::BrandTheme;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::alphacode_tui::tui::transcript_search::TranscriptSearchView;
use crate::alphacode_tui_tool_display::{truncate_head_display, truncate_tail_display};

/// Rows the overlay occupies.
const OVERLAY_HEIGHT: u16 = 2;

/// Draw the find-in-page overlay, if a search is open.
///
/// `input_area` is the composer chunk; the bar is placed directly above it
/// (or below it, when there is no room above) so it stays next to the thing
/// the user is typing into.
pub(super) fn draw_transcript_search_overlay(
    frame: &mut Frame,
    app: &dyn TuiState,
    input_area: Rect,
) {
    let Some(view) = app.transcript_search() else {
        return;
    };
    let Some(rect) = overlay_rect(input_area, frame.area()) else {
        return;
    };

    let accent = Style::default().fg(BrandTheme::accent());
    let dim = Style::default().fg(BrandTheme::dim());
    let muted = Style::default().fg(BrandTheme::dim_bright());

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(OVERLAY_HEIGHT as usize);

    let width = usize::from(rect.width);
    let counter = truncate_tail_display(&counter_text(&view), width.saturating_sub(1));
    let counter_style = if view.match_count == 0 && !view.query.is_empty() {
        Style::default().fg(BrandTheme::warning())
    } else {
        dim
    };

    // Reserve space for the result count first; narrow terminals should still
    // explain whether the search found anything, even for a long query.
    let label = if width >= counter.width() + 7 {
        "Find "
    } else {
        ""
    };
    let case_marker = if view.case_sensitive && width >= counter.width() + label.len() + 6 {
        " Aa"
    } else {
        ""
    };
    let separator = if width >= counter.width() + label.len() + case_marker.len() + 3 {
        "  "
    } else {
        ""
    };
    let reserved = label.len() + case_marker.len() + separator.len() + counter.width();
    let query_width = width.saturating_sub(reserved);
    let cursor = app.transcript_search_cursor().unwrap_or(view.query.len());
    let (before, after) = fit_query_at_cursor(&view.query, cursor, query_width);
    let cursor_col = label.len() + before.width();
    let mut query_line = Line::from(vec![
        Span::styled(label, accent),
        Span::raw(before),
        Span::raw(after),
        Span::styled(case_marker, Style::default().fg(BrandTheme::warning())),
    ]);
    let centered = app.centered_mode();
    let used = query_line.width();
    if !centered && used + counter.width() + separator.len() <= width {
        let pad = width - used - counter.width();
        query_line.spans.push(Span::raw(" ".repeat(pad)));
    } else {
        query_line.spans.push(Span::styled(separator, dim));
    }
    query_line.spans.push(Span::styled(counter, counter_style));
    let cursor_col = if centered {
        cursor_col
            + usize::from(super::input_ui::centered_line_offset(
                rect.width,
                query_line.width() as u16,
            ))
    } else {
        cursor_col
    };
    lines.push(query_line);

    // Row 2: the keys that drive the overlay.
    lines.push(if rect.width < 24 {
        Line::from(vec![
            Span::styled("Esc", muted),
            Span::styled(" close", dim),
        ])
    } else if rect.width < 55 {
        Line::from(vec![
            Span::styled("↑↓", muted),
            Span::styled(" navigate  ", dim),
            Span::styled("Esc", muted),
            Span::styled(" close", dim),
        ])
    } else {
        Line::from(vec![
            Span::styled("↑↓", muted),
            Span::styled(" next/prev  ", dim),
            Span::styled("⏎", muted),
            Span::styled(" next  ", dim),
            Span::styled("⇧⏎", muted),
            Span::styled(" prev  ", dim),
            Span::styled("Alt+C", muted),
            Span::styled(" case  ", dim),
            Span::styled("Esc", muted),
            Span::styled(" close", dim),
        ])
    });

    if centered {
        // Match the command palette: in centered mode the overlay is centred
        // too, so the bar reads as part of the centred column rather than
        // sticking out to the left of it.
        lines = lines
            .into_iter()
            .map(|line| line.alignment(Alignment::Center))
            .collect();
    }

    frame.render_widget(ratatui::widgets::Clear, rect);
    frame.render_widget(Paragraph::new(lines), rect);
    frame.set_cursor_position(Position::new(
        rect.x + cursor_col.min(width - 1) as u16,
        rect.y,
    ));
}

fn fit_query_at_cursor(query: &str, cursor: usize, width: usize) -> (String, String) {
    let cursor = super::super::core::ceil_char_boundary(query, cursor);
    let (before, after) = query.split_at(cursor);
    let mut graphemes = after.graphemes(true);
    let next_width = graphemes.next().map_or(0, UnicodeWidthStr::width);
    let right_reserve = next_width + usize::from(graphemes.next().is_some());
    let text_width = width.saturating_sub(1);
    let before = truncate_head_display(before, text_width.saturating_sub(right_reserve));
    let mut after = truncate_tail_display(after, text_width - before.width());
    // Keep the end-of-query caret cell reserved while the caret moves, so
    // centered mode does not shift the rest of the overlay by a column.
    if width > 0 {
        after.push(' ');
    }
    (before, after)
}

/// Place the overlay next to the composer.
///
/// Preferred position is directly above the composer (over the transcript
/// tail). When the composer sits at the very top of the screen there is no
/// room above, so the bar flips below it.
fn overlay_rect(input_area: Rect, frame_area: Rect) -> Option<Rect> {
    if input_area.width == 0 {
        return None;
    }
    let space_above = input_area.y.saturating_sub(frame_area.y);
    if space_above >= OVERLAY_HEIGHT {
        return Some(Rect::new(
            input_area.x,
            input_area.y - OVERLAY_HEIGHT,
            input_area.width,
            OVERLAY_HEIGHT,
        ));
    }
    let below_start = input_area.y.saturating_add(input_area.height);
    let space_below = frame_area.bottom().saturating_sub(below_start);
    if space_below >= OVERLAY_HEIGHT {
        return Some(Rect::new(
            input_area.x,
            below_start,
            input_area.width,
            OVERLAY_HEIGHT,
        ));
    }
    if space_above > 0 {
        Some(Rect::new(
            input_area.x,
            input_area.y - 1,
            input_area.width,
            1,
        ))
    } else if space_below > 0 {
        Some(Rect::new(input_area.x, below_start, input_area.width, 1))
    } else {
        None
    }
}

fn counter_text(view: &TranscriptSearchView) -> String {
    if view.match_count == 0 {
        if view.query.is_empty() {
            "type to search".to_string()
        } else {
            "no matches".to_string()
        }
    } else {
        format!("{}/{}", view.selected + 1, view.match_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(query: &str, matches: usize, selected: usize) -> TranscriptSearchView {
        TranscriptSearchView {
            query: query.to_string(),
            case_sensitive: false,
            selected,
            match_count: matches,
            current_line: None,
        }
    }

    #[test]
    fn long_query_keeps_whole_graphemes_and_tail() {
        let query = "abc界e\u{301}";
        assert_eq!(
            fit_query_at_cursor(query, query.len(), 5),
            ("…界e\u{301}".to_owned(), " ".to_owned())
        );
        assert_eq!(
            fit_query_at_cursor("abcdefgh界xy", 8, 7),
            ("…gh".to_owned(), "界… ".to_owned())
        );
    }

    #[test]
    fn counter_reports_the_position_within_the_hits() {
        assert_eq!(counter_text(&view("x", 17, 2)), "3/17");
    }

    #[test]
    fn counter_explains_an_empty_result_set() {
        assert_eq!(counter_text(&view("x", 0, 0)), "no matches");
        assert_eq!(counter_text(&view("", 0, 0)), "type to search");
    }

    #[test]
    fn overlay_sits_above_the_composer_when_there_is_room() {
        let frame_area = Rect::new(0, 0, 80, 24);
        let input_area = Rect::new(0, 18, 80, 5);
        let rect = overlay_rect(input_area, frame_area).expect("overlay rect");
        assert_eq!(rect.y, 16);
        assert_eq!(rect.height, OVERLAY_HEIGHT);
    }

    #[test]
    fn overlay_flips_below_when_the_composer_is_at_the_top() {
        let frame_area = Rect::new(0, 0, 80, 24);
        let input_area = Rect::new(0, 0, 80, 5);
        let rect = overlay_rect(input_area, frame_area).expect("overlay rect");
        assert_eq!(rect.y, 5);
    }

    #[test]
    fn overlay_is_omitted_when_there_is_no_room_at_all() {
        let frame_area = Rect::new(0, 0, 80, 3);
        let input_area = Rect::new(0, 0, 80, 3);
        assert!(overlay_rect(input_area, frame_area).is_none());
    }

    #[test]
    fn overlay_keeps_query_when_only_one_row_is_available() {
        let frame_area = Rect::new(0, 0, 24, 4);
        let input_area = Rect::new(0, 1, 24, 2);
        assert_eq!(
            overlay_rect(input_area, frame_area),
            Some(Rect::new(0, 0, 24, 1))
        );
    }
}
