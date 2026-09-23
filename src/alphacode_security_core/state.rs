use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Where a piece of state came from. Every important conclusion must be
/// traceable back to concrete observations — never a provenance-free model
/// statement.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub tool: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp: String,
    pub state_hint: Option<String>,
    pub reproducible: bool,
}

impl Provenance {
    pub fn new(tool: Option<String>, agent_id: Option<String>) -> Self {
        Self {
            tool,
            agent_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            state_hint: None,
            reproducible: false,
        }
    }

    pub fn reproducible(mut self) -> Self {
        self.reproducible = true;
        self
    }
}

/// A task objective with explicit verification status.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Objective {
    pub id: String,
    pub description: String,
    pub completed: bool,
    pub verification: VerificationStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    #[default]
    Unverified,
    PartiallyVerified,
    Verified,
    Refuted,
    Blocked {
        reason: String,
    },
}

/// An explicit assumption (distinct from a verified fact).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Assumption {
    pub id: String,
    pub statement: String,
    pub provenance: Provenance,
    pub still_valid: bool,
}

/// A recorded contradiction between observations / claims.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Contradiction {
    pub id: String,
    pub claim_a: String,
    pub claim_b: String,
    pub required_evidence: String,
    pub resolved: bool,
}

/// Strategy history — what was tried and why it changed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StrategyRecord {
    pub name: String,
    pub reason: String,
    pub started_at: String,
    pub ended_at: Option<String>,
}

/// Heuristic uncertainty — explicitly NOT a calibrated probability.
/// Distinguishes heuristic confidence from empirical evidence / verified fact.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Uncertainty {
    /// Heuristic model confidence 0.0-1.0.
    pub heuristic_confidence: f32,
    /// Count of empirical supporting observations.
    pub empirical_support: u32,
    /// Count of contradicting observations.
    pub empirical_against: u32,
    /// Whether an independent verifier reproduced the claim.
    pub independently_verified: bool,
}

impl Uncertainty {
    pub fn is_verified_fact(&self) -> bool {
        self.independently_verified && self.empirical_support >= 2 && self.empirical_against == 0
    }
}

/// Resource consumption ledger for cost-aware decisions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceConsumption {
    pub tool_calls: u32,
    pub model_calls: u32,
    pub network_requests: u32,
    pub agent_spawns: u32,
    pub wall_clock_secs: u64,
    pub estimated_tokens: u64,
}

impl ResourceConsumption {
    pub fn record_tool_call(&mut self) {
        self.tool_calls += 1;
    }
    pub fn record_model_call(&mut self, tokens: u64) {
        self.model_calls += 1;
        self.estimated_tokens += tokens;
    }
    pub fn total_cost_units(&self) -> u64 {
        self.tool_calls as u64 + self.model_calls as u64 * 10 + self.network_requests as u64
    }
}

/// Structured world state. Incrementally updateable; never a giant string.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldState {
    pub task: String,
    pub objectives: Vec<Objective>,
    pub facts: HashMap<String, String>,
    pub fact_provenance: HashMap<String, Provenance>,
    pub assumptions: Vec<Assumption>,
    pub hypothesis_ids: HashSet<String>,
    pub evidence_ids: HashSet<String>,
    pub contradictions: Vec<Contradiction>,
    pub actions_attempted: Vec<String>,
    pub actions_succeeded: Vec<String>,
    pub actions_failed: Vec<String>,
    pub tool_results: HashMap<String, u32>,
    pub discovered_capabilities: HashSet<String>,
    pub environmental_characteristics: HashMap<String, String>,
    pub dependencies: Vec<String>,
    pub constraints: Vec<String>,
    pub current_strategy: Option<String>,
    pub previous_strategies: Vec<StrategyRecord>,
    pub verification: VerificationStatus,
    pub uncertainty: Uncertainty,
    pub resources: ResourceConsumption,
    pub agent_owner: Option<String>,
    pub updated_at: String,
}

impl WorldState {
    pub fn new(task: String) -> Self {
        Self {
            task,
            objectives: Vec::new(),
            facts: HashMap::new(),
            fact_provenance: HashMap::new(),
            assumptions: Vec::new(),
            hypothesis_ids: HashSet::new(),
            evidence_ids: HashSet::new(),
            contradictions: Vec::new(),
            actions_attempted: Vec::new(),
            actions_succeeded: Vec::new(),
            actions_failed: Vec::new(),
            tool_results: HashMap::new(),
            discovered_capabilities: HashSet::new(),
            environmental_characteristics: HashMap::new(),
            dependencies: Vec::new(),
            constraints: Vec::new(),
            current_strategy: None,
            previous_strategies: Vec::new(),
            verification: VerificationStatus::Unverified,
            uncertainty: Uncertainty::default(),
            resources: ResourceConsumption::default(),
            agent_owner: None,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn add_fact(&mut self, key: String, value: String, provenance: Provenance) {
        self.facts.insert(key.clone(), value);
        self.fact_provenance.insert(key, provenance);
        self.touch();
    }

    pub fn add_assumption(&mut self, statement: String, provenance: Provenance) -> String {
        let id = format!("asm_{}", self.assumptions.len() + 1);
        self.assumptions.push(Assumption {
            id: id.clone(),
            statement,
            provenance,
            still_valid: true,
        });
        self.touch();
        id
    }

    pub fn invalidate_assumption(&mut self, id: &str) {
        if let Some(a) = self.assumptions.iter_mut().find(|a| a.id == id) {
            a.still_valid = false;
        }
        self.touch();
    }

    pub fn record_action_attempt(&mut self, action_id: String) {
        if !self.actions_attempted.contains(&action_id) {
            self.actions_attempted.push(action_id);
        }
        self.resources.record_tool_call();
        self.touch();
    }

    pub fn record_action_success(&mut self, action_id: String) {
        if !self.actions_succeeded.contains(&action_id) {
            self.actions_succeeded.push(action_id);
        }
        self.touch();
    }

    pub fn record_action_failure(&mut self, action_id: String) {
        if !self.actions_failed.contains(&action_id) {
            self.actions_failed.push(action_id);
        }
        self.touch();
    }

    pub fn has_tried(&self, action_id: &str) -> bool {
        self.actions_attempted.iter().any(|a| a == action_id)
    }

    pub fn has_failed(&self, action_id: &str) -> bool {
        self.actions_failed.iter().any(|a| a == action_id)
    }

    pub fn add_contradiction(
        &mut self,
        claim_a: String,
        claim_b: String,
        required_evidence: String,
    ) -> String {
        let id = format!("ctr_{}", self.contradictions.len() + 1);
        self.contradictions.push(Contradiction {
            id: id.clone(),
            claim_a,
            claim_b,
            required_evidence,
            resolved: false,
        });
        self.touch();
        id
    }

    pub fn resolve_contradiction(&mut self, id: &str) {
        if let Some(c) = self.contradictions.iter_mut().find(|c| c.id == id) {
            c.resolved = true;
        }
        self.touch();
    }

    pub fn open_contradictions(&self) -> Vec<&Contradiction> {
        self.contradictions.iter().filter(|c| !c.resolved).collect()
    }

    pub fn set_strategy(&mut self, name: String, reason: String) {
        if let Some(current) = self.current_strategy.take() {
            self.previous_strategies.push(StrategyRecord {
                name: current,
                reason: "superseded".to_string(),
                started_at: self.updated_at.clone(),
                ended_at: Some(chrono::Utc::now().to_rfc3339()),
            });
        }
        self.current_strategy = Some(name);
        // Record reason as a constraint hint for observability.
        if !reason.is_empty() {
            self.previous_strategies.push(StrategyRecord {
                name: "strategy_reason".to_string(),
                reason,
                started_at: chrono::Utc::now().to_rfc3339(),
                ended_at: None,
            });
        }
        self.touch();
    }

    /// Compact structured summary for prompts / telemetry. Never dumps raw transcripts.
    pub fn compact_summary(&self) -> String {
        format!(
            "task={} objectives={}/{} facts={} assumptions={} hypos={} evidence={} contradictions_open={} tried={} failed={} strategy={} verification={:?}",
            truncate(&self.task, 120),
            self.objectives.iter().filter(|o| o.completed).count(),
            self.objectives.len(),
            self.facts.len(),
            self.assumptions.iter().filter(|a| a.still_valid).count(),
            self.hypothesis_ids.len(),
            self.evidence_ids.len(),
            self.open_contradictions().len(),
            self.actions_attempted.len(),
            self.actions_failed.len(),
            self.current_strategy.as_deref().unwrap_or("none"),
            self.verification,
        )
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_tracks_facts_with_provenance() {
        let mut s = WorldState::new("test task".into());
        s.add_fact(
            "tech".into(),
            "nginx".into(),
            Provenance::new(Some("httpx".into()), Some("agent-1".into())),
        );
        assert_eq!(s.facts.len(), 1);
        assert!(s.fact_provenance.contains_key("tech"));
    }

    #[test]
    fn state_dedups_action_attempts() {
        let mut s = WorldState::new("t".into());
        s.record_action_attempt("a1".into());
        s.record_action_attempt("a1".into());
        assert_eq!(s.actions_attempted.len(), 1);
        assert_eq!(s.resources.tool_calls, 2);
    }

    #[test]
    fn contradictions_open_and_resolve() {
        let mut s = WorldState::new("t".into());
        let id = s.add_contradiction("a".into(), "b".into(), "repro".into());
        assert_eq!(s.open_contradictions().len(), 1);
        s.resolve_contradiction(&id);
        assert!(s.open_contradictions().is_empty());
    }

    #[test]
    fn uncertainty_verified_fact_requires_independence() {
        let u = Uncertainty {
            heuristic_confidence: 0.95,
            empirical_support: 5,
            empirical_against: 0,
            independently_verified: false,
        };
        assert!(!u.is_verified_fact());
        let v = Uncertainty {
            independently_verified: true,
            empirical_support: 2,
            empirical_against: 0,
            heuristic_confidence: 0.7,
        };
        assert!(v.is_verified_fact());
    }
}
