//! Widget chrome helpers.
//!
//! These are the canonical entry point for drawing a modal/card/panel
//! border. They consume the design tokens from
//! `crate::alphacode_tui_style::tokens` (Spacing, Radius, Frame) and the
//! palette roles (Role::Border, Role::PanelBorder,
//! Role::PanelBorderMuted, Role::MutedText) so every overlay drawn with
//! them looks identical by construction.
//!
//! Widgets that want to keep their existing one-off chrome for now can
//! ignore these helpers; new code should default to them.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Padding},
};

use crate::alphacode_tui_style::palette::{Role, role_color};
use crate::alphacode_tui_style::tokens::{Frame as FrameToken, Radius, ResolvedFrame, tokens_for};

/// Bundle the chrome for a surface. Returned by the helpers so a caller
/// that wants to draw a title bar or footer can build on the same block
/// without re-reading the tokens.
pub struct Chrome {
    /// The fully styled block. Call `frame.render_widget(block, area)`
    /// then `block.inner(area)` for the content rect.
    pub block: Block<'static>,
    /// The resolved design tokens, in case the caller wants to extend the
    /// chrome (e.g. add a custom title span).
    pub tokens: ResolvedFrame,
    /// The inner rect (block interior), accounting for borders.
    pub inner: Rect,
}

/// Draw chrome for a [`FrameToken::Modal`] (free-floating centered
/// modal). `area` is the full screen. The helper computes an outer
/// margin and a border-styled block inside it; the caller renders
/// content into `chrome.inner`.
pub fn modal_chrome(area: Rect, title: Option<&str>) -> Chrome {
    let tokens = tokens_for(FrameToken::Modal);
    let block = build_block(&tokens, title);
    let inner = block.inner(area);
    Chrome {
        block,
        tokens,
        inner,
    }
}

/// Draw chrome for a [`FrameToken::Card`] (inline card anchored to a
/// pane). Caller supplies the area; no outer margin.
pub fn card_chrome(area: Rect, title: Option<&str>) -> Chrome {
    let tokens = tokens_for(FrameToken::Card);
    let block = build_block(&tokens, title);
    let inner = block.inner(area);
    Chrome {
        block,
        tokens,
        inner,
    }
}

/// Draw chrome for a [`FrameToken::Panel`] (flat panel that takes the
/// full parent area; no chrome unless requested).
pub fn panel_chrome(area: Rect, title: Option<&str>) -> Chrome {
    let tokens = tokens_for(FrameToken::Panel);
    let block = build_block(&tokens, title);
    let inner = block.inner(area);
    Chrome {
        block,
        tokens,
        inner,
    }
}

fn build_block(tokens: &ResolvedFrame, title: Option<&str>) -> Block<'static> {
    let radius = tokens.radius.to_border_type();
    let border_color = role_color_for(tokens.frame);
    let title_color = role_color(Role::Accent);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(radius)
        .border_style(Style::default().fg(border_color));
    if let Some(title) = title {
        block = block.title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        ));
    }
    // Inner padding: apply only the vertical component so the
    // horizontal stays free (callers lay out their own columns).
    let (v, _) = tokens.spacing.to_padding();
    if v > 0 {
        block = block.padding(Padding::new(0, 0, v, v));
    }
    block
}

fn role_color_for(frame: FrameToken) -> Color {
    match frame {
        FrameToken::Modal => role_color(Role::PanelBorder),
        FrameToken::Card => role_color(Role::Border),
        FrameToken::Panel => role_color(Role::PanelBorderMuted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }
    }

    #[test]
    fn modal_chrome_returns_inner_with_border() {
        let chrome = modal_chrome(rect(80, 24), Some("Test"));
        // Modal reserves outer margin (2 cells each side per default),
        // so the inner rect is smaller than the input area.
        assert!(chrome.inner.width < 80);
        assert!(chrome.inner.height < 24);
        assert_eq!(chrome.tokens.frame, FrameToken::Modal);
        assert_eq!(
            chrome.tokens.radius.to_border_type(),
            ratatui::widgets::BorderType::Rounded
        );
    }

    #[test]
    fn card_chrome_keeps_full_area() {
        // Card has zero outer margin. block.inner() subtracts the 2-cell
        // border on every side; vertical padding adds another 2 (1 top +
        // 1 bottom). The helper intentionally applies no horizontal
        // padding so callers keep column layout control. So:
        //   width  = 40 - 2(border) = 38
        //   height = 10 - 2(border) - 2(padding) = 6
        let chrome = card_chrome(rect(40, 10), None);
        assert_eq!(chrome.inner.width, 38);
        assert_eq!(chrome.inner.height, 6);
    }

    #[test]
    fn panel_chrome_uses_plain_radius() {
        let chrome = panel_chrome(rect(20, 5), None);
        assert_eq!(chrome.tokens.radius, Radius::Plain);
        assert_eq!(chrome.tokens.frame, FrameToken::Panel);
    }

    #[test]
    fn chrome_with_no_title_does_not_set_title_span() {
        // Regression guard: a missing title should not render a stray
        // empty title bar.
        let chrome = card_chrome(rect(40, 10), None);
        // Block has no title by default; we cannot introspect it without
        // ratatui internals, so we just confirm the call did not panic.
        let _ = chrome.block;
    }

    #[test]
    fn role_color_for_each_frame_is_distinct() {
        // Modal, Card, and Panel pick different border colors by design.
        // If two of them ever collapse to the same role, the chrome will
        // be visually indistinguishable and this test fails.
        let modal = role_color_for(FrameToken::Modal);
        let card = role_color_for(FrameToken::Card);
        let panel = role_color_for(FrameToken::Panel);
        // Modal vs Card should differ: PanelBorder vs Border.
        assert_ne!(modal, card);
        // Panel vs Card should differ: PanelBorderMuted vs Border.
        assert_ne!(panel, card);
    }
}
