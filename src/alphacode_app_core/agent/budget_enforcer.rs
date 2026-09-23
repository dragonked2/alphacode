use std::collections::HashMap;
use std::time::Instant;

use crate::alphacode_task_types::goal_contract::{ContractPhase, GoalContract, PhaseBudgets};

/// Tracks per-phase call counts and wall-clock time, and enforces budgets.
/// Also tracks consecutive failures per tool for the auxiliary-tool stop-loss.
pub struct BudgetEnforcer {
    /// Start time of the current phase.
    phase_start: Instant,
    /// Call count in the current phase.
    phase_calls: u32,
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
            current_phase: ContractPhase::Recon,
            consecutive_failures: HashMap::new(),
            degraded_tools: HashMap::new(),
            total_calls: 0,
            goal_achieved_at_call: None,
        }
    }

    /// Record a tool call. Returns `true` if the call is allowed, `false` if
    /// it should be rejected (budget overrun or degraded tool).
    pub fn record_call(&mut self, tool_name: &str, contract: &GoalContract) -> BudgetVerdict {
        self.total_calls += 1;
        self.phase_calls += 1;

        // Check if the tool is degraded
        if let Some(info) = self.degraded_tools.get(tool_name) {
            // Only reject if the contract is satisfiable without this tool
            if contract.is_terminal() || self.current_phase == ContractPhase::Report {
                return BudgetVerdict::Rejected(format!(
                    "Tool '{}' is degraded ({} consecutive failures). Goal is already satisfiable without it.",
                    tool_name, info.failures
                ));
            }
        }

        // Check phase budget
        let budget = self.budget_for_phase(contract.phase);
        let elapsed = self.phase_start.elapsed();

        if self.phase_calls > budget.max_calls {
            return BudgetVerdict::OverBudget(format!(
                "Phase '{}' exceeded budget: {} calls (max {})",
                contract.phase.as_str(),
                self.phase_calls,
                budget.max_calls
            ));
        }

        if elapsed.as_secs() > budget.max_seconds {
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
            self.phase_start = Instant::now();
        }
    }

    /// Get the budget for the current phase.
    fn budget_for_phase(
        &self,
        phase: ContractPhase,
    ) -> crate::alphacode_task_types::goal_contract::Budget {
        let defaults = PhaseBudgets::default();
        match phase {
            ContractPhase::Recon => defaults.recon,
            ContractPhase::Execute => defaults.execute,
            ContractPhase::Verify => defaults.verify,
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
        let verify_budget = PhaseBudgets::default().verify;
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
    /// Call is rejected due to budget overrun.
    OverBudget(String),
    /// Call is rejected due to degraded tool.
    Rejected(String),
}

impl BudgetVerdict {
    #[allow(dead_code)]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
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
