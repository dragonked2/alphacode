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
use std::sync::Arc;

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
    /// Last committed view used to detect another coordinator's plan changes.
    persisted_plan: Vec<PlanPhase>,
    project_objective: String,
    run_id: String,
    /// Active agent specs (by agent ID).
    active_agents: HashMap<String, AgentSpec>,
    /// Completed agent reports (by agent ID).
    completed_reports: Vec<AgentReport>,
    /// Detected file conflicts.
    conflicts: Vec<FileConflict>,
    /// Current project index (from analyzer).
    index: ProjectIndex,
    /// Decision Plane integration for bounded judgment inference.
    decision_plane: Option<Arc<crate::alphacode_decision_core::DecisionIntegration>>,
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

        let state = match memory.load_state_snapshot() {
            Ok(Some(state)) => state,
            Ok(None) => {
                if let Some(checkpoint) = checkpoints.best_for_recovery()? {
                    checkpoints.recover(&checkpoint.id)?;
                    memory.load_state()?
                } else {
                    ProjectState::default()
                }
            }
            Err(error) => {
                let checkpoint = checkpoints.best_for_recovery()?.with_context(|| {
                    format!("Project state is corrupt and no checkpoint can recover it: {error:#}")
                })?;
                checkpoints.recover(&checkpoint.id)?;
                memory.load_state()?
            }
        };
        let active_agents = state
            .agent_specs
            .iter()
            .filter(|(id, _)| state.active_agents.contains(id))
            .map(|(id, spec)| (id.clone(), spec.clone()))
            .collect();
        let conflicts = detect_file_conflicts(&state.agent_reports);
        Ok(Self {
            run_id: state.run_id,
            persisted_plan: state.plan.clone(),
            project_objective: state.objective,
            plan: state.plan,
            memory,
            checkpoints,
            analyzer,
            prompt_builder,
            resources,
            limits,
            active_agents,
            completed_reports: state.agent_reports,
            conflicts,
            index: ProjectIndex::default(),
            decision_plane: Self::init_decision_plane(),
        })
    }

    /// Initialize Decision Plane from config.
    fn init_decision_plane() -> Option<Arc<crate::alphacode_decision_core::DecisionIntegration>> {
        let cfg = crate::config::config();
        let dp_cfg = &cfg.features.decisions;
        if dp_cfg.mode == crate::alphacode_config_types::DecisionPlaneMode::Off {
            return None;
        }
        let decision_cfg =
            crate::alphacode_decision_core::DecisionConfig::from_config_types(dp_cfg);
        let engine = Arc::new(crate::alphacode_decision_core::DecisionEngine::heuristic(
            decision_cfg,
        ));
        Some(Arc::new(
            crate::alphacode_decision_core::DecisionIntegration::new(
                crate::alphacode_decision_core::DecisionConfig::from_config_types(dp_cfg),
                engine,
            ),
        ))
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
        anyhow::ensure!(
            !objective.trim().is_empty(),
            "Project objective cannot be empty"
        );
        let run_id = super::new_id();
        // Save objective to state and goals file.
        self.memory.update_state(|s| {
            s.objective = objective.to_string();
            s.run_id = run_id.clone();
            s.active_phase = None;
            s.completed_phases.clear();
            s.plan.clear();
            s.active_agents.clear();
            s.agent_specs.clear();
            s.agent_reports.clear();
            s.statistics = ProjectStatistics::default();
        })?;
        self.persisted_plan.clear();
        self.project_objective = objective.to_string();
        self.run_id = run_id;
        self.active_agents.clear();
        self.completed_reports.clear();
        self.conflicts.clear();
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
        use sha2::{Digest, Sha256};
        let state = match self.memory.load_state() {
            Ok(state) => state,
            Err(error) => {
                crate::logging::warn(&format!(
                    "Cannot plan workers from unreadable state: {error:#}"
                ));
                return Vec::new();
            }
        };
        if state.run_id != self.run_id
            || state.objective != self.project_objective
            || state.plan != self.persisted_plan
        {
            return Vec::new();
        }
        let limit = self
            .resources
            .current_concurrency()
            .min(self.limits.max_child_agents as usize)
            .min(self.limits.max_parallel_tasks as usize);
        let available = limit.saturating_sub(state.active_agents.len());
        if available == 0 {
            return Vec::new();
        }
        let Some(phase) = state
            .plan
            .iter()
            .find(|phase| phase.status == PhaseStatus::Active)
        else {
            return Vec::new();
        };
        let complexity = self.estimate_current_complexity(&state);
        let decomposition =
            task_decomposition::decompose(&phase.description, complexity, &self.limits);
        task_decomposition::flatten(&decomposition)
            .into_iter()
            .enumerate()
            .filter_map(|(index, mut spec)| {
                // Do not hash random IDs/timestamps: repeated proposals must
                // refer to the same work instead of creating duplicate agents.
                let definition = serde_json::json!({
                    "index": index, "label": &spec.label, "objective": &spec.objective,
                    "files": &spec.required_files, "summaries": &spec.relevant_summaries,
                    "constraints": &spec.constraints, "standards": &spec.coding_standards,
                    "criteria": &spec.acceptance_criteria, "model": &spec.model,
                    "max_depth": spec.max_depth,
                });
                let digest = format!("{:x}", Sha256::digest(definition.to_string().as_bytes()));
                spec.id = format!("worker-{}-{}-{}", state.run_id, phase.id, &digest[..16]);
                (!state.agent_specs.contains_key(&spec.id)
                    && !state.active_agents.contains(&spec.id)
                    && !state
                        .agent_reports
                        .iter()
                        .any(|report| report.agent_id == spec.id))
                .then_some(spec)
            })
            .take(available)
            .collect()
    }

    /// Commit dispatch intent before exposing the assignment in memory. This
    /// record does not establish that a process was started or remains alive.
    pub fn register_agent(&mut self, spec: AgentSpec) -> Result<()> {
        anyhow::ensure!(
            !spec.id.trim().is_empty() && !spec.objective.trim().is_empty(),
            "Agent assignment requires an ID and objective"
        );
        let limit = self
            .resources
            .current_concurrency()
            .min(self.limits.max_child_agents as usize)
            .min(self.limits.max_parallel_tasks as usize);
        let state = self.memory.try_update_state(|state| {
            anyhow::ensure!(
                state.run_id == self.run_id
                    && state.objective == self.project_objective
                    && state.plan == self.persisted_plan,
                "Coordinator state changed; reload before registering an agent"
            );
            anyhow::ensure!(
                state
                    .plan
                    .iter()
                    .any(|phase| phase.status == PhaseStatus::Active),
                "Agent registration requires an active phase"
            );
            anyhow::ensure!(
                !state
                    .agent_reports
                    .iter()
                    .any(|report| report.agent_id == spec.id),
                "Agent ID already has a completed report"
            );
            if let Some(existing) = state.agent_specs.get(&spec.id) {
                anyhow::ensure!(
                    existing == &spec,
                    "Agent ID already belongs to a different assignment"
                );
                return Ok(());
            }
            if !state.active_agents.contains(&spec.id) {
                anyhow::ensure!(
                    state.active_agents.len() < limit,
                    "Agent concurrency limit reached"
                );
                state.active_agents.push(spec.id.clone());
                state.statistics.agents_spawned = state.statistics.agents_spawned.saturating_add(1);
            }
            state.agent_specs.insert(spec.id.clone(), spec.clone());
            if let Some(phase) = state
                .plan
                .iter_mut()
                .find(|phase| phase.status == PhaseStatus::Active)
                && !phase.agent_ids.contains(&spec.id)
            {
                phase.agent_ids.push(spec.id.clone());
                phase.updated_at = Utc::now();
            }
            Ok(())
        })?;
        self.persisted_plan = state.plan.clone();
        self.plan = state.plan;
        self.active_agents = state
            .agent_specs
            .into_iter()
            .filter(|(id, _)| state.active_agents.contains(id))
            .collect();
        Ok(())
    }

    /// Accept a report from a completed agent.
    ///
    /// The Main Brain validates every report before accepting it.  It
    /// merges reports, detects conflicts, and updates project state.
    pub fn accept_report(&mut self, report: AgentReport) -> Result<()> {
        anyhow::ensure!(
            !report.agent_id.trim().is_empty() && !report.summary.trim().is_empty(),
            "Invalid agent report: missing agent_id or summary"
        );
        anyhow::ensure!(
            report.confidence.is_finite() && (0.0..=1.0).contains(&report.confidence),
            "Invalid agent report confidence"
        );
        let mut inserted = false;
        let state = self.memory.try_update_state(|state| {
            anyhow::ensure!(
                state.run_id == self.run_id && state.objective == self.project_objective,
                "Report belongs to an obsolete coordinator run"
            );
            if let Some(existing) = state
                .agent_reports
                .iter()
                .find(|existing| existing.agent_id == report.agent_id)
            {
                anyhow::ensure!(
                    existing.same_outcome_as(&report),
                    "A conflicting report already exists for this agent"
                );
                return Ok(());
            }
            anyhow::ensure!(
                state.agent_specs.contains_key(&report.agent_id)
                    && state.active_agents.contains(&report.agent_id),
                "Report has no registered unfinished assignment"
            );
            state.agent_reports.push(report.clone());
            state.active_agents.retain(|id| id != &report.agent_id);
            inserted = true;
            Ok(())
        })?;
        self.active_agents = state
            .agent_specs
            .into_iter()
            .filter(|(id, _)| state.active_agents.contains(id))
            .collect();
        self.completed_reports = state.agent_reports;
        self.conflicts = detect_file_conflicts(&self.completed_reports);
        if !inserted {
            return Ok(());
        }

        // Decision Plane observation: evaluate report trustworthiness
        // Only spawn when a Tokio runtime is available (unit tests run
        // without one); observations are best-effort and must never panic.
        if let Some(ref dp) = self.decision_plane
            && tokio::runtime::Handle::try_current().is_ok()
        {
            let dp = dp.clone();
            let agent_id = report.agent_id.clone();
            let confidence = report.confidence;
            let completed = report.completed_tasks.len() as u32;
            let files_mod = report.files_modified.len() as u32;
            let summary_len = report.summary.len();
            tokio::spawn(async move {
                let _ = dp
                    .decide_report_trustworthy(
                        &agent_id,
                        confidence,
                        completed,
                        files_mod,
                        summary_len,
                    )
                    .await;
            });
        }

        // Compress completed work into stored memory.
        if let Err(error) = self.memory.compress_work(
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
        ) {
            // The report is already durable. A summary projection failure
            // must not invite duplicate report acceptance on retry.
            crate::logging::warn(&format!(
                "Report committed but summary projection failed: {error:#}"
            ));
        }

        // Decision Plane observation: evaluate merge coherence
        // Only spawn when a Tokio runtime is available (unit tests run
        // without one); observations are best-effort and must never panic.
        if !self.conflicts.is_empty()
            && let Some(ref dp) = self.decision_plane
            && tokio::runtime::Handle::try_current().is_ok()
        {
            let dp = dp.clone();
            let agents: Vec<String> = self
                .completed_reports
                .iter()
                .map(|r| r.agent_id.clone())
                .collect();
            let files: Vec<String> = self
                .conflicts
                .iter()
                .flat_map(|c| c.conflicting_files.iter().cloned())
                .collect();
            let conflict_count = self.conflicts.len() as u32;
            tokio::spawn(async move {
                let _ = dp
                    .decide_merge_coherent(&agents, &files, conflict_count)
                    .await;
            });
        }

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
        let state = self.memory.load_state()?;
        anyhow::ensure!(
            state.run_id == self.run_id && state.plan == self.persisted_plan,
            "Coordinator state changed; reload before completing a phase"
        );
        if !state.active_agents.is_empty() {
            return Ok(PhaseAdvance::Blocked(vec![format!(
                "{} unfinished worker assignment(s) require completion or reconciliation",
                state.active_agents.len(),
            )]));
        }
        // Decision Plane observation: evaluate quality gate
        // Only spawn when a Tokio runtime is available (unit tests run
        // without one); observations are best-effort and must never panic.
        if let Some(ref dp) = self.decision_plane
            && tokio::runtime::Handle::try_current().is_ok()
        {
            let dp = dp.clone();
            let impl_complete = quality_gate_result.implementation_complete;
            let tests = quality_gate_result.tests_pass;
            let build = quality_gate_result.build_passes;
            let docs = quality_gate_result.documentation_updated;
            let no_crit = quality_gate_result.no_critical_issues;
            let review = quality_gate_result.review_approved;
            let checkpoint = quality_gate_result.checkpoint_created;
            tokio::spawn(async move {
                let _ = dp
                    .decide_quality_gate(
                        impl_complete,
                        tests,
                        build,
                        docs,
                        no_crit,
                        review,
                        checkpoint,
                    )
                    .await;
            });
        }

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

        let previous_plan = self.plan.clone();
        self.plan[idx].status = PhaseStatus::Complete;
        self.plan[idx].updated_at = Utc::now();
        let phase_name = self.plan[idx].name.clone();
        let outcome = match self
            .plan
            .iter_mut()
            .skip(idx + 1)
            .find(|phase| !matches!(phase.status, PhaseStatus::Complete | PhaseStatus::Skipped))
        {
            Some(next) if next.status == PhaseStatus::Blocked => {
                PhaseAdvance::Blocked(vec![format!("Next phase is blocked: {}", next.name)])
            }
            Some(next) => {
                next.status = PhaseStatus::Active;
                next.updated_at = Utc::now();
                PhaseAdvance::Advanced
            }
            None => PhaseAdvance::Complete,
        };
        // Commit the complete transition in one state replacement: phase
        // statuses, active phase, completion summary, and statistics agree.
        if let Err(error) = self.save_plan_to_disk() {
            self.plan = previous_plan;
            return Err(error);
        }
        if let Err(error) = self.checkpoints.create(
            &phase_name,
            &format!("Phase complete: {phase_name}"),
            CheckpointTrigger::PhaseCompleted,
        ) {
            // State already committed. Returning an execution failure here
            // could make a caller repeat the transition on the next phase.
            crate::logging::warn(&format!(
                "Phase {phase_name} committed but checkpoint creation failed: {error:#}"
            ));
        }
        Ok(outcome)
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
    /// Prefer the latest readable committed state; fall back to the selected
    /// checkpoint snapshot if state is missing or corrupt. This does not
    /// restore files or reconcile external side effects.
    pub fn recover_from_crash(&mut self) -> Result<Option<Checkpoint>> {
        let best = self.checkpoints.best_for_recovery()?;
        if let Some(ref cp) = best {
            self.checkpoints.recover(&cp.id)?;
            let state = self.memory.load_state()?;
            self.persisted_plan = state.plan.clone();
            self.plan = state.plan;
            self.project_objective = state.objective;
            self.run_id = state.run_id;
            self.active_agents = state
                .agent_specs
                .iter()
                .filter(|(id, _)| state.active_agents.contains(id))
                .map(|(id, spec)| (id.clone(), spec.clone()))
                .collect();
            self.completed_reports = state.agent_reports;
            self.conflicts = detect_file_conflicts(&self.completed_reports);
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
        // Convert to new tuple format with Medium severity
        let checks_with_severity: Vec<(
            super::ReviewCategory,
            bool,
            super::ReviewSeverity,
            String,
        )> = checks
            .into_iter()
            .map(|(cat, passed, detail)| (cat, passed, super::ReviewSeverity::Medium, detail))
            .collect();
        let result = self_review::run_review(item, checks_with_severity, Vec::new());
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

    fn save_plan_to_disk(&mut self) -> Result<()> {
        self.memory.try_update_state(|state| {
            anyhow::ensure!(
                state.run_id == self.run_id
                    && state.objective == self.project_objective
                    && state.plan == self.persisted_plan,
                "Coordinator plan changed in another writer; reload project state before retrying"
            );
            anyhow::ensure!(
                state.active_agents.is_empty(),
                "Cannot commit a phase transition with unfinished worker assignments"
            );
            state.plan = self.plan.clone();
            state.active_phase = self
                .plan
                .iter()
                .find(|phase| phase.status == PhaseStatus::Active)
                .map(|phase| phase.name.clone());
            state.completed_phases = self
                .plan
                .iter()
                .filter(|phase| phase.status == PhaseStatus::Complete)
                .map(|phase| phase.name.clone())
                .collect();
            state.statistics.phases_completed =
                u32::try_from(state.completed_phases.len()).unwrap_or(u32::MAX);
            Ok(())
        })?;
        self.persisted_plan = self.plan.clone();
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
        // The machine-readable state is the commit record. A projection
        // failure must not make the caller repeat an already-committed phase.
        if let Err(error) = self.memory.save_plan_projection(
            &self.plan,
            &self.project_objective,
            &self.run_id,
            &content,
        ) {
            crate::logging::warn(&format!(
                "Plan committed but PLAN.md refresh failed: {error:#}"
            ));
        }
        Ok(())
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

    fn assignment(id: &str) -> AgentSpec {
        AgentSpec {
            id: id.into(),
            label: "worker".into(),
            objective: "assigned work".into(),
            required_files: vec![],
            relevant_summaries: vec![],
            constraints: vec![],
            coding_standards: vec![],
            acceptance_criteria: vec![],
            model: None,
            max_depth: 0,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn worker_reports_survive_restart_and_replay_without_duplicate_summary() {
        let (dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();
        let spec = assignment("worker-1");
        brain.register_agent(spec.clone()).unwrap();
        brain.register_agent(spec).unwrap();
        assert_eq!(
            brain.memory.load_state().unwrap().statistics.agents_spawned,
            1
        );
        drop(brain);
        let mut reopened = MainBrain::new(dir.path()).unwrap();
        assert!(reopened.active_agents.contains_key("worker-1"));
        assert!(reopened.next_agent_specs().is_empty());
        let report = AgentReport::new("worker-1", "reported work");
        reopened.accept_report(report.clone()).unwrap();
        assert!(reopened.active_agents.is_empty());
        drop(reopened);
        let mut restored = MainBrain::new(dir.path()).unwrap();
        assert_eq!(restored.completed_reports(), std::slice::from_ref(&report));
        restored.accept_report(report.clone()).unwrap();
        let mut retried = report.clone();
        retried.created_at += chrono::Duration::seconds(1);
        restored.accept_report(retried).unwrap();
        assert_eq!(restored.completed_reports().len(), 1);
        assert_eq!(
            restored
                .memory
                .load_summary()
                .unwrap()
                .matches("## Work Summary")
                .count(),
            1
        );
        let mut conflicting = report;
        conflicting.summary = "different result".into();
        assert!(restored.accept_report(conflicting).is_err());
        assert!(
            restored
                .accept_report(AgentReport::new("unknown", "not assigned"))
                .is_err()
        );
    }

    #[test]
    fn failed_assignment_persistence_does_not_claim_a_dispatch() {
        let (dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();
        std::fs::write(dir.path().join("STATE.json"), b"{invalid").unwrap();
        assert!(brain.register_agent(assignment("worker-1")).is_err());
        assert!(brain.active_agents.is_empty());
        assert_eq!(
            std::fs::read(dir.path().join("STATE.json")).unwrap(),
            b"{invalid"
        );
    }

    #[test]
    fn phase_completion_waits_for_unfinished_assignments() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();
        brain.register_agent(assignment("worker-1")).unwrap();
        let before = brain.plan.clone();
        let gate = QualityGateResult {
            implementation_complete: true,
            tests_pass: true,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: true,
            checkpoint_created: true,
            failed_checks: vec![],
            evaluated_at: Utc::now(),
        };
        assert!(matches!(
            brain.complete_phase(gate).unwrap(),
            PhaseAdvance::Blocked(_)
        ));
        assert_eq!(brain.plan, before);
        assert_eq!(brain.memory.load_state().unwrap().plan, before);
    }

    #[test]
    fn old_coordinator_cannot_write_into_a_new_run_of_the_same_objective() {
        let (dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();
        let mut stale = MainBrain::new(dir.path()).unwrap();
        stale.register_agent(assignment("old-worker")).unwrap();
        brain.set_objective("Build app").unwrap();
        assert!(stale.register_agent(assignment("late-worker")).is_err());
        assert!(
            stale
                .accept_report(AgentReport::new("old-worker", "late result"))
                .is_err()
        );
        let state = brain.memory.load_state().unwrap();
        assert!(state.agent_specs.is_empty());
        assert!(state.agent_reports.is_empty());
    }

    #[test]
    fn worker_proposals_are_stable_and_zero_capacity_is_respected() {
        let (_dir, mut brain) = make_brain();
        brain.set_objective("Build app").unwrap();
        let first = brain.next_agent_specs();
        let repeated = brain.next_agent_specs();
        assert!(!first.is_empty());
        assert_eq!(first[0].id, repeated[0].id);
        brain.register_agent(first[0].clone()).unwrap();
        assert!(brain.next_agent_specs().is_empty());
        let zero_limits = AgentLimits {
            max_child_agents: 0,
            ..AgentLimits::default()
        };
        brain = brain.with_limits(zero_limits);
        assert!(brain.next_agent_specs().is_empty());
        assert!(brain.register_agent(assignment("over-limit")).is_err());
    }

    #[test]
    fn stale_coordinator_cannot_replace_newer_plan_or_markdown() {
        let (dir, mut first) = make_brain();
        first.set_objective("Build a REST API server").unwrap();
        let mut stale = MainBrain::new(dir.path()).unwrap();
        first.plan[0].status = PhaseStatus::Blocked;
        first.save_plan_to_disk().unwrap();
        let committed_markdown = first.memory.load_plan().unwrap();
        stale.plan[0].description = "stale replacement".into();
        assert!(stale.save_plan_to_disk().is_err());
        assert_eq!(
            first.memory.load_state().unwrap().plan[0].status,
            PhaseStatus::Blocked
        );
        assert_eq!(first.memory.load_plan().unwrap(), committed_markdown);
        assert!(
            !first
                .memory
                .save_plan_projection(
                    &stale.persisted_plan,
                    &stale.project_objective,
                    &stale.run_id,
                    "stale markdown",
                )
                .unwrap()
        );
        assert_eq!(first.memory.load_plan().unwrap(), committed_markdown);
    }

    #[test]
    fn plan_survives_restart_and_missing_state_recovers_from_checkpoint() {
        let (dir, mut brain) = make_brain();
        brain.set_objective("Build a REST API server").unwrap();
        let expected_plan = brain.plan().to_vec();
        assert!(!expected_plan.is_empty());
        drop(brain);
        let reopened = MainBrain::new(dir.path()).unwrap();
        assert_eq!(reopened.plan(), expected_plan.as_slice());
        assert_eq!(reopened.memory().load_state().unwrap().plan, expected_plan);
        drop(reopened);
        std::fs::remove_file(dir.path().join("STATE.json")).unwrap();
        let recovered = MainBrain::new(dir.path()).unwrap();
        assert_eq!(recovered.plan(), expected_plan.as_slice());
        assert_eq!(
            recovered.memory().load_state().unwrap().objective,
            "Build a REST API server"
        );
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
        brain.register_agent(spec).unwrap();

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
        brain.register_agent(assignment("a1")).unwrap();
        brain.accept_report(r1).unwrap();
        brain.register_agent(assignment("a2")).unwrap();
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
