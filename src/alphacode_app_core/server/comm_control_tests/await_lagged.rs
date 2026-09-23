#[tokio::test]
async fn await_members_watcher_survives_broadcast_lag() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-lag";
    let requester = "req";
    let peer = "peer-1";
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
    // Tiny capacity so a burst of unconsumed events deterministically lags the
    // watcher's broadcast receiver.
    let (swarm_event_tx, swarm_event_rx) = broadcast::channel(1);
    drop(swarm_event_rx);

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

    // Wait until the watcher has subscribed, then give it a beat to finish its
    // first status check and park in recv().
    tokio::time::timeout(Duration::from_secs(1), async {
        while swarm_event_tx.receiver_count() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("watcher should subscribe to swarm events");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Mark the peer done, then flood the channel without yielding so the
    // parked watcher observes RecvError::Lagged instead of the actual events.
    {
        let mut members = swarm_members.write().await;
        members.get_mut(peer).expect("peer exists").status = "completed".to_string();
    }
    for _ in 0..4 {
        let _ = swarm_event_tx.send(swarm_event(
            peer,
            swarm_id,
            SwarmEventType::StatusChange {
                old_status: "running".to_string(),
                new_status: "completed".to_string(),
            },
        ));
    }

    let response = tokio::time::timeout(Duration::from_secs(1), client_rx.recv())
        .await
        .expect("lagged watcher should recover and respond instead of exiting")
        .expect("channel should stay open");

    match response {
        ServerEvent::CommAwaitMembersResponse {
            completed, members, ..
        } => {
            assert!(completed, "await should complete after lag recovery");
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].session_id, peer);
        }
        other => panic!("expected CommAwaitMembersResponse, got {other:?}"),
    }
}
