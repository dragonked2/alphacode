#[tokio::test]
async fn await_members_blocking_to_background_upgrade_survives_waiter_disconnect() {
    let (_env, _runtime_dir) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-upgrade";
    let requester = "req-upgrade";
    let peer = "peer-1";
    let await_runtime = AwaitMembersRuntime::default();

    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, swarm_id, "ready")),
        (peer.to_string(), member(peer, swarm_id, "running")),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([requester.to_string(), peer.to_string()]),
    )])));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);

    let mut bus_rx = crate::bus::Bus::global().subscribe();

    // First request: blocking. Spawns the watcher with a blocking-state copy.
    let (blocking_tx, blocking_rx) = mpsc::unbounded_channel();
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
            client_event_tx: &blocking_tx,
            swarm_members: &swarm_members,
            swarms_by_id: &swarms_by_id,
            swarm_event_tx: &swarm_event_tx,
            await_members_runtime: &await_runtime,
        },
    )
    .await;

    // Duplicate request upgrades the same wait to background delivery. The
    // watcher is already active, so no new one spawns; only the persisted
    // prefs change.
    let (bg_tx, mut bg_rx) = mpsc::unbounded_channel();
    handle_comm_await_members(
        2,
        requester.to_string(),
        vec!["completed".to_string()],
        vec![],
        None,
        Some(60),
        true,
        true,
        true,
        CommAwaitMembersContext {
            client_event_tx: &bg_tx,
            swarm_members: &swarm_members,
            swarms_by_id: &swarms_by_id,
            swarm_event_tx: &swarm_event_tx,
            await_members_runtime: &await_runtime,
        },
    )
    .await;

    match tokio::time::timeout(Duration::from_secs(1), bg_rx.recv())
        .await
        .expect("background upgrade should answer immediately")
        .expect("channel should stay open")
    {
        ServerEvent::CommAwaitMembersResponse {
            background_started, ..
        } => {
            assert!(
                background_started,
                "duplicate background request should report a running background watcher"
            );
        }
        other => panic!("expected CommAwaitMembersResponse, got {other:?}"),
    }

    // The original blocking waiter disconnects. The upgraded watcher must keep
    // running instead of exiting on waiter disconnect without finalizing.
    drop(blocking_rx);
    drop(blocking_tx);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // A non-satisfying event forces the watcher through its waiter check while
    // zero socket waiters remain. Before the fix it exited here silently.
    let _ = swarm_event_tx.send(swarm_event(
        peer,
        swarm_id,
        SwarmEventType::StatusChange {
            old_status: "running".to_string(),
            new_status: "working".to_string(),
        },
    ));
    tokio::time::sleep(Duration::from_millis(50)).await;

    {
        let mut members = swarm_members.write().await;
        members.get_mut(peer).expect("peer exists").status = "completed".to_string();
    }
    let _ = swarm_event_tx.send(swarm_event(
        peer,
        swarm_id,
        SwarmEventType::StatusChange {
            old_status: "running".to_string(),
            new_status: "completed".to_string(),
        },
    ));

    let event = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match bus_rx.recv().await {
                Ok(crate::bus::BusEvent::SwarmAwaitCompleted(event))
                    if event.session_id == requester =>
                {
                    return event;
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("bus closed before SwarmAwaitCompleted arrived")
                }
            }
        }
    })
    .await
    .expect("upgraded background await should deliver SwarmAwaitCompleted despite waiter disconnect");

    assert!(event.completed, "await should complete once peer is done");
    assert!(event.notify);
    assert!(event.wake);
}
