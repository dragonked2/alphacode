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
