use serde::{Deserialize, Serialize};

/// Multi-dimensional verification quality — replaces a single fake-precise
/// confidence number. The final verdict is derived, never fabricated.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct VerificationQuality {
    /// 0.0-1.0: can an independent agent reproduce it?
    pub reproducibility: f32,
    /// 0.0-1.0: how strong is the supporting evidence?
    pub evidence_strength: f32,
    /// 0.0-1.0: is impact demonstrated rather than assumed?
    pub impact_demonstration: f32,
    /// 0.0-1.0: is there exploitability evidence (not theory)?
    pub exploitability_evidence: f32,
    /// 0.0-1.0: was it independently verified?
    pub independence: f32,
    /// 0.0-1.0: level of contradiction (higher = worse).
    pub contradiction_level: f32,
    /// Number of unverified assumptions the claim depends on.
    pub assumption_count: u32,
}

impl VerificationQuality {
    pub fn verification_state(&self) -> VerificationState {
        if self.contradiction_level > 0.5 {
            return VerificationState::Disputed;
        }
        if self.assumption_count > 3 {
            return VerificationState::AssumptionHeavy;
        }
        let score = self.score();
        if score >= 0.85 && self.reproducibility >= 0.7 && self.independence > 0.0 {
            VerificationState::VerifiedReportable
        } else if score >= 0.6 {
            VerificationState::StrongCandidate
        } else if score >= 0.35 {
            VerificationState::WeakCandidate
        } else {
            VerificationState::InsufficientEvidence
        }
    }

    pub fn score(&self) -> f32 {
        let positive = self.reproducibility * 0.25
            + self.evidence_strength * 0.25
            + self.impact_demonstration * 0.2
            + self.exploitability_evidence * 0.15
            + self.independence * 0.15;
        let penalty =
            self.contradiction_level * 0.3 + (self.assumption_count as f32 * 0.05).min(0.3);
        (positive - penalty).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    InsufficientEvidence,
    WeakCandidate,
    StrongCandidate,
    VerifiedReportable,
    Disputed,
    AssumptionHeavy,
}

/// Explicit negative reasoning: what would make this NOT a vulnerability?
/// A candidate becomes stronger by surviving attempts to disprove it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NegativeHypothesis {
    PublicByDesign,
    NonSensitive,
    Unreachable,
    NonExploitable,
    RequiresImpossibleConditions,
    ClientOnlyBehavior,
    InformationalExposure,
    RateLimited,
    AlreadyProtected,
    ExpectedApiBehavior,
    ScannerArtifact,
    EnvironmentArtifact,
}

impl NegativeHypothesis {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PublicByDesign => "public_by_design",
            Self::NonSensitive => "non_sensitive",
            Self::Unreachable => "unreachable",
            Self::NonExploitable => "non_exploitable",
            Self::RequiresImpossibleConditions => "requires_impossible_conditions",
            Self::ClientOnlyBehavior => "client_only_behavior",
            Self::InformationalExposure => "informational_exposure",
            Self::RateLimited => "rate_limited",
            Self::AlreadyProtected => "already_protected",
            Self::ExpectedApiBehavior => "expected_api_behavior",
            Self::ScannerArtifact => "scanner_artifact",
            Self::EnvironmentArtifact => "environment_artifact",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::PublicByDesign,
            Self::NonSensitive,
            Self::Unreachable,
            Self::NonExploitable,
            Self::RequiresImpossibleConditions,
            Self::ClientOnlyBehavior,
            Self::InformationalExposure,
            Self::RateLimited,
            Self::AlreadyProtected,
            Self::ExpectedApiBehavior,
            Self::ScannerArtifact,
            Self::EnvironmentArtifact,
        ]
    }

    /// Cheap deterministic test hint for each negative hypothesis.
    pub fn discriminating_test(&self) -> &'static str {
        match self {
            Self::PublicByDesign => "compare unauthenticated vs authenticated; check docs",
            Self::NonSensitive => "verify data sensitivity with second source",
            Self::Unreachable => "confirm route reachable from attacker network",
            Self::NonExploitable => "attempt benign proof-of-impact, not theory",
            Self::RequiresImpossibleConditions => "list preconditions; test each",
            Self::ClientOnlyBehavior => "confirm server-side effect, not just DOM",
            Self::InformationalExposure => "check for security boundary crossing",
            Self::RateLimited => "test repeatability under rate limits",
            Self::AlreadyProtected => "verify WAF/auth actually blocks exploit",
            Self::ExpectedApiBehavior => "compare against API spec / baseline",
            Self::ScannerArtifact => "reproduce manually without scanner",
            Self::EnvironmentArtifact => "reproduce in clean environment",
        }
    }
}

/// Result of testing negative hypotheses against a candidate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FalsePositiveDefense {
    pub tested: Vec<NegativeHypothesis>,
    pub survived: Vec<NegativeHypothesis>,
    pub refuted_by: Vec<NegativeHypothesis>,
}

impl FalsePositiveDefense {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_survived(&mut self, h: NegativeHypothesis) {
        if !self.tested.contains(&h) {
            self.tested.push(h.clone());
        }
        if !self.survived.contains(&h) {
            self.survived.push(h);
        }
    }

    pub fn record_refuted(&mut self, h: NegativeHypothesis) {
        if !self.tested.contains(&h) {
            self.tested.push(h.clone());
        }
        if !self.refuted_by.contains(&h) {
            self.refuted_by.push(h);
        }
    }

    /// A finding that was refuted by any negative hypothesis is not reportable.
    pub fn is_still_viable(&self) -> bool {
        self.refuted_by.is_empty()
    }

    pub fn defense_score(&self) -> f32 {
        if !self.is_still_viable() {
            return 0.0;
        }
        (self.survived.len() as f32 / NegativeHypothesis::all().len() as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_score_derives_state() {
        let q = VerificationQuality {
            reproducibility: 0.9,
            evidence_strength: 0.9,
            impact_demonstration: 0.9,
            exploitability_evidence: 0.8,
            independence: 0.8,
            contradiction_level: 0.0,
            assumption_count: 0,
        };
        assert_eq!(
            q.verification_state(),
            VerificationState::VerifiedReportable
        );
    }

    #[test]
    fn contradiction_forces_disputed() {
        let q = VerificationQuality {
            contradiction_level: 0.8,
            ..Default::default()
        };
        assert_eq!(q.verification_state(), VerificationState::Disputed);
    }

    #[test]
    fn negative_hypothesis_defense() {
        let mut d = FalsePositiveDefense::new();
        d.record_survived(NegativeHypothesis::PublicByDesign);
        assert!(d.is_still_viable());
        d.record_refuted(NegativeHypothesis::ScannerArtifact);
        assert!(!d.is_still_viable());
        assert_eq!(d.defense_score(), 0.0);
    }
}
