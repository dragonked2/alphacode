// `/help <command>` must never dead-end on a command alphacode advertises.
//
// History: the detailed topic table in `app/input_help.rs` was hand-written and
// covered roughly two thirds of the registry. Asking for help on any command it
// missed answered "Unknown command 'theme'. Use /help to list commands." — a
// help command denying the existence of a command the palette had just offered.
// `registry_help` now falls back to the single registry the palette and `/help`
// share, so coverage is a property of the registry rather than of a hand list.

/// Every public registry entry resolves to *some* help text.
///
/// This is the invariant that keeps the two surfaces honest: if a command is
/// advertised by the palette, `/help <that command>` must answer.
#[test]
fn every_registered_command_has_help() {
    let app = create_test_app();
    let names: Vec<&'static str> = crate::alphacode_tui::tui::app::registered_command_entries()
        .map(|(name, _)| name)
        .collect();
    assert!(
        names.len() > 100,
        "expected the full registry, got {} entries",
        names.len()
    );

    let missing: Vec<&str> = names
        .into_iter()
        .filter(|name| app.command_help(name).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "/help has no answer for registered commands: {missing:?}"
    );
}

/// Spot-check the commands that used to answer "Unknown command".
#[test]
fn previously_undocumented_commands_now_have_help() {
    let app = create_test_app();
    for command in [
        "/theme",
        "/colors",
        "/keys",
        "/diff",
        "/transport",
        "/cancel",
        "/session",
        "/active",
        "/update",
        "/transcript",
        "/thinking-display",
        "/remote",
        "/ssh",
        "/workspace",
        "/terminal-setup",
        "/record",
        "/telemetry",
        "/feedback",
        "/support",
        "/skills",
        "/productivity",
        "/test",
        "/initiatives",
        "/hotkeys",
        "/debug-visual",
        "/screenshot-mode",
    ] {
        let help = app
            .command_help(command)
            .unwrap_or_else(|| panic!("/help {command} returned nothing"));
        assert!(
            help.to_lowercase().contains(&command.trim_start_matches('/').to_lowercase()),
            "/help {command} does not name the command: {help:?}"
        );
    }
}

/// An alias row names its target, and help follows the pointer instead of
/// repeating the one-line registry blurb.
#[test]
fn alias_help_redirects_to_the_target_command() {
    let app = create_test_app();
    let help = app
        .command_help("/models")
        .expect("/models is registered and must have help");
    assert!(
        help.contains("alias for /model"),
        "/models help should say it is an alias for /model: {help:?}"
    );
    assert!(
        help.contains("Open model picker") || help.contains("Switch model"),
        "alias help should carry the target's detail: {help:?}"
    );
}

/// A typo still misses, but the answer names the likely command so the user can
/// retry without rereading the reference.
#[test]
fn typo_suggests_the_nearest_registered_command() {
    let app = create_test_app();
    assert!(
        app.command_help("/them3").is_none(),
        "a miss should stay a miss"
    );
    let suggestions = app.command_help_suggestions("/them3");
    assert!(
        suggestions.iter().any(|name| name == "/theme"),
        "expected /theme among suggestions, got {suggestions:?}"
    );
    assert!(
        suggestions.len() <= 3,
        "suggestions should stay short: {suggestions:?}"
    );
}

/// `handle_help_command` reports the suggestion in the rendered error, not a
/// bare dead end.
#[test]
fn help_command_miss_names_a_suggestion() {
    let mut app = create_test_app();
    assert!(commands::handle_help_command(&mut app, "/help them3"));
    let last = app.display_messages().last().expect("a help reply");
    assert_eq!(last.role, "error");
    assert!(
        last.content.contains("Did you mean"),
        "miss message should suggest a command: {}",
        last.content
    );
    assert!(
        last.content.contains("/theme"),
        "miss message should name /theme: {}",
        last.content
    );
}

/// Very short topics are ambiguous by nature and must not produce noise.
#[test]
fn short_topics_produce_no_suggestions() {
    let app = create_test_app();
    assert!(app.command_help_suggestions("/x").is_empty());
}

#[test]
fn help_search_edits_unicode_without_touching_the_composer_and_esc_has_two_levels() {
    let mut app = create_test_app();
    app.input = "preserve this draft".into();
    app.cursor_pos = 8;
    app.handle_key(KeyCode::F(1), KeyModifiers::NONE).unwrap();
    app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE).unwrap();
    for ch in "model 👩‍💻".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE).unwrap();
    }
    app.handle_key(KeyCode::Backspace, KeyModifiers::NONE).unwrap();
    assert_eq!(app.help_query, "model ");
    app.handle_key(KeyCode::Home, KeyModifiers::NONE).unwrap();
    app.handle_key(KeyCode::Delete, KeyModifiers::NONE).unwrap();
    app.handle_key(KeyCode::Char('M'), KeyModifiers::SHIFT).unwrap();
    assert_eq!(app.help_query, "Model ");
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE).unwrap();
    assert!(!app.help_search_editing);
    assert!(app.help_scroll.is_some());
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(app.help_query.is_empty());
    assert!(app.help_scroll.is_some(), "first Esc clears the filter");
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(app.help_scroll.is_none(), "second Esc closes help");
    assert_eq!(app.input, "preserve this draft");
    assert_eq!(app.cursor_pos, 8);
    app.handle_key(KeyCode::F(1), KeyModifiers::NONE).unwrap();
    assert!(app.help_query.is_empty());
    assert!(!app.help_search_editing);
}

#[test]
fn help_native_paste_is_bounded_sanitized_and_never_becomes_a_draft_attachment() {
    let mut app = create_test_app();
    app.input = "draft".into();
    app.cursor_pos = 5;
    app.open_help();
    app.handle_paste("model\n\x1b[31msearch\x1b[0m".into());
    assert_eq!(app.help_query, "model search");
    assert_eq!(app.help_query_cursor, "model search".len());
    app.handle_paste("x".repeat(1000));
    assert_eq!(app.help_query.chars().count(), 256);
    assert_eq!(app.input, "draft");
    assert_eq!(app.cursor_pos, 5);
    assert!(app.pasted_contents.is_empty());
    app.close_help();
    assert!(app.help_query.is_empty());
}

#[test]
fn help_paging_uses_the_rendered_viewport_after_a_resize() {
    let _lock = crate::alphacode_tui::tui::ui::render_state_test_lock();
    let mut app = create_test_app();
    app.open_help();
    for height in [12, 24] {
        app.help_scroll = Some(0);
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height)).unwrap();
        terminal.draw(|frame| crate::alphacode_tui::tui::ui::draw(frame, &app)).unwrap();
        let page = (height as usize).saturating_sub(4);
        app.handle_key(KeyCode::PageDown, KeyModifiers::NONE).unwrap();
        assert_eq!(app.help_scroll, Some(page));
        app.handle_key(KeyCode::PageUp, KeyModifiers::NONE).unwrap();
        assert_eq!(app.help_scroll, Some(0));
    }
}

#[test]
fn remote_help_search_owns_its_keys_and_keeps_the_draft() {
    let mut app = create_test_app();
    app.input = "remote draft".into();
    app.cursor_pos = app.input.len();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
    rt.block_on(async {
        app.handle_remote_key(KeyCode::F(1), KeyModifiers::NONE, &mut remote).await.unwrap();
        app.handle_remote_key(KeyCode::Char('f'), KeyModifiers::CONTROL, &mut remote).await.unwrap();
        for ch in "query".chars() {
            app.handle_remote_key(KeyCode::Char(ch), KeyModifiers::NONE, &mut remote).await.unwrap();
        }
        assert_eq!(app.help_query, "query", "q must edit the query, not close help");
        app.handle_remote_key(KeyCode::F(1), KeyModifiers::NONE, &mut remote).await.unwrap();
    });
    assert!(app.help_scroll.is_none());
    assert_eq!(app.input, "remote draft");
}

#[test]
fn remote_f1_does_not_cover_an_existing_search_with_an_unfocused_help_overlay() {
    let mut app = create_test_app();
    app.open_transcript_search();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
    rt.block_on(app.handle_remote_key(KeyCode::F(1), KeyModifiers::NONE, &mut remote)).unwrap();
    assert!(app.help_scroll.is_none());
    assert!(app.transcript_search_is_open());
}
