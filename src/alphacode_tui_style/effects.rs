//! Visual effects — composable animation primitives for premium UI polish.
//!
//! These functions produce animated colors, progress indicators, and visual
//! effects that can be applied directly in render functions.

use ratatui::style::Color;
use ratatui::text::Span;

use super::color::rgb;
use super::easing;
use super::theme::{blend_color, color_to_floats, hsl_to_rgb};

// ── Glow Effects ────────────────────────────────────────────────────────────

/// A pulsing glow effect around an element.
/// Returns a color that oscillates between the base and a brightened version.
pub fn glow_pulse(base: Color, phase: f32) -> Color {
    let pulse = easing::ease_in_out_sine(phase);
    let (r, g, b) = color_to_floats(base, (128.0, 128.0, 128.0));
    let factor = 1.0 + pulse * 0.4;
    rgb(
        (r * factor).min(255.0) as u8,
        (g * factor).min(255.0) as u8,
        (b * factor).min(255.0) as u8,
    )
}

/// A breathing glow that slowly expands and contracts.
pub fn glow_breathing(base: Color, elapsed: f32) -> Color {
    let phase = (elapsed * 0.5) % 1.0; // 0.5 Hz
    glow_pulse(base, phase)
}

/// A focus ring glow — bright ring around a focused element.
pub fn glow_focus_ring(base: Color, phase: f32) -> Color {
    let pulse = (phase * std::f32::consts::PI * 2.0).sin() * 0.5 + 0.5;
    let (r, g, b) = color_to_floats(base, (128.0, 128.0, 128.0));
    let factor = 1.0 + pulse * 0.25;
    rgb(
        (r * factor).min(255.0) as u8,
        (g * factor).min(255.0) as u8,
        (b * factor).min(255.0) as u8,
    )
}

// ── Wave Effects ────────────────────────────────────────────────────────────

/// A wave that travels across a horizontal span.
/// `position` is 0.0..=1.0 (left to right), `time` drives the wave.
pub fn wave_horizontal(position: f32, time: f32, base: Color, peak: Color) -> Color {
    let wave = ((position * 4.0 - time * 3.0) * std::f32::consts::PI).sin() * 0.5 + 0.5;
    blend_color(base, peak, wave * 0.6)
}

/// A vertical wave effect.
pub fn wave_vertical(row: f32, time: f32, base: Color, peak: Color) -> Color {
    let wave = ((row * 0.3 - time * 2.0) * std::f32::consts::PI).sin() * 0.5 + 0.5;
    blend_color(base, peak, wave * 0.4)
}

/// A traveling shimmer that moves across a surface.
pub fn shimmer_travel(position: f32, time: f32, base: Color) -> Color {
    let travel = (time * 0.8) % 1.0;
    let width = 0.15;
    let dist = (position - travel).abs();
    let shimmer = (1.0 - (dist / width).max(0.0)).clamp(0.0, 1.0).powf(2.0);
    let highlight = rgb(255, 248, 220);
    blend_color(base, highlight, shimmer * 0.7)
}

// ── Typewriter Effect ───────────────────────────────────────────────────────

/// A typewriter reveal effect — characters appear one by one.
/// Returns the number of visible characters.
pub fn typewriter_visible(total_chars: usize, elapsed: f32, chars_per_second: f32) -> usize {
    let visible = (elapsed * chars_per_second) as usize;
    visible.min(total_chars)
}

/// A typewriter cursor that blinks after the text is fully revealed.
pub fn typewriter_cursor(visible: usize, total_chars: usize, elapsed: f32) -> bool {
    if visible < total_chars {
        true // Show cursor while typing
    } else {
        // Blink at 1 Hz after typing is done
        ((elapsed * 2.0) as usize) % 2 == 0
    }
}

// ── Progress Bar Effects ────────────────────────────────────────────────────

/// A smooth progress bar with gradient fill.
pub fn progress_bar_gradient(
    width: usize,
    progress: f32,
    _fill_color: Color,
    _bg_color: Color,
    elapsed: f32,
) -> Vec<Color> {
    let mut colors = Vec::with_capacity(width);
    let fill_width = (progress * width as f32) as usize;

    for i in 0..width {
        if i < fill_width {
            // Gradient from darker to lighter across the fill
            let gradient_t = if fill_width > 0 {
                i as f32 / fill_width as f32
            } else {
                0.0
            };
            let wave = (elapsed * 2.0 + i as f32 * 0.1).sin() * 0.1;
            let t = (gradient_t + wave).clamp(0.0, 1.0);
            colors.push(blend_color(_fill_color, rgb(255, 255, 255), t * 0.3));
        } else if i == fill_width {
            // Leading edge glow
            let glow = (elapsed * 4.0).sin() * 0.5 + 0.5;
            colors.push(blend_color(_fill_color, rgb(255, 255, 255), glow * 0.5));
        } else {
            colors.push(_bg_color);
        }
    }

    colors
}

/// A segmented progress bar with animated fill.
pub fn progress_bar_segmented(
    total_segments: usize,
    filled_segments: usize,
    _fill_color: Color,
    _bg_color: Color,
    elapsed: f32,
) -> String {
    let mut result = String::with_capacity(total_segments);
    let fill_chars = ["▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

    for i in 0..total_segments {
        if i < filled_segments {
            result.push('█');
        } else if i == filled_segments {
            // Animated partial fill at the leading edge
            let partial = ((elapsed * 8.0) as usize) % fill_chars.len();
            result.push_str(fill_chars[partial]);
        } else {
            result.push('░');
        }
    }

    result
}

// ── Spinner Effects ─────────────────────────────────────────────────────────

/// A premium spinner with smooth rotation.
pub fn spinner_smooth(elapsed: f32) -> &'static str {
    let frames = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
    let idx = ((elapsed * 12.0) as usize) % frames.len();
    frames[idx]
}

/// A dots spinner with progressive fill.
pub fn spinner_dots(elapsed: f32) -> String {
    let count = ((elapsed * 3.0) as usize % 4) + 1;
    let mut dots = String::with_capacity(4);
    for i in 0..4 {
        if i < count {
            dots.push('●');
        } else {
            dots.push('○');
        }
    }
    dots
}

/// A bar spinner with smooth animation.
pub fn spinner_bar(elapsed: f32) -> &'static str {
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let idx = ((elapsed * 10.0) as usize) % frames.len();
    frames[idx]
}

// ── Color Cycling Effects ───────────────────────────────────────────────────

/// Smooth hue rotation through the color wheel.
pub fn hue_cycle(elapsed: f32, saturation: f32, lightness: f32) -> Color {
    let hue = (elapsed * 30.0) % 360.0; // ~12 second full cycle
    let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
    rgb(r, g, b)
}

/// Rainbow text effect — each character gets a different hue.
pub fn rainbow_text(
    text: &str,
    elapsed: f32,
    saturation: f32,
    lightness: f32,
) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(text.len());
    for (i, ch) in text.chars().enumerate() {
        let hue = (elapsed * 30.0 + i as f32 * 15.0) % 360.0;
        let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
        spans.push(Span::styled(ch.to_string(), Color::Rgb(r, g, b)));
    }
    spans
}

/// Gradient text — linear color gradient across the text.
pub fn gradient_text(text: &str, from: Color, to: Color) -> Vec<Span<'static>> {
    let len = text.chars().count();
    if len == 0 {
        return vec![];
    }

    let mut spans = Vec::with_capacity(len);
    for (i, ch) in text.chars().enumerate() {
        let t = i as f32 / (len - 1) as f32;
        let color = blend_color(from, to, t);
        spans.push(Span::styled(ch.to_string(), color));
    }
    spans
}

// ── Panel Transition Effects ────────────────────────────────────────────────

/// A slide-in offset for a panel.
/// Returns the horizontal offset (in cells) for a panel sliding in from the side.
pub fn slide_in_offset(progress: f32, max_offset: u16) -> u16 {
    let eased = easing::ease_out_cubic(progress);
    ((1.0 - eased) * max_offset as f32) as u16
}

/// A slide-in vertical offset.
pub fn slide_in_vertical(progress: f32, max_offset: u16) -> u16 {
    let eased = easing::ease_out_cubic(progress);
    ((1.0 - eased) * max_offset as f32) as u16
}

/// A fade-in opacity multiplier (0.0 = invisible, 1.0 = fully visible).
pub fn fade_in(progress: f32) -> f32 {
    easing::ease_in_out_sine(progress)
}

// ── Line Decoration Effects ─────────────────────────────────────────────────

/// A decorative line with animated dots.
pub fn animated_dots_line(width: usize, elapsed: f32) -> String {
    let mut line = String::with_capacity(width);
    for i in 0..width {
        let phase = (elapsed * 2.0 + i as f32 * 0.3) % 1.0;
        let dot = if phase < 0.3 {
            '·'
        } else if phase < 0.6 {
            '•'
        } else {
            '·'
        };
        line.push(dot);
    }
    line
}

/// A gradient horizontal rule.
pub fn gradient_rule(width: usize, from: Color, to: Color) -> Vec<Color> {
    (0..width)
        .map(|i| {
            let t = i as f32 / (width - 1).max(1) as f32;
            blend_color(from, to, t)
        })
        .collect()
}

/// A pulsing border color for focused elements.
pub fn pulsing_border(base: Color, elapsed: f32) -> Color {
    let pulse = (elapsed * 2.0).sin() * 0.5 + 0.5;
    blend_color(base, rgb(255, 255, 255), pulse * 0.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typewriter_visible_never_exceeds_total() {
        let visible = typewriter_visible(10, 100.0, 1.0);
        assert!(visible <= 10);
    }

    #[test]
    fn progress_bar_has_correct_length() {
        let bar = progress_bar_segmented(20, 10, rgb(100, 200, 100), rgb(50, 50, 50), 0.0);
        assert_eq!(bar.chars().count(), 20);
    }

    #[test]
    fn rainbow_text_preserves_length() {
        let spans = rainbow_text("hello", 0.0, 0.7, 0.7);
        assert_eq!(spans.len(), 5);
    }

    #[test]
    fn slide_in_offset_at_full_progress_is_zero() {
        let offset = slide_in_offset(1.0, 20);
        assert_eq!(offset, 0);
    }

    #[test]
    fn fade_in_at_zero_is_zero() {
        assert!((fade_in(0.0)) < 0.01);
    }

    #[test]
    fn fade_in_at_one_is_one() {
        assert!((fade_in(1.0) - 1.0).abs() < 0.01);
    }
}
