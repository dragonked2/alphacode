//! Canonical "themed color" surface for the TUI.
//!
//! # Why this exists
//!
//! Three competing paths existed for picking a color in a widget:
//!
//! 1. `crate::alphacode_tui_style::palette::role_color(Role::Foo)` — the
//!    ground-truth semantic lookup. Resolves against the active palette.
//! 2. `crate::alphacode_tui_style::theme::foo_color()` — per-role helper
//!    functions with shorter call sites. Functionally identical to (1).
//! 3. `crate::alphacode_tui_style::color::rgb(r, g, b)` — a literal that
//!    ignores the palette until `adapt_buffer_for_palette` rewrites the cell.
//!
//! Path (1) and (2) reach the same color. Path (3) is correct for "I want
//! this exact shade regardless of theme" but historically widgets reached for
//! it by default, which made the palette appear to do nothing.
//!
//! This module is the single import a widget should need to make a
//! themability-correct choice. New widget code should `use
//! crate::alphacode_tui_style::role;` and never touch `palette::role_color`
//! or `color::rgb` directly. The full API is intentionally tiny so the
//! choice is obvious at the call site.
//!
//! # Usage
//!
//! ```ignore
//! use crate::alphacode_tui_style::role::{Role, role_color};
//!
//! let c = role_color(Role::Accent);          // thematic color
//! let dim = role_color(Role::Dim);            // themed dim
//! // Avoid: `crate::color::rgb(120, 230, 160)` — that's the old literal path.
//! ```
//!
//! # Why not deprecate `rgb(...)`
//!
//! Many widgets legitimately want a *derived shade* of a role: a "darker
//! warning" for a hover row, a "lighter success" for a chip background.
//! Those need an off-palette color and `rgb(...)` is the right tool, with
//! the caveat that they should derive it from the role default. See
//! [`themed_rgb`] for the documented pattern.

use ratatui::style::Color;

pub use crate::alphacode_tui_style::palette::{ALL_ROLES, Palette, Role, palette, role_color};

/// Look up the current palette's color for `role`.
///
/// Identical to `palette::role_color`; re-exported so widgets have one import
/// (`crate::alphacode_tui_style::role`) instead of two.
///
/// # Why this looks trivial
///
/// It is. The point is the call site: every widget goes through this function
/// (or [`themed_rgb`]), so a future change to how the role lookup works
/// (e.g. animation-aware selection states) happens in exactly one place
/// rather than scattered across 250 widgets.
#[inline]
pub fn themed(role: Role) -> Color {
    role_color(role)
}

/// Build a `Color` whose RGB is "this role's default, shifted toward
/// `(r, g, b)` by `(dr, dg, db)`".
///
/// The common case: a widget wants a "20% darker warning" or a "10% lighter
/// dim". Computing it from the role default keeps the shade coherent across
/// themes — when the user picks a new warning color, the derived shade moves
/// with it instead of looking like an off-palette hardcoded literal.
///
/// # Implementation
///
/// `offset_rgb` clamps each channel to `0..=255` and returns a literal that
/// `adapt_buffer_for_palette` will rewrite onto the configured role with
/// the same offset preserved. See `palette::remap_literal`.
///
/// # When NOT to use this
///
/// - If the color is fully off-palette (e.g. an image preview background),
///   use `crate::alphacode_tui_style::color::rgb` directly. The signature
///   makes it obvious at the call site that the color is *not* themed.
/// - If the color is exactly the role default, use [`themed`] instead —
///   going through `offset_rgb` would skip the palette substitution for no
///   benefit.
pub fn themed_rgb(role: Role, offset: (i16, i16, i16)) -> Color {
    let (r, g, b) = role.default_rgb();
    let r = (r as i16 + offset.0).clamp(0, 255) as u8;
    let g = (g as i16 + offset.1).clamp(0, 255) as u8;
    let b = (b as i16 + offset.2).clamp(0, 255) as u8;
    crate::alphacode_tui_style::color::rgb(r, g, b)
}

/// Whether `role` has been overridden in the active palette.
///
/// Widgets can use this to make a small UX decision — e.g. show a "theme
/// changed" badge or skip an animation when the user has customized the
/// color. Most widgets should ignore this; the palette handles substitution
/// transparently.
#[inline]
pub fn is_overridden(role: Role) -> bool {
    palette().is_overridden(role)
}

/// Convenience: list all role names in declaration order. Used by the
/// `/colors` slash command and the harmony analyzer.
#[inline]
pub fn all_roles() -> &'static [Role] {
    ALL_ROLES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themed_returns_role_default() {
        // With no user overrides, themed(Role::Accent) must match the
        // role's documented default RGB. If this ever drifts, every widget
        // that calls themed() will look subtly wrong.
        let (r, g, b) = Role::Accent.default_rgb();
        let c = themed(Role::Accent);
        match c {
            Color::Rgb(cr, cg, cb) => {
                assert_eq!((cr, cg, cb), (r, g, b));
            }
            Color::Indexed(_) => {
                // 256-color terminals: quantization is allowed; just verify
                // the role is something distinct from another role so we
                // know we're not returning a fixed placeholder.
                assert_ne!(c, themed(Role::Dim));
            }
            other => panic!("unexpected Color variant from themed: {other:?}"),
        }
    }

    #[test]
    fn themed_rgb_clamps_channels() {
        // Asking for +999 from a bright role should saturate to white, not
        // overflow into a negative channel.
        let white = themed_rgb(Role::UserText, (999, 999, 999));
        if let Color::Rgb(r, g, b) = white {
            assert_eq!((r, g, b), (255, 255, 255));
        }

        // Asking for -999 from a dark role should saturate to black.
        let black = themed_rgb(Role::UserBg, (-999, -999, -999));
        if let Color::Rgb(r, g, b) = black {
            assert_eq!((r, g, b), (0, 0, 0));
        }
    }

    #[test]
    fn themed_rgb_tracks_role_when_default_changes() {
        // A 20-point darken of two different roles must produce two
        // different colors. If they collapsed, themed_rgb would not be
        // doing its job.
        let warning = themed_rgb(Role::Warning, (-20, -20, -20));
        let error = themed_rgb(Role::Error, (-20, -20, -20));
        assert_ne!(warning, error);
    }

    #[test]
    fn all_roles_is_nonempty() {
        assert!(!all_roles().is_empty());
        assert!(all_roles().contains(&Role::Accent));
        assert!(all_roles().contains(&Role::Error));
        assert!(all_roles().contains(&Role::Border));
    }

    #[test]
    fn is_overridden_default_is_false() {
        // With no user config, nothing is overridden. This is the property
        // that lets the palette short-circuit to identity substitution.
        assert!(!is_overridden(Role::Accent));
        assert!(!is_overridden(Role::Error));
    }
}
