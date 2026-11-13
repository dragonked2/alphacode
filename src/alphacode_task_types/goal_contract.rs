use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A runtime success predicate created at turn start and persisted with the
/// session. The contract converts prose objectives into machine-checkable
/// criteria so the agent loop can enforce termination without relying on the
/// model's vibes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalContract {
    /// The user's objective, copied verbatim from the mission/goal.
    pub objective: String,
    /// Machine-checkable success criteria, ranked by evidence tier.
    pub success_criteria: Vec<SuccessCriterion>,
    /// Evidence tiers ordered from strongest to weakest.
    pub evidence_tiers: Vec<EvidenceTier>,
    /// Per-phase budgets (calls + wall-clock).
    pub budgets: PhaseBudgets,
    /// When to declare the contract satisfied.
    pub stop_policy: StopPolicy,
    /// Current execution phase.
    pub phase: ContractPhase,
    /// When the contract was created.
    pub created_at: DateTime<Utc>,
    /// Highest-tier evidence that arrived, if any.
    pub terminal_evidence: Option<TerminalEvidence>,
    /// Provenance for individually verified criteria. Old snapshots start with
    /// no observations; a prose claim never upgrades itself during restore.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criterion_evidence: Vec<CriterionEvidence>,
}

/// A single success criterion with a machine-checkable predicate where possible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuccessCriterion {
    /// Human-readable description of what "done" means.
    pub statement: String,
    /// The check used to evaluate this criterion.
    pub check: CriterionCheck,
    /// Which evidence tier this criterion belongs to.
    pub tier: EvidenceTier,
}

/// How a criterion is checked at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CriterionCheck {
    /// Weakest: the model asserts it's done. No runtime verification.
    ModelAsserted,
    /// Legacy prose predicate. Retained for serialization compatibility;
    /// text matches alone cannot establish runtime completion.
    RuntimeDetectable(String),
    /// Legacy artifact request. A separate path-bound verifier must establish
    /// its result; arbitrary output mentioning the pattern is insufficient.
    ExternalArtifact {
        path: PathBuf,
        pattern: Option<String>,
    },
    /// Match metadata emitted by a completed tool, bound to its exact inputs.
    /// Text printed by the command is deliberately excluded from this check.
    ToolMetadata {
        tool_name: String,
        input: serde_json::Value,
        metadata_pointer: String,
        expected: serde_json::Value,
    },
}

/// Runtime-owned provenance for one tool result. Construct this from the
/// executed call and native result, never by parsing model-authored prose.
pub struct ToolObservation<'a> {
    pub tool_call_id: &'a str,
    pub tool_name: &'a str,
    pub input: &'a serde_json::Value,
    pub metadata: Option<&'a serde_json::Value>,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CriterionEvidence {
    pub criterion_index: usize,
    pub criterion: SuccessCriterion,
    pub tool_call_id: String,
    pub tool_name: String,
    pub observed_at: DateTime<Utc>,
}

/// The shared top-level `intent` argument is a UI label, not an execution
/// parameter. Keep every other argument exact, including nested `intent` keys.
fn verification_inputs_match(expected: &serde_json::Value, actual: &serde_json::Value) -> bool {
    match (expected.as_object(), actual.as_object()) {
        (Some(expected), Some(actual)) => {
            let expected_len = expected.len() - usize::from(expected.contains_key("intent"));
            let actual_len = actual.len() - usize::from(actual.contains_key("intent"));
            expected_len == actual_len
                && expected
                    .iter()
                    .filter(|(key, _)| key.as_str() != "intent")
                    .all(|(key, value)| actual.get(key) == Some(value))
        }
        _ => expected == actual,
    }
}

impl SuccessCriterion {
    pub fn matches_observation(&self, observation: &ToolObservation<'_>) -> bool {
        if !observation.succeeded || observation.tool_call_id.trim().is_empty() {
            return false;
        }
        match &self.check {
            CriterionCheck::ToolMetadata {
                tool_name,
                input,
                metadata_pointer,
                expected,
            } => {
                !tool_name.trim().is_empty()
                    && observation.tool_name == tool_name
                    && verification_inputs_match(input, observation.input)
                    && metadata_pointer.starts_with('/')
                    && !expected.is_null()
                    && observation
                        .metadata
                        .and_then(|metadata| metadata.pointer(metadata_pointer))
                        == Some(expected)
            }
            // Legacy prose checks remain readable in saved contracts but
            // cannot prove completion. Artifact verification needs provenance
            // for the actual path/content, not a mention in unrelated output.
            _ => false,
        }
    }
}

/// Evidence tiers: Authoritative > FirstParty > Inferred.
/// Tier-1 evidence terminates; lower tiers supplement but never delay.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceTier {
    /// Vendor/system signal: lab solve banner, CI pass, exit code 0,
    /// HTTP 201 Created. This IS the definition of done.
    Authoritative,
    /// First-party observation: file exists, test passes, build succeeds.
    FirstParty,
    /// Model assertion only. Weakest.
    Inferred,
}

/// Per-phase resource budgets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhaseBudgets {
    pub recon: Budget,
    pub execute: Budget,
    pub verify: Budget,
}

impl Default for PhaseBudgets {
    fn default() -> Self {
        Self {
            recon: Budget {
                max_calls: 5,
                max_seconds: 120,
            },
            execute: Budget {
                max_calls: 20,
                max_seconds: 300,
            },
            verify: Budget {
                max_calls: 5,
                max_seconds: 60,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Budget {
    pub max_calls: u32,
    pub max_seconds: u64,
}

/// When to stop the loop.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum StopPolicy {
    /// Stop as soon as any Authoritative-tier evidence arrives.
    #[default]
    FirstAuthoritativeEvidence,
    /// Stop when all criteria are satisfied (any tier).
    AllCriteriaSatisfied,
    /// Stop when a fraction of criteria are satisfied.
    MostCriteriaSatisfied { threshold: f64 },
}

/// Current phase of the contract lifecycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContractPhase {
    /// Gathering context, exploring the problem space.
    Recon,
    /// Implementing the solution.
    Execute,
    /// Validating the solution works.
    Verify,
    /// Producing the final report.
    Report,
}

impl ContractPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Recon => "recon",
            Self::Execute => "execute",
            Self::Verify => "verify",
            Self::Report => "report",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Recon => Self::Execute,
            Self::Execute => Self::Verify,
            Self::Verify => Self::Report,
            Self::Report => Self::Report,
        }
    }
}

/// Evidence that triggered terminal state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalEvidence {
    /// Which criterion was satisfied.
    pub criterion_index: usize,
    /// The tier of evidence.
    pub tier: EvidenceTier,
    /// Human-readable description of the evidence.
    pub description: String,
    /// When the evidence was observed.
    pub observed_at: DateTime<Utc>,
}

impl GoalContract {
    /// Create a new contract from an objective and success criteria strings.
    pub fn new(objective: &str, criteria: Vec<String>) -> Self {
        let success_criteria: Vec<SuccessCriterion> = criteria
            .into_iter()
            .map(|c| SuccessCriterion {
                statement: c,
                check: CriterionCheck::ModelAsserted,
                tier: EvidenceTier::Inferred,
            })
            .collect();

        Self {
            objective: objective.to_string(),
            success_criteria,
            evidence_tiers: vec![
                EvidenceTier::Authoritative,
                EvidenceTier::FirstParty,
                EvidenceTier::Inferred,
            ],
            budgets: PhaseBudgets::default(),
            stop_policy: StopPolicy::default(),
            phase: ContractPhase::Recon,
            created_at: Utc::now(),
            terminal_evidence: None,
            criterion_evidence: Vec::new(),
        }
    }

    /// Check if the contract has terminal evidence (highest-tier satisfied).
    pub fn is_terminal(&self) -> bool {
        self.terminal_evidence.is_some()
    }

    /// Record terminal evidence and transition to Report phase.
    pub fn mark_terminal(&mut self, evidence: TerminalEvidence) {
        self.terminal_evidence = Some(evidence);
        self.phase = ContractPhase::Report;
    }

    /// Advance to the next phase.
    pub fn advance_phase(&mut self) {
        self.phase = self.phase.next();
    }

    /// Reconcile the latest result for each matching verification call. This
    /// lets independent criteria complete in separate calls, and revokes an
    /// earlier pass when the same verification is rerun unsuccessfully.
    pub fn observe(&mut self, observation: &ToolObservation<'_>) {
        if self.is_terminal() {
            return;
        }
        // Drop stale observations after a caller changes or reorders criteria.
        self.criterion_evidence.retain(|evidence| {
            self.success_criteria.get(evidence.criterion_index) == Some(&evidence.criterion)
        });
        for (index, criterion) in self.success_criteria.iter().enumerate() {
            let (same_input, same_source) = match &criterion.check {
                CriterionCheck::ToolMetadata {
                    tool_name, input, ..
                } => (
                    verification_inputs_match(input, observation.input),
                    tool_name == observation.tool_name
                        && verification_inputs_match(input, observation.input),
                ),
                _ => (false, false),
            };
            if same_input {
                // A rerun with the same verification input is authoritative for
                // that criterion, including when it came from the wrong tool or
                // failed before producing valid metadata.
                self.criterion_evidence
                    .retain(|evidence| evidence.criterion_index != index);
            }
            if !same_source {
                continue;
            }
            if criterion.matches_observation(observation) {
                self.criterion_evidence.push(CriterionEvidence {
                    criterion_index: index,
                    criterion: criterion.clone(),
                    tool_call_id: observation.tool_call_id.to_string(),
                    tool_name: observation.tool_name.to_string(),
                    observed_at: Utc::now(),
                });
            }
        }
    }

    pub fn criterion_verified(&self, index: usize) -> bool {
        self.criterion_evidence.iter().any(|evidence| {
            evidence.criterion_index == index
                && self.success_criteria.get(index) == Some(&evidence.criterion)
                && !evidence.tool_call_id.trim().is_empty()
        })
    }

    /// Compatibility entrypoint for text-only callers. No completion check can
    /// be established without runtime provenance; use `observe` instead.
    pub fn check_criterion(&self, index: usize, _tool_output: &str) -> Option<bool> {
        self.success_criteria.get(index)?;
        // Text alone carries neither tool identity nor execution status.
        None
    }

    /// Evaluate all criteria against tool output. Returns the index of the
    /// first satisfied criterion, if any.
    pub fn evaluate_tool_output(&self, tool_output: &str) -> Option<usize> {
        for (i, _criterion) in self.success_criteria.iter().enumerate() {
            if let Some(true) = self.check_criterion(i, tool_output) {
                return Some(i);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_ignores_only_the_top_level_ui_intent() {
        use serde_json::json;
        let expected = json!({"command": "cargo test --lib"});
        for actual in [
            expected.clone(),
            json!({"command": "cargo test --lib", "intent": "Verify the fix"}),
        ] {
            assert!(verification_inputs_match(&expected, &actual));
            assert!(verification_inputs_match(&actual, &expected));
        }
        for actual in [
            json!({"command": "cargo test --lib; exit 0"}),
            json!({"command": "cargo test --lib", "working_dir": "elsewhere"}),
            json!({"command": "cargo test --lib", "timeout": 1}),
        ] {
            assert!(!verification_inputs_match(&expected, &actual));
        }
        assert!(!verification_inputs_match(
            &json!({"payload": {"intent": "original"}}),
            &json!({"payload": {"intent": "changed"}}),
        ));
    }

    #[test]
    fn relabeled_failed_verification_revokes_the_previous_pass() {
        use serde_json::json;
        let mut contract = GoalContract::new("verify", vec![]);
        contract.success_criteria.push(SuccessCriterion {
            statement: "library tests pass".into(),
            tier: EvidenceTier::FirstParty,
            check: CriterionCheck::ToolMetadata {
                tool_name: "bash".into(),
                input: json!({"command": "cargo test --lib"}),
                metadata_pointer: "/process_exit_code".into(),
                expected: json!(0),
            },
        });
        let input = json!({"command": "cargo test --lib", "intent": "Verify"});
        let metadata = json!({"process_exit_code": 0});
        contract.observe(&ToolObservation {
            tool_call_id: "first",
            tool_name: "bash",
            input: &input,
            metadata: Some(&metadata),
            succeeded: true,
        });
        assert!(contract.criterion_verified(0));
        let retry = json!({"command": "cargo test --lib", "intent": "Recheck"});
        contract.observe(&ToolObservation {
            tool_call_id: "retry",
            tool_name: "bash",
            input: &retry,
            metadata: None,
            succeeded: false,
        });
        assert!(!contract.criterion_verified(0));
        assert!(contract.criterion_evidence.is_empty());
    }

    #[test]
    fn metadata_criteria_bind_input_and_reject_missing_provenance() {
        use serde_json::json;
        let criterion = SuccessCriterion {
            statement: "the requested test command passed".into(),
            tier: EvidenceTier::FirstParty,
            check: CriterionCheck::ToolMetadata {
                tool_name: "bash".into(),
                input: json!({"command": "cargo test --lib"}),
                metadata_pointer: "/process_exit_code".into(),
                expected: json!(0),
            },
        };
        let input = json!({"command": "cargo test --lib"});
        let metadata = json!({"process_exit_code": 0});
        let mut observation = ToolObservation {
            tool_call_id: "call-1",
            tool_name: "bash",
            input: &input,
            metadata: Some(&metadata),
            succeeded: true,
        };
        assert!(criterion.matches_observation(&observation));
        let unrelated_input = json!({"command": "echo success"});
        observation.input = &unrelated_input;
        assert!(!criterion.matches_observation(&observation));
        observation.input = &input;
        observation.tool_call_id = "";
        assert!(!criterion.matches_observation(&observation));
        observation.tool_call_id = "call-1";
        let null_metadata = json!({"process_exit_code": null});
        observation.metadata = Some(&null_metadata);
        assert!(!criterion.matches_observation(&observation));
    }

    #[test]
    fn evidence_roundtrip_preserves_provenance_and_old_snapshots_start_empty() {
        use serde_json::json;
        let mut contract = GoalContract::new("verify", vec![]);
        contract.success_criteria.push(SuccessCriterion {
            statement: "verification passed".into(),
            tier: EvidenceTier::FirstParty,
            check: CriterionCheck::ToolMetadata {
                tool_name: "bash".into(),
                input: json!({"command": "verify"}),
                metadata_pointer: "/process_exit_code".into(),
                expected: json!(0),
            },
        });
        let input = json!({"command": "verify"});
        let metadata = json!({"process_exit_code": 0});
        contract.observe(&ToolObservation {
            tool_call_id: "call-1",
            tool_name: "bash",
            input: &input,
            metadata: Some(&metadata),
            succeeded: true,
        });
        let mut saved = serde_json::to_value(&contract).unwrap();
        let restored: GoalContract = serde_json::from_value(saved.clone()).unwrap();
        assert!(restored.criterion_verified(0));
        assert_eq!(restored.criterion_evidence[0].tool_call_id, "call-1");
        saved.as_object_mut().unwrap().remove("criterion_evidence");
        let legacy: GoalContract = serde_json::from_value(saved).unwrap();
        assert!(!legacy.criterion_verified(0));
    }

    #[test]
    fn new_contract_defaults() {
        let contract = GoalContract::new("fix the bug", vec!["test passes".into()]);
        assert_eq!(contract.objective, "fix the bug");
        assert_eq!(contract.success_criteria.len(), 1);
        assert_eq!(contract.phase, ContractPhase::Recon);
        assert!(!contract.is_terminal());
    }

    #[test]
    fn phase_advance() {
        assert_eq!(ContractPhase::Recon.next(), ContractPhase::Execute);
        assert_eq!(ContractPhase::Execute.next(), ContractPhase::Verify);
        assert_eq!(ContractPhase::Verify.next(), ContractPhase::Report);
        assert_eq!(ContractPhase::Report.next(), ContractPhase::Report);
    }

    #[test]
    fn legacy_runtime_text_is_not_completion_evidence() {
        let contract = GoalContract::new("solve lab", vec![]);
        // manually add a detectable criterion
        let mut contract = contract;
        contract.success_criteria.push(SuccessCriterion {
            statement: "lab shows solved".into(),
            check: CriterionCheck::RuntimeDetectable("is-solved".into()),
            tier: EvidenceTier::Authoritative,
        });

        assert_eq!(contract.check_criterion(0, "status: is-solved"), None);
        assert_eq!(contract.check_criterion(0, "status: pending"), None);
    }

    #[test]
    fn arbitrary_text_cannot_verify_runtime_or_artifact_criteria() {
        let mut contract = GoalContract::new("test", vec![]);
        contract.success_criteria.push(SuccessCriterion {
            statement: "exit 0".into(),
            check: CriterionCheck::RuntimeDetectable("exit code: 0".into()),
            tier: EvidenceTier::Authoritative,
        });
        contract.success_criteria.push(SuccessCriterion {
            statement: "file exists".into(),
            check: CriterionCheck::ExternalArtifact {
                path: PathBuf::from("output.txt"),
                pattern: Some("done".into()),
            },
            tier: EvidenceTier::FirstParty,
        });

        assert_eq!(
            contract.evaluate_tool_output("running tests...\nexit code: 0"),
            None
        );
        assert_eq!(contract.evaluate_tool_output("output:\ndone"), None);
        assert_eq!(contract.evaluate_tool_output("nothing here"), None);
    }

    #[test]
    fn mark_terminal_sets_phase() {
        let mut contract = GoalContract::new("test", vec![]);
        assert_eq!(contract.phase, ContractPhase::Recon);

        contract.mark_terminal(TerminalEvidence {
            criterion_index: 0,
            tier: EvidenceTier::Authoritative,
            description: "lab solved".into(),
            observed_at: Utc::now(),
        });

        assert!(contract.is_terminal());
        assert_eq!(contract.phase, ContractPhase::Report);
    }
}
