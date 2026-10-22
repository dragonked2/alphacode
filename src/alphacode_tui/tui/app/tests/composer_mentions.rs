// Tests for `@`-mention path completion in the composer.
//
// A path reference is the other half of a modern coding-agent prompt: naming
// a file should be as cheap as invoking a command, and the reference has to
// survive the completion.

/// An `@` mention offers file paths instead of slash commands.
#[test]
fn an_at_mention_offers_file_paths() {
    let mut app = create_test_app();
    app.input = "please review @Cargo".to_string();
    app.cursor_pos = app.input.len();

    let suggestions = app.command_suggestions();
    assert!(
        suggestions
            .iter()
            .any(|(label, _)| label.ends_with("Cargo.toml")),
        "expected Cargo.toml among the path suggestions, got {suggestions:?}"
    );
    assert!(
        suggestions.iter().all(|(label, _)| label.starts_with('@')),
        "path suggestions are labels, not commands: {suggestions:?}"
    );
}

/// Accepting a path replaces the mention token and nothing else.
#[test]
fn accepting_a_path_replaces_only_the_mention_token() {
    let mut app = create_test_app();
    app.input = "please review @Cargo now".to_string();
    // Put the caret right after the mention token.
    app.cursor_pos = "please review @Cargo".len();

    let suggestions = app.command_suggestions();
    let Some(index) = suggestions
        .iter()
        .position(|(label, _)| label.ends_with("Cargo.toml"))
    else {
        panic!("expected Cargo.toml among the path suggestions, got {suggestions:?}");
    };
    app.command_suggestion_selected = index;

    app.accept_selected_command_suggestion();

    assert!(
        app.input.starts_with("please review @Cargo.toml"),
        "the mention token should be replaced in place, got {:?}",
        app.input
    );
    assert!(
        app.input.ends_with(" now"),
        "the rest of the draft must survive, got {:?}",
        app.input
    );
}

/// Moving the caret out of the mention before accepting must not destroy the
/// draft. The palette keeps showing the paths it was built from, so the
/// acceptance has to target the token that list came from.
#[test]
fn a_caret_move_out_of_the_mention_does_not_destroy_the_draft() {
    let mut app = create_test_app();
    app.input = "please review @Cargo now".to_string();
    app.cursor_pos = "please review @Cargo".len();

    let suggestions = app.command_suggestions();
    let Some(index) = suggestions
        .iter()
        .position(|(label, _)| label.ends_with("Cargo.toml"))
    else {
        panic!("expected Cargo.toml among the path suggestions, got {suggestions:?}");
    };
    app.command_suggestion_selected = index;

    // Move the caret to the start of the draft before accepting.
    app.cursor_pos = 0;
    app.accept_selected_command_suggestion();

    assert!(
        app.input.starts_with("please review @Cargo.toml"),
        "only the mention token may be replaced, got {:?}",
        app.input
    );
}

/// A path is not a command: the palette must not offer slash commands for it.
#[test]
fn a_path_token_does_not_offer_command_suggestions() {
    let mut app = create_test_app();
    app.input = "/usr/bin/env".to_string();
    app.cursor_pos = app.input.len();

    assert!(
        app.command_suggestions().is_empty(),
        "a path must not produce command suggestions"
    );
}
