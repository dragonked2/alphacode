//! Validated graph mutations.
//!
//! Every mutation is append-style and server-validated. Writes are partitioned by
//! owner (you may only expand/complete a node you own), edges may only reference
//! existing nodes, and the result must stay acyclic. In deep mode, expanding a
//! node auto-inserts a critique/verify gate so a composite node cannot close
//! without surviving its gate (doc sections 2, 3, 6).

use super::{
    DagError, HandoffArtifact, Mode, NodeKind, NodeOrigin, NodeSpec, NodeStatus, TaskGraph,
    TaskNode,
};

/// Seed the initial DAG from a batch of specs (the first agent's draft). All
/// referenced dependencies must resolve within the supplied set and the result
/// must be acyclic. Replaying an identical seed definition is a no-op, which makes
/// transport/tool retries safe. Reusing an id for a different definition remains
/// an error. The seed has no owner yet; ownership is assigned on dispatch.
pub fn seed(graph: &mut TaskGraph, specs: Vec<NodeSpec>) -> Result<(), DagError> {
    // Validate ids and collapse exact replays within the same request. A repeated
    // id with different declarative fields is still ambiguous and rejected.
    let mut unique_specs: Vec<NodeSpec> = Vec::with_capacity(specs.len());
    let mut indexes = std::collections::HashMap::<String, usize>::new();
    for spec in specs {
        let id = validated_spec_id(&spec, "seed")?;
        if let Some(existing_index) = indexes.get(&id).copied() {
            if !seed_specs_equivalent(&unique_specs[existing_index], &spec) {
                return Err(DagError::DuplicateNode(id));
            }
            continue;
        }
        indexes.insert(id, unique_specs.len());
        unique_specs.push(spec);
    }

    let known: std::collections::HashSet<&str> = indexes.keys().map(String::as_str).collect();
    for spec in &unique_specs {
        for dep in &spec.depends_on {
            if !known.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: spec.id.clone().unwrap_or_default(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Apply onto a clone, verify acyclicity, then commit.
    let mut staged = graph.clone();
    for spec in unique_specs {
        let id = spec.id.as_deref().expect("seed ids were validated above");
        if let Some(existing) = graph.get(id) {
            if !seed_spec_matches_existing(graph, existing, &spec) {
                return Err(DagError::DuplicateNode(id.to_string()));
            }
            continue;
        }
        staged.push(spec_to_node(spec, None, NodeOrigin::Seed));
    }
    // Deep mode: the whole plan ends in a mandatory adversarial audit. Without
    // this, a flat seed whose nodes all execute atomically would close with
    // zero gates ever firing, silently downgrading deep mode to light. The root
    // gate depends on every root-level node, so the plan cannot reach a
    // terminal state until a final critique/verify pass over everything
    // succeeds — and that gate can `inject_from_gate` new root-level work,
    // which is the top-of-tree growth lever (doc sections 6.2, 7).
    if staged.mode.requires_gates() {
        ensure_root_gate(&mut staged);
    }
    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(())
}

fn seed_specs_equivalent(left: &NodeSpec, right: &NodeSpec) -> bool {
    left.id == right.id
        && left.content == right.content
        && left.kind == right.kind
        && left.priority == right.priority
        && dependency_sets_equal(&left.depends_on, &right.depends_on)
}

fn seed_spec_matches_existing(graph: &TaskGraph, node: &TaskNode, spec: &NodeSpec) -> bool {
    // Only top-level seeded (or legacy, origin-less) work can be replayed. A
    // collision with an expanded child or an auto-generated gate must never be
    // silently treated as the same declaration.
    if node.parent.is_some()
        || node.is_gate
        || !matches!(node.origin, None | Some(NodeOrigin::Seed))
        || node.content != spec.content
        || node.kind != spec.kind
        || node.priority != spec.priority
    {
        return false;
    }

    // Expanding a seeded node appends its children and gate to the parent's
    // dependency list. Filter those machinery-owned join edges so a later replay
    // can still be recognized from the original upstream dependencies.
    let declared_dependencies: Vec<&str> = node
        .depends_on
        .iter()
        .filter(|dependency| {
            graph
                .get(dependency)
                .is_none_or(|candidate| candidate.parent.as_deref() != Some(node.id.as_str()))
        })
        .map(String::as_str)
        .collect();
    dependency_sets_equal_iter(
        declared_dependencies,
        spec.depends_on.iter().map(String::as_str),
    )
}

fn dependency_sets_equal(left: &[String], right: &[String]) -> bool {
    dependency_sets_equal_iter(
        left.iter().map(String::as_str),
        right.iter().map(String::as_str),
    )
}

fn dependency_sets_equal_iter<'a>(
    left: impl IntoIterator<Item = &'a str>,
    right: impl IntoIterator<Item = &'a str>,
) -> bool {
    let left: std::collections::HashSet<&str> = left.into_iter().collect();
    let right: std::collections::HashSet<&str> = right.into_iter().collect();
    left == right
}

/// Insert or refresh the deep-mode root gate so it audits the current
/// root-level node set.
///
/// - No root gate yet and root work exists: create one depending on every
///   non-gate root node.
/// - Root gate exists: extend its dependencies to any new root nodes, and if it
///   already reached a terminal state, re-queue it — new work re-opens the
///   audit, so a re-seeded plan can never stay "finished" unaudited.
fn ensure_root_gate(graph: &mut TaskGraph) {
    let root_ids: Vec<String> = graph
        .nodes()
        .iter()
        .filter(|node| node.parent.is_none() && !node.is_gate)
        .map(|node| node.id.clone())
        .collect();
    if root_ids.is_empty() {
        return;
    }
    let existing_gate = graph
        .nodes()
        .iter()
        .find(|node| node.is_gate && node.parent.is_none())
        .map(|node| node.id.clone());

    match existing_gate {
        Some(gate_id) => {
            // Id was resolved from `graph` two lines above; skip silently if a
            // racecondition-free graph somehow lost it rather than panic.
            let Some(gate) = graph.get_mut(&gate_id) else {
                return;
            };
            let mut widened = false;
            for id in root_ids {
                if !gate.depends_on.contains(&id) {
                    gate.depends_on.push(id);
                    widened = true;
                }
            }
            if widened && gate.is_terminal() {
                gate.status = NodeStatus::Queued;
                gate.owner = None;
            }
        }
        None => {
            // Verify-style root gate only when the whole root set is code work;
            // any exploration in the mix gets the critique (gap-finding) form.
            let all_code = graph
                .nodes()
                .iter()
                .filter(|node| node.parent.is_none() && !node.is_gate)
                .all(|node| matches!(node.kind, NodeKind::Implement | NodeKind::Fix));
            let gate_kind = if all_code {
                NodeKind::Verify
            } else {
                NodeKind::Critique
            };
            let gate_id = unique_gate_id(graph, "plan");
            graph.push(TaskNode {
                id: gate_id,
                content: root_gate_content(gate_kind),
                kind: gate_kind,
                status: NodeStatus::Queued,
                owner: None,
                parent: None,
                depends_on: root_ids,
                expanded: false,
                is_gate: true,
                planner: None,
                priority: 0,
                output: None,
                origin: Some(NodeOrigin::Gate),
            });
        }
    }
}

/// The result of expanding a node into children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandOutcome {
    /// Ids of the child nodes created.
    pub child_ids: Vec<String>,
    /// The id of the auto-inserted gate, if deep mode inserted one.
    pub gate_id: Option<String>,
}

/// Decompose a node the actor owns into a child sub-DAG (the composite path). The
/// node flips to composite and becomes a join/synthesis point that depends on its
/// children. In deep mode a critique/verify gate is auto-inserted between the
/// children and the synthesis, so the composite cannot close without surviving it.
///
/// Children may depend on each other and on the parent's own upstream
/// dependencies (already-existing nodes), preserving acyclicity by construction.
pub fn expand_node(
    graph: &mut TaskGraph,
    node_id: &str,
    actor: &str,
    children: Vec<NodeSpec>,
) -> Result<ExpandOutcome, DagError> {
    {
        let node = graph
            .get(node_id)
            .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
        if node.owner.as_deref() != Some(actor) {
            return Err(DagError::NotOwner {
                node: node_id.to_string(),
                actor: actor.to_string(),
            });
        }
        // Only a running, not-yet-expanded, non-gate node may be decomposed.
        if node.is_gate {
            return Err(DagError::GateMisuse(format!(
                "gate node '{node_id}' cannot be decomposed"
            )));
        }
        if node.expanded || node.status != NodeStatus::Running {
            return Err(DagError::InvalidState {
                node: node_id.to_string(),
                status: node.status,
            });
        }
        if children.is_empty() {
            return Err(DagError::GateMisuse(
                "expand requires at least one child".into(),
            ));
        }
    }

    // Validate child ids and dependency references. Collect the validated ids
    // once so later steps never re-unwrap `spec.id`.
    let mut seen = std::collections::HashSet::new();
    let mut child_ids: Vec<String> = Vec::with_capacity(children.len());
    for spec in &children {
        let id = validated_spec_id(spec, "expand")?;
        if graph.contains(&id) || !seen.insert(id.clone()) {
            return Err(DagError::DuplicateNode(id));
        }
        child_ids.push(id);
    }
    let child_set: std::collections::HashSet<&str> = child_ids.iter().map(String::as_str).collect();
    for (spec, child_id) in children.iter().zip(child_ids.iter()) {
        for dep in &spec.depends_on {
            // A child may depend on a sibling or any already-existing node.
            if !child_set.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: child_id.clone(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Stage onto a clone so a cycle rejects the whole expansion.
    let mut staged = graph.clone();

    // Insert children, parented to this node.
    for spec in children {
        staged.push(spec_to_node(
            spec,
            Some(node_id.to_string()),
            NodeOrigin::Expand,
        ));
    }

    // The synthesis (parent) must wait for every child. In deep mode it must also
    // wait for the gate. We keep the child edges even in deep mode: the gate
    // already depends on every child, so "gate done" implies "children done" for
    // *scheduling*, but the forward-dataflow hydration only reads a node's *direct*
    // dependencies. Dropping the child edges would mean the map-reduce synthesis
    // re-wake never receives its children's artifacts (doc section 5).
    let mut synth_deps = child_ids.clone();

    // Deep mode: insert a gate that depends on all children; the synthesis then
    // additionally depends on the gate so it cannot close until the gate passes.
    let gate_id = if staged.mode.requires_gates() {
        let parent_kind = staged
            .get(node_id)
            .map(|n| n.kind)
            .unwrap_or(NodeKind::Explore);
        let gate_kind = parent_kind.gate_kind();
        let gate_id = unique_gate_id(&staged, node_id);
        let gate = TaskNode {
            id: gate_id.clone(),
            content: gate_content(gate_kind, node_id),
            kind: gate_kind,
            status: NodeStatus::Queued,
            owner: None,
            parent: Some(node_id.to_string()),
            depends_on: child_ids.clone(),
            expanded: false,
            is_gate: true,
            planner: None,
            priority: 0,
            output: None,
            origin: Some(NodeOrigin::Gate),
        };
        staged.push(gate);
        synth_deps.push(gate_id.clone());
        Some(gate_id)
    } else {
        None
    };

    // Flip the parent into a composite join: it re-queues, depends on the
    // gate/children, and is marked expanded. Its prior upstream deps are retained
    // so the synthesis still waits on the original dependencies too.
    {
        let node = staged
            .get_mut(node_id)
            .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
        node.expanded = true;
        node.status = NodeStatus::Queued;
        // Record the planner (current owner) for synthesis re-wake affinity, then
        // free `owner` so the re-queued composite is eligible for normal
        // scheduling once its children + gate complete.
        if node.planner.is_none() {
            node.planner = node.owner.clone();
        }
        node.owner = None;
        // Keep its original upstream deps and add the join deps.
        for dep in synth_deps {
            if !node.depends_on.contains(&dep) {
                node.depends_on.push(dep);
            }
        }
    }

    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(ExpandOutcome { child_ids, gate_id })
}

/// Complete a node the actor owns with a typed handoff artifact. In deep mode the
/// artifact is validated for thinness (findings + an honest "what I did not check"
/// on substantive work) and must carry a parseable confidence rung. A gate
/// additionally may not pass while a sibling under the same composite completed
/// with low confidence, unless the gate's artifact explicitly addresses that node
/// by id — the intended escape hatch is `inject_from_gate`, which converts the
/// doubt into new breadth. The artifact becomes the dataflow payload for
/// dependents.
pub fn complete_node(
    graph: &mut TaskGraph,
    node_id: &str,
    actor: &str,
    artifact: HandoffArtifact,
) -> Result<(), DagError> {
    let mode = graph.mode;
    let node = graph
        .get(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    if node.owner.as_deref() != Some(actor) {
        return Err(DagError::NotOwner {
            node: node_id.to_string(),
            actor: actor.to_string(),
        });
    }
    if node.status != NodeStatus::Running {
        return Err(DagError::InvalidState {
            node: node_id.to_string(),
            status: node.status,
        });
    }
    let is_gate = node.is_gate;
    validate_artifact(mode, node_id, is_gate, &artifact)?;
    if is_gate && mode.requires_gates() {
        validate_gate_pass(graph, node_id, &artifact)?;
    }

    let node = graph
        .get_mut(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    node.status = NodeStatus::Done;
    node.output = Some(artifact);
    Ok(())
}

/// Mark a node the actor owns as failed. A downstream verify/fix path may then
/// supersede it.
pub fn fail_node(graph: &mut TaskGraph, node_id: &str, actor: &str) -> Result<(), DagError> {
    let node = graph
        .get(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    if node.owner.as_deref() != Some(actor) {
        return Err(DagError::NotOwner {
            node: node_id.to_string(),
            actor: actor.to_string(),
        });
    }
    if node.status != NodeStatus::Running {
        return Err(DagError::InvalidState {
            node: node_id.to_string(),
            status: node.status,
        });
    }
    graph
        .get_mut(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?
        .status = NodeStatus::Failed;
    Ok(())
}

/// Inject new gap/fix nodes from a gate that found a problem (the adversarial
/// path). The gate does not decompose itself; instead it adds new sibling nodes
/// under the same composite parent and re-queues itself to depend on them. This is
/// the "re-critique"/"re-verify" loop: the gate cannot pass, and the composite
/// parent (which depends on the gate) cannot close, until the new nodes drain and
/// the gate re-runs cleanly (doc section 6.2).
pub fn inject_from_gate(
    graph: &mut TaskGraph,
    gate_id: &str,
    actor: &str,
    new_nodes: Vec<NodeSpec>,
) -> Result<Vec<String>, DagError> {
    let parent = {
        let gate = graph
            .get(gate_id)
            .ok_or_else(|| DagError::UnknownNode(gate_id.to_string()))?;
        if gate.owner.as_deref() != Some(actor) {
            return Err(DagError::NotOwner {
                node: gate_id.to_string(),
                actor: actor.to_string(),
            });
        }
        if !gate.is_gate {
            return Err(DagError::GateMisuse(format!(
                "node '{gate_id}' is not a gate; use expand_node to decompose work"
            )));
        }
        if gate.status != NodeStatus::Running {
            return Err(DagError::InvalidState {
                node: gate_id.to_string(),
                status: gate.status,
            });
        }
        if new_nodes.is_empty() {
            return Err(DagError::GateMisuse(
                "inject_from_gate requires at least one new node".into(),
            ));
        }
        gate.parent.clone()
    };

    // Validate new node ids/deps.
    let mut seen = std::collections::HashSet::new();
    for spec in &new_nodes {
        let id = validated_spec_id(spec, "inject_from_gate")?;
        if graph.contains(&id) || !seen.insert(id.clone()) {
            return Err(DagError::DuplicateNode(id));
        }
    }
    let mut new_ids: Vec<String> = Vec::with_capacity(new_nodes.len());
    for spec in &new_nodes {
        new_ids.push(validated_spec_id(spec, "inject_from_gate")?);
    }
    let new_set: std::collections::HashSet<&str> = new_ids.iter().map(String::as_str).collect();
    for (spec, new_id) in new_nodes.iter().zip(new_ids.iter()) {
        for dep in &spec.depends_on {
            if !new_set.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: new_id.clone(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    let mut staged = graph.clone();
    for spec in new_nodes {
        staged.push(spec_to_node(spec, parent.clone(), NodeOrigin::Gap));
    }
    // Re-queue the gate, now depending on the new nodes (re-critique/re-verify).
    {
        let gate = staged
            .get_mut(gate_id)
            .ok_or_else(|| DagError::UnknownNode(gate_id.to_string()))?;
        gate.status = NodeStatus::Queued;
        gate.owner = None;
        for id in &new_ids {
            if !gate.depends_on.contains(id) {
                gate.depends_on.push(id.clone());
            }
        }
    }
    // The composite parent must also depend on the gap nodes directly. Scheduling
    // alone would not need this (the gate already gates the parent), but forward
    // dataflow hydration reads only a node's *direct* dependencies, so without
    // these edges the synthesis re-wake would never receive the gap nodes'
    // artifacts — the same reason expand_node keeps child edges (doc section 5).
    if let Some(parent_id) = &parent
        && let Some(parent_node) = staged.get_mut(parent_id)
    {
        for id in &new_ids {
            if !parent_node.depends_on.contains(id) {
                parent_node.depends_on.push(id.clone());
            }
        }
    }
    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(new_ids)
}

/// Re-queue a failed node so it can be dispatched again (the retry path). The
/// owner is cleared: the retry may go to any worker. This is the engine-level
/// counterpart of the live `task_control retry` action; without it a failed
/// deep-mode gate would wedge its composite forever, because `deps_satisfied`
/// requires `Done` and every other mutation requires `Running`.
pub fn requeue_failed(graph: &mut TaskGraph, node_id: &str) -> Result<(), DagError> {
    let node = graph
        .get(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    if node.status != NodeStatus::Failed {
        return Err(DagError::InvalidState {
            node: node_id.to_string(),
            status: node.status,
        });
    }
    let node = graph
        .get_mut(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    node.status = NodeStatus::Queued;
    node.owner = None;
    Ok(())
}

/// Derive a gate id for a composite node that does not collide with an existing
/// node id. The natural choice is `{node}::gate`; if a user happened to seed a
/// node by that exact id we suffix a counter so the engine never silently creates
/// a duplicate id (which would corrupt id-based lookups).
fn unique_gate_id(graph: &TaskGraph, node_id: &str) -> String {
    let base = format!("{node_id}::gate");
    if !graph.contains(&base) {
        return base;
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}{n}");
        if !graph.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Whether free text mentions a node id as a standalone token (not merely as a
/// substring of a longer word). The confidence-debt rule turns on this: with
/// bare `contains`, a short child id like "a" or "fix" would match nearly any
/// English sentence and let a gate rubber-stamp an unaddressed low-confidence
/// sibling. Boundaries are any non-id characters; ids themselves may contain
/// alphanumerics plus `-_.:`/`::` (matching the gate-id convention). A `.` or
/// `:` directly after the id only extends it when followed by another id
/// character, so sentence punctuation ("checked explore.hot.udev.") does not
/// reject an otherwise exact mention.
fn mentions_node_id(text: &str, id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let is_id_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':');
    let mut start = 0;
    while let Some(pos) = text[start..].find(id) {
        let begin = start + pos;
        let end = begin + id.len();
        let before_ok = begin == 0 || !text[..begin].chars().next_back().is_some_and(is_id_char);
        let after_ok = match text[end..].chars().next() {
            None => true,
            Some(c @ ('.' | ':')) => {
                // Ambiguous: '.'/':' are legal id characters AND common prose
                // punctuation. They only continue the id if an id character
                // follows; "node.a." at the end of a sentence is a mention,
                // "node.a.b" is a different id.
                !text[end + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(is_id_char)
            }
            Some(c) => !is_id_char(c),
        };
        if before_ok && after_ok {
            return true;
        }
        // Advance past the first char of this match (char-boundary safe).
        let step = text[begin..].chars().next().map_or(1, char::len_utf8);
        start = begin + step;
        if start >= text.len() {
            break;
        }
    }
    false
}

/// Validate that a spec carries an explicit, non-blank id and return it. A
/// missing id is a misuse; an empty/whitespace id would corrupt id-based
/// lookups and edge references just like a duplicate would.
fn validated_spec_id(spec: &NodeSpec, op: &str) -> Result<String, DagError> {
    let id = spec
        .id
        .clone()
        .ok_or_else(|| DagError::GateMisuse(format!("{op} specs must carry explicit ids")))?;
    if id.trim().is_empty() {
        return Err(DagError::GateMisuse(format!(
            "{op} specs must carry non-empty ids"
        )));
    }
    Ok(id)
}

fn spec_to_node(spec: NodeSpec, parent: Option<String>, origin: NodeOrigin) -> TaskNode {
    // Dedup dependencies (order-preserving). Agent-supplied specs sometimes
    // repeat a dep; duplicates carry no meaning and used to trip the cycle
    // detector's indegree accounting.
    let mut seen = std::collections::HashSet::new();
    let depends_on: Vec<String> = spec
        .depends_on
        .into_iter()
        .filter(|dep| seen.insert(dep.clone()))
        .collect();
    TaskNode {
        id: spec.id.unwrap_or_default(),
        content: spec.content,
        kind: spec.kind,
        status: NodeStatus::Queued,
        owner: None,
        parent,
        depends_on,
        expanded: false,
        is_gate: false,
        planner: None,
        priority: spec.priority,
        output: None,
        origin: Some(origin),
    }
}

fn gate_content(kind: NodeKind, parent: &str) -> String {
    match kind {
        NodeKind::Verify => format!(
            "Verify the work of '{parent}': run the declared acceptance checks (build, tests, lint). \
             If anything fails, emit fix nodes back into the graph; do not pass until they drain."
        ),
        _ => format!(
            "Critique the work of '{parent}' adversarially. Read every child's 'what_i_did_not_check' \
             and find unexplored gaps given this task's stated scope. For each gap, emit a new child node; \
             do not pass until no gaps remain."
        ),
    }
}

/// Content for the auto-inserted root gate: the plan-wide final audit.
fn root_gate_content(kind: NodeKind) -> String {
    match kind {
        NodeKind::Verify => "Final plan-wide verify: run the acceptance checks for the whole \
             plan's declared scope (build, tests, lint) across everything the plan changed. If \
             anything fails, inject fix nodes; the plan cannot finish until they drain."
            .to_string(),
        _ => "Final plan-wide critique: audit the ENTIRE plan adversarially before it may \
             finish. Read every completed node's artifact, especially each \
             'what_i_did_not_check' and every open question, and hunt for whole facets the \
             plan never covered. For each gap, inject a new node; the plan cannot finish \
             until they drain."
            .to_string(),
    }
}

fn validate_artifact(
    mode: Mode,
    node_id: &str,
    is_gate: bool,
    artifact: &HandoffArtifact,
) -> Result<(), DagError> {
    if !mode.requires_gates() {
        // Light mode accepts any artifact.
        return Ok(());
    }
    if is_gate {
        // Gate artifacts are pass/fail records; thinness rules don't apply, and
        // their confidence is about the *gate's* judgement, not the work.
        return Ok(());
    }
    if artifact.findings.trim().is_empty() {
        return Err(DagError::ThinArtifact {
            node: node_id.to_string(),
            reason: "deep-mode artifact requires non-empty findings".into(),
        });
    }
    if artifact.what_i_did_not_check.is_empty() {
        return Err(DagError::ThinArtifact {
            node: node_id.to_string(),
            reason: "deep-mode artifact must list 'what_i_did_not_check' (use an explicit \
                     'nothing, fully covered' entry only when truly exhaustive)"
                .into(),
        });
    }
    // Confidence is the breadth signal: gates prioritize probing low-confidence
    // siblings and cannot pass over unaddressed ones, and status surfaces report
    // them. That machinery only works if every substantive artifact carries a
    // parseable rung, so an absent/unparseable confidence is rejected the same
    // way thin findings are.
    if artifact.confidence_level().is_none() {
        return Err(DagError::ThinArtifact {
            node: node_id.to_string(),
            reason: "deep-mode artifact must state a confidence of low, medium, or high \
                     (honest 'low' is welcome: it routes follow-up work instead of \
                     penalizing you)"
                .into(),
        });
    }
    Ok(())
}

/// Above this many audited nodes, a passing gate artifact no longer has to
/// enumerate every id (the artifact would degenerate into a list); instead only
/// non-HIGH-confidence nodes must be addressed by id (see `validate_gate_pass`).
pub const GATE_COVERAGE_ENUMERATION_CAP: usize = 20;

/// A gate's audit scope: the non-gate nodes it depends on. For a composite
/// gate this is the parent's children plus any gap nodes injected so far; for
/// the root gate it is the plan's root-level node set. Using `depends_on`
/// (rather than parent-based sibling lookup) makes both cases one rule: a gate
/// audits exactly what it waits for.
fn gate_audit_scope<'a>(graph: &'a TaskGraph, gate: &TaskNode) -> Vec<&'a TaskNode> {
    gate.depends_on
        .iter()
        .filter_map(|id| graph.get(id))
        .filter(|node| !node.is_gate)
        .collect()
}

/// Deep-mode rules for a gate trying to PASS (complete rather than inject).
///
/// Three checks, most-specific error first:
///
/// 1. **Stale scope**: every node in the audit scope must be done. Normally
///    guaranteed by dispatch, but the live bridge allows out-of-band mutations
///    (re-seeds widening the root gate, task_control restarts), so a running
///    gate can go stale. Its pass is rejected; it re-runs after the scope
///    drains.
/// 2. **Confidence debt**: a done scope node whose artifact self-reported LOW
///    confidence must be addressed by id in the gate's findings or
///    open_questions (or shored up via `inject_from_gate` first). Applies at
///    any scope width. The gate's own `what_i_did_not_check` deliberately does
///    NOT count: declaring "I did not check X" is the opposite of addressing X.
/// 3. **Coverage debt**: up to [`GATE_COVERAGE_ENUMERATION_CAP`] audited
///    nodes, the passing artifact must address EVERY done node in scope, not
///    just the shaky ones. Enumerated accounting is what separates an audit
///    from a rubber stamp: "all good, no gaps" cannot pass over work it never
///    names. Above the cap, enumeration relaxes only for HIGH-confidence
///    nodes: every node that self-reported medium/low/unparseable confidence
///    must still be addressed by id, so rigor does not silently degrade on
///    exactly the widest scopes where the audit matters most.
fn validate_gate_pass(
    graph: &TaskGraph,
    gate_id: &str,
    artifact: &HandoffArtifact,
) -> Result<(), DagError> {
    let Some(gate) = graph.get(gate_id) else {
        return Ok(());
    };
    let scope = gate_audit_scope(graph, gate);
    if scope.is_empty() {
        return Ok(());
    }

    let pending: Vec<String> = scope
        .iter()
        .filter(|node| !node.is_done())
        .map(|node| node.id.clone())
        .collect();
    if !pending.is_empty() {
        return Err(DagError::StaleGateScope {
            gate: gate_id.to_string(),
            pending,
        });
    }

    let addressed = |id: &str| {
        mentions_node_id(&artifact.findings, id)
            || artifact
                .open_questions
                .iter()
                .any(|q| mentions_node_id(q, id))
    };

    let confidence_debts: Vec<String> = scope
        .iter()
        .filter(|node| {
            node.output
                .as_ref()
                .and_then(HandoffArtifact::confidence_level)
                == Some(super::ConfidenceLevel::Low)
        })
        .filter(|node| !addressed(&node.id))
        .map(|node| node.id.clone())
        .collect();
    if !confidence_debts.is_empty() {
        return Err(DagError::UnaddressedLowConfidence {
            gate: gate_id.to_string(),
            nodes: confidence_debts,
        });
    }

    if scope.len() <= GATE_COVERAGE_ENUMERATION_CAP {
        let uncovered: Vec<String> = scope
            .iter()
            .filter(|node| !addressed(&node.id))
            .map(|node| node.id.clone())
            .collect();
        if !uncovered.is_empty() {
            return Err(DagError::UncoveredSiblings {
                gate: gate_id.to_string(),
                nodes: uncovered,
            });
        }
    } else {
        // Wide scope: naming every id would degenerate into a list, so full
        // enumeration relaxes. But the audit must still drain every doubt by
        // id: any node that did not self-report HIGH confidence (medium, low,
        // or unparseable) stays on the hook and must be addressed. Without
        // this, gate rigor would silently degrade exactly when the scope is
        // largest and the audit matters most.
        let uncovered: Vec<String> = scope
            .iter()
            .filter(|node| {
                node.output
                    .as_ref()
                    .and_then(HandoffArtifact::confidence_level)
                    != Some(super::ConfidenceLevel::High)
            })
            .filter(|node| !addressed(&node.id))
            .map(|node| node.id.clone())
            .collect();
        if !uncovered.is_empty() {
            return Err(DagError::UncoveredSiblings {
                gate: gate_id.to_string(),
                nodes: uncovered,
            });
        }
    }
    Ok(())
}
