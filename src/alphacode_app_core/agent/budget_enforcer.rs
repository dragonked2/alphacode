use std::collections::HashMap;
use std::time::Instant;

use crate::alphacode_task_types::goal_contract::{ContractPhase, GoalContract};

/// Tracks per-phase call counts and wall-clock time for checkpoint reminders.
/// Also tracks consecutive failures per tool for the auxiliary-tool stop-loss.
pub struct BudgetEnforcer {
    /// Start time of the current phase.
    phase_start: Instant,
    /// Call count in the current phase.
    phase_calls: u32,
    checkpoint_reported: bool,
    /// Current phase being tracked.
    current_phase: ContractPhase,
    /// Per-tool consecutive failure counts.
    consecutive_failures: HashMap<String, u32>,
    /// Tools flagged as degraded (consecutive failures >= threshold).
    degraded_tools: HashMap<String, DegradedInfo>,
    /// Total calls across all phases.
    total_calls: u32,
    /// Call count when the highest-tier evidence arrived (if any).
    goal_achieved_at_call: Option<u32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DegradedInfo {
    failures: u32,
    flagged_at: u32,
}

impl BudgetEnforcer {
    pub fn new() -> Self {
        Self {
            phase_start: Instant::now(),
            phase_calls: 0,
            checkpoint_reported: false,
            current_phase: ContractPhase::Recon,
            consecutive_failures: HashMap::new(),
            degraded_tools: HashMap::new(),
            total_calls: 0,
            goal_achieved_at_call: None,
        }
    }

    /// Record a tool call and flag checkpoints or degraded report-phase tools.
    pub fn record_call(&mut self, tool_name: &str, contract: &GoalContract) -> BudgetVerdict {
        self.transition_phase(contract.phase);
        self.total_calls = self.total_calls.saturating_add(1);
        self.phase_calls = self.phase_calls.saturating_add(1);

        // Check if the tool is degraded.
        //
        // Use `contract.phase` as the source of truth, not `self.current_phase`:
        // `current_phase` was pinned at `Recon` forever (nothing advanced it on
        // normal progress), so this `Report` arm was unreachable and the
        // stop-loss only ever fired via `is_terminal()`.
        if let Some(info) = self.degraded_tools.get(tool_name) {
            // Only reject if the contract is satisfiable without this tool
            if contract.is_terminal() || contract.phase == ContractPhase::Report {
                return BudgetVerdict::Rejected(format!(
                    "Tool '{}' is degraded ({} consecutive failures). Goal is already satisfiable without it.",
                    tool_name, info.failures
                ));
            }
        }

        if self.checkpoint_reported {
            return BudgetVerdict::Allowed;
        }
        let budget = self.budget_for_phase(contract);
        let elapsed = self.phase_start.elapsed();

        if self.phase_calls > budget.max_calls {
            self.checkpoint_reported = true;
            return BudgetVerdict::OverBudget(format!(
                "Phase '{}' exceeded budget: {} calls (max {})",
                contract.phase.as_str(),
                self.phase_calls,
                budget.max_calls
            ));
        }

        if elapsed.as_secs() > budget.max_seconds {
            self.checkpoint_reported = true;
            return BudgetVerdict::OverBudget(format!(
                "Phase '{}' exceeded time budget: {}s (max {}s)",
                contract.phase.as_str(),
                elapsed.as_secs(),
                budget.max_seconds
            ));
        }

        BudgetVerdict::Allowed
    }

    /// Record a tool call result (success or failure).
    pub fn record_result(&mut self, tool_name: &str, success: bool) {
        if success {
            self.consecutive_failures.insert(tool_name.to_string(), 0);
            // A tool that has recovered is not degraded. Without this removal a
            // tool that failed twice (transient DNS, a flaky endpoint) stayed in
            // `degraded_tools` with a stale failure count for the rest of the
            // session, and every later call was rejected with a message that was
            // no longer true.
            self.degraded_tools.remove(tool_name);
        } else {
            let failures = self
                .consecutive_failures
                .entry(tool_name.to_string())
                .or_insert(0);
            *failures += 1;

            // Flag as degraded after 2 consecutive failures
            if *failures >= 2 {
                self.degraded_tools.insert(
                    tool_name.to_string(),
                    DegradedInfo {
                        failures: *failures,
                        flagged_at: self.total_calls,
                    },
                );
            }
        }
    }

    /// Mark the goal as achieved (called when terminal evidence arrives).
    pub fn mark_goal_achieved(&mut self) {
        if self.goal_achieved_at_call.is_none() {
            self.goal_achieved_at_call = Some(self.total_calls);
        }
    }

    /// Transition to a new phase. Resets phase-local counters.
    pub fn transition_phase(&mut self, new_phase: ContractPhase) {
        if new_phase != self.current_phase {
            self.current_phase = new_phase;
            self.phase_calls = 0;
            self.checkpoint_reported = false;
            self.phase_start = Instant::now();
        }
    }

    /// Start a new turn: reset the per-phase counters and adopt the contract's
    /// current phase.
    ///
    /// `BudgetEnforcer` is a per-`Agent` field that lives across turns, so
    /// without this reset `phase_calls` accumulated over an entire session.
    /// A session doing 5 recon calls per turn tripped the 20-call `Execute`
    /// budget after ~4 turns and was then permanently forced into `Report`.
    pub fn begin_turn(&mut self, phase: Option<ContractPhase>) {
        let phase = phase.unwrap_or(ContractPhase::Recon);
        if phase != self.current_phase {
            self.current_phase = phase;
        }
        self.phase_calls = 0;
        self.checkpoint_reported = false;
        self.phase_start = Instant::now();
    }

    /// Get the budget for a phase.
    ///
    /// Reads the contract's own `budgets` rather than `PhaseBudgets::default()`.
    /// `GoalContract.budgets` is a real, serialized, per-mission field, but it
    /// was never read here, so any budget a mission actually configured was
    /// silently ignored and the hardcoded default was always enforced.
    fn budget_for_phase(
        &self,
        contract: &GoalContract,
    ) -> crate::alphacode_task_types::goal_contract::Budget {
        let budgets = &contract.budgets;
        match contract.phase {
            ContractPhase::Recon => budgets.recon.clone(),
            ContractPhase::Execute => budgets.execute.clone(),
            ContractPhase::Verify => budgets.verify.clone(),
            ContractPhase::Report => crate::alphacode_task_types::goal_contract::Budget {
                max_calls: 3,
                max_seconds: 60,
            },
        }
    }

    /// Check if the verify phase has outlived the execute phase (wall-clock
    /// awareness). Returns `true` if verification is taking too long.
    #[allow(dead_code)]
    pub fn verify_phase_overrun(&self, contract: &GoalContract) -> bool {
        if contract.phase != ContractPhase::Verify {
            return false;
        }
        let verify_budget = contract.budgets.verify.clone();
        self.phase_start.elapsed().as_secs() > verify_budget.max_seconds
    }

    /// Get the number of calls after the goal was achieved.
    pub fn post_goal_calls(&self) -> u32 {
        match self.goal_achieved_at_call {
            Some(achieved_at) => self.total_calls.saturating_sub(achieved_at),
            None => 0,
        }
    }

    /// Get total calls.
    pub fn total_calls(&self) -> u32 {
        self.total_calls
    }

    /// Get the call count when the goal was achieved.
    #[allow(dead_code)]
    pub fn goal_achieved_at_call(&self) -> Option<u32> {
        self.goal_achieved_at_call
    }

    /// Get consecutive failures for a tool.
    #[allow(dead_code)]
    pub fn consecutive_failures(&self, tool_name: &str) -> u32 {
        self.consecutive_failures
            .get(tool_name)
            .copied()
            .unwrap_or(0)
    }

    /// Check if a tool is degraded.
    #[allow(dead_code)]
    pub fn is_degraded(&self, tool_name: &str) -> bool {
        self.degraded_tools.contains_key(tool_name)
    }

    /// Get all degraded tools.
    #[allow(dead_code)]
    pub fn degraded_tools(&self) -> &HashMap<String, DegradedInfo> {
        &self.degraded_tools
    }
}

/// Result of a budget check.
#[derive(Debug, Clone)]
pub enum BudgetVerdict {
    /// Call is allowed.
    Allowed,
    /// Call is allowed and a phase checkpoint is due.
    OverBudget(String),
    /// Call is rejected due to degraded tool.
    Rejected(String),
}

impl BudgetVerdict {
    #[allow(dead_code)]
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Self::Rejected(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_task_types::goal_contract::*;

    fn make_contract(phase: ContractPhase) -> GoalContract {
        GoalContract {
            objective: "test".into(),
            success_criteria: vec![],
            evidence_tiers: vec![],
            budgets: PhaseBudgets::default(),
            stop_policy: StopPolicy::FirstAuthoritativeEvidence,
            phase,
            created_at: chrono::Utc::now(),
            terminal_evidence: None,
        }
    }

    #[test]
    fn record_call_within_budget() {
        let mut enforcer = BudgetEnforcer::new();
        let contract = make_contract(ContractPhase::Recon);
        let verdict = enforcer.record_call("read", &contract);
        assert!(verdict.is_allowed());
    }

    #[test]
    fn record_result_tracks_failures() {
        let mut enforcer = BudgetEnforcer::new();
        enforcer.record_result("browser", false);
        enforcer.record_result("browser", false);
        assert!(enforcer.is_degraded("browser"));
        assert_eq!(enforcer.consecutive_failures("browser"), 2);
    }

    #[test]
    fn record_result_success_resets_failures() {
        let mut enforcer = BudgetEnforcer::new();
        enforcer.record_result("browser", false);
        enforcer.record_result("browser", true);
        assert!(!enforcer.is_degraded("browser"));
        assert_eq!(enforcer.consecutive_failures("browser"), 0);
    }

    /// A tool that reached the degraded threshold and then succeeded must be
    /// cleared, not left degraded forever.
    ///
    /// `record_result_success_resets_failures` above cannot catch a missing
    /// `degraded_tools.remove`: it only accumulates one failure before the
    /// success, so the tool was never inserted in the first place. This one
    /// crosses the threshold (2 failures), confirms it is degraded, then
    /// succeeds — which is the sequence a transient failure produces.
    #[test]
    fn success_clears_a_tool_that_was_already_degraded() {
        let mut enforcer = BudgetEnforcer::new();
        enforcer.record_result("webfetch", false);
        enforcer.record_result("webfetch", false);
        assert!(enforcer.is_degraded("webfetch"), "2 failures must degrade");

        enforcer.record_result("webfetch", true);
        assert!(
            !enforcer.is_degraded("webfetch"),
            "a recovered tool must not stay degraded"
        );
        assert_eq!(enforcer.consecutive_failures("webfetch"), 0);
    }

    /// A degraded tool is still rejected once the goal is satisfiable without
    /// it — and that must key off `contract.phase`, which is the only phase
    /// source that actually advances. `current_phase` was pinned at `Recon`
    /// forever, so this branch was previously unreachable.
    #[test]
    fn degraded_tool_is_rejected_in_the_report_phase() {
        let mut enforcer = BudgetEnforcer::new();
        enforcer.record_result("webfetch", false);
        enforcer.record_result("webfetch", false);

        let contract = make_contract(ContractPhase::Report);
        match enforcer.record_call("webfetch", &contract) {
            BudgetVerdict::Rejected(_) => {}
            other => panic!("expected Rejected in the Report phase, got {other:?}"),
        }
    }

    /// The contract's own budgets must be enforced, not the hardcoded defaults.
    #[test]
    fn configured_budget_is_an_advisory_checkpoint() {
        let mut enforcer = BudgetEnforcer::new();
        let mut contract = make_contract(ContractPhase::Execute);
        // Default `execute` budget is 20 calls; make this contract much tighter.
        contract.budgets.execute.max_calls = 2;

        assert!(enforcer.record_call("read", &contract).is_allowed());
        assert!(enforcer.record_call("read", &contract).is_allowed());
        let checkpoint = enforcer.record_call("read", &contract);
        assert!(matches!(checkpoint, BudgetVerdict::OverBudget(_)));
        assert!(checkpoint.is_allowed());
        assert!(matches!(
            enforcer.record_call("read", &contract),
            BudgetVerdict::Allowed
        ));
    }

    /// `begin_turn` must reset the per-phase counters, otherwise a long session
    /// carries one turn's progress counters into the next.
    #[test]
    fn begin_turn_resets_phase_counters() {
        let mut enforcer = BudgetEnforcer::new();
        let contract = make_contract(ContractPhase::Execute);
        for _ in 0..19 {
            enforcer.record_call("read", &contract);
        }
        enforcer.begin_turn(Some(ContractPhase::Execute));
        assert!(enforcer.record_call("read", &contract).is_allowed());
    }

    #[test]
    fn post_goal_calls_tracking() {
        let mut enforcer = BudgetEnforcer::new();
        let contract = make_contract(ContractPhase::Execute);

        // Simulate 5 calls
        for _ in 0..5 {
            enforcer.record_call("read", &contract);
        }

        // Goal achieved at call 3
        enforcer.mark_goal_achieved();

        // 2 more calls
        for _ in 0..2 {
            enforcer.record_call("read", &contract);
        }

        assert_eq!(enforcer.post_goal_calls(), 2);
        assert_eq!(enforcer.total_calls(), 7);
    }

    #[test]
    fn phase_transition_resets_counters() {
        let mut enforcer = BudgetEnforcer::new();
        let contract = make_contract(ContractPhase::Recon);

        for _ in 0..3 {
            enforcer.record_call("read", &contract);
        }

        enforcer.transition_phase(ContractPhase::Execute);
        assert_eq!(enforcer.phase_calls, 0);
    }
}
