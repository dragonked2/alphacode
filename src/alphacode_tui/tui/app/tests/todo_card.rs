// Tests for the inline chat todo card (`/todos` command + todo-card hotkey).

#[test]
fn toggle_todo_card_pushes_then_dismisses_trailing_card() {
    let mut app = create_test_app();
    assert!(!app.display_messages.iter().any(|m| m.role == "todos"));

    app.toggle_todo_card();
    assert_eq!(
        app.display_messages
            .iter()
            .filter(|m| m.role == "todos")
            .count(),
        1
    );
    assert_eq!(
        app.display_messages.last().map(|m| m.role.as_str()),
        Some("todos")
    );

    // Toggling again while the card is the trailing message dismisses it.
    app.toggle_todo_card();
    assert!(!app.display_messages.iter().any(|m| m.role == "todos"));
}

#[test]
fn toggle_todo_card_moves_stale_card_to_bottom_instead_of_stacking() {
    let mut app = create_test_app();
    app.toggle_todo_card();
    app.push_display_message(DisplayMessage::system("later activity".to_string()));

    // Card exists but is no longer trailing: toggling re-shows at the bottom.
    app.toggle_todo_card();
    let card_count = app
        .display_messages
        .iter()
        .filter(|m| m.role == "todos")
        .count();
    assert_eq!(card_count, 1, "the transcript keeps at most one todo card");
    assert_eq!(
        app.display_messages.last().map(|m| m.role.as_str()),
        Some("todos")
    );
}

#[test]
fn todos_command_defaults_to_card_and_panel_subcommand_keeps_side_panel() {
    let mut app = create_test_app();

    assert!(commands::handle_session_command(&mut app, "/todos"));
    assert!(app.display_messages.iter().any(|m| m.role == "todos"));
    assert!(!app.todos_view_enabled());

    assert!(commands::handle_session_command(&mut app, "/todos panel"));
    assert!(app.todos_view_enabled());

    assert!(commands::handle_session_command(&mut app, "/todos off"));
    assert!(!app.todos_view_enabled());
}

#[test]
fn todo_alias_shows_card() {
    let mut app = create_test_app();
    assert!(commands::handle_session_command(&mut app, "/todo"));
    assert!(app.display_messages.iter().any(|m| m.role == "todos"));
}

#[test]
fn refresh_todo_card_updates_content_when_todos_change() {
    let _env_lock = crate::storage::lock_test_env();
    let mut app = create_test_app();
    let session_id = app.session.id.clone();

    let todo = |content: &str, status: &str| crate::todo::TodoItem {
        id: "t1".to_string(),
        content: content.to_string(),
        status: status.to_string(),
        priority: "high".to_string(),
        group: None,
        confidence: Some(70),
        completion_confidence: None,
        confidence_history: Vec::new(),
        blocked_by: Vec::new(),
        assigned_to: None,
    };

    crate::todo::save_todos(&session_id, &[todo("write the card", "pending")]).unwrap();
    app.toggle_todo_card();
    let card = app
        .display_messages
        .iter()
        .find(|m| m.role == "todos")
        .expect("todo card pushed");
    assert!(card.content.contains("write the card"));
    assert!(card.content.contains("\"goals\""));

    // Unchanged todos: refresh is a no-op.
    assert!(!app.refresh_todo_card_if_needed());

    crate::todo::save_todos(&session_id, &[todo("write the card", "completed")]).unwrap();
    assert!(app.refresh_todo_card_if_needed());
    let card = app
        .display_messages
        .iter()
        .find(|m| m.role == "todos")
        .expect("todo card still present");
    assert!(card.content.contains("completed"));

    // Cleanup the persisted todo file for this throwaway session.
    let _ = crate::todo::save_todos(&session_id, &[]);
}

#[test]
fn refresh_todo_card_updates_content_when_goal_scores_change() {
    let _env_lock = crate::storage::lock_test_env();
    let mut app = create_test_app();
    let session_id = app.session.id.clone();
    let todos = [crate::todo::TodoItem {
        id: "t1".to_string(),
        content: "render scores".to_string(),
        status: "in_progress".to_string(),
        priority: "high".to_string(),
        group: None,
        confidence: Some(80),
        completion_confidence: None,
        confidence_history: Vec::new(),
        blocked_by: Vec::new(),
        assigned_to: None,
    }];
    let goal = |score| crate::todo::TodoGoal {
        group: None,
        closed_feedback_loop: Some(score),
        feedback_loop: Some("inspect the frame".to_string()),
        end_to_end_ownership: Some(90),
        ..Default::default()
    };

    let plan = crate::todo::TodoPlan {
        user_intention: Some("keep the plan state visible".to_string()),
        understands_user_intent: Some(95),
        ..Default::default()
    };

    crate::todo::save_todos(&session_id, &todos).unwrap();
    crate::todo::save_goals(&session_id, &[goal(70)]).unwrap();
    crate::todo::save_plan(&session_id, &plan).unwrap();
    app.toggle_todo_card();
    let card = app
        .display_messages
        .iter()
        .find(|message| message.role == "todos")
        .expect("todo card pushed");
    assert!(card.content.contains("\"closed_feedback_loop\":70"));
    assert!(card.content.contains("\"understands_user_intent\":95"));

    crate::todo::save_goals(&session_id, &[goal(95)]).unwrap();
    assert!(app.refresh_todo_card_if_needed());
    let card = app
        .display_messages
        .iter()
        .find(|message| message.role == "todos")
        .expect("todo card still present");
    assert!(card.content.contains("\"closed_feedback_loop\":95"));

    let _ = crate::todo::save_todos(&session_id, &[]);
    let _ = crate::todo::save_goals(&session_id, &[]);
    let _ = crate::todo::save_plan(&session_id, &crate::todo::TodoPlan::default());
}

/// Simple todo used by the pinned-band tests.
fn pinned_band_todo(id: &str, content: &str, status: &str) -> crate::todo::TodoItem {
    crate::todo::TodoItem {
        id: id.to_string(),
        content: content.to_string(),
        status: status.to_string(),
        priority: "high".to_string(),
        group: None,
        confidence: Some(80),
        completion_confidence: None,
        confidence_history: Vec::new(),
        blocked_by: Vec::new(),
        assigned_to: None,
    }
}

/// RAII guard for the ALPHACODE_PIN_TODOS env override used by the band tests.
struct PinTodosEnvGuard;

impl PinTodosEnvGuard {
    fn enable() -> Self {
        crate::alphacode_core::env::set_var("ALPHACODE_PIN_TODOS", "1");
        // alphacode-base's config cache throttles env re-checks (the zero
        // interval under cfg!(test) applies only when alphacode-base itself is
        // the crate under test), so force a reload or a sibling test's
        // ALPHACODE_PIN_TODOS state leaks into this one for up to 500ms.
        crate::config::invalidate_config_cache();
        Self
    }
}

impl Drop for PinTodosEnvGuard {
    fn drop(&mut self) {
        crate::alphacode_core::env::remove_var("ALPHACODE_PIN_TODOS");
        // See enable(): flush the removal too, so later tests that expect
        // pin_todos off do not observe this test's stale cached config.
        crate::config::invalidate_config_cache();
    }
}

#[test]
fn pinned_todos_payload_stays_empty_when_config_off() {
    let _env_lock = crate::storage::lock_test_env();
    // `display.pin_todos` defaults to off; establish it explicitly anyway so
    // the contract stays observable even if the default ever flips.
    let mut config = crate::config::Config::load();
    let previous = config.display.pin_todos;
    config.display.pin_todos = false;
    config.save().expect("disable pinned todos");
    crate::config::invalidate_config_cache();

    let mut app = create_test_app();
    let session_id = app.session.id.clone();
    crate::todo::save_todos(&session_id, &[pinned_band_todo("t1", "pin me", "pending")]).unwrap();

    // Off: no payload, no redraw churn.
    assert!(!app.refresh_pinned_todos_if_needed());
    assert!(app.pinned_todos_payload_ref().is_none());

    let _ = crate::todo::save_todos(&session_id, &[]);

    let mut config = crate::config::Config::load();
    config.display.pin_todos = previous;
    config.save().expect("restore pinned todos setting");
    crate::config::invalidate_config_cache();
}

#[test]
fn pinned_todos_payload_refreshes_and_clears_with_config_and_todos() {
    let _env_lock = crate::storage::lock_test_env();
    let _pin = PinTodosEnvGuard::enable();
    let mut app = create_test_app();
    let session_id = app.session.id.clone();

    // No todos yet: enabled but nothing to pin.
    app.refresh_pinned_todos_now();
    assert!(app.pinned_todos_payload_ref().is_none());

    crate::todo::save_todos(&session_id, &[pinned_band_todo("t1", "pin me", "pending")]).unwrap();
    app.refresh_pinned_todos_now();
    let payload = app
        .pinned_todos_payload_ref()
        .expect("payload populated when enabled with todos");
    assert!(payload.contains("pin me"));

    // Unchanged todos within the throttle window: no redraw.
    assert!(!app.refresh_pinned_todos_if_needed());

    // Todos cleared: payload clears too.
    crate::todo::save_todos(&session_id, &[]).unwrap();
    app.refresh_pinned_todos_now();
    assert!(app.pinned_todos_payload_ref().is_none());
}

#[test]
fn pinned_todo_band_renders_at_top_when_scrolled() {
    let _env_lock = crate::storage::lock_test_env();
    let _render_lock = crate::alphacode_tui::tui::ui::render_state_test_lock();
    let _pin = PinTodosEnvGuard::enable();
    let mut app = create_test_app();
    let session_id = app.session.id.clone();
    crate::todo::save_todos(
        &session_id,
        &[pinned_band_todo("t1", "pinned band item", "in_progress")],
    )
    .unwrap();
    app.refresh_pinned_todos_now();
    assert!(app.pinned_todos_payload_ref().is_some());

    app.display_messages = vec![
        DisplayMessage {
            role: "user".to_string(),
            content: "kick off the work".to_string(),
            tool_calls: vec![],
            duration_secs: None,
            title: None,
            tool_data: None,
        },
        DisplayMessage {
            role: "assistant".to_string(),
            content: App::build_scroll_test_content(0, 40, None),
            tool_calls: vec![],
            duration_secs: None,
            title: None,
            tool_data: None,
        },
    ];
    app.bump_display_messages_version();
    app.scroll_offset = 0;
    app.auto_scroll_paused = false;
    app.is_processing = false;
    app.streaming.streaming_text.clear();
    app.status = ProcessingStatus::Idle;
    app.session.short_name = Some("test".to_string());

    let backend = ratatui::backend::TestBackend::new(60, 16);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    let text = render_and_snap(&app, &mut terminal);

    let first_rows = text.lines().take(4).collect::<Vec<_>>().join(" ");
    assert!(
        first_rows.contains("pinned band item"),
        "pinned todo band should render at the top of the scrolled viewport, got:\n{}",
        text
    );

    let _ = crate::todo::save_todos(&session_id, &[]);
}

/// A long turn writes todos but the user never presses anything. Before this
/// fix the card only ever refreshed an existing card, so it could not appear on
/// its own: `refresh_todo_card_if_needed` returned early whenever no card was
/// present. The auto-open path must create it exactly once.
#[test]
fn todo_card_auto_opens_once_when_the_session_has_todos() {
    with_temp_alphacode_home(|| {
        let mut app = create_test_app();
        let session_id = app.session.id.clone();

        // With no todos on disk the probe must stay quiet.
        app.todo_card_probed_at = None;
        assert!(!app.refresh_todo_card_if_needed(), "no todos means no card");
        assert!(!app.display_messages.iter().any(|m| m.role == "todos"));

        crate::todo::save_todos(
            &session_id,
            &[crate::todo::TodoItem {
                id: "t1".to_string(),
                content: "Audit the hint surfaces".to_string(),
                status: "in_progress".to_string(),
                priority: "high".to_string(),
                confidence: Some(70),
                ..Default::default()
            }],
        )
        .expect("seed session todos");

        // The probe is throttled, so clear the stamp to model "a second elapsed".
        app.todo_card_probed_at = None;
        assert!(
            app.refresh_todo_card_if_needed(),
            "the card should open by itself once todos exist"
        );
        assert_eq!(
            app.display_messages
                .iter()
                .filter(|m| m.role == "todos")
                .count(),
            1,
            "auto-open must not stack cards"
        );

        // A second probe finds the card already there and does not duplicate it.
        app.todo_card_probed_at = None;
        app.refresh_todo_card_if_needed();
        assert_eq!(
            app.display_messages
                .iter()
                .filter(|m| m.role == "todos")
                .count(),
            1
        );
    });
}

/// Dismissing the card must survive further todo writes: the user closed it on
/// purpose, and a background update should not re-open it mid-read.
#[test]
fn dismissed_todo_card_does_not_reopen_on_the_next_write() {
    with_temp_alphacode_home(|| {
        let mut app = create_test_app();
        let session_id = app.session.id.clone();

        app.show_todo_card();
        assert_eq!(
            app.display_messages
                .iter()
                .filter(|m| m.role == "todos")
                .count(),
            1
        );

        // Close it, exactly as the hotkey or /todos does.
        app.toggle_todo_card();
        assert!(!app.display_messages.iter().any(|m| m.role == "todos"));
        assert!(app.todo_card_dismissed, "dismissal must be recorded");

        crate::todo::save_todos(
            &session_id,
            &[crate::todo::TodoItem {
                id: "t2".to_string(),
                content: "Keep working".to_string(),
                status: "pending".to_string(),
                priority: "medium".to_string(),
                ..Default::default()
            }],
        )
        .expect("write more todos");

        app.todo_card_probed_at = None;
        assert!(
            !app.refresh_todo_card_if_needed(),
            "a dismissed card must not reopen"
        );

        // Asking for it again clears the dismissal.
        app.show_todo_card();
        assert!(!app.todo_card_dismissed);
        assert_eq!(
            app.display_messages
                .iter()
                .filter(|m| m.role == "todos")
                .count(),
            1
        );
        if let Some(idx) = app.display_messages.iter().rposition(|m| m.role == "todos") {
            app.remove_display_message(idx);
        }
    });
}

/// A newly created session must not draw the previous session's todo list,
/// plan intention, or goal scores.
///
/// `/clear` (local) and the remote clear both replace the active session, but
/// `active_client_session_id()` follows the *live* session immediately while
/// three render surfaces (`todos_view_markdown`, the inline card's rendered
/// payload, and `pinned_todos_payload`) only rebuild when their payload hash
/// changes. The renderer can therefore draw the previous session's cached
/// payload before the next tick notices the session id changed — which is the
/// reported "new session shows old todo and plan data".
#[test]
fn new_session_does_not_show_the_previous_sessions_todos_or_plan() {
    with_temp_alphacode_home(|| {
        let mut app = create_test_app();
        app.todos_view_enabled = true;
        let old_session_id = app.session.id.clone();

        crate::todo::save_todos(
            &old_session_id,
            &[crate::todo::TodoItem {
                id: "old-1".to_string(),
                content: "stale item from the previous session".to_string(),
                status: "in_progress".to_string(),
                priority: "high".to_string(),
                confidence: Some(70),
                ..Default::default()
            }],
        )
        .expect("seed old session todos");
        crate::todo::save_plan(
            &old_session_id,
            &crate::todo::TodoPlan {
                user_intention: Some("stale intention from the previous session".to_string()),
                ..Default::default()
            },
        )
        .expect("seed old session plan");

        // Render every surface for the old session so each cache is populated.
        app.refresh_todos_view_now();
        assert!(
            app.todos_view_markdown
                .contains("stale item from the previous session"),
            "precondition: the Todo panel cached the old session's list"
        );
        app.show_todo_card();
        app.refresh_pinned_todos_now();
        assert!(
            app.pinned_todos_payload
                .as_deref()
                .is_some_and(|p| p.contains("stale item from the previous session")),
            "precondition: the pinned band cached the old session's list"
        );
        app.todo_card_dismissed = true;

        // Same path `/clear` uses to start a fresh session.
        commands::reset_current_session(&mut app);
        assert_ne!(app.session.id, old_session_id, "a new session was created");

        // Nothing about the old session may survive into the new one.
        assert!(
            !app.todos_view_markdown.contains("stale item from the previous session"),
            "the Todo panel still shows the previous session's list:\n{}",
            app.todos_view_markdown
        );
        assert!(
            !app.todos_view_markdown.contains("stale intention from the previous session"),
            "the Todo panel still shows the previous session's plan intention:\n{}",
            app.todos_view_markdown
        );
        assert!(
            app.pinned_todos_payload.is_none(),
            "the pinned band still carries the previous session's payload"
        );
        assert!(
            !app.todo_card_dismissed,
            "the previous session's card dismissal must not suppress the new session's card"
        );

        // The refreshers must agree: a new session with no todos renders empty.
        app.refresh_todos_view_now();
        assert!(
            !app.todos_view_markdown.contains("stale item"),
            "post-refresh the Todo panel must stay empty for a fresh session"
        );
    });
}
