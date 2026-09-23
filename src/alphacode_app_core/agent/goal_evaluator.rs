use crate::alphacode_task_types::goal_contract::{
    CriterionCheck, EvidenceTier, GoalContract, StopPolicy, TerminalEvidence,
};

/// Evaluates tool outputs against a GoalContract's success criteria to detect
/// terminal state. This is the runtime enforcement layer that replaces
/// "termination delegated to the model's vibes."
pub struct GoalEvaluator;

impl GoalEvaluator {
    /// Evaluate tool output against the contract. Returns `Some(TerminalEvidence)`
    /// if a terminal condition is met, `None` otherwise.
    pub fn evaluate(
        contract: &GoalContract,
        _tool_name: &str,
        tool_output: &str,
    ) -> Option<TerminalEvidence> {
        if contract.is_terminal() {
            return None; // already terminated
        }

        match contract.stop_policy {
            StopPolicy::FirstAuthoritativeEvidence => {
                Self::check_first_authoritative(contract, tool_output)
            }
            StopPolicy::AllCriteriaSatisfied => Self::check_all_satisfied(contract, tool_output),
            StopPolicy::MostCriteriaSatisfied { threshold } => {
                Self::check_most_satisfied(contract, tool_output, threshold)
            }
        }
    }

    /// Check if any Authoritative-tier criterion is satisfied by tool output.
    fn check_first_authoritative(
        contract: &GoalContract,
        tool_output: &str,
    ) -> Option<TerminalEvidence> {
        for (i, criterion) in contract.success_criteria.iter().enumerate() {
            if criterion.tier != EvidenceTier::Authoritative {
                continue;
            }
            if Self::criterion_satisfied(criterion, tool_output) {
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
    fn check_all_satisfied(contract: &GoalContract, tool_output: &str) -> Option<TerminalEvidence> {
        if contract.success_criteria.is_empty() {
            return None;
        }

        let all_met = contract
            .success_criteria
            .iter()
            .all(|c| Self::criterion_satisfied(c, tool_output));

        if all_met {
            // Find the highest-tier evidence
            let highest = contract
                .success_criteria
                .iter()
                .enumerate()
                .max_by_key(|(_, c)| c.tier)
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
    fn check_most_satisfied(
        contract: &GoalContract,
        tool_output: &str,
        threshold: f64,
    ) -> Option<TerminalEvidence> {
        if contract.success_criteria.is_empty() {
            return None;
        }

        let satisfied = contract
            .success_criteria
            .iter()
            .filter(|c| Self::criterion_satisfied(c, tool_output))
            .count();

        let ratio = satisfied as f64 / contract.success_criteria.len() as f64;
        if ratio >= threshold {
            let highest = contract
                .success_criteria
                .iter()
                .enumerate()
                .max_by_key(|(_, c)| c.tier)
                .map(|(i, c)| (i, c.clone()));

            if let Some((idx, criterion)) = highest {
                return Some(TerminalEvidence {
                    criterion_index: idx,
                    tier: criterion.tier,
                    description: format!(
                        "{}/{} criteria satisfied (threshold: {:.0}%)",
                        satisfied,
                        contract.success_criteria.len(),
                        threshold * 100.0
                    ),
                    observed_at: chrono::Utc::now(),
                });
            }
        }
        None
    }

    /// Check if a single criterion is satisfied by tool output.
    fn criterion_satisfied(
        criterion: &crate::alphacode_task_types::goal_contract::SuccessCriterion,
        tool_output: &str,
    ) -> bool {
        match &criterion.check {
            CriterionCheck::ModelAsserted => false, // can't auto-verify
            CriterionCheck::RuntimeDetectable(predicate) => {
                tool_output.contains(predicate.as_str())
            }
            CriterionCheck::ExternalArtifact { pattern, .. } => {
                if let Some(pattern) = pattern {
                    tool_output.contains(pattern.as_str())
                } else {
                    false // file existence needs filesystem check
                }
            }
        }
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
            tool_name,
            "read" | "read_file" | "file_read" | "git_status" | "git_log" | "git_diff"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_task_types::goal_contract::*;

    fn make_contract(criteria: Vec<(&str, CriterionCheck, EvidenceTier)>) -> GoalContract {
        let success_criteria: Vec<SuccessCriterion> = criteria
            .into_iter()
            .map(|(stmt, check, tier)| SuccessCriterion {
                statement: stmt.into(),
                check,
                tier,
            })
            .collect();

        GoalContract {
            objective: "test".into(),
            success_criteria,
            evidence_tiers: vec![
                EvidenceTier::Authoritative,
                EvidenceTier::FirstParty,
                EvidenceTier::Inferred,
            ],
            budgets: PhaseBudgets::default(),
            stop_policy: StopPolicy::FirstAuthoritativeEvidence,
            phase: ContractPhase::Execute,
            created_at: chrono::Utc::now(),
            terminal_evidence: None,
        }
    }

    #[test]
    fn detects_authoritative_evidence() {
        let contract = make_contract(vec![
            (
                "lab shows solved",
                CriterionCheck::RuntimeDetectable("is-solved".into()),
                EvidenceTier::Authoritative,
            ),
            (
                "test passes",
                CriterionCheck::RuntimeDetectable("test result: ok".into()),
                EvidenceTier::FirstParty,
            ),
        ]);

        let evidence =
            GoalEvaluator::evaluate(&contract, "bash", "running lab...\nis-solved: true");

        assert!(evidence.is_some());
        let ev = evidence.unwrap();
        assert_eq!(ev.tier, EvidenceTier::Authoritative);
        assert_eq!(ev.criterion_index, 0);
    }

    #[test]
    fn no_termination_on_first_party_only() {
        let contract = make_contract(vec![
            (
                "lab shows solved",
                CriterionCheck::RuntimeDetectable("is-solved".into()),
                EvidenceTier::Authoritative,
            ),
            (
                "test passes",
                CriterionCheck::RuntimeDetectable("test result: ok".into()),
                EvidenceTier::FirstParty,
            ),
        ]);

        let evidence = GoalEvaluator::evaluate(&contract, "bash", "test result: ok");

        // First-party evidence does NOT terminate with FirstAuthoritativeEvidence policy
        assert!(evidence.is_none());
    }

    #[test]
    fn all_satisfied_policy() {
        let mut contract = make_contract(vec![
            (
                "lab shows solved",
                CriterionCheck::RuntimeDetectable("is-solved".into()),
                EvidenceTier::Authoritative,
            ),
            (
                "test passes",
                CriterionCheck::RuntimeDetectable("test result: ok".into()),
                EvidenceTier::FirstParty,
            ),
        ]);
        contract.stop_policy = StopPolicy::AllCriteriaSatisfied;

        let evidence =
            GoalEvaluator::evaluate(&contract, "bash", "is-solved: true\ntest result: ok");

        assert!(evidence.is_some());
    }

    #[test]
    fn rejects_tool_call_after_terminal() {
        let mut contract = GoalContract::new("test", vec![]);
        contract.mark_terminal(TerminalEvidence {
            criterion_index: 0,
            tier: EvidenceTier::Authoritative,
            description: "done".into(),
            observed_at: chrono::Utc::now(),
        });

        assert!(GoalEvaluator::should_reject_tool_call(&contract, "bash"));
        assert!(GoalEvaluator::should_reject_tool_call(
            &contract, "webfetch"
        ));
        assert!(!GoalEvaluator::should_reject_tool_call(&contract, "read")); // read is allowed
    }

    #[test]
    fn terminal_reminder_format() {
        let ev = TerminalEvidence {
            criterion_index: 0,
            tier: EvidenceTier::Authoritative,
            description: "lab solved".into(),
            observed_at: chrono::Utc::now(),
        };
        let reminder = GoalEvaluator::terminal_reminder(&ev);
        assert!(reminder.contains("Success criterion #1"));
        assert!(reminder.contains("Authoritative"));
        assert!(reminder.contains("lab solved"));
    }
}
