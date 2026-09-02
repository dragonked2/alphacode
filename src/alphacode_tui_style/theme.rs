use crate::alphacode_tui_style::color;
use crate::alphacode_tui_style::color::rgb;
use ratatui::prelude::*;

pub fn user_color() -> Color {
    crate::palette::role_color(crate::palette::Role::User)
}
pub fn ai_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Ai)
}
pub fn tool_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Tool)
}
pub fn file_link_color() -> Color {
    crate::palette::role_color(crate::palette::Role::FileLink)
}
pub fn dim_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Dim)
}
pub fn accent_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Accent)
}
pub fn system_message_color() -> Color {
    crate::palette::role_color(crate::palette::Role::System)
}
pub fn queued_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Queued)
}
pub fn asap_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Asap)
}
pub fn pending_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Pending)
}
pub fn user_text() -> Color {
    crate::palette::role_color(crate::palette::Role::UserText)
}
pub fn user_bg() -> Color {
    crate::palette::role_color(crate::palette::Role::UserBg)
}
pub fn ai_text() -> Color {
    crate::palette::role_color(crate::palette::Role::AiText)
}
pub fn header_icon_color() -> Color {
    crate::palette::role_color(crate::palette::Role::HeaderIcon)
}
pub fn header_name_color() -> Color {
    crate::palette::role_color(crate::palette::Role::HeaderName)
}
pub fn header_session_color() -> Color {
    crate::palette::role_color(crate::palette::Role::HeaderSession)
}

/// Header model display name accent.
pub fn model_name_color() -> Color {
    crate::palette::role_color(crate::palette::Role::ModelName)
}

/// Inline code background.
pub fn code_bg_color() -> Color {
    crate::palette::role_color(crate::palette::Role::CodeBg)
}
/// Markdown heading accent.
pub fn heading_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Heading)
}
/// Link color.
pub fn link_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Link)
}
/// Quote block accent.
pub fn quote_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Quote)
}
/// Spinner animation color.
pub fn spinner_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Spinner)
}
/// Progress bar fill color.
pub fn progress_fill_color() -> Color {
    crate::palette::role_color(crate::palette::Role::ProgressFill)
}
/// Progress bar background color.
pub fn progress_bg_color() -> Color {
    crate::palette::role_color(crate::palette::Role::ProgressBg)
}
/// Tool call background color.
pub fn tool_bg_color() -> Color {
    crate::palette::role_color(crate::palette::Role::ToolBg)
}
/// Diff added lines color.
pub fn diff_add_color() -> Color {
    crate::palette::role_color(crate::palette::Role::DiffAdd)
}
/// Diff removed lines color.
pub fn diff_remove_color() -> Color {
    crate::palette::role_color(crate::palette::Role::DiffRemove)
}
/// Diff context lines color.
pub fn diff_context_color() -> Color {
    crate::palette::role_color(crate::palette::Role::DiffContext)
}
/// Swarm agent name color.
pub fn swarm_agent_color() -> Color {
    crate::palette::role_color(crate::palette::Role::SwarmAgent)
}
/// Swarm task status color.
pub fn swarm_task_color() -> Color {
    crate::palette::role_color(crate::palette::Role::SwarmTask)
}
/// Memory entry accent color.
pub fn memory_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Memory)
}
/// Todo completion color.
pub fn todo_done_color() -> Color {
    crate::palette::role_color(crate::palette::Role::TodoDone)
}
/// Todo pending color.
pub fn todo_pending_color() -> Color {
    crate::palette::role_color(crate::palette::Role::TodoPending)
}

/// Success / additions accent.
pub fn success_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Success)
}
/// Warning accent.
pub fn warning_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Warning)
}
/// Error / deletions accent.
pub fn error_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Error)
}
/// Informational accent.
pub fn info_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Info)
}
/// Borders and rules.
pub fn border_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Border)
}
/// Selected-row background.
pub fn selection_bg_color() -> Color {
    crate::palette::role_color(crate::palette::Role::SelectionBg)
}

// Spinner frames for animated status. Keep these single-cell because the fast
// spinner-only renderer patches one status cell between full TUI redraws. This
// sequence should read as a circular spin, not a grow/recede pulse.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Frame rate for slow, full-line "liveness" indicators that can only be
/// repainted by a full TUI redraw (e.g. the running-tool progress bar) when
/// decorative animations are disabled (Minimal tier, SSH, WSL, etc.). These
/// ride the ~1 Hz passive-liveness redraw, so advancing them faster would just
/// skip frames. Keep this slow so they read as alive without forcing more
/// expensive full-frame redraws.
pub const LIVENESS_INDICATOR_FPS: f32 = 1.5;

/// Frame rate for the low-cost single-cell circular spinner when decorative
/// animations are disabled. Unlike the full-line indicators above, this spinner
/// is patched by the cheap one-cell fast path between full redraws, so it can
/// animate at a smooth, responsive cadence (well above ~1 Hz) while still
/// staying very light on resources. Keep this in sync with the spinner-only
/// tick interval in the TUI run loop (`STATUS_SPINNER_ONLY_INTERVAL`, 80ms) so
/// each tick lands on exactly one new frame.
pub const LIVENESS_SPINNER_FPS: f32 = 12.5;

pub fn spinner_frame_index(elapsed: f32, fps: f32) -> usize {
    ((elapsed * fps) as usize) % SPINNER_FRAMES.len()
}

pub fn spinner_frame(elapsed: f32, fps: f32) -> &'static str {
    SPINNER_FRAMES[spinner_frame_index(elapsed, fps)]
}

/// Whether `symbol` is one of the cells owned by the primary activity spinner.
///
/// The TUI's single-cell spinner redraw uses this to avoid patching a status-row
/// cell after a late overlay, such as the slash-command palette, has taken
/// ownership of it.
pub fn is_activity_indicator_frame(symbol: &str) -> bool {
    SPINNER_FRAMES.contains(&symbol)
}

pub fn activity_indicator_frame_index(
    elapsed: f32,
    fps: f32,
    enable_decorative_animations: bool,
) -> usize {
    if enable_decorative_animations {
        spinner_frame_index(elapsed, fps)
    } else {
        // Keep ticking at the smooth liveness rate instead of freezing on a
        // single frame. The single-cell fast path repaints this cheaply, so it
        // can animate well above ~1 Hz without a full-frame redraw.
        spinner_frame_index(elapsed, LIVENESS_SPINNER_FPS)
    }
}

pub fn activity_indicator(
    elapsed: f32,
    fps: f32,
    enable_decorative_animations: bool,
) -> &'static str {
    SPINNER_FRAMES[activity_indicator_frame_index(elapsed, fps, enable_decorative_animations)]
}

/// Convert HSL to RGB (h in 0-360, s and l in 0-1)
/// Chroma color based on position and time - creates flowing rainbow wave
/// Calculate chroma color with fade-in from dim during startup
/// Calculate smooth animated color for the header (single color, no position)
pub fn color_to_floats(c: Color, fallback: (f32, f32, f32)) -> (f32, f32, f32) {
    match c {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        Color::Indexed(n) => {
            let (r, g, b) = color::indexed_to_rgb(n);
            (r as f32, g as f32, b as f32)
        }
        _ => fallback,
    }
}

pub fn blend_color(from: Color, to: Color, t: f32) -> Color {
    let (fr, fg, fb) = color_to_floats(from, (80.0, 80.0, 80.0));
    let (tr, tg, tb) = color_to_floats(to, (200.0, 200.0, 200.0));
    let r = fr + (tr - fr) * t;
    let g = fg + (tg - fg) * t;
    let b = fb + (tb - fb) * t;
    rgb(
        r.clamp(0.0, 255.0) as u8,
        g.clamp(0.0, 255.0) as u8,
        b.clamp(0.0, 255.0) as u8,
    )
}

pub fn rainbow_prompt_color(distance: usize) -> Color {
    // Smooth HSL-based rainbow using continuous hue rotation instead of
    // discrete color stops. This produces a fluid gradient that looks
    // premium and modern, cycling through the full spectrum naturally.
    const SATURATION: f32 = 0.75;
    const LIGHTNESS: f32 = 0.75;
    const HUE_STEP: f32 = 30.0; // degrees per distance unit
    const HUE_OFFSET: f32 = 0.0; // starting hue (red)

    // Gray target (dim_color()) for fade-out
    const GRAY: (u8, u8, u8) = (80, 80, 80);

    // Exponential decay with a slightly slower rate so the rainbow
    // stays vivid for the first few prompt entries before fading.
    let decay = (-0.35 * distance as f32).exp();

    // Continuous hue: rotates through the color wheel smoothly
    let hue = (HUE_OFFSET + distance as f32 * HUE_STEP) % 360.0;
    let (r, g, b) = hsl_to_rgb(hue, SATURATION, LIGHTNESS);

    // Blend rainbow color with gray based on decay
    let blend_val = |rainbow: u8, gray: u8| -> u8 {
        (rainbow as f32 * decay + gray as f32 * (1.0 - decay)) as u8
    };

    rgb(
        blend_val(r, GRAY.0),
        blend_val(g, GRAY.1),
        blend_val(b, GRAY.2),
    )
}

/// Convert HSL to RGB. h in 0-360, s and l in 0-1.
/// This produces perceptually smooth color transitions.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

pub fn prompt_entry_color(base: Color, t: f32) -> Color {
    let peak = rgb(255, 230, 120);
    // Smooth bell-curve pulse using a Gaussian-like envelope for a more
    // organic feel. The wider sigma (0.25) creates a gentle rise and
    // fade instead of a sharp triangular peak.
    let sigma = 0.25;
    let phase = (-(t - 0.5).powi(2) / (2.0 * sigma * sigma)).exp();
    blend_color(base, peak, phase.clamp(0.0, 1.0) * 0.75)
}

pub fn prompt_entry_bg_color(base: Color, t: f32) -> Color {
    let spotlight = rgb(58, 66, 82);
    // Smoother quad-in/quad-out easing with a gentle overshoot via
    // a power curve that peaks slightly above 1.0 for a subtle bounce.
    let ease_in = 1.0 - (1.0 - t).powi(4);
    let ease_out = (1.0 - t).powi(3);
    let phase = (ease_in * ease_out * 1.72).clamp(0.0, 1.0);
    blend_color(base, spotlight, phase * 0.88)
}

pub fn prompt_entry_shimmer_color(base: Color, pos: f32, t: f32) -> Color {
    // Travel speed and width tuned for a smooth, visible shimmer that
    // crosses the prompt area in roughly the first half of the animation.
    let travel = (t * 1.2).clamp(0.0, 1.0);
    let width = 0.20;
    let dist = (pos - travel).abs();
    // Smoother bell-shaped shimmer using a squared falloff.
    let shimmer = (1.0 - (dist / width).powf(1.5)).clamp(0.0, 1.0).powf(2.0);
    let pulse = (1.0 - t).powf(0.45);
    let highlight = rgb(255, 248, 210);
    blend_color(base, highlight, shimmer * pulse * 0.75)
}

/// Generate an animated color that smoothly cycles through the brand gradient.
/// Uses continuous hue rotation for a premium, fluid feel instead of a simple
/// two-color interpolation.
pub fn animated_tool_color(elapsed: f32, enable_decorative_animations: bool) -> Color {
    if !enable_decorative_animations {
        return tool_color();
    }

    // Smooth hue rotation through the brand colors at ~1.8 second period
    let hue = (elapsed * 60.0) % 360.0; // ~6 degrees per frame at 60fps
    let saturation = 0.65;
    let lightness = 0.72;

    let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
    rgb(r, g, b)
}

/// Smoothly interpolate between two colors using perceptual blending.
/// `t` should be in 0.0..=1.0 where 0.0 gives `from` and 1.0 gives `to`.
/// Uses a smoothstep easing for natural-looking transitions.
pub fn smooth_color_transition(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    // Smoothstep: 3t^2 - 2t^3 for perceptually smooth interpolation
    let t = t * t * (3.0 - 2.0 * t);
    blend_color(from, to, t)
}

/// Get a brighter version of a color for hover/focus states.
/// Increases lightness by 15-20% while keeping hue and saturation similar.
pub fn brighten(color: Color, amount: f32) -> Color {
    let (r, g, b) = color_to_floats(color, (128.0, 128.0, 128.0));
    let factor = 1.0 + amount.clamp(0.0, 1.0);
    rgb(
        (r * factor).min(255.0) as u8,
        (g * factor).min(255.0) as u8,
        (b * factor).min(255.0) as u8,
    )
}

/// Get a dimmer version of a color for disabled/muted states.
/// Decreases brightness while preserving hue.
pub fn dim(color: Color, amount: f32) -> Color {
    let (r, g, b) = color_to_floats(color, (128.0, 128.0, 128.0));
    let factor = 1.0 - amount.clamp(0.0, 1.0);
    rgb((r * factor) as u8, (g * factor) as u8, (b * factor) as u8)
}

/// Generate a color with alpha transparency (for overlays and semi-transparent effects).
/// Since ratatui doesn't natively support alpha, this blends toward a background color.
pub fn with_alpha(color: Color, alpha: f32, background: Color) -> Color {
    blend_color(background, color, alpha.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_frames_are_circular_braille_sequence() {
        assert_eq!(
            SPINNER_FRAMES,
            &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        );
        assert!(is_activity_indicator_frame("⠋"));
        assert!(is_activity_indicator_frame("⠏"));
        assert!(!is_activity_indicator_frame("/"));
    }

    #[test]
    fn spinner_frame_wraps_at_sequence_length() {
        let fps = 10.0;
        assert_eq!(spinner_frame(0.0, fps), "⠋");
        assert_eq!(spinner_frame(0.9, fps), "⠏");
        assert_eq!(spinner_frame(1.0, fps), "⠋");
    }

    #[test]
    fn activity_indicator_still_advances_without_decorative_animations() {
        // With decorative animations disabled the single-cell spinner must keep
        // ticking instead of freezing on one frame.
        let first = activity_indicator(0.0, 12.5, false);
        let later = activity_indicator(1.0, 12.5, false);
        assert!(SPINNER_FRAMES.contains(&first));
        assert_ne!(
            first, later,
            "liveness spinner should advance within one second"
        );
    }

    #[test]
    fn liveness_spinner_advances_smoothly_within_a_few_frames() {
        // The single-cell fast path patches one status cell per 80ms tick, so the
        // non-decorative liveness spinner should advance well faster than ~1 Hz
        // (it should not still read as frozen between consecutive fast-path ticks).
        let frame_at = |elapsed: f32| activity_indicator(elapsed, 12.5, false);
        // One 80ms fast-path tick should already move to the next frame.
        assert_ne!(
            frame_at(0.0),
            frame_at(0.08),
            "liveness spinner should advance every fast-path tick (80ms)"
        );
        // It must be meaningfully faster than the old ~1.5 Hz cadence.
        const {
            assert!(
                LIVENESS_SPINNER_FPS >= 8.0,
                "liveness spinner should animate at a smooth, responsive rate"
            );
        }
    }
}
