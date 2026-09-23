use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::finding::Confidence;
use super::hypothesis::{Hypothesis, HypothesisStatus};

/// Manages competing hypotheses without collapsing uncertainty prematurely.
/// Never exposes fake-calibrated probabilities: confidence stays heuristic
/// (Low/Medium/High/Certain) and promotion requires empirical evidence.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct HypothesisSet {
    pub hypotheses: HashMap<String, Hypothesis>,
    /// hypothesis_id -> required verification steps still missing.
    pub required_verification: HashMap<String, Vec<String>>,
    /// hypothesis_id -> assumptions it depends on.
    pub assumptions: HashMap<String, Vec<String>>,
}

impl HypothesisSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, h: Hypothesis) {
        self.hypotheses.insert(h.id.clone(), h);
    }

    pub fn get(&self, id: &str) -> Option<&Hypothesis> {
        self.hypotheses.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Hypothesis> {
        self.hypotheses.get_mut(id)
    }

    pub fn active(&self) -> Vec<&Hypothesis> {
        self.hypotheses
            .values()
            .filter(|h| {
                matches!(
                    h.status,
                    HypothesisStatus::Proposed
                        | HypothesisStatus::SelectedForTesting
                        | HypothesisStatus::InProgress
                )
            })
            .collect()
    }

    pub fn supported(&self) -> Vec<&Hypothesis> {
        self.hypotheses
            .values()
            .filter(|h| h.status == HypothesisStatus::Supported)
            .collect()
    }

    /// Record supporting (true) or contradicting (false) evidence for one hypothesis.
    /// Also updates the shared set: same evidence may contradict a rival.
    pub fn record_evidence_outcome(
        &mut self,
        hypothesis_id: &str,
        evidence: super::hypothesis::HypothesisEvidence,
        contradicts_rivals: &[String],
    ) {
        if let Some(h) = self.hypotheses.get_mut(hypothesis_id) {
            h.add_evidence(evidence.clone());
        }
        for rival in contradicts_rivals {
            if let Some(r) = self.hypotheses.get_mut(rival) {
                let mut neg = evidence.clone();
                neg.id = format!("{}_neg_{}", evidence.id, rival);
                neg.supports_hypothesis = Some(false);
                r.add_evidence(neg);
            }
        }
    }

    pub fn mark_supported(&mut self, id: &str) {
        if let Some(h) = self.hypotheses.get_mut(id) {
            h.status = HypothesisStatus::Supported;
        }
    }

    pub fn mark_refuted(&mut self, id: &str) {
        if let Some(h) = self.hypotheses.get_mut(id) {
            h.status = HypothesisStatus::Refuted;
        }
    }

    pub fn mark_insufficient(&mut self, id: &str, missing: Vec<String>) {
        if let Some(h) = self.hypotheses.get_mut(id) {
            h.status = HypothesisStatus::InsufficientData;
        }
        self.required_verification.insert(id.to_string(), missing);
    }

    /// Which hypothesis most deserves attention? Prefers: InProgress >
    /// SelectedForTesting > Proposed, then higher heuristic confidence, then
    /// most evidence. Deterministic, no model call.
    pub fn most_urgent(&self) -> Option<&Hypothesis> {
        let mut best: Option<&Hypothesis> = None;
        for h in self.hypotheses.values() {
            if matches!(
                h.status,
                HypothesisStatus::Refuted | HypothesisStatus::Supported
            ) {
                continue;
            }
            let score = urgency_score(h);
            let best_score = best.map(urgency_score).unwrap_or(-1);
            if score > best_score {
                best = Some(h);
            }
        }
        best
    }

    /// Hypotheses that directly compete (same target component, different class
    /// or opposite expectation). Used to pick discriminating experiments.
    pub fn competitors_of(&self, id: &str) -> Vec<&Hypothesis> {
        let Some(base) = self.hypotheses.get(id) else {
            return Vec::new();
        };
        self.hypotheses
            .values()
            .filter(|h| h.id != id && h.target_component == base.target_component)
            .collect()
    }

    pub fn promotion_candidates(&self) -> Vec<&Hypothesis> {
        self.hypotheses
            .values()
            .filter(|h| h.should_promote())
            .collect()
    }
}

fn urgency_score(h: &Hypothesis) -> i32 {
    let status_w = match h.status {
        HypothesisStatus::InProgress => 30,
        HypothesisStatus::SelectedForTesting => 20,
        HypothesisStatus::Proposed => 10,
        HypothesisStatus::InsufficientData => 5,
        _ => 0,
    };
    let conf_w = match h.confidence {
        Confidence::Certain => 4,
        Confidence::High => 3,
        Confidence::Medium => 2,
        Confidence::Low => 1,
    };
    status_w + conf_w + (h.evidence.len().min(10) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::finding::VulnerabilityClass;

    fn hypo(id: &str, target: &str) -> Hypothesis {
        Hypothesis::new(
            id.into(),
            format!("hypo {id}"),
            VulnerabilityClass::Xss,
            target.into(),
            "trigger".into(),
        )
    }

    #[test]
    fn most_urgent_prefers_in_progress() {
        let mut set = HypothesisSet::new();
        let mut a = hypo("a", "/x");
        a.status = HypothesisStatus::Proposed;
        let mut b = hypo("b", "/y");
        b.status = HypothesisStatus::InProgress;
        set.insert(a);
        set.insert(b);
        assert_eq!(set.most_urgent().unwrap().id, "b");
    }

    #[test]
    fn competitors_share_target() {
        let mut set = HypothesisSet::new();
        set.insert(hypo("a", "/same"));
        set.insert(hypo("b", "/same"));
        set.insert(hypo("c", "/other"));
        assert_eq!(set.competitors_of("a").len(), 1);
    }

    #[test]
    fn rival_contradiction_propagates() {
        use crate::alphacode_security_core::hypothesis::{EvidenceType, HypothesisEvidence};
        let mut set = HypothesisSet::new();
        set.insert(hypo("h1", "/t"));
        set.insert(hypo("h2", "/t"));
        let ev = HypothesisEvidence {
            id: "e1".into(),
            description: "supports h1".into(),
            evidence_type: EvidenceType::HttpResponse,
            data: "x".into(),
            supports_hypothesis: Some(true),
            collected_by: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        set.record_evidence_outcome("h1", ev, &["h2".to_string()]);
        assert_eq!(set.get("h1").unwrap().evidence.len(), 1);
        assert_eq!(set.get("h2").unwrap().evidence.len(), 1);
        assert_eq!(
            set.get("h2").unwrap().evidence[0].supports_hypothesis,
            Some(false)
        );
    }
}
