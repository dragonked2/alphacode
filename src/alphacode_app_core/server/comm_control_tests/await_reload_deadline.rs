#[test]
fn await_observed_cohort_survives_reload_removal_and_preference_updates() {
    use crate::alphacode_app_core::server::await_members_state as persistence;
    let (_env, _runtime_dir) = RuntimeEnvGuard::new();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let state = persistence::ensure_pending_state(
        "cohort-regression",
        "requester",
        "swarm",
        &[],
        &["completed".into()],
        None,
        now_ms + 60_000,
        true,
        false,
        false,
    );
    let running = crate::protocol::AwaitedMemberStatus {
        session_id: "peer-1".into(),
        friendly_name: None,
        status: "running".into(),
        done: false,
        completion_report: None,
    };
    persistence::record_observations(&state, &[running.clone()]).unwrap();
    let restored = persistence::load_state(&state.key).unwrap();
    let missing = persistence::record_observations(&restored, &[]).unwrap();
    assert_eq!(missing.observed_members.len(), 1);
    assert_eq!(missing.observed_members[0].status, "unknown");
    assert!(!missing.observed_members[0].done);
    let mut finished = running.clone();
    finished.status = "completed".into();
    finished.done = true;
    let mut late_peer = running;
    late_peer.session_id = "peer-2".into();
    let observed =
        persistence::record_observations(&missing, &[finished, late_peer.clone()]).unwrap();
    let removed_finished = persistence::record_observations(&observed, &[late_peer]).unwrap();
    assert_eq!(removed_finished.observed_members.len(), 2);
    assert!(
        removed_finished
            .observed_members
            .iter()
            .any(|member| member.session_id == "peer-1" && member.done)
    );
    let mut prefs = state;
    prefs.notify = true;
    persistence::save_state(&prefs);
    let updated = persistence::load_state(&prefs.key).unwrap();
    assert!(updated.notify);
    assert_eq!(updated.observed_members, removed_finished.observed_members);
    let (_, first_finalize) = persistence::persist_final_response(
        &updated,
        false,
        updated.observed_members.clone(),
        "timeout".into(),
    );
    assert!(first_finalize);
    let (winner, second_finalize) = persistence::persist_final_response(
        &updated,
        true,
        vec![],
        "conflicting late completion".into(),
    );
    assert!(!second_finalize);
    assert!(!winner.final_response.unwrap().completed);
    persistence::save_state(&prefs);
    assert!(!persistence::load_state(&prefs.key).unwrap().is_pending());
    let mut legacy = serde_json::to_value(updated).unwrap();
    legacy.as_object_mut().unwrap().remove("observed_members");
    let legacy: persistence::PersistedAwaitMembersState = serde_json::from_value(legacy).unwrap();
    assert!(legacy.observed_members.is_empty());
}

#[tokio::test]
async fn await_members_reuses_persisted_deadline_after_reload_retry() {
    let (_env, _runtime_dir) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-c";
    let requester = "req";
    let peer = "peer-1";
    let key = crate::alphacode_app_core::server::await_members_state::request_key(
        requester,
        swarm_id,
        &[],
        &["completed".to_string()],
        None,
    );
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    crate::alphacode_app_core::server::await_members_state::save_state(
        &crate::alphacode_app_core::server::await_members_state::PersistedAwaitMembersState {
            key,
            session_id: requester.to_string(),
            swarm_id: swarm_id.to_string(),
            target_status: vec!["completed".to_string()],
            requested_ids: vec![],
            observed_members: vec![],
            mode: None,
            created_at_unix_ms: now_ms,
            deadline_unix_ms: now_ms + 150,
            background: false,
            notify: false,
            wake: false,
            final_response: None,
        },
    );

    let await_runtime = AwaitMembersRuntime::default();
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, swarm_id, "ready")),
        (peer.to_string(), member(peer, swarm_id, "running")),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([requester.to_string(), peer.to_string()]),
    )])));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);

    handle_comm_await_members(
        1,
        requester.to_string(),
        vec!["completed".to_string()],
        vec![],
        None,
        Some(60),
        false,
        false,
        false,
        CommAwaitMembersContext {
            client_event_tx: &client_tx,
            swarm_members: &swarm_members,
            swarms_by_id: &swarms_by_id,
            swarm_event_tx: &swarm_event_tx,
            await_members_runtime: &await_runtime,
        },
    )
    .await;

    let started = Instant::now();
    let response = tokio::time::timeout(Duration::from_secs(1), client_rx.recv())
        .await
        .expect("response should arrive")
        .expect("channel should stay open");

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "persisted deadline should win over new timeout"
    );

    match response {
        ServerEvent::CommAwaitMembersResponse {
            completed, summary, ..
        } => {
            assert!(!completed, "persisted expired wait should time out");
            assert!(summary.contains("Timed out"));
        }
        other => panic!("expected CommAwaitMembersResponse, got {other:?}"),
    }
}
