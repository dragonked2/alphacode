//! Buffer-level regressions for the modern composer and floating input menus.

use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Position, style::Color};

fn draw_composer(
    state: &TestState,
    width: u16,
    height: u16,
) -> (Terminal<TestBackend>, Option<Position>) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut cursor = None;
    terminal
        .draw(|frame| {
            let mut capture = None;
            cursor = input_ui::draw_input(
                frame,
                state,
                Rect::new(0, 0, width, height),
                1,
                &mut capture,
            );
        })
        .expect("composer frame");
    (terminal, cursor)
}

fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn centered_keyboard_selection_highlights_the_selected_text_cells() {
    let state = TestState {
        input: "alpha bravo".to_string(),
        cursor_pos: "alpha bravo".len(),
        input_selection: Some(("alpha ".len(), "alpha bravo".len())),
        centered_mode: true,
        suppress_info_widgets: true,
        ..Default::default()
    };
    let (terminal, cursor) = draw_composer(&state, 44, 5);
    assert!(cursor.is_some());
    let rendered = rows(&terminal);
    let (y, row) = rendered
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("alpha bravo"))
        .expect("draft must be visible");
    // `row.find` returns a BYTE offset. The frame border is a multi-byte
    // glyph (`│` is 3 bytes), so using it directly as a buffer column would
    // address the wrong cell. Convert through the char index instead.
    let input_x = row[..row.find("alpha bravo").expect("input start")]
        .chars()
        .count() as u16;
    let buffer = terminal.backend().buffer();
    for x in input_x + 6..input_x + 11 {
        assert_ne!(
            buffer[(x, y as u16)].bg,
            Color::Reset,
            "selected cell {x} was not highlighted\n{}",
            rendered.join("\n")
        );
    }
    assert_eq!(buffer[(input_x, y as u16)].bg, Color::Reset);
}

#[test]
fn centered_history_ghost_text_does_not_move_the_caret() {
    let base = TestState {
        input: "git".to_string(),
        cursor_pos: 3,
        centered_mode: true,
        suppress_info_widgets: true,
        ..Default::default()
    };
    let (_, without_preview) = draw_composer(&base, 44, 5);
    let with_preview = TestState {
        history_completion: Some(" status".to_string()),
        ..base
    };
    let (terminal, with_preview_cursor) = draw_composer(&with_preview, 44, 5);
    assert_eq!(with_preview_cursor, without_preview);
    assert!(
        rows(&terminal).iter().any(|row| row.contains("git status")),
        "ghost completion should remain visible without joining editable geometry"
    );
}

#[test]
fn command_menu_is_framed_left_aligned_and_keeps_the_selected_token_visible() {
    let state = TestState {
        input: "/very".to_string(),
        cursor_pos: 5,
        command_options: vec![
            (
                "/very-long-command-token".to_string(),
                "Run the focused action",
            ),
            ("/other".to_string(), "Another action"),
        ],
        selected_command: 0,
        suppress_info_widgets: true,
        ..Default::default()
    };
    let backend = TestBackend::new(60, 18);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            input_ui::draw_command_suggestions_overlay(frame, &state, Rect::new(2, 13, 56, 3));
        })
        .expect("command menu");
    let rendered = rows(&terminal);
    assert!(rendered.iter().any(|row| row.contains("Commands")));
    let selected_y = rendered
        .iter()
        .position(|row| row.contains("/very-long-command-token"))
        .expect("full selected command token");
    assert!(rendered[selected_y].contains("▸ /very-long-command-token"));

    let buffer = terminal.backend().buffer();
    let selected_backgrounds: std::collections::HashSet<_> =
        (3..57).map(|x| buffer[(x, selected_y as u16)].bg).collect();
    assert_eq!(
        selected_backgrounds.len(),
        1,
        "selected menu row should be one continuous surface"
    );
    assert!(!selected_backgrounds.contains(&Color::Reset));
}

#[test]
fn history_menu_clamps_selection_and_sanitizes_multiline_text() {
    let state = TestState {
        history_search: Some(crate::alphacode_tui::tui::PromptHistorySearchView {
            query: "fix\nparser".to_string(),
            matches: vec![
                "first prompt".to_string(),
                "second\nprompt".to_string(),
                "selected prompt".to_string(),
            ],
            selected: usize::MAX,
        }),
        suppress_info_widgets: true,
        ..Default::default()
    };
    let backend = TestBackend::new(52, 18);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            input_ui::draw_prompt_history_search_overlay(frame, &state, Rect::new(2, 13, 48, 3));
        })
        .expect("history menu");
    let rendered = rows(&terminal).join("\n");
    assert!(rendered.contains("Prompt history"));
    assert!(rendered.contains("fix parser"));
    assert!(rendered.contains("second prompt"));
    assert!(rendered.contains("▸ selected prompt"));
}
