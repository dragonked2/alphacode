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
use crate::alphacode_tui::tui::color_support::rgb;

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

/// A shorter, centered gradient separator for use inside cards or narrow
/// panels. Renders `──  gradient  ──` with the accent color at the center
/// fading to dim at the edges.
pub fn centered_gradient_separator(width: usize, color: Color) -> Line<'static> {
    if width < 4 {
        return gradient_separator_line(width, color);
    }
    let inner = width.saturating_sub(4);
    let half = inner / 2;
    let left = gradient_segment(half, color, true);
    let right = gradient_segment(inner - half, color, false);
    let mut spans = vec![Span::styled("  ", Style::default())];
    spans.extend(left);
    spans.push(Span::styled("  ", Style::default()));
    spans.extend(right);
    Line::from(spans)
}

/// Build one side of a centered gradient separator.
/// `from_center` = true for the left side (bright near center),
/// false for the right side (bright near center, fading right).
fn gradient_segment(count: usize, color: Color, from_center: bool) -> Vec<Span<'static>> {
    let dim = BrandTheme::dim();
    let mut spans = Vec::with_capacity(count);
    for i in 0..count {
        let t = i as f32 / count.max(1) as f32;
        let blend_t = if from_center { t } else { 1.0 - t };
        let blended = blend_toward(dim, color, blend_t);
        spans.push(Span::styled("─", Style::default().fg(blended)));
    }
    spans
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

/// A more pronounced glass surface with a full thin border and tinted
/// background. Used for side panels and pinned content panes.
pub fn glass_panel(frame: &mut ratatui::Frame, area: Rect, border_color: Color, bg_tint: Color) {
    use ratatui::widgets::Widget;
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            if cell.symbol() == " " || cell.symbol().is_empty() {
                cell.set_bg(bg_tint);
            }
        }
    }
    let block = Block::default()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(border_color));
    Widget::render(block, area, buffer);
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
// Enhanced content frame
// ---------------------------------------------------------------------------

/// A modern content frame: rounded border with a subtle top accent line.
/// The top border uses the accent color while the other three sides use
/// a dimmer color, creating a visual "light from above" effect.
pub fn modern_content_frame(_area: Rect, accent: Color, border_dim: Color) -> Block<'static> {
    let top_style = Style::default().fg(accent);
    let side_style = Style::default().fg(border_dim);

    Block::default()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(side_style)
        .title_top(
            Line::from(Span::styled("━", top_style)).alignment(ratatui::layout::Alignment::Left),
        )
        .title_top(
            Line::from(Span::styled("━", top_style)).alignment(ratatui::layout::Alignment::Right),
        )
}

// ---------------------------------------------------------------------------
// Status line background
// ---------------------------------------------------------------------------

/// A very subtle gradient background for status lines. Darker at the
/// left, slightly lighter at the right — creates a sense of depth.
pub fn status_line_background(width: usize) -> Vec<Span<'static>> {
    let base = rgb(18, 20, 28);
    let lighter = rgb(24, 27, 36);
    let mut spans = Vec::with_capacity(width);
    for i in 0..width {
        let t = i as f32 / width.max(1) as f32;
        let blended = blend_toward(base, lighter, t);
        spans.push(Span::styled(" ", Style::default().bg(blended)));
    }
    spans
}

// ---------------------------------------------------------------------------
// Input area treatment
// ---------------------------------------------------------------------------

/// Draw a subtle top border above the input area that separates it from
/// the transcript. Uses a gradient from dim (left) to accent (center) to
/// dim (right), creating a smooth visual separation.
pub fn input_area_separator(width: usize, accent: Color) -> Line<'static> {
    if width < 6 {
        return gradient_separator_line(width, accent);
    }
    let side = width / 4;
    let center = width - 2 * side;
    let mut spans = Vec::with_capacity(width);
    spans.extend(gradient_segment(side, accent, false));
    spans.extend(gradient_segment(center, accent, true));
    spans.extend(gradient_segment(side, accent, false));
    Line::from(spans)
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

// ---------------------------------------------------------------------------
// Re-exports for convenience
// ---------------------------------------------------------------------------

pub use ratatui::widgets::BorderType;

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
    fn centered_gradient_separator_long_enough() {
        let line = centered_gradient_separator(40, Color::Rgb(100, 150, 200));
        // Assert display width, not span count. The two-cell gutters are each a
        // single two-character span, so a 40-cell line is 38 spans. Counting
        // spans here reported a real 40-cell separator as 2 cells short.
        assert_eq!(line.width(), 40);
    }

    #[test]
    fn centered_gradient_separator_falls_back_when_narrow() {
        let line = centered_gradient_separator(2, Color::Rgb(100, 150, 200));
        assert_eq!(line.width(), 2);
    }

    #[test]
    fn status_line_background_correct_width() {
        let spans = status_line_background(50);
        assert_eq!(spans.len(), 50);
    }

    #[test]
    fn input_area_separator_correct_width() {
        let line = input_area_separator(80, Color::Rgb(100, 150, 200));
        assert_eq!(line.spans.len(), 80);
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
