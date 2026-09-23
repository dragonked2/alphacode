//! Professional UI components for alphacode.
//!
//! Provides reusable professional-grade UI components:
//! - Banners and headers
//! - Badges and status indicators
//! - Dividers and separators
//! - Progress indicators
//! - Professional confirmations
//! - Cards and panels
//! - Status dots and chips

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

/// Build a section banner with accent border and gradient underline.
pub fn section_banner(title: &str) -> Vec<Line<'static>> {
    let accent = BrandTheme::accent();
    let gradient = BrandTheme::gradient();
    let border_width = title.len() + 4;

    let mut underline_spans = Vec::with_capacity(border_width);
    for i in 0..border_width {
        let color = gradient[i % gradient.len()];
        underline_spans.push(Span::styled("─", Style::default().fg(color)));
    }

    vec![
        Line::from(Span::styled(
            format!(" ┌ {} ┐ ", title),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(underline_spans).alignment(Alignment::Center),
    ]
}

/// Build a professional confirmation badge with icon.
pub fn confirmation_badge(success: bool, label: &str) -> Line<'static> {
    let (icon, color, modifier) = if success {
        (" ✓ ", BrandTheme::success(), Modifier::BOLD)
    } else {
        (" 🚫 ", BrandTheme::error(), Modifier::BOLD)
    };
    Line::from(vec![
        Span::styled(icon, Style::default().fg(color).add_modifier(modifier)),
        Span::styled(
            label.to_string(),
            Style::default().fg(color).add_modifier(modifier),
        ),
    ])
}

/// Build a professional info panel header.
pub fn info_panel_header(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " ◆ ",
            Style::default()
                .fg(BrandTheme::gradient_color(0))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Build a professional status indicator line.
pub fn status_line(label: &str, status: &status_indicator::Status) -> Line<'static> {
    let (icon, color) = match status {
        status_indicator::Status::Active => ("●", BrandTheme::success()),
        status_indicator::Status::Idle => ("○", BrandTheme::dim()),
        status_indicator::Status::Error => ("🚫", BrandTheme::error()),
        status_indicator::Status::Warning => ("⚠", BrandTheme::warning()),
        status_indicator::Status::Loading => ("◌", BrandTheme::info()),
    };
    Line::from(vec![
        Span::styled(
            format!(" {} ", icon),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(color)),
    ])
}

/// Professional tip card for tips, callouts, and notices.
pub fn tip_card(title: &str, body: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled(
                " 💡 ",
                Style::default()
                    .fg(rgb(255, 215, 0))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(rgb(255, 215, 0))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            format!("     {}", body),
            Style::default().fg(rgb(200, 200, 200)),
        )),
    ]
}

/// Build a keybinding hint line with key and description.
pub fn key_hint(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {:<20}", key),
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            description.to_string(),
            Style::default().fg(rgb(180, 180, 200)),
        ),
    ])
}

/// Status dot — colored circle indicating state.
pub fn status_dot(active: bool) -> Span<'static> {
    let (icon, color) = if active {
        ("●", BrandTheme::success())
    } else {
        ("○", BrandTheme::dim())
    };
    Span::styled(
        format!("{} ", icon),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

/// Gradient brand mark span.
pub fn brand_mark() -> Span<'static> {
    Span::styled(
        "◆ ",
        Style::default()
            .fg(BrandTheme::gradient_color(0))
            .add_modifier(Modifier::BOLD),
    )
}

/// Build a branded label (brand mark + text).
pub fn branded_label(text: &str) -> Line<'static> {
    Line::from(vec![
        brand_mark(),
        Span::styled(
            text.to_string(),
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Build a gradient text line with brand gradient colors.
pub fn gradient_text(text: &str) -> Line<'static> {
    let spans = BrandTheme::gradient_spans(text);
    Line::from(spans)
}

/// Build a chip/badge with background color.
pub fn chip(label: &str, color: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label),
        Style::default().fg(rgb(20, 20, 30)).bg(color),
    )
}

use ratatui::style::Color;

/// Professional card with border and title.
pub fn card(title: &str, body: &[Line<'static>]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Card top border
    let gradient = BrandTheme::gradient();
    let mut border_spans = vec![Span::styled("╭", Style::default().fg(gradient[0]))];
    for i in 1..48 {
        border_spans.push(Span::styled(
            "─",
            Style::default().fg(gradient[i % gradient.len()]),
        ));
    }
    border_spans.push(Span::styled(
        "╮",
        Style::default().fg(gradient[gradient.len() - 1]),
    ));
    lines.push(Line::from(border_spans));

    // Title line
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(gradient[0])),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Separator
    let mut sep_spans = vec![Span::styled("├", Style::default().fg(gradient[0]))];
    for i in 1..48 {
        sep_spans.push(Span::styled(
            "─",
            Style::default().fg(gradient[i % gradient.len()]),
        ));
    }
    sep_spans.push(Span::styled(
        "┤",
        Style::default().fg(gradient[gradient.len() - 1]),
    ));
    lines.push(Line::from(sep_spans));

    // Body lines
    for line in body {
        let mut line_spans = vec![Span::styled("│ ", Style::default().fg(gradient[0]))];
        line_spans.extend(line.spans.iter().cloned());
        lines.push(Line::from(line_spans));
    }

    // Card bottom border
    let mut bottom_spans = vec![Span::styled("╰", Style::default().fg(gradient[0]))];
    for i in 1..48 {
        bottom_spans.push(Span::styled(
            "─",
            Style::default().fg(gradient[i % gradient.len()]),
        ));
    }
    bottom_spans.push(Span::styled(
        "╯",
        Style::default().fg(gradient[gradient.len() - 1]),
    ));
    lines.push(Line::from(bottom_spans));

    lines
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
        assert!(line.to_string().contains("🚫"));
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

    #[test]
    fn card_has_borders() {
        let lines = card("Test", &[Line::from("content")]);
        assert!(lines.len() >= 4);
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("╭"));
        assert!(text.contains("╯"));
    }

    #[test]
    fn branded_label_has_brand_mark() {
        let line = branded_label("Test");
        assert!(line.to_string().contains("◆"));
    }
}
