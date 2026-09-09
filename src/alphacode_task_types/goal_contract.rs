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
    /// The model describes a detectable predicate (e.g. "lab status shows solved",
    /// "cargo test exits 0"). The evaluator matches against known patterns.
    RuntimeDetectable(String),
    /// A specific file must exist (and optionally contain a pattern).
    ExternalArtifact {
        path: PathBuf,
        pattern: Option<String>,
    },
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

    /// Check if a specific criterion is satisfied based on its check type.
    pub fn check_criterion(&self, index: usize, tool_output: &str) -> Option<bool> {
        let criterion = self.success_criteria.get(index)?;
        match &criterion.check {
            CriterionCheck::ModelAsserted => None, // can't auto-check
            CriterionCheck::RuntimeDetectable(predicate) => {
                Some(tool_output.contains(predicate.as_str()))
            }
            CriterionCheck::ExternalArtifact { pattern, .. } => {
                if let Some(pattern) = pattern {
                    Some(tool_output.contains(pattern.as_str()))
                } else {
                    Some(true) // file existence is checked externally
                }
            }
        }
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
    fn runtime_detectable_check() {
        let contract = GoalContract::new("solve lab", vec![]);
        // manually add a detectable criterion
        let mut contract = contract;
        contract.success_criteria.push(SuccessCriterion {
            statement: "lab shows solved".into(),
            check: CriterionCheck::RuntimeDetectable("is-solved".into()),
            tier: EvidenceTier::Authoritative,
        });

        assert_eq!(contract.check_criterion(0, "status: is-solved"), Some(true));
        assert_eq!(contract.check_criterion(0, "status: pending"), Some(false));
    }

    #[test]
    fn evaluate_tool_output_finds_match() {
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
            Some(0)
        );
        assert_eq!(contract.evaluate_tool_output("output:\ndone"), Some(1));
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
