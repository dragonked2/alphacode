//! Buffer-level regression tests for the chat-column vertical layout.
//!
//! Two failures motivated this file, and both are invisible to a test that
//! only inspects `Line`s instead of real terminal cells:
//!
//! 1. The composer was not pinned to the bottom of the screen. Every vertical
//!    chunk was a `Constraint::Length`, and ratatui's default `Flex::Start`
//!    parks whatever the constraints do not claim in the *final* spacer — so
//!    the slack landed below the composer. On a 100x40 terminal with a short
//!    conversation the composer rendered at row 26 with 13 blank rows under
//!    it, and it slid one row further down for every transcript line that
//!    arrived.
//! 2. A full-width gradient rule was painted at `chunks[3].y - 1`, which is the
//!    transcript's own last row whenever the transcript fills its chunk. That
//!    erased the newest transcript line: the prompt the user had just sent
//!    (nothing is appended after it until the first token arrives), the live
//!    streaming line, and the legacy `↓N` overflow indicator, which lives on
//!    that same row.

use super::*;
use crate::alphacode_tui::tui::ui::clear_flicker_frame_history_for_tests;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const SENT_PROMPT: &str = "SENTNOW please fix the parser";

fn layout_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buf = terminal.backend().buffer();
    let width = buf.area.width;
    let height = buf.area.height;
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn draw_frame(state: &TestState, width: u16, height: u16) -> (Vec<String>, LayoutSnapshot) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::alphacode_tui::tui::ui::draw(frame, state))
        .expect("layout frame");
    let rows = layout_rows(&terminal);
    let layout = crate::alphacode_tui::tui::ui::last_layout_snapshot().expect("layout snapshot");
    (rows, layout)
}

fn working_state(messages: Vec<DisplayMessage>, streaming: Option<&str>) -> TestState {
    TestState {
        status: ProcessingStatus::Sending,
        streaming_text: streaming.unwrap_or_default().to_string(),
        suppress_info_widgets: true,
        display_messages: messages,
        messages_version: 1,
        provider_name: Some("openai".to_string()),
        provider_model: Some("gpt-5.6-sol".to_string()),
        ..Default::default()
    }
}

#[test]
fn composer_is_pinned_to_the_bottom_of_the_chat_column() {
    let _lock = viewport_snapshot_test_lock();
    clear_flicker_frame_history_for_tests();
    // A short conversation on a tall terminal is exactly the case the packed
    // layout used to get wrong: plenty of slack, all of it handed to the final
    // spacer.
    let state = working_state(
        vec![DisplayMessage::user("first question")],
        Some("partial answer"),
    );

    for (width, height) in [(100_u16, 40_u16), (80, 24), (120, 60), (60, 20)] {
        clear_flicker_frame_history_for_tests();
        let (rows, layout) = draw_frame(&state, width, height);
        let input = layout.input_area.expect("input area");
        assert_eq!(
            input.bottom(),
            height,
            "the composer must end flush with the bottom of the frame \
             ({width}x{height}), got {input:?}\n{}",
            rows.join("\n")
        );
        let status = crate::alphacode_tui::tui::ui::last_status_area().expect("status area");
        assert!(
            status.bottom() <= input.y,
            "the status row must sit above the composer ({width}x{height})"
        );
        assert!(
            input.y > layout.messages_area.y,
            "the composer must sit below the transcript ({width}x{height})"
        );
    }
}

#[test]
fn bottom_chrome_stays_put_while_the_transcript_grows() {
    let _lock = viewport_snapshot_test_lock();
    clear_flicker_frame_history_for_tests();

    // The regression was not just "the composer floated": it moved every frame.
    // Hold the bottom chrome still while the transcript grows underneath it.
    let mut input_top = None;
    for answer in [
        "",
        "one line",
        "a slightly longer answer that wraps across the composer width of this terminal",
    ] {
        clear_flicker_frame_history_for_tests();
        let state = working_state(vec![DisplayMessage::user("first question")], Some(answer));
        let (rows, layout) = draw_frame(&state, 100, 40);
        let y = layout.input_area.expect("input area").y;
        match input_top {
            None => input_top = Some(y),
            Some(previous) => assert_eq!(
                previous,
                y,
                "the composer must not move as the transcript grows:\n{}",
                rows.join("\n")
            ),
        }
    }
    assert!(
        input_top.unwrap() < 40,
        "sanity: the composer is on screen, not past the bottom"
    );
}

#[test]
fn freshly_sent_prompt_is_visible_while_the_agent_is_still_thinking() {
    let _lock = viewport_snapshot_test_lock();
    clear_flicker_frame_history_for_tests();

    // Nothing has streamed yet, so the prompt the user just sent *is* the last
    // transcript row. The gradient rule used to be painted straight over it.
    let state = working_state(
        vec![
            DisplayMessage::assistant("previous answer"),
            DisplayMessage::user(SENT_PROMPT),
        ],
        None,
    );
    let (rows, layout) = draw_frame(&state, 100, 40);
    let prompt_row = rows
        .iter()
        .position(|row| row.contains(SENT_PROMPT))
        .unwrap_or_else(|| {
            panic!(
                "the prompt just sent must be on screen before the first token arrives\n{}",
                rows.join("\n")
            )
        }) as u16;
    let messages = layout.messages_area;
    assert!(
        messages.y <= prompt_row && prompt_row < messages.bottom(),
        "the prompt must render inside the transcript viewport {messages:?}"
    );
    assert!(
        !rows[prompt_row as usize].contains('─'),
        "the prompt row must not be overwritten by a separator rule\n{}",
        rows.join("\n")
    );
}

#[test]
fn separator_rule_never_overwrites_the_last_transcript_line() {
    let _lock = viewport_snapshot_test_lock();
    clear_flicker_frame_history_for_tests();

    // A long transcript fills its chunk, so there is no slack row for a rule to
    // live in. The newest line must survive regardless.
    let mut messages = Vec::new();
    for index in 0..40 {
        messages.push(DisplayMessage::assistant(format!("assistant line {index}")));
        messages.push(DisplayMessage::user(format!("user line {index}")));
    }
    messages.push(DisplayMessage::user(SENT_PROMPT));
    let state = working_state(messages, Some("streaming tail line"));

    let (rows, layout) = draw_frame(&state, 100, 40);
    let messages_area = layout.messages_area;
    let last_row = &rows[messages_area.bottom() as usize - 1];
    assert!(
        !last_row.contains('─'),
        "the last transcript row must never be replaced by a gradient rule:\n{last_row}"
    );
    assert!(
        rows.iter().any(|row| row.contains("streaming tail line")),
        "the newest streaming line must be visible\n{}",
        rows.join("\n")
    );
}

#[test]
fn short_terminals_keep_composer_and_picker_visible() {
    use crate::alphacode_tui::tui::{
        InlineInteractiveState, PickerAction, PickerEntry, PickerKind, PickerOption,
    };
    let _lock = viewport_snapshot_test_lock();
    let entries: Vec<_> = (0..30)
        .map(|i| PickerEntry {
            name: format!("model-{i}"),
            options: vec![PickerOption::new(
                "provider".into(),
                "api".into(),
                true,
                String::new(),
                None,
            )],
            action: PickerAction::Model,
            selected_option: 0,
            is_current: false,
            is_default: false,
            is_favorite: false,
            recommended: false,
            recommendation_rank: usize::MAX,
            usage_score: 0,
            old: false,
            created_date: None,
            effort: None,
        })
        .collect();
    let state = TestState {
        input: "DRAFT sentinel".into(),
        suppress_info_widgets: true,
        inline_interactive_state: Some(InlineInteractiveState {
            kind: PickerKind::Model,
            filtered: (0..entries.len()).collect(),
            entries,
            selected: 0,
            column: 0,
            filter: String::new(),
            last_scored_filter: String::new(),
            preview: false,
        }),
        ..Default::default()
    };
    for height in [9, 12, 16, 24] {
        clear_flicker_frame_history_for_tests();
        let (rows, layout) = draw_frame(&state, 80, height);
        let input = layout.input_area.expect("composer");
        assert!(input.height >= 3, "height {height}: {input:?}");
        assert!(layout.messages_area.height >= 1, "height {height}");
        assert!(
            rows.iter().any(|row| row.contains("DRAFT sentinel")),
            "height {height}: {}",
            rows.join("\n")
        );
        assert!(
            rows.iter().any(|row| row.contains("MODEL")),
            "picker missing at height {height}: {}",
            rows.join("\n")
        );
    }
}

#[test]
fn long_draft_keeps_its_cursor_visible_even_with_one_input_row() {
    let _lock = viewport_snapshot_test_lock();
    let mut state = working_state(Vec::new(), None);
    state.input = (0..20)
        .map(|i| format!("draft-line-{i:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    state.cursor_pos = 0;
    let (rows, layout) = draw_frame(&state, 80, 5);
    assert_eq!(layout.input_area.expect("input").height, 3);
    assert!(
        rows.iter().any(|row| row.contains("draft-line-00")),
        "{}",
        rows.join("\n")
    );
    assert!(
        !rows.iter().any(|row| row.contains("draft-line-19")),
        "{}",
        rows.join("\n")
    );
}

#[test]
fn tiny_terminal_clears_old_hit_targets_and_shows_resize_hint() {
    let _lock = viewport_snapshot_test_lock();
    let state = working_state(Vec::new(), None);
    let _ = draw_frame(&state, 80, 24);
    let (rows, layout) = draw_frame(&state, 11, 4);
    assert!(
        rows.iter().any(|row| row.contains("Resize")),
        "{}",
        rows.join("\n")
    );
    assert!(layout.input_area.is_none());
    assert_eq!(layout.messages_area, Rect::default());
}
