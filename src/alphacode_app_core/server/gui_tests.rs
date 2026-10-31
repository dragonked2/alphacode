use super::client_actions::{NotifySessionContext, handle_companion_prompt};
use super::*;

#[tokio::test]
async fn companion_prompt_queues_once_as_user_input_without_attaching_a_client() {
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    let queue = Arc::new(std::sync::Mutex::new(Vec::new()));
    let queues = Arc::new(RwLock::new(HashMap::from([(
        "target".to_string(),
        Arc::clone(&queue),
    )])));
    let connections = Arc::new(RwLock::new(HashMap::new()));
    let members = Arc::new(RwLock::new(HashMap::new()));
    let swarms = Arc::new(RwLock::new(HashMap::new()));
    let history = Arc::new(RwLock::new(std::collections::VecDeque::new()));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let (events, _) = tokio::sync::broadcast::channel(8);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    for (id, target, prompt) in [
        (1, "target", " "),
        (2, "missing", "review it"),
        (3, "target", "review it"),
    ] {
        handle_companion_prompt(
            id,
            target.into(),
            prompt.into(),
            NotifySessionContext {
                sessions: &sessions,
                soft_interrupt_queues: &queues,
                client_connections: &connections,
                swarm_members: &members,
                swarms_by_id: &swarms,
                event_history: &history,
                event_counter: &counter,
                swarm_event_tx: &events,
                client_event_tx: &tx,
            },
        )
        .await;
    }
    assert!(matches!(
        rx.try_recv().unwrap(),
        ServerEvent::Error { id: 1, .. }
    ));
    assert!(matches!(
        rx.try_recv().unwrap(),
        ServerEvent::Error { id: 2, .. }
    ));
    assert!(matches!(
        rx.try_recv().unwrap(),
        ServerEvent::Done { id: 3 }
    ));
    assert!(connections.read().await.is_empty());
    let queued = queue.lock().unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].content, "review it");
    assert!(matches!(
        queued[0].source,
        crate::alphacode_agent_runtime::SoftInterruptSource::User
    ));
}
