// Issue #544: stray Enter after a bracketed paste must not submit.
#[test]
fn control_v_terminal_delivery_collapses_like_alt_v() {
    use crate::alphacode_tui::bus::{
        ClipboardPasteCompleted, ClipboardPasteContent, ClipboardPasteKind,
    };

    for terminal_delivered in [false, true] {
        let mut app = create_test_app();
        let before = "Explain:  please";
        let cursor = "Explain: ".len();
        let text = "first\nsecond 👩‍💻";
        app.set_input_for_test(before);
        app.cursor_pos = cursor;
        let session_id = app.active_client_session_id().unwrap().to_string();
        app.clipboard_paste_draft = Some((session_id.clone(), before.to_string(), cursor));
        if terminal_delivered {
            app.input.insert_str(cursor, text);
            app.cursor_pos += text.len();
        }
        assert!(
            app.handle_clipboard_paste_completed(ClipboardPasteCompleted {
                session_id,
                kind: ClipboardPasteKind::Smart,
                content: ClipboardPasteContent::Text(text.to_string()),
            })
        );
        assert_eq!(app.input, "Explain: [pasted content] please");
        assert_eq!(app.pasted_contents, vec![text.to_string()]);
        assert_eq!(
            app.expand_paste_placeholders(&app.input.clone()),
            "Explain: first\nsecond 👩‍💻 please"
        );
    }
}

#[test]
fn pasting_matching_text_does_not_silently_discard_the_paste() {
    let mut app = create_test_app();
    app.set_input_for_test("hello");
    app.cursor_pos = app.input.len();
    app.handle_paste("hello".to_string());
    assert_eq!(app.input, "hello[pasted content]");
    assert_eq!(
        app.expand_paste_placeholders(&app.input.clone()),
        "hellohello"
    );
}

#[test]
fn control_v_does_not_duplicate_a_terminal_paste_token() {
    use crate::alphacode_tui::bus::{
        ClipboardPasteCompleted, ClipboardPasteContent, ClipboardPasteKind,
    };
    let mut app = create_test_app();
    let session_id = app.active_client_session_id().unwrap().to_string();
    app.clipboard_paste_draft = Some((session_id.clone(), String::new(), 0));
    app.handle_paste("hello\nworld".to_string());
    app.handle_clipboard_paste_completed(ClipboardPasteCompleted {
        session_id,
        kind: ClipboardPasteKind::Smart,
        content: ClipboardPasteContent::Text("hello\nworld".to_string()),
    });
    assert_eq!(app.input, "[pasted content]");
    assert_eq!(app.pasted_contents.len(), 1);
}

#[test]
fn bare_enter_immediately_after_paste_does_not_submit() {
    // Windows Terminal / conhost sends a separate bare Enter key event after
    // a bracketed paste ending with \n; it must not submit the chat (#544).
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = create_test_app();
    crate::alphacode_tui::tui::app::input::handle_paste(&mut app, "hello world\n".to_string());
    assert_eq!(app.input, "[pasted content]");

    app.handle_key_press_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(
        !app.is_processing,
        "Enter right after paste must not submit"
    );
    assert!(
        !app.input.is_empty(),
        "input should be preserved after paste"
    );

    // A later, human-timed Enter still submits.  Expire only the timestamp so
    // this also catches a regression where `Event::Paste` leaves the
    // in-flight flag set forever.
    crate::alphacode_tui::tui::app::input::paste_guard_expire_trailing_window_for_test();
    app.handle_key_press_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.input.is_empty(), "later Enter should submit normally");
}

// Keymap-independent multi-line input: a trailing backslash makes Enter insert
// a newline instead of submitting.
//
// Motivation: both newline chords (Shift+Enter, Option/Alt+Enter) require the
// terminal to disambiguate modified Enter via the kitty keyboard protocol.
// macOS Terminal.app and default-profile iTerm2 send a bare CR instead, so
// those users previously had no way to type a multi-line prompt.

#[test]
fn trailing_backslash_enter_inserts_newline_instead_of_submitting() {
    let mut app = create_test_app();
    app.set_input_for_test("first\\");
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert_eq!(app.input, "first\n");
    assert_eq!(app.cursor_pos, app.input.len());
    assert!(
        app.display_messages().is_empty(),
        "the draft must not be submitted: {:?}",
        app.display_messages()
    );
}

#[test]
fn plain_enter_still_submits_without_a_trailing_backslash() {
    let mut app = create_test_app();
    app.set_input_for_test("first");
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert!(app.input.is_empty(), "input should be consumed by submit");
}

#[test]
fn escaped_double_backslash_is_literal_text_and_still_submits() {
    let mut app = create_test_app();
    app.set_input_for_test("path\\\\");
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert!(
        app.input.is_empty(),
        "an escaped backslash is literal, so Enter submits: {:?}",
        app.input
    );
}

#[test]
fn backslash_continuation_only_applies_at_the_end_of_the_draft() {
    let mut app = create_test_app();
    app.set_input_for_test("a\\b");
    app.cursor_pos = 2; // just after the backslash, not at end

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("enter should be handled");

    assert!(
        app.input.is_empty(),
        "mid-draft backslash must not become a continuation: {:?}",
        app.input
    );
}

#[test]
fn repeated_continuations_build_a_multi_line_draft() {
    let mut app = create_test_app();
    for line in ["one\\", "two\\"] {
        for c in line.chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
                .expect("char should be handled");
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::empty())
            .expect("enter should be handled");
    }
    for c in "three".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
            .expect("char should be handled");
    }

    assert_eq!(app.input, "one\ntwo\nthree");
}

#[test]
fn native_control_v_is_one_draft_in_local_remote_and_streaming_modes() {
    use crate::alphacode_tui::tui::terminal_events::CoalescedEvents;
    use crossterm::event::{Event, KeyEvent};
    use futures::StreamExt;
    let text = "x\n".to_owned() + &"next line 世界 👩‍💻\n".repeat(200);
    for remote_mode in [false, true] {
        for processing in [false, true] {
            let mut app = create_test_app();
            app.is_remote = remote_mode;
            app.is_processing = processing;
            app.status = if processing {
                ProcessingStatus::Streaming
            } else {
                ProcessingStatus::Idle
            };
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let _entered = runtime.enter();
            let mut connection = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
            runtime.block_on(async {
                let events = text.chars().map(|c| {
                    Ok::<_, std::io::Error>(Event::Key(KeyEvent::new(
                        if c == '\n' {
                            KeyCode::Enter
                        } else {
                            KeyCode::Char(c)
                        },
                        KeyModifiers::NONE,
                    )))
                });
                let mut source = CoalescedEvents::from_source(futures::stream::iter(events));
                while let Some(event) = source.next().await {
                    match event.unwrap() {
                        Event::Paste(text) => app.handle_paste(text),
                        Event::Key(key) if remote_mode => {
                            app.handle_remote_key(key.code, key.modifiers, &mut connection)
                                .await
                                .unwrap();
                        }
                        Event::Key(key) => {
                            app.handle_key_press_event(key).unwrap();
                        }
                        _ => panic!("unexpected input event"),
                    }
                }
                assert_eq!(app.input, "[pasted content]");
                assert_eq!(app.pasted_contents, vec![text.clone()]);
                assert_eq!(app.expand_paste_placeholders(&app.input.clone()), text);
                assert!(app.interleave_message.is_none());
                assert!(app.pending_soft_interrupt_requests.is_empty());
                if remote_mode {
                    app.handle_remote_key(KeyCode::Enter, KeyModifiers::NONE, &mut connection)
                        .await
                        .unwrap();
                } else {
                    app.handle_key_press_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                        .unwrap();
                }
                assert_eq!(
                    app.input, "[pasted content]",
                    "trailing paste Enter submitted the draft"
                );
            });
        }
    }
}

#[test]
fn control_v_clipboard_and_native_chunks_reconcile_in_either_order() {
    use crate::alphacode_tui::bus::{
        ClipboardPasteCompleted, ClipboardPasteContent, ClipboardPasteKind,
    };
    let chunks = ["first\n", "second 世界\n", "third\n"];
    let text = chunks.concat();
    for native_before in 0..=chunks.len() {
        let mut app = create_test_app();
        let id = app.active_client_session_id().unwrap().to_string();
        app.clipboard_paste_draft = Some((id.clone(), String::new(), 0));
        app.clipboard_paste_native_echo_expected = true;
        for chunk in &chunks[..native_before] {
            app.handle_paste((*chunk).into());
        }
        assert!(
            app.handle_clipboard_paste_completed(ClipboardPasteCompleted {
                session_id: id,
                kind: ClipboardPasteKind::Smart,
                content: ClipboardPasteContent::Text(text.clone()),
            })
        );
        for chunk in &chunks[native_before..] {
            app.handle_paste((*chunk).into());
        }
        assert_eq!(
            app.input, "[pasted content]",
            "delivery order {native_before}"
        );
        assert_eq!(app.pasted_contents, vec![text.clone()]);
        // A later intentional paste of identical text must still be accepted.
        app.handle_paste(text.clone());
        assert_eq!(app.pasted_contents.len(), 2);
    }
}

#[test]
fn pasted_placeholder_literals_are_never_expanded_recursively() {
    let mut app = create_test_app();
    app.pasted_contents = vec![
        "first [pasted content] literal 世界".into(),
        "second [pasted content] literal 👩‍💻".into(),
    ];
    let draft = "before [pasted content] between [pasted content] after";
    assert_eq!(
        app.expand_paste_placeholders(draft),
        "before first [pasted content] literal 世界 between second [pasted content] literal 👩‍💻 after"
    );
}
