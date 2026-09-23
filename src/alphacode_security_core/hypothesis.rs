use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use super::finding::{Confidence, Severity, VulnerabilityClass};

/// A testable security assumption about the target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub description: String,
    /// What vulnerability class this hypothesis relates to.
    pub vuln_class: VulnerabilityClass,
    /// What evidence triggered this hypothesis.
    pub trigger_evidence: String,
    /// Which specific endpoint/host/parameter is hypothesized to be vulnerable.
    pub target_component: String,
    /// Current confidence in this hypothesis.
    pub confidence: Confidence,
    /// Current status.
    pub status: HypothesisStatus,
    /// Skills that should be loaded to test this hypothesis.
    pub relevant_skills: Vec<String>,
    /// Which agent type should investigate this.
    pub recommended_agent: Option<String>,
    /// Evidence collected while testing this hypothesis.
    pub evidence: Vec<HypothesisEvidence>,
    /// Result if tested.
    pub result: Option<HypothesisResult>,
    /// Timestamp of creation.
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisStatus {
    Proposed,
    SelectedForTesting,
    InProgress,
    Supported,
    Refuted,
    InsufficientData,
}

impl HypothesisStatus {
    pub fn as_str(&self) -> Cow<'_, str> {
        match self {
            Self::Proposed => Cow::Borrowed("proposed"),
            Self::SelectedForTesting => Cow::Borrowed("selected_for_testing"),
            Self::InProgress => Cow::Borrowed("in_progress"),
            Self::Supported => Cow::Borrowed("supported"),
            Self::Refuted => Cow::Borrowed("refuted"),
            Self::InsufficientData => Cow::Borrowed("insufficient_data"),
        }
    }
}

/// A piece of evidence collected during hypothesis testing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HypothesisEvidence {
    pub id: String,
    pub description: String,
    pub evidence_type: EvidenceType,
    pub data: String,
    pub supports_hypothesis: Option<bool>,
    pub collected_by: Option<String>,
    pub timestamp: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    HttpRequest,
    HttpResponse,
    Error,
    Observation,
    Screenshot,
    Payload,
    Analysis,
    ToolOutput,
}

impl EvidenceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HttpRequest => "http_request",
            Self::HttpResponse => "http_response",
            Self::Error => "error",
            Self::Observation => "observation",
            Self::Screenshot => "screenshot",
            Self::Payload => "payload",
            Self::Analysis => "analysis",
            Self::ToolOutput => "tool_output",
        }
    }
}

/// Result of testing a hypothesis.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HypothesisResult {
    pub outcome: HypothesisOutcome,
    pub severity: Option<Severity>,
    pub confidence: Confidence,
    pub reasoning: String,
    pub reproduction_possible: bool,
    pub finding_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisOutcome {
    Confirmed,
    PartiallyConfirmed,
    Refuted,
    Inconclusive,
    RequiresFurtherTesting,
}

impl HypothesisOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::PartiallyConfirmed => "partially_confirmed",
            Self::Refuted => "refuted",
            Self::Inconclusive => "inconclusive",
            Self::RequiresFurtherTesting => "requires_further_testing",
        }
    }
}

impl Hypothesis {
    pub fn new(
        id: String,
        description: String,
        vuln_class: VulnerabilityClass,
        target_component: String,
        trigger_evidence: String,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            description,
            vuln_class,
            trigger_evidence,
            target_component,
            confidence: Confidence::Low,
            status: HypothesisStatus::Proposed,
            relevant_skills: Vec::new(),
            recommended_agent: None,
            evidence: Vec::new(),
            result: None,
            created_at: now,
        }
    }

    /// Add evidence and auto-update confidence based on evidence balance.
    pub fn add_evidence(&mut self, evidence: HypothesisEvidence) {
        self.evidence.push(evidence);
        self.recalculate_confidence();
    }

    /// Recalculate confidence based on the balance of supporting/refuting evidence.
    fn recalculate_confidence(&mut self) {
        let supporting = self
            .evidence
            .iter()
            .filter(|e| e.supports_hypothesis == Some(true))
            .count();
        let refuting = self
            .evidence
            .iter()
            .filter(|e| e.supports_hypothesis == Some(false))
            .count();
        let total = supporting + refuting;

        if total == 0 {
            self.confidence = Confidence::Low;
            return;
        }

        if refuting > 0 {
            // Any refuting evidence caps confidence until resolved.
            let ratio = supporting as f64 / total as f64;
            self.confidence = if ratio >= 0.66 {
                Confidence::Medium
            } else {
                Confidence::Low
            };
            return;
        }

        // Only supporting evidence so far: require corroboration.
        self.confidence = match supporting {
            1 => Confidence::Low,
            2 => Confidence::Medium,
            3..=4 => Confidence::High,
            _ => Confidence::Certain,
        };
    }

    /// Check if this hypothesis should be promoted to a finding candidate.
    pub fn should_promote(&self) -> bool {
        matches!(self.status, HypothesisStatus::Supported)
            && matches!(
                self.confidence,
                Confidence::Medium | Confidence::High | Confidence::Certain
            )
            && self
                .result
                .as_ref()
                .map(|r| r.reproduction_possible)
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypothesis_confidence_updates_with_evidence() {
        let mut h = Hypothesis::new(
            "h1".to_string(),
            "Reflected XSS in search".to_string(),
            VulnerabilityClass::Xss,
            "/search?q=".to_string(),
            "Input appears in response without encoding".to_string(),
        );
        assert_eq!(h.confidence, Confidence::Low);

        h.add_evidence(HypothesisEvidence {
            id: "e1".to_string(),
            description: "Payload executes".to_string(),
            evidence_type: EvidenceType::Payload,
            data: "<script>alert(1)</script>".to_string(),
            supports_hypothesis: Some(true),
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        // Single evidence is not enough for High — requires corroboration.
        assert_eq!(h.confidence, Confidence::Low);

        h.add_evidence(HypothesisEvidence {
            id: "e2".to_string(),
            description: "Second independent confirmation".to_string(),
            evidence_type: EvidenceType::HttpResponse,
            data: "payload reflected unencoded".to_string(),
            supports_hypothesis: Some(true),
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        assert_eq!(h.confidence, Confidence::Medium);

        h.add_evidence(HypothesisEvidence {
            id: "e3".to_string(),
            description: "CSP blocks inline".to_string(),
            evidence_type: EvidenceType::HttpResponse,
            data: "Content-Security-Policy: script-src 'self'".to_string(),
            supports_hypothesis: Some(false),
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        // Refuting evidence caps confidence at Medium at best.
        assert_eq!(h.confidence, Confidence::Medium);
    }

    #[test]
    fn hypothesis_promotion_check() {
        let mut h = Hypothesis::new(
            "h1".to_string(),
            "Test".to_string(),
            VulnerabilityClass::Xss,
            "/test".to_string(),
            "evidence".to_string(),
        );
        assert!(!h.should_promote());

        h.status = HypothesisStatus::Supported;
        h.confidence = Confidence::High;
        h.result = Some(HypothesisResult {
            outcome: HypothesisOutcome::Confirmed,
            severity: Some(Severity::Medium),
            confidence: Confidence::High,
            reasoning: "Confirmed".to_string(),
            reproduction_possible: true,
            finding_id: None,
        });
        assert!(h.should_promote());
    }
}
