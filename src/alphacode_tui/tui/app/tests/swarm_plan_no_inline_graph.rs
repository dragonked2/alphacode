#[test]
fn swarm_plan_updates_state_without_adding_an_inline_diagram() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
    remote.mark_history_loaded();
    let message_count = app.display_messages().len();

    let item = crate::plan::PlanItem {
        content: "write a haiku".to_string(),
        status: "running".to_string(),
        priority: "high".to_string(),
        id: "haiku-1".to_string(),
        subsystem: None,
        file_scope: Vec::new(),
        blocked_by: Vec::new(),
        assigned_to: Some("worker-fox".to_string()),
    };

    app.handle_server_event(
        crate::protocol::ServerEvent::SwarmPlan {
            swarm_id: "test-swarm".to_string(),
            version: 3,
            items: vec![item.clone()],
            participants: vec!["session_a".to_string()],
            reason: None,
            summary: None,
        },
        &mut remote,
    );

    assert_eq!(app.swarm_plan_swarm_id.as_deref(), Some("test-swarm"));
    assert_eq!(app.swarm_plan_version, Some(3));
    assert_eq!(app.swarm_plan_items, vec![item]);
    assert_eq!(
        app.display_messages().len(),
        message_count,
        "plan updates should not add transcript messages"
    );
    assert!(app.display_messages().iter().all(|message| {
        !message
            .title
            .as_deref()
            .is_some_and(|title| title.starts_with("Plan graph · "))
    }));
}

