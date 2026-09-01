//! Deterministic task-DAG simulator.
//!
//! This drives the engine end-to-end with scripted mock workers so the scheduler,
//! ops, dataflow, and gate mechanics can be verified without any live agents. It
//! is the executable analogue of the worked example in `docs/SWARM_TASK_GRAPH.md`
//! section 9.
//!
//! A worker is a closure that, given the assembled input for a node, returns a
//! [`WorkerAction`]. The driver loops: dispatch all ready nodes round-robin to a
//! bounded worker pool, run each one step, apply the resulting mutation, and
//! repeat until the graph is fully terminal or it stalls.

use super::{
    DagError, HandoffArtifact, Mode, NodeKind, NodeSpec, TaskGraph, complete_node, dispatch,
    expand_node, fail_node, inject_from_gate, ready_nodes,
};

/// What a mock worker decides to do with the node it was handed.
#[derive(Debug, Clone)]
pub enum WorkerAction {
    /// Execute the node directly and complete it with this artifact.
    Complete(HandoffArtifact),
    /// Decompose the node into these children (composite path).
    Expand(Vec<NodeSpec>),
    /// Gate found a problem: inject these gap/fix nodes and re-queue the gate.
    /// Only valid when the dispatched node is a gate.
    InjectGap(Vec<NodeSpec>),
    /// Fail the node.
    Fail,
}

/// A scripted worker. Receives the node id, kind, and assembled input; returns an
/// action. The closure may capture mutable state (e.g. to expand only once).
pub type Worker<'a> = dyn FnMut(&str, NodeKind, &str) -> WorkerAction + 'a;

/// Outcome of a simulation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimReport {
    pub steps: usize,
    pub completed: usize,
    pub failed: usize,
    pub stalled: bool,
}

/// Run the simulation to completion (or stall). `max_workers` bounds how many
/// nodes run concurrently per step; `max_steps` guards against runaway loops.
pub fn run(
    graph: &mut TaskGraph,
    max_workers: usize,
    max_steps: usize,
    worker: &mut Worker<'_>,
) -> Result<SimReport, DagError> {
    let mut steps = 0usize;
    loop {
        if graph.all_terminal() {
            break;
        }
        if steps >= max_steps {
            return Ok(report(graph, steps, true));
        }

        // Dispatch up to `max_workers` ready nodes this step. We collect ids first
        // to avoid borrowing the graph while mutating it.
        let ready: Vec<(String, NodeKind)> = ready_nodes(graph)
            .into_iter()
            .take(max_workers)
            .map(|node| (node.id.clone(), node.kind))
            .collect();

        if ready.is_empty() {
            // Nothing runnable and not all terminal => stall (e.g. a Failed node
            // blocking its dependents with no fix path).
            return Ok(report(graph, steps, true));
        }

        for (idx, (node_id, kind)) in ready.into_iter().enumerate() {
            let worker_name = format!("w{}", idx % max_workers);
            if !dispatch(graph, &node_id, &worker_name) {
                continue;
            }
            let input = super::assemble_input(graph, &node_id);
            let action = worker(&node_id, kind, &input);
            match action {
                WorkerAction::Complete(artifact) => {
                    complete_node(graph, &node_id, &worker_name, artifact)?;
                }
                WorkerAction::Expand(children) => {
                    expand_node(graph, &node_id, &worker_name, children)?;
                }
                WorkerAction::InjectGap(new_nodes) => {
                    inject_from_gate(graph, &node_id, &worker_name, new_nodes)?;
                }
                WorkerAction::Fail => {
                    fail_node(graph, &node_id, &worker_name)?;
                }
            }
            steps += 1;
        }
    }
    Ok(report(graph, steps, false))
}

fn report(graph: &TaskGraph, steps: usize, stalled: bool) -> SimReport {
    let completed = graph.nodes().iter().filter(|node| node.is_done()).count();
    let failed = graph
        .nodes()
        .iter()
        .filter(|node| matches!(node.status, super::NodeStatus::Failed))
        .count();
    SimReport {
        steps,
        completed,
        failed,
        stalled,
    }
}

/// Convenience: a deep-mode artifact that satisfies validation, for tests/sims.
pub fn deep_artifact(findings: &str) -> HandoffArtifact {
    HandoffArtifact {
        findings: findings.to_string(),
        what_i_did_not_check: vec!["nothing material; covered the stated scope".to_string()],
        confidence: Some("high".to_string()),
        ..HandoffArtifact::default()
    }
}

/// Convenience: a passing gate artifact that satisfies the deep-mode coverage
/// rule by naming every audited node found in the gate's assembled input.
///
/// The scheduler hydrates a gate's input with one `## <id> (<kind>)` section per
/// done dependency, and a gate's dependencies are exactly its audit scope, so
/// scraping those headers enumerates the scope without needing the graph. This
/// is what a real gate is instructed to do: account for each id it audited.
pub fn gate_pass_artifact(input: &str) -> HandoffArtifact {
    let audited: Vec<&str> = input
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .filter_map(|rest| rest.split(" (").next())
        .collect();
    let findings = if audited.is_empty() {
        "gate passed: no audited nodes in scope".to_string()
    } else {
        format!(
            "gate passed; audited each node: {}. No gaps remain.",
            audited.join(", ")
        )
    };
    HandoffArtifact::brief(findings)
}

/// Convenience: build a graph in a mode for sims/tests.
pub fn graph(mode: Mode) -> TaskGraph {
    TaskGraph::new(mode)
}
