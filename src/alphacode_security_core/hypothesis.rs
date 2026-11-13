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
    /// Optional prediction that must be observed for this hypothesis to hold.
    /// When set, a refutation requires contradicting *this* observation, not
    /// merely a negative probe — which is what distinguishes "refuted" from
    /// "inconclusive".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction: Option<String>,
    /// Evidence that would invalidate a refutation and reopen the hypothesis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen_condition: Option<String>,
    /// How many consecutive equivalent negative results have been observed.
    #[serde(default)]
    pub equivalent_negative_count: u32,
    /// The discriminating observation that refuted this hypothesis, if any.
    /// Negative probes and tool failures must never populate this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kill_condition: Option<String>,
    /// Dimension last changed when this hypothesis was re-probed, so a
    /// re-plan can pick a genuinely different dimension instead of repeating
    /// the same probe with different wording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_dimension_changed: Option<String>,
    /// What is blocking this hypothesis from being tested, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
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
            prediction: None,
            reopen_condition: None,
            equivalent_negative_count: 0,
            kill_condition: None,
            last_dimension_changed: None,
            blocker: None,
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

    /// Record an equivalent negative result without treating absence as proof.
    ///
    /// Three equivalent negatives do not by themselves refute a hypothesis:
    /// reachability and the oracle may both be broken. What *does* kill it is
    /// a discriminating control that contradicts the prediction. Until that
    /// happens the hypothesis stays open and the caller is told to change a
    /// dimension — which is a re-plan instruction, not a surrender.
    ///
    /// The return type is retained for callers of the original API, but this
    /// method never returns a kill condition. After three negatives it marks
    /// the investigation inconclusive and requests a different experiment.
    pub fn record_negative(&mut self, observation: &str, dimension: &str) -> Option<String> {
        self.equivalent_negative_count = self.equivalent_negative_count.saturating_add(1);
        self.last_dimension_changed = Some(dimension.to_string());

        if let Some(prediction) = self.prediction.as_deref()
            && !prediction.is_empty()
            && observation
                .to_ascii_lowercase()
                .contains(&prediction.to_ascii_lowercase())
        {
            // The prediction was observed: this is not a negative result.
            self.equivalent_negative_count = 0;
            return None;
        }

        if self.needs_reassessment() {
            self.status = HypothesisStatus::InsufficientData;
            self.blocker.get_or_insert_with(|| {
                format!(
                    "{} equivalent negative probes; validate reachability and the oracle, \
                     then change a test dimension. No discriminating control refuted the prediction.",
                    self.equivalent_negative_count,
                )
            });
        }
        None
    }

    /// Whether this hypothesis is dead and the reason, if any.
    pub fn is_killed(&self) -> Option<&str> {
        self.kill_condition.as_deref()
    }

    /// Whether the caller should change a dimension rather than repeat the
    /// same probe. True once equivalent negatives have stacked up without a
    /// discriminating control killing the claim.
    pub fn needs_dimension_change(&self) -> bool {
        self.equivalent_negative_count > 0 && self.kill_condition.is_none()
    }

    /// Repeated absence is a reason to reassess prerequisites, not to refute.
    pub fn needs_reassessment(&self) -> bool {
        self.equivalent_negative_count >= 3 && self.kill_condition.is_none()
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

    #[test]
    fn three_equivalent_negatives_require_reassessment_not_refutation() {
        let mut h = Hypothesis::new(
            "h".into(),
            "test".into(),
            VulnerabilityClass::Xss,
            "/test".into(),
            "clue".into(),
        );
        h.prediction = Some("payload reflected unencoded".into());
        h.reopen_condition = Some("a control that changes the oracle".into());

        // Two negatives are not enough: reachability and the oracle may both
        // be broken, so the hypothesis stays open and asks for a dimension
        // change rather than surrendering.
        assert!(h.record_negative("no reflection seen", "actor").is_none());
        assert!(h.record_negative("no reflection seen", "method").is_none());
        assert_eq!(h.equivalent_negative_count, 2);
        assert!(h.needs_dimension_change());
        assert!(h.is_killed().is_none());

        assert!(!h.needs_reassessment());
        assert!(
            h.record_negative("no reflection seen", "representation")
                .is_none()
        );
        assert_eq!(h.status, HypothesisStatus::InsufficientData);
        assert!(h.is_killed().is_none());
        assert!(h.needs_dimension_change());
        assert!(h.needs_reassessment());
        assert_eq!(h.last_dimension_changed.as_deref(), Some("representation"));
        assert!(
            h.blocker
                .as_deref()
                .unwrap()
                .contains("3 equivalent negative probes")
        );
        assert_eq!(
            h.reopen_condition.as_deref(),
            Some("a control that changes the oracle")
        );
    }

    #[test]
    fn observing_the_prediction_is_not_a_negative() {
        let mut h = Hypothesis::new(
            "h".into(),
            "test".into(),
            VulnerabilityClass::Xss,
            "/test".into(),
            "clue".into(),
        );
        h.prediction = Some("payload reflected unencoded".into());

        // The prediction was observed, so this is a positive — the counter
        // must not advance and the hypothesis must not be killed.
        assert!(
            h.record_negative("response contains payload reflected unencoded", "actor")
                .is_none()
        );
        assert_eq!(h.equivalent_negative_count, 0);
        assert!(h.is_killed().is_none());
        assert!(!h.needs_dimension_change());
    }

    #[test]
    fn hypotheses_without_a_prediction_never_kill_on_negatives_alone() {
        let mut h = Hypothesis::new(
            "h".into(),
            "test".into(),
            VulnerabilityClass::Xss,
            "/test".into(),
            "clue".into(),
        );
        // No prediction: negatives are inconclusive, never refutation.
        for _ in 0..10 {
            assert!(h.record_negative("nothing observed", "actor").is_none());
        }
        assert_eq!(h.equivalent_negative_count, 10);
        assert_eq!(h.status, HypothesisStatus::InsufficientData);
        assert!(h.is_killed().is_none());
        assert!(h.needs_dimension_change());
        assert!(h.needs_reassessment());
    }

    #[test]
    fn negative_request_failures_never_refute_or_overwrite_a_prerequisite_blocker() {
        for observation in [
            "timeout",
            "connection refused",
            "401 expired session",
            "404 not found",
        ] {
            let mut h = Hypothesis::new(
                "h".into(),
                "test".into(),
                VulnerabilityClass::Xss,
                "/test".into(),
                "clue".into(),
            );
            h.prediction = Some("payload executes".into());
            h.blocker = Some("second controlled account required".into());
            for _ in 0..5 {
                assert!(h.record_negative(observation, "actor").is_none());
                assert_ne!(h.status, HypothesisStatus::Refuted);
                assert!(h.kill_condition.is_none());
            }
            assert_eq!(
                h.blocker.as_deref(),
                Some("second controlled account required")
            );
        }
    }

    #[test]
    fn negative_probe_counter_saturates_and_survives_reload() {
        let mut h = Hypothesis::new(
            "h".into(),
            "test".into(),
            VulnerabilityClass::Xss,
            "/test".into(),
            "clue".into(),
        );
        h.equivalent_negative_count = u32::MAX;
        assert!(h.record_negative("timeout", "transport").is_none());
        let loaded: Hypothesis = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(loaded.equivalent_negative_count, u32::MAX);
        assert_eq!(loaded.status, HypothesisStatus::InsufficientData);
        assert!(loaded.kill_condition.is_none());
        assert!(loaded.needs_reassessment());
    }
}
