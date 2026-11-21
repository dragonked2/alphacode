//! Stable, width-aware composer guidance. All chrome occupies existing borders.
use super::{ComposerMode, TuiState};
use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use ratatui::{prelude::*, widgets::Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

fn primary_action(mode: ComposerMode, new_session: bool, processing: bool) -> &'static str {
    // Explicit command/shell modes consume Enter before normal chat routing.
    if mode.is_shell() {
        "run"
    } else if mode == ComposerMode::SlashCommand {
        "choose"
    } else if new_session {
        "new chat"
    } else if processing {
        // Mid-turn prompts interleave into the running turn now that queue
        // mode is gone.
        "send now"
    } else {
        "send"
    }
}

fn actions(mode: ComposerMode, app: &dyn TuiState) -> Vec<(&'static str, &'static str)> {
    if app.diff_pane_focus() || app.diagram_focus() {
        return vec![("Up/Down", "scroll"), ("Esc", "back"), ("F1", "help")];
    }
    let primary = primary_action(
        mode,
        app.next_prompt_new_session_armed(),
        app.is_processing(),
    );
    let mut actions = vec![("Enter", primary)];
    if app.is_processing() {
        actions.push(("Esc", "stop"));
    }
    actions.push(("F1", "help"));
    if app.input().is_empty() && !mode.is_shell() {
        actions.push(("/", "commands"));
        actions.push(("@", "files"));
    }
    actions.push(("Alt+Enter", "newline"));
    actions.push(("Ctrl+R", "history"));
    // One newline affordance, not three. Shift+Enter (kitty keyboard
    // protocol), Alt/Option+Enter, and a trailing `\` then Enter all insert
    // a newline (`newline::enter_inserts_newline`). Listing the last two as
    // separate "Alt+Enter newline" and "\+Enter newline" rows spent footer
    // width repeating one action and made the footer look uncurated.
    actions.push(("Ctrl+F", "find"));
    actions
}

fn action_line(actions: &[(&str, &str)], width: usize) -> Line<'static> {
    if let [("Enter", label), ("Esc", "stop"), ..] = actions
        && width < "Enter  · Esc stop".width() + label.width()
    {
        if width >= "↵  · Esc stop".width() + label.width() {
            return action_line(&[("↵", label), ("Esc", "stop")], width);
        }
        return action_line(&[("Esc", "stop")], width);
    }
    let mut spans = Vec::new();
    let mut used = 0;
    for (index, (key, label)) in actions.iter().enumerate() {
        let needed = key.width() + label.width() + 1 + usize::from(!spans.is_empty()) * 3;
        if used + needed > width {
            // At very small widths keep the primary key discoverable instead
            // of dropping it in favour of a shorter secondary shortcut.
            if index == 0 {
                let compact_key = if *key == "Enter" { "↵" } else { key };
                let compact = format!("{compact_key} {label}");
                let text = if compact.width() <= width {
                    compact
                } else if key.width() <= width {
                    key.to_string()
                } else if compact_key.width() <= width {
                    compact_key.to_string()
                } else {
                    String::new()
                };
                spans.push(Span::styled(
                    text,
                    Style::default()
                        .fg(BrandTheme::accent())
                        .add_modifier(Modifier::BOLD),
                ));
            }
            // Preserve action priority across resizes. Do not replace a
            // hidden primary action with an unrelated, shorter shortcut.
            break;
        }
        if !spans.is_empty() {
            spans.push(Span::styled(" · ", Style::default().fg(BrandTheme::dim())));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default().fg(BrandTheme::dim_bright()),
        ));
        used += needed;
    }
    Line::from(spans)
}

pub(super) fn draw_footer(
    frame: &mut Frame,
    app: &dyn TuiState,
    mode: ComposerMode,
    area: Rect,
    first_row: usize,
    visible_rows: usize,
    total_rows: usize,
) {
    if area.height < 3 || area.width < 11 {
        return;
    }
    // In a narrow split the border corners are enough of an inset. The
    // reclaimed cells leave room for the full stop action.
    let gutter = if area.width < 24 { 1 } else { 3 };
    let width = usize::from(area.width - 2 * gutter);
    let position = if total_rows > visible_rows && visible_rows > 0 {
        format!(
            " {}-{}/{} ",
            first_row + 1,
            (first_row + visible_rows).min(total_rows),
            total_rows
        )
    } else if total_rows > 1 {
        let before_cursor = app.input().get(..app.cursor_pos()).unwrap_or("");
        let line = before_cursor.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = before_cursor
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .graphemes(true)
            .count()
            + 1;
        format!(" Ln {line}, Col {column} ")
    } else {
        String::new()
    };
    let position_width = position.width();
    // Reserve the actual primary shortcut plus the gutter. A fixed reserve
    // could hide "Enter send now" behind line/column metadata on narrow frames.
    let actions = actions(mode, app);
    let primary_width = actions
        .first()
        .map(|(key, label)| key.width() + label.width() + 1)
        .unwrap_or(0);
    let stop_width = usize::from(actions.get(1) == Some(&("Esc", "stop"))) * " · Esc stop".width();
    let show_position =
        !position.is_empty() && width >= position_width + primary_width + stop_width + 2;
    let left_width = if show_position {
        width - position_width - 2
    } else {
        width
    };
    let line = action_line(&actions, left_width);
    let row = area.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(line),
        Rect::new(area.x + gutter, row, left_width as u16, 1),
    );
    if show_position {
        frame.render_widget(
            Paragraph::new(position).style(Style::default().fg(BrandTheme::dim_bright())),
            Rect::new(
                area.right() - gutter - position_width as u16,
                row,
                position_width as u16,
                1,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_action_labels_follow_enter_routing_priority() {
        assert_eq!(primary_action(ComposerMode::Chat, false, false), "send");
        assert_eq!(primary_action(ComposerMode::Chat, false, true), "send now");
        assert_eq!(primary_action(ComposerMode::Chat, true, true), "new chat");
        assert_eq!(
            primary_action(ComposerMode::SlashCommand, true, true),
            "choose"
        );
        assert_eq!(primary_action(ComposerMode::ShellRemote, true, true), "run");
    }

    #[test]
    fn composer_actions_fit_every_width_without_partial_shortcuts() {
        let actions = [("Enter", "send"), ("Alt+Enter", "newline"), ("F1", "help")];
        for width in 0..200 {
            let line = action_line(&actions, width);
            assert!(line.width() <= width);
            if width >= 10 {
                assert!(line.spans.iter().any(|span| span.content == "Enter"));
            }
        }
    }

    #[test]
    fn narrow_footer_does_not_promote_secondary_actions_over_primary() {
        let actions = [("Enter", "send now"), ("F1", "help"), ("/", "commands")];
        assert_eq!(action_line(&actions, 4).width(), 1);
        let line = action_line(&actions, 10);
        assert_eq!(
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "↵ send now"
        );
        let line = action_line(&actions, 24);
        assert!(line.spans.iter().any(|span| span.content == "F1"));
        assert!(!line.spans.iter().any(|span| span.content == "/"));
    }

    #[test]
    fn running_footer_keeps_stop_visible_at_narrow_widths() {
        let actions = [("Enter", "send now"), ("Esc", "stop"), ("F1", "help")];
        for width in 5..=30 {
            let line = action_line(&actions, width);
            assert!(line.width() <= width);
            assert!(line.spans.iter().any(|span| span.content == "Esc"));
        }
    }
}
