#[test]
fn skill_invocation_matches_a_multi_word_skill_name() {
    let mut app = create_test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let skill_dir = temp.path().join(".alphacode/skills/my-custom-skill");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: My Custom Skill\ndescription: Commit staged changes\n---\nUse it.\n",
    )
    .expect("write skill");
    app.session.working_dir = Some(temp.path().to_string_lossy().to_string());
    app.input = "/My Custom Skill".to_string();
    app.cursor_pos = app.input.len();

    app.submit_input();

    assert_eq!(app.active_skill.as_deref(), Some("My Custom Skill"));
    let last = app.display_messages().last().expect("activation message");
    assert!(
        last.content.contains("Activated skill: My Custom Skill"),
        "{}",
        last.content
    );
    assert!(!last.content.contains("Unknown skill"), "{}", last.content);
}

#[test]
fn accepting_a_skill_suggestion_activates_it_and_clears_the_composer() {
    let mut app = create_test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let skill_dir = temp.path().join(".alphacode/skills/bugbounty");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: bugbounty\ndescription: Test a scoped target\n---\nUse the authorized scope.\n",
    )
    .expect("write skill");
    app.session.working_dir = Some(temp.path().to_string_lossy().to_string());
    app.input = "/bug".to_string();
    app.cursor_pos = app.input.len();

    let suggestions = app.command_suggestions();
    let selected = suggestions
        .iter()
        .position(|(command, _)| command == "/bugbounty")
        .expect("project skill suggestion");
    app.command_suggestion_selected = selected;
    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .expect("accept skill suggestion");

    assert_eq!(app.active_skill.as_deref(), Some("bugbounty"));
    assert!(app.input.is_empty(), "skill command remained in the composer");
    assert!(app
        .display_messages()
        .iter()
        .any(|message| message.content.contains("Activated skill: bugbounty")));
}
