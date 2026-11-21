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
    // Alt/Option+Enter and a trailing `\` then Enter are the same newline
    // action (as is Shift+Enter under the kitty keyboard protocol). The footer
    // used to list two of them as separate hints, spending width on a repeat.
    assert!(
        !footer.contains("\\+Enter"),
        "the footer must not repeat the newline hint: {footer}"
    );
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
fn compact_empty_composer_shows_command_and_file_discovery() {
    let _lock = viewport_snapshot_test_lock();
    use ratatui::{Terminal, backend::TestBackend};
    let mut terminal = Terminal::new(TestBackend::new(50, 3)).unwrap();
    let state = TestState::default();
    terminal.draw(|frame| {
        input_ui::draw_input(frame, &state, frame.area(), 1, &mut None);
    }).unwrap();
    let footer: String = (0..50).map(|x| terminal.backend().buffer()[(x, 2)].symbol()).collect();
    assert!(footer.contains("/ commands") && footer.contains("@ files"), "{footer}");
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
fn pinned_banner_stays_in_place_through_the_first_prompt() {
    let _lock = viewport_snapshot_test_lock();
    use ratatui::{Terminal, backend::TestBackend};

    for (width, height) in [(40, 10), (80, 24), (120, 45)] {
        let mut state = TestState {
            provider_name: Some("Test Provider".into()),
            provider_model: Some("gpt-5.2".into()),
            working_dir: Some("C:/projects/世界-workspace".into()),
            suppress_info_widgets: true,
            ..Default::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut first_banner = None;
        for status in [
            ProcessingStatus::Idle,
            ProcessingStatus::Sending,
            ProcessingStatus::Streaming,
            ProcessingStatus::Idle,
        ] {
            state.status = status;
            if matches!(state.status, ProcessingStatus::Sending) {
                state.display_messages.push(DisplayMessage::user("Hello"));
                state.messages_version += 1;
            } else if matches!(state.status, ProcessingStatus::Streaming) {
                state.streaming_text = "The first answer is streaming".into();
            } else if !state.streaming_text.is_empty() {
                state.streaming_text.clear();
                state
                    .display_messages
                    .push(DisplayMessage::assistant("Hello there"));
                state.messages_version += 1;
            }

            terminal.draw(|frame| draw(frame, &state)).unwrap();
            let layout = last_layout_snapshot().expect("chat layout");
            let buffer = terminal.backend().buffer();
            let banner = (0..layout.messages_area.y)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            let rendered = banner.join("\n");
            assert!(
                rendered.contains("GPT-5.2") && rendered.contains("Test Provider"),
                "{rendered}"
            );
            if height >= 24 {
                assert!(rendered.contains("-workspace"), "{rendered}");
                assert!(banner.len() > 2, "the full wordmark must stay visible");
            } else {
                assert!(rendered.contains("AlphaCode"), "{rendered}");
            }
            if let Some(first) = &first_banner {
                assert_eq!(&banner, first, "{width}x{height} after the first prompt");
            } else {
                first_banner = Some(banner);
            }
        }
        export_composer_review_frame(
            terminal.backend().buffer(),
            &format!("steady-banner-{width}x{height}"),
        );
    }
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
