//! Modern visual polish helpers.
//!
//! This module used to include a "glass surface" tint and a pulsing accent
//! border style. Both were never wired into the live draw path, so only the
//! helper that is actually used — `gradient_separator_line` for the
//! transcript-to-composer separator — remains. Build new primitives on the
//! design tokens in `crate::alphacode_tui_style` and `brand_ux::BrandTheme`
//! where background/theme adaptation already happens frame-wide.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

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
// Helper: color blending
// ---------------------------------------------------------------------------

/// Blend `from` toward `to` by amount `t` (0.0 = `from`, 1.0 = `to`).
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
}
