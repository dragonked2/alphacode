//! Main Brain Coordinator — the project manager that orchestrates everything.
//!
//! The Main Brain never performs implementation work.  Its responsibilities:
//! understand the user's objective, build a complete execution plan, estimate
//! complexity, divide work into phases, create milestones, assign work to
//! agents, merge results, detect conflicts, update project state, decide next
//! actions, and recover after crashes.
//!
//! The Main Brain acts like a project manager.  It validates every report
//! before accepting it.  Agents never communicate directly with each other,
//! only through the Main Brain.

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;

use super::checkpoint_manager::{CheckpointManager, CheckpointTrigger};
use super::project_analyzer::ProjectAnalyzer;
use super::prompt_builder::PromptBuilder;
use super::resource_monitor::ResourceMonitor;
use super::self_review;
use super::task_decomposition;
use super::{
    AgentLimits, AgentReport, AgentSpec, Checkpoint, FileConflict, PhaseStatus, PlanPhase,
    QualityGateResult, ReviewResult, TaskComplexity, detect_file_conflicts,
};
use crate::alphacode_app_core::memory_manager::{
    MemoryManager, ProjectIndex, ProjectState, ProjectStatistics,
};

/// The Main Brain — orchestrates the entire autonomous agent system.
///
/// It owns all sub-managers and coordinates them.  The Main Brain:
/// 1. Understands the user's objective.
/// 2. Builds a complete execution plan (decompose into phases).
/// 3. Estimates complexity.
/// 4. Assigns work to child agents.
/// 5. Merges results and validates reports.
/// 6. Detects conflicts between agents.
/// 7. Updates project state.
/// 8. Creates checkpoints.
/// 9. Runs quality gates.
/// 10. Recovers after crashes.
pub struct MainBrain {
    /// Persistent memory manager (STATE.json, GOALS.md, etc.).
    memory: MemoryManager,
    /// Checkpoint manager.
    checkpoints: CheckpointManager,
    /// Project analyzer / code indexer.
    analyzer: ProjectAnalyzer,
    /// Prompt builder.
    prompt_builder: PromptBuilder,
    /// Resource monitor.
    resources: ResourceMonitor,
    /// Configurable limits.
    limits: AgentLimits,
    /// Execution plan (phases).
    plan: Vec<PlanPhase>,
    /// Active agent specs (by agent ID).
    active_agents: HashMap<String, AgentSpec>,
    /// Completed agent reports (by agent ID).
    completed_reports: Vec<AgentReport>,
    /// Detected file conflicts.
    conflicts: Vec<FileConflict>,
    /// Current project index (from analyzer).
    index: ProjectIndex,
}

impl MainBrain {
    /// Create a new MainBrain rooted at the given directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let memory =
            MemoryManager::new(root).with_context(|| "MainBrain: creating MemoryManager")?;
        let checkpoints = CheckpointManager::new(memory.clone())
            .with_context(|| "MainBrain: creating CheckpointManager")?;
        let analyzer = ProjectAnalyzer::new(root);
        let prompt_builder = PromptBuilder::new(memory.clone());
        let limits = AgentLimits::default();
        let resources = ResourceMonitor::new(limits.clone());

        Ok(Self {
            memory,
            checkpoints,
            analyzer,
            prompt_builder,
            resources,
            limits,
            plan: Vec::new(),
            active_agents: HashMap::new(),
            completed_reports: Vec::new(),
            conflicts: Vec::new(),
            index: ProjectIndex::default(),
        })
    }

    /// Configure the system with custom limits.
    pub fn with_limits(mut self, limits: AgentLimits) -> Self {
        self.resources = ResourceMonitor::new(limits.clone());
        self.limits = limits;
        self
    }

    // ── Objective & Planning ──────────────────────────────────────────

    /// Set the project objective and create an initial execution plan.
    ///
    /// This:
    /// 1. Saves the objective to STATE.json and GOALS.md.
    /// 2. Estimates complexity.
    /// 3. Decomposes into phases if needed.
    /// 4. Creates an initial checkpoint.
    pub fn set_objective(&mut self, objective: &str) -> Result<()> {
        // Save objective to state and goals file.
        self.memory.update_state(|s| {
            s.objective = objective.to_string();
            s.active_phase = None;
            s.completed_phases.clear();
            s.statistics = ProjectStatistics::default();
        })?;
        self.memory
            .save_goals(&format!("# Project Goal\n\n{objective}\n"))?;

        // Index the project to estimate file count.
        self.index = self.analyzer.index_all().unwrap_or_default();
        let file_count = self.index.files.len();

        // Estimate complexity and decompose.
        let complexity = task_decomposition::estimate_complexity(objective, file_count);
        let decomposition = task_decomposition::decompose(objective, complexity, &self.limits);

        // Build plan from decomposition.
        self.plan = decomposition
            .children
            .iter()
            .enumerate()
            .map(|(i, node)| PlanPhase {
                id: format!("phase-{i}"),
                name: node.spec.label.clone(),
                description: node.spec.objective.clone(),
                status: if i == 0 {
                    PhaseStatus::Active
                } else {
                    PhaseStatus::Pending
                },
                milestones: Vec::new(),
                agent_ids: Vec::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .collect();

        // Save plan to PLAN.md.
        self.save_plan_to_disk()?;

        // Create initial checkpoint.
        self.checkpoints.create(
            "init",
            &format!("Objective set: {objective}"),
            CheckpointTrigger::PhaseCompleted,
        )?;

        Ok(())
    }

    // ── Agent Management ──────────────────────────────────────────────

    /// Get the specs of agents to spawn for the current active phase.
    ///
    /// Returns a list of `AgentSpec`s that can be dispatched.  The Main
    /// Brain limits concurrency based on `max_parallel_tasks` and
    /// resource availability.
    pub fn next_agent_specs(&self) -> Vec<AgentSpec> {
        let state = self.memory.load_state().unwrap_or_default();
        let complexity = self.estimate_current_complexity(&state);
        let decomposition =
            task_decomposition::decompose(&state.objective, complexity, &self.limits);
        task_decomposition::flatten(&decomposition)
            .into_iter()
            .take(self.resources.current_concurrency().max(1))
            .collect()
    }

    /// Record that an agent has been dispatched.
    pub fn register_agent(&mut self, spec: AgentSpec) {
        self.memory
            .update_state(|s| {
                if !s.active_agents.contains(&spec.id) {
                    s.active_agents.push(spec.id.clone());
                    s.statistics.agents_spawned += 1;
                }
            })
            .ok();
        self.active_agents.insert(spec.id.clone(), spec);
    }

    /// Accept a report from a completed agent.
    ///
    /// The Main Brain validates every report before accepting it.  It
    /// merges reports, detects conflicts, and updates project state.
    pub fn accept_report(&mut self, report: AgentReport) -> Result<()> {
        // Validate the report.
        if report.agent_id.is_empty() || report.summary.is_empty() {
            anyhow::bail!("invalid agent report: missing agent_id or summary");
        }

        // Remove agent from active list.
        self.memory.update_state(|s| {
            s.active_agents.retain(|id| id != &report.agent_id);
        })?;

        // Record the report.
        self.completed_reports.push(report.clone());

        // Compress completed work into stored memory.
        self.memory.compress_work(
            &report.agent_id,
            &report.summary,
            &[],
            &report.recommendations,
            &report
                .files_modified
                .iter()
                .chain(report.files_created.iter())
                .cloned()
                .collect::<Vec<_>>(),
            &[],
        )?;

        // Detect conflicts with all other reports.
        self.conflicts = detect_file_conflicts(&self.completed_reports);

        Ok(())
    }

    /// Get all detected file conflicts.
    pub fn conflicts(&self) -> &[FileConflict] {
        &self.conflicts
    }

    // ── Phase & Plan Management ─────────────────────────────────────

    /// Mark the current active phase as complete and advance to the next.
    ///
    /// Runs the quality gate before accepting the phase.  If the gate
    /// fails, the phase stays open and follow-up tasks are returned.
    pub fn complete_phase(
        &mut self,
        quality_gate_result: QualityGateResult,
    ) -> Result<PhaseAdvance> {
        if !quality_gate_result.all_pass() {
            return Ok(PhaseAdvance::Blocked(quality_gate_result.failed_checks));
        }

        let current_idx = self
            .plan
            .iter()
            .position(|p| p.status == PhaseStatus::Active);
        let Some(idx) = current_idx else {
            return Ok(PhaseAdvance::NoActivePhase);
        };

        // Mark current as complete.
        self.plan[idx].status = PhaseStatus::Complete;
        self.plan[idx].updated_at = Utc::now();

        // Update state.
        let phase_name = self.plan[idx].name.clone();
        self.memory.update_state(|s| {
            s.completed_phases.push(phase_name.clone());
            s.statistics.phases_completed += 1;
        })?;

        // Create checkpoint.
        self.checkpoints.create(
            &phase_name,
            &format!("Phase complete: {phase_name}"),
            CheckpointTrigger::PhaseCompleted,
        )?;

        // Advance to next pending phase or finish.
        if let Some(next) = self.plan.get_mut(idx + 1)
            && next.status == PhaseStatus::Pending
        {
            next.status = PhaseStatus::Active;
            next.updated_at = Utc::now();
            self.memory.update_state(|s| {
                s.active_phase = Some(next.name.clone());
            })?;
            self.save_plan_to_disk()?;
            return Ok(PhaseAdvance::Advanced);
        }

        self.memory.update_state(|s| {
            s.active_phase = None;
        })?;
        self.save_plan_to_disk()?;
        Ok(PhaseAdvance::Complete)
    }

    // ── Checkpoints & Recovery ──────────────────────────────────────────

    /// Create a checkpoint at the current state.
    pub fn checkpoint(
        &self,
        phase: &str,
        summary: &str,
        trigger: CheckpointTrigger,
    ) -> Result<Checkpoint> {
        self.checkpoints.create(phase, summary, trigger)
    }

    /// Recover from a crash by restoring the best checkpoint.
    ///
    /// After recovery, AlphaCode continues exactly where it stopped.
    /// It never restarts the entire project.
    pub fn recover_from_crash(&self) -> Result<Option<Checkpoint>> {
        let best = self.checkpoints.best_for_recovery()?;
        if let Some(ref cp) = best {
            self.checkpoints.restore(&cp.id)?;
        }
        Ok(best)
    }

    // ── Self-Review ───────────────────────────────────────────────────────

    /// Run a self-review pass on a completed task.
    pub fn run_review(
        &self,
        item: &str,
        checks: Vec<(super::ReviewCategory, bool, String)>,
    ) -> ReviewResult {
        let result = self_review::run_review(item, checks);
        // If problems found, compress them as bugs.
        for task in &result.tasks_created {
            let _ = self.memory.append_bug(task);
        }
        result
    }

    // ── Resource Monitoring ──────────────────────────────────────────────

    /// Get the current recommended concurrency level.
    pub fn recommended_concurrency(&self) -> usize {
        self.resources.recommended_concurrency()
    }

    /// Record a resource snapshot.
    pub fn record_resource(&self, running_agents: usize, queue_length: usize) {
        let snap = self.resources.snapshot(running_agents, queue_length);
        self.resources.record(snap);
    }

    /// Whether resources are currently constrained.
    pub fn is_constrained(&self) -> bool {
        self.resources.is_constrained()
    }

    // ── Prompt Building ──────────────────────────────────────────────────

    /// Build a prompt for a specific agent.
    pub fn build_agent_prompt(&self, spec: &AgentSpec) -> super::prompt_builder::AgentPrompt {
        self.prompt_builder.build_agent_prompt(spec, &self.index)
    }

    /// Build a recovery prompt.
    pub fn build_recovery_prompt(
        &self,
        checkpoint_summary: &str,
    ) -> Result<super::prompt_builder::AgentPrompt> {
        self.prompt_builder
            .build_recovery_prompt(checkpoint_summary)
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    pub fn memory(&self) -> &MemoryManager {
        &self.memory
    }
    pub fn checkpoints(&self) -> &CheckpointManager {
        &self.checkpoints
    }
    pub fn analyzer(&self) -> &ProjectAnalyzer {
        &self.analyzer
    }
    pub fn limits(&self) -> &AgentLimits {
        &self.limits
    }
    pub fn plan(&self) -> &[PlanPhase] {
        &self.plan
    }
    pub fn completed_reports(&self) -> &[AgentReport] {
        &self.completed_reports
    }
    pub fn index(&self) -> &ProjectIndex {
        &self.index
    }

    // ── Internal Helpers ──────────────────────────────────────────────────

    fn estimate_current_complexity(&self, _state: &ProjectState) -> TaskComplexity {
        // Heuristic: more files = higher complexity.
        let file_count = self.index.files.len();
        if file_count > 50 {
            TaskComplexity::Extreme
        } else if file_count > 20 {
            TaskComplexity::High
        } else if file_count > 5 {
            TaskComplexity::Medium
        } else if file_count > 0 {
            TaskComplexity::Low
        } else {
            TaskComplexity::Trivial
        }
    }

    fn save_plan_to_disk(&self) -> Result<()> {
        let mut content = String::from("# Master Execution Plan\n\n");
        for phase in &self.plan {
            content.push_str(&format!(
                "## Phase: {} — `{}`\n\n{}\n\nStatus: `{}`\n\n",
                phase.id,
                phase.name,
                phase.description,
                match phase.status {
                    PhaseStatus::Pending => "pending",
                    PhaseStatus::Active => "active",
                    PhaseStatus::Complete => "complete",
                    PhaseStatus::Blocked => "blocked",
                    PhaseStatus::Skipped => "skipped",
                },
            ));
        }
        self.memory.save_plan(&content)
    }
}

/// Result of attempting to advance to the next phase.
#[derive(Debug, Clone)]
pub enum PhaseAdvance {
    /// Successfully advanced to the next phase.
    Advanced,
    /// All phases are complete: the project is done.
    Complete,
    /// No active phase found.
    NoActivePhase,
    /// Quality gate blocked the advance. Contains the failed checks.
    Blocked(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_brain() -> (TempDir, MainBrain) {
        let dir = TempDir::new().unwrap();
        let brain = MainBrain::new(dir.path()).unwrap();
        (dir, brain)
    }

    #[test]
    fn test_set_objective_creates_plan() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build a REST API server").unwrap();

        // State saved.
        let state = brain.memory().load_state().unwrap();
        assert_eq!(state.objective, "Build a REST API server");

        // Plan created.
        assert!(!brain.plan().is_empty());

        // Goals saved.
        let goals = brain.memory().load_goals().unwrap();
        assert!(goals.contains("Build a REST API server"));

        // Plan saved.
        let plan = brain.memory().load_plan().unwrap();
        assert!(plan.contains("Master Execution Plan"));

        // Checkpoint created.
        let all = brain.checkpoints().list().unwrap();
        assert!(!all.is_empty());
    }

    #[test]
    fn test_accept_report() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Write tests").unwrap();

        let spec = AgentSpec {
            id: "agent-1".into(),
            label: "tester".into(),
            objective: "Write tests for module X".into(),
            required_files: vec!["src/x.rs".into()],
            relevant_summaries: vec![],
            constraints: vec![],
            coding_standards: vec![],
            acceptance_criteria: vec!["All tests pass".into()],
            model: None,
            max_depth: 0,
            created_at: Utc::now(),
        };
        brain.register_agent(spec);

        let report = AgentReport::new("agent-1", "Wrote 10 tests");
        brain.accept_report(report).unwrap();

        assert_eq!(brain.completed_reports().len(), 1);
        let state = brain.memory().load_state().unwrap();
        assert!(!state.active_agents.contains(&"agent-1".to_string()));
    }

    #[test]
    fn test_accept_report_validates() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Do work").unwrap();

        let bad = AgentReport::new("", "");
        assert!(brain.accept_report(bad).is_err());
    }

    #[test]
    fn test_conflict_detection() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Do work").unwrap();

        let r1 = AgentReport {
            files_modified: vec!["src/main.rs".into()],
            ..AgentReport::new("a1", "done")
        };
        let r2 = AgentReport {
            files_modified: vec!["src/main.rs".into()],
            ..AgentReport::new("a2", "done")
        };
        brain.accept_report(r1).unwrap();
        brain.accept_report(r2).unwrap();
        assert!(!brain.conflicts().is_empty());
    }

    #[test]
    fn test_complete_phase_blocked() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();

        let gate = QualityGateResult {
            implementation_complete: true,
            tests_pass: false,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: true,
            checkpoint_created: true,
            failed_checks: vec!["Tests".to_string()],
            evaluated_at: Utc::now(),
        };
        let result = brain.complete_phase(gate).unwrap();
        match result {
            PhaseAdvance::Blocked(fails) => assert_eq!(fails, vec!["Tests".to_string()]),
            _ => panic!("expected Blocked"),
        }
    }

    #[test]
    fn test_complete_phase_all_pass() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();

        let gate = QualityGateResult {
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
        let result = brain.complete_phase(gate).unwrap();
        // Should either advance or complete (depending on plan size).
        match result {
            PhaseAdvance::Advanced | PhaseAdvance::Complete => {}
            PhaseAdvance::NoActivePhase => panic!("phase not found"),
            PhaseAdvance::Blocked(_) => panic!("should not be blocked"),
        }
    }

    #[test]
    fn test_recover_from_crash() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();
        brain
            .checkpoint(
                "phase-0",
                "work in progress",
                CheckpointTrigger::PhaseCompleted,
            )
            .unwrap();

        let recovered = brain.recover_from_crash().unwrap();
        assert!(recovered.is_some());
    }

    #[test]
    fn test_build_agent_prompt_includes_context() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build server").unwrap();

        let spec = AgentSpec {
            id: "agent-1".into(),
            label: "impl".into(),
            objective: "Write HTTP handlers".into(),
            required_files: vec![],
            relevant_summaries: vec!["The server uses Rust".into()],
            constraints: vec!["No blocking calls".into()],
            coding_standards: vec!["Use anyhow".into()],
            acceptance_criteria: vec!["Handles GET and POST".into()],
            model: None,
            max_depth: 0,
            created_at: Utc::now(),
        };
        let prompt = brain.build_agent_prompt(&spec);
        assert!(prompt.body.contains("Write HTTP handlers"));
        assert!(prompt.body.contains("No blocking calls"));
        assert!(prompt.body.contains("Handles GET and POST"));
    }

    #[test]
    fn test_run_review_creates_bug_entries() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Do work").unwrap();

        let result = brain.run_review(
            "src/main.rs",
            vec![(
                super::super::ReviewCategory::Security,
                false,
                "eval injection".into(),
            )],
        );
        assert!(!result.overall_pass);
        let bugs = brain.memory().load_bugs().unwrap();
        assert!(bugs.contains("eval injection"));
    }

    #[test]
    fn test_resource_monitoring() {
        let (_dir, brain) = make_brain();
        brain.record_resource(2, 5);
        let concurrency = brain.recommended_concurrency();
        assert!(concurrency >= 1);
    }

    #[test]
    fn test_custom_limits() {
        let (_dir, brain) = make_brain();
        let limits = AgentLimits {
            max_child_agents: 16,
            ..Default::default()
        };
        let brain = brain.with_limits(limits);
        assert_eq!(brain.limits().max_child_agents, 16);
    }
}
