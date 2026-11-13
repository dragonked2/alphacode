use serde::{Deserialize, Serialize};

/// Structured multi-agent arbitration — never a blind "which agent is correct?"
/// LLM vote. Each contribution is scored on evidence quality dimensions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ContributionScore {
    pub agent_id: String,
    pub evidence_quality: f32,
    pub provenance_present: bool,
    pub reproducibility: f32,
    pub consistency: f32,
    pub hypothesis_support: f32,
    pub contradiction_count: u32,
    pub historical_reliability: f32,
    pub task_relevance: f32,
}

impl ContributionScore {
    pub fn overall(&self) -> f32 {
        let prov = if self.provenance_present { 0.1 } else { -0.2 };
        (self.evidence_quality * 0.3
            + self.reproducibility * 0.2
            + self.consistency * 0.15
            + self.hypothesis_support * 0.15
            + self.historical_reliability * 0.1
            + self.task_relevance * 0.1
            + prov
            - self.contradiction_count as f32 * 0.1)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ArbitrationVerdict {
    Supported,
    Contradicted,
    InsufficientEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArbitratedContribution {
    pub agent_id: String,
    pub verdict: ArbitrationVerdict,
    pub score: f32,
    pub reason: String,
}

/// Arbitrate without forcing premature consensus. Deterministic.
pub fn arbitrate(contributions: &[ContributionScore]) -> Vec<ArbitratedContribution> {
    contributions
        .iter()
        .map(|c| {
            let score = c.overall();
            let verdict = if score >= 0.6 && c.contradiction_count == 0 {
                ArbitrationVerdict::Supported
            } else if score < 0.35 || c.contradiction_count >= 2 {
                ArbitrationVerdict::Contradicted
            } else {
                ArbitrationVerdict::InsufficientEvidence
            };
            ArbitratedContribution {
                agent_id: c.agent_id.clone(),
                verdict,
                score,
                reason: format!(
                    "evidence={:.2} repro={:.2} contradictions={}",
                    c.evidence_quality, c.reproducibility, c.contradiction_count
                ),
            }
        })
        .collect()
}

/// Independent verifier: job is to DISPROVE, not confirm.
/// Gets evidence but not the original reasoning chain.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VerifierAssignment {
    pub finding_id: String,
    pub evidence_ids: Vec<String>,
    pub disallowed_context: Vec<String>,
    pub required_steps: Vec<String>,
}

impl VerifierAssignment {
    pub fn standard(finding_id: String, evidence_ids: Vec<String>) -> Self {
        Self {
            finding_id,
            evidence_ids,
            disallowed_context: vec!["original_chain_of_thought".to_string()],
            required_steps: vec![
                "reproduce independently".to_string(),
                "search alternative explanations".to_string(),
                "attempt disproof".to_string(),
                "control comparison".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VerifierVerdict {
    pub finding_id: String,
    pub verifier: String,
    pub reproduced: bool,
    pub alternative_explanations: Vec<String>,
    pub disproof_attempts: Vec<String>,
    pub disproof_successful: bool,
    pub final_verdict: ArbitrationVerdict,
}

impl VerifierVerdict {
    pub fn decide(
        finding_id: String,
        verifier: String,
        reproduced: bool,
        alternative_explanations: Vec<String>,
        disproof_attempts: Vec<String>,
    ) -> Self {
        // A *successful disproof* requires that the verifier actually
        // reproduced the issue and then found a way to explain it away. The
        // old expression folded "could not reproduce" into `disproof_successful`
        // and mapped both to `Contradicted`, which made the
        // `InsufficientEvidence` arm unreachable — a flaky test or a rate
        // limit was recorded as a hard refutation, and downstream consumers
        // treat `Contradicted` as proof the finding is bogus.
        // The number of unsuccessful attempts says nothing about whether a
        // reproduction was disproven. Only a recorded explanatory result can
        // contradict it; timeouts and missing prerequisites remain attempts.
        let disproof_successful = reproduced
            && alternative_explanations
                .iter()
                .any(|explanation| !explanation.trim().is_empty());
        let final_verdict = if reproduced && !disproof_successful {
            ArbitrationVerdict::Supported
        } else if reproduced {
            // Reproduced, but explained away -> actively contradicted.
            ArbitrationVerdict::Contradicted
        } else {
            // Not reproduced: we do not know, which is not the same as false.
            ArbitrationVerdict::InsufficientEvidence
        };
        Self {
            finding_id,
            verifier,
            reproduced,
            alternative_explanations,
            disproof_attempts,
            disproof_successful,
            final_verdict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_disproof_attempts_never_contradict_a_reproduction() {
        let verdict = VerifierVerdict::decide(
            "f1".into(),
            "independent-verifier".into(),
            true,
            vec![],
            vec![
                "timeout".into(),
                "expired session".into(),
                "missing route".into(),
                "no change under a working control".into(),
            ],
        );
        assert!(!verdict.disproof_successful);
        assert_eq!(verdict.final_verdict, ArbitrationVerdict::Supported);
    }

    #[test]
    fn arbitration_supports_strong_evidence() {
        let c = ContributionScore {
            agent_id: "a".into(),
            evidence_quality: 0.9,
            provenance_present: true,
            reproducibility: 0.9,
            consistency: 0.8,
            hypothesis_support: 0.8,
            contradiction_count: 0,
            historical_reliability: 0.7,
            task_relevance: 0.9,
        };
        let out = arbitrate(&[c]);
        assert_eq!(out[0].verdict, ArbitrationVerdict::Supported);
    }

    #[test]
    fn arbitration_contradicts_on_contradictions() {
        let c = ContributionScore {
            agent_id: "b".into(),
            evidence_quality: 0.4,
            provenance_present: false,
            reproducibility: 0.2,
            consistency: 0.3,
            hypothesis_support: 0.2,
            contradiction_count: 3,
            historical_reliability: 0.5,
            task_relevance: 0.5,
        };
        let out = arbitrate(&[c]);
        assert_eq!(out[0].verdict, ArbitrationVerdict::Contradicted);
    }

    #[test]
    fn verifier_must_reproduce() {
        // Not reproduced is *unknown*, not disproven. Recording it as
        // `Contradicted` made a flaky test or a rate limit indistinguishable
        // from a genuine refutation, and `Contradicted` is consumed as proof
        // the finding is bogus.
        let v = VerifierVerdict::decide(
            "f1".into(),
            "v1".into(),
            false,
            vec![],
            vec!["tried".into()],
        );
        assert_eq!(v.final_verdict, ArbitrationVerdict::InsufficientEvidence);
        assert!(
            !v.disproof_successful,
            "inability to reproduce is not a successful disproof"
        );
        let ok = VerifierVerdict::decide("f1".into(), "v1".into(), true, vec![], vec![]);
        assert_eq!(ok.final_verdict, ArbitrationVerdict::Supported);

        // Reproduced but explained away is a real refutation.
        let explained = VerifierVerdict::decide(
            "f1".into(),
            "v1".into(),
            true,
            vec!["the endpoint requires an internal network".into()],
            vec![],
        );
        assert!(explained.disproof_successful);
        assert_eq!(explained.final_verdict, ArbitrationVerdict::Contradicted);
    }

    /// Reproduced findings survive unsuccessful disproof attempts regardless
    /// of their count. Attempt volume is not evidence of a refutation.
    #[test]
    fn reproduced_and_lightly_attacked_is_still_supported() {
        let light = VerifierVerdict::decide(
            "f1".into(),
            "v1".into(),
            true,
            vec![],
            vec!["a".into(), "b".into()],
        );
        assert!(!light.disproof_successful);
        assert_eq!(light.final_verdict, ArbitrationVerdict::Supported);

        // More attempts without an explanatory result still do not refute it.
        let thorough = VerifierVerdict::decide(
            "f1".into(),
            "v1".into(),
            true,
            vec![],
            vec!["a".into(), "b".into(), "c".into(), "d".into()],
        );
        assert!(!thorough.disproof_successful);
        assert_eq!(thorough.final_verdict, ArbitrationVerdict::Supported);
    }
}
