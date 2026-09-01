//! Task-DAG engine model.
//!
//! This is the DAG-first reframe of swarm described in `docs/SWARM_TASK_GRAPH.md`.
//! The graph is the primary object: nodes are tasks, edges are dependencies, and
//! agents are fungible workers that execute, decompose (composite nodes), and
//! verify (gate nodes) those tasks.
//!
//! The model here is deliberately decoupled from the server/runtime wiring so it
//! can be exercised end-to-end by the deterministic simulator in [`crate::alphacode_plan::dag::sim`]
//! before being attached to live swarm sessions.

use serde::{Deserialize, Serialize};

mod ops;
mod schedule;
pub mod sim;

#[cfg(test)]
mod tests;

pub use ops::{
    ExpandOutcome, GATE_COVERAGE_ENUMERATION_CAP, complete_node, expand_node, fail_node,
    inject_from_gate, requeue_failed, seed,
};
pub use schedule::{
    LIGHT_MODE_SUGGESTED_WORKERS, assemble_input, dispatch, is_terminal, ready_nodes,
};

/// A node identifier. Stable string ids keep the model serializable and let the
/// auto-generated gate ids derive deterministically from their parent.
pub type NodeId = String;

/// Engine mode. One engine, two presets (see doc section 1a). The data model,
/// scheduler, and dataflow are identical; the mode only controls whether the
/// rigor machinery (mandatory gates + strict artifact validation) is engaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Comprehensive: composite nodes get an auto-inserted critique/verify gate
    /// before they can close, and completion artifacts are strictly validated.
    Deep,
    /// Fan-out: cheap parallelism. No mandatory gates, lightweight artifacts.
    Light,
}

impl Mode {
    pub fn requires_gates(self) -> bool {
        matches!(self, Mode::Deep)
    }
}

/// Where a node came from. Deep mode's growth pressure is measured against
/// this: `Seed` nodes are the first agent's draft, everything else is growth
/// the machinery generated (decomposition, gate-injected gaps, or the gates
/// themselves). Status surfaces report seeded-vs-grown so a plan that never
/// outgrew its seed is visibly under-explored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeOrigin {
    /// Part of the initial `seed` batch (or a later re-seed).
    Seed,
    /// Born from `expand_node` decomposition.
    Expand,
    /// Injected by a gate that found a gap or failure.
    Gap,
    /// An auto-inserted critique/verify gate (including the root gate).
    Gate,
}

/// The terminal action a node represents. The DAG is task-type agnostic; only the
/// artifact contract and which gate kind is inserted vary by node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Research/analysis. Artifact = findings. Gated by `Critique`.
    Explore,
    /// Code change. Artifact = diff/commit ref. Gated by `Verify`.
    Implement,
    /// Acceptance check (build/tests). A gate kind.
    Verify,
    /// Repair after a failed verify. Gated by `Verify`.
    Fix,
    /// Map-reduce rollup of a composite node's children. Gated by `Critique`.
    Synthesize,
    /// Adversarial gap-finder for exploration. A gate kind.
    Critique,
}

impl NodeKind {
    /// Whether this kind is itself a gate (auto-inserted, not user-seeded work).
    pub fn is_gate_kind(self) -> bool {
        matches!(self, NodeKind::Critique | NodeKind::Verify)
    }

    /// The gate kind that guards a composite node of `self` before it may close.
    /// Exploration-style work is guarded by a critique (gap-finding); code-style
    /// work is guarded by a verify (does it actually work).
    pub fn gate_kind(self) -> NodeKind {
        match self {
            NodeKind::Implement | NodeKind::Fix => NodeKind::Verify,
            _ => NodeKind::Critique,
        }
    }
}

/// Node lifecycle status. "Blocked" is intentionally not stored: it is computed
/// from dependency state by the scheduler, so there is a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Not yet dispatched. Becomes runnable once all dependencies are `Done`.
    Queued,
    /// Dispatched to a worker and actively executing.
    Running,
    /// Finished successfully; `output` artifact is attached.
    Done,
    /// Unrecoverable failure. A `Fix`/re-verify path may supersede it.
    Failed,
}

/// Machine-readable confidence rung parsed from an artifact's free-text
/// `confidence` field.
///
/// Confidence is the breadth signal of the task graph: a node completed at
/// [`ConfidenceLevel::Low`] is an admission that its scope was not adequately
/// covered, so the machinery treats it like `what_i_did_not_check` — gates are
/// pointed at low-confidence siblings and (in deep mode) cannot pass while such
/// a sibling is unaddressed. The artifact field stays a free string on the wire
/// for compatibility; this enum is the single lenient interpretation of it so
/// the engine, prompts, and status surfaces never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
}

impl ConfidenceLevel {
    /// Lenient parse. Accepts the common shapes agents actually emit: rung
    /// words with qualifiers ("very low", "medium-high", "High."), negations
    /// ("not confident", "uncertain"), and bare percentages, fractions
    /// ("1/10", "7 out of 10"), or 0-1/0-10/0-100 scores. Returns `None` when
    /// nothing recognizable is present.
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }
        // Negated/uncertain phrasing reads as low. This must run before the
        // word rungs, or "not confident" would match "confident" -> High and
        // silently erase a confidence debt the gate machinery should enforce.
        const NEGATIONS: [&str; 7] = [
            "not high",
            "not confident",
            "not certain",
            "not sure",
            "no confidence",
            "unsure",
            "uncertain",
        ];
        if NEGATIONS.iter().any(|neg| normalized.contains(neg)) {
            return Some(Self::Low);
        }
        // Word rungs; check "low" before "high" so "low-to-high" style
        // hedges resolve pessimistically.
        if normalized.contains("low") {
            return Some(Self::Low);
        }
        if normalized.contains("med") || normalized.contains("moderate") {
            return Some(Self::Medium);
        }
        if normalized.contains("high")
            || normalized.contains("certain")
            || normalized.contains("confident")
        {
            return Some(Self::High);
        }
        // Numeric: take the first number, honoring an explicit denominator
        // ("1/10", "7 out of 10", "3 of 5") before inferring the scale, so a
        // fractional low score is not misread as a 0-1 probability.
        let (value, raw_token, after) = extract_leading_number(&normalized)?;
        let after = after.trim_start();
        let denominator = after
            .strip_prefix('/')
            .or_else(|| after.strip_prefix("out of "))
            .or_else(|| after.strip_prefix("of "))
            .and_then(|rest| extract_leading_number(rest.trim_start()).map(|(d, _, _)| d))
            .filter(|d| *d > 0.0);
        let percent = if let Some(denominator) = denominator {
            value / denominator * 100.0
        } else if normalized.contains('%') || value > 10.0 {
            value
        } else if value <= 1.0 && raw_token.contains('.') {
            // Only a decimal like "0.9" reads as a 0-1 probability; a bare
            // integer "1" is a 1-of-10 score, not full confidence.
            value * 100.0
        } else {
            value * 10.0
        };
        Some(if percent < 50.0 {
            Self::Low
        } else if percent < 80.0 {
            Self::Medium
        } else {
            Self::High
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Deserialize `confidence` from either a JSON string or a bare number.
/// Agents frequently emit `"confidence": 0.8` instead of `"0.8"`; rejecting
/// that with a serde type error is pointless friction, so numbers are
/// stringified and handed to the same lenient [`ConfidenceLevel::parse`].
fn de_confidence_scalar<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar {
        Text(String),
        Number(f64),
        Bool(bool),
    }
    Ok(
        Option::<Scalar>::deserialize(deserializer)?.map(|scalar| match scalar {
            Scalar::Text(text) => text,
            Scalar::Number(number) => number.to_string(),
            Scalar::Bool(flag) => flag.to_string(),
        }),
    )
}

/// Extract the first number in `s`, returning its value, raw token, and the
/// remainder of the string after it. Used by [`ConfidenceLevel::parse`] for
/// score inference (the raw token distinguishes "0.9" from a bare "1").
fn extract_leading_number(s: &str) -> Option<(f64, &str, &str)> {
    let start = s.find(|c: char| c.is_ascii_digit() || c == '.')?;
    let rest = &s[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(rest.len());
    let token = &rest[..end];
    let value: f64 = token.parse().ok()?;
    Some((value, token, &rest[end..]))
}

/// The typed handoff artifact attached to a node on completion. This is the
/// dataflow payload that travels forward along edges to dependents.
///
/// In deep mode, `findings` and `what_i_did_not_check` are required: forcing an
/// agent to enumerate what it did *not* check is what makes thin work structurally
/// visible (doc section 6.3). In light mode any artifact is accepted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffArtifact {
    /// The deliverable summary (findings for explore, what shipped for implement).
    #[serde(default)]
    pub findings: String,
    /// References, not claims: file:line, commit refs, paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_cases_considered: Vec<String>,
    /// Verify results for code-style nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_confidence_scalar"
    )]
    pub confidence: Option<String>,
    /// The cheat code: explicit unexplored surface. Gates convert these into new
    /// nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub what_i_did_not_check: Vec<String>,
}

impl HandoffArtifact {
    /// A minimal artifact for light mode or tests.
    pub fn brief(findings: impl Into<String>) -> Self {
        Self {
            findings: findings.into(),
            ..Self::default()
        }
    }

    /// The machine-readable confidence rung of this artifact, if the free-text
    /// `confidence` field parses to one. See [`ConfidenceLevel`].
    pub fn confidence_level(&self) -> Option<ConfidenceLevel> {
        self.confidence.as_deref().and_then(ConfidenceLevel::parse)
    }

    /// Render this artifact as a forward-dataflow section for a downstream worker
    /// (or a gate). This is the single source of truth for how an artifact is
    /// surfaced on a dependency edge, so the engine scheduler and the live bridge
    /// stay in lockstep.
    ///
    /// Critically this includes `edge_cases_considered` and `what_i_did_not_check`:
    /// a critique gate is explicitly instructed to read what each child did *not*
    /// check, so dropping those fields here would make the gate structurally unable
    /// to do its job (doc sections 5, 6.3).
    pub fn render_section(&self, id: &str, kind: &str) -> String {
        let mut body = format!("## {id} ({kind})\n");
        if !self.findings.trim().is_empty() {
            body.push_str(&self.findings);
            body.push('\n');
        }
        if !self.evidence.is_empty() {
            body.push_str(&format!("Evidence: {}\n", self.evidence.join("; ")));
        }
        if !self.edge_cases_considered.is_empty() {
            body.push_str(&format!(
                "Edge cases considered: {}\n",
                self.edge_cases_considered.join("; ")
            ));
        }
        if let Some(validation) = &self.validation {
            body.push_str(&format!("Validation: {validation}\n"));
        }
        if !self.open_questions.is_empty() {
            body.push_str(&format!(
                "Open questions: {}\n",
                self.open_questions.join("; ")
            ));
        }
        if let Some(confidence) = &self.confidence {
            body.push_str(&format!("Confidence: {confidence}\n"));
        }
        if !self.what_i_did_not_check.is_empty() {
            body.push_str(&format!(
                "What was not checked: {}\n",
                self.what_i_did_not_check.join("; ")
            ));
        }
        body
    }
}

/// A single task node in the DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: NodeId,
    /// The task prompt/instructions for the worker.
    pub content: String,
    pub kind: NodeKind,
    pub status: NodeStatus,
    /// The worker that owns this node (assigned on dispatch). Only the owner may
    /// expand or complete it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The composite node this was decomposed from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    /// Upstream node ids that must be `Done` before this node is runnable. This is
    /// both the dependency relation and the dataflow channel.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    /// True once this node has been decomposed into children (composite). A
    /// composite node re-runs as a synthesis/join once its children + gate close.
    #[serde(default)]
    pub expanded: bool,
    /// True if this node is an auto-inserted gate (critique/verify).
    #[serde(default)]
    pub is_gate: bool,
    /// The agent that planned this node's decomposition. Set when a node is
    /// expanded into a composite; used to prefer the same planner for the
    /// synthesis re-wake while leaving `owner` free for normal scheduling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner: Option<String>,
    /// Priority used to order the ready set. Lower rank runs first.
    #[serde(default)]
    pub priority: u8,
    /// The typed handoff artifact, present once `Done`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<HandoffArtifact>,
    /// Where this node came from (seed vs machinery-generated growth). `None`
    /// on legacy nodes, which are treated as seeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<NodeOrigin>,
}

impl TaskNode {
    pub fn is_composite(&self) -> bool {
        self.expanded
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, NodeStatus::Done)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, NodeStatus::Done | NodeStatus::Failed)
    }
}

/// A declarative spec for a node to add (seed or expand). Ids may be omitted to be
/// auto-assigned by the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<NodeId>,
    pub content: String,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    #[serde(default)]
    pub priority: u8,
}

impl NodeSpec {
    pub fn new(id: impl Into<String>, content: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id: Some(id.into()),
            content: content.into(),
            kind,
            depends_on: Vec::new(),
            priority: 0,
        }
    }

    pub fn depends_on(mut self, deps: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.depends_on = deps.into_iter().map(Into::into).collect();
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Errors produced by validated graph mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    /// A referenced node id does not exist.
    UnknownNode(NodeId),
    /// A node id collides with an existing one.
    DuplicateNode(NodeId),
    /// An edge references a node id that exists nowhere in the operation.
    UnknownDependency { node: NodeId, dependency: NodeId },
    /// The mutation would introduce a cycle.
    WouldCreateCycle(Vec<NodeId>),
    /// The actor is not the owner of the node it tried to mutate.
    NotOwner { node: NodeId, actor: String },
    /// The node is not in a state where the operation is valid.
    InvalidState { node: NodeId, status: NodeStatus },
    /// The completion artifact failed deep-mode validation.
    ThinArtifact { node: NodeId, reason: String },
    /// A deep gate tried to pass while low-confidence sibling work was
    /// unaddressed. The gate must either `inject_from_gate` to convert the doubt
    /// into new nodes, or explicitly address each listed node id in its artifact.
    UnaddressedLowConfidence { gate: NodeId, nodes: Vec<NodeId> },
    /// A deep gate tried to pass without accounting for every completed node in
    /// its audit scope. A passing gate artifact must name each id it reviewed;
    /// enumeration is what makes the audit real instead of a rubber stamp.
    UncoveredSiblings { gate: NodeId, nodes: Vec<NodeId> },
    /// A deep gate tried to pass while its audit scope has non-terminal nodes
    /// (new work arrived after the gate was dispatched, e.g. a re-seed widened
    /// the root set). The gate's view is stale; it must re-run after they drain.
    StaleGateScope { gate: NodeId, pending: Vec<NodeId> },
    /// A gate kind was supplied as user work, or vice versa.
    GateMisuse(String),
}

impl std::fmt::Display for DagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DagError::UnknownNode(id) => write!(f, "unknown node '{id}'"),
            DagError::DuplicateNode(id) => write!(f, "duplicate node id '{id}'"),
            DagError::UnknownDependency { node, dependency } => {
                write!(f, "node '{node}' depends on unknown node '{dependency}'")
            }
            DagError::WouldCreateCycle(ids) => {
                write!(
                    f,
                    "operation would create a cycle among: {}",
                    ids.join(", ")
                )
            }
            DagError::NotOwner { node, actor } => {
                write!(f, "actor '{actor}' does not own node '{node}'")
            }
            DagError::InvalidState { node, status } => {
                write!(
                    f,
                    "node '{node}' is in invalid state {status:?} for this operation"
                )
            }
            DagError::ThinArtifact { node, reason } => {
                write!(f, "node '{node}' artifact rejected: {reason}")
            }
            DagError::UnaddressedLowConfidence { gate, nodes } => {
                write!(
                    f,
                    "gate '{gate}' cannot pass: sibling node(s) [{}] completed with LOW \
                     confidence and the gate artifact does not address them. Either \
                     inject_gap with follow-up nodes that shore up that work, or name each \
                     id in your findings with why its low confidence is acceptable",
                    nodes.join(", ")
                )
            }
            DagError::UncoveredSiblings { gate, nodes } => {
                write!(
                    f,
                    "gate '{gate}' cannot pass: completed node(s) [{}] in its audit scope are \
                     not addressed in the gate artifact. A passing deep gate must account for \
                     every node it audits: name each id in findings/open_questions with what \
                     you checked, or inject_gap with follow-up nodes for anything shaky",
                    nodes.join(", ")
                )
            }
            DagError::StaleGateScope { gate, pending } => {
                write!(
                    f,
                    "gate '{gate}' cannot pass: node(s) [{}] entered its audit scope after it \
                     was dispatched and are not finished. The gate's view is stale; it re-runs \
                     after they drain",
                    pending.join(", ")
                )
            }
            DagError::GateMisuse(msg) => write!(f, "gate misuse: {msg}"),
        }
    }
}

impl std::error::Error for DagError {}

/// The task DAG: a mode plus a set of nodes. Insertion order is preserved for
/// deterministic iteration; lookups are by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraph {
    pub mode: Mode,
    nodes: Vec<TaskNode>,
}

impl TaskGraph {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            nodes: Vec::new(),
        }
    }

    pub fn nodes(&self) -> &[TaskNode] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&TaskNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut TaskNode> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    pub(crate) fn push(&mut self, node: TaskNode) {
        self.nodes.push(node);
    }

    /// Push a fully-formed node. Used by the bridge to lift a `VersionedPlan` into
    /// a `TaskGraph`. Callers are responsible for keeping ids unique; the
    /// validated ops (`seed`/`expand_node`) enforce uniqueness on the write path.
    pub fn push_node(&mut self, node: TaskNode) {
        self.nodes.push(node);
    }

    /// Children of a composite node (excluding its gate).
    pub fn children_of(&self, id: &str) -> Vec<&TaskNode> {
        self.nodes
            .iter()
            .filter(|node| node.parent.as_deref() == Some(id) && !node.is_gate)
            .collect()
    }

    /// The gate node guarding a composite node, if any.
    pub fn gate_of(&self, id: &str) -> Option<&TaskNode> {
        self.nodes
            .iter()
            .find(|node| node.parent.as_deref() == Some(id) && node.is_gate)
    }

    /// Ids of `Done` nodes whose artifact self-reported low confidence. This is
    /// the graph's "shaky coverage" set: work that finished but whose author did
    /// not trust it. Gates treat these as priority probe targets and (in deep
    /// mode) cannot pass over an unaddressed one; status surfaces report them so
    /// a coordinator can widen the graph.
    pub fn low_confidence_done_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|node| node.is_done() && !node.is_gate)
            .filter(|node| {
                node.output
                    .as_ref()
                    .and_then(HandoffArtifact::confidence_level)
                    == Some(ConfidenceLevel::Low)
            })
            .map(|node| node.id.clone())
            .collect()
    }

    /// Whether every node has reached a terminal status.
    pub fn all_terminal(&self) -> bool {
        self.nodes.iter().all(TaskNode::is_terminal)
    }

    /// Detect a cycle over the current `depends_on` edges, returning the node ids
    /// that participate in (or are downstream of) a cycle. Empty when acyclic.
    pub fn cycle_nodes(&self) -> Vec<NodeId> {
        // Kahn's algorithm: repeatedly remove zero-indegree nodes. Anything left
        // is part of, or fed by, a cycle.
        use std::collections::HashMap;
        let known: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut indegree: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            indegree.entry(node.id.as_str()).or_insert(0);
        }
        for node in &self.nodes {
            // Count each unique in-graph dependency once. `depends_on` can carry
            // duplicates (agent-supplied specs are not deduped), and the
            // relaxation below decrements once per unique (dep, dependent) pair,
            // so counting occurrences here would strand acyclic nodes at
            // indegree > 0 and falsely report a cycle.
            let unique_deps: std::collections::HashSet<&str> = node
                .depends_on
                .iter()
                .map(String::as_str)
                .filter(|dep| known.contains(dep))
                .collect();
            *indegree.entry(node.id.as_str()).or_insert(0) += unique_deps.len();
        }
        let mut queue: Vec<&str> = indegree
            .iter()
            .filter_map(|(id, deg)| (*deg == 0).then_some(*id))
            .collect();
        queue.sort_unstable();
        let mut visited = std::collections::HashSet::new();
        while let Some(id) = queue.pop() {
            if !visited.insert(id) {
                continue;
            }
            for node in &self.nodes {
                if node.depends_on.iter().any(|dep| dep == id)
                    && let Some(deg) = indegree.get_mut(node.id.as_str())
                {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push(node.id.as_str());
                    }
                }
            }
        }
        let mut leftover: Vec<NodeId> = self
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .filter(|id| !visited.contains(id.as_str()))
            .collect();
        leftover.sort();
        leftover
    }
}
