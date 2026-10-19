//! Contextual empty-state displays for when there's nothing to show.
//!
//! Every empty state provides:
//! - A clear explanation of WHY it's empty
//! - A specific, actionable NEXT STEP
//! - A visual indicator that this is intentional (not a bug)

use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui_style::theme::{ai_text, code_bg_color, dim_color, muted_text_color};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
/// Palette-correct body-text color for the primary call to action. Uses the
/// named text role instead of a hard-coded near-white so light themes (and
/// any preset) re-tune it with the rest of the UI.
fn primary_text() -> ratatui::style::Color {
    ai_text()
}

/// Horizontal gradient divider that fades from accent to dim.
fn gradient_line(width: usize) -> Line<'static> {
    let gradient = BrandTheme::gradient();
    let chars: Vec<char> = "─".repeat(width).chars().collect();
    let mut spans = Vec::with_capacity(chars.len());
    for (i, ch) in chars.iter().enumerate() {
        let color = gradient[i % gradient.len()];
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }
    Line::from(spans)
}

/// Empty state for a fresh conversation (no messages yet).
///
/// This is the first thing a user sees after the splash screen. It must
/// communicate brand identity, suggest next steps, and look polished.
pub fn empty_conversation() -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(""),
        gradient_line(48).alignment(Alignment::Center),
        Line::from(""),
    ];

    // Brand mark
    lines.push(
        Line::from(vec![
            Span::styled(
                "  ◆ ",
                Style::default()
                    .fg(BrandTheme::gradient_color(0))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "alphacode",
                Style::default()
                    .fg(BrandTheme::accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " · AI Coding Agent",
                Style::default()
                    .fg(BrandTheme::dim_bright())
                    .add_modifier(Modifier::ITALIC),
            ),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    // Primary action prompt
    lines.push(
        Line::from(Span::styled(
            "  Describe what you want below — press Enter to send it",
            Style::default()
                .fg(primary_text())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));

    // Command suggestion cards. Descriptions are written for people who do
    // not already speak terminal: what the command does for them, plainly.
    let suggestions: &[(&str, &str, &str)] = &[
        ("/help", "F1", "What I can do"),
        ("/model", "", "Switch the AI model"),
        ("/theme", "", "Pick your color mood"),
    ];
    for (cmd, shortcut, desc) in suggestions {
        let shortcut_display = if shortcut.is_empty() {
            String::new()
        } else {
            format!(" ({})", shortcut)
        };
        lines.push(
            Line::from(vec![
                Span::styled(
                    format!("    {} ", cmd),
                    Style::default()
                        .fg(BrandTheme::accent())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<20}", desc),
                    Style::default().fg(muted_text_color()),
                ),
                Span::styled(
                    shortcut_display,
                    Style::default()
                        .fg(BrandTheme::dim())
                        .add_modifier(Modifier::ITALIC),
                ),
            ])
            .alignment(Alignment::Center),
        );
    }

    lines.push(Line::from(""));

    // Key hints with gradient dots
    let key_hints: &[(&str, &str)] = &[
        ("Shift+Enter", "new line"),
        ("Esc", "cancel"),
        ("Ctrl+K", "kill line"),
    ];
    let mut key_spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, desc)) in key_hints.iter().enumerate() {
        if i > 0 {
            key_spans.push(Span::styled(
                "  ·  ",
                Style::default().fg(BrandTheme::dim()),
            ));
        }
        key_spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        ));
        key_spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(BrandTheme::dim()),
        ));
    }
    lines.push(Line::from(key_spans).alignment(Alignment::Center));

    // Decorative bottom divider — gradient separator with modern polish
    lines.push(Line::from(""));
    lines.push(gradient_line(48).alignment(Alignment::Center));
    lines.push(Line::from(""));

    lines
}

/// Modern empty-state card with glass-effect surface treatment.
/// Renders inside `area` with a subtle tinted background and accent edge.
pub fn render_modern_empty_state(
    frame: &mut ratatui::Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
) {
    use ratatui::widgets::{Block, Borders, Widget};
    let accent = BrandTheme::accent();
    // Theme-resolved panel tint instead of a hard-coded near-black, so
    // light presets do not paint a dark glass block over a light UI.
    let bg_tint = code_bg_color();

    // Glass surface: subtle tinted background + accent left edge
    crate::alphacode_tui::tui::ui_polish::glass_surface(frame, area, accent, bg_tint);

    // Render the content lines centered inside the area
    let inner = Block::default()
        .borders(Borders::NONE)
        .padding(ratatui::widgets::Padding::new(2, 4, 2, 4));
    let inner_area = inner.inner(area);
    Widget::render(
        ratatui::widgets::Paragraph::new(lines),
        inner_area,
        frame.buffer_mut(),
    );
}

/// Empty state when a search returns no results.
pub fn empty_search(query: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  No results for \"{}\"", query),
            Style::default()
                .fg(BrandTheme::warning())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Try different keywords, check spelling, or browse all items.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

/// Empty state when no files are found in a directory.
pub fn empty_directory(path: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Directory is empty: {}", path),
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Create files with the write tool, or navigate to a different directory.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

/// Empty state when no providers are configured.
pub fn empty_providers() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No providers configured",
            Style::default()
                .fg(BrandTheme::warning())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Alphacode Free Models (Alphax Series) is active — no API key required.",
            Style::default()
                .fg(BrandTheme::success())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Add more providers with /login, or browse models with /model.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

/// Empty state for sessions list.
pub fn empty_sessions() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No saved sessions",
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Sessions are saved automatically when you /compact or /fork.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

/// Empty state for search results in settings.
pub fn empty_settings_search(query: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  No settings match \"{}\"", query),
            Style::default()
                .fg(BrandTheme::warning())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Try /settings to see all available options.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
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
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  This is normal — the operation completed successfully with no results.",
            Style::default().fg(dim_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

/// Empty state for git branches list.
pub fn empty_branches() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No branches found",
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Initialize a git repository in this directory to see branches.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

/// Welcome message after first successful command.
pub fn welcome_confirmation() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  ◆ ",
                Style::default()
                    .fg(BrandTheme::gradient_color(0))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Welcome to Alphacode!",
                Style::default()
                    .fg(BrandTheme::accent())
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Type /help for commands, /model to switch models, or just start typing.",
            Style::default().fg(muted_text_color()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "  Using Alphacode Free Models (Alphax Series) — no API key required.",
            Style::default()
                .fg(BrandTheme::success())
                .add_modifier(Modifier::ITALIC),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_conversation_has_content() {
        let lines = empty_conversation();
        assert!(lines.len() >= 8);
    }

    #[test]
    fn empty_conversation_has_brand() {
        let lines = empty_conversation();
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("alphacode"));
    }

    #[test]
    fn empty_conversation_has_key_hints() {
        let lines = empty_conversation();
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Shift+Enter"));
    }

    #[test]
    fn empty_conversation_has_commands() {
        let lines = empty_conversation();
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("/help"));
        assert!(text.contains("/model"));
        assert!(text.contains("/theme"));
    }

    #[test]
    fn empty_search_has_query() {
        let lines = empty_search("test");
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("test"));
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
        assert!(lines.len() >= 5);
    }
}
