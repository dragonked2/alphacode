//! Invariant tests for the task-DAG engine, including a full simulator run that
//! reproduces the worked example in `docs/SWARM_TASK_GRAPH.md` section 9.

use super::sim::{self, WorkerAction};
use super::*;

fn spec(id: &str, kind: NodeKind) -> NodeSpec {
    NodeSpec::new(id, format!("task {id}"), kind)
}

fn dag(mode: Mode, specs: Vec<NodeSpec>) -> TaskGraph {
    let mut g = TaskGraph::new(mode);
    seed(&mut g, specs).expect("seed should succeed");
    g
}

// ----- seed validation -----

#[test]
fn seed_rejects_duplicate_ids_with_conflicting_definitions() {
    let mut g = TaskGraph::new(Mode::Light);
    let mut conflicting = spec("a", NodeKind::Explore);
    conflicting.content = "different task".to_string();
    let err = seed(&mut g, vec![spec("a", NodeKind::Explore), conflicting]).unwrap_err();
    assert_eq!(err, DagError::DuplicateNode("a".into()));
}

#[test]
fn seed_collapses_identical_duplicate_specs_in_one_batch() {
    let mut g = TaskGraph::new(Mode::Light);
    let repeated = spec("a", NodeKind::Explore).depends_on(["upstream", "upstream"]);
    seed(
        &mut g,
        vec![
            spec("upstream", NodeKind::Explore),
            repeated.clone(),
            repeated,
        ],
    )
    .unwrap();
    assert_eq!(g.nodes().iter().filter(|node| node.id == "a").count(), 1);
}

#[test]
fn seed_replay_is_idempotent_and_preserves_runtime_state() {
    let original = spec("a", NodeKind::Explore).depends_on(["upstream"]);
    let mut g = dag(
        Mode::Light,
        vec![spec("upstream", NodeKind::Explore), original.clone()],
    );
    dispatch(&mut g, "upstream", "worker");
    let before = g.clone();

    seed(&mut g, vec![original]).unwrap();

    assert_eq!(g, before, "an identical replay must be a complete no-op");
}

#[test]
fn seed_replay_accepts_dependency_order_and_duplicate_differences() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore),
            spec("b", NodeKind::Explore),
            spec("join", NodeKind::Synthesize).depends_on(["a", "b"]),
        ],
    );
    let before = g.clone();

    seed(
        &mut g,
        vec![spec("join", NodeKind::Synthesize).depends_on(["b", "a", "a"])],
    )
    .unwrap();

    assert_eq!(g, before);
}

#[test]
fn partial_seed_replay_adds_only_new_nodes() {
    let existing = spec("a", NodeKind::Explore);
    let mut g = dag(Mode::Deep, vec![existing.clone()]);

    seed(
        &mut g,
        vec![existing, spec("b", NodeKind::Implement).depends_on(["a"])],
    )
    .unwrap();

    assert_eq!(g.nodes().iter().filter(|node| node.id == "a").count(), 1);
    assert_eq!(g.nodes().iter().filter(|node| node.id == "b").count(), 1);
    let root_gate = g.get("plan::gate").expect("root gate");
    assert!(root_gate.depends_on.contains(&"a".to_string()));
    assert!(root_gate.depends_on.contains(&"b".to_string()));
}

#[test]
fn seed_replay_rejects_changed_existing_definition() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    let before = g.clone();
    let mut changed = spec("a", NodeKind::Explore);
    changed.priority = 7;

    let err = seed(&mut g, vec![changed]).unwrap_err();

    assert_eq!(err, DagError::DuplicateNode("a".into()));
    assert_eq!(g, before, "a conflict must not partially mutate the graph");
}

#[test]
fn seed_rejects_unknown_dependency() {
    let mut g = TaskGraph::new(Mode::Light);
    let err = seed(
        &mut g,
        vec![spec("a", NodeKind::Explore).depends_on(["ghost"])],
    )
    .unwrap_err();
    assert_eq!(
        err,
        DagError::UnknownDependency {
            node: "a".into(),
            dependency: "ghost".into()
        }
    );
}

#[test]
fn seed_rejects_cycle() {
    let mut g = TaskGraph::new(Mode::Light);
    let err = seed(
        &mut g,
        vec![
            spec("a", NodeKind::Explore).depends_on(["b"]),
            spec("b", NodeKind::Explore).depends_on(["a"]),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, DagError::WouldCreateCycle(_)));
}

// ----- scheduling / ready set -----

#[test]
fn ready_set_respects_dependencies_and_priority() {
    let g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore).priority(1),
            spec("b", NodeKind::Explore).priority(0),
            spec("c", NodeKind::Explore).depends_on(["a"]),
        ],
    );
    // a and b are ready (no deps); c is blocked on a. b sorts first (priority 0).
    let ready: Vec<&str> = ready_nodes(&g).iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ready, vec!["b", "a"]);
}

#[test]
fn dispatch_assigns_owner_and_blocks_dependents() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore),
            spec("b", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    assert!(dispatch(&mut g, "a", "w0"));
    assert_eq!(g.get("a").unwrap().owner.as_deref(), Some("w0"));
    assert_eq!(g.get("a").unwrap().status, NodeStatus::Running);
    // b is still blocked: a is not Done.
    assert!(ready_nodes(&g).iter().all(|n| n.id != "b"));
    // cannot dispatch b yet
    assert!(!dispatch(&mut g, "b", "w1"));
}

// ----- ownership enforcement -----

#[test]
fn complete_rejects_non_owner() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err = complete_node(&mut g, "a", "intruder", HandoffArtifact::brief("x")).unwrap_err();
    assert_eq!(
        err,
        DagError::NotOwner {
            node: "a".into(),
            actor: "intruder".into()
        }
    );
}

#[test]
fn expand_rejects_non_owner() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err = expand_node(
        &mut g,
        "a",
        "intruder",
        vec![spec("a.1", NodeKind::Explore)],
    )
    .unwrap_err();
    assert!(matches!(err, DagError::NotOwner { .. }));
}

// ----- dataflow on edges -----

#[test]
fn assembled_input_includes_upstream_artifacts() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore),
            spec("b", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    dispatch(&mut g, "a", "w0");
    let mut artifact = HandoffArtifact::brief("API lives in foo.rs");
    artifact.evidence = vec!["crates/foo/api.rs:12".into()];
    complete_node(&mut g, "a", "w0", artifact).unwrap();

    let input = assemble_input(&g, "b");
    assert!(input.contains("task b"));
    assert!(input.contains("API lives in foo.rs"));
    assert!(input.contains("crates/foo/api.rs:12"));
}

// ----- deep-mode artifact validation -----

#[test]
fn deep_mode_rejects_thin_artifact() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    // empty what_i_did_not_check is rejected
    let err = complete_node(&mut g, "a", "w0", HandoffArtifact::brief("found stuff")).unwrap_err();
    assert!(matches!(err, DagError::ThinArtifact { .. }));

    // a complete artifact passes
    assert!(complete_node(&mut g, "a", "w0", sim::deep_artifact("found stuff")).is_ok());
}

#[test]
fn light_mode_accepts_thin_artifact() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    assert!(complete_node(&mut g, "a", "w0", HandoffArtifact::brief("ok")).is_ok());
}

// ----- confidence: parsing, required rung, and the gate debt rule -----

#[test]
fn confidence_parse_is_lenient_and_pessimistic_on_hedges() {
    use ConfidenceLevel::*;
    // Word rungs with qualifiers.
    assert_eq!(ConfidenceLevel::parse("high"), Some(High));
    assert_eq!(ConfidenceLevel::parse("  Very High.  "), Some(High));
    assert_eq!(ConfidenceLevel::parse("medium"), Some(Medium));
    assert_eq!(ConfidenceLevel::parse("moderate"), Some(Medium));
    assert_eq!(ConfidenceLevel::parse("low"), Some(Low));
    // Hedges resolve pessimistically.
    assert_eq!(ConfidenceLevel::parse("low-to-high"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("medium-high"), Some(Medium));
    // Numeric scales: percent, 0-1, 0-10.
    assert_eq!(ConfidenceLevel::parse("90%"), Some(High));
    assert_eq!(ConfidenceLevel::parse("0.9"), Some(High));
    assert_eq!(ConfidenceLevel::parse("6/10"), Some(Medium));
    assert_eq!(ConfidenceLevel::parse("30"), Some(Low));
    // Garbage stays unparsed.
    assert_eq!(ConfidenceLevel::parse("banana"), None);
    assert_eq!(ConfidenceLevel::parse(""), None);
}

#[test]
fn deep_mode_requires_parseable_confidence() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");

    // Findings + wid-n-c but no confidence -> rejected.
    let mut artifact = HandoffArtifact::brief("found stuff");
    artifact.what_i_did_not_check = vec!["error paths".into()];
    let err = complete_node(&mut g, "a", "w0", artifact.clone()).unwrap_err();
    assert!(matches!(err, DagError::ThinArtifact { .. }));
    assert!(err.to_string().contains("confidence"));

    // Unparseable confidence -> rejected too.
    artifact.confidence = Some("banana".into());
    let err = complete_node(&mut g, "a", "w0", artifact.clone()).unwrap_err();
    assert!(matches!(err, DagError::ThinArtifact { .. }));

    // Honest low confidence is accepted (it routes work, not punishment).
    artifact.confidence = Some("low".into());
    assert!(complete_node(&mut g, "a", "w0", artifact).is_ok());
}

#[test]
fn light_mode_does_not_require_confidence() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    assert!(complete_node(&mut g, "a", "w0", HandoffArtifact::brief("ok")).is_ok());
}

/// The breadth mechanism: a deep gate cannot pass while a sibling completed at
/// low confidence unless the gate's artifact addresses it by id. inject_gap
/// remains the intended escape hatch and clears the debt by adding breadth.
#[test]
fn deep_gate_cannot_pass_over_unaddressed_low_confidence_sibling() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![
            spec("root.solid", NodeKind::Explore),
            spec("root.shaky", NodeKind::Explore),
        ],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    // solid finishes high, shaky finishes LOW.
    dispatch(&mut g, "root.solid", "w1");
    complete_node(&mut g, "root.solid", "w1", sim::deep_artifact("solid work")).unwrap();
    dispatch(&mut g, "root.shaky", "w2");
    let mut shaky = sim::deep_artifact("not sure about this");
    shaky.confidence = Some("low".into());
    complete_node(&mut g, "root.shaky", "w2", shaky).unwrap();

    // Gate tries to rubber-stamp without mentioning the shaky node -> rejected.
    dispatch(&mut g, &gate_id, "w3");
    let err = complete_node(
        &mut g,
        &gate_id,
        "w3",
        HandoffArtifact::brief("all good, no gaps"),
    )
    .unwrap_err();
    match &err {
        DagError::UnaddressedLowConfidence { gate, nodes } => {
            assert_eq!(gate, &gate_id);
            assert_eq!(nodes, &vec!["root.shaky".to_string()]);
        }
        other => panic!("expected UnaddressedLowConfidence, got {other:?}"),
    }
    assert!(err.to_string().contains("root.shaky"));
    assert!(err.to_string().contains("inject_gap"));

    // Path 1: the gate addresses the shaky node by id in its findings -> passes.
    let mut addressed = g.clone();
    assert!(
        complete_node(
            &mut addressed,
            &gate_id,
            "w3",
            HandoffArtifact::brief(
                "root.shaky's low confidence is acceptable: its scope was re-derived from \
                 root.solid's evidence and cross-checked"
            ),
        )
        .is_ok()
    );

    // Path 2: the gate injects follow-up breadth instead; after the gap drains
    // and the gate re-runs, the debt is cleared by addressing it.
    let injected = inject_from_gate(
        &mut g,
        &gate_id,
        "w3",
        vec![spec("root.shaky.recheck", NodeKind::Explore)],
    )
    .unwrap();
    assert_eq!(injected, vec!["root.shaky.recheck".to_string()]);
    dispatch(&mut g, "root.shaky.recheck", "w4");
    complete_node(
        &mut g,
        "root.shaky.recheck",
        "w4",
        sim::deep_artifact("re-checked the shaky scope thoroughly"),
    )
    .unwrap();
    dispatch(&mut g, &gate_id, "w3");
    assert!(
        complete_node(
            &mut g,
            &gate_id,
            "w3",
            HandoffArtifact::brief(
                "root.solid verified clean; root.shaky was shored up by root.shaky.recheck; \
                 no gaps remain"
            ),
        )
        .is_ok()
    );
}

#[test]
fn low_confidence_done_ids_reports_only_low_non_gate_nodes() {
    let mut g = dag(
        Mode::Deep,
        vec![spec("a", NodeKind::Explore), spec("b", NodeKind::Explore)],
    );
    dispatch(&mut g, "a", "w0");
    let mut low = sim::deep_artifact("shaky");
    low.confidence = Some("low".into());
    complete_node(&mut g, "a", "w0", low).unwrap();
    dispatch(&mut g, "b", "w1");
    complete_node(&mut g, "b", "w1", sim::deep_artifact("solid")).unwrap();

    assert_eq!(g.low_confidence_done_ids(), vec!["a".to_string()]);
}

// ----- composite expansion + gate insertion -----

#[test]
fn deep_expand_inserts_gate_between_children_and_synthesis() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![
            spec("root.1", NodeKind::Explore),
            spec("root.2", NodeKind::Explore),
        ],
    )
    .unwrap();

    // gate inserted, depends on both children
    let gate_id = outcome.gate_id.expect("deep mode inserts a gate");
    let gate = g.get(&gate_id).unwrap();
    assert!(gate.is_gate);
    assert_eq!(gate.kind, NodeKind::Critique);
    let mut gate_deps = gate.depends_on.clone();
    gate_deps.sort();
    assert_eq!(gate_deps, vec!["root.1", "root.2"]);

    // composite root now depends on the gate AND retains its child edges (so the
    // synthesis re-wake is hydrated with the children's artifacts) and is marked
    // expanded + re-queued.
    let root = g.get("root").unwrap();
    assert!(root.expanded);
    assert_eq!(root.status, NodeStatus::Queued);
    assert!(root.depends_on.contains(&gate_id));
    assert!(root.depends_on.contains(&"root.1".to_string()));
    assert!(root.depends_on.contains(&"root.2".to_string()));

    // root is NOT ready until children + gate complete.
    assert!(ready_nodes(&g).iter().all(|n| n.id != "root"));
}

#[test]
fn light_expand_inserts_no_gate() {
    let mut g = dag(Mode::Light, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    assert!(outcome.gate_id.is_none());
    assert!(g.gate_of("root").is_none());
}

#[test]
fn expand_rejecting_cycle_leaves_graph_unchanged() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("root", NodeKind::Explore),
            spec("other", NodeKind::Explore),
        ],
    );
    dispatch(&mut g, "root", "w0");
    let before = g.clone();
    // child depends on a node that depends back on the child => cycle once the
    // synthesis edge is added. Construct a direct child self-cycle.
    let err = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore).depends_on(["root"])],
    );
    // root.1 depends on root, and root (synthesis) depends on root.1 => cycle.
    assert!(matches!(err, Err(DagError::WouldCreateCycle(_))));
    assert_eq!(g, before, "failed expand must not mutate the graph");
}

#[test]
fn gate_injection_reblocks_composite_until_gap_drains() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    // Finish the single child so the gate becomes runnable.
    dispatch(&mut g, "root.1", "w0");
    complete_node(&mut g, "root.1", "w0", sim::deep_artifact("child done")).unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == gate_id));

    // Gate runs and finds a gap, injecting a new node and re-queuing itself.
    dispatch(&mut g, &gate_id, "w0");
    let gaps = inject_from_gate(
        &mut g,
        &gate_id,
        "w0",
        vec![NodeSpec::new("root.gap", "missed thing", NodeKind::Explore)],
    )
    .unwrap();
    assert_eq!(gaps, vec!["root.gap".to_string()]);

    // Gate is re-queued and now blocked on the gap; root (composite) still blocked.
    assert_eq!(g.get(&gate_id).unwrap().status, NodeStatus::Queued);
    assert!(
        g.get(&gate_id)
            .unwrap()
            .depends_on
            .contains(&"root.gap".to_string())
    );
    assert!(!ready_nodes(&g).iter().any(|n| n.id == "root"));
    // The gap node is the only newly-ready work.
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root.gap"));

    // Drain the gap, gate passes (accounting for everything it audited), root
    // finally closes.
    dispatch(&mut g, "root.gap", "w0");
    complete_node(&mut g, "root.gap", "w0", sim::deep_artifact("gap covered")).unwrap();
    dispatch(&mut g, &gate_id, "w0");
    complete_node(
        &mut g,
        &gate_id,
        "w0",
        HandoffArtifact::brief("audited root.1 and root.gap; no gaps remain"),
    )
    .unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
}

#[test]
fn inject_from_gate_rejects_non_gate_node() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err =
        inject_from_gate(&mut g, "a", "w0", vec![spec("a.gap", NodeKind::Explore)]).unwrap_err();
    assert!(matches!(err, DagError::GateMisuse(_)));
}

#[test]
fn expand_records_planner_and_frees_owner_for_rescheduling() {
    let mut g = dag(Mode::Light, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();

    let root = g.get("root").unwrap();
    // Owner is freed so the re-queued composite can be auto-scheduled, but the
    // planner is recorded for synthesis affinity.
    assert_eq!(root.owner, None);
    assert_eq!(root.planner.as_deref(), Some("w0"));
    assert!(root.expanded);

    // Once the child completes, the composite is runnable again (no owner gate).
    dispatch(&mut g, "root.1", "w0");
    complete_node(&mut g, "root.1", "w0", HandoffArtifact::brief("done")).unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
}

// ----- full simulator: explore-then-act with gate-spawned gap -----

#[test]
fn simulator_runs_deep_graph_with_composite_and_gap_to_completion() {
    let mut g = dag(
        Mode::Deep,
        vec![
            spec("explore", NodeKind::Explore),
            spec("synth", NodeKind::Synthesize).depends_on(["explore"]),
        ],
    );

    // Scripted behavior:
    //  - "explore" decomposes once into two facets.
    //  - facet "explore.hot" decomposes once into a sub-child, then synthesizes.
    //  - the critique gate "explore::gate" spawns one gap node the first time.
    //  - everything else just completes.
    let mut expanded: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut gate_fired: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut worker = move |id: &str, kind: NodeKind, input: &str| -> WorkerAction {
        // Gate nodes: first time, find a gap and inject a node; second time, pass
        // with an artifact that accounts for every audited node (coverage rule).
        if kind == NodeKind::Critique || kind == NodeKind::Verify {
            if id == "explore::gate" && gate_fired.insert(id.to_string()) {
                return WorkerAction::InjectGap(vec![NodeSpec::new(
                    "explore.gap",
                    "cover the missed facet",
                    NodeKind::Explore,
                )]);
            }
            return WorkerAction::Complete(sim::gate_pass_artifact(input));
        }

        match id {
            "explore" if expanded.insert(id.to_string()) => WorkerAction::Expand(vec![
                spec("explore.geo", NodeKind::Explore),
                spec("explore.hot", NodeKind::Explore),
            ]),
            "explore.hot" if expanded.insert(id.to_string()) => {
                WorkerAction::Expand(vec![spec("explore.hot.udev", NodeKind::Explore)])
            }
            _ => WorkerAction::Complete(sim::deep_artifact(&format!("did {id}"))),
        }
    };

    let report = sim::run(&mut g, 8, 200, &mut worker).unwrap();

    assert!(!report.stalled, "graph should not stall: {report:?}");
    assert_eq!(report.failed, 0);
    assert!(g.all_terminal());

    // The gate-spawned gap node must exist and be done (comprehensiveness gate
    // converted a miss into graph).
    assert!(
        g.get("explore.gap").is_some(),
        "gate should have spawned a gap node"
    );
    assert!(g.get("explore.gap").unwrap().is_done());

    // The composite nodes are expanded and completed via synthesis.
    assert!(g.get("explore").unwrap().expanded);
    assert!(g.get("explore").unwrap().is_done());
    assert!(g.get("explore.hot").unwrap().expanded);
    assert!(g.get("explore.hot").unwrap().is_done());

    // Downstream synthesis ran after explore completed.
    assert!(g.get("synth").unwrap().is_done());

    // The auto-inserted root gate audited the whole plan before it finished.
    let root_gate = g
        .nodes()
        .iter()
        .find(|n| n.is_gate && n.parent.is_none())
        .expect("deep seed must insert a root gate");
    assert!(root_gate.is_done());
}

#[test]
fn simulator_light_mode_flat_fanout_completes_fast() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Implement),
            spec("b", NodeKind::Implement),
            spec("c", NodeKind::Implement),
            spec("merge", NodeKind::Synthesize).depends_on(["a", "b", "c"]),
        ],
    );
    let mut worker =
        |id: &str, _k: NodeKind, _i: &str| WorkerAction::Complete(HandoffArtifact::brief(id));
    let report = sim::run(&mut g, 4, 50, &mut worker).unwrap();
    assert!(!report.stalled);
    assert_eq!(report.failed, 0);
    assert_eq!(report.completed, 4);
    assert!(g.all_terminal());
}

#[test]
fn simulator_stalls_when_failed_node_blocks_dependents() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Implement),
            spec("b", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    let mut worker = |id: &str, _k: NodeKind, _i: &str| {
        if id == "a" {
            WorkerAction::Fail
        } else {
            WorkerAction::Complete(HandoffArtifact::brief(id))
        }
    };
    let report = sim::run(&mut g, 2, 50, &mut worker).unwrap();
    assert!(
        report.stalled,
        "a failed dependency must stall its dependent"
    );
    assert_eq!(report.failed, 1);
    assert!(!g.get("b").unwrap().is_terminal());
}

// ----- dataflow surfaces every artifact field (the critique gate cheat code) -----

#[test]
fn assembled_input_surfaces_what_i_did_not_check_for_gate() {
    // A deep critique gate is told to read each child's `what_i_did_not_check`.
    // It can only do that if hydration actually forwards that field.
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    dispatch(&mut g, "root.1", "w0");
    let mut artifact = HandoffArtifact::brief("explored the easy path");
    artifact.edge_cases_considered = vec!["empty input".into()];
    artifact.what_i_did_not_check = vec!["the concurrent hotplug path".into()];
    artifact.confidence = Some("medium".into());
    complete_node(&mut g, "root.1", "w0", artifact).unwrap();

    let gate_input = assemble_input(&g, &gate_id);
    assert!(
        gate_input.contains("the concurrent hotplug path"),
        "gate must see what_i_did_not_check: {gate_input}"
    );
    assert!(
        gate_input.contains("empty input"),
        "gate must see edge_cases_considered: {gate_input}"
    );
    assert!(
        gate_input.contains("medium"),
        "gate must see confidence: {gate_input}"
    );
}

#[test]
fn composite_synthesis_rewake_is_hydrated_with_child_artifacts() {
    // The map-reduce synthesis re-wake must receive its children's findings, not
    // just a thin "gate passed" token (doc section 5). The composite retains its
    // child edges precisely so direct-dependency hydration covers the children.
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    dispatch(&mut g, "root.1", "w0");
    complete_node(
        &mut g,
        "root.1",
        "w0",
        sim::deep_artifact("child found the answer in foo.rs"),
    )
    .unwrap();
    dispatch(&mut g, &gate_id, "w0");
    complete_node(
        &mut g,
        &gate_id,
        "w0",
        HandoffArtifact::brief("audited root.1; gate passed"),
    )
    .unwrap();

    // root is now runnable; its assembled synthesis input must include the child.
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
    let synth_input = assemble_input(&g, "root");
    assert!(
        synth_input.contains("child found the answer in foo.rs"),
        "synthesis re-wake must be hydrated with child artifacts: {synth_input}"
    );
}

// ----- gate id never collides with a user-seeded node id -----

#[test]
fn expand_gate_id_avoids_collision_with_seeded_node() {
    // A user seeds a node whose id is exactly the natural gate id. The auto gate
    // must pick a non-colliding id so id-based lookups are never corrupted.
    let mut g = dag(
        Mode::Deep,
        vec![
            spec("root", NodeKind::Explore),
            spec("root::gate", NodeKind::Explore),
        ],
    );
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    assert_ne!(
        gate_id, "root::gate",
        "gate id must not collide with the seeded node"
    );
    assert!(g.get(&gate_id).unwrap().is_gate);
    // The pre-existing user node is still a non-gate node, intact.
    assert!(!g.get("root::gate").unwrap().is_gate);
    // No duplicate ids in the graph.
    let mut ids: Vec<&str> = g.nodes().iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "graph must not contain duplicate ids");
}

// ----- audit regression tests (2026-07-01 deep swarm audit) -----

#[test]
fn seed_accepts_duplicate_dependencies_without_false_cycle() {
    // A repeated dep in agent JSON must not be misread as a cycle: indegree used
    // to count occurrences while relaxation decremented unique pairs.
    let mut g = TaskGraph::new(Mode::Deep);
    let mut a = spec("a", NodeKind::Explore);
    a.depends_on = vec!["b".into(), "b".into()];
    seed(&mut g, vec![a, spec("b", NodeKind::Explore)]).expect("duplicate deps are not a cycle");
    // Deps are deduped on insertion.
    assert_eq!(g.get("a").unwrap().depends_on, vec!["b".to_string()]);
    // And the graph drains normally.
    dispatch(&mut g, "b", "w0");
    complete_node(&mut g, "b", "w0", sim::deep_artifact("b done")).unwrap();
    assert_eq!(ready_nodes(&g).len(), 1);
}

#[test]
fn seed_rejects_blank_ids() {
    let mut g = TaskGraph::new(Mode::Light);
    let blank = NodeSpec::new("", "task", NodeKind::Explore);
    let err = seed(&mut g, vec![blank]).unwrap_err();
    assert!(matches!(err, DagError::GateMisuse(_)));
    let mut ws = TaskGraph::new(Mode::Light);
    let white = NodeSpec::new("   ", "task", NodeKind::Explore);
    assert!(seed(&mut ws, vec![white]).is_err());
}

#[test]
fn confidence_parse_handles_fractions_and_negation() {
    use ConfidenceLevel::*;
    // Fractional scores: honor the explicit denominator.
    assert_eq!(ConfidenceLevel::parse("1/10"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("9/10"), Some(High));
    assert_eq!(ConfidenceLevel::parse("1 out of 10"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("3 of 5"), Some(Medium));
    // Negations must not resolve to the positive rung they contain.
    assert_eq!(ConfidenceLevel::parse("not high"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("not confident"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("uncertain"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("unsure"), Some(Low));
    // Bare small integer reads on the 0-10 scale.
    assert_eq!(ConfidenceLevel::parse("1"), Some(Low));
    assert_eq!(ConfidenceLevel::parse("8"), Some(High));
}

#[test]
fn artifact_accepts_numeric_confidence_json() {
    // Agents emit {"confidence": 0.8}; the deserializer must coerce, not reject.
    let artifact: HandoffArtifact =
        serde_json::from_str(r#"{"findings":"x","confidence":0.8,"what_i_did_not_check":["y"]}"#)
            .expect("numeric confidence should deserialize");
    assert_eq!(artifact.confidence_level(), Some(ConfidenceLevel::High));
    let artifact: HandoffArtifact =
        serde_json::from_str(r#"{"findings":"x","confidence":"low"}"#).unwrap();
    assert_eq!(artifact.confidence_level(), Some(ConfidenceLevel::Low));
}

#[test]
fn gate_debt_requires_token_level_mention_not_substring() {
    // A gate must not be able to clear a debt on child "a" just because its
    // findings contain the letter 'a' somewhere.
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("a", NodeKind::Explore), spec("b", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    dispatch(&mut g, "a", "w1");
    let mut shaky = sim::deep_artifact("shaky");
    shaky.confidence = Some("low".into());
    complete_node(&mut g, "a", "w1", shaky).unwrap();
    dispatch(&mut g, "b", "w2");
    complete_node(&mut g, "b", "w2", sim::deep_artifact("solid")).unwrap();

    dispatch(&mut g, &gate_id, "w3");
    // "all good" contains 'a' as a substring but never mentions node `a`.
    let err = complete_node(
        &mut g,
        &gate_id,
        "w3",
        HandoffArtifact::brief("all good, no gaps remain"),
    )
    .unwrap_err();
    assert!(matches!(err, DagError::UnaddressedLowConfidence { .. }));

    // A token-level mention passes (naming node b too: coverage accounting).
    assert!(
        complete_node(
            &mut g,
            &gate_id,
            "w3",
            HandoffArtifact::brief(
                "child a is low confidence but acceptable: scope re-checked; b verified solid"
            ),
        )
        .is_ok()
    );
}

#[test]
fn gate_debt_cannot_be_cleared_via_gates_own_what_i_did_not_check() {
    // Declaring "I did not check X" is the opposite of addressing X.
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.shaky", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    dispatch(&mut g, "root.shaky", "w1");
    let mut shaky = sim::deep_artifact("unsure work");
    shaky.confidence = Some("low".into());
    complete_node(&mut g, "root.shaky", "w1", shaky).unwrap();

    dispatch(&mut g, &gate_id, "w2");
    let mut evasive = HandoffArtifact::brief("looks fine overall");
    evasive.what_i_did_not_check = vec!["root.shaky".into()];
    let err = complete_node(&mut g, &gate_id, "w2", evasive).unwrap_err();
    assert!(matches!(err, DagError::UnaddressedLowConfidence { .. }));
}

#[test]
fn inject_from_gate_wires_gap_artifacts_into_parent_synthesis() {
    // The synthesis re-wake must receive the gap node's artifact, not just the
    // original children's (forward dataflow reads direct deps only).
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome =
        expand_node(&mut g, "root", "w0", vec![spec("child", NodeKind::Explore)]).unwrap();
    let gate_id = outcome.gate_id.unwrap();
    dispatch(&mut g, "child", "w1");
    complete_node(&mut g, "child", "w1", sim::deep_artifact("child findings")).unwrap();

    dispatch(&mut g, &gate_id, "w2");
    inject_from_gate(
        &mut g,
        &gate_id,
        "w2",
        vec![spec("gapnode", NodeKind::Explore)],
    )
    .unwrap();
    dispatch(&mut g, "gapnode", "w3");
    complete_node(
        &mut g,
        "gapnode",
        "w3",
        sim::deep_artifact("gap findings: the missing corner"),
    )
    .unwrap();
    dispatch(&mut g, &gate_id, "w2");
    complete_node(
        &mut g,
        &gate_id,
        "w2",
        HandoffArtifact::brief("child audited; gapnode drained; no gaps remain"),
    )
    .unwrap();

    // Synthesis re-wake input must include BOTH child and gap artifacts.
    let input = assemble_input(&g, "root");
    assert!(
        input.contains("child findings"),
        "missing child artifact: {input}"
    );
    assert!(
        input.contains("gap findings: the missing corner"),
        "synthesis must be hydrated with injected gap artifacts: {input}"
    );
}

#[test]
fn requeue_failed_unwedges_a_failed_gate() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(&mut g, "root", "w0", vec![spec("c", NodeKind::Explore)]).unwrap();
    let gate_id = outcome.gate_id.unwrap();
    dispatch(&mut g, "c", "w1");
    complete_node(&mut g, "c", "w1", sim::deep_artifact("done")).unwrap();

    // Gate worker crashes: the gate fails, wedging the composite.
    dispatch(&mut g, &gate_id, "w2");
    fail_node(&mut g, &gate_id, "w2").unwrap();
    assert!(
        ready_nodes(&g).is_empty(),
        "composite is wedged behind failed gate"
    );

    // requeue_failed is the recovery primitive.
    requeue_failed(&mut g, &gate_id).unwrap();
    let ready: Vec<&str> = ready_nodes(&g).iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ready, vec![gate_id.as_str()]);
    // Only failed nodes can be requeued.
    assert!(requeue_failed(&mut g, "c").is_err());
}

// ----- unbounded-growth mechanics (root gate, coverage debt, stale scope) -----

/// Every deep seed gets a plan-wide root gate: a flat plan whose nodes all
/// execute atomically still cannot finish without a final adversarial audit.
#[test]
fn deep_seed_inserts_root_gate_over_all_roots() {
    let g = dag(
        Mode::Deep,
        vec![
            spec("a", NodeKind::Explore),
            spec("b", NodeKind::Explore),
            spec("c", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    let root_gate = g
        .nodes()
        .iter()
        .find(|n| n.is_gate && n.parent.is_none())
        .expect("deep seed must insert a root gate");
    assert_eq!(root_gate.id, "plan::gate");
    assert_eq!(root_gate.kind, NodeKind::Critique);
    assert_eq!(root_gate.origin, Some(NodeOrigin::Gate));
    for id in ["a", "b", "c"] {
        assert!(
            root_gate.depends_on.contains(&id.to_string()),
            "root gate must audit '{id}': {:?}",
            root_gate.depends_on
        );
    }
    // Non-terminal until the audit passes even after all work completes.
    assert!(!g.all_terminal());
}

#[test]
fn deep_seed_of_pure_code_plan_gets_verify_root_gate() {
    let g = dag(
        Mode::Deep,
        vec![
            spec("impl1", NodeKind::Implement),
            spec("fix1", NodeKind::Fix),
        ],
    );
    let root_gate = g
        .nodes()
        .iter()
        .find(|n| n.is_gate && n.parent.is_none())
        .expect("root gate");
    assert_eq!(root_gate.kind, NodeKind::Verify);
}

#[test]
fn light_seed_inserts_no_root_gate() {
    let g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    assert!(g.nodes().iter().all(|n| !n.is_gate));
}

/// Re-seeding widens the root gate's audit scope, and re-opens it if it had
/// already passed: new work re-opens the plan-wide audit.
#[test]
fn reseed_widens_and_reopens_root_gate() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    complete_node(&mut g, "a", "w0", sim::deep_artifact("did a")).unwrap();
    dispatch(&mut g, "plan::gate", "w1");
    complete_node(
        &mut g,
        "plan::gate",
        "w1",
        HandoffArtifact::brief("audited a; clean"),
    )
    .unwrap();
    assert!(g.all_terminal());

    // New root work arrives: the passed root gate must re-open and audit it.
    seed(&mut g, vec![spec("b", NodeKind::Explore)]).unwrap();
    let root_gate = g.get("plan::gate").unwrap();
    assert_eq!(root_gate.status, NodeStatus::Queued);
    assert!(root_gate.depends_on.contains(&"b".to_string()));
    assert!(!g.all_terminal());

    // Drain the new node; the gate re-runs and must account for BOTH nodes.
    dispatch(&mut g, "b", "w0");
    complete_node(&mut g, "b", "w0", sim::deep_artifact("did b")).unwrap();
    dispatch(&mut g, "plan::gate", "w1");
    let err = complete_node(
        &mut g,
        "plan::gate",
        "w1",
        HandoffArtifact::brief("audited b; clean"),
    )
    .unwrap_err();
    assert!(matches!(err, DagError::UncoveredSiblings { .. }));
    complete_node(
        &mut g,
        "plan::gate",
        "w1",
        HandoffArtifact::brief("re-audited a and b; clean"),
    )
    .unwrap();
    assert!(g.all_terminal());
}

/// The coverage-debt rule: a passing gate must account for EVERY done node in
/// its audit scope by id, not just the low-confidence ones. "All good, no
/// gaps" is structurally rejected.
#[test]
fn gate_pass_requires_enumerated_coverage_of_all_siblings() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![
            spec("root.x", NodeKind::Explore),
            spec("root.y", NodeKind::Explore),
        ],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    dispatch(&mut g, "root.x", "w1");
    complete_node(&mut g, "root.x", "w1", sim::deep_artifact("solid x")).unwrap();
    dispatch(&mut g, "root.y", "w2");
    complete_node(&mut g, "root.y", "w2", sim::deep_artifact("solid y")).unwrap();

    // Both children are HIGH confidence, yet a rubber stamp still fails.
    dispatch(&mut g, &gate_id, "w3");
    let err = complete_node(
        &mut g,
        &gate_id,
        "w3",
        HandoffArtifact::brief("all good, no gaps"),
    )
    .unwrap_err();
    match &err {
        DagError::UncoveredSiblings { gate, nodes } => {
            assert_eq!(gate, &gate_id);
            assert_eq!(
                nodes,
                &vec!["root.x".to_string(), "root.y".to_string()],
                "every unaddressed sibling is listed"
            );
        }
        other => panic!("expected UncoveredSiblings, got {other:?}"),
    }
    // Naming only one still fails, listing the remaining debt.
    let err = complete_node(
        &mut g,
        &gate_id,
        "w3",
        HandoffArtifact::brief("root.x verified clean"),
    )
    .unwrap_err();
    assert!(
        matches!(err, DagError::UncoveredSiblings { ref nodes, .. } if nodes == &vec!["root.y".to_string()])
    );
    // Accounting via open_questions also counts as addressing.
    let mut pass = HandoffArtifact::brief("root.x verified clean; no gaps remain");
    pass.open_questions = vec!["root.y: minor doubt about edge ordering, acceptable".into()];
    complete_node(&mut g, &gate_id, "w3", pass).unwrap();
}

/// Above the enumeration cap the per-id coverage rule relaxes for
/// HIGH-confidence nodes (an artifact naming 30+ ids degenerates into a list),
/// but low-confidence debts still bind at any width.
#[test]
fn gate_coverage_enumeration_relaxes_above_cap_but_confidence_debt_remains() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let children: Vec<NodeSpec> = (0..GATE_COVERAGE_ENUMERATION_CAP + 1)
        .map(|i| spec(&format!("root.c{i}"), NodeKind::Explore))
        .collect();
    let outcome = expand_node(&mut g, "root", "w0", children).unwrap();
    let gate_id = outcome.gate_id.unwrap();
    for i in 0..GATE_COVERAGE_ENUMERATION_CAP + 1 {
        let id = format!("root.c{i}");
        dispatch(&mut g, &id, "w1");
        let mut artifact = sim::deep_artifact(&format!("did {id}"));
        if i == 3 {
            artifact.confidence = Some("low".into());
        }
        complete_node(&mut g, &id, "w1", artifact).unwrap();
    }
    dispatch(&mut g, &gate_id, "w2");
    // Wide scope: full enumeration not required, but the LOW node still is.
    let err = complete_node(
        &mut g,
        &gate_id,
        "w2",
        HandoffArtifact::brief("sampled the set; looks clean"),
    )
    .unwrap_err();
    assert!(
        matches!(err, DagError::UnaddressedLowConfidence { ref nodes, .. } if nodes == &vec!["root.c3".to_string()])
    );
    complete_node(
        &mut g,
        &gate_id,
        "w2",
        HandoffArtifact::brief(
            "sampled the set; root.c3's low confidence acceptable after cross-check",
        ),
    )
    .unwrap();
}

/// Above the enumeration cap, only HIGH-confidence nodes escape per-id
/// accounting: a node that self-reported medium confidence must still be
/// addressed, so gate rigor does not silently collapse on wide scopes.
#[test]
fn gate_coverage_above_cap_still_binds_non_high_confidence_nodes() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let children: Vec<NodeSpec> = (0..GATE_COVERAGE_ENUMERATION_CAP + 1)
        .map(|i| spec(&format!("root.c{i}"), NodeKind::Explore))
        .collect();
    let outcome = expand_node(&mut g, "root", "w0", children).unwrap();
    let gate_id = outcome.gate_id.unwrap();
    for i in 0..GATE_COVERAGE_ENUMERATION_CAP + 1 {
        let id = format!("root.c{i}");
        dispatch(&mut g, &id, "w1");
        let mut artifact = sim::deep_artifact(&format!("did {id}"));
        if i == 5 {
            artifact.confidence = Some("medium".into());
        }
        complete_node(&mut g, &id, "w1", artifact).unwrap();
    }
    dispatch(&mut g, &gate_id, "w2");
    // Wide scope: HIGH nodes need not be enumerated, but the MEDIUM node must
    // still be addressed by id.
    let err = complete_node(
        &mut g,
        &gate_id,
        "w2",
        HandoffArtifact::brief("sampled the set; looks clean"),
    )
    .unwrap_err();
    assert!(
        matches!(err, DagError::UncoveredSiblings { ref nodes, .. } if nodes == &vec!["root.c5".to_string()])
    );
    complete_node(
        &mut g,
        &gate_id,
        "w2",
        HandoffArtifact::brief("spot-checked root.c5's medium-confidence findings; they hold up"),
    )
    .unwrap();
}

/// A gate cannot pass while nodes entered its audit scope after dispatch and
/// are unfinished (stale view), e.g. a re-seed widened the root gate while it
/// was running.
#[test]
fn running_gate_cannot_pass_over_scope_added_after_dispatch() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    complete_node(&mut g, "a", "w0", sim::deep_artifact("did a")).unwrap();
    dispatch(&mut g, "plan::gate", "w1");
    // While the root gate runs, a re-seed adds node "b" to its scope.
    seed(&mut g, vec![spec("b", NodeKind::Explore)]).unwrap();
    let err = complete_node(
        &mut g,
        "plan::gate",
        "w1",
        HandoffArtifact::brief("audited a; clean"),
    )
    .unwrap_err();
    assert!(
        matches!(err, DagError::StaleGateScope { ref pending, .. } if pending == &vec!["b".to_string()]),
        "got: {err:?}"
    );
}

/// Origins are stamped by every op: seed/expand/gap/gate.
#[test]
fn node_origins_are_recorded_per_op() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    assert_eq!(g.get("root").unwrap().origin, Some(NodeOrigin::Seed));
    assert_eq!(g.get("plan::gate").unwrap().origin, Some(NodeOrigin::Gate));

    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    assert_eq!(g.get("root.1").unwrap().origin, Some(NodeOrigin::Expand));
    assert_eq!(g.get(&gate_id).unwrap().origin, Some(NodeOrigin::Gate));

    dispatch(&mut g, "root.1", "w1");
    complete_node(&mut g, "root.1", "w1", sim::deep_artifact("done")).unwrap();
    dispatch(&mut g, &gate_id, "w2");
    inject_from_gate(
        &mut g,
        &gate_id,
        "w2",
        vec![spec("root.gap", NodeKind::Explore)],
    )
    .unwrap();
    assert_eq!(g.get("root.gap").unwrap().origin, Some(NodeOrigin::Gap));
}

/// mentions_node_id treats trailing sentence punctuation as a boundary, so
/// "audited explore.hot.udev." addresses `explore.hot.udev` but not a longer
/// id like `explore.hot.udev.x`.
#[test]
fn gate_pass_accepts_id_followed_by_sentence_punctuation() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.only", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    dispatch(&mut g, "root.only", "w1");
    complete_node(&mut g, "root.only", "w1", sim::deep_artifact("done")).unwrap();
    dispatch(&mut g, &gate_id, "w2");
    complete_node(
        &mut g,
        &gate_id,
        "w2",
        HandoffArtifact::brief("I audited root.only. No gaps remain"),
    )
    .unwrap();
}

/// End-to-end: a flat deep seed with lazy atomic workers STILL ends in a
/// root-gate audit that spawns growth before the plan can finish.
#[test]
fn simulator_flat_deep_seed_grows_via_root_gate() {
    let mut g = dag(
        Mode::Deep,
        vec![spec("t1", NodeKind::Explore), spec("t2", NodeKind::Explore)],
    );
    let mut gate_fired = false;
    let mut worker = move |id: &str, kind: NodeKind, input: &str| -> WorkerAction {
        if kind == NodeKind::Critique || kind == NodeKind::Verify {
            if !gate_fired {
                gate_fired = true;
                return WorkerAction::InjectGap(vec![NodeSpec::new(
                    "t3.missed",
                    "the facet the flat seed never covered",
                    NodeKind::Explore,
                )]);
            }
            return WorkerAction::Complete(sim::gate_pass_artifact(input));
        }
        WorkerAction::Complete(sim::deep_artifact(&format!("did {id}")))
    };
    let report = sim::run(&mut g, 8, 100, &mut worker).unwrap();
    assert!(!report.stalled, "flat deep plan must not stall: {report:?}");
    assert!(g.all_terminal());
    // The audit grew the plan beyond its seed.
    let gap = g
        .get("t3.missed")
        .expect("root gate must have grown the plan");
    assert!(gap.is_done());
    assert_eq!(gap.origin, Some(NodeOrigin::Gap));
    assert!(g.get("plan::gate").unwrap().is_done());
}
