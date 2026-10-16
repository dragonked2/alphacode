// Tests for recency/frequency ranking of slash-command suggestions.
//
// The palette used to rank equal-quality matches purely by length and
// alphabet, so muscle memory (`/re` leading with the `/rewind` the user just
// ran) was lost on every keystroke. These tests pin the learned tiebreak:
// recorded commands lead never-used peers within the same match bucket, the
// newest use wins, and usage can never outrank a better match.

fn candidate(name: &str) -> (String, &'static str) {
    (name.to_string(), "test help")
}

#[test]
fn the_first_token_is_what_gets_remembered() {
    let mut app = create_test_app();
    app.record_command_usage("plain prompt");
    app.record_command_usage("/rewind 3");

    // `/rewind 3` must credit `/rewind`; the plain prompt must be ignored.
    let ranked = app.rank_suggestions("/r", vec![candidate("/rewind"), candidate("/rerun")]);
    assert_eq!(ranked[0].0, "/rewind");
}

#[test]
fn usage_breaks_ties_within_the_literal_prefix_bucket() {
    let mut app = create_test_app();
    let candidates = vec![
        candidate("/redo"),
        candidate("/reload"),
        candidate("/rebuild"),
    ];

    // Without usage, the shortest name wins.
    let ranked = app.rank_suggestions("/re", candidates.clone());
    assert_eq!(ranked[0].0, "/redo");

    app.record_command_usage("/rebuild");
    let ranked = app.rank_suggestions("/re", candidates.clone());
    assert_eq!(ranked[0].0, "/rebuild", "recently used command must lead");

    // A newer use outranks an older one...
    app.record_command_usage("/reload");
    let ranked = app.rank_suggestions("/re", candidates.clone());
    assert_eq!(ranked[0].0, "/reload");

    // ...and running an older command again makes it the newest again.
    app.record_command_usage("/rebuild");
    let ranked = app.rank_suggestions("/re", candidates);
    assert_eq!(ranked[0].0, "/rebuild");
}

#[test]
fn usage_never_lifts_a_fuzzy_match_above_a_literal_prefix() {
    let mut app = create_test_app();
    // "/rw" typo-matches "/rewind" but literally prefixes "/rwlock".
    app.record_command_usage("/rewind");

    let ranked = app.rank_suggestions(
        "/rw",
        vec![candidate("/rwlock"), candidate("/rewind")],
    );
    assert_eq!(
        ranked[0].0, "/rwlock",
        "typing an exact prefix must always win over learned ranking"
    );
}

#[test]
fn tracker_evicts_the_oldest_token_past_capacity() {
    let mut app = create_test_app();
    // The oldest token is the longest name so, once evicted, the shorter
    // never-used peer takes the lead again.
    app.record_command_usage("/cmdzero");
    for i in 0..128 {
        app.record_command_usage(&format!("/cmd{i}"));
    }

    let ranked = app.rank_suggestions(
        "/cmdz",
        vec![candidate("/cmdz"), candidate("/cmdzero")],
    );
    assert_eq!(
        ranked[0].0, "/cmdz",
        "the oldest token must have been evicted to keep the map bounded"
    );
}

#[test]
fn submitted_slash_command_leads_the_live_palette() {
    let mut app = create_test_app();

    app.input = "/re".to_string();
    app.cursor_pos = app.input.len();
    app.advance_command_suggestions_epoch();
    let before = app.command_suggestions();
    assert!(
        before.iter().any(|(cmd, _)| cmd == "/rewind"),
        "precondition: /rewind should be a suggestion for /re, got {before:?}"
    );
    assert_ne!(
        before[0].0, "/rewind",
        "precondition: /rewind should not already lead the /re bucket"
    );

    app.record_command_usage("/rewind");
    app.advance_command_suggestions_epoch();
    let after = app.command_suggestions();
    assert_eq!(
        after[0].0, "/rewind",
        "recording a command must reorder the live palette"
    );
}

#[test]
fn submit_input_records_the_command_for_ranking() {
    let mut app = create_test_app();

    app.input = "/version".to_string();
    app.cursor_pos = app.input.len();
    app.submit_input();
    assert!(app.input.is_empty(), "submit must clear the composer");

    app.input = "/v".to_string();
    app.cursor_pos = app.input.len();
    app.advance_command_suggestions_epoch();
    let suggestions = app.command_suggestions();
    assert_eq!(
        suggestions[0].0, "/version",
        "the command just run must lead its prefix, got {suggestions:?}"
    );
}
