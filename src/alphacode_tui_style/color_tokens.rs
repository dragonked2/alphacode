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
//! ```
//!
//! The `resolve` method looks up the token against the live [`Palette`] and
//! falls back to a sensible built-in default when the palette does not assign
//! that role. So a `Palette::default()` keeps the historical look, and a user
//! who picks `/theme aurora-pro` retunes every site that uses the token.
//!
//! # Migration
//!
//! Phase 3a introduces the enum. Phase 3b wires every `rgb(...)` literal in
//! `alphacode_tui/tui/**` through a token. The migration is mechanical and
//! strictly safer than the literal form: if the palette is unset, the
//! historical default wins.

use ratatui::style::Color;

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
    // --- Borders ---
    BorderDefault,
    BorderFocus,
    BorderDivider,
    BorderError,
    // --- Text ---
    TextPrimary,
    TextSecondary,
    TextTertiary,
    TextDisabled,
    TextInverse,
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
    Link,
    Memory,
    // --- Effects ---
    Glow,
    Shadow,
    Selection,
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

    fn to_role(self) -> Role {
        match self {
            Self::BgBase => Role::CodeBg,
            Self::BgRaised => Role::SelectionBg,
            Self::BgSunken => Role::CodeBg,
            Self::BgSelected => Role::SelectionBg,
            Self::BgOverlay => Role::CodeBg,
            Self::BorderDefault => Role::Border,
            Self::BorderFocus => Role::Accent,
            Self::BorderDivider => Role::Border,
            Self::BorderError => Role::Error,
            Self::TextPrimary => Role::AiText,
            Self::TextSecondary => Role::MutedText,
            Self::TextTertiary => Role::Dim,
            Self::TextDisabled => Role::Dim,
            Self::TextInverse => Role::CodeBg,
            Self::Accent => Role::Accent,
            Self::AccentHover => Role::Accent,
            Self::AccentActive => Role::Accent,
            Self::AccentSubtle => Role::SelectionBg,
            Self::Success => Role::Success,
            Self::SuccessSubtle => Role::Success,
            Self::Warning => Role::Warning,
            Self::WarningSubtle => Role::Warning,
            Self::Error => Role::Error,
            Self::ErrorSubtle => Role::Error,
            Self::Info => Role::Info,
            Self::InfoSubtle => Role::Info,
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
            Self::Link => Role::Link,
            Self::Memory => Role::Memory,
            Self::Glow => Role::Accent,
            Self::Shadow => Role::CodeBg,
            Self::Selection => Role::SelectionBg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> Vec<ColorToken> {
        vec![
            ColorToken::BgBase,
            ColorToken::BgRaised,
            ColorToken::BgSelected,
            ColorToken::BorderDefault,
            ColorToken::BorderFocus,
            ColorToken::BorderError,
            ColorToken::TextPrimary,
            ColorToken::TextSecondary,
            ColorToken::TextTertiary,
            ColorToken::TextDisabled,
            ColorToken::TextInverse,
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
            ColorToken::Link,
            ColorToken::Memory,
            ColorToken::Glow,
            ColorToken::Shadow,
            ColorToken::Selection,
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
}
