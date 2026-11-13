//! A small, non-interactive guide: setup commands never submit a model prompt.
use ratatui::{
    layout::Alignment,
    style::{Modifier, Style},
    text::{Line, Span},
};

pub(super) const GUIDE_HEIGHT: u16 = 8;

const COMMANDS: [(&str, &str, &str); 5] = [
    (
        "/login",
        "Choose a provider and sign in",
        "Provider sign-in",
    ),
    ("/model", "Select an available model", "Choose a model"),
    (
        "/refresh-model-list",
        "Refresh missing or stale models",
        "Refresh models",
    ),
    (
        "/thinking-display",
        "Thinking text: off / full / current",
        "Thinking text",
    ),
    (
        "/effort",
        "Set reasoning effort (model-dependent)",
        "Reasoning effort",
    ),
];

pub(super) fn build_lines(width: u16, centered: bool) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let width = usize::from(width);
    let alignment = if centered {
        Alignment::Center
    } else {
        Alignment::Left
    };
    let pad = if centered || width < 40 { "" } else { "  " };
    let accent = Style::default()
        .fg(super::accent_color())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(super::dim_color());
    let command_width = COMMANDS
        .iter()
        .map(|(command, _, _)| command.len())
        .max()
        .unwrap_or(0);
    let mut lines = Vec::with_capacity(usize::from(GUIDE_HEIGHT));
    lines.push(Line::from(Span::styled(
        format!("{pad}Quick start"),
        accent,
    )));
    for (command, description, short) in COMMANDS {
        let mut spans = vec![Span::raw(pad), Span::styled(command, accent)];
        // Never sacrifice the command to make room for explanatory prose.
        // Short descriptions keep all five commands readable on small panes.
        if width >= 38 {
            spans.push(Span::raw(
                " ".repeat(command_width.saturating_sub(command.len()) + 2),
            ));
            spans.push(Span::styled(
                if width >= 64 { description } else { short },
                dim,
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    let footer = if width >= 64 {
        "Enter sends · / opens commands · Tab completes · /help for more"
    } else if width >= 38 {
        "Enter sends · / commands · /help"
    } else {
        "More: /help"
    };
    lines.push(Line::from(Span::styled(format!("{pad}{footer}"), dim)));
    lines
        .into_iter()
        .map(|line| super::truncate_line_with_ellipsis_to_width(&line, width).alignment(alignment))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn quick_start_buffer_shows_every_setup_command() {
        for width in [24, 40, 64, 80, 120] {
            for centered in [false, true] {
                let mut terminal = Terminal::new(TestBackend::new(width, GUIDE_HEIGHT)).unwrap();
                terminal
                    .draw(|frame| {
                        frame.render_widget(
                            Paragraph::new(build_lines(width, centered)),
                            frame.area(),
                        );
                    })
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let text = (0..GUIDE_HEIGHT)
                    .map(|y| {
                        (0..width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for (command, _, _) in COMMANDS {
                    assert!(
                        text.contains(command),
                        "{width} columns, centered={centered}: {text}"
                    );
                }
                assert!(text.contains("/help"), "{text}");
                assert!(!text.contains("Press 1-"), "{text}");
            }
        }
    }

    #[test]
    fn quick_start_rows_fit_even_zero_and_tiny_widths() {
        for width in 0..=120 {
            for line in build_lines(width, false) {
                assert!(
                    UnicodeWidthStr::width(super::super::line_plain_text(&line).as_str())
                        <= usize::from(width)
                );
            }
        }
    }

    #[test]
    fn quick_start_distinguishes_thinking_text_from_reasoning_effort() {
        let text = build_lines(100, false)
            .iter()
            .map(super::super::line_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("off / full / current"));
        assert!(text.contains("model-dependent"));
        assert!(text.contains("Choose a provider and sign in"));
    }
}
