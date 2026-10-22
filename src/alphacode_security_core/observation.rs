use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::state::Provenance;

/// Strict pipeline: RAW OBSERVATION -> NORMALIZED FACT -> INTERPRETATION
/// -> HYPOTHESIS -> CONCLUSION. This separation reduces hallucinated findings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RawObservation {
    pub id: String,
    pub tool: String,
    pub agent_id: Option<String>,
    pub content: String,
    pub timestamp: String,
    pub content_hash: u64,
    // Enhanced observation fields
    pub source_url: Option<String>,
    pub source_ip: Option<String>,
    pub source_port: Option<u16>,
    pub protocol: Option<String>,
    pub request_method: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub response_time_ms: Option<u64>,
    pub screenshot_path: Option<String>,
    pub dom_snapshot: Option<String>,
    pub network_requests: Vec<String>,
    pub console_logs: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
    pub tags: Vec<String>,
    pub severity: Option<String>,
    pub confidence: Option<f64>,
    pub related_observations: Vec<String>,
    pub parent_observation: Option<String>,
    pub child_observations: Vec<String>,
    pub is_security_relevant: bool,
    pub security_category: Option<String>,
    pub attack_vector: Option<String>,
    pub impact_assessment: Option<String>,
    pub remediation_hint: Option<String>,
    pub false_positive_likelihood: Option<f64>,
    pub true_positive_likelihood: Option<f64>,
    pub verification_status: Option<String>,
    pub verified_by: Option<String>,
    pub verification_timestamp: Option<String>,
    pub notes: Vec<String>,
}

impl RawObservation {
    pub fn new(tool: String, content: String, agent_id: Option<String>) -> Self {
        let content_hash = stable_hash(&content);
        let id = format!("obs_{content_hash:016x}");
        Self {
            id,
            tool,
            agent_id,
            content,
            timestamp: chrono::Utc::now().to_rfc3339(),
            content_hash,
            // Enhanced observation fields
            source_url: None,
            source_ip: None,
            source_port: None,
            protocol: None,
            request_method: None,
            request_headers: Vec::new(),
            request_body: None,
            response_status: None,
            response_headers: Vec::new(),
            response_body: None,
            response_time_ms: None,
            screenshot_path: None,
            dom_snapshot: None,
            network_requests: Vec::new(),
            console_logs: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            metadata: std::collections::HashMap::new(),
            tags: Vec::new(),
            severity: None,
            confidence: None,
            related_observations: Vec::new(),
            parent_observation: None,
            child_observations: Vec::new(),
            is_security_relevant: false,
            security_category: None,
            attack_vector: None,
            impact_assessment: None,
            remediation_hint: None,
            false_positive_likelihood: None,
            true_positive_likelihood: None,
            verification_status: None,
            verified_by: None,
            verification_timestamp: None,
            notes: Vec::new(),
        }
    }

    /// Create a new observation with HTTP request/response data.
    #[allow(clippy::too_many_arguments)]
    pub fn with_http_data(
        tool: String,
        content: String,
        agent_id: Option<String>,
        method: String,
        url: String,
        request_headers: Vec<(String, String)>,
        request_body: Option<String>,
        response_status: u16,
        response_headers: Vec<(String, String)>,
        response_body: Option<String>,
    ) -> Self {
        let mut obs = Self::new(tool, content, agent_id);
        obs.request_method = Some(method);
        obs.source_url = Some(url);
        obs.request_headers = request_headers;
        obs.request_body = request_body;
        obs.response_status = Some(response_status);
        obs.response_headers = response_headers;
        obs.response_body = response_body;
        obs
    }

    /// Mark this observation as security relevant.
    pub fn mark_security_relevant(&mut self, category: String, attack_vector: String) {
        self.is_security_relevant = true;
        self.security_category = Some(category);
        self.attack_vector = Some(attack_vector);
    }

    /// Add a related observation.
    pub fn add_related(&mut self, observation_id: String) {
        if !self.related_observations.contains(&observation_id) {
            self.related_observations.push(observation_id);
        }
    }

    /// Add a note to this observation.
    pub fn add_note(&mut self, note: String) {
        self.notes.push(note);
    }

    /// Calculate the security relevance score in `0.0..=1.0`.
    ///
    /// The five weights below already sum to 1.0, so the score is their
    /// weighted sum. This used to divide by a `factors` counter that was
    /// incremented unconditionally, i.e. always 5 — which capped the result
    /// at 0.2 and made both downstream predicates degenerate: the
    /// `> 0.6` true-positive test could never fire, and the `< 0.3`
    /// false-positive test was true for every observation not explicitly
    /// flagged security-relevant.
    pub fn security_score(&self) -> f64 {
        let mut score = 0.0;

        // Security relevance flag
        if self.is_security_relevant {
            score += 0.3;
        }

        // Has security category
        if self.security_category.is_some() {
            score += 0.2;
        }

        // Has attack vector
        if self.attack_vector.is_some() {
            score += 0.2;
        }

        // Has impact assessment
        if self.impact_assessment.is_some() {
            score += 0.15;
        }

        // Has confidence score
        if let Some(confidence) = self.confidence {
            score += confidence * 0.15;
        }

        score.clamp(0.0, 1.0)
    }

    /// Check if this observation is likely a true positive.
    pub fn is_likely_true_positive(&self) -> bool {
        if let (Some(tp), Some(fp)) = (
            self.true_positive_likelihood,
            self.false_positive_likelihood,
        ) {
            tp > fp && tp > 0.7
        } else {
            self.is_security_relevant && self.security_score() > 0.6
        }
    }

    /// Check if this observation is likely a false positive.
    pub fn is_likely_false_positive(&self) -> bool {
        if let (Some(tp), Some(fp)) = (
            self.true_positive_likelihood,
            self.false_positive_likelihood,
        ) {
            fp > tp && fp > 0.7
        } else {
            !self.is_security_relevant && self.security_score() < 0.3
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NormalizedFact {
    pub id: String,
    pub observation_id: String,
    pub key: String,
    pub value: String,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Interpretation {
    pub id: String,
    pub fact_ids: Vec<String>,
    pub statement: String,
    pub is_model_statement: bool,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceReliability {
    Unchecked,
    SingleSource,
    Corroborated,
    IndependentlyReproduced,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EvidenceLink {
    pub evidence_id: String,
    pub hypothesis_id: String,
    pub supports: bool,
    pub note: String,
}

/// Lightweight evidence graph: observation -> fact -> interpretation ->
/// hypothesis -> finding. Stored as adjacency with dedup.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EvidenceGraph {
    pub observations: HashMap<String, RawObservation>,
    pub facts: HashMap<String, NormalizedFact>,
    pub interpretations: HashMap<String, Interpretation>,
    pub links: Vec<EvidenceLink>,
    pub seen_hashes: HashSet<u64>,
    /// Dedup key set for `links` — avoids an O(n) linear scan per new link
    /// on the evidence-ingest hot path. Kept as `serde(default)` so sessions
    /// persisted by older builds still load.
    #[serde(default)]
    pub link_keys: HashSet<(String, String, bool)>,
    pub reliability: HashMap<String, EvidenceReliability>,
}

impl EvidenceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns None when this exact content was already seen (dedup).
    pub fn ingest_observation(
        &mut self,
        tool: String,
        content: String,
        agent_id: Option<String>,
    ) -> Option<RawObservation> {
        let hash = stable_hash(&content);
        if self.seen_hashes.contains(&hash) {
            return None;
        }
        let obs = RawObservation::new(tool, content, agent_id);
        self.seen_hashes.insert(hash);
        self.observations.insert(obs.id.clone(), obs.clone());
        self.reliability
            .insert(obs.id.clone(), EvidenceReliability::SingleSource);
        Some(obs)
    }

    pub fn normalize(
        &mut self,
        observation_id: &str,
        key: String,
        value: String,
        provenance: Provenance,
    ) -> Option<NormalizedFact> {
        if !self.observations.contains_key(observation_id) {
            return None;
        }
        let id = format!(
            "fact_{}_{}",
            observation_id,
            stable_hash(&format!("{key}={value}"))
        );
        let fact = NormalizedFact {
            id: id.clone(),
            observation_id: observation_id.to_string(),
            key,
            value,
            provenance,
        };
        self.facts.insert(id.clone(), fact.clone());
        Some(fact)
    }

    pub fn interpret(
        &mut self,
        fact_ids: Vec<String>,
        statement: String,
        is_model_statement: bool,
        provenance: Provenance,
    ) -> Interpretation {
        let id = format!("interp_{:016x}", stable_hash(&statement));
        let interp = Interpretation {
            id: id.clone(),
            fact_ids,
            statement,
            is_model_statement,
            provenance,
        };
        self.interpretations.insert(id.clone(), interp.clone());
        interp
    }

    pub fn link_evidence(
        &mut self,
        evidence_id: String,
        hypothesis_id: String,
        supports: bool,
        note: String,
    ) {
        // Deduplicate identical links in O(1) via the key set (the pre-set
        // linear scan cost grew with every link added in a long session).
        let key = (evidence_id.clone(), hypothesis_id.to_string(), supports);
        if !self.link_keys.insert(key) {
            return;
        }
        self.links.push(EvidenceLink {
            evidence_id,
            hypothesis_id,
            supports,
            note,
        });
    }

    pub fn mark_reproduced(&mut self, evidence_id: &str) {
        self.reliability.insert(
            evidence_id.to_string(),
            EvidenceReliability::IndependentlyReproduced,
        );
    }

    pub fn mark_corroborated(&mut self, evidence_id: &str) {
        // Only upgrade, never downgrade.
        let entry = self
            .reliability
            .entry(evidence_id.to_string())
            .or_insert(EvidenceReliability::SingleSource);
        if *entry == EvidenceReliability::SingleSource || *entry == EvidenceReliability::Unchecked {
            *entry = EvidenceReliability::Corroborated;
        }
    }

    pub fn supporting(&self, hypothesis_id: &str) -> Vec<&EvidenceLink> {
        self.links
            .iter()
            .filter(|l| l.hypothesis_id == hypothesis_id && l.supports)
            .collect()
    }

    pub fn contradicting(&self, hypothesis_id: &str) -> Vec<&EvidenceLink> {
        self.links
            .iter()
            .filter(|l| l.hypothesis_id == hypothesis_id && !l.supports)
            .collect()
    }

    pub fn is_novel(&self, content: &str) -> bool {
        !self.seen_hashes.contains(&stable_hash(content))
    }
}

fn stable_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::state::Provenance;

    /// `security_score` divided by a `factors` counter that was incremented
    /// unconditionally, capping the result at 0.2. That made
    /// `is_likely_true_positive`'s `> 0.6` unreachable and
    /// `is_likely_false_positive`'s `< 0.3` true for anything not explicitly
    /// flagged. Pin the reachable range.
    #[test]
    fn security_score_spans_the_documented_zero_to_one_range() {
        let mut obs = RawObservation::new("tool".into(), "data".into(), None);
        assert_eq!(
            obs.security_score(),
            0.0,
            "a bare observation scores nothing"
        );

        obs.is_security_relevant = true;
        obs.security_category = Some("xss".into());
        obs.attack_vector = Some("network".into());
        obs.impact_assessment = Some("data exposure".into());
        obs.confidence = Some(1.0);
        let best = obs.security_score();
        assert!(
            best > 0.9 && best <= 1.0,
            "a fully-populated observation must approach 1.0, got {best}"
        );
    }

    #[test]
    fn likely_true_positive_is_reachable_without_explicit_likelihoods() {
        let mut obs = RawObservation::new("tool".into(), "data".into(), None);
        obs.true_positive_likelihood = None;
        obs.false_positive_likelihood = None;
        obs.is_security_relevant = true;
        obs.security_category = Some("sqli".into());
        obs.attack_vector = Some("network".into());
        obs.impact_assessment = Some("db dump".into());
        obs.confidence = Some(1.0);
        assert!(
            obs.is_likely_true_positive(),
            "a well-populated security-relevant observation must be able to read as a likely TP"
        );
    }

    #[test]
    fn a_populated_non_security_observation_is_not_automatically_a_false_positive() {
        let mut obs = RawObservation::new("tool".into(), "data".into(), None);
        obs.true_positive_likelihood = None;
        obs.false_positive_likelihood = None;
        obs.is_security_relevant = false;
        // Carries real security metadata despite the flag.
        obs.security_category = Some("authz".into());
        obs.attack_vector = Some("network".into());
        obs.impact_assessment = Some("cross-tenant read".into());
        obs.confidence = Some(1.0);
        assert!(
            !obs.is_likely_false_positive(),
            "the FP fallback must not fire on every unflagged observation"
        );
    }

    #[test]
    fn dedups_identical_observations() {
        let mut g = EvidenceGraph::new();
        let a = g.ingest_observation("httpx".into(), "200 OK".into(), None);
        assert!(a.is_some());
        let b = g.ingest_observation("httpx".into(), "200 OK".into(), None);
        assert!(b.is_none());
        assert!(!g.is_novel("200 OK"));
        assert!(g.is_novel("404"));
    }

    #[test]
    fn observation_fact_interpretation_chain() {
        let mut g = EvidenceGraph::new();
        let obs = g
            .ingest_observation("browser".into(), "header: x-powered-by".into(), None)
            .unwrap();
        let prov = Provenance::new(Some("browser".into()), None);
        let fact = g
            .normalize(
                &obs.id,
                "header".into(),
                "x-powered-by".into(),
                prov.clone(),
            )
            .unwrap();
        let interp = g.interpret(vec![fact.id.clone()], "tech leak".into(), true, prov);
        assert!(interp.is_model_statement);
        g.link_evidence(fact.id.clone(), "h1".into(), true, "supports".into());
        assert_eq!(g.supporting("h1").len(), 1);
        assert!(g.contradicting("h1").is_empty());
    }

    #[test]
    fn reliability_only_upgrades() {
        let mut g = EvidenceGraph::new();
        let obs = g.ingest_observation("t".into(), "c".into(), None).unwrap();
        g.mark_reproduced(&obs.id);
        assert_eq!(
            g.reliability[&obs.id],
            EvidenceReliability::IndependentlyReproduced
        );
        g.mark_corroborated(&obs.id);
        // Must not downgrade reproduced -> corroborated.
        assert_eq!(
            g.reliability[&obs.id],
            EvidenceReliability::IndependentlyReproduced
        );
    }
}
