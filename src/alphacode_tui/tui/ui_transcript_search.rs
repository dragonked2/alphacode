//! The find-in-page overlay (`Ctrl+F`).
//!
//! A compact bar that floats just above the composer, mirroring the command
//! palette so the two overlays feel like the same family of surfaces: it
//! never reflows the transcript, it floats over it.
//!
//! The bar shows the query, the hit counter, and the keys that drive it. It
//! is deliberately two rows tall: the query row carries the state, the hint
//! row carries the instructions, and neither has to share space with the
//! other.

use ratatui::{prelude::*, widgets::Paragraph};

use super::super::TuiState;
use super::super::brand_ux::BrandTheme;

/// Only the test-only `counter_text` helper below needs the view type, so the
/// import is gated to avoid an unused-import warning in normal builds.
#[cfg(test)]
use crate::alphacode_tui::tui::transcript_search::TranscriptSearchView;

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

    // Row 1: the query, the case-sensitivity marker, and the hit counter.
    let mut query_spans: Vec<Span<'static>> = vec![
        Span::styled("Find ", accent),
        Span::styled(view.query.clone(), Style::default()),
    ];
    if view.case_sensitive {
        query_spans.push(Span::styled(
            " Aa",
            Style::default().fg(BrandTheme::warning()),
        ));
    }
    query_spans.push(Span::styled("█", accent));

    let counter = if view.match_count == 0 {
        if view.query.is_empty() {
            "type to search".to_string()
        } else {
            "no matches".to_string()
        }
    } else {
        format!("{}/{}", view.selected + 1, view.match_count)
    };
    let counter_style = if view.match_count == 0 && !view.query.is_empty() {
        Style::default().fg(BrandTheme::warning())
    } else {
        dim
    };

    let mut query_line = Line::from(query_spans);
    let centered = app.centered_mode();
    let used = query_line.width();
    let counter_width = unicode_width::UnicodeWidthStr::width(counter.as_str());
    if !centered && used + counter_width + 2 <= rect.width as usize {
        // Right-align the counter at the edge of the bar.
        let pad = rect.width as usize - used - counter_width;
        query_line.spans.push(Span::raw(" ".repeat(pad)));
        query_line.spans.push(Span::styled(counter, counter_style));
    } else {
        // Centred layout (or a bar too narrow to right-align): the counter
        // follows the query and the whole line is centred.
        query_line.spans.push(Span::styled("  ", dim));
        query_line.spans.push(Span::styled(counter, counter_style));
    }
    lines.push(query_line);

    // Row 2: the keys that drive the overlay.
    lines.push(Line::from(vec![
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
    ]));

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
    None
}

/// Render-friendly helper used by the tests: the counter text for a view.
#[cfg(test)]
pub(super) fn counter_text(view: &TranscriptSearchView) -> String {
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
    use crate::alphacode_tui::tui::transcript_search::TranscriptSearchView;
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
}
