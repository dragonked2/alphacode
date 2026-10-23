//! Professional UI components for alphacode.
//!
//! This module used to host a large library of speculative "professional"
//! widgets (banners, badges, cards, tip blocks, status dots). None of them
//! were ever referenced by the live draw path, so only the pieces actually
//! used — `section_banner` for the onboarding flow — remain. If you need a
//! new reusable UI primitive, prefer building it on the design tokens in
//! `crate::alphacode_tui_style` and `brand_ux::BrandTheme` where the shared
//! theming and background-adaptation already work.

use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_banner_has_title() {
        let lines = section_banner("Test");
        assert!(!lines.is_empty());
        assert!(lines[0].to_string().contains("Test"));
    }
}
