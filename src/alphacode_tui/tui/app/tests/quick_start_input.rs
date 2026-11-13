#[test]
fn ordinary_empty_chat_digits_are_inserted_not_auto_submitted() {
    let mut app = create_test_app();
    app.display_messages.clear();
    app.is_processing = false;
    assert!(input::handle_text_input(&mut app, "1"));
    assert_eq!(app.input, "1");
    assert_eq!(app.cursor_pos, 1);
    assert!(app.display_messages.is_empty());
}
