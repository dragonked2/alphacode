// Tests for the composer text-selection and palette-dismissal work.
//
// Both features exist because the composer had a real gap against every
// mainstream text field: it could only be edited with the mouse, and the
// slash palette could not be dismissed without losing the draft.

/// `Shift+Right` must select the character under the caret.
#[test]
fn shift_right_selects_the_character_under_the_caret() {
    let mut app = create_test_app();
    app.input = "abc".to_string();
    app.cursor_pos = 0;

    app.handle_key(KeyCode::Right, KeyModifiers::SHIFT)
        .expect("extend selection");
    assert_eq!(app.input_selection_range(), Some((0, 1)));

    app.handle_key(KeyCode::Right, KeyModifiers::SHIFT)
        .expect("extend selection");
    assert_eq!(app.input_selection_range(), Some((0, 2)));
}

/// `Shift+Left` from the end of the draft selects backwards.
#[test]
fn shift_left_selects_backwards() {
    let mut app = create_test_app();
    app.input = "abc".to_string();
    app.cursor_pos = 3;

    app.handle_key(KeyCode::Left, KeyModifiers::SHIFT)
        .expect("extend selection");
    assert_eq!(app.input_selection_range(), Some((2, 3)));
}

/// `Ctrl+Shift+Left` extends by word, not by character.
#[test]
fn ctrl_shift_left_extends_by_word() {
    let mut app = create_test_app();
    app.input = "alpha beta".to_string();
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        .expect("extend selection by word");
    let range = app.input_selection_range().expect("a selection");
    assert_eq!(range, (6, 10), "expected the previous word to be selected");
}

/// `Shift+Home` / `Shift+End` select to the boundaries of the draft.
///
/// The anchor stays where the selection started (the standard model every
/// editor uses), so `Shift+Home` from the middle selects backwards to the
/// start, and a following `Shift+End` swings the moving end to the end.
#[test]
fn shift_home_and_end_select_to_the_draft_boundaries() {
    let mut app = create_test_app();
    app.input = "hello world".to_string();
    app.cursor_pos = 6;

    app.handle_key(KeyCode::Home, KeyModifiers::SHIFT)
        .expect("extend to start");
    assert_eq!(app.input_selection_range(), Some((0, 6)));

    app.handle_key(KeyCode::End, KeyModifiers::SHIFT)
        .expect("extend to end");
    assert_eq!(app.input_selection_range(), Some((6, 11)));
}

/// `Shift+End` on its own selects from the caret to the end of the draft.
#[test]
fn shift_end_selects_to_the_end_of_the_draft() {
    let mut app = create_test_app();
    app.input = "hello world".to_string();
    app.cursor_pos = 6;

    app.handle_key(KeyCode::End, KeyModifiers::SHIFT)
        .expect("extend to end");
    assert_eq!(app.input_selection_range(), Some((6, 11)));
}

/// Typing over a selection replaces it, like every other text field.
#[test]
fn typing_replaces_the_selection() {
    let mut app = create_test_app();
    app.input = "hello world".to_string();
    app.cursor_pos = 11;

    app.handle_key(KeyCode::Home, KeyModifiers::SHIFT)
        .expect("select all");
    assert_eq!(app.input_selection_range(), Some((0, 11)));

    app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE)
        .expect("type over the selection");
    assert_eq!(app.input, "x");
    assert!(!app.has_input_selection());
}

/// `Backspace` removes the whole selection as one edit.
#[test]
fn backspace_removes_the_selection() {
    let mut app = create_test_app();
    app.input = "hello world".to_string();
    app.cursor_pos = 11;

    app.handle_key(KeyCode::Home, KeyModifiers::SHIFT)
        .expect("select all");
    app.handle_key(KeyCode::Backspace, KeyModifiers::NONE)
        .expect("delete the selection");
    assert_eq!(app.input, "");
    assert!(!app.has_input_selection());
}

/// A caret move without `Shift` collapses the selection.
#[test]
fn plain_arrow_collapses_the_selection() {
    let mut app = create_test_app();
    app.input = "abc".to_string();
    app.cursor_pos = 0;

    app.handle_key(KeyCode::Right, KeyModifiers::SHIFT)
        .expect("extend selection");
    assert!(app.has_input_selection());

    app.handle_key(KeyCode::Right, KeyModifiers::NONE)
        .expect("collapse selection");
    assert!(!app.has_input_selection());
}

/// `Esc` closes the palette but must not eat the draft. This is the bug that
/// made the palette undismissable: an absolute path opened the palette, and
/// the only `Esc` handler cleared the whole input.
#[test]
fn escape_dismisses_the_palette_without_clearing_the_draft() {
    let mut app = create_test_app();
    app.input = "/mo".to_string();
    app.cursor_pos = app.input.len();

    assert!(
        !app.command_suggestions().is_empty(),
        "the palette should be open for a slash command"
    );

    app.handle_key(KeyCode::Esc, KeyModifiers::NONE)
        .expect("dismiss the palette");
    assert_eq!(app.input, "/mo", "Esc must not clear the draft");
    assert!(
        app.command_suggestions().is_empty(),
        "the palette must be hidden after Esc"
    );
}

/// A second `Esc` clears the draft, the way it always did.
#[test]
fn a_second_escape_clears_the_draft() {
    let mut app = create_test_app();
    app.input = "/mo".to_string();
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Esc, KeyModifiers::NONE)
        .expect("dismiss the palette");
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE)
        .expect("clear the draft");
    assert_eq!(app.input, "");
}

/// An absolute path is not a command: the palette must not open for it.
#[test]
fn an_absolute_path_does_not_open_the_command_palette() {
    let mut app = create_test_app();
    app.input = "/usr/bin/env".to_string();
    app.cursor_pos = app.input.len();

    assert!(
        app.command_suggestions().is_empty(),
        "a path must not produce command suggestions"
    );
}

/// `Tab` reopens a palette the user dismissed with `Esc`.
#[test]
fn tab_reopens_a_dismissed_palette() {
    let mut app = create_test_app();
    app.input = "/mo".to_string();
    app.cursor_pos = app.input.len();

    app.handle_key(KeyCode::Esc, KeyModifiers::NONE)
        .expect("dismiss the palette");
    assert!(app.command_suggestions().is_empty());

    app.handle_key(KeyCode::Tab, KeyModifiers::NONE)
        .expect("tab complete");
    assert!(
        !app.command_suggestions().is_empty(),
        "Tab is an explicit request for completions"
    );
}

/// The selected text is what a copy would put on the clipboard.
#[test]
fn the_selection_text_is_the_selected_range() {
    let mut app = create_test_app();
    app.input = "hello world".to_string();
    app.cursor_pos = 0;

    for _ in 0..5 {
        app.handle_key(KeyCode::Right, KeyModifiers::SHIFT)
            .expect("extend selection");
    }
    assert_eq!(app.selected_input_text().as_deref(), Some("hello"));
}
