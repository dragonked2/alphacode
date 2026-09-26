use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub(crate) fn adapt_buffer_for_emoji_preference(buffer: &mut Buffer) {
    adapt_buffer_for_emoji_enabled(buffer, crate::output_style::emoji_enabled());
}

/// Region-scoped [`adapt_buffer_for_emoji_preference`] for partial repaints:
/// only the cells the caller rewrote are re-encoded, so the rest of a reused
/// buffer keeps its already-adapted symbols.
pub(crate) fn adapt_region_for_emoji_preference(buffer: &mut Buffer, area: Rect) {
    if crate::output_style::emoji_enabled() {
        return;
    }
    let area = area.intersection(*buffer.area());
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            if let std::borrow::Cow::Owned(symbol) =
                crate::output_style::terminal_text_with_emoji(cell.symbol(), false)
            {
                cell.set_symbol(&symbol);
            }
        }
    }
}

fn adapt_buffer_for_emoji_enabled(buffer: &mut Buffer, enabled: bool) {
    if enabled {
        return;
    }
    for cell in &mut buffer.content {
        if let std::borrow::Cow::Owned(symbol) =
            crate::output_style::terminal_text_with_emoji(cell.symbol(), enabled)
        {
            cell.set_symbol(&symbol);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{layout::Rect, style::Style};

    #[test]
    fn no_emoji_mode_rewrites_completed_frame_cells_to_ascii() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 24, 1));
        buffer.set_string(0, 0, "🐝 ready ✅ box ─", Style::default());
        adapt_buffer_for_emoji_enabled(&mut buffer, false);
        let rendered = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(
            rendered.split_whitespace().collect::<Vec<_>>(),
            vec!["*", "ready", "+", "box", "─"]
        );
        assert!(!rendered.contains(['🐝', '✅']));
    }
}
