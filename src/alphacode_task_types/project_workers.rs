//! Durable worker assignment and report data shared with project state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Specification for spawning a child agent.
///
/// Each child agent receives an objective, constraints, required files,
/// relevant summaries, project rules, coding standards, and acceptance
/// criteria.  Agents work independently and never communicate directly
/// with each other — only through the Main Brain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSpec {
    /// Unique identifier for this agent spawn.
    pub id: String,
    /// Short label for the agent (e.g. "api reviewer").
    pub label: String,
    /// What the agent should accomplish.
    pub objective: String,
    /// Files the agent is allowed to modify.
    #[serde(default)]
    pub required_files: Vec<String>,
    /// Summaries of relevant project areas.
    #[serde(default)]
    pub relevant_summaries: Vec<String>,
    /// Project-specific rules the agent must follow.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Coding standards to enforce.
    #[serde(default)]
    pub coding_standards: Vec<String>,
    /// Criteria that define done.
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    /// Model hint for this agent (e.g. "claude-api:claude-fable-5").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Maximum depth of child agents this agent may spawn (0 = leaf).
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    pub created_at: DateTime<Utc>,
}

fn default_max_depth() -> u32 {
    1
}

// ── Agent Report ──────────────────────────────────────────────────────────

/// Report returned by a child agent after completing its work.
///
/// The Main Brain validates every report before accepting it and merges
/// these reports to detect conflicts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReport {
    /// ID of the agent that produced this report.
    pub agent_id: String,
    /// Short human-readable summary.
    pub summary: String,
    /// Tasks the agent completed.
    #[serde(default)]
    pub completed_tasks: Vec<String>,
    /// Files the agent modified.
    #[serde(default)]
    pub files_modified: Vec<String>,
    /// Files the agent created.
    #[serde(default)]
    pub files_created: Vec<String>,
    /// Problems encountered.
    #[serde(default)]
    pub problems: Vec<String>,
    /// Work remaining in this agent's scope.
    #[serde(default)]
    pub remaining_work: Vec<String>,
    /// Recommendations for the Main Brain.
    #[serde(default)]
    pub recommendations: Vec<String>,
    /// Self-assessed confidence (0.0–1.0).
    pub confidence: f32,
    /// Files or areas that might conflict with other agents.
    #[serde(default)]
    pub potential_conflicts: Vec<String>,
    /// Work the agent thinks needs follow-up.
    #[serde(default)]
    pub required_follow_up: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl AgentReport {
    /// Delivery timestamps can change when a sender rebuilds a retry. Compare
    /// the outcome payload while retaining the first committed timestamp.
    pub fn same_outcome_as(&self, other: &Self) -> bool {
        self.agent_id == other.agent_id
            && self.summary == other.summary
            && self.completed_tasks == other.completed_tasks
            && self.files_modified == other.files_modified
            && self.files_created == other.files_created
            && self.problems == other.problems
            && self.remaining_work == other.remaining_work
            && self.recommendations == other.recommendations
            && self.confidence == other.confidence
            && self.potential_conflicts == other.potential_conflicts
            && self.required_follow_up == other.required_follow_up
    }

    /// Create a minimal report with just the agent ID and a summary.
    pub fn new(agent_id: impl Into<String>, summary: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            agent_id: agent_id.into(),
            summary: summary.into(),
            completed_tasks: Vec::new(),
            files_modified: Vec::new(),
            files_created: Vec::new(),
            problems: Vec::new(),
            remaining_work: Vec::new(),
            recommendations: Vec::new(),
            confidence: 0.5,
            potential_conflicts: Vec::new(),
            required_follow_up: Vec::new(),
            created_at: now,
        }
    }
}
