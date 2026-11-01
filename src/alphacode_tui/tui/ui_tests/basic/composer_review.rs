#[test]
fn composer_review_matrix_keeps_caret_and_chrome_inside_the_frame() {
    let _lock = viewport_snapshot_test_lock();
    use ratatui::{Terminal, backend::TestBackend};
    for (width, height) in [(12, 5), (40, 10), (80, 24), (120, 32), (200, 45)] {
        let draft = (1..=24)
            .map(|n| format!("Line {n}: 世界 e\u{301} 👩‍💻 مرحبا"))
            .collect::<Vec<_>>()
            .join("\n");
        for (name, input, status) in [
            ("idle", String::new(), ProcessingStatus::Idle),
            ("draft", draft.clone(), ProcessingStatus::Idle),
            (
                "streaming",
                "Please also check the tests".into(),
                ProcessingStatus::Streaming,
            ),
            ("shell", "! git status".into(), ProcessingStatus::Idle),
            ("commands", "/mo".into(), ProcessingStatus::Idle),
        ] {
            let state = TestState {
                cursor_pos: input.len(),
                input,
                status,
                display_messages: if name == "streaming" {
                    vec![
                        DisplayMessage::user("Review this project and improve its reliability."),
                        DisplayMessage::assistant(
                            "I found three improvements:\n\n- Correct draft navigation\n- Reuse input geometry\n- Keep more space for the conversation\n\nI am checking the changes now.",
                        ),
                    ]
                } else {
                    Vec::new()
                },
                work_tasks: if name == "streaming" {
                    vec![("Checking 世界 👩‍💻 and the project tests".into(), "running")]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
            let mut caret = None;
            terminal
                .draw(|frame| {
                    caret = input_ui::draw_input(frame, &state, frame.area(), 1, &mut None);
                })
                .expect("composer render");
            if let Some(caret) = caret {
                let inner = input_ui::composer_inner_area(Rect::new(0, 0, width, height));
                assert!(
                    inner.contains(caret),
                    "{name} {width}x{height}: {caret:?} outside {inner:?}"
                );
            }
            if width >= 56 {
                let title: String = (0..width)
                    .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
                    .collect();
                assert!(
                    title.contains(&crate::alphacode_brand::widget_title()),
                    "brand collided with work ribbon: {title}"
                );
            }
            export_composer_review_frame(
                terminal.backend().buffer(),
                &format!("composer-{name}-{width}x{height}"),
            );

            // Exercise the real page layout as well as the isolated composer.
            terminal
                .draw(|frame| draw(frame, &state))
                .expect("page render");
            if let Some(snapshot) = last_layout_snapshot() {
                for area in [Some(snapshot.messages_area), snapshot.input_area]
                    .into_iter()
                    .flatten()
                {
                    assert!(rect_within_bounds(area, Rect::new(0, 0, width, height)));
                }
            }
            export_composer_review_frame(
                terminal.backend().buffer(),
                &format!("page-{name}-{width}x{height}"),
            );
        }
    }
}

#[test]
fn composer_review_scroll_feedback_and_shortcuts_share_the_existing_border() {
    let _lock = viewport_snapshot_test_lock();
    use ratatui::{Terminal, backend::TestBackend};
    let input = (1..=24)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let state = TestState {
        cursor_pos: input.len(),
        input,
        ..Default::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(80, 6)).unwrap();
    terminal
        .draw(|frame| {
            input_ui::draw_input(frame, &state, frame.area(), 1, &mut None);
        })
        .unwrap();
    let footer: String = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 5)].symbol())
        .collect();
    assert!(footer.contains("Enter send"), "{footer}");
    assert!(footer.contains("21-24/24"), "{footer}");
    assert!(footer.contains("Alt+Enter newline"), "{footer}");
}

#[test]
fn composer_review_collapsed_interior_does_not_publish_a_cursor() {
    let _lock = viewport_snapshot_test_lock();
    use ratatui::{Terminal, backend::TestBackend};
    let state = TestState::default();
    let mut terminal = Terminal::new(TestBackend::new(40, 2)).unwrap();
    let mut caret = None;
    terminal
        .draw(|frame| {
            caret = input_ui::draw_input(frame, &state, frame.area(), 1, &mut None);
        })
        .unwrap();
    assert!(caret.is_none());
}

#[test]
fn composer_mouse_mapping_tracks_the_visible_draft_not_the_tail() {
    let _lock = viewport_snapshot_test_lock();
    let input = (1..=24)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let area = Rect::new(0, 0, 80, 6);
    for (caret, first_visible) in [
        ("line 3", "line 1"),
        ("line 13", "line 10"),
        ("line 24", "line 21"),
    ] {
        let state = TestState {
            cursor_pos: input.find(caret).unwrap(),
            input: input.clone(),
            ..Default::default()
        };
        assert_eq!(
            input_ui::input_cursor_pos_from_screen(&state, area, 1, 3, 1),
            input.find(first_visible)
        );
        assert_eq!(
            input_ui::input_cursor_pos_from_screen(&state, area, 1, 3, 5),
            None,
            "footer is not draft text"
        );
    }
}

#[test]
fn composer_mouse_mapping_does_not_reserve_a_hidden_hint_on_short_windows() {
    let state = TestState {
        input: "hello".into(),
        cursor_pos: 5,
        status: ProcessingStatus::Streaming,
        ..Default::default()
    };
    assert_eq!(
        input_ui::input_cursor_pos_from_screen(&state, Rect::new(0, 0, 80, 3), 1, 3, 1),
        Some(0)
    );
}

#[test]
fn conversation_banner_preserves_its_size_during_and_after_chat() {
    let idle = TestState::default();
    let welcome = header::conversation_banner_height(&idle, 120, 45);
    assert!(welcome > 1);
    let mut working = TestState {
        status: ProcessingStatus::Streaming,
        display_messages: vec![DisplayMessage::user("hello")],
        ..Default::default()
    };
    assert_eq!(header::conversation_banner_height(&working, 120, 45), welcome);
    working.status = ProcessingStatus::Idle;
    assert_eq!(header::conversation_banner_height(&working, 120, 45), welcome);
    assert_eq!(header::conversation_banner_height(&working, 40, 10), 0);
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
    let mut terminal = Terminal::new(TestBackend::new(120, welcome)).unwrap();
    terminal.draw(|frame| {
        let lines = header::build_banner_band_lines(&working, 120, welcome);
        frame.render_widget(Paragraph::new(lines), frame.area());
    }).unwrap();
    let rendered: String = terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
    assert!(rendered.contains(crate::alphacode_brand::WORDMARK_BLOCK[0].trim()));
    export_composer_review_frame(terminal.backend().buffer(), "conversation-banner-active");
}

#[test]
fn composer_guidance_follows_side_pane_focus() {
    let _lock = viewport_snapshot_test_lock();
    use ratatui::{Terminal, backend::TestBackend};
    let state = TestState {
        side_pane_focused: true,
        ..Default::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    terminal
        .draw(|frame| {
            input_ui::draw_input(frame, &state, frame.area(), 1, &mut None);
        })
        .unwrap();
    let footer: String = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 2)].symbol())
        .collect();
    assert!(
        footer.contains("Up/Down scroll") && footer.contains("Esc back"),
        "{footer}"
    );
    assert!(
        !footer.contains("Enter send"),
        "inactive composer must not advertise submission"
    );
}

#[test]
fn long_transcripts_do_not_wrap_terminal_coordinate_limits() {
    assert!(!transcript_overflows(20, 3, 24));
    assert!(transcript_overflows(23, 3, 24));
    assert!(transcript_overflows(65_536, 3, u16::MAX));
    assert!(transcript_overflows(65_530, 10, 100));
    assert!(transcript_overflows(usize::MAX, 25, u16::MAX));
}

// Optional review artifacts contain actual TestBackend cells and styles. No
// files are written during ordinary test runs.
fn export_composer_review_frame(buffer: &ratatui::buffer::Buffer, name: &str) {
    let Some(directory) = std::env::var_os("ALPHACODE_UI_REVIEW_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&path).expect("review directory");
    let cells: Vec<_> = buffer
        .content()
        .iter()
        .map(|cell| {
            serde_json::json!({
                "text": cell.symbol(), "fg": format!("{:?}", cell.fg),
                "bg": format!("{:?}", cell.bg), "bold": cell.modifier.contains(Modifier::BOLD),
            })
        })
        .collect();
    let json = serde_json::json!({"width": buffer.area.width, "height": buffer.area.height, "cells": cells});
    std::fs::write(
        path.join(format!("{name}.json")),
        serde_json::to_vec(&json).unwrap(),
    )
    .expect("review export");
}
