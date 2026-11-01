//! Stable, width-aware composer guidance. All chrome occupies existing borders.
use super::{ComposerMode, TuiState};
use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use ratatui::{prelude::*, widgets::Paragraph};

fn primary_action(
    mode: ComposerMode,
    new_session: bool,
    processing: bool,
    queue: bool,
) -> &'static str {
    // Explicit command/shell modes consume Enter before normal chat routing.
    if mode.is_shell() {
        "run"
    } else if mode == ComposerMode::SlashCommand {
        "choose"
    } else if new_session {
        "new chat"
    } else if processing && queue {
        "queue"
    } else if processing {
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
        app.queue_mode(),
    );
    let mut actions = vec![("Enter", primary)];
    if app.is_processing() {
        actions.push(("Esc", "stop"));
    }
    actions.push(("Alt+Enter", "newline"));
    actions.push(("F1", "help"));
    actions
}

fn action_line(actions: &[(&str, &str)], width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    let mut used = 0;
    for (key, label) in actions {
        let needed = key.len() + label.len() + 1 + usize::from(!spans.is_empty()) * 3;
        if used + needed > width {
            continue;
        }
        if !spans.is_empty() {
            spans.push(Span::styled(" · ", Style::default().fg(BrandTheme::dim())));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(BrandTheme::dim_bright())
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
    if area.height < 3 || area.width < 16 {
        return;
    }
    let width = usize::from(area.width.saturating_sub(6));
    let position = if total_rows > visible_rows && visible_rows > 0 {
        format!(
            " {}-{}/{} ",
            first_row + 1,
            (first_row + visible_rows).min(total_rows),
            total_rows
        )
    } else if total_rows > 1 {
        format!(" {total_rows} lines ")
    } else {
        String::new()
    };
    let position_width = position.len();
    // Keep the primary action visible before adding secondary draft metadata.
    let show_position = !position.is_empty() && width >= position_width + 14;
    let left_width = if show_position {
        width - position_width - 2
    } else {
        width
    };
    let line = action_line(&actions(mode, app), left_width);
    let row = area.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(line),
        Rect::new(area.x + 3, row, left_width as u16, 1),
    );
    if show_position {
        frame.render_widget(
            Paragraph::new(position).style(Style::default().fg(BrandTheme::dim_bright())),
            Rect::new(
                area.right() - 3 - position_width as u16,
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
        assert_eq!(
            primary_action(ComposerMode::Chat, false, false, true),
            "send"
        );
        assert_eq!(
            primary_action(ComposerMode::Chat, false, true, true),
            "queue"
        );
        assert_eq!(
            primary_action(ComposerMode::Chat, false, true, false),
            "send now"
        );
        assert_eq!(
            primary_action(ComposerMode::Chat, true, true, true),
            "new chat"
        );
        assert_eq!(
            primary_action(ComposerMode::SlashCommand, true, true, true),
            "choose"
        );
        assert_eq!(
            primary_action(ComposerMode::ShellRemote, true, true, true),
            "run"
        );
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
}
