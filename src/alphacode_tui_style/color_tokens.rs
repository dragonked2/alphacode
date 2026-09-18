//! Semantic color tokens — Phase 3 of the TUI 100x plan.
//!
//! Background: every widget in the codebase uses ad-hoc `rgb(r, g, b)`
//! literals. When the user changes theme, `palette::remap_literal` snaps the
//! nearest literal to the new palette, but the result is approximate and the
//! intent of the original literal is lost. A *token* captures the intent
//! ("this is the model-name color") and resolves against the palette
//! deterministically.
//!
//! # Usage
//!
//! ```ignore
//! let color = ColorToken::ModelName.resolve();
//! Style::default().fg(color).add_modifier(Modifier::BOLD);
//!
//! // Or with contrast checking
//! let safe = ColorToken::TextPrimary.on_bg(ColorToken::BgBase);
//! ```
//!
//! The `resolve` method looks up the token against the live [`Palette`] and
//! falls back to a sensible built-in default when the palette does not assign
//! that role. So a `Palette::default()` keeps the historical look, and a user
//! who picks `/theme aurora-pro` retunes every site that uses the token.

use ratatui::style::Color;

use super::color::rgb;
use super::palette::{Palette, Role};

/// A semantic color slot. The name describes *what* the color is for, not
/// *which* color it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorToken {
    // --- Surfaces ---
    BgBase,
    BgRaised,
    BgSunken,
    BgSelected,
    BgOverlay,
    BgInput,
    BgPanel,
    BgModal,
    BgCode,
    // --- Borders ---
    BorderDefault,
    BorderFocus,
    BorderDivider,
    BorderError,
    BorderSuccess,
    BorderWarning,
    BorderMuted,
    // --- Text ---
    TextPrimary,
    TextSecondary,
    TextTertiary,
    TextDisabled,
    TextInverse,
    TextLink,
    TextCode,
    // --- Brand ---
    Accent,
    AccentHover,
    AccentActive,
    AccentSubtle,
    // --- Status ---
    Success,
    SuccessSubtle,
    Warning,
    WarningSubtle,
    Error,
    ErrorSubtle,
    Info,
    InfoSubtle,
    // --- Semantic roles ---
    ModelName,
    ProviderLabel,
    ToolName,
    FilePath,
    DiffAdd,
    DiffRemove,
    DiffContext,
    Reasoning,
    Quote,
    Heading,
    HeadingH1,
    HeadingH2,
    HeadingH3,
    Link,
    Memory,
    Spinner,
    ProgressFill,
    ProgressBg,
    UserMessage,
    AiMessage,
    SystemMessage,
    // --- Effects ---
    Glow,
    Shadow,
    Selection,
    Shimmer,
    Pulse,
}

impl ColorToken {
    /// Resolve this token against the global palette.
    pub fn resolve(self) -> Color {
        super::palette::role_color(self.to_role())
    }

    /// Resolve against an explicit palette (test-friendly).
    pub fn resolve_with(self, palette: &Palette) -> Color {
        palette.color(self.to_role())
    }

    /// Get the contrast-safe foreground for this token on a given background.
    /// If the contrast is too low, returns a high-contrast alternative.
    pub fn on_bg(self, bg: ColorToken) -> Color {
        let fg = self.resolve();
        let background = bg.resolve();
        ensure_contrast(fg, background, 4.5)
    }

    /// Get a brightened variant of this token (for hover/focus states).
    pub fn brightened(self) -> Color {
        let c = self.resolve();
        brighten_color(c, 0.2)
    }

    /// Get a dimmed variant of this token (for disabled/muted states).
    pub fn dimmed(self) -> Color {
        let c = self.resolve();
        dim_color(c, 0.4)
    }

    /// Get a subtle background variant of this token.
    pub fn subtle_bg(self) -> Color {
        let c = self.resolve();
        let bg = ColorToken::BgBase.resolve();
        blend_colors(bg, c, 0.12)
    }

    fn to_role(self) -> Role {
        match self {
            // Surfaces
            Self::BgBase => Role::CodeBg,
            Self::BgRaised => Role::SelectionBg,
            Self::BgSunken => Role::CodeBg,
            Self::BgSelected => Role::SelectionBg,
            Self::BgOverlay => Role::CodeBg,
            Self::BgInput => Role::SelectionBg,
            Self::BgPanel => Role::ToolBg,
            Self::BgModal => Role::CodeBg,
            Self::BgCode => Role::CodeBg,
            // Borders
            Self::BorderDefault => Role::Border,
            Self::BorderFocus => Role::Accent,
            Self::BorderDivider => Role::Border,
            Self::BorderError => Role::Error,
            Self::BorderSuccess => Role::Success,
            Self::BorderWarning => Role::Warning,
            Self::BorderMuted => Role::Dim,
            // Text
            Self::TextPrimary => Role::AiText,
            Self::TextSecondary => Role::MutedText,
            Self::TextTertiary => Role::Dim,
            Self::TextDisabled => Role::Dim,
            Self::TextInverse => Role::CodeBg,
            Self::TextLink => Role::Link,
            Self::TextCode => Role::Tool,
            // Brand
            Self::Accent => Role::Accent,
            Self::AccentHover => Role::Accent,
            Self::AccentActive => Role::Accent,
            Self::AccentSubtle => Role::SelectionBg,
            // Status
            Self::Success => Role::Success,
            Self::SuccessSubtle => Role::Success,
            Self::Warning => Role::Warning,
            Self::WarningSubtle => Role::Warning,
            Self::Error => Role::Error,
            Self::ErrorSubtle => Role::Error,
            Self::Info => Role::Info,
            Self::InfoSubtle => Role::Info,
            // Semantic
            Self::ModelName => Role::ModelName,
            Self::ProviderLabel => Role::HeaderName,
            Self::ToolName => Role::Tool,
            Self::FilePath => Role::FileLink,
            Self::DiffAdd => Role::DiffAdd,
            Self::DiffRemove => Role::DiffRemove,
            Self::DiffContext => Role::DiffContext,
            Self::Reasoning => Role::Memory,
            Self::Quote => Role::Quote,
            Self::Heading => Role::Heading,
            Self::HeadingH1 => Role::Heading,
            Self::HeadingH2 => Role::Heading,
            Self::HeadingH3 => Role::Heading,
            Self::Link => Role::Link,
            Self::Memory => Role::Memory,
            Self::Spinner => Role::Spinner,
            Self::ProgressFill => Role::ProgressFill,
            Self::ProgressBg => Role::ProgressBg,
            Self::UserMessage => Role::User,
            Self::AiMessage => Role::Ai,
            Self::SystemMessage => Role::System,
            // Effects
            Self::Glow => Role::Accent,
            Self::Shadow => Role::CodeBg,
            Self::Selection => Role::SelectionBg,
            Self::Shimmer => Role::Accent,
            Self::Pulse => Role::Spinner,
        }
    }
}

/// Ensure a foreground color has sufficient contrast against a background.
/// Returns the original color if contrast is adequate, or a high-contrast
/// alternative if not.
pub fn ensure_contrast(fg: Color, bg: Color, min_ratio: f32) -> Color {
    let fg_lum = relative_luminance(fg);
    let bg_lum = relative_luminance(bg);
    let ratio = if fg_lum > bg_lum {
        (fg_lum + 0.05) / (bg_lum + 0.05)
    } else {
        (bg_lum + 0.05) / (fg_lum + 0.05)
    };

    if ratio >= min_ratio {
        return fg;
    }

    // Try brightening the foreground
    let bright = brighten_color(fg, 0.3);
    let bright_lum = relative_luminance(bright);
    let bright_ratio = if bright_lum > bg_lum {
        (bright_lum + 0.05) / (bg_lum + 0.05)
    } else {
        (bg_lum + 0.05) / (bright_lum + 0.05)
    };

    if bright_ratio >= min_ratio {
        return bright;
    }

    // Fall back to white or black depending on background
    if bg_lum > 0.5 {
        rgb(20, 20, 30) // Dark text on light bg
    } else {
        rgb(240, 240, 250) // Light text on dark bg
    }
}

/// Calculate relative luminance for WCAG contrast ratio.
fn relative_luminance(c: Color) -> f32 {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0),
        _ => return 0.5, // Default to mid-luminance for non-RGB colors
    };

    let linearize = |v: f32| -> f32 {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

/// Brighten a color by a factor (0.0 = no change, 1.0 = white).
fn brighten_color(c: Color, amount: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return c,
    };
    let factor = 1.0 + amount.clamp(0.0, 1.0);
    rgb(
        (r * factor).min(255.0) as u8,
        (g * factor).min(255.0) as u8,
        (b * factor).min(255.0) as u8,
    )
}

/// Dim a color by a factor (0.0 = no change, 1.0 = black).
fn dim_color(c: Color, amount: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return c,
    };
    let factor = 1.0 - amount.clamp(0.0, 1.0);
    rgb((r * factor) as u8, (g * factor) as u8, (b * factor) as u8)
}

/// Blend two colors linearly.
fn blend_colors(from: Color, to: Color, t: f32) -> Color {
    let (fr, fg, fb) = match from {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (128.0, 128.0, 128.0),
    };
    let (tr, tg, tb) = match to {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => (128.0, 128.0, 128.0),
    };
    let t = t.clamp(0.0, 1.0);
    rgb(
        (fr + (tr - fr) * t) as u8,
        (fg + (tg - fg) * t) as u8,
        (fb + (tb - fb) * t) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> Vec<ColorToken> {
        vec![
            ColorToken::BgBase,
            ColorToken::BgRaised,
            ColorToken::BgSelected,
            ColorToken::BgInput,
            ColorToken::BgPanel,
            ColorToken::BgModal,
            ColorToken::BgCode,
            ColorToken::BorderDefault,
            ColorToken::BorderFocus,
            ColorToken::BorderError,
            ColorToken::BorderSuccess,
            ColorToken::BorderWarning,
            ColorToken::BorderMuted,
            ColorToken::TextPrimary,
            ColorToken::TextSecondary,
            ColorToken::TextTertiary,
            ColorToken::TextDisabled,
            ColorToken::TextInverse,
            ColorToken::TextLink,
            ColorToken::TextCode,
            ColorToken::Accent,
            ColorToken::AccentHover,
            ColorToken::AccentActive,
            ColorToken::AccentSubtle,
            ColorToken::Success,
            ColorToken::SuccessSubtle,
            ColorToken::Warning,
            ColorToken::WarningSubtle,
            ColorToken::Error,
            ColorToken::ErrorSubtle,
            ColorToken::Info,
            ColorToken::InfoSubtle,
            ColorToken::ModelName,
            ColorToken::ProviderLabel,
            ColorToken::ToolName,
            ColorToken::FilePath,
            ColorToken::DiffAdd,
            ColorToken::DiffRemove,
            ColorToken::DiffContext,
            ColorToken::Reasoning,
            ColorToken::Quote,
            ColorToken::Heading,
            ColorToken::HeadingH1,
            ColorToken::HeadingH2,
            ColorToken::HeadingH3,
            ColorToken::Link,
            ColorToken::Memory,
            ColorToken::Spinner,
            ColorToken::ProgressFill,
            ColorToken::ProgressBg,
            ColorToken::UserMessage,
            ColorToken::AiMessage,
            ColorToken::SystemMessage,
            ColorToken::Glow,
            ColorToken::Shadow,
            ColorToken::Selection,
            ColorToken::Shimmer,
            ColorToken::Pulse,
        ]
    }

    #[test]
    fn resolve_returns_a_color_for_every_token() {
        for token in tokens() {
            let _ = token.resolve();
        }
    }

    #[test]
    fn tokens_are_distinct() {
        let a = ColorToken::Accent;
        let b = ColorToken::Success;
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_with_palette_matches_resolve() {
        let palette = Palette::default();
        for token in [ColorToken::Accent, ColorToken::ModelName, ColorToken::Error] {
            assert_eq!(token.resolve(), token.resolve_with(&palette));
        }
    }

    #[test]
    fn contrast_check_returns_color() {
        let fg = rgb(200, 200, 200);
        let bg = rgb(30, 30, 40);
        let safe = ensure_contrast(fg, bg, 4.5);
        // Should return a valid color
        match safe {
            Color::Rgb(_, _, _) => {}
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn brightened_is_brighter() {
        let c = rgb(100, 100, 100);
        let b = brighten_color(c, 0.3);
        match b {
            Color::Rgb(r, g, b) => {
                assert!(r > 100);
                assert!(g > 100);
                assert!(b > 100);
            }
            _ => panic!("Expected RGB"),
        }
    }

    #[test]
    fn dimmed_is_darker() {
        let c = rgb(200, 200, 200);
        let d = dim_color(c, 0.4);
        match d {
            Color::Rgb(r, g, b) => {
                assert!(r < 200);
                assert!(g < 200);
                assert!(b < 200);
            }
            _ => panic!("Expected RGB"),
        }
    }
}
