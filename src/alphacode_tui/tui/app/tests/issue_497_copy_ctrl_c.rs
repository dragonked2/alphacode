// Issue #497: Ctrl+C while a copy-mode selection is active must copy the
// selection, not fall through to the global interrupt/quit handler (which
// closed alphacode and lost the very error the user was trying to copy).

#[test]
fn test_ctrl_c_with_active_copy_selection_copies_instead_of_quitting() {
    let _render_lock = scroll_render_test_lock();
    let mut app = create_test_app();
    app.push_display_message(DisplayMessage::error(
        "some provider error worth copying".to_string(),
    ));

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    render_and_snap(&app, &mut terminal);

    app.enter_copy_selection_mode();
    assert!(app.select_all_in_copy_mode());
    assert!(
        app.current_copy_selection_text()
            .is_some_and(|t| !t.is_empty()),
        "test needs a non-empty selection"
    );

    app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL)
        .unwrap();

    assert!(
        app.quit_pending.is_none(),
        "Ctrl+C over a selection must not arm quit"
    );
    // The copy path ran (not the quit path): it always sets a copy status.
    // Clipboard success depends on the environment (headless CI has none),
    // so accept either outcome of the copy itself.
    let notice = app.status_notice();
    assert!(
        notice.as_deref() == Some("Copied selection")
            || notice.as_deref() == Some("Failed to copy selection"),
        "expected copy-path status notice, got {notice:?}"
    );
}

#[test]
fn test_ctrl_c_in_copy_mode_without_selection_still_falls_through() {
    let _render_lock = scroll_render_test_lock();
    let mut app = create_test_app();

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    render_and_snap(&app, &mut terminal);

    app.enter_copy_selection_mode();
    assert!(app.current_copy_selection_text().is_none());

    app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL)
        .unwrap();

    // No selection: preserve the existing interrupt/quit semantics.
    assert!(
        app.quit_pending.is_some() || app.cancel_requested,
        "Ctrl+C without a selection keeps interrupt/quit behavior"
    );
}

#[test]
fn copy_export_error_is_consumed_without_interrupting_the_turn() {
    let mut app = create_test_app();
    let session_id = app.session.id.clone();
    app.pending_session_copy = Some(crate::alphacode_tui::tui::app::commands_copy::PendingSessionCopy {
        request_id: 77, session_id, started: std::time::Instant::now(), live_text:String::new(),
    });
    assert!(!crate::alphacode_tui::tui::app::commands_copy::handle_session_copy_error(&mut app, 78, "unrelated error"));
    assert!(app.pending_session_copy.is_some());
    assert!(crate::alphacode_tui::tui::app::commands_copy::handle_session_copy_error(&mut app, 77, "Session changed; run /copy again"));
    assert!(app.pending_session_copy.is_none());
}

#[test]
fn old_daemon_copy_variant_error_starts_saved_snapshot_recovery() {
    let mut app = create_test_app();
    let session_id = app.session.id.clone();
    app.pending_session_copy = Some(crate::alphacode_tui::tui::app::commands_copy::PendingSessionCopy {
        request_id: 77, session_id, started: std::time::Instant::now(), live_text:"live tail".into(),
    });
    assert!(crate::alphacode_tui::tui::app::commands_copy::handle_session_copy_error(&mut app, 0, "Invalid request: unknown variant `get_session_transcript`"));
    assert!(app.pending_session_copy.is_none());
    assert!(app.clipboard_copy_job.is_some());
}
