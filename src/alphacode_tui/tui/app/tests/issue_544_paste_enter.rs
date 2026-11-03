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
