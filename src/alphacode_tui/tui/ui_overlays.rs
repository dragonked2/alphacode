use super::{
    accent_color, ai_color, ai_text, asap_color, clear_area, dim_color, get_grouped_changelog,
    header_icon_color, header_name_color, header_session_color, pending_color, queued_color,
    record_chat_overlay_copy_snapshot, rgb, tool_color, user_bg, user_color, user_text,
};
use crate::alphacode_tui::tui::TuiState;
use crate::alphacode_tui::tui::info_widget::WidgetPlacement;
use crate::alphacode_tui_style::theme::{
    border_color, error_color, info_color, muted_text_color, success_color, warning_color,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use super::selection_highlight::highlight_line_selection;

use super::config_overlay::{self, ConfigSetting};
#[cfg(not(test))]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(test))]
static HELP_MAX_SCROLL: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(test))]
static HELP_PAGE_HEIGHT: AtomicUsize = AtomicUsize::new(20);

#[cfg(test)]
thread_local! {
    static HELP_VIEWPORT: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 20)) };
}

pub(crate) fn help_max_scroll() -> usize {
    #[cfg(not(test))]
    {
        HELP_MAX_SCROLL.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    {
        HELP_VIEWPORT.with(|state| state.get().0)
    }
}

pub(crate) fn help_page_height() -> usize {
    #[cfg(not(test))]
    {
        HELP_PAGE_HEIGHT.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    {
        HELP_VIEWPORT.with(|state| state.get().1)
    }
}

fn set_help_viewport(max_scroll: usize, page_height: usize) {
    #[cfg(not(test))]
    {
        HELP_MAX_SCROLL.store(max_scroll, Ordering::Relaxed);
        HELP_PAGE_HEIGHT.store(page_height, Ordering::Relaxed);
    }
    #[cfg(test)]
    HELP_VIEWPORT.with(|state| state.set((max_scroll, page_height)));
}

/// Render the interactive settings overlay (`/config ui`).
///
/// Geometry and content come from `config_overlay`; painting lives here because
/// this module already owns the shared theme helpers. The highlight clamp is
/// *not* reimplemented here — it is called from there — so the row the arrow
/// keys move and the row that is painted can never disagree.
pub(super) fn draw_config_overlay(
    frame: &mut Frame,
    area: Rect,
    settings: &[ConfigSetting],
    selected: usize,
    notice: Option<&str>,
) {
    clear_area(frame, area);

    // The footer row is reserved unconditionally here rather than only when a
    // notice exists, because the row also shows details for the selected setting.
    let layout = config_overlay::layout(area, settings.len(), true);
    let block = Block::default()
        .title(Span::styled(
            config_overlay::title(layout.frame.width.saturating_sub(2)),
            Style::default()
                .fg(accent_color())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(border_color()));
    frame.render_widget(Clear, layout.frame);
    frame.render_widget(block, layout.frame);
    // The mouse handler dismisses the overlay on a click outside the box, and
    // it has to judge that against the rectangle that was actually drawn.
    config_overlay::record_painted_frame(layout.frame);

    // Clamp once so the highlight the keys move and the row that gets painted
    // cannot disagree if the list shrank since the last keypress.
    let selected = config_overlay::clamp_selection(selected, settings.len());

    if layout.list.height > 0 {
        let items: Vec<ListItem> = config_overlay::visible_rows(settings, selected, layout)
            .into_iter()
            .map(|(index, setting)| {
                let base = if index == selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let marker = if setting.current { "on" } else { "off" };
                let marker_style = if setting.current {
                    Style::default().fg(accent_color())
                } else {
                    Style::default().fg(dim_color())
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if index == selected { "▸" } else { " " },
                        Style::default().fg(accent_color()),
                    ),
                    Span::styled(format!(" {marker:>3}  "), marker_style),
                    Span::styled(
                        if layout.list.width >= 52 {
                            format!("{:<22}", setting.label)
                        } else {
                            setting.label.to_string()
                        },
                        base.fg(user_text()),
                    ),
                    Span::styled(
                        if layout.list.width >= 52 {
                            setting.description
                        } else {
                            ""
                        },
                        base.fg(dim_color()),
                    ),
                ]))
            })
            .collect();
        frame.render_widget(List::new(items), layout.list);
    }

    if layout.footer.height > 0 {
        // On narrow screens the description cannot fit beside the label, so
        // show it here; wider screens can show the configuration path instead.
        let footer = match notice {
            Some(notice) => notice.to_string(),
            None => settings
                .get(selected)
                .map(|setting| {
                    if layout.list.width < 52 {
                        setting.description.to_string()
                    } else {
                        format!("config.toml: {}", setting.key)
                    }
                })
                .unwrap_or_default(),
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {footer}"),
                Style::default().fg(dim_color()),
            )),
            layout.footer,
        );
    }
}

pub(super) fn draw_changelog_overlay(
    frame: &mut Frame,
    area: Rect,
    scroll: usize,
    app: &dyn TuiState,
) {
    clear_area(frame, area);

    let groups = get_grouped_changelog();
    let mut lines: Vec<Line<'static>> = Vec::new();

    if groups.is_empty() {
        lines.push(Line::from(Span::styled(
            "No changelog entries available.",
            Style::default().fg(dim_color()),
        )));
    } else {
        // Add a decorative header spacer
        lines.push(Line::from(""));
        for group in &groups {
            let heading = match &group.released_at {
                Some(released_at) => {
                    format!("  \u{25c6} {} \u{2500} {} ", group.version, released_at)
                }
                None => format!("  \u{25c6} {} ", group.version),
            };
            lines.push(Line::from(Span::styled(
                heading,
                Style::default()
                    .fg(accent_color())
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "  \u{2500}".repeat(40),
                Style::default().fg(dim_color()),
            )));
            lines.push(Line::from(""));
            for entry in &group.entries {
                // Detect entry type by prefix for visual categorization
                let (bullet, entry_style) = if entry.starts_with("feat") || entry.starts_with("add")
                {
                    ("\u{25b8}", Style::default().fg(success_color()))
                } else if entry.starts_with("fix") {
                    ("\u{25b8}", Style::default().fg(info_color()))
                } else if entry.starts_with("perf") {
                    ("\u{25b8}", Style::default().fg(warning_color()))
                } else if entry.starts_with("break") || entry.starts_with("remov") {
                    ("\u{25b8}", Style::default().fg(error_color()))
                } else {
                    ("\u{25b8}", Style::default().fg(muted_text_color()))
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("    {} ", bullet), Style::default().fg(dim_color())),
                    Span::styled(entry.clone(), entry_style),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    let total_lines = lines.len();
    let visible_height = area.height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = scroll.min(max_scroll);

    let scroll_info = if total_lines > visible_height {
        let pct = if max_scroll > 0 {
            (scroll * 100) / max_scroll
        } else {
            100
        };
        format!(" {}% ", pct)
    } else {
        String::new()
    };

    let title = format!(" \u{25c6} Changelog {} ", scroll_info);
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(accent_color())
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(
            " Esc to close \u{00b7} drag to select, release to copy \u{00b7} wheel/j/k scroll ",
            Style::default().fg(dim_color()),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(dim_color()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let visible_end = scroll
        .saturating_add(inner.height as usize)
        .min(total_lines);

    // Register the rendered lines so the shared copy-selection machinery can map
    // mouse drags to text and highlight + copy the selection, exactly like the
    // chat viewport. Without this, mouse capture would block native terminal
    // selection and there would be no way to copy from the overlay.
    record_chat_overlay_copy_snapshot(&lines, scroll, visible_end, inner);

    let mut visible_lines: Vec<Line<'static>> =
        lines.get(scroll..visible_end).unwrap_or(&[]).to_vec();

    if let Some(range) = app.copy_selection_range().filter(|range| {
        range.start.pane == crate::alphacode_tui::tui::CopySelectionPane::Chat
            && range.end.pane == crate::alphacode_tui::tui::CopySelectionPane::Chat
    }) {
        let (start, end) = if (range.start.abs_line, range.start.column)
            <= (range.end.abs_line, range.end.column)
        {
            (range.start, range.end)
        } else {
            (range.end, range.start)
        };
        for abs_idx in start.abs_line.max(scroll)..=end.abs_line.min(visible_end.saturating_sub(1))
        {
            let rel_idx = abs_idx.saturating_sub(scroll);
            if let Some(line) = visible_lines.get_mut(rel_idx) {
                let start_col = if abs_idx == start.abs_line {
                    start.column
                } else {
                    0
                };
                let end_col = if abs_idx == end.abs_line {
                    end.column
                } else {
                    line.width()
                };
                *line = highlight_line_selection(line, start_col, end_col);
            }
        }
    }

    frame.render_widget(Paragraph::new(visible_lines), inner);
}

/// Keep an editable query's caret visible without splitting terminal cells.
pub(super) fn help_search_display(query: &str, cursor: usize, width: usize) -> (String, usize) {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    if width == 0 {
        return (String::new(), 0);
    }
    let prefix = if width >= 12 { "Search: " } else { "/" };
    let budget = width.saturating_sub(prefix.width() + 1);
    let cursor = crate::alphacode_tui::tui::core::ceil_char_boundary(query, cursor);
    let mut start = cursor;
    let mut used = 0;
    for (index, grapheme) in query[..cursor].grapheme_indices(true).rev() {
        if used + grapheme.width() > budget {
            break;
        }
        used += grapheme.width();
        start = index;
    }
    let elided = start > 0 && budget > 0;
    if elided {
        while used + 1 > budget && start < cursor {
            let next =
                crate::alphacode_tui::tui::core::next_grapheme_boundary(query, start).min(cursor);
            used = used.saturating_sub(query[start..next].width());
            start = next;
        }
    }
    let marker = if elided { "…" } else { "" };
    let caret = prefix.width() + marker.width() + used;
    let mut text = format!("{prefix}{marker}{}", &query[start..cursor]);
    let mut remaining = width.saturating_sub(caret + 1);
    for grapheme in query[cursor..].graphemes(true) {
        if grapheme.width() > remaining {
            break;
        }
        text.push_str(grapheme);
        remaining -= grapheme.width();
    }
    (text, caret.min(width - 1))
}

pub(super) fn draw_help_overlay(frame: &mut Frame, area: Rect, scroll: usize, app: &dyn TuiState) {
    clear_area(frame, area);

    let section_style = Style::default()
        .fg(accent_color())
        .add_modifier(Modifier::BOLD);
    let cmd_style = Style::default().fg(ai_text());
    let desc_style = Style::default().fg(muted_text_color());
    let key_style = Style::default().fg(warning_color());
    let sep_style = Style::default().fg(border_color());

    let mut lines: Vec<Line<'static>> = Vec::new();

    let separator = || -> Line<'static> {
        Line::from(Span::styled(
            "  ─────────────────────────────────────────────────",
            sep_style,
        ))
    };

    let help_entry = |cmd: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(cmd.to_string(), cmd_style),
            Span::styled("  ", Style::default()),
            Span::styled(desc.to_string(), desc_style),
        ])
    };

    let alt = crate::alphacode_tui_core::keybind::alt_chord;
    let key_entry = |key: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(format!("{:<22}", key), key_style),
            Span::styled(desc.to_string(), desc_style),
        ])
    };

    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Start here", section_style)));
    lines.push(Line::from(
        "  Choose a model: /model    Resume a chat: /resume",
    ));
    lines.push(Line::from(
        "  Type a task and press Enter. Shift+Enter adds a line.",
    ));
    lines.push(Line::from(
        "  While working, Enter sends guidance to the running turn.",
    ));
    lines.push(Line::from("  Search this reference with / or Ctrl+F."));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Commands", section_style)));
    lines.push(Line::from(""));
    lines.push(help_entry("/help", "Show this help overlay"));
    lines.push(help_entry(
        "/help <command>",
        "Show details for one command",
    ));
    lines.push(help_entry("/find [text]", "Find text in the transcript"));
    lines.push(help_entry("/model", "List or switch models"));
    lines.push(help_entry("/model <name>", "Switch to a different model"));
    lines.push(help_entry(
        "/provider-test-coverage",
        "Show live-test evidence for the current provider/model",
    ));
    lines.push(help_entry("/agents", "Configure models for agent roles"));
    lines.push(help_entry(
        "/swarm-prompt",
        "Open the active swarm routing prompt in your editor",
    ));
    lines.push(help_entry(
        "/effort <level>",
        "Set effort (none|minimal|low|medium|high|xhigh|max|swarm|swarm-deep)",
    ));
    lines.push(help_entry(
        "/fast [on|off|status|default ...]",
        "Toggle fast mode",
    ));
    lines.push(help_entry(
        "/transport <mode>",
        "Set connection transport (auto|https|websocket)",
    ));
    lines.push(help_entry(
        "/alignment [status|centered|left]",
        "Show or persist text alignment preference",
    ));
    lines.push(help_entry(
        "/compact-notifications [status|on|off]",
        "Collapse swarm/file-activity notifications to one line",
    ));
    lines.push(help_entry(
        "/show-alphagrep-output [status|on|off]",
        "Render full alphagrep search output inline in chat",
    ));
    lines.push(help_entry("/config", "Show active configuration"));
    lines.push(help_entry(
        "/config ui",
        "Open the interactive settings overlay",
    ));
    lines.push(help_entry("/config init", "Create default config file"));
    lines.push(help_entry("/config edit", "Open config in $EDITOR"));
    lines.push(help_entry("/dictate", "Run configured external dictation"));
    lines.push(help_entry(
        "/git [status]",
        "Show branch and working tree status for the repo",
    ));
    lines.push(help_entry(
        "/context",
        "Show the full session context snapshot",
    ));
    lines.push(help_entry(
        "/skills",
        "Show loaded skills and alphacode-endorsed recommendations",
    ));
    lines.push(help_entry("/info", "Show session info and token usage"));
    lines.push(help_entry(
        "/keys",
        "Show keybinding conflicts with your terminal/OS",
    ));
    lines.push(help_entry("/usage", "Show connected provider usage limits"));
    lines.push(help_entry(
        "/support",
        "Email support with diagnostics prefilled",
    ));
    lines.push(help_entry("/version", "Show version and build details"));
    lines.push(help_entry(
        "/changelog",
        "Show recent changes in this build",
    ));

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Session", section_style)));
    lines.push(Line::from(""));
    lines.push(help_entry("/copy", "Copy the entire session to clipboard"));
    lines.push(help_entry("/clear", "Clear conversation and start fresh"));
    lines.push(help_entry(
        "/compact",
        "Summarize old messages to free context",
    ));
    lines.push(help_entry(
        "/rewind",
        "Show numbered history, /rewind N to rewind",
    ));
    lines.push(help_entry(
        "/fix",
        "Attempt recovery when model cannot continue",
    ));
    lines.push(help_entry(
        "/poke",
        "Poke model to resume with incomplete todos (on/off/status)",
    ));
    lines.push(help_entry(
        "/plan [goal]",
        "Draft a plan-only proposal as a plan card (no edits)",
    ));
    lines.push(help_entry(
        "/improve",
        "Autonomously improve the repo until returns diminish",
    ));
    lines.push(help_entry(
        "/improve resume",
        "Resume the last saved improve loop/plan",
    ));
    lines.push(help_entry(
        "/refactor",
        "Run a safe refactor loop with independent review",
    ));
    lines.push(help_entry(
        "/refactor resume",
        "Resume the last saved refactor loop/plan",
    ));
    lines.push(help_entry(
        "/splitview [on|off|status]",
        "Mirror the current chat in the side panel",
    ));
    lines.push(help_entry(
        "/fork [prompt]",
        "Fork session into a new window (alias: /split)",
    ));
    lines.push(help_entry(
        "/transfer",
        "Open a fresh session with only compacted context + copied todos",
    ));
    lines.push(help_entry(
        "/workspace [status|on|off|add]",
        "Enable and manage the Niri-style session workspace",
    ));
    lines.push(help_entry(
        "/catchup [next|list]",
        "Jump to finished sessions and open a Catch Up brief",
    ));
    lines.push(help_entry(
        "/back",
        "Return to the previous Catch Up source session",
    ));
    lines.push(help_entry("/resume", "Browse and resume previous sessions"));
    lines.push(help_entry(
        "/active",
        "Manage live sessions: see which are working vs ready",
    ));
    lines.push(help_entry("/save [label]", "Bookmark session for /resume"));
    lines.push(help_entry(
        "/rename <name>|--clear",
        "Set or clear current session name",
    ));
    lines.push(help_entry(
        "/unsave",
        "Remove bookmark from current session",
    ));

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Memory & Swarm", section_style)));
    lines.push(Line::from(""));
    lines.push(help_entry("/memory [on|off]", "Toggle memory features"));
    lines.push(help_entry(
        "/test [claim]",
        "Run layered verification and produce proof",
    ));
    lines.push(help_entry(
        "/initiatives",
        "Open initiatives overview / resume an initiative",
    ));
    lines.push(help_entry("/swarm [on|off]", "Toggle swarm features"));

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Auth & Accounts", section_style)));
    lines.push(Line::from(""));
    lines.push(help_entry("/auth", "Show authentication status"));
    lines.push(help_entry(
        "/login [provider]",
        "Interactive or direct login",
    ));
    lines.push(help_entry(
        "/account",
        "Open combined Claude/OpenAI account picker",
    ));
    lines.push(help_entry(
        "/subscription",
        "Inspect alphacode subscription scaffold",
    ));
    lines.push(help_entry(
        "/subscribe",
        "Why and how to subscribe to alphacode",
    ));

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  System", section_style)));
    lines.push(Line::from(""));
    lines.push(help_entry("/reload", "Reload to newer binary if available"));
    lines.push(help_entry(
        "/restart",
        "Restart with current binary (no build)",
    ));
    lines.push(help_entry(
        "/rebuild",
        "Full update (git pull + build + tests)",
    ));
    if app.is_remote_mode() {
        lines.push(help_entry("/client-reload", "Force reload client binary"));
        lines.push(help_entry("/server-reload", "Force reload server binary"));
        lines.push(help_entry(
            "/continue",
            "Continue every interrupted live session that would auto-resume",
        ));
    }
    lines.push(help_entry(
        "/debug-visual",
        "Enable visual debugging for TUI issues",
    ));
    lines.push(help_entry("/quit", "Exit alphacode"));

    // The sections above are hand-curated for ordering, but they drift as
    // commands are added. Anything registered and not already shown gets listed
    // here so no working command is invisible in /help.
    //
    // The comparison is on the *first token* of the command name on both
    // sides: a curated row may spell the syntax as "/find [text]" while the
    // registry calls it "/find", and comparing the whole string would list the
    // command twice.
    let shown: std::collections::HashSet<String> = lines
        .iter()
        .filter_map(|line| line.spans.get(1).map(|span| span.content.to_string()))
        .map(|cmd| {
            cmd.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    let uncovered: Vec<(&str, &str)> = crate::alphacode_tui::tui::app::registered_command_entries()
        .filter(|(name, _)| {
            let head = name.split_whitespace().next().unwrap_or_default();
            !shown.contains(head)
        })
        .collect();
    if !uncovered.is_empty() {
        lines.push(Line::from(""));
        lines.push(separator());
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  More commands", section_style)));
        lines.push(Line::from(""));
        for (name, desc) in uncovered {
            lines.push(help_entry(name, desc));
        }
    }

    let skills = app.available_skills();
    if !skills.is_empty() {
        lines.push(Line::from(""));
        lines.push(separator());
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled("  Skills", section_style)));
        lines.push(Line::from(""));
        for skill in &skills {
            lines.push(help_entry(&format!("/{}", skill), "Activate skill"));
        }
    }

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Navigation", section_style)));
    lines.push(Line::from(""));
    lines.push(key_entry("PageUp / PageDown", "Scroll history"));
    lines.push(key_entry("Up / Down", "Scroll history (when input empty)"));
    lines.push(key_entry(
        "Ctrl+J / Ctrl+K",
        "Jump to next / previous user prompt (also Ctrl+] / Ctrl+[)",
    ));
    lines.push(key_entry(
        "Ctrl+Shift+J / Ctrl+Shift+K",
        "Scroll history down / up one line",
    ));
    lines.push(key_entry(
        "Cmd/Super+K / J",
        "Jump to previous / next user prompt (macOS, if forwarded)",
    ));
    lines.push(key_entry("Ctrl+1..4", "Resize side panel to 25/50/75/100%"));
    lines.push(key_entry(
        "Ctrl+5..9",
        "Jump by recency (5 = 5th most recent)",
    ));

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        "  Diagrams & Diffs",
        section_style,
    )));
    lines.push(Line::from(""));
    lines.push(key_entry(
        &crate::alphacode_tui::tui::keybind::side_panel_toggle_key_label(),
        "Toggle side panel (or diagram pane if empty)",
    ));
    lines.push(key_entry(&alt("T"), "Toggle diagram position (side/top)"));
    lines.push(key_entry(
        &alt("Shift+I"),
        "Show/hide inline images (persists)",
    ));
    lines.push(key_entry("Ctrl+H / Ctrl+L", "Focus chat / diagram / diffs"));
    lines.push(key_entry(
        "Ctrl+L",
        "Clear the view, keep context (/cls; no pane focused)",
    ));
    lines.push(key_entry(
        "Ctrl+Left / Right",
        "Cycle diagrams (when diagram focused)",
    ));
    lines.push(key_entry("h/j/k/l / arrows", "Pan diagram (when focused)"));
    lines.push(key_entry("[ / ]", "Zoom diagram (when focused)"));
    lines.push(key_entry("+ / -", "Resize diagram pane"));
    lines.push(key_entry(
        &format!("{} / /diff", alt("G")),
        "Cycle diff mode (Off/Inline/Pinned/File)",
    ));
    lines.push(key_entry("Shift+Tab", "Cycle favorited models"));
    lines.push(key_entry("Ctrl+O", "Set default model (in /model picker)"));
    lines.push(key_entry(
        "Ctrl+N",
        "Toggle favorite model (in /model picker)",
    ));

    lines.push(Line::from(""));
    lines.push(separator());
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled("  Input & Editing", section_style)));
    lines.push(Line::from(""));
    lines.push(key_entry("F1", "Open or close this help overlay"));
    lines.push(key_entry("Ctrl+D", "Quit (press twice to confirm)"));
    lines.push(key_entry(
        "Shift+Left / Shift+Right",
        "Select text in the composer (Ctrl/Alt+Shift extends by word)",
    ));
    lines.push(key_entry(
        "Shift+Home / Shift+End",
        "Select to the start / end of the draft",
    ));
    lines.push(key_entry(
        "Ctrl+C",
        "Copy selection; otherwise interrupt (idle: quit confirmation)",
    ));
    lines.push(key_entry(
        "Ctrl+X",
        "Cut the composer selection (or the whole draft when nothing is selected)",
    ));
    lines.push(key_entry(
        "Esc",
        "Dismiss the command palette (keeps the draft); press again to clear it",
    ));
    lines.push(key_entry("@", "Complete a file path from the project tree"));
    lines.push(key_entry("Ctrl+A", "Move to start of input"));
    lines.push(key_entry("Ctrl+U", "Clear input line"));
    lines.push(key_entry("Ctrl+K", "Delete to end of input"));
    lines.push(key_entry(
        &format!("{} / {}", alt("Backspace"), alt("Delete")),
        "Delete previous word in input",
    ));
    lines.push(key_entry(
        "Cmd/Super+Backspace / Delete",
        "Delete previous word in input",
    ));
    if cfg!(target_os = "macos") {
        // On macOS, Cmd+Left/Right default to effort cycling; Home/End and
        // Cmd+A/E still jump to the start/end of the input.
        lines.push(key_entry("Home / End", "Move to start / end of input"));
    } else {
        lines.push(key_entry(
            "Cmd/Super+Left / Right",
            "Move to start / end of input",
        ));
    }
    lines.push(key_entry("Cmd/Super+Z", "Undo input edit"));
    lines.push(key_entry("Cmd/Super+X / V", "Cut input / paste clipboard"));
    lines.push(key_entry("Ctrl+S", "Stash / pop input (save for later)"));
    lines.push(key_entry("Ctrl+Backspace", "Delete previous word in input"));
    lines.push(key_entry("Ctrl+B", "Move by word left in input"));
    lines.push(key_entry(
        &format!("{} / Ctrl+Right", alt("F")),
        "Move by word right in input",
    ));
    lines.push(key_entry("Ctrl+Left / Right", "Move by word left / right"));
    lines.push(key_entry(
        &format!("Shift+Enter / {}", alt("Enter")),
        "Insert newline in input",
    ));
    lines.push(key_entry(
        "Trailing \\ then Enter",
        "Insert newline (fallback; run /terminal-setup to fix Shift+Enter)",
    ));
    lines.push(key_entry(
        "Ctrl+Enter / Cmd+Enter",
        "Send prompt; guidance while working (also Enter)",
    ));
    lines.push(key_entry("Ctrl+Up", "Retrieve pending message for editing"));
    lines.push(key_entry(
        "Ctrl+R",
        "Search prompt history (across sessions)",
    ));
    lines.push(key_entry("Ctrl+F", "Find text in the transcript"));
    lines.push(key_entry(
        &format!("Ctrl+V / {}", alt("V")),
        "Paste clipboard (text or image)",
    ));
    lines.push(key_entry(
        &alt("A"),
        "Quick-copy visible chat viewport plus nearby context",
    ));
    lines.push(key_entry(&alt("Y"), "Toggle chat selection/copy mode"));
    lines.push(key_entry(&alt("S"), "Toggle typing scroll lock"));
    lines.push(key_entry("Ctrl+P", "Toggle auto-poke for incomplete todos"));
    lines.push(key_entry(&alt("X"), "Show/dismiss todo list card in chat"));
    lines.push(key_entry(
        &crate::alphacode_tui::tui::keybind::effort_switch_keys_label(),
        "Cycle effort (reasoning + swarm)",
    ));
    if cfg!(target_os = "macos") {
        lines.push(key_entry(
            &alt("Left / Right"),
            &format!("Move by word in input (also {} / {})", alt("B"), alt("F")),
        ));
    }
    if let Some(label) = app.dictation_key_label() {
        lines.push(key_entry(&label, "Run configured dictation"));
    }
    if let Some(label) = crate::alphacode_tui::tui::keybind::load_open_resume_key().label {
        lines.push(key_entry(&label, "Open the /resume session picker"));
    }
    if let Some(label) = crate::alphacode_tui::tui::keybind::load_new_terminal_key().label {
        lines.push(key_entry(
            &label,
            "Spawn new alphacode session in a new terminal",
        ));
    }

    lines.push(Line::from(""));

    // Pre-wrap so the scroll range counts the same visible rows that the
    // terminal renders. Otherwise descriptions are silently clipped on narrow
    // screens and scrolling cannot reach the missing text.
    let query = app.help_query().trim();
    let matching = if query.is_empty() {
        None
    } else {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        lines.retain(|line| {
            let text = line.to_string();
            let searchable = text.to_lowercase();
            text.starts_with("    ") && terms.iter().all(|term| searchable.contains(term))
        });
        let count = lines.len();
        if count == 0 {
            lines.push(Line::from(Span::styled(
                "No matches. Try model, copy, files, or theme.",
                desc_style,
            )));
        }
        Some(count)
    };
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let search_height = u16::from(inner.height >= 2);
    let content_area = Rect {
        y: inner.y.saturating_add(search_height),
        height: inner.height.saturating_sub(search_height),
        ..inner
    };
    let width = inner.width.max(1) as usize;
    let lines = super::markdown::wrap_lines(lines, width);
    let total_lines = lines.len();
    let visible_height = content_area.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    set_help_viewport(max_scroll, visible_height.max(1));
    let scroll = scroll.min(max_scroll);

    let scroll_info = if total_lines > visible_height {
        let pct = if max_scroll > 0 {
            (scroll * 100) / max_scroll
        } else {
            100
        };
        format!(" {}% ", pct)
    } else {
        String::new()
    };

    let title = match matching {
        Some(count) => format!(
            " Help · {count} {} {scroll_info}",
            if count == 1 { "match" } else { "matches" }
        ),
        None => format!(" Help {scroll_info}"),
    };
    let footer = if area.width < 12 {
        " Esc "
    } else if area.width < 24 {
        if app.help_search_editing() || !query.is_empty() {
            " Esc clear "
        } else {
            " Esc close "
        }
    } else if app.help_search_editing() && area.width < 44 {
        " Enter browse · Esc clear "
    } else if app.help_search_editing() {
        " Type to filter · Enter browse · Esc clear "
    } else if !query.is_empty() {
        " / edit search · Esc clear · F1 close "
    } else if area.width < 60 {
        " / search · Esc close · ↑↓ scroll "
    } else {
        " / or Ctrl+F search · F1/Esc/q close · ↑↓ scroll · PgUp/PgDn page "
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(ai_text()).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(
            footer,
            Style::default().fg(dim_color()),
        )))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(border_color()));

    frame.render_widget(block, area);
    if search_height > 0 {
        let (search_text, caret) = if app.help_search_editing() || !query.is_empty() {
            help_search_display(
                app.help_query(),
                app.help_query_cursor(),
                inner.width as usize,
            )
        } else {
            let hint = if inner.width >= 30 {
                "/ Search commands and shortcuts"
            } else if inner.width >= 8 {
                "/ Search"
            } else {
                "/"
            };
            (hint.to_string(), 0)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(search_text, key_style))),
            Rect { height: 1, ..inner },
        );
        if app.help_search_editing() && inner.width > 0 {
            frame.set_cursor_position((inner.x + caret as u16, inner.y));
        }
    }
    let paragraph = Paragraph::new(lines).scroll((scroll.min(u16::MAX as usize) as u16, 0));

    frame.render_widget(paragraph, content_area);
}

pub(super) fn draw_model_status_overlay(
    frame: &mut Frame,
    area: Rect,
    scroll: usize,
    content: &str,
) {
    clear_area(frame, area);

    let title_style = Style::default()
        .fg(accent_color())
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(muted_text_color());
    let dim_style = Style::default().fg(dim_color());

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled("  Model Status", title_style)));
    lines.push(Line::from(Span::styled(
        "  Live verification evidence for provider/model behavior in alphacode",
        dim_style,
    )));
    lines.push(Line::from(""));

    for raw in content.lines() {
        if let Some(title) = raw.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(format!("  {title}"), title_style)));
        } else if let Some(title) = raw.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(format!("  {title}"), title_style)));
        } else if raw.trim().is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {raw}"),
                model_status_line_style(raw, text_style),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ↑/↓ scroll, PgUp/PgDn page, c copy report, q/Esc close",
        dim_style,
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" /provider-test-coverage "),
        )
        .scroll((scroll.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(paragraph, area);
}

fn model_status_line_style(raw: &str, default: Style) -> Style {
    // Reuse the same semantic classification the CLI uses so the TUI overlay
    // and `alphacode provider-test-coverage` stay color-consistent.
    use crate::alphacode_tui::live_tests::CoverageLineStyle;
    match crate::live_tests::classify_provider_test_coverage_line(raw) {
        CoverageLineStyle::Title => Style::default()
            .fg(accent_color())
            .add_modifier(Modifier::BOLD),
        CoverageLineStyle::Pass => Style::default().fg(rgb(120, 220, 150)),
        CoverageLineStyle::Fail => Style::default().fg(rgb(240, 110, 110)),
        CoverageLineStyle::Warn => Style::default().fg(rgb(235, 190, 105)),
        CoverageLineStyle::Dim => Style::default().fg(dim_color()),
        CoverageLineStyle::Plain => default,
    }
}

pub(super) fn draw_debug_overlay(
    frame: &mut Frame,
    placements: &[WidgetPlacement],
    chunks: &[Rect],
) {
    // Indices mirror the `CHUNK_*` constants in `ui.rs`. They used to be
    // hardcoded against an older layout, so the boxes were already labelling
    // the wrong regions (the status row as "picker", the notification row as
    // "input").
    if chunks.len() <= super::CHUNK_INPUT {
        return;
    }
    render_overlay_box(frame, chunks[super::CHUNK_MESSAGES], "messages", Color::Red);
    render_overlay_box(frame, chunks[super::CHUNK_STATUS], "status", Color::Cyan);
    render_overlay_box(
        frame,
        chunks[super::CHUNK_NOTIFICATION],
        "notification",
        Color::Magenta,
    );
    render_overlay_box(frame, chunks[super::CHUNK_INPUT], "input", Color::Green);
    if chunks.len() > super::CHUNK_DONUT && chunks[super::CHUNK_DONUT].height > 0 {
        render_overlay_box(frame, chunks[super::CHUNK_DONUT], "donut", Color::Blue);
    }

    for placement in placements {
        let title = format!("widget:{}", placement.kind.as_str());
        render_overlay_box(frame, placement.rect, &title, Color::Magenta);
    }
}

fn render_overlay_box(frame: &mut Frame, area: Rect, title: &str, color: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(title.to_string(), Style::default().fg(color)));
    frame.render_widget(block, area);
}

pub(super) fn debug_palette_json() -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "user_color": color_to_rgb(user_color()),
        "ai_color": color_to_rgb(ai_color()),
        "tool_color": color_to_rgb(tool_color()),
        "dim_color": color_to_rgb(dim_color()),
        "accent_color": color_to_rgb(accent_color()),
        "queued_color": color_to_rgb(queued_color()),
        "asap_color": color_to_rgb(asap_color()),
        "pending_color": color_to_rgb(pending_color()),
        "user_text": color_to_rgb(user_text()),
        "user_bg": color_to_rgb(user_bg()),
        "ai_text": color_to_rgb(ai_text()),
        "header_icon_color": color_to_rgb(header_icon_color()),
        "header_name_color": color_to_rgb(header_name_color()),
        "header_session_color": color_to_rgb(header_session_color()),
    }))
}

fn color_to_rgb(color: Color) -> Option<[u8; 3]> {
    match color {
        Color::Rgb(r, g, b) => Some([r, g, b]),
        Color::Indexed(n) if n >= 16 => {
            let (r, g, b) = crate::alphacode_tui::tui::color_support::indexed_to_rgb(n);
            Some([r, g, b])
        }
        _ => None,
    }
}

#[cfg(test)]
mod config_overlay_render_tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn narrow_settings_show_actions_and_selected_description() {
        let settings = [ConfigSetting {
            key: "display.example",
            label: "Example setting",
            description: "A helpful explanation",
            current: true,
            on_toggle: |_| {},
        }];
        let mut terminal = Terminal::new(TestBackend::new(38, 10)).unwrap();
        terminal
            .draw(|frame| {
                draw_config_overlay(frame, frame.area(), &settings, 0, None);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Enter toggle"));
        assert!(text.contains("Example setting"));
        assert!(text.contains("A helpful explanation"));
    }
}
