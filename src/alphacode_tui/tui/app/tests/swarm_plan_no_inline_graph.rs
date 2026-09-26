/// A `SwarmPlan` broadcast with an empty item list has nothing to draw, so no
/// inline diagram is pushed. The live plan state still updates (the swarm strip
/// and swarm page read `swarm_plan_*` directly), and an already-rendered card
/// from a previous non-empty plan is withdrawn rather than left stale.
#[test]
fn swarm_plan_with_no_items_does_not_add_an_inline_diagram() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::alphacode_tui::tui::backend::RemoteConnection::dummy();
    remote.mark_history_loaded();
    let message_count = app.display_messages().len();

    let plan = |version: u64, items: Vec<crate::plan::PlanItem>| {
        crate::protocol::ServerEvent::SwarmPlan {
            swarm_id: "test-swarm".to_string(),
            version,
            items,
            participants: vec!["session_a".to_string()],
            reason: None,
            summary: None,
        }
    };
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

    // A non-empty plan does render the diagram, so the assertion below is
    // about the empty plan rather than about the diagram feature as a whole.
    app.handle_server_event(plan(2, vec![item.clone()]), &mut remote);
    assert!(
        app.display_messages().iter().any(|message| {
            message
                .title
                .as_deref()
                .is_some_and(|title| title.starts_with("Plan graph · "))
        }),
        "a non-empty plan should render its inline diagram"
    );

    // The plan is emptied: the stale card is withdrawn instead of lingering.
    app.handle_server_event(plan(3, Vec::new()), &mut remote);
    assert_eq!(app.swarm_plan_version, Some(3));
    assert_eq!(app.display_messages().len(), message_count);
    assert!(app.display_messages().iter().all(|message| {
        !message
            .title
            .as_deref()
            .is_some_and(|title| title.starts_with("Plan graph · "))
    }));
}

