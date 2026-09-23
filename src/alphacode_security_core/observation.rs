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
        // Deduplicate identical links.
        if self.links.iter().any(|l| {
            l.evidence_id == evidence_id
                && l.hypothesis_id == hypothesis_id
                && l.supports == supports
        }) {
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
