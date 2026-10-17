//! Modern visual polish helpers — gradient separators, glass-effect surfaces,
//! animated accent borders, and enhanced content frames.
//!
//! These helpers build on the existing design-token system
//! (`crate::alphacode_tui_style::tokens`) to add subtle, modern visual
//! refinements: gradient divider lines between sections, softly tinted
//! backgrounds for content panels, and a pulsing accent border that gives
//! focused surfaces a "alive" feel without being distracting.
//!
//! All functions are purely additive — they never remove or alter existing
//! content, only enhance the visual treatment around it.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Block,
};

use crate::alphacode_tui::tui::brand_ux::BrandTheme;

// ---------------------------------------------------------------------------
// Gradient separators
// ---------------------------------------------------------------------------

/// A horizontal gradient separator that fades from `color` (left) to dim
/// (right). More visually interesting than a flat `────────` line because
/// the eye is led from the content above into the content below.
///
/// The separator uses `─` block characters with a smooth color ramp.
pub fn gradient_separator_line(width: usize, color: Color) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    let chars: Vec<char> = "─".repeat(width).chars().collect();
    let dim = BrandTheme::dim();
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(chars.len());
    for (i, ch) in chars.iter().enumerate() {
        let t = i as f32 / width.max(1) as f32;
        let blended = blend_toward(dim, color, 1.0 - t);
        spans.push(Span::styled(ch.to_string(), Style::default().fg(blended)));
    }
    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Glass-effect surfaces
// ---------------------------------------------------------------------------

/// Render a "glass" surface — a very subtle background tint with a thin
/// accent-colored left edge. Used to visually distinguish a content panel
/// from the main transcript without the heavy look of a full border.
///
/// The left edge is 1 cell wide in the accent color; the rest of the area
/// gets a barely-perceptible darkening.
pub fn glass_surface(frame: &mut ratatui::Frame, area: Rect, accent: Color, bg_tint: Color) {
    use ratatui::widgets::Widget;
    // Fill the area with the subtle tint
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            if cell.symbol() == " " || cell.symbol().is_empty() {
                cell.set_bg(bg_tint);
            }
        }
    }
    // Draw the left accent edge
    if area.width >= 2 {
        let edge = Rect {
            x: area.x,
            y: area.y,
            width: 1,
            height: area.height,
        };
        let block = Block::default().style(Style::default().fg(accent));
        Widget::render(block, edge, buffer);
    }
}

// ---------------------------------------------------------------------------
// Animated accent border
// ---------------------------------------------------------------------------

/// Draw a border that pulses between the accent color and a dimmed version
/// of it. The pulse is driven by `elapsed` seconds and completes one cycle
/// every `cycle_secs` seconds.
///
/// This gives focused surfaces (modals, input areas, side panels) a subtle
/// "alive" quality without the harshness of a blinking cursor or spinner.
pub fn animated_accent_border(_area: Rect, accent: Color, elapsed: f32, cycle_secs: f32) -> Style {
    let phase = (elapsed / cycle_secs.max(0.1)) % 1.0;
    let pulse = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5; // 0..1
    let dim = BrandTheme::dim();
    let blended = blend_toward(dim, accent, 0.3 + 0.7 * pulse);
    Style::default().fg(blended)
}

// ---------------------------------------------------------------------------
// Helper: color blending
// ---------------------------------------------------------------------------

/// Blend `from` toward `by` by amount `t` (0.0 = `from`, 1.0 = `by`).
fn blend_toward(from: Color, by: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (from, by) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
            let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;
            Color::Rgb(r, g, b)
        }
        _ => from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_separator_line_respects_width() {
        let line = gradient_separator_line(20, Color::Rgb(100, 150, 200));
        assert_eq!(line.spans.len(), 20);
    }

    #[test]
    fn gradient_separator_line_empty_for_zero_width() {
        let line = gradient_separator_line(0, Color::Rgb(100, 150, 200));
        assert!(line.spans.is_empty());
    }

    #[test]
    fn blend_toward_endpoints() {
        let from = Color::Rgb(100, 100, 100);
        let to = Color::Rgb(200, 200, 200);
        assert_eq!(blend_toward(from, to, 0.0), from);
        assert_eq!(blend_toward(from, to, 1.0), to);
    }

    #[test]
    fn animated_accent_border_returns_style() {
        let style =
            animated_accent_border(Rect::new(0, 0, 80, 24), Color::Rgb(100, 150, 200), 0.5, 2.0);
        // Just verify it doesn't panic and returns a valid style
        let _ = style.fg;
    }
}
