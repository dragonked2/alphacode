//! Professional help system for alphacode.
//!
//! Provides organized, searchable command reference with:
//! - Categorized commands (chat, tools, config, providers, etc.)
//! - Inline command descriptions
//! - Keyboard shortcut reference
//! - Context-aware suggestions

use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// A single command in the help reference.
#[derive(Debug, Clone)]
pub struct HelpCommand {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub category: HelpCategory,
    pub shortcut: Option<String>,
}

/// Command categories for organization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HelpCategory {
    Chat,
    Navigation,
    Tools,
    Configuration,
    Providers,
    Session,
    Development,
    System,
}

impl HelpCategory {
    fn display_name(&self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Navigation => "Navigation",
            Self::Tools => "Tools",
            Self::Configuration => "Configuration",
            Self::Providers => "Providers",
            Self::Session => "Session",
            Self::Development => "Development",
            Self::System => "System",
        }
    }

    fn color(&self) -> Color {
        match self {
            Self::Chat => BrandTheme::accent(),
            Self::Navigation => BrandTheme::info(),
            Self::Tools => BrandTheme::success(),
            Self::Configuration => rgb(170, 140, 255),
            Self::Providers => rgb(100, 225, 155),
            Self::Session => rgb(255, 200, 115),
            Self::Development => rgb(180, 120, 255),
            Self::System => BrandTheme::warning(),
        }
    }
}

/// Generate the full help reference organized by category.
pub fn help_reference() -> Vec<Line<'static>> {
    let commands = build_command_list();
    let mut lines = Vec::new();

    lines.push(
        Line::from(Span::styled(
            "  ALPHACODE COMMAND REFERENCE",
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(ratatui::layout::Alignment::Center),
    );
    lines.push(Line::from(""));

    for category in [
        HelpCategory::Chat,
        HelpCategory::Navigation,
        HelpCategory::Tools,
        HelpCategory::Configuration,
        HelpCategory::Providers,
        HelpCategory::Session,
        HelpCategory::Development,
        HelpCategory::System,
    ] {
        let cat_commands: Vec<_> = commands.iter().filter(|c| c.category == category).collect();
        if cat_commands.is_empty() {
            continue;
        }

        lines.push(Line::from(Span::styled(
            format!(" {} ", category.display_name()),
            Style::default()
                .fg(category.color())
                .add_modifier(Modifier::BOLD),
        )));

        for cmd in cat_commands {
            let aliases_str = if cmd.aliases.is_empty() {
                String::new()
            } else {
                format!(" [aliases: {}]", cmd.aliases.join(", "))
            };
            lines.push(Line::from(Span::styled(
                format!("  /{} — {}{}", cmd.name, cmd.description, aliases_str),
                Style::default().fg(rgb(200, 200, 200)),
            )));
        }
        lines.push(Line::from(""));
    }

    // Keyboard shortcuts section
    lines.push(Line::from(Span::styled(
        " KEYBOARD SHORTCUTS",
        Style::default()
            .fg(BrandTheme::info())
            .add_modifier(Modifier::BOLD),
    )));

    let shortcuts = [
        ("Enter", "Submit prompt"),
        ("Shift+Enter", "New line"),
        ("/ [Tab]", "Show command suggestions"),
        ("Up / Down", "Message history"),
        ("Ctrl+K", "Kill to end of line"),
        ("Ctrl+U", "Kill to beginning of line"),
        ("Ctrl+L", "Clear screen"),
        ("Ctrl+C", "Cancel / interrupt"),
        ("Esc", "Cancel / close"),
        ("Ctrl+B", "Toggle model picker"),
        ("Ctrl+N", "Favorite / unfavorite model"),
        ("Ctrl+1-4", "Switch side panels"),
        ("Ctrl+5-9", "Jump to prompt (by position)"),
        ("Ctrl+Shift+;", "New terminal (hotkey)"),
        ("/model", "Switch model"),
        ("/theme", "Change theme"),
        ("/help", "Show this help"),
        ("/settings", "Open settings"),
    ];

    for (key, desc) in &shortcuts {
        lines.push(Line::from(Span::styled(
            format!("  {:<20} {}", key, desc),
            Style::default().fg(rgb(180, 180, 200)),
        )));
    }

    lines.push(Line::from(""));
    lines
}

/// Context-aware command suggestions based on input.
pub fn suggest_commands(input: &str) -> Vec<String> {
    let all = build_command_list();
    let input_lower = input.to_lowercase();

    let mut matches: Vec<_> = all
        .iter()
        .filter(|c| {
            c.name.contains(&input_lower)
                || c.aliases.iter().any(|a| a.contains(&input_lower))
                || c.description.to_lowercase().contains(&input_lower)
        })
        .map(|c| c.name.clone())
        .collect();

    matches.sort();
    matches.dedup();

    if matches.is_empty() {
        vec![
            "/help".to_string(),
            "/model".to_string(),
            "/theme".to_string(),
            "/settings".to_string(),
        ]
    } else {
        matches.truncate(10);
        matches
    }
}

/// Build the complete command list.
fn build_command_list() -> Vec<HelpCommand> {
    vec![
        // Chat
        HelpCommand {
            name: "help".to_string(),
            aliases: vec!["?".to_string(), "h".to_string()],
            description: "Show command reference".to_string(),
            category: HelpCategory::Chat,
            shortcut: Some("/ [Tab]".to_string()),
        },
        HelpCommand {
            name: "model".to_string(),
            aliases: vec!["/m".to_string()],
            description: "Switch active model".to_string(),
            category: HelpCategory::Chat,
            shortcut: Some("Ctrl+B".to_string()),
        },
        HelpCommand {
            name: "clear".to_string(),
            aliases: vec!["/cls".to_string()],
            description: "Clear conversation history".to_string(),
            category: HelpCategory::Chat,
            shortcut: Some("Ctrl+L".to_string()),
        },
        HelpCommand {
            name: "resume".to_string(),
            aliases: vec!["/r".to_string()],
            description: "Resume previous session".to_string(),
            category: HelpCategory::Chat,
            shortcut: None,
        },
        HelpCommand {
            name: "fork".to_string(),
            aliases: vec!["/f".to_string()],
            description: "Fork current conversation".to_string(),
            category: HelpCategory::Chat,
            shortcut: None,
        },
        HelpCommand {
            name: "share".to_string(),
            aliases: vec!["/s".to_string()],
            description: "Share conversation as link".to_string(),
            category: HelpCategory::Chat,
            shortcut: None,
        },
        // Navigation
        HelpCommand {
            name: "up".to_string(),
            aliases: vec!["/u".to_string()],
            description: "Scroll message history up".to_string(),
            category: HelpCategory::Navigation,
            shortcut: Some("↑".to_string()),
        },
        HelpCommand {
            name: "down".to_string(),
            aliases: vec!["/d".to_string()],
            description: "Scroll message history down".to_string(),
            category: HelpCategory::Navigation,
            shortcut: Some("↓".to_string()),
        },
        HelpCommand {
            name: "top".to_string(),
            aliases: vec!["/t".to_string()],
            description: "Jump to top of transcript".to_string(),
            category: HelpCategory::Navigation,
            shortcut: Some("Ctrl+Home".to_string()),
        },
        HelpCommand {
            name: "bottom".to_string(),
            aliases: vec!["/b".to_string()],
            description: "Jump to latest message".to_string(),
            category: HelpCategory::Navigation,
            shortcut: Some("Ctrl+End".to_string()),
        },
        // Tools
        HelpCommand {
            name: "read".to_string(),
            aliases: vec!["/cat".to_string()],
            description: "Read file contents".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        HelpCommand {
            name: "write".to_string(),
            aliases: vec!["/w".to_string()],
            description: "Write to file".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        HelpCommand {
            name: "edit".to_string(),
            aliases: vec!["/e".to_string()],
            description: "Edit file with LLM".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        HelpCommand {
            name: "bash".to_string(),
            aliases: vec!["/shell".to_string()],
            description: "Run shell command".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        HelpCommand {
            name: "grep".to_string(),
            aliases: vec!["/search".to_string()],
            description: "Search file contents".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        HelpCommand {
            name: "ls".to_string(),
            aliases: vec!["/dir".to_string()],
            description: "List directory contents".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        HelpCommand {
            name: "glob".to_string(),
            aliases: vec!["/find".to_string()],
            description: "Find files by pattern".to_string(),
            category: HelpCategory::Tools,
            shortcut: None,
        },
        // Configuration
        HelpCommand {
            name: "theme".to_string(),
            aliases: vec!["/color".to_string()],
            description: "Change color theme".to_string(),
            category: HelpCategory::Configuration,
            shortcut: Some("/theme".to_string()),
        },
        HelpCommand {
            name: "settings".to_string(),
            aliases: vec!["/config".to_string()],
            description: "Open settings panel".to_string(),
            category: HelpCategory::Configuration,
            shortcut: Some("/settings".to_string()),
        },
        // Providers
        HelpCommand {
            name: "login".to_string(),
            aliases: vec!["/auth".to_string()],
            description: "Authenticate with provider".to_string(),
            category: HelpCategory::Providers,
            shortcut: None,
        },
        HelpCommand {
            name: "logout".to_string(),
            aliases: vec!["/unauth".to_string()],
            description: "Log out of provider".to_string(),
            category: HelpCategory::Providers,
            shortcut: None,
        },
        HelpCommand {
            name: "provider".to_string(),
            aliases: vec!["/provider".to_string()],
            description: "Switch provider".to_string(),
            category: HelpCategory::Providers,
            shortcut: None,
        },
        // Session
        HelpCommand {
            name: "compact".to_string(),
            aliases: vec!["/compress".to_string()],
            description: "Compact conversation context".to_string(),
            category: HelpCategory::Session,
            shortcut: None,
        },
        HelpCommand {
            name: "export".to_string(),
            aliases: vec!["/dump".to_string()],
            description: "Export session as JSON".to_string(),
            category: HelpCategory::Session,
            shortcut: None,
        },
        // Development
        HelpCommand {
            name: "plan".to_string(),
            aliases: vec!["/task".to_string()],
            description: "Create and track a plan".to_string(),
            category: HelpCategory::Development,
            shortcut: None,
        },
        HelpCommand {
            name: "agent".to_string(),
            aliases: vec!["/spawn".to_string()],
            description: "Spawn a subagent".to_string(),
            category: HelpCategory::Development,
            shortcut: None,
        },
        HelpCommand {
            name: "debug".to_string(),
            aliases: vec!["/diag".to_string()],
            description: "Diagnostic commands".to_string(),
            category: HelpCategory::Development,
            shortcut: None,
        },
        // System
        HelpCommand {
            name: "ping".to_string(),
            aliases: vec!["/health".to_string()],
            description: "Check system health".to_string(),
            category: HelpCategory::System,
            shortcut: None,
        },
        HelpCommand {
            name: "update".to_string(),
            aliases: vec!["/upgrade".to_string()],
            description: "Update alphacode".to_string(),
            category: HelpCategory::System,
            shortcut: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_reference_has_commands() {
        let lines = help_reference();
        assert!(!lines.is_empty());
        assert!(lines.len() > 20);
    }

    #[test]
    fn help_reference_has_categories() {
        let lines = help_reference();
        let text = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Chat"));
        assert!(text.contains("Tools"));
        assert!(text.contains("Configuration"));
    }

    #[test]
    fn suggest_commands_fuzzy() {
        let suggestions = suggest_commands("th");
        assert!(
            suggestions.contains(&"/theme".to_string()),
            "should suggest theme for 'th'"
        );
    }

    #[test]
    fn suggest_commands_falls_back() {
        let suggestions = suggest_commands("zzzznotacommand");
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn commands_have_categories() {
        let commands = build_command_list();
        for cmd in &commands {
            assert_ne!(
                cmd.category,
                HelpCategory::Chat,
                "every command should have a meaningful category"
            );
        }
    }
}
