//! Light/dark terminal theme support.
//!
//! alphacode's palette (`theme.rs` and the many ad hoc `rgb(...)` call sites) is
//! designed for dark terminal backgrounds. Rather than maintaining a second
//! hand-tuned palette for light terminals, we adapt colors at the single choke
//! point every style ultimately flows through: the rendered frame buffer.
//!
//! When the theme mode is [`ThemeMode::Light`], [`adapt_buffer_for_theme`]
//! rewrites each cell's colors with a hue-preserving luminance flip: light
//! text designed for dark backgrounds becomes dark text of the same hue, and
//! dark panel backgrounds become light ones. `Color::Reset` is left alone so
//! the terminal's own (light) default background shows through, exactly like
//! it does on dark themes today.
//!
//! The mode itself is set once at startup by the TUI's terminal-background
//! detection (OSC 11 query / `ALPHACODE_THEME` / `display.theme` config) and
//! defaults to dark, which keeps every existing code path byte-identical.

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use std::sync::atomic::{AtomicU8, Ordering};

/// Whether the terminal background is dark or light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Dark background, light text (alphacode's native palette).
    #[default]
    Dark,
    /// Light background, dark text. Colors are adapted at render time.
    Light,
}

static THEME_MODE: AtomicU8 = AtomicU8::new(0);

/// Set the global theme mode. Called once at startup after terminal
/// background detection (and again if the user overrides it).
pub fn set_theme_mode(mode: ThemeMode) {
    THEME_MODE.store(
        match mode {
            ThemeMode::Dark => 0,
            ThemeMode::Light => 1,
        },
        Ordering::Relaxed,
    );
}

/// Current global theme mode. Defaults to [`ThemeMode::Dark`].
pub fn theme_mode() -> ThemeMode {
    match THEME_MODE.load(Ordering::Relaxed) {
        1 => ThemeMode::Light,
        _ => ThemeMode::Dark,
    }
}

pub fn is_light_theme() -> bool {
    theme_mode() == ThemeMode::Light
}

/// Adapt a single color for the current theme mode. Identity in dark mode.
///
/// In light mode this flips the color's perceived lightness while preserving
/// hue and saturation, so "light blue on dark" becomes "dark blue on light".
/// `Color::Reset` is preserved (the terminal supplies correct defaults).
pub fn adapt_color_for_theme(color: Color) -> Color {
    if !is_light_theme() {
        return color;
    }
    adapt_color_for_light(color)
}

fn adapt_color_for_light(color: Color) -> Color {
    let (r, g, b) = match color {
        Color::Reset => return color,
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(n) => crate::color::indexed_to_rgb(n),
        named => {
            let idx = match named {
                Color::Black => 0,
                Color::Red => 1,
                Color::Green => 2,
                Color::Yellow => 3,
                Color::Blue => 4,
                Color::Magenta => 5,
                Color::Cyan => 6,
                Color::Gray => 7,
                Color::DarkGray => 8,
                Color::LightRed => 9,
                Color::LightGreen => 10,
                Color::LightYellow => 11,
                Color::LightBlue => 12,
                Color::LightMagenta => 13,
                Color::LightCyan => 14,
                Color::White => 15,
                _ => return color,
            };
            crate::color::indexed_to_rgb(idx)
        }
    };
    flip_luminance(r, g, b)
}

/// Hue/saturation-preserving lightness inversion, quantized through the
/// capability-aware `rgb()` so 256-color terminals stay palette-bounded.
fn flip_luminance(r: u8, g: u8, b: u8) -> Color {
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let (r2, g2, b2) = hsl_to_rgb(h, s, 1.0 - l);
    crate::color::rgb(r2, g2, b2)
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let l = l.clamp(0.0, 1.0);
    if s <= 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_u8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

/// Adapt a fully rendered frame buffer for the current theme mode.
///
/// No-op in dark mode. In light mode, rewrites each cell's foreground,
/// background, and underline colors via [`adapt_color_for_theme`]. This is
/// called once per frame after the UI has been drawn, so every widget
/// (transcript, markdown, pickers, overlays) is covered without needing
/// per-call-site changes.
pub fn adapt_buffer_for_theme(buf: &mut Buffer) {
    adapt_buffer(buf, theme_mode());
}

/// Explicit-mode variant of [`adapt_buffer_for_theme`]. Useful for tests and
/// callers that already resolved the mode.
pub fn adapt_buffer(buf: &mut Buffer, mode: ThemeMode) {
    if mode != ThemeMode::Light {
        return;
    }
    // Frames contain few distinct colors; memoize the flip per unique color.
    let mut cache: std::collections::HashMap<Color, Color> = std::collections::HashMap::new();
    let mut adapt = |c: Color| -> Color {
        if c == Color::Reset {
            return c;
        }
        *cache.entry(c).or_insert_with(|| adapt_color_for_light(c))
    };
    for cell in buf.content.iter_mut() {
        cell.fg = adapt(cell.fg);
        cell.bg = adapt(cell.bg);
        cell.underline_color = adapt(cell.underline_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    struct ThemeGuard;
    impl Drop for ThemeGuard {
        fn drop(&mut self) {
            set_theme_mode(ThemeMode::Dark);
        }
    }

    // The theme mode is a process-global; serialize tests that mutate it.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_light_theme(f: impl FnOnce()) {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = ThemeGuard;
        set_theme_mode(ThemeMode::Light);
        f();
    }

    fn as_rgb(c: Color) -> (u8, u8, u8) {
        match c {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(n) => crate::color::indexed_to_rgb(n),
            other => panic!("expected concrete color, got {other:?}"),
        }
    }

    fn luminance(c: Color) -> f32 {
        let (r, g, b) = as_rgb(c);
        let (_, _, l) = rgb_to_hsl(r, g, b);
        l
    }

    #[test]
    fn dark_mode_is_identity() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_theme_mode(ThemeMode::Dark);
        let c = Color::Rgb(138, 180, 248);
        assert_eq!(adapt_color_for_theme(c), c);
        assert_eq!(adapt_color_for_theme(Color::White), Color::White);
    }

    #[test]
    fn light_mode_flips_black_and_white() {
        with_light_theme(|| {
            assert_eq!(as_rgb(adapt_color_for_theme(Color::Rgb(0, 0, 0))).0, 255);
            assert_eq!(
                as_rgb(adapt_color_for_theme(Color::Rgb(255, 255, 255))),
                (0, 0, 0)
            );
        });
    }

    #[test]
    fn light_mode_darkens_light_palette_colors_preserving_hue() {
        with_light_theme(|| {
            // user_color: a light blue. Should become a dark blue (blue channel
            // still dominant) that reads on a white background.
            let adapted = adapt_color_for_theme(Color::Rgb(138, 180, 248));
            let (r, g, b) = as_rgb(adapted);
            assert!(
                b > r && b > g,
                "hue should stay blue-dominant, got ({r},{g},{b})"
            );
            assert!(
                luminance(adapted) < 0.45,
                "light blue should become dark on light bg, got ({r},{g},{b})"
            );
        });
    }

    #[test]
    fn light_mode_lightens_dark_backgrounds() {
        with_light_theme(|| {
            // user_bg: a dark navy panel. Should become a light tint.
            let adapted = adapt_color_for_theme(Color::Rgb(35, 40, 50));
            assert!(luminance(adapted) > 0.7, "dark bg should become light");
        });
    }

    #[test]
    fn light_mode_preserves_reset() {
        with_light_theme(|| {
            assert_eq!(adapt_color_for_theme(Color::Reset), Color::Reset);
        });
    }

    #[test]
    fn light_mode_maps_named_colors() {
        with_light_theme(|| {
            assert_eq!(as_rgb(adapt_color_for_theme(Color::White)), (0, 0, 0));
            assert_eq!(as_rgb(adapt_color_for_theme(Color::Black)), (255, 255, 255));
        });
    }

    #[test]
    fn adapt_buffer_rewrites_cells_only_in_light_mode() {
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);
        for cell in buf.content.iter_mut() {
            cell.fg = Color::Rgb(245, 245, 255);
            cell.bg = Color::Rgb(35, 40, 50);
        }

        {
            let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            set_theme_mode(ThemeMode::Dark);
            let mut dark_buf = buf.clone();
            adapt_buffer_for_theme(&mut dark_buf);
            assert_eq!(dark_buf.content[0].fg, Color::Rgb(245, 245, 255));
        }

        with_light_theme(|| {
            let mut light_buf = buf.clone();
            adapt_buffer_for_theme(&mut light_buf);
            let fg = light_buf.content[0].fg;
            let bg = light_buf.content[0].bg;
            assert!(luminance(fg) < 0.3, "near-white text should become dark");
            assert!(luminance(bg) > 0.7, "dark panel bg should become light");
        });
    }

    #[test]
    fn hsl_round_trips_reasonably() {
        for (r, g, b) in [
            (0, 0, 0),
            (255, 255, 255),
            (138, 180, 248),
            (129, 199, 132),
            (255, 80, 80),
            (80, 80, 80),
        ] {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r2, g2, b2) = hsl_to_rgb(h, s, l);
            assert!(
                (r as i16 - r2 as i16).abs() <= 2
                    && (g as i16 - g2 as i16).abs() <= 2
                    && (b as i16 - b2 as i16).abs() <= 2,
                "round trip drifted: ({r},{g},{b}) -> ({r2},{g2},{b2})"
            );
        }
    }
}
