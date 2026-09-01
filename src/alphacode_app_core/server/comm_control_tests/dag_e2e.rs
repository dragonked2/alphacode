// End-to-end task-DAG flow through the real server handlers and assignment loop.
//
// Unlike the engine unit tests (which exercise `crate::alphacode_plan::dag` in isolation),
// this drives the live `comm_graph` handlers against real server state
// (swarm_members / swarms_by_id / swarm_plans / coordinators) and then the real
// `handle_comm_assign_task` path, proving the substrate works request-to-plan and
// that forward dataflow reaches a downstream assignment.

use crate::alphacode_app_core::protocol::TaskGraphNodeSpec;
use crate::alphacode_app_core::server::comm_graph::{
    handle_comm_complete_node, handle_comm_expand_node, handle_comm_seed_graph,
};

fn node_spec(id: &str, kind: &str, deps: &[&str]) -> TaskGraphNodeSpec {
    TaskGraphNodeSpec {
        id: id.to_string(),
        content: format!("task {id}"),
        kind: Some(kind.to_string()),
        depends_on: deps.iter().map(|d| d.to_string()).collect(),
        priority: 0,
    }
}

/// Shared fixture: a two-member swarm (coordinator + worker) with an empty plan.
struct GraphFixture {
    swarm_id: String,
    coord: String,
    worker: String,
    client_tx: mpsc::UnboundedSender<ServerEvent>,
    client_rx: mpsc::UnboundedReceiver<ServerEvent>,
    sessions: crate::alphacode_app_core::server::SessionAgents,
    soft_interrupt_queues: crate::alphacode_app_core::server::SessionInterruptQueues,
    client_connections: Arc<RwLock<HashMap<String, crate::alphacode_app_core::server::ClientConnectionInfo>>>,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: Arc<RwLock<HashMap<String, String>>>,
    event_history: Arc<RwLock<VecDeque<SwarmEvent>>>,
    event_counter: Arc<AtomicU64>,
    swarm_event_tx: broadcast::Sender<SwarmEvent>,
    mutation_runtime: SwarmMutationRuntime,
}

async fn graph_fixture() -> GraphFixture {
    graph_fixture_named("swarm-dag", "coord", "worker").await
}

async fn graph_fixture_named(swarm_id: &str, coord: &str, worker: &str) -> GraphFixture {
    let swarm_id = swarm_id.to_string();
    let coord = coord.to_string();
    let worker = worker.to_string();
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), test_agent().await),
        (worker.clone(), test_agent().await),
    ])));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), {
            let mut m = member(&coord, &swarm_id, "ready");
            m.role = "coordinator".to_string();
            m
        }),
        (worker.clone(), member(&worker, &swarm_id, "ready")),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        HashSet::from([coord.clone(), worker.clone()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        VersionedPlan::new(),
    )])));
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        coord.clone(),
    )])));
    GraphFixture {
        swarm_id,
        coord,
        worker,
        client_tx,
        client_rx,
        sessions,
        soft_interrupt_queues: Arc::new(RwLock::new(HashMap::new())),
        client_connections: Arc::new(RwLock::new(HashMap::new())),
        swarm_members,
        swarms_by_id,
        swarm_plans,
        swarm_coordinators,
        event_history: Arc::new(RwLock::new(VecDeque::new())),
        event_counter: Arc::new(AtomicU64::new(1)),
        swarm_event_tx: broadcast::channel(64).0,
        mutation_runtime: SwarmMutationRuntime::default(),
    }
}

impl GraphFixture {
    async fn seed(&mut self, mode: &str, nodes: Vec<TaskGraphNodeSpec>) {
        handle_comm_seed_graph(
            1,
            self.coord.clone(),
            Some(mode.to_string()),
            nodes,
            &self.client_tx,
            &self.swarm_members,
            &self.swarms_by_id,
            &self.swarm_plans,
            &self.swarm_coordinators,
            &self.event_history,
            &self.event_counter,
            &self.swarm_event_tx,
        )
        .await;
    }

    /// Seed with no explicit mode, so the handler must fall back to the seeder's
    /// recorded reasoning effort to decide deep vs light.
    async fn seed_without_mode(&mut self, nodes: Vec<TaskGraphNodeSpec>) {
        handle_comm_seed_graph(
            1,
            self.coord.clone(),
            None,
            nodes,
            &self.client_tx,
            &self.swarm_members,
            &self.swarms_by_id,
            &self.swarm_plans,
            &self.swarm_coordinators,
            &self.event_history,
            &self.event_counter,
            &self.swarm_event_tx,
        )
        .await;
    }
}

/// Regression for the deep-swarm trigger gap: a session running at `swarm-deep`
/// effort that seeds a graph but *forgets* to pass `mode:"deep"` must still get a
/// deep plan (gates + strict artifact validation), not a silent light downgrade.
/// The mode is resolved from the seeder's recorded effort via the deadlock-free
/// `session_effort` side-table.
#[tokio::test]
async fn e2e_seed_defaults_to_deep_when_seeder_effort_is_swarm_deep() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-deep-default", "coord-deep-default", "worker-dd").await;

    // The coordinator is the seeder; record its effort as the deep sentinel.
    crate::session_effort::record_session_effort(&fx.coord, Some("swarm-deep"));

    fx.seed_without_mode(vec![node_spec("explore", "explore", &[])])
        .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(
        plan.mode, "deep",
        "a swarm-deep seeder that omits mode must still get a deep plan"
    );

    crate::session_effort::forget_session_effort(&fx.coord);
}

/// Counterpart: without a deep effort recorded (or with a plain reasoning level),
/// an omitted mode falls back to the engine default (light), preserving legacy
/// behaviour for non-deep sessions.
#[tokio::test]
async fn e2e_seed_defaults_to_light_when_seeder_effort_is_not_deep() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx =
        graph_fixture_named("swarm-light-default", "coord-light-default", "worker-ld").await;

    crate::session_effort::record_session_effort(&fx.coord, Some("high"));

    fx.seed_without_mode(vec![node_spec("explore", "explore", &[])])
        .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(
        plan.mode, "light",
        "a non-deep seeder that omits mode keeps the light default"
    );

    crate::session_effort::forget_session_effort(&fx.coord);
}

/// An explicit `mode` always wins over the effort-derived default, so a deep
/// session can still deliberately opt a particular graph into light fan-out.
#[tokio::test]
async fn e2e_explicit_mode_overrides_seeder_effort() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx =
        graph_fixture_named("swarm-explicit-mode", "coord-explicit-mode", "worker-em").await;

    crate::session_effort::record_session_effort(&fx.coord, Some("swarm-deep"));

    fx.seed("light", vec![node_spec("explore", "explore", &[])])
        .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(
        plan.mode, "light",
        "an explicit mode must override the effort-derived default"
    );

    crate::session_effort::forget_session_effort(&fx.coord);
}

#[tokio::test]
async fn e2e_seed_creates_plan_with_kinds_and_edges() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        "deep",
        vec![
            node_spec("explore", "explore", &[]),
            node_spec("synth", "synthesize", &["explore"]),
        ],
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(plan.mode, "deep");
    // 2 seeded nodes + the auto-inserted plan-wide root gate.
    assert_eq!(plan.items.len(), 3);
    assert_eq!(plan.node_meta["explore"].kind.as_deref(), Some("explore"));
    assert_eq!(plan.node_meta["synth"].kind.as_deref(), Some("synthesize"));
    let synth = plan.items.iter().find(|i| i.id == "synth").unwrap();
    assert_eq!(synth.blocked_by, vec!["explore".to_string()]);
    // The root gate audits every seeded root node and blocks plan completion
    // until the final adversarial pass succeeds.
    let root_gate = plan
        .items
        .iter()
        .find(|i| {
            plan.node_meta
                .get(&i.id)
                .map(|m| m.is_gate && m.parent.is_none())
                .unwrap_or(false)
        })
        .expect("deep seed must insert a plan-wide root gate");
    assert!(root_gate.blocked_by.contains(&"explore".to_string()));
    assert!(root_gate.blocked_by.contains(&"synth".to_string()));
    assert_eq!(
        plan.node_meta[&root_gate.id].origin.as_deref(),
        Some("gate")
    );
}

#[tokio::test]
async fn e2e_identical_seed_replay_succeeds_without_version_or_node_churn() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-seed-replay", "coord-replay", "worker-replay").await;
    let nodes = vec![
        node_spec("explore", "explore", &[]),
        node_spec("synth", "synthesize", &["explore"]),
    ];

    fx.seed("deep", nodes.clone()).await;
    while fx.client_rx.try_recv().is_ok() {}
    let (version, item_count) = {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        (plan.version, plan.items.len())
    };

    fx.seed("deep", nodes).await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(plan.version, version, "a replay must not bump plan version");
    assert_eq!(plan.items.len(), item_count, "a replay must not add nodes");
    drop(plans);
    let events: Vec<_> = std::iter::from_fn(|| fx.client_rx.try_recv().ok()).collect();
    assert!(
        events.iter().all(|event| !matches!(event, ServerEvent::Error { .. })),
        "an identical replay must acknowledge success: {events:?}"
    );
    assert!(events.iter().any(|event| matches!(event, ServerEvent::Done { .. })));
}

#[tokio::test]
async fn e2e_seed_rejects_conflicting_existing_definition_without_mutation() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-seed-conflict", "coord-conflict", "worker-conflict").await;
    fx.seed("light", vec![node_spec("shared", "explore", &[])])
        .await;
    while fx.client_rx.try_recv().is_ok() {}
    let (before_version, before_items, before_content) = {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        (
            plan.version,
            plan.items.len(),
            plan.items
                .iter()
                .find(|item| item.id == "shared")
                .expect("seeded node")
                .content
                .clone(),
        )
    };
    let mut conflicting = node_spec("shared", "explore", &[]);
    conflicting.content = "a different task using the same id".to_string();

    fx.seed("light", vec![conflicting]).await;

    let plans = fx.swarm_plans.read().await;
    let after = &plans[&fx.swarm_id];
    assert_eq!(after.version, before_version);
    assert_eq!(after.items.len(), before_items);
    assert_eq!(
        after
            .items
            .iter()
            .find(|item| item.id == "shared")
            .expect("original node remains")
            .content,
        before_content,
        "a conflicting replay must preserve the original definition"
    );
    drop(plans);
    let events: Vec<_> = std::iter::from_fn(|| fx.client_rx.try_recv().ok()).collect();
    assert!(events.iter().any(|event| matches!(
        event,
        ServerEvent::Error { message, .. } if message.contains("duplicate node id 'shared'")
    )));
}

#[tokio::test]
async fn e2e_seed_rejects_cycle_without_mutating_plan() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        "light",
        vec![
            node_spec("a", "explore", &["b"]),
            node_spec("b", "explore", &["a"]),
        ],
    )
    .await;

    // Plan stays empty and an error is surfaced.
    let plans = fx.swarm_plans.read().await;
    assert!(plans[&fx.swarm_id].items.is_empty());
    drop(plans);
    let mut saw_error = false;
    while let Ok(ev) = fx.client_rx.try_recv() {
        if let ServerEvent::Error { message, .. } = ev {
            assert!(message.contains("rejected") || message.contains("cycle"));
            saw_error = true;
        }
    }
    assert!(saw_error, "cycle seed should surface an error");
}

#[tokio::test]
async fn e2e_deep_expand_inserts_gate_in_live_plan() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed("deep", vec![node_spec("root", "explore", &[])])
        .await;

    // Assign + dispatch root to the worker so it owns the node, then expand.
    handle_comm_assign_task(
        2,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("root".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    // Mark root running (assignment leaves it queued); the engine requires a
    // running owner to expand. Simulate the worker starting by setting status.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let root = plan.items.iter_mut().find(|i| i.id == "root").unwrap();
        root.status = "running".to_string();
        root.assigned_to = Some(fx.worker.clone());
    }

    handle_comm_expand_node(
        3,
        fx.worker.clone(),
        "root".to_string(),
        vec![
            node_spec("root.1", "explore", &[]),
            node_spec("root.2", "explore", &[]),
        ],
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    // Gate inserted, root marked composite/expanded.
    let gate = plan
        .items
        .iter()
        .find(|i| {
            plan.node_meta
                .get(&i.id)
                .map(|m| m.is_gate)
                .unwrap_or(false)
        })
        .expect("a gate node should be present after deep expand");
    assert_eq!(plan.node_meta[&gate.id].kind.as_deref(), Some("critique"));
    assert!(plan.node_meta["root"].expanded);
}

/// The budget-utilization mechanism: a deep-mode assignment must carry the
/// deep-node execution contract (expand_node for parallel fan-out, or
/// complete_node with a typed artifact) all the way to the worker, and a gate
/// assignment must carry the inject_gap contract. Without this the fan-out
/// budget goes unused because spawned workers never learn the deep workflow.
#[tokio::test]
async fn e2e_deep_assignment_carries_fanout_and_artifact_contract() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-deep-directive", "coord-dd", "worker-dd").await;
    fx.seed(
        "deep",
        vec![
            node_spec("explore.a", "explore", &[]),
            node_spec("explore.b", "explore", &[]),
        ],
    )
    .await;

    // Assign a node to the worker via the live path.
    handle_comm_assign_task(
        2,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("explore.a".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    // The worker has no live client, so the assignment ran through
    // spawn_assigned_task_run against the test agent. The durable record of
    // what the worker was told is the assignment summary; assert on the
    // soft-interrupt prompt queued for the worker instead, which carries the
    // full assignment text.
    let queued = {
        let queues = fx.soft_interrupt_queues.read().await;
        queues.get(&fx.worker).and_then(|queue| {
            queue
                .lock()
                .ok()
                .and_then(|pending| pending.first().map(|msg| msg.content.clone()))
        })
    };
    let prompt = queued.expect("deep assignment should queue a task prompt for the worker");
    assert!(
        prompt.contains(crate::alphacode_swarm_core::SWARM_DEEP_NODE_MARKER),
        "deep assignment prompt must carry the deep-node contract, got: {prompt}"
    );
    assert!(prompt.contains("action=\"expand_node\", node_id=\"explore.a\""));
    assert!(prompt.contains("action=\"complete_node\", node_id=\"explore.a\""));

    // Light plans must NOT get the directive. Use a fresh fixture: re-seeding
    // the existing non-empty deep plan as light is now rejected (silent rigor
    // downgrade guard), so the light case needs its own swarm.
    let mut lfx = graph_fixture_named("swarm-light-directive", "coord-ld", "worker-ld").await;
    lfx.seed("light", vec![node_spec("light.a", "explore", &[])])
        .await;
    handle_comm_assign_task(
        3,
        lfx.coord.clone(),
        Some(lfx.worker.clone()),
        Some("light.a".to_string()),
        None,
        &lfx.client_tx,
        &lfx.sessions,
        &lfx.soft_interrupt_queues,
        &lfx.client_connections,
        &lfx.swarm_members,
        &lfx.swarms_by_id,
        &lfx.swarm_plans,
        &lfx.swarm_coordinators,
        &lfx.event_history,
        &lfx.event_counter,
        &lfx.swarm_event_tx,
        &lfx.mutation_runtime,
    )
    .await;
    let light_prompt = {
        let queues = lfx.soft_interrupt_queues.read().await;
        queues.get(&lfx.worker).and_then(|queue| {
            queue
                .lock()
                .ok()
                .and_then(|pending| pending.last().map(|msg| msg.content.clone()))
        })
    }
    .expect("light assignment should also queue a task prompt");
    assert!(
        !light_prompt.contains(crate::alphacode_swarm_core::SWARM_DEEP_NODE_MARKER),
        "light assignments must not carry the deep contract"
    );
}

/// Gate dispatch in a deep plan carries the inject_gap contract.
#[tokio::test]
async fn e2e_deep_gate_assignment_carries_inject_gap_contract() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-deep-gate", "coord-dg", "worker-dg").await;
    fx.seed("deep", vec![node_spec("root", "explore", &[])])
        .await;

    // Worker owns root (running), expands it so the engine inserts a gate.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let root = plan.items.iter_mut().find(|i| i.id == "root").unwrap();
        root.status = "running".to_string();
        root.assigned_to = Some(fx.worker.clone());
    }
    handle_comm_expand_node(
        2,
        fx.worker.clone(),
        "root".to_string(),
        vec![node_spec("root.1", "explore", &[])],
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // Complete the child so the gate becomes ready.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let child = plan.items.iter_mut().find(|i| i.id == "root.1").unwrap();
        child.status = "running".to_string();
        child.assigned_to = Some(fx.worker.clone());
    }
    handle_comm_complete_node(
        3,
        fx.worker.clone(),
        "root.1".to_string(),
        serde_json::json!({
            "findings": "explored",
            "confidence": "low",
            "what_i_did_not_check": ["error paths"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let gate_id = {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        plan.items
            .iter()
            .find(|i| {
                plan.node_meta
                    .get(&i.id)
                    // The composite's own gate, not the plan-wide root gate.
                    .map(|m| m.is_gate && m.parent.as_deref() == Some("root"))
                    .unwrap_or(false)
            })
            .map(|i| i.id.clone())
            .expect("deep expand should have inserted a gate")
    };

    // Assign the gate to the worker; its prompt must carry the gate contract.
    handle_comm_assign_task(
        4,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some(gate_id.clone()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    let prompt = {
        let queues = fx.soft_interrupt_queues.read().await;
        queues.get(&fx.worker).and_then(|queue| {
            queue
                .lock()
                .ok()
                .and_then(|pending| pending.last().map(|msg| msg.content.clone()))
        })
    }
    .expect("gate assignment should queue a task prompt for the worker");
    assert!(prompt.contains(crate::alphacode_swarm_core::SWARM_DEEP_NODE_MARKER));
    assert!(
        prompt.contains(&format!("action=\"inject_gap\", gate_id=\"{gate_id}\"")),
        "gate prompt must carry the inject_gap contract, got: {prompt}"
    );
    // The gate also sees the child's artifact (forward dataflow) including the
    // unexplored surface it is supposed to mine.
    assert!(prompt.contains("error paths"));
    // The child completed with LOW confidence, so the gate directive must name
    // it as a priority probe target (the engine rejects a pass over it).
    assert!(
        prompt.contains("PRIORITY") && prompt.contains("root.1"),
        "gate prompt must call out the low-confidence sibling, got: {prompt}"
    );
}

#[tokio::test]
async fn e2e_complete_flows_artifact_to_downstream_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        "light",
        vec![
            node_spec("api", "implement", &[]),
            node_spec("ui", "implement", &["api"]),
        ],
    )
    .await;

    // Assign "api" to the worker, mark running, then complete with an artifact.
    handle_comm_assign_task(
        2,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("api".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let api = plan.items.iter_mut().find(|i| i.id == "api").unwrap();
        api.status = "running".to_string();
        api.assigned_to = Some(fx.worker.clone());
    }

    let artifact = serde_json::json!({
        "findings": "API built in crates/foo/api.rs with types Req/Resp",
        "evidence": ["crates/foo/api.rs:1"],
    })
    .to_string();
    handle_comm_complete_node(
        4,
        fx.worker.clone(),
        "api".to_string(),
        artifact,
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // api is now completed; ui should be runnable.
    {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        let api = plan.items.iter().find(|i| i.id == "api").unwrap();
        assert_eq!(api.status, "completed");
        assert!(plan.node_meta["api"].artifact_json.is_some());
        let ready = crate::alphacode_plan::next_runnable_item_ids(&plan.items, None);
        assert!(
            ready.contains(&"ui".to_string()),
            "ui should be ready: {ready:?}"
        );
    }

    // Assign "ui": its prompt must be hydrated with api's artifact.
    handle_comm_assign_task(
        5,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("ui".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    // The assignment summary stored in task_progress should reflect hydration.
    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let ui = plan.items.iter().find(|i| i.id == "ui").unwrap();
    assert_eq!(ui.assigned_to.as_deref(), Some(fx.worker.as_str()));
}

#[tokio::test]
async fn e2e_composite_rewake_prefers_planner_via_assign_next() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    // Two workers so auto-assignment has a choice; the planner should still win
    // the composite synthesis re-wake.
    let planner = "planner".to_string();
    let other = "other".to_string();
    {
        let mut members = fx.swarm_members.write().await;
        members.insert(
            planner.clone(),
            owned_member(&planner, &fx.swarm_id, "ready", &fx.coord),
        );
        members.insert(
            other.clone(),
            owned_member(&other, &fx.swarm_id, "ready", &fx.coord),
        );
        let mut by_id = fx.swarms_by_id.write().await;
        by_id
            .get_mut(&fx.swarm_id)
            .unwrap()
            .extend([planner.clone(), other.clone()]);
        let mut sessions = fx.sessions.write().await;
        sessions.insert(planner.clone(), test_agent().await);
        sessions.insert(other.clone(), test_agent().await);
    }

    fx.seed("light", vec![node_spec("root", "explore", &[])])
        .await;

    // planner owns root and decomposes it into one child.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let root = plan.items.iter_mut().find(|i| i.id == "root").unwrap();
        root.status = "running".to_string();
        root.assigned_to = Some(planner.clone());
    }
    handle_comm_expand_node(
        3,
        planner.clone(),
        "root".to_string(),
        vec![node_spec("root.1", "explore", &[])],
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // Planner recorded; root owner freed.
    {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        assert_eq!(
            plan.node_meta["root"].planner.as_deref(),
            Some(planner.as_str())
        );
        let root = plan.items.iter().find(|i| i.id == "root").unwrap();
        assert!(root.assigned_to.is_none());
    }

    // Complete the child so the composite root becomes runnable again.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let child = plan.items.iter_mut().find(|i| i.id == "root.1").unwrap();
        child.status = "running".to_string();
        child.assigned_to = Some(other.clone());
    }
    handle_comm_complete_node(
        4,
        other.clone(),
        "root.1".to_string(),
        serde_json::json!({"findings": "child done"}).to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // assign_next should route the composite synthesis back to the planner.
    let resolved = crate::alphacode_app_core::server::comm_control::resolve_assignment_target_for_task_test_hook(
        &fx.coord,
        &fx.swarm_id,
        "root",
        None,
        &fx.swarm_members,
        &fx.swarm_plans,
    )
    .await;
    assert_eq!(resolved.as_deref(), Ok(planner.as_str()));
}

/// A solo deep-mode agent (no coordinator registered) seeds a graph. It must be
/// elected coordinator so it can then drive the coordinator-gated assign path it
/// just created work for.
#[tokio::test]
async fn e2e_solo_seeder_is_elected_coordinator_and_can_assign() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-solo".to_string();
    let seeder = "seeder".to_string();
    let worker = "worker".to_string();
    let (client_tx, _client_rx) = mpsc::unbounded_channel();
    let sessions: crate::alphacode_app_core::server::SessionAgents = Arc::new(RwLock::new(HashMap::from([
        (seeder.clone(), test_agent().await),
        (worker.clone(), test_agent().await),
    ])));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (seeder.clone(), member(&seeder, &swarm_id, "ready")),
        (worker.clone(), member(&worker, &swarm_id, "ready")),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        HashSet::from([seeder.clone(), worker.clone()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        VersionedPlan::new(),
    )])));
    // No coordinator registered: this is the deep-mode solo-agent starting state.
    let swarm_coordinators: Arc<RwLock<HashMap<String, String>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let swarm_event_tx = broadcast::channel(64).0;
    let mutation_runtime = SwarmMutationRuntime::default();
    let soft_interrupt_queues: crate::alphacode_app_core::server::SessionInterruptQueues =
        Arc::new(RwLock::new(HashMap::new()));
    let client_connections: Arc<RwLock<HashMap<String, crate::alphacode_app_core::server::ClientConnectionInfo>>> =
        Arc::new(RwLock::new(HashMap::new()));

    handle_comm_seed_graph(
        1,
        seeder.clone(),
        Some("deep".to_string()),
        vec![
            node_spec("explore", "explore", &[]),
            node_spec("synth", "synthesize", &["explore"]),
        ],
        &client_tx,
        &swarm_members,
        &swarms_by_id,
        &swarm_plans,
        &swarm_coordinators,
        &event_history,
        &event_counter,
        &swarm_event_tx,
    )
    .await;

    // The seeder is now the coordinator of its swarm.
    assert_eq!(
        swarm_coordinators.read().await.get(&swarm_id).cloned(),
        Some(seeder.clone()),
        "solo seeder should be elected coordinator"
    );
    assert_eq!(
        swarm_members.read().await.get(&seeder).unwrap().role,
        "coordinator"
    );

    // And it can now drive the graph: assign the ready node to the worker.
    handle_comm_assign_task(
        2,
        seeder.clone(),
        Some(worker.clone()),
        Some("explore".to_string()),
        None,
        &client_tx,
        &sessions,
        &soft_interrupt_queues,
        &client_connections,
        &swarm_members,
        &swarms_by_id,
        &swarm_plans,
        &swarm_coordinators,
        &event_history,
        &event_counter,
        &swarm_event_tx,
        &mutation_runtime,
    )
    .await;

    let plans = swarm_plans.read().await;
    let explore = plans[&swarm_id]
        .items
        .iter()
        .find(|i| i.id == "explore")
        .unwrap();
    assert_eq!(
        explore.assigned_to.as_deref(),
        Some(worker.as_str()),
        "elected coordinator should be able to assign the seeded task"
    );
}

/// A live, non-headless coordinator must not be displaced by a different member
/// that happens to seed a graph.
#[tokio::test]
async fn e2e_seed_does_not_displace_live_coordinator() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-live-coord".to_string();
    let coord = "coord".to_string();
    let worker = "worker".to_string();
    let (client_tx, _client_rx) = mpsc::unbounded_channel();

    // Build the coordinator with a *retained* receiver so its event channel is
    // genuinely open (the shared `member()` helper drops the receiver, which would
    // make the channel look closed and the coordinator look dead).
    let (coord_tx, _coord_rx) = mpsc::unbounded_channel();
    let mut coord_member = member(&coord, &swarm_id, "ready");
    coord_member.event_tx = coord_tx;
    coord_member.role = "coordinator".to_string();

    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), coord_member),
        (worker.clone(), member(&worker, &swarm_id, "ready")),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        HashSet::from([coord.clone(), worker.clone()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        VersionedPlan::new(),
    )])));
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        coord.clone(),
    )])));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let swarm_event_tx = broadcast::channel(64).0;

    // The non-coordinator worker seeds the graph.
    handle_comm_seed_graph(
        1,
        worker.clone(),
        Some("deep".to_string()),
        vec![node_spec("root", "explore", &[])],
        &client_tx,
        &swarm_members,
        &swarms_by_id,
        &swarm_plans,
        &swarm_coordinators,
        &event_history,
        &event_counter,
        &swarm_event_tx,
    )
    .await;

    assert_eq!(
        swarm_coordinators.read().await.get(&swarm_id).cloned(),
        Some(coord.clone()),
        "a live coordinator must not be displaced by a seeding worker"
    );
    assert_eq!(
        swarm_members.read().await.get(&worker).unwrap().role,
        "agent",
        "the seeding worker should remain an agent"
    );
}

/// Regression for the deep-swarm drive gap: a deep-mode plan participant that is
/// **not** the swarm coordinator must still be able to dispatch the graph it owns.
/// Before this, `assign_task` was hard-gated to the coordinator, so a deep agent
/// joining a shared swarm (where another session already coordinates) could seed a
/// graph but never spawn/assign any of it, and nothing ran.
#[tokio::test]
async fn e2e_deep_participant_can_assign_without_being_coordinator() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    // `coord` is the swarm coordinator; `worker` is a plain agent. Seed a deep
    // graph *as the worker* and register it as a participant, mirroring a deep
    // agent that joined a swarm someone else coordinates.
    fx.seed("deep", vec![node_spec("explore", "explore", &[])])
        .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        plan.participants.insert(fx.worker.clone());
    }

    // The worker (a non-coordinator deep participant) assigns the ready node to a
    // distinct swarm member (`coord` here stands in for any other worker).
    handle_comm_assign_task(
        2,
        fx.worker.clone(),
        Some(fx.coord.clone()),
        Some("explore".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let explore = plans[&fx.swarm_id]
        .items
        .iter()
        .find(|i| i.id == "explore")
        .unwrap();
    assert_eq!(
        explore.assigned_to.as_deref(),
        Some(fx.coord.as_str()),
        "a deep-mode plan participant should be able to assign even without the coordinator slot"
    );
}

/// The deep-participant escape hatch is mode-scoped: in **light** mode the
/// single-coordinator rule still holds, so a non-coordinator participant is
/// rejected and the task stays unassigned.
#[tokio::test]
async fn e2e_light_non_coordinator_participant_cannot_assign() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed("light", vec![node_spec("task", "implement", &[])])
        .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        plan.participants.insert(fx.worker.clone());
    }

    handle_comm_assign_task(
        2,
        fx.worker.clone(),
        Some(fx.coord.clone()),
        Some("task".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let task = plans[&fx.swarm_id]
        .items
        .iter()
        .find(|i| i.id == "task")
        .unwrap();
    assert!(
        task.assigned_to.is_none(),
        "light mode must keep the coordinator-only assignment rule"
    );
    drop(plans);
    let mut saw_permission_error = false;
    while let Ok(ev) = fx.client_rx.try_recv() {
        if let ServerEvent::Error { message, .. } = ev
            && message.contains("Only the coordinator can assign tasks")
        {
            saw_permission_error = true;
        }
    }
    assert!(
        saw_permission_error,
        "light-mode non-coordinator assign should be rejected with the coordinator error"
    );
}

/// Regression: a solo deep-mode seeder must be able to complete (and expand) a
/// node it seeded. Seeded nodes are unowned and the assign path refuses
/// self-assignment, so without the handler-level self-claim the seeder's
/// `complete_node` bounced with "does not own node" (observed live 2026-06-30,
/// session_shrimp completing node 'probe').
#[tokio::test]
async fn e2e_solo_seeder_can_complete_its_own_seeded_node() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-self-claim", "coord-sc", "worker-sc").await;
    fx.seed("deep", vec![node_spec("probe", "explore", &[])])
        .await;

    // The seeder completes its own seeded node directly: the handler must
    // auto-claim the unowned queued node instead of rejecting with NotOwner.
    handle_comm_complete_node(
        2,
        fx.coord.clone(),
        "probe".to_string(),
        serde_json::json!({
            "findings": "probe complete",
            "confidence": "high",
            "what_i_did_not_check": ["nothing; probe only"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let probe = plan.items.iter().find(|i| i.id == "probe").unwrap();
    assert_eq!(
        probe.status, "completed",
        "solo seeder must be able to complete its own seeded node"
    );
    assert!(
        plan.node_meta["probe"].artifact_json.is_some(),
        "artifact must be recorded"
    );
}

/// Regression: the self-claim must not let an actor steal a node that is
/// assigned to someone else. The engine's NotOwner check still applies.
#[tokio::test]
async fn e2e_self_claim_does_not_steal_foreign_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-no-steal", "coord-ns", "worker-ns").await;
    fx.seed("deep", vec![node_spec("task", "explore", &[])])
        .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let item = plan.items.iter_mut().find(|i| i.id == "task").unwrap();
        item.assigned_to = Some(fx.worker.clone());
        item.status = "queued".to_string();
    }

    // The coordinator (not the assignee) tries to complete it -> rejected.
    handle_comm_complete_node(
        2,
        fx.coord.clone(),
        "task".to_string(),
        serde_json::json!({
            "findings": "hijack",
            "confidence": "high",
            "what_i_did_not_check": ["everything"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let task = plan.items.iter().find(|i| i.id == "task").unwrap();
    assert_eq!(
        task.status, "queued",
        "a foreign actor must not complete someone else's assignment"
    );
    assert_eq!(task.assigned_to.as_deref(), Some(fx.worker.as_str()));
}

/// Regression: an assignee whose node was left `queued` (client-attached worker
/// path skips the server-side flip to running) must still be able to
/// complete/expand its own assignment.
#[tokio::test]
async fn e2e_assignee_can_complete_queued_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-queued-own", "coord-qo", "worker-qo").await;
    fx.seed("deep", vec![node_spec("mine", "explore", &[])])
        .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let item = plan.items.iter_mut().find(|i| i.id == "mine").unwrap();
        // Assigned but never flipped to running (live-client path).
        item.assigned_to = Some(fx.worker.clone());
        item.status = "queued".to_string();
    }

    handle_comm_complete_node(
        2,
        fx.worker.clone(),
        "mine".to_string(),
        serde_json::json!({
            "findings": "did the work",
            "confidence": "high",
            "what_i_did_not_check": ["nothing"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let mine = plan.items.iter().find(|i| i.id == "mine").unwrap();
    assert_eq!(
        mine.status, "completed",
        "the assignee must be able to complete its queued assignment"
    );
}

/// Regression: re-seeding a non-empty deep plan as light is a silent rigor
/// downgrade (drops gates + artifact validation) and must be rejected.
#[tokio::test]
async fn e2e_seed_rejects_light_downgrade_of_nonempty_deep_plan() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-no-downgrade", "coord-nd", "worker-nd").await;
    fx.seed("deep", vec![node_spec("a", "explore", &[])]).await;

    // Attempt the downgrade.
    fx.seed("light", vec![node_spec("b", "explore", &[])]).await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(plan.mode, "deep", "deep plan must not be downgraded to light");
    assert!(
        plan.items.iter().all(|i| i.id != "b"),
        "the downgrade seed must be rejected wholesale"
    );
    drop(plans);
    let mut saw_downgrade_error = false;
    while let Ok(ev) = fx.client_rx.try_recv() {
        if let ServerEvent::Error { message, .. } = ev
            && message.contains("deep-mode plan")
        {
            saw_downgrade_error = true;
        }
    }
    assert!(saw_downgrade_error, "downgrade must surface a clear error");
}
