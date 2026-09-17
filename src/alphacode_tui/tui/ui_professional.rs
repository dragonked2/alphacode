//! Professional UI components for alphacode.
//!
//! Provides reusable professional-grade UI components:
//! - Banners and headers
//! - Badges and status indicators
//! - Dividers and separators
//! - Progress indicators
//! - Professional confirmations

use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Build a full-width gradient divider line.
pub fn gradient_divider(width: usize) -> Line<'static> {
    let gradient = BrandTheme::gradient();
    let chars: Vec<char> = "─".repeat(width.min(80)).chars().collect();
    let mut spans = Vec::with_capacity(chars.len());
    for (i, ch) in chars.iter().enumerate() {
        spans.push(Span::styled(
            ch.to_string(),
            Style::default().fg(gradient[i % gradient.len()]),
        ));
    }
    Line::from(spans)
}

/// Build a thin dim divider.
pub fn thin_divider() -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(40),
        Style::default().fg(BrandTheme::dim()),
    ))
}

/// Build a section banner with accent border.
pub fn section_banner(title: &str) -> Vec<Line<'static>> {
    let accent = BrandTheme::accent();
    vec![
        Line::from(Span::styled(
            format!(" ┌ {} ┐ ", title),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            format!(" {}", "─".repeat(title.len().saturating_add(2))),
            Style::default().fg(accent),
        ))
        .alignment(Alignment::Center),
    ]
}

/// Build a professional confirmation badge.
pub fn confirmation_badge(success: bool, label: &str) -> Line<'static> {
    let (icon, color, modifier) = if success {
        (" ✓ ", BrandTheme::success(), Modifier::BOLD)
    } else {
        (" ✗ ", BrandTheme::error(), Modifier::BOLD)
    };
    Line::from(Span::styled(
        format!("{} {}", icon, label),
        Style::default().fg(color).add_modifier(modifier),
    ))
}

/// Build a professional info panel header.
pub fn info_panel_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {} ", title),
        Style::default()
            .fg(BrandTheme::accent())
            .add_modifier(Modifier::BOLD),
    ))
}

/// Build a professional status indicator line.
pub fn status_line(label: &str, status: &status_indicator::Status) -> Line<'static> {
    let (icon, color) = match status {
        status_indicator::Status::Active => ("●", BrandTheme::success()),
        status_indicator::Status::Idle => ("○", BrandTheme::dim()),
        status_indicator::Status::Error => ("✗", BrandTheme::error()),
        status_indicator::Status::Warning => ("⚠", BrandTheme::warning()),
        status_indicator::Status::Loading => ("◌", BrandTheme::info()),
    };
    Line::from(Span::styled(
        format!(" {} {}", icon, label),
        Style::default().fg(color),
    ))
}

/// Professional tip card for tips, callouts, and notices.
pub fn tip_card(title: &str, body: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            format!(" 💡 {}", title),
            Style::default()
                .fg(rgb(255, 215, 0))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            body.to_string(),
            Style::default().fg(rgb(200, 200, 200)),
        )),
    ]
}

/// Build a keybinding hint line.
pub fn key_hint(key: &str, description: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {:<20} {}", key, description),
        Style::default().fg(rgb(180, 180, 200)),
    ))
}

/// Status indicator module.
pub mod status_indicator {
    /// Status severity levels.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Status {
        Active,
        Idle,
        Error,
        Warning,
        Loading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_divider_has_content() {
        let line = gradient_divider(40);
        assert!(!line.to_string().is_empty());
    }

    #[test]
    fn thin_divider_has_content() {
        let line = thin_divider();
        assert!(!line.to_string().is_empty());
    }

    #[test]
    fn section_banner_has_title() {
        let lines = section_banner("Test");
        assert!(!lines.is_empty());
        assert!(lines[0].to_string().contains("Test"));
    }

    #[test]
    fn confirmation_badge_success() {
        let line = confirmation_badge(true, "Test");
        assert!(line.to_string().contains("✓"));
    }

    #[test]
    fn confirmation_badge_failure() {
        let line = confirmation_badge(false, "Test");
        assert!(line.to_string().contains("✗"));
    }

    #[test]
    fn tip_card_format() {
        let lines = tip_card("Title", "Body");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].to_string().contains("Title"));
    }

    #[test]
    fn key_hint_format() {
        let line = key_hint("Ctrl+C", "Cancel");
        assert!(line.to_string().contains("Ctrl+C"));
        assert!(line.to_string().contains("Cancel"));
    }
}
