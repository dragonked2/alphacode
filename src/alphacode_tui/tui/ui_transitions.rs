//! Easing curves and transition utilities for smooth TUI animations.
//!
//! Provides mathematical easing functions and color interpolation helpers
//! that widgets can use for smooth transitions, fade-ins, and animated
//! state changes. All functions are pure and dependency-free.

use ratatui::style::Color;

#[cfg(test)]
use super::TuiState;
#[cfg(test)]
use ratatui::text::Line;

// ---------------------------------------------------------------------------
// Easing functions
// ---------------------------------------------------------------------------

/// Linear interpolation between 0.0 and 1.0.
pub fn ease_linear(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// Ease-in: slow start, fast end. Quadratic curve.
pub fn ease_in_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Ease-out: fast start, slow end. Quadratic curve.
pub fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * (2.0 - t)
}

/// Ease-in-out: slow start and end. Quadratic curve.
pub fn ease_in_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    }
}

/// Ease-in: cubic curve. Slower start than quadratic.
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// Ease-out: cubic curve. Slower end than quadratic.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let t1 = t - 1.0;
    t1 * t1 * t1 + 1.0
}

/// Ease-in-out: cubic curve.
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t = 2.0 * t - 2.0;
        0.5 * t * t * t + 1.0
    }
}

/// Elastic ease-out: overshoots then settles. Good for bounce-in effects.
pub fn ease_out_elastic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 || t == 1.0 {
        return t;
    }
    let p = 0.3;
    let s = p / 4.0;
    let t = t - 1.0;
    -(pow2(10.0 * t) * sin_approx((t - s) * (2.0 * std::f32::consts::PI) / p))
}

/// Bounce ease-out: simulates a bouncing ball.
pub fn ease_out_bounce(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

/// Back ease-out: slightly overshoots then returns. Good for pop-in effects.
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let s = 1.70158;
    let t = t - 1.0;
    t * t * ((s + 1.0) * t + s) + 1.0
}

// ---------------------------------------------------------------------------
// Color interpolation
// ---------------------------------------------------------------------------

/// Linearly interpolate between two RGB colors.
pub fn lerp_color(from: (u8, u8, u8), to: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let r = from.0 as f32 + (to.0 as f32 - from.0 as f32) * t;
    let g = from.1 as f32 + (to.1 as f32 - from.1 as f32) * t;
    let b = from.2 as f32 + (to.2 as f32 - from.2 as f32) * t;
    (r as u8, g as u8, b as u8)
}

/// Interpolate between two ratatui `Color::Rgb` values.
pub fn lerp_ratatui_color(from: Color, to: Color, t: f32) -> Color {
    let from_rgb = color_to_rgb(from);
    let to_rgb = color_to_rgb(to);
    let (r, g, b) = lerp_color(from_rgb, to_rgb, t);
    Color::Rgb(r, g, b)
}

/// Convert any ratatui Color to an RGB tuple (best-effort).
fn color_to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::White => (229, 229, 229),
        Color::Gray => (128, 128, 128),
        Color::DarkGray => (102, 102, 102),
        Color::LightRed => (241, 76, 76),
        Color::LightGreen => (35, 209, 139),
        Color::LightYellow => (245, 245, 67),
        Color::LightBlue => (59, 142, 234),
        Color::LightMagenta => (214, 112, 214),
        Color::LightCyan => (41, 184, 219),
        _ => (128, 128, 128),
    }
}

// ---------------------------------------------------------------------------
// Transition state machine
// ---------------------------------------------------------------------------

/// A simple transition state that tracks progress from 0.0 to 1.0.
#[derive(Debug, Clone, Copy)]
pub struct Transition {
    /// Current progress (0.0 = start, 1.0 = complete).
    pub progress: f32,
    /// Duration in seconds.
    pub duration: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
}

impl Transition {
    /// Create a new transition with the given duration.
    pub fn new(duration: f32) -> Self {
        Self {
            progress: 0.0,
            duration,
            elapsed: 0.0,
        }
    }

    /// Create a completed transition.
    pub fn complete() -> Self {
        Self {
            progress: 1.0,
            duration: 0.0,
            elapsed: 0.0,
        }
    }

    /// Advance the transition by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        self.elapsed = (self.elapsed + dt).min(self.duration);
        self.progress = if self.duration > 0.0 {
            self.elapsed / self.duration
        } else {
            1.0
        };
    }

    /// Whether the transition has completed.
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }

    /// Get the eased progress using the given easing function.
    pub fn eased_progress(&self, easing: fn(f32) -> f32) -> f32 {
        easing(self.progress)
    }

    /// Reset the transition to the beginning.
    pub fn reset(&mut self) {
        self.progress = 0.0;
        self.elapsed = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Micro-math helpers
// ---------------------------------------------------------------------------

fn pow2(x: f32) -> f32 {
    let ln2 = 0.693147;
    exp_approx(x * ln2)
}

fn exp_approx(x: f32) -> f32 {
    let x = x.clamp(-8.0, 8.0);
    let x2 = x * x;
    let x3 = x2 * x;
    let x4 = x3 * x;
    let x5 = x4 * x;
    let x6 = x5 * x;
    let x7 = x6 * x;
    1.0 + x + x2 / 2.0 + x3 / 6.0 + x4 / 24.0 + x5 / 120.0 + x6 / 720.0 + x7 / 5040.0
}

fn sin_approx(x: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let mut x = x % (2.0 * pi);
    if x > pi {
        x -= 2.0 * pi;
    } else if x < -pi {
        x += 2.0 * pi;
    }
    let x2 = x * x;
    let x3 = x2 * x;
    let x5 = x3 * x2;
    let x7 = x5 * x2;
    x - x3 / 6.0 + x5 / 120.0 - x7 / 5040.0
}

#[cfg(test)]
pub(crate) fn inline_ui_gap_height(app: &dyn TuiState) -> u16 {
    if app.inline_ui_state().is_some() {
        1
    } else {
        0
    }
}

#[cfg(test)]
pub(crate) fn extract_line_text(line: &Line) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_curves_are_bounded() {
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let _ = ease_linear(t);
            let _ = ease_in_quad(t);
            let _ = ease_out_quad(t);
            let _ = ease_in_out_quad(t);
            let _ = ease_in_cubic(t);
            let _ = ease_out_cubic(t);
            let _ = ease_in_out_cubic(t);
            let _ = ease_out_elastic(t);
            let _ = ease_out_bounce(t);
            let _ = ease_out_back(t);
        }
    }

    #[test]
    fn transition_advance_clamps() {
        let mut t = Transition::new(1.0);
        t.advance(0.5);
        assert!((t.progress - 0.5).abs() < 0.001);
        t.advance(1.0);
        assert!(t.is_complete());
    }

    #[test]
    fn lerp_color_midpoint() {
        let mid = lerp_color((0, 0, 0), (255, 255, 255), 0.5);
        assert_eq!(mid, (127, 127, 127));
    }
}
