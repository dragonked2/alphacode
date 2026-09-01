//! Core autonomous agent architecture types.
//!
//! These types implement the plan2.md design for a persistent autonomous
//! software engineering agent: hierarchical agents, task decomposition,
//! agent reports, checkpoints, quality gates, resource monitoring, and
//! configurable limits.

pub mod checkpoint_manager;
pub mod main_brain;
pub mod project_analyzer;
pub mod prompt_builder;
pub mod quality_gate;
pub mod resource_monitor;
pub mod self_review;
pub mod task_decomposition;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Plan & Phases ────────────────────────────────────────────────────────

/// A single phase in a master execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPhase {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    #[serde(default)]
    pub milestones: Vec<Milestone>,
    #[serde(default)]
    pub agent_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PhaseStatus {
    #[default]
    Pending,
    Active,
    Complete,
    Blocked,
    Skipped,
}


/// A milestone within a phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: String,
    pub name: String,
    pub criteria: Vec<String>,
    pub status: PhaseStatus,
    pub created_at: DateTime<Utc>,
}

// ── Task Decomposition ────────────────────────────────────────────────────

/// Estimated complexity of a task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[derive(Default)]
pub enum TaskComplexity {
    /// Trivial: single small change, < 100 lines.
    Trivial = 0,
    /// Low: a few files, straightforward logic.
    #[default]
    Low = 1,
    /// Medium: multiple files, some design decisions.
    Medium = 2,
    /// High: many files, new architecture, cross-module.
    High = 3,
    /// Extreme: system-level redesign, multi-day.
    Extreme = 4,
}

impl TaskComplexity {
    /// Returns `true` if the complexity exceeds the threshold requiring
    /// decomposition into smaller phases.
    pub fn needs_decomposition(self, threshold: TaskComplexity) -> bool {
        self >= threshold
    }
}


// ── Agent Spec ───────────────────────────────────────────────────────────┐

/// Specification for spawning a child agent.
///
/// Each child agent receives an objective, constraints, required files,
/// relevant summaries, project rules, coding standards, and acceptance
/// criteria.  Agents work independently and never communicate directly
/// with each other — only through the Main Brain.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ── Agent Limits ──────────────────────────────────────────────────────────

/// Configurable limits for the autonomous agent system.
///
/// Controls how many child agents can be spawned, how deep the hierarchy
/// goes, how much context each agent gets, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLimits {
    /// Maximum number of concurrent child agents.
    pub max_child_agents: u32,
    /// Maximum recursion depth for agent spawning.
    pub max_depth: u32,
    /// Maximum number of tasks running in parallel.
    pub max_parallel_tasks: u32,
    /// Maximum context tokens per agent before auto-compaction.
    pub max_context_per_agent: u64,
    /// Maximum total memory usage in bytes before throttling.
    pub max_memory_bytes: u64,
    /// Maximum number of retries for a failed task.
    pub max_retry_count: u32,
    /// Maximum runtime per task in seconds (0 = unlimited).
    pub max_runtime_secs: u64,
    /// Complexity threshold above which tasks are automatically decomposed.
    #[serde(default)]
    pub decomposition_threshold: TaskComplexity,
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            max_child_agents: 16,
            max_depth: 5,
            max_parallel_tasks: 12,
            max_context_per_agent: 400_000,
            max_memory_bytes: 16 * 1024 * 1024 * 1024, // 16 GB
            max_retry_count: 8,
            max_runtime_secs: 259200, // 3 days (month-long sessions)
            decomposition_threshold: TaskComplexity::High,
        }
    }
}

// ── Quality Gate ──────────────────────────────────────────────────────────

/// Quality gate result for a phase.
///
/// A phase is only considered complete if **all** of these checks pass:
/// implementation complete, tests pass, build passes, documentation updated,
/// no critical issues, review approved, and checkpoint created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGateResult {
    pub implementation_complete: bool,
    pub tests_pass: bool,
    pub build_passes: bool,
    pub documentation_updated: bool,
    pub no_critical_issues: bool,
    pub review_approved: bool,
    pub checkpoint_created: bool,
    #[serde(default)]
    pub failed_checks: Vec<String>,
    pub evaluated_at: DateTime<Utc>,
}

impl QualityGateResult {
    /// Returns `true` if all quality gate checks pass.
    pub fn all_pass(&self) -> bool {
        self.implementation_complete
            && self.tests_pass
            && self.build_passes
            && self.documentation_updated
            && self.no_critical_issues
            && self.review_approved
            && self.checkpoint_created
    }

    /// A fresh quality gate with all checks pending (all `false`).
    ///
    /// `failed_checks` starts empty; callers that evaluate real checks
    /// (e.g. [`crate::alphacode_app_core::autonomous::quality_gate::evaluate`])
    /// populate it with the reasons for each failing check.
    pub fn pending() -> Self {
        Self {
            implementation_complete: false,
            tests_pass: false,
            build_passes: false,
            documentation_updated: false,
            no_critical_issues: false,
            review_approved: false,
            checkpoint_created: false,
            failed_checks: Vec::new(),
            evaluated_at: Utc::now(),
        }
    }
}

// ── Checkpoint ─────────────────────────────────────────────────────────────

/// A checkpoint capturing full project state at a point in time.
///
/// Checkpoints are created on phase completion, important discoveries,
/// large file modifications, at configurable time intervals, and before
/// risky operations.  They allow rolling back to a known good state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub phase: String,
    pub summary: String,
    /// Open tasks at checkpoint time.
    #[serde(default)]
    pub open_tasks: Vec<String>,
    /// Completed tasks at checkpoint time.
    #[serde(default)]
    pub completed_tasks: Vec<String>,
    /// Agent status snapshot at checkpoint time.
    #[serde(default)]
    pub agent_status: Vec<String>,
    /// Git commit hash if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Progress summary text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_summary: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ── Review ────────────────────────────────────────────────────────────────

/// Self-review pass over completed work.
///
/// After each major task, an independent review pass checks:
/// logic, performance, security, correctness, style, documentation,
/// and architecture.  If problems are found, new tasks are created
/// automatically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub reviewed_item: String,
    pub checks: Vec<ReviewCheck>,
    pub overall_pass: bool,
    #[serde(default)]
    pub tasks_created: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// A single check in a self-review pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCheck {
    pub category: ReviewCategory,
    pub passed: bool,
    pub detail: String,
}

/// Categories checked during self-review.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCategory {
    Logic,
    Performance,
    Security,
    Correctness,
    Style,
    Documentation,
    Architecture,
}

impl ReviewCategory {
    pub fn all() -> &'static [ReviewCategory] {
        &[
            Self::Logic,
            Self::Performance,
            Self::Security,
            Self::Correctness,
            Self::Style,
            Self::Documentation,
            Self::Architecture,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Logic => "logic",
            Self::Performance => "performance",
            Self::Security => "security",
            Self::Correctness => "correctness",
            Self::Style => "style",
            Self::Documentation => "documentation",
            Self::Architecture => "architecture",
        }
    }

    /// Scheduling priority for follow-up work: lower is fixed sooner.
    /// Security and correctness outrank style and documentation so the most
    /// dangerous failures are addressed first.
    pub fn priority(self) -> u8 {
        match self {
            Self::Security => 0,
            Self::Logic => 1,
            Self::Correctness => 2,
            Self::Performance => 3,
            Self::Architecture => 4,
            Self::Documentation => 5,
            Self::Style => 6,
        }
    }
}

// ── Resource Monitor ──────────────────────────────────────────────────────

/// Snapshot of system resource usage.
///
/// The resource monitor tracks VRAM, RAM, CPU, disk, context usage,
/// inference speed, queue length, and running agents.  It can
/// automatically reduce concurrency when resources are constrained
/// and increase concurrency when resources are available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub vram_bytes: Option<u64>,
    pub ram_bytes: Option<u64>,
    pub cpu_percent: Option<f32>,
    pub disk_free_bytes: Option<u64>,
    pub context_tokens: Option<u64>,
    pub inference_speed_tokens_per_sec: Option<f64>,
    pub queue_length: usize,
    pub running_agents: usize,
    pub measured_at: DateTime<Utc>,
}

impl ResourceSnapshot {
    pub fn now(running_agents: usize, queue_length: usize) -> Self {
        Self {
            vram_bytes: None,
            ram_bytes: sys_ram_bytes(),
            cpu_percent: None,
            disk_free_bytes: sys_disk_free_bytes(),
            context_tokens: None,
            inference_speed_tokens_per_sec: None,
            queue_length,
            running_agents,
            measured_at: Utc::now(),
        }
    }

    /// Suggested concurrency level based on available resources.
    pub fn suggested_concurrency(&self, max: usize) -> usize {
        if self.running_agents >= max {
            return max;
        }
        // Heuristic: leave one slot free per 4 GB of RAM consumed.
        if let Some(ram) = self.ram_bytes {
            let gb4 = 4_u64 * 1024 * 1024 * 1024;
            let slack = ram.saturating_sub(gb4 * 2);
            let from_ram = slack / gb4;
            let result = (self.running_agents + 1 + from_ram as usize).min(max);
            return result.max(1);
        }
        (self.running_agents + 1).min(max)
    }
}

fn sys_ram_bytes() -> Option<u64> {
    // Best-effort: on Windows, use GlobalMemoryStatusEx; otherwise, return None.
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        unsafe {
            let mut status = std::mem::zeroed::<MEMORYSTATUSEX>();
            status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut status) != 0 {
                return Some(status.ullTotalPhys);
            }
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn sys_disk_free_bytes() -> Option<u64> {
    // Best-effort cross-platform disk free: use std::fs metadata on cwd.
    // The precise statvfs call is platform-specific; omit for portability here.
    None
}

// ── Decision ──────────────────────────────────────────────────────────────

/// An architectural or engineering decision, persisted to `DECISIONS.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// ── Dependency ────────────────────────────────────────────────────────────

/// A dependency in the project's dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub version: Option<String>,
    pub source: Option<String>,
    #[serde(default)]
    pub dependents: Vec<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Generate a unique short ID using timestamp + random.
pub fn new_id() -> String {
    let ts = Utc::now().timestamp_millis();
    let rand: u64 = rand::random();
    format!("{ts:x}{rand:x}")
}

/// Compute file overlap between two lists of files (for conflict detection).
pub fn file_overlap(a: &[String], b: &[String]) -> Vec<String> {
    let set: std::collections::HashSet<_> = b.iter().collect();
    a.iter().filter(|f| set.contains(f)).cloned().collect()
}

/// Detect conflicts between agent reports based on file overlap.
pub fn detect_file_conflicts(reports: &[AgentReport]) -> Vec<FileConflict> {
    let mut conflicts = Vec::new();
    for i in 0..reports.len() {
        for j in (i + 1)..reports.len() {
            let a = &reports[i];
            let b = &reports[j];
            let all_a: Vec<String> = a
                .files_modified
                .iter()
                .chain(a.files_created.iter())
                .cloned()
                .collect();
            let all_b: Vec<String> = b
                .files_modified
                .iter()
                .chain(b.files_created.iter())
                .cloned()
                .collect();
            let overlap = file_overlap(&all_a, &all_b);
            if !overlap.is_empty() {
                conflicts.push(FileConflict {
                    agent_a: a.agent_id.clone(),
                    agent_b: b.agent_id.clone(),
                    conflicting_files: overlap,
                });
            }
        }
    }
    conflicts
}

/// A conflict between two agents over overlapping files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConflict {
    pub agent_a: String,
    pub agent_b: String,
    pub conflicting_files: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_complexity_ordering() {
        assert!(TaskComplexity::Extreme > TaskComplexity::High);
        assert!(TaskComplexity::High.needs_decomposition(TaskComplexity::Medium));
        assert!(!TaskComplexity::Low.needs_decomposition(TaskComplexity::Medium));
    }

    #[test]
    fn test_quality_gate_pending() {
        let q = QualityGateResult::pending();
        assert!(!q.all_pass());
    }

    #[test]
    fn test_quality_gate_all_pass() {
        let q = QualityGateResult {
            implementation_complete: true,
            tests_pass: true,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: true,
            checkpoint_created: true,
            failed_checks: Vec::new(),
            evaluated_at: Utc::now(),
        };
        assert!(q.all_pass());
    }

    #[test]
    fn test_file_overlap() {
        let a = vec!["foo.rs".to_string(), "bar.rs".to_string()];
        let b = vec!["bar.rs".to_string(), "baz.rs".to_string()];
        let overlap = file_overlap(&a, &b);
        assert_eq!(overlap, vec!["bar.rs"]);
    }

    #[test]
    fn test_detect_conflicts() {
        let r1 = AgentReport {
            agent_id: "a1".into(),
            summary: "done".into(),
            files_modified: vec!["src/main.rs".into()],
            files_created: vec![],
            confidence: 0.8,
            ..AgentReport::new("a1", "done")
        };
        let r2 = AgentReport {
            agent_id: "a2".into(),
            summary: "done".into(),
            files_modified: vec!["src/main.rs".into()],
            files_created: vec![],
            confidence: 0.8,
            ..AgentReport::new("a2", "done")
        };
        let conflicts = detect_file_conflicts(&[r1, r2]);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].conflicting_files.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_agent_limits_default() {
        let l = AgentLimits::default();
        assert_eq!(l.max_child_agents, 8);
        assert_eq!(l.max_depth, 3);
        assert_eq!(l.max_parallel_tasks, 4);
    }

    #[test]
    fn test_review_category_all() {
        assert_eq!(ReviewCategory::all().len(), 7);
    }

    #[test]
    fn test_agent_spec_roundtrip() {
        let spec = AgentSpec {
            id: new_id(),
            label: "api reviewer".into(),
            objective: "Review API endpoints".into(),
            required_files: vec!["src/api.rs".into()],
            relevant_summaries: vec!["The API layer handles HTTP".into()],
            constraints: vec!["No breaking changes".into()],
            coding_standards: vec!["Use anyhow::Result".into()],
            acceptance_criteria: vec!["All tests pass".into()],
            model: None,
            max_depth: 1,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: AgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label, "api reviewer");
    }

    #[test]
    fn test_checkpoint_roundtrip() {
        let cp = Checkpoint {
            id: new_id(),
            phase: "Phase 1".into(),
            summary: "Architecture complete".into(),
            open_tasks: vec!["Write tests".into()],
            completed_tasks: vec!["Design".into()],
            agent_status: vec!["agent-1: done".into()],
            git_commit: Some("abc123".into()),
            progress_summary: Some("50% done".into()),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&cp).unwrap();
        let back: Checkpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase, "Phase 1");
        assert_eq!(back.git_commit.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_resource_snapshot_concurrency() {
        let snap = ResourceSnapshot::now(2, 5);
        let good = snap.suggested_concurrency(4);
        assert!((1..=4).contains(&good));
    }
}
