use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use super::finding::{Confidence, Severity, VulnerabilityClass};

/// A testable security assumption about the target.
///
/// Uses a HashMap for impact fields instead of 350+ individual Option<String> fields.
/// This reduces memory usage from ~10KB per hypothesis to ~200 bytes for the common case.
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
    /// Impact assessment fields stored as key-value pairs.
    /// Keys are impact types (e.g., "confidentiality", "integrity", "availability").
    /// This replaces 350+ individual Option<String> fields with a compact HashMap.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub impact_fields: HashMap<String, String>,
    /// Attack vector description (e.g., "network", "adjacent", "local", "physical").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_vector: Option<String>,
    /// Attack complexity (e.g., "low", "high").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_complexity: Option<String>,
    /// Privileges required (e.g., "none", "low", "high").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privileges_required: Option<String>,
    /// User interaction required (e.g., "none", "required").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_interaction: Option<String>,
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
            impact_fields: std::collections::HashMap::new(),
            attack_vector: None,
            attack_complexity: None,
            privileges_required: None,
            user_interaction: None,
        }
    }

    /// Set an impact field value.
    pub fn set_impact(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.impact_fields.insert(key.into(), value.into());
    }

    /// Get an impact field value.
    pub fn get_impact(&self, key: &str) -> Option<&str> {
        self.impact_fields.get(key).map(|s| s.as_str())
    }

    /// Check if any impact fields are set.
    pub fn has_impacts(&self) -> bool {
        !self.impact_fields.is_empty()
    }

    /// Add evidence and auto-update confidence based on evidence balance.
    pub fn add_evidence(&mut self, evidence: HypothesisEvidence) {
        if let Some(existing) = self.evidence.iter_mut().find(|item| item.id == evidence.id) {
            *existing = evidence;
        } else {
            self.evidence.push(evidence);
        }
        self.recalculate_confidence();
    }

    /// Recalculate confidence based on the balance of supporting/refuting evidence.
    fn recalculate_confidence(&mut self) {
        // Re-importing an artifact (even with a fresh id) is not independent
        // corroboration. Payload proposals and analysis are not observations.
        let observations: HashSet<_> = self
            .evidence
            .iter()
            .filter(|e| !e.data.trim().is_empty())
            .filter(|e| {
                !matches!(
                    e.evidence_type,
                    EvidenceType::Analysis | EvidenceType::Payload
                )
            })
            .map(|e| (e.data.as_str(), e.supports_hypothesis))
            .collect();
        let supporting = observations
            .iter()
            .filter(|(_, supports)| *supports == Some(true))
            .count();
        // Retain explicit contradictory analysis as a blocker until resolved.
        let refuting = self
            .evidence
            .iter()
            .filter(|e| e.supports_hypothesis == Some(false))
            .map(|e| e.data.as_str())
            .collect::<HashSet<_>>()
            .len();
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
            _ => Confidence::High,
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
                .map(|r| r.reproduction_possible && r.outcome == HypothesisOutcome::Confirmed)
                .unwrap_or(false)
            && !self
                .evidence
                .iter()
                .any(|e| e.supports_hypothesis == Some(false))
            && self
                .evidence
                .iter()
                .filter(|e| e.supports_hypothesis == Some(true) && !e.data.trim().is_empty())
                .filter(|e| {
                    !matches!(
                        e.evidence_type,
                        EvidenceType::Analysis | EvidenceType::Payload
                    )
                })
                .map(|e| e.data.as_str())
                .collect::<HashSet<_>>()
                .len()
                >= 2
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
            evidence_type: EvidenceType::Screenshot,
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
        assert!(
            !h.should_promote(),
            "a confidence label alone is not corroboration"
        );
        for (id, data) in [
            ("baseline", "owned baseline observed"),
            ("reproduction", "independent controlled reproduction"),
        ] {
            h.add_evidence(HypothesisEvidence {
                id: id.into(),
                description: "observed result".into(),
                evidence_type: EvidenceType::Observation,
                data: data.into(),
                supports_hypothesis: Some(true),
                collected_by: None,
                timestamp: String::new(),
            });
        }
        assert!(h.should_promote());
        h.result.as_mut().unwrap().outcome = HypothesisOutcome::Inconclusive;
        assert!(!h.should_promote());
        h.result.as_mut().unwrap().outcome = HypothesisOutcome::Confirmed;
        h.add_evidence(HypothesisEvidence {
            id: "negative".into(),
            description: "Control contradicts the claim".into(),
            evidence_type: EvidenceType::Observation,
            data: "same behavior in negative control".into(),
            supports_hypothesis: Some(false),
            collected_by: None,
            timestamp: String::new(),
        });
        h.confidence = Confidence::High;
        assert!(!h.should_promote());
    }

    #[test]
    fn repeated_and_proposed_evidence_cannot_manufacture_confidence() {
        let mut h = Hypothesis::new(
            "h".into(),
            "test".into(),
            VulnerabilityClass::Xss,
            "/test".into(),
            "clue".into(),
        );
        let mut evidence = HypothesisEvidence {
            id: "e".into(),
            description: "observation".into(),
            evidence_type: EvidenceType::HttpResponse,
            data: "one captured response".into(),
            supports_hypothesis: Some(true),
            collected_by: None,
            timestamp: String::new(),
        };
        h.add_evidence(evidence.clone());
        h.add_evidence(evidence.clone());
        assert_eq!(h.evidence.len(), 1);
        for index in 0..10 {
            evidence.id = format!("copy-{index}");
            h.add_evidence(evidence.clone());
        }
        assert_eq!(h.confidence, Confidence::Low);
        evidence.evidence_type = EvidenceType::Analysis;
        for index in 0..10 {
            evidence.id = format!("idea-{index}");
            evidence.data = format!("untested idea {index}");
            h.add_evidence(evidence.clone());
        }
        assert_eq!(h.confidence, Confidence::Low);
    }
}
