use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Maximum content width in centered mode. Wide terminals center content at
/// this width rather than stretching messages across the full display, which
/// keeps line lengths readable without a horizontal scan.
pub const MAX_CENTERED_CONTENT_WIDTH: usize = 96;

pub fn clear_area(frame: &mut Frame, area: Rect) {
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buffer[(x, y)].reset();
        }
    }
}

/// Fill an area with a background color without touching existing symbols.
/// Used to give panels a subtle tinted background that reads as a distinct
/// surface without overwhelming the content.
pub fn fill_background(frame: &mut Frame, area: Rect, color: Color) {
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            if cell.symbol() == " " || cell.symbol().is_empty() {
                cell.set_bg(color);
            }
        }
    }
}

/// Draw a thin gradient separator line across the full width of `area`.
/// The gradient fades from `color` at the left to dim at the right, giving
/// a subtle visual separation between sections without a hard border.
pub fn draw_gradient_separator(frame: &mut Frame, area: Rect, color: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let buffer = frame.buffer_mut();
    let steps = area.width as usize;
    for (i, x) in (area.left()..area.right()).enumerate() {
        let t = i as f32 / steps.max(1) as f32;
        let cell = &mut buffer[(x, area.y)];
        let blended = blend_color(color, Color::Reset, t);
        cell.set_symbol("─");
        cell.set_fg(blended);
    }
}

/// Blend two colors by `t` (0.0 = `from`, 1.0 = `to`). Used for gradient
/// separators and subtle visual transitions.
pub fn blend_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
            let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;
            Color::Rgb(r, g, b)
        }
        _ => from,
    }
}

/// Draw a content frame with a subtle background tint and optional border.
/// This is the modern replacement for bare `Block::default()` panels — it
/// gives every content area a consistent, polished surface treatment.
pub fn draw_content_frame(frame: &mut Frame, area: Rect, border_color: Color, bg_color: Color) {
    fill_background(frame, area, bg_color);
    let block = Block::default()
        .borders(Borders::NONE)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(block, area);
}

pub fn left_aligned_content_inset(width: u16, centered: bool) -> u16 {
    if centered || width <= 1 { 0 } else { 1 }
}

pub fn centered_content_block_width(width: u16, max_width: usize) -> usize {
    (width as usize).min(max_width).max(1)
}

pub fn left_pad_lines_to_block_width(lines: &mut [Line<'static>], width: u16, block_width: usize) {
    let block_width = block_width.min(width as usize);
    let pad = (width as usize).saturating_sub(block_width) / 2;
    for line in lines {
        if pad > 0 {
            line.spans.insert(0, Span::raw(" ".repeat(pad)));
        }
        line.alignment = Some(Alignment::Left);
    }
}

const RIGHT_RAIL_HEADER_HEIGHT: u16 = 1;

pub fn right_rail_border_style(focused: bool, focus_color: Color, dim_color: Color) -> Style {
    let border_color = if focused { focus_color } else { dim_color };
    Style::default().fg(border_color)
}

fn right_rail_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::LEFT).inner(area)
}

fn right_rail_content_area(area: Rect) -> Option<Rect> {
    let inner = right_rail_inner(area);
    if inner.width == 0 || inner.height <= RIGHT_RAIL_HEADER_HEIGHT {
        return None;
    }

    Some(Rect {
        x: inner.x,
        y: inner.y + RIGHT_RAIL_HEADER_HEIGHT,
        width: inner.width,
        height: inner.height - RIGHT_RAIL_HEADER_HEIGHT,
    })
}

pub fn draw_right_rail_chrome(
    frame: &mut Frame,
    area: Rect,
    title: Line<'static>,
    border_style: Style,
) -> Option<Rect> {
    let inner = right_rail_inner(area);
    let content_area = right_rail_content_area(area)?;

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(border_style);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(title),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: RIGHT_RAIL_HEADER_HEIGHT,
        },
    );

    Some(content_area)
}

/// Set alignment on a line only if it doesn't already have one set.
/// This allows markdown rendering to mark code blocks as left-aligned while
/// other content inherits the default alignment (e.g., centered mode).
pub fn align_if_unset(line: Line<'static>, align: Alignment) -> Line<'static> {
    if line.alignment.is_some() {
        line
    } else {
        line.alignment(align)
    }
}
