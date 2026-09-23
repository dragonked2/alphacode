//! Scheduler: ready-set computation, dispatch, and dataflow hydration.
//!
//! The scheduler walks the DAG. A node becomes runnable when all its dependencies
//! are `Done`. On dispatch it is assigned to a worker (ownership) and its input is
//! hydrated from the merged artifacts of its upstream dependencies, which is the
//! forward dataflow along edges (doc section 5).

use super::{NodeStatus, TaskGraph, TaskNode};

/// Suggested default worker ceiling for light mode (doc section 1a). Deep mode is
/// bounded by the swarm-level `MAX_SWARM_MEMBERS` cap instead.
pub const LIGHT_MODE_SUGGESTED_WORKERS: usize = 32;

/// Whether a node has reached a terminal status.
pub fn is_terminal(node: &TaskNode) -> bool {
    node.is_terminal()
}

/// The set of nodes that are runnable right now: queued, with every dependency
/// `Done`. Returned in scheduling order (priority asc, then id) for determinism.
pub fn ready_nodes(graph: &TaskGraph) -> Vec<&TaskNode> {
    let mut ready: Vec<&TaskNode> = graph
        .nodes()
        .iter()
        .filter(|node| node.status == NodeStatus::Queued && deps_satisfied(graph, node))
        .collect();
    ready.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    ready
}

fn deps_satisfied(graph: &TaskGraph, node: &TaskNode) -> bool {
    node.depends_on.iter().all(|dep| {
        graph
            .get(dep)
            .map(TaskNode::is_done)
            // A dependency that does not exist is treated as unsatisfiable; this
            // should never happen because edges are validated on insertion.
            .unwrap_or(false)
    })
}

/// Dispatch a ready node to `worker`: assign ownership and flip it to `Running`.
/// Returns false if the node is not currently dispatchable.
pub fn dispatch(graph: &mut TaskGraph, node_id: &str, worker: &str) -> bool {
    let dispatchable = graph
        .get(node_id)
        .map(|node| node.status == NodeStatus::Queued && deps_satisfied(graph, node))
        .unwrap_or(false);
    if !dispatchable {
        return false;
    }
    // `dispatchable` proved the node exists under this same borrow of `graph`.
    let Some(node) = graph.get_mut(node_id) else {
        return false;
    };
    node.owner = Some(worker.to_string());
    node.status = NodeStatus::Running;
    true
}

/// Assemble the worker input for a node: its own prompt plus the merged handoff
/// artifacts of all its upstream dependencies. Artifacts are passed by reference
/// (findings + evidence), keeping context small (doc section 5).
pub fn assemble_input(graph: &TaskGraph, node_id: &str) -> String {
    let Some(node) = graph.get(node_id) else {
        return String::new();
    };
    let mut out = String::new();
    out.push_str(&node.content);

    let upstream: Vec<&TaskNode> = node
        .depends_on
        .iter()
        .filter_map(|dep| graph.get(dep))
        .filter(|dep| dep.is_done())
        .collect();

    if upstream.is_empty() {
        return out;
    }

    out.push_str("\n\n# Inputs from completed dependencies\n");
    for dep in upstream {
        out.push('\n');
        if let Some(artifact) = &dep.output {
            out.push_str(&artifact.render_section(&dep.id, kind_label(dep.kind)));
        } else {
            out.push_str(&format!("## {} ({})\n", dep.id, kind_label(dep.kind)));
        }
    }
    out
}

/// Lowercase label for a node kind, matching the bridge's `kind_str` so engine and
/// live formatting agree.
fn kind_label(kind: super::NodeKind) -> &'static str {
    use super::NodeKind::*;
    match kind {
        Explore => "explore",
        Implement => "implement",
        Verify => "verify",
        Fix => "fix",
        Synthesize => "synthesize",
        Critique => "critique",
    }
}
