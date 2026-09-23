use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete decision trajectory: state -> hypotheses -> candidates ->
/// selected -> execution -> observation -> evidence -> belief update ->
/// verification -> outcome. Foundation for future policy learning.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryStep {
    pub seq: u64,
    pub state_summary: String,
    pub active_hypotheses: Vec<String>,
    pub candidate_actions: Vec<String>,
    pub selected_action: Option<String>,
    pub selection_reason: String,
    pub expected_information_gain: f32,
    pub outcome: String,
    pub belief_delta: String,
    pub verifier_result: Option<String>,
    pub stopping_reason: Option<String>,
    pub memory_consulted: Vec<String>,
    pub lessons_generated: Vec<String>,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Trajectory {
    pub id: String,
    pub task: String,
    pub steps: Vec<TrajectoryStep>,
    pub final_verdict: Option<String>,
}

impl Trajectory {
    pub fn new(task: String) -> Self {
        let id = format!("traj_{:016x}", {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            task.hash(&mut h);
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or(0)
                .hash(&mut h);
            h.finish()
        });
        Self {
            id,
            task,
            steps: Vec::new(),
            final_verdict: None,
        }
    }

    pub fn push(&mut self, mut step: TrajectoryStep) {
        step.seq = self.steps.len() as u64;
        self.steps.push(step);
    }

    /// Compact observability view — structured metadata, never raw transcripts.
    pub fn observability_snapshot(&self) -> TrajectorySnapshot {
        let last = self.steps.last();
        TrajectorySnapshot {
            trajectory_id: self.id.clone(),
            steps: self.steps.len(),
            current_state: last.map(|s| s.state_summary.clone()).unwrap_or_default(),
            active_hypotheses: last
                .map(|s| s.active_hypotheses.clone())
                .unwrap_or_default(),
            selected_action: last.and_then(|s| s.selected_action.clone()),
            selection_reason: last.map(|s| s.selection_reason.clone()).unwrap_or_default(),
            last_outcome: last.map(|s| s.outcome.clone()).unwrap_or_default(),
            stopping_reason: last.and_then(|s| s.stopping_reason.clone()),
            final_verdict: self.final_verdict.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct TrajectorySnapshot {
    pub trajectory_id: String,
    pub steps: usize,
    pub current_state: String,
    pub active_hypotheses: Vec<String>,
    pub selected_action: Option<String>,
    pub selection_reason: String,
    pub last_outcome: String,
    pub stopping_reason: Option<String>,
    pub final_verdict: Option<String>,
}

/// Decision cache: repeated states should not always trigger fresh reasoning.
/// Keyed on relevant state features; invalidated when evidence changes.
#[derive(Clone, Debug, Default)]
pub struct DecisionCache {
    entries: HashMap<String, CachedDecision>,
    max_entries: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CachedDecision {
    pub state_key: String,
    pub evidence_version: u64,
    pub action_id: String,
    pub value: f32,
    pub inserted_at: String,
}

impl DecisionCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: max_entries.max(1),
        }
    }

    pub fn state_key(features: &[&str]) -> String {
        let mut sorted: Vec<&str> = features.to_vec();
        sorted.sort_unstable();
        format!("{:016x}", {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            sorted.hash(&mut h);
            h.finish()
        })
    }

    pub fn get(&self, state_key: &str, evidence_version: u64) -> Option<&CachedDecision> {
        self.entries.get(state_key).filter(|e| {
            // Stale decisions must never override new evidence.
            e.evidence_version == evidence_version
        })
    }

    pub fn put(&mut self, entry: CachedDecision) {
        if self.entries.len() >= self.max_entries {
            // Evict an arbitrary (oldest-ish) entry; HashMap order is fine for a bounded cache.
            if let Some(k) = self.entries.keys().next().cloned() {
                self.entries.remove(&k);
            }
        }
        self.entries.insert(entry.state_key.clone(), entry);
    }

    pub fn invalidate_on_evidence_change(&mut self, evidence_version: u64) {
        self.entries
            .retain(|_, e| e.evidence_version == evidence_version);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trajectory_snapshot_is_compact() {
        let mut t = Trajectory::new("task".into());
        t.push(TrajectoryStep {
            seq: 0,
            state_summary: "s".into(),
            active_hypotheses: vec!["h1".into()],
            candidate_actions: vec!["a".into()],
            selected_action: Some("a".into()),
            selection_reason: "best value".into(),
            expected_information_gain: 0.7,
            outcome: "ok".into(),
            belief_delta: "+h1".into(),
            verifier_result: None,
            stopping_reason: None,
            memory_consulted: vec![],
            lessons_generated: vec![],
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let snap = t.observability_snapshot();
        assert_eq!(snap.steps, 1);
        assert_eq!(snap.selected_action.as_deref(), Some("a"));
    }

    #[test]
    fn cache_invalidates_on_new_evidence() {
        let mut c = DecisionCache::new(10);
        c.put(CachedDecision {
            state_key: "k1".into(),
            evidence_version: 1,
            action_id: "a".into(),
            value: 0.5,
            inserted_at: chrono::Utc::now().to_rfc3339(),
        });
        assert!(c.get("k1", 1).is_some());
        assert!(c.get("k1", 2).is_none());
        c.invalidate_on_evidence_change(2);
        assert!(c.is_empty());
    }
}
