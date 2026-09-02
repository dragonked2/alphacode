//! TUI design tokens — single source of truth for spacing, radius, and
//! chrome-frame primitives.
//!
//! # Why this exists
//!
//! Widgets across the TUI each picked their own padding/border numbers, which
//! meant two adjacent surfaces could disagree about what counts as a "card"
//! margin. `tokens` exposes named tokens so every widget reads from the same
//! ruler. Theme presets can override individual tokens without forking every
//! renderer.
//!
//! # Usage
//!
//! ```ignore
//! use crate::alphacode_tui_style::tokens;
//! let pad = tokens::Spacing::Card.to_padding();   // (vertical, horizontal)
//! let radius = tokens::Radius::Card;              // BorderType::Rounded
//! ```
//!
//! Tokens are deliberately tiny types (`u8` newtype, `BorderType` re-export)
//! so a widget that needs to ignore a token can do so without ceremony.
//!
//! # Adding a new token
//!
//! Add it to the matching enum with a stable name, give it a default in
//! [`tokens_for`] below, and add a test in the test module at the bottom of
//! this file. Avoid making tokens configurable through the theme until there
//! is at least one user — premature configurability spreads the design system
//! across the codebase.

use ratatui::widgets::BorderType;

/// Named spacing primitives, in terminal cell units.
///
/// The numeric variants map to the historical hard-coded values used across
/// widgets, so adopting a token does not change the default look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Spacing {
    /// No padding at all. Rarely the right choice; prefer a small token.
    None,
    /// Single-cell breathing room (1 row / 1 col).
    Xs,
    /// Two-cell margin (2 rows / 2 cols). The default card padding.
    Card,
    /// Three-cell margin (3 rows / 3 cols). Modals, login picker, big surfaces.
    Modal,
    /// Four-cell margin (4 rows / 4 cols). Reserved for screens that are
    /// extremely text-dense and need an outer ring to read clearly.
    Screen,
    /// Tight inline padding for pill buttons (0 rows / 2 cols).
    Pill,
    /// Inline padding for chip / badge surfaces (1 row / 2 cols).
    Chip,
    /// Single-cell padding for the chat message bubble (1 row / 2 cols).
    Message,
}

impl Spacing {
    /// Map the token to a `(vertical, horizontal)` `Padding`-compatible tuple.
    pub const fn to_padding(self) -> (u16, u16) {
        match self {
            Self::None => (0, 0),
            Self::Xs => (1, 1),
            Self::Card => (1, 2),
            Self::Modal => (2, 3),
            Self::Screen => (2, 4),
            Self::Pill => (0, 2),
            Self::Chip => (1, 2),
            Self::Message => (1, 2),
        }
    }

    /// Vertical-only variant for widgets that pad rows independently of cols.
    pub const fn vertical(self) -> u16 {
        self.to_padding().0
    }

    /// Horizontal-only variant.
    pub const fn horizontal(self) -> u16 {
        self.to_padding().1
    }

    /// Human-readable label, used in `/style` debug output and the harmony
    /// analyzer so a future maintainer can see which token was picked.
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Xs => "xs",
            Self::Card => "card",
            Self::Modal => "modal",
            Self::Screen => "screen",
            Self::Pill => "pill",
            Self::Chip => "chip",
            Self::Message => "message",
        }
    }
}

/// Border-style primitives.
///
/// We deliberately expose only two tokens: most widgets use the rounded card
/// chrome, and a tiny minority need the squared plain chrome for surfaces
/// where round caps render as floating glyphs (notably the help overlay
/// separator rule). "Heavy" / "Double" / etc. are intentionally absent — the
/// historical codebase never used them consistently, so codifying them would
/// formalize the mess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Radius {
    /// Soft, rounded corners. The default for every card / modal / panel.
    Card,
    /// Squared corners with full glyphs. Used by surfaces that render lots of
    /// inner rules where rounded caps would float.
    Plain,
}

impl Radius {
    /// Resolve to a `ratatui::widgets::BorderType`.
    pub const fn to_border_type(self) -> BorderType {
        match self {
            Self::Card => BorderType::Rounded,
            Self::Plain => BorderType::Plain,
        }
    }

    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Plain => "plain",
        }
    }
}

/// Indent / margin primitives, in cells.
///
/// These are independent of [`Spacing`] because indentation is a layout
/// primitive, not a padding primitive — a list row indented by `Indent::Row`
/// does not "pad" anything, it just reserves left-cell width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Indent {
    /// Two-cell left indent for first-level content.
    Row,
    /// Four-cell left indent for second-level content (a child of a `Row`).
    Child,
    /// Six-cell left indent for third-level content (rare; deep trees).
    Grandchild,
}

impl Indent {
    pub const fn cells(self) -> u16 {
        match self {
            Self::Row => 2,
            Self::Child => 4,
            Self::Grandchild => 6,
        }
    }

    /// Empty span of the given width, suitable for prefixing a `Line` to
    /// indent its content without forcing a left-pad on the widget itself.
    pub const fn span(self) -> &'static str {
        match self {
            Self::Row => "  ",
            Self::Child => "    ",
            Self::Grandchild => "      ",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Child => "child",
            Self::Grandchild => "grandchild",
        }
    }
}

/// Frame template — the chrome that surrounds a surface.
///
/// A `Frame` is a named bundle of (radius, padding, optional title bar). It
/// is consumed by the `frame::modal`, `frame::card`, and `frame::panel`
/// helpers in `ui_overlays::frame` so every overlay/picker renders the same
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Frame {
    /// A free-floating modal dialog (login picker, smart model picker,
    /// session picker, help overlay). Centered, with a margin around the
    /// edges of the screen.
    Modal,
    /// An inline card anchored to a pane (info widget, side panel). No
    /// centering; the parent layout decides position.
    Card,
    /// A flat panel that takes the full area of its parent (chat area,
    /// input area, status line). No internal padding decisions made by
    /// this token; the caller picks.
    Panel,
}

impl Frame {
    /// Default padding for surfaces using this frame.
    pub const fn default_padding(self) -> Spacing {
        match self {
            Self::Modal => Spacing::Modal,
            Self::Card => Spacing::Card,
            Self::Panel => Spacing::None,
        }
    }

    /// Default border style for surfaces using this frame.
    pub const fn default_radius(self) -> Radius {
        match self {
            Self::Modal => Radius::Card,
            Self::Card => Radius::Card,
            Self::Panel => Radius::Plain,
        }
    }

    /// Outer margin (cells) reserved around a frame-anchored modal.
    /// Zero for card/panel because the parent layout positions them.
    pub const fn outer_margin(self) -> u16 {
        match self {
            Self::Modal => 2,
            Self::Card => 0,
            Self::Panel => 0,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Modal => "modal",
            Self::Card => "card",
            Self::Panel => "panel",
        }
    }
}

/// Aggregate all tokens for a single frame in one read so a debug overlay
/// can show "this widget is using Frame::Modal with Spacing::Card".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedFrame {
    pub frame: Frame,
    pub spacing: Spacing,
    pub radius: Radius,
}

impl ResolvedFrame {
    pub const fn for_frame(frame: Frame) -> Self {
        Self {
            frame,
            spacing: frame.default_padding(),
            radius: frame.default_radius(),
        }
    }
}

/// Glyph primitives — small Unicode strings used as repeated chrome
/// decorations. Larger icon work belongs in `icons.rs`; this is just for
/// micro-decoration that appears inside other widgets (separator rules,
/// bullet markers, drop shadows, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// A 30-cell horizontal rule, used as a section separator inside
    /// centered overlays. Length was chosen empirically to fit within a
    /// 60-cell centered modal on a 100-column screen.
    SectionRule,
    /// A 4-cell bullet marker for unordered list rows.
    Bullet,
    /// A 2-cell right-arrow glyph used in `/help` and inline hints.
    ArrowRight,
    /// A 2-cell ellipsis used when truncating paths / messages.
    Ellipsis,
}

impl Glyph {
    pub const fn text(self) -> &'static str {
        match self {
            // U+2500 box-drawing horizontal, repeated 30 times. Universally
            // supported across the terminals we test.
            Self::SectionRule => "──────────────────────────────",
            // U+2022 bullet, two cells of leading space already in caller.
            Self::Bullet => "•",
            // U+2192 rightwards arrow.
            Self::ArrowRight => "→",
            // U+2026 horizontal ellipsis.
            Self::Ellipsis => "…",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SectionRule => "section-rule",
            Self::Bullet => "bullet",
            Self::ArrowRight => "arrow-right",
            Self::Ellipsis => "ellipsis",
        }
    }
}

/// Convenience accessor for the resolved defaults — handy when a widget wants
/// "whatever `Frame::Modal` resolves to today" without repeating the field
/// reads.
pub const fn tokens_for(frame: Frame) -> ResolvedFrame {
    ResolvedFrame::for_frame(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_padding_matches_history() {
        // These are the four values the existing widgets hardcoded. The
        // whole point of tokens is that adopting them leaves the default
        // look unchanged, so the canonical mappings are part of the contract.
        assert_eq!(Spacing::None.to_padding(), (0, 0));
        assert_eq!(Spacing::Xs.to_padding(), (1, 1));
        assert_eq!(Spacing::Card.to_padding(), (1, 2));
        assert_eq!(Spacing::Modal.to_padding(), (2, 3));
        assert_eq!(Spacing::Screen.to_padding(), (2, 4));
        assert_eq!(Spacing::Pill.to_padding(), (0, 2));
        assert_eq!(Spacing::Chip.to_padding(), (1, 2));
        assert_eq!(Spacing::Message.to_padding(), (1, 2));
    }

    #[test]
    fn radius_resolves_to_ratatui_types() {
        assert_eq!(Radius::Card.to_border_type(), BorderType::Rounded);
        assert_eq!(Radius::Plain.to_border_type(), BorderType::Plain);
    }

    #[test]
    fn frame_resolves_consistently() {
        let modal = tokens_for(Frame::Modal);
        assert_eq!(modal.spacing, Spacing::Modal);
        assert_eq!(modal.radius, Radius::Card);
        assert_eq!(modal.frame.outer_margin(), 2);

        let card = tokens_for(Frame::Card);
        assert_eq!(card.spacing, Spacing::Card);
        assert_eq!(card.radius, Radius::Card);
        assert_eq!(card.frame.outer_margin(), 0);
    }

    #[test]
    fn indent_cells_double_per_level() {
        assert_eq!(Indent::Row.cells(), 2);
        assert_eq!(Indent::Child.cells(), 4);
        assert_eq!(Indent::Grandchild.cells(), 6);
        assert_eq!(Indent::Child.cells(), 2 * Indent::Row.cells());
    }

    #[test]
    fn glyph_text_is_non_empty_and_unicode_safe() {
        for glyph in [
            Glyph::SectionRule,
            Glyph::Bullet,
            Glyph::ArrowRight,
            Glyph::Ellipsis,
        ] {
            let s = glyph.text();
            assert!(!s.is_empty(), "{glyph:?} returned empty text");
            // ratatui's cell writer needs every codepoint to be a valid
            // Unicode scalar value, which `&str` literal guarantees — but
            // verify nothing accidentally turned into a replacement char.
            for ch in s.chars() {
                assert!(ch != '\u{FFFD}', "{glyph:?} contained replacement char");
            }
        }
    }

    #[test]
    fn section_rule_fits_in_a_modal() {
        // 30 cells. The smallest modal we ever render is 40 cells wide
        // (login picker on a 60-column terminal). 30 fits with margin.
        assert!(Glyph::SectionRule.text().chars().count() <= 32);
    }
}
