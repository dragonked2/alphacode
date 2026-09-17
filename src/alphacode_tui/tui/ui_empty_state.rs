//! Contextual empty-state displays for when there's nothing to show.
//!
//! Every empty state provides:
//! - A clear explanation of WHY it's empty
//! - A specific, actionable NEXT STEP
//! - A visual indicator that this is intentional (not a bug)

use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Empty state for a fresh conversation (no messages yet).
pub fn empty_conversation() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "  ⬡  No messages yet",
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Start by typing a prompt below, or try a suggested prompt:",
            Style::default().fg(rgb(180, 180, 180)),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "    /help      Show command reference",
            Style::default().fg(rgb(150, 150, 170)),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(Span::styled(
            "    /model     Pick a model",
            Style::default().fg(rgb(150, 150, 170)),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(Span::styled(
            "    /theme     Change appearance",
            Style::default().fg(rgb(150, 150, 170)),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Ctrl+B to browse models, or just start typing.",
            Style::default()
                .fg(rgb(120, 120, 140))
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(ratatui::layout::Alignment::Center),
    ]
}

/// Empty state when a search returns no results.
pub fn empty_search(query: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            format!("  No results for \"{}\"", query),
            Style::default()
                .fg(BrandTheme::warning())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Try different keywords, check spelling, or browse all items.",
            Style::default().fg(rgb(180, 180, 180)),
        )),
    ]
}

/// Empty state when no files are found in a directory.
pub fn empty_directory(path: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            format!("  Directory is empty: {}", path),
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Create files with the write tool, or navigate to a different directory.",
            Style::default().fg(rgb(180, 180, 180)),
        )),
    ]
}

/// Empty state when no providers are configured.
pub fn empty_providers() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "  No providers configured",
            Style::default()
                .fg(BrandTheme::warning())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Alphacode Free Models (Alphax Series) is active — no API key required.",
            Style::default()
                .fg(BrandTheme::success())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Add more providers with /login, or browse models with /model.",
            Style::default().fg(rgb(180, 180, 180)),
        )),
    ]
}

/// Empty state for sessions list.
pub fn empty_sessions() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "  No saved sessions",
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Sessions are saved automatically when you /compact or /fork.",
            Style::default().fg(rgb(180, 180, 180)),
        )),
    ]
}

/// Empty state for search results in settings.
pub fn empty_settings_search(query: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            format!("  No settings match \"{}\"", query),
            Style::default()
                .fg(BrandTheme::warning())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Try /settings to see all available options.",
            Style::default().fg(rgb(180, 180, 180)),
        )),
    ]
}

/// Empty state for tool output when a tool returns no data.
pub fn empty_tool_output(tool_name: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            format!("  {} returned no data", tool_name),
            Style::default()
                .fg(BrandTheme::dim())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  This is normal — the operation completed successfully with no results.",
            Style::default().fg(rgb(150, 150, 170)),
        )),
    ]
}

/// Empty state for git branches list.
pub fn empty_branches() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "  No branches found",
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Initialize a git repository in this directory to see branches.",
            Style::default().fg(rgb(180, 180, 180)),
        )),
    ]
}

/// Welcome message after first successful command.
pub fn welcome_confirmation() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "  Welcome to Alphacode! ⬡",
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Type /help for commands, /model to switch models, or just start typing.",
            Style::default().fg(rgb(200, 200, 200)),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Using Alphacode Free Models (Alphax Series) — no API key required.",
            Style::default()
                .fg(BrandTheme::success())
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(ratatui::layout::Alignment::Center),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_conversation_has_content() {
        let lines = empty_conversation();
        assert!(lines.len() >= 5);
    }

    #[test]
    fn empty_search_has_query() {
        let lines = empty_search("test");
        assert!(lines[0].to_string().contains("test"));
    }

    #[test]
    fn empty_providers_has_free_model() {
        let lines = empty_providers();
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Alphacode Free"));
    }

    #[test]
    fn welcome_has_accent() {
        let lines = welcome_confirmation();
        assert!(lines.len() >= 3);
    }
}
