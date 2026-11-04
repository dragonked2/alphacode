#[test]
fn add_opens_a_browser_and_cancel_preserves_the_session() {
    let mut app = create_test_app();
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("custom name.txt"), "not a skill").unwrap();
    app.session.working_dir = Some(temp.path().to_string_lossy().to_string());
    app.input = "/add".into();
    app.cursor_pos = app.input.len();
    app.submit_input();
    assert!(app.account_picker_overlay.is_some());
    assert!(!app.pending_turn);
    assert!(!app.is_processing);
    assert!(App::is_registered_slash_command("/add"));
    app.handle_key(KeyCode::Esc, KeyModifiers::empty()).unwrap();
    assert!(app.account_picker_overlay.is_none());
    assert!(app.active_skill.is_none());
    wait_for_add_job(&mut app);
    assert!(app.account_picker_overlay.is_none(), "a cancelled scan must not reopen the browser");
}

#[test]
fn add_invalid_arbitrary_file_reports_error_without_starting_a_turn() {
    let mut app = create_test_app();
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("custom name.txt"), "not a skill").unwrap();
    app.session.working_dir = Some(temp.path().to_string_lossy().to_string());
    app.input = "/add \"custom name.txt\"".into();
    app.cursor_pos = app.input.len();
    app.submit_input();
    wait_for_add_job(&mut app);
    assert!(!app.pending_turn);
    assert!(app.display_messages().iter().any(|message| message.content.contains("Invalid skill file")));
}

fn wait_for_add_job(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.pending_add.is_some() {
        crate::alphacode_tui::tui::app::commands_add::poll_add(app);
        assert!(std::time::Instant::now() < deadline, "file selection worker did not complete");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn active_local_skill_instructions_are_available_to_connected_turns() {
    let mut app = create_test_app();
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join(".alphacode/skills/connected-custom");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), "---\nname: connected-custom\ndescription: Custom connected workflow\n---\nCompare owned canaries before reporting.").unwrap();
    app.session.working_dir = Some(temp.path().to_string_lossy().to_string());
    assert!(app.remote_active_skill_prompt().is_none());
    app.input = "/connected-custom".into();
    app.cursor_pos = app.input.len();
    app.submit_input();
    let prompt = app.remote_active_skill_prompt().unwrap();
    assert!(prompt.contains("Compare owned canaries before reporting."));
    assert!(prompt.contains("connected-custom"));
}
