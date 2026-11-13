use crate::alphacode_task_types::goal_contract::{
    EvidenceTier, GoalContract, StopPolicy, TerminalEvidence, ToolObservation,
};

/// Evaluates tool outputs against a GoalContract's success criteria to detect
/// terminal state. This is the runtime enforcement layer that replaces
/// "termination delegated to the model's vibes."
pub struct GoalEvaluator;

impl GoalEvaluator {
    /// Evaluate tool output against the contract. Returns `Some(TerminalEvidence)`
    /// if a terminal condition is met, `None` otherwise.
    pub fn evaluate(
        contract: &mut GoalContract,
        observation: &ToolObservation<'_>,
    ) -> Option<TerminalEvidence> {
        if contract.is_terminal() {
            return None; // already terminated
        }

        contract.observe(observation);
        match contract.stop_policy {
            StopPolicy::FirstAuthoritativeEvidence => Self::check_first_authoritative(contract),
            StopPolicy::AllCriteriaSatisfied => Self::check_all_satisfied(contract),
            StopPolicy::MostCriteriaSatisfied { threshold } => {
                Self::check_most_satisfied(contract, threshold)
            }
        }
    }

    /// Check if any Authoritative-tier criterion is satisfied by tool output.
    fn check_first_authoritative(contract: &GoalContract) -> Option<TerminalEvidence> {
        for (i, criterion) in contract.success_criteria.iter().enumerate() {
            if criterion.tier != EvidenceTier::Authoritative {
                continue;
            }
            if contract.criterion_verified(i) {
                return Some(TerminalEvidence {
                    criterion_index: i,
                    tier: criterion.tier,
                    description: criterion.statement.clone(),
                    observed_at: chrono::Utc::now(),
                });
            }
        }
        None
    }

    /// Check if all criteria (any tier) are satisfied.
    fn check_all_satisfied(contract: &GoalContract) -> Option<TerminalEvidence> {
        if contract.success_criteria.is_empty() {
            return None;
        }

        let all_met = contract
            .success_criteria
            .iter()
            .enumerate()
            .all(|(index, _)| contract.criterion_verified(index));

        if all_met {
            // Find the highest-tier evidence
            let highest = contract
                .success_criteria
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| c.tier)
                .map(|(i, c)| (i, c.clone()));

            if let Some((idx, criterion)) = highest {
                return Some(TerminalEvidence {
                    criterion_index: idx,
                    tier: criterion.tier,
                    description: format!(
                        "All {} criteria satisfied",
                        contract.success_criteria.len()
                    ),
                    observed_at: chrono::Utc::now(),
                });
            }
        }
        None
    }

    /// Check if a threshold of criteria are satisfied.
    fn check_most_satisfied(contract: &GoalContract, threshold: f64) -> Option<TerminalEvidence> {
        let total = contract.success_criteria.len();
        if total == 0 || !(0.0..=1.0).contains(&threshold) {
            return None;
        }
        let mut satisfied = 0;
        let highest = contract
            .success_criteria
            .iter()
            .enumerate()
            .filter(|(index, _)| contract.criterion_verified(*index))
            .inspect(|_| satisfied += 1)
            .min_by_key(|(_, criterion)| criterion.tier);
        let (criterion_index, criterion) = highest?;
        if (satisfied as f64 / total as f64) < threshold {
            return None;
        }
        Some(TerminalEvidence {
            criterion_index,
            tier: criterion.tier,
            description: format!(
                "{satisfied}/{total} criteria satisfied (threshold: {:.0}%)",
                threshold * 100.0
            ),
            observed_at: chrono::Utc::now(),
        })
    }

    /// Build a system-reminder injection for terminal state.
    pub fn terminal_reminder(evidence: &TerminalEvidence) -> String {
        format!(
            "<system-reminder>\nSuccess criterion #{} satisfied by {:?} evidence: \"{}\"\nProduce the final report now. Further tool calls will be rejected.\n</system-reminder>",
            evidence.criterion_index + 1,
            evidence.tier,
            evidence.description
        )
    }

    /// Check if a tool call should be rejected given a terminal contract.
    pub fn should_reject_tool_call(contract: &GoalContract, tool_name: &str) -> bool {
        if !contract.is_terminal() {
            return false;
        }
        // Allow report-adjacent tools but reject action tools
        !matches!(
            crate::alphacode_tool_types::resolve_tool_name(tool_name),
            "read" | "read_file" | "file_read" | "git_status" | "git_log" | "git_diff"
        )
    }
}

impl super::Agent {
    /// An interrupted or handed-off rerun has no completed observation yet.
    /// Revoke its old verification without treating a notice as new evidence.
    pub(super) fn invalidate_goal_tool_result(
        &mut self,
        call: &crate::alphacode_message_types::ToolCall,
    ) {
        if let Some(contract) = &mut self.goal_contract {
            contract.observe(&ToolObservation {
                tool_call_id: &call.id,
                tool_name: crate::alphacode_tool_types::resolve_tool_name(&call.name),
                input: &call.input,
                metadata: None,
                succeeded: false,
            });
        }
    }

    pub(super) fn terminal_goal_rejection(&self, tool_name: &str) -> Option<String> {
        self.goal_contract.as_ref().and_then(|contract| {
            GoalEvaluator::should_reject_tool_call(contract, tool_name)
                .then(|| "The goal contract is satisfied. Produce the final report now.".into())
        })
    }

    /// One admission rule for local and streaming execution. Call immediately
    /// before dispatch, since a preceding tool can satisfy the contract.
    pub(super) fn goal_tool_rejection(&mut self, tool_name: &str) -> Option<String> {
        if let Some(message) = self.terminal_goal_rejection(tool_name) {
            return Some(message);
        }
        let contract = self.goal_contract.as_ref()?;
        match self.budget_enforcer.record_call(tool_name, contract) {
            super::budget_enforcer::BudgetVerdict::Rejected(message) => Some(message),
            super::budget_enforcer::BudgetVerdict::OverBudget(message) => {
                crate::logging::info(&format!("Goal checkpoint due: {message}"));
                None
            }
            super::budget_enforcer::BudgetVerdict::Allowed => None,
        }
    }

    /// Compose goal tracking identically for streaming and non-streaming turns.
    pub(super) fn initialize_goal_contract(&mut self) {
        let mission = match crate::mission::load(&self.session.id) {
            Ok(mission) => mission,
            Err(error) => {
                crate::logging::warn(&format!("Unable to refresh goal contract: {error}"));
                return;
            }
        };
        let fresh = mission
            .filter(|mission| mission.status == crate::mission::MissionStatus::Active)
            .and_then(|mission| crate::mission::contract_from_mission(&mission));
        let unchanged = match (&self.goal_contract, &fresh) {
            (Some(current), Some(fresh)) => {
                current.objective == fresh.objective
                    && current.success_criteria == fresh.success_criteria
                    && current.stop_policy == fresh.stop_policy
            }
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self.goal_contract = fresh;
            crate::telemetry::waste_metrics::begin_contract();
        }
    }

    /// Observe native metadata, which survives output truncation. Both recorders
    /// call this after storing the result so the reminder follows its evidence.
    pub(super) fn evaluate_goal_tool_result(
        &mut self,
        call: &crate::alphacode_message_types::ToolCall,
        output: Option<&crate::alphacode_tool_types::ToolOutput>,
    ) -> Option<String> {
        let observation = ToolObservation {
            tool_call_id: &call.id,
            tool_name: crate::alphacode_tool_types::resolve_tool_name(&call.name),
            input: &call.input,
            metadata: output.and_then(|output| output.metadata.as_ref()),
            succeeded: output.is_some(),
        };
        let Some(evidence) = self
            .goal_contract
            .as_mut()
            .and_then(|contract| GoalEvaluator::evaluate(contract, &observation))
        else {
            return None;
        };
        if let Some(contract) = &mut self.goal_contract {
            contract.mark_terminal(evidence.clone());
        }
        self.budget_enforcer.mark_goal_achieved();
        crate::telemetry::waste_metrics::mark_goal_achieved(self.budget_enforcer.total_calls());
        crate::logging::info(&format!(
            "Goal evidence: criterion={} tool={} call={}",
            evidence.criterion_index, observation.tool_name, observation.tool_call_id,
        ));
        Some(GoalEvaluator::terminal_reminder(&evidence))
    }

    pub(super) fn observe_goal_tool_result(
        &mut self,
        call: &crate::alphacode_message_types::ToolCall,
        output: Option<&crate::alphacode_tool_types::ToolOutput>,
    ) {
        let reminder = self.evaluate_goal_tool_result(call, output);
        self.append_goal_reminder(reminder);
    }

    pub(super) fn append_goal_reminder(&mut self, reminder: Option<String>) {
        if let Some(reminder) = reminder {
            self.add_message(
                crate::alphacode_message_types::Role::User,
                vec![crate::alphacode_message_types::ContentBlock::Text {
                    text: reminder,
                    cache_control: None,
                }],
            );
        }
    }

    /// SDK-native requests use the same admission and evidence rules. Return
    /// the reminder inside the native result so SDK tool/result ordering stays
    /// intact rather than inserting a history message during an open call.
    pub(super) async fn execute_native_goal_tool(
        &mut self,
        request_id: String,
        tool_name: String,
        input: serde_json::Value,
    ) -> crate::alphacode_app_core::provider::NativeToolResult {
        use crate::alphacode_app_core::provider::NativeToolResult;
        use crate::alphacode_app_core::tool::{ToolContext, ToolExecutionMode};
        use crate::alphacode_message_types::ToolCall;
        let call = ToolCall {
            id: request_id.clone(),
            name: tool_name,
            input: ToolCall::normalize_input_to_object(input),
            ..ToolCall::default()
        };
        if let Some(message) = call.validation_error().or_else(|| {
            self.validate_tool_allowed(&call.name)
                .err()
                .map(|error| error.to_string())
        }) {
            return NativeToolResult::error(request_id, message);
        }
        if let Some(message) = self.goal_tool_rejection(&call.name) {
            return NativeToolResult::error(request_id, message);
        }
        let ctx = ToolContext {
            session_id: self.session.id.clone(),
            message_id: self.session.id.clone(),
            tool_call_id: request_id.clone(),
            working_dir: self.working_dir().map(std::path::PathBuf::from),
            stdin_request_tx: self.stdin_request_tx.clone(),
            graceful_shutdown_signal: Some(self.graceful_shutdown.clone()),
            execution_mode: ToolExecutionMode::AgentTurn,
        };
        crate::telemetry::record_tool_call();
        let result = self
            .registry
            .execute(&call.name, call.input.clone(), ctx)
            .await;
        self.budget_enforcer
            .record_result(&call.name, result.is_ok());
        let reminder = self.evaluate_goal_tool_result(&call, result.as_ref().ok());
        match result {
            Ok(mut output) => {
                if let Some(reminder) = reminder {
                    output.output.push_str("\n\n");
                    output.output.push_str(&reminder);
                }
                NativeToolResult::success(request_id, output.output)
            }
            Err(error) => {
                crate::telemetry::record_tool_failure();
                NativeToolResult::error(request_id, error.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_task_types::goal_contract::{CriterionCheck, SuccessCriterion};
    use serde_json::json;

    fn criterion(statement: &str, pointer: &str, tier: EvidenceTier) -> SuccessCriterion {
        SuccessCriterion {
            statement: statement.into(),
            tier,
            check: CriterionCheck::ToolMetadata {
                tool_name: "bash".into(),
                input: json!({"command": "verify"}),
                metadata_pointer: pointer.into(),
                expected: json!(0),
            },
        }
    }

    #[test]
    fn completion_requires_matching_call_and_native_metadata() {
        let mut contract = GoalContract::new("verify", vec![]);
        contract.success_criteria.push(criterion(
            "verified",
            "/process_exit_code",
            EvidenceTier::Authoritative,
        ));
        let input = json!({"command": "verify"});
        let metadata = json!({"process_exit_code": 0});
        let mut observation = ToolObservation {
            tool_call_id: "call-1",
            tool_name: "bash",
            input: &input,
            metadata: Some(&metadata),
            succeeded: true,
        };
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_some());
        observation.tool_name = "read";
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        observation.tool_name = "bash";
        observation.succeeded = false;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        observation.succeeded = true;
        observation.metadata = None;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
    }

    #[test]
    fn threshold_reports_evidence_from_a_satisfied_criterion() {
        let mut contract = GoalContract::new("verify", vec![]);
        contract.success_criteria = vec![
            criterion("unverified", "/missing", EvidenceTier::Authoritative),
            criterion("verified", "/process_exit_code", EvidenceTier::FirstParty),
        ];
        contract.stop_policy = StopPolicy::MostCriteriaSatisfied { threshold: 0.5 };
        let input = json!({"command": "verify"});
        let metadata = json!({"process_exit_code": 0});
        let observation = ToolObservation {
            tool_call_id: "call-1",
            tool_name: "bash",
            input: &input,
            metadata: Some(&metadata),
            succeeded: true,
        };
        let evidence = GoalEvaluator::evaluate(&mut contract, &observation).unwrap();
        assert_eq!(evidence.criterion_index, 1);
        assert_eq!(evidence.tier, EvidenceTier::FirstParty);
        contract.stop_policy = StopPolicy::AllCriteriaSatisfied;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        contract.stop_policy = StopPolicy::FirstAuthoritativeEvidence;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        for threshold in [-0.1, 1.1, f64::NAN] {
            contract.stop_policy = StopPolicy::MostCriteriaSatisfied { threshold };
            assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        }
    }

    #[test]
    fn all_criteria_accumulate_across_calls_and_failed_reruns_revoke_passes() {
        let mut contract = GoalContract::new("build and test", vec![]);
        let build = criterion("build", "/process_exit_code", EvidenceTier::FirstParty);
        let mut test = build.clone();
        test.statement = "test".into();
        if let CriterionCheck::ToolMetadata { input, .. } = &mut test.check {
            *input = json!({"command": "test"});
        }
        contract.success_criteria = vec![build, test];
        contract.stop_policy = StopPolicy::AllCriteriaSatisfied;
        let build_input = json!({"command": "verify"});
        let test_input = json!({"command": "test"});
        let metadata = json!({"process_exit_code": 0});
        let mut observation = ToolObservation {
            tool_call_id: "build-1",
            tool_name: "bash",
            input: &build_input,
            metadata: Some(&metadata),
            succeeded: true,
        };
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        observation.tool_call_id = "build-2";
        observation.succeeded = false;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        assert!(!contract.criterion_verified(0));
        observation.tool_call_id = "test-1";
        observation.input = &test_input;
        observation.succeeded = true;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_none());
        observation.tool_call_id = "build-3";
        observation.input = &build_input;
        assert!(GoalEvaluator::evaluate(&mut contract, &observation).is_some());
        assert_eq!(contract.criterion_evidence.len(), 2);
        contract.success_criteria[0].statement = "new requirement".into();
        assert!(!contract.criterion_verified(0));
    }

    #[test]
    fn rejects_actions_after_terminal_but_permits_report_reads() {
        let mut contract = GoalContract::new("test", vec![]);
        let evidence = TerminalEvidence {
            criterion_index: 0,
            tier: EvidenceTier::Authoritative,
            description: "verified".into(),
            observed_at: chrono::Utc::now(),
        };
        assert!(GoalEvaluator::terminal_reminder(&evidence).contains("Success criterion #1"));
        contract.mark_terminal(evidence);
        assert!(GoalEvaluator::should_reject_tool_call(&contract, "bash"));
        assert!(!GoalEvaluator::should_reject_tool_call(&contract, "read"));
    }
}
