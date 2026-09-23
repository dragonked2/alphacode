//! Picker spacing helpers — Phase 4b of the TUI 100x plan.
//!
//! Adds 2-cell breathing space (1 row top, 1 row bottom) around the picker
//! box so the box reads as a floating overlay instead of a continuation of
//! the chat. The breathing is opt-in: callers wrap their existing layout
//! with [`with_picker_breathing`].
//!
//! The picker is currently drawn by [`super::draw_inline_interactive`] which
//! uses tight spacing. Phase 4b adds the helpers; a follow-up commit enables
//! the breathing for the runtime model picker.

use ratatui::prelude::*;

/// Wraps `area` with one-cell breathing space on top and bottom.
///
/// ```text
///   \u250c\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2510  (1 cell breathing)
///   \u2502   /model picker contents                \u2502
///   \u2514\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2518
///   \u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7\u00b7  (1 cell breathing)
/// ```
///
/// The breathing rows are reserved as empty rectangles; the caller renders
/// them as plain chat-area cells, which keeps the overlay read as "floating"
/// rather than embedded.
///
/// Returns the original area unchanged when the screen is too small to give
/// up two cells. The minimum height for breathing is 5 rows: 1 top + 3 box +
/// 1 bottom.
pub fn with_picker_breathing(area: Rect) -> (Rect, Option<Rect>, Option<Rect>) {
    if area.height < 5 {
        return (area, None, None);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);
    (chunks[1], Some(chunks[0]), Some(chunks[2]))
}

/// Spacing weight to apply to a header row when the configured font is
/// `mono-bold`. Returns 1 for bold; 0 for the default font.
pub fn header_weight_for(font: &str) -> u16 {
    match font {
        "mono-bold" => 1,
        _ => 0,
    }
}

/// Whether the picker hint should adopt bold weight under the configured font.
pub fn bold_headers(font: &str) -> bool {
    matches!(font, "mono-bold")
}

/// Compact-column hint: `true` when the configured font is `compact`. The
/// renderer uses this to subtract one cell from each picker column budget.
pub fn compact_columns(font: &str) -> bool {
    matches!(font, "compact")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breathing_passthrough_for_small_areas() {
        let area = Rect::new(0, 0, 80, 3);
        let (center, top, bottom) = with_picker_breathing(area);
        assert_eq!(center, area);
        assert!(top.is_none());
        assert!(bottom.is_none());
    }

    #[test]
    fn breathing_splits_a_tall_area() {
        let area = Rect::new(0, 0, 80, 20);
        let (center, top, bottom) = with_picker_breathing(area);
        assert_eq!(center.height, 18);
        assert!(top.is_some());
        assert!(bottom.is_some());
        assert_eq!(top.unwrap().height, 1);
        assert_eq!(bottom.unwrap().height, 1);
    }

    #[test]
    fn header_weight_only_for_bold() {
        assert_eq!(header_weight_for("mono"), 0);
        assert_eq!(header_weight_for("mono-bold"), 1);
        assert_eq!(header_weight_for("compact"), 0);
        assert_eq!(header_weight_for("dyslexic"), 0);
    }

    #[test]
    fn bold_headers_predicate() {
        assert!(bold_headers("mono-bold"));
        assert!(!bold_headers("mono"));
    }

    #[test]
    fn compact_columns_predicate() {
        assert!(compact_columns("compact"));
        assert!(!compact_columns("mono"));
    }

    #[test]
    fn breathing_keeps_x_position() {
        let area = Rect::new(5, 5, 80, 20);
        let (center, _, _) = with_picker_breathing(area);
        assert_eq!(center.x, 5);
        assert_eq!(center.y, 6); // 1 cell down
    }
}
