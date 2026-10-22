// End-to-end tests for the find-in-page overlay (Ctrl+F).
//
// The feature spans three layers — the app owns the query and the selection,
// the renderer owns the hit index, and the overlay renders the counter — so
// these tests drive it the way a user does: press the chord, type a query,
// render a frame, and check what the screen shows and where the viewport
// ended up.

/// Render one frame of the TUI and return the visible text.
fn render_frame(app: &App, terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>) {
    terminal
        .draw(|f| crate::alphacode_tui::tui::ui::draw(f, app))
        .expect("draw");
}

/// The whole rendered screen as text, one string per row.
fn screen_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Build an app whose transcript contains the given assistant messages.
///
/// Deliberately does not go through `create_test_app()`: that helper clears
/// the persisted UI state, which invalidates the *process-global* auth-status
/// cache. Every test that does that adds another writer to a global cache
/// that other tests read, and the resulting race shows up as an unrelated
/// test losing an OpenAI suggestion. These tests only need an app and a
/// transcript, so they skip the global churn.
fn app_with_transcript(messages: &[&str]) -> App {
    ensure_test_alphacode_home_if_unset();
    crate::alphacode_tui::tui::ui::clear_test_render_state_for_tests();

    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
    let mut app = App::new_for_test_harness(provider, registry);
    app.queue_mode = false;
    app.diff_mode = crate::config::DiffDisplayMode::Inline;

    for message in messages {
        app.display_messages
            .push(DisplayMessage::assistant((*message).to_string()));
    }
    app.bump_display_messages_version();
    app
}

/// Press Ctrl+F and confirm the overlay opened.
#[test]
fn ctrl_f_opens_the_find_overlay() {
    let mut app = app_with_transcript(&["hello world"]);
    assert!(app.transcript_search_view().is_none());

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");

    let view = app.transcript_search_view().expect("overlay should be open");
    assert_eq!(view.query, "");
    assert_eq!(view.match_count, 0);
}

/// Typing a query lands in the overlay, not in the composer.
#[test]
fn typing_goes_to_the_query_not_the_composer() {
    let mut app = app_with_transcript(&["hello world"]);
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");

    for ch in "find".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }

    let view = app.transcript_search_view().expect("overlay should be open");
    assert_eq!(view.query, "find");
    assert!(
        app.input.is_empty(),
        "the composer must not receive the search query, got {:?}",
        app.input
    );
}

/// Esc closes the overlay and leaves the composer untouched.
#[test]
fn escape_closes_the_overlay() {
    let mut app = app_with_transcript(&["hello world"]);
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE)
        .expect("handle key");

    assert!(app.transcript_search_view().is_none());
    assert!(app.input.is_empty());
}

/// Backspace removes a character from the query.
#[test]
fn backspace_edits_the_query() {
    let mut app = app_with_transcript(&["hello world"]);
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "abc".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    app.handle_key(KeyCode::Backspace, KeyModifiers::NONE)
        .expect("handle key");

    assert_eq!(app.transcript_search_view().expect("open").query, "ab");
}

/// Ctrl+W removes the word before the cursor.
#[test]
fn ctrl_w_removes_the_previous_word() {
    let mut app = app_with_transcript(&["hello world"]);
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "alpha beta".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    app.handle_key(KeyCode::Char('w'), KeyModifiers::CONTROL)
        .expect("handle key");

    assert_eq!(
        app.transcript_search_view().expect("open").query,
        "alpha ",
        "Ctrl+W must remove the word before the cursor, keeping the space"
    );
}

/// Ctrl+U clears the whole query.
#[test]
fn ctrl_u_clears_the_query() {
    let mut app = app_with_transcript(&["hello world"]);
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "alpha".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    app.handle_key(KeyCode::Char('u'), KeyModifiers::CONTROL)
        .expect("handle key");

    assert_eq!(app.transcript_search_view().expect("open").query, "");
}

/// The renderer finds the hits and the overlay reports them.
#[test]
fn the_renderer_reports_the_hits() {
    let mut app = app_with_transcript(&[
        "the quick brown fox",
        "jumps over the lazy dog",
        "the end",
    ]);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "the".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }

    render_frame(&app, &mut terminal);

    let view = app.transcript_search_view().expect("overlay should be open");
    assert_eq!(
        view.match_count, 3,
        "expected one hit per line, got {view:?}"
    );
    assert_eq!(view.selected, 0);
    assert!(view.current_line.is_some());
}

/// The overlay shows the query and the hit counter.
#[test]
fn the_overlay_renders_the_query_and_the_counter() {    let mut app = app_with_transcript(&["alpha needle", "beta needle"]);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "needle".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    render_frame(&app, &mut terminal);

    let text = screen_text(&terminal);
    assert!(
        text.contains("needle"),
        "the overlay must show the query, got:\n{text}"
    );
    assert!(
        text.contains("1/2"),
        "the overlay must show the hit counter, got:\n{text}"
    );
}

/// Enter moves to the next hit and scrolls the viewport to it.
#[test]
fn enter_advances_to_the_next_hit_and_scrolls_to_it() {    let mut app = app_with_transcript(&[
        "first line\nneedle here\nthird line",
        "another message without the word",
    ]);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "needle".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    render_frame(&app, &mut terminal);

    let first_line = app.transcript_search_view().expect("open").current_line;
    assert!(first_line.is_some(), "the first hit must be located");

    app.handle_key(KeyCode::Enter, KeyModifiers::NONE)
        .expect("handle key");
    render_frame(&app, &mut terminal);

    let view = app.transcript_search_view().expect("open");
    assert_eq!(view.selected, 0, "there is only one hit to step to");
    assert_eq!(view.current_line, first_line);
    assert!(
        app.auto_scroll_paused,
        "jumping to a hit must stop the viewport from snapping back to the tail"
    );
}

/// Alt+C toggles case sensitivity, and the hit count follows.
#[test]
fn alt_c_toggles_case_sensitivity() {
    let mut app = app_with_transcript(&["Needle and needle"]);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "needle".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    render_frame(&app, &mut terminal);
    assert_eq!(
        app.transcript_search_view().expect("open").match_count,
        2,
        "the default search is case-insensitive"
    );

    app.handle_key(KeyCode::Char('c'), KeyModifiers::ALT)
        .expect("handle key");
    render_frame(&app, &mut terminal);

    let view = app.transcript_search_view().expect("open");
    assert!(view.case_sensitive);
    assert_eq!(view.match_count, 1, "case-sensitive search finds one hit");
}

/// A query that matches nothing reports "no matches" instead of pretending
/// the search is still running.
#[test]
fn a_query_without_hits_reports_no_matches() {
    let mut app = app_with_transcript(&["hello world"]);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "zzz".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    render_frame(&app, &mut terminal);

    assert_eq!(
        app.transcript_search_view().expect("open").match_count,
        0,
        "there is no hit for a query that does not occur"
    );
    assert!(screen_text(&terminal).contains("no matches"));
}

/// The matched text is painted with the find-in-page background, so the user
/// can see *what* matched and not only *where*.
#[test]
fn the_match_is_highlighted_in_the_transcript() {
    let mut app = app_with_transcript(&["alpha needle beta"]);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "needle".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    render_frame(&app, &mut terminal);

    // Find the row that shows the transcript line, then compare the
    // background of the matched word with the background of the text right
    // before it. A highlight changes the background of exactly the match.
    let buffer = terminal.backend().buffer();
    let mut found = false;
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let Some(start) = row.find("needle") else {
            continue;
        };
        let start = start.min(buffer.area.width as usize - 1);
        let match_bg = buffer[(start as u16, y)].bg;
        let neighbour_bg = if start > 0 {
            buffer[((start - 1) as u16, y)].bg
        } else {
            match_bg
        };
        assert_ne!(
            match_bg, neighbour_bg,
            "the matched word must be painted with a distinct background \
             (row {y}: {row:?})"
        );
        found = true;
        break;
    }
    assert!(found, "the transcript line must be visible on screen");
}

/// `/find <text>` opens the overlay with the query already filled in, which
/// is the discoverable path to the feature.
#[test]
fn the_find_command_opens_the_overlay_with_a_query() {
    let mut app = app_with_transcript(&["alpha needle beta"]);

    assert!(
        crate::alphacode_tui::tui::app::commands::handle_find_command(&mut app, "/find needle"),
        "/find must be handled"
    );

    let view = app.transcript_search_view().expect("overlay should be open");
    assert_eq!(view.query, "needle");
}

/// The command is registered, so it shows up in the palette and in `/help`.
#[test]
fn the_find_command_is_registered() {
    let registered: Vec<&str> =
        crate::alphacode_tui::tui::app::registered_command_entries()
            .map(|(name, _)| name)
            .collect();
    assert!(
        registered.iter().any(|name| name.starts_with("/find")),
        "/find must be a registered command, got {registered:?}"
    );
}

/// The remote client has its own key dispatcher, so the search has to be
/// wired into it separately.
#[test]
fn the_remote_client_opens_the_search_too() {
    with_temp_alphacode_home(|| {
        let mut app = app_with_transcript(&["alpha needle beta"]);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();

        app.is_remote = true;

        rt.block_on(app.handle_remote_key(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
            &mut remote,
        ))
        .expect("Ctrl+F should be handled in the remote path");

        assert!(
            app.transcript_search_view().is_some(),
            "Ctrl+F must open the find overlay in remote sessions too"
        );

        // Typing goes to the query, not to the composer.
        rt.block_on(app.handle_remote_key(
            KeyCode::Char('n'),
            KeyModifiers::NONE,
            &mut remote,
        ))
        .expect("typing should be handled in the remote path");
        let view = app.transcript_search_view().expect("overlay should be open");
        assert_eq!(view.query, "n");
        assert!(app.input.is_empty());
    });
}

/// A step requested before the renderer had caught up with the query is
/// deferred, not dropped: typing a query and pressing Enter inside one event
/// batch must still advance the search.
#[test]
fn a_step_requested_before_the_first_render_is_deferred_not_dropped() {
    let mut app = app_with_transcript(&["alpha needle", "beta needle"]);

    // No frame is rendered between the keystrokes, so the hit index does not
    // exist yet when Enter arrives.
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    for ch in "needle".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    app.handle_key(KeyCode::Enter, KeyModifiers::NONE)
        .expect("handle key");

    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    render_frame(&app, &mut terminal);
    app.apply_pending_transcript_search_step();

    let view = app.transcript_search_view().expect("open");
    assert_eq!(view.match_count, 2, "both lines contain the query");
    assert_eq!(
        view.selected, 1,
        "the deferred step must advance to the second hit"
    );
}

/// The query cursor moves by character, not by byte, so a multi-byte
/// character is never split.
#[test]
fn the_query_cursor_moves_by_character() {
    let mut app = app_with_transcript(&["hello world"]);
    app.handle_key(KeyCode::Char('f'), KeyModifiers::CONTROL)
        .expect("handle key");
    // "é" is two bytes.
    for ch in "aé".chars() {
        app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE)
            .expect("handle key");
    }
    // Back over the "é": the cursor lands on its first byte, not inside it.
    app.handle_key(KeyCode::Left, KeyModifiers::NONE)
        .expect("handle key");

    // Typing now must land before the "é", not inside it.
    app.handle_key(KeyCode::Char('z'), KeyModifiers::NONE)
        .expect("handle key");
    assert_eq!(
        app.transcript_search_view().expect("open").query,
        "azé",
        "the cursor must move by character, not by byte"
    );
}

/// The composer's word-forward binding moves to Alt+F now that Ctrl+F opens
/// the search.
#[test]
fn alt_f_still_moves_the_caret_a_word_forward() {
    let mut app = app_with_transcript(&["hello world"]);
    app.input = "alpha beta".to_string();
    app.cursor_pos = 0;

    app.handle_key(KeyCode::Char('f'), KeyModifiers::ALT)
        .expect("handle key");

    assert_eq!(app.cursor_pos, 6, "Alt+F moves to the next word boundary");
    assert!(
        app.transcript_search_view().is_none(),
        "Alt+F must not open the search overlay"
    );
}
