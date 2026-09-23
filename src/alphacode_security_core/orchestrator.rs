use serde::{Deserialize, Serialize};

use super::action::ActionSpace;
use super::arbitration::{ArbitratedContribution, ContributionScore, arbitrate};
use super::hypothesis_set::HypothesisSet;
use super::learning::{ActionOutcomeStats, Lesson, lesson_adjustment};
use super::observation::EvidenceGraph;
use super::resources::{ResourceLedger, TerminationState, should_stop};
use super::state::WorldState;
use super::trajectory::{Trajectory, TrajectoryStep};
use super::verification_quality::{FalsePositiveDefense, VerificationQuality};

/// The adaptive decision cycle as pure deterministic logic:
///
/// OBSERVE -> STATE -> HYPOTHESES -> ACTION GENERATION -> ACTION EVALUATION
/// -> EXECUTION (by caller) -> EVIDENCE -> BELIEF UPDATE -> VERIFICATION
/// -> CONTINUE / CHANGE / DELEGATE / STOP -> MEMORY + TRAJECTORY -> LEARNING
///
/// The LLM remains the reasoning component; this orchestrator handles the
/// low-level structured decisions around it with near-zero overhead.
#[derive(Clone, Debug)]
pub struct AdaptiveCycle {
    pub state: WorldState,
    pub hypotheses: HypothesisSet,
    pub actions: ActionSpace,
    pub evidence: EvidenceGraph,
    pub ledger: ResourceLedger,
    pub trajectory: Trajectory,
    pub outcomes: std::collections::HashMap<String, ActionOutcomeStats>,
    pub lessons: Vec<Lesson>,
    pub evidence_version: u64,
}

impl AdaptiveCycle {
    pub fn new(task: String) -> Self {
        Self {
            state: WorldState::new(task.clone()),
            hypotheses: HypothesisSet::new(),
            actions: ActionSpace::with_builtin_security_actions(),
            evidence: EvidenceGraph::new(),
            ledger: ResourceLedger::default(),
            trajectory: Trajectory::new(task),
            outcomes: std::collections::HashMap::new(),
            lessons: Vec::new(),
            evidence_version: 0,
        }
    }

    /// OBSERVE: ingest a raw tool observation. Returns false when duplicate.
    pub fn observe(&mut self, tool: String, content: String, agent_id: Option<String>) -> bool {
        if !self.evidence.is_novel(&content) {
            return false;
        }
        if let Some(obs) = self.evidence.ingest_observation(tool, content, agent_id) {
            self.state.evidence_ids.insert(obs.id.clone());
            self.evidence_version += 1;
            true
        } else {
            false
        }
    }

    /// Select the next action using info-gain/cost + lesson adjustments.
    /// Returns (action_id, value, reason). No model call.
    pub fn select_next_action(
        &self,
        max_cost: f32,
        max_risk: f32,
    ) -> Option<(String, f32, String)> {
        let tried: std::collections::HashSet<String> =
            self.state.actions_attempted.iter().cloned().collect();
        let failed: std::collections::HashSet<String> =
            self.state.actions_failed.iter().cloned().collect();
        let candidates = self.actions.affordable(&tried, &failed, max_cost, max_risk);
        let ranked = self.actions.ranked(candidates);
        let mut best: Option<(&crate::alphacode_security_core::action::Action, f32)> = None;
        for (action, base_value) in ranked {
            let outcome_bonus = self
                .outcomes
                .get(&action.id)
                .map(|o| (o.evidence_rate() * 0.3 - o.avg_cost() * 0.05).clamp(-0.2, 0.3))
                .unwrap_or(0.0);
            let lesson_adj = lesson_adjustment(&action.id, &self.state.task, &self.lessons);
            let total = base_value + outcome_bonus + lesson_adj;
            if best.map(|(_, v)| total > v).unwrap_or(true) {
                best = Some((action, total));
            }
        }
        best.map(|(a, v)| {
            let reason = format!(
                "value={:.2} gain={:.2} progress={:.2} cost={:.2} risk={:.2} discrimination_bonus={:.2}",
                v,
                a.expected_information_gain,
                a.expected_goal_progress,
                a.estimated_cost,
                a.estimated_risk,
                self.actions.discrimination_bonus(&a.id),
            );
            (a.id.clone(), v, reason)
        })
    }

    /// Record execution outcome and update beliefs + trajectory.
    pub fn record_outcome(
        &mut self,
        action_id: String,
        success: bool,
        useful_evidence: bool,
        info_gain: f32,
        outcome_text: String,
    ) {
        self.state.record_action_attempt(action_id.clone());
        if success {
            self.state.record_action_success(action_id.clone());
        } else {
            self.state.record_action_failure(action_id.clone());
        }
        let cost = 1;
        self.ledger.tool_calls += 1;
        self.outcomes.entry(action_id.clone()).or_default().record(
            success,
            useful_evidence,
            cost,
            info_gain,
        );
        let step = TrajectoryStep {
            seq: 0,
            state_summary: self.state.compact_summary(),
            active_hypotheses: self
                .hypotheses
                .active()
                .iter()
                .map(|h| h.id.clone())
                .collect(),
            candidate_actions: self.actions.actions.keys().cloned().collect(),
            selected_action: Some(action_id),
            selection_reason: format!("info_gain={info_gain:.2} success={success}"),
            expected_information_gain: info_gain,
            outcome: outcome_text,
            belief_delta: format!("evidence_version={}", self.evidence_version),
            verifier_result: None,
            stopping_reason: None,
            memory_consulted: Vec::new(),
            lessons_generated: Vec::new(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.trajectory.push(step);
    }

    /// Adaptive stopping check.
    pub fn stopping_decision(
        &self,
        expected_gain: f32,
        expected_cost: f32,
        consecutive_failures: u32,
        repeated: bool,
    ) -> TerminationState {
        should_stop(
            expected_gain,
            expected_cost,
            consecutive_failures,
            repeated,
            &self.ledger,
        )
    }

    /// Structured arbitration over swarm contributions.
    pub fn arbitrate_swarm(contributions: &[ContributionScore]) -> Vec<ArbitratedContribution> {
        arbitrate(contributions)
    }

    /// Verification gate: combine quality + false-positive defense.
    pub fn verification_verdict(
        quality: &VerificationQuality,
        defense: &FalsePositiveDefense,
    ) -> TerminationState {
        if !defense.is_still_viable() {
            return TerminationState::Disproven;
        }
        match quality.verification_state() {
            super::verification_quality::VerificationState::VerifiedReportable => {
                TerminationState::VerifiedReportable
            }
            super::verification_quality::VerificationState::StrongCandidate => {
                TerminationState::Continue
            }
            super::verification_quality::VerificationState::WeakCandidate => {
                TerminationState::Continue
            }
            super::verification_quality::VerificationState::InsufficientEvidence => {
                TerminationState::InsufficientEvidence
            }
            super::verification_quality::VerificationState::Disputed => TerminationState::Blocked,
            super::verification_quality::VerificationState::AssumptionHeavy => {
                TerminationState::Deferred
            }
        }
    }

    pub fn snapshot(&self) -> AdaptiveSnapshot {
        let traj = self.trajectory.observability_snapshot();
        AdaptiveSnapshot {
            state_summary: self.state.compact_summary(),
            active_hypotheses: self
                .hypotheses
                .active()
                .iter()
                .map(|h| h.id.clone())
                .collect(),
            evidence_items: self.evidence.observations.len(),
            trajectory_steps: traj.steps,
            selected_action: traj.selected_action,
            stopping: None,
            ledger_pressure: self.ledger.pressure(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AdaptiveSnapshot {
    pub state_summary: String,
    pub active_hypotheses: Vec<String>,
    pub evidence_items: usize,
    pub trajectory_steps: usize,
    pub selected_action: Option<String>,
    pub stopping: Option<TerminationState>,
    pub ledger_pressure: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_cycle_observe_select_record() {
        let mut cycle = AdaptiveCycle::new("test engagement".into());
        assert!(cycle.observe("httpx".into(), "200 OK".into(), None));
        // Duplicate is not novel.
        assert!(!cycle.observe("httpx".into(), "200 OK".into(), None));
        let next = cycle.select_next_action(1.0, 0.5);
        assert!(next.is_some());
        let (id, _, _) = next.unwrap();
        cycle.record_outcome(id, true, true, 0.7, "got headers".into());
        assert_eq!(cycle.trajectory.steps.len(), 1);
    }

    #[test]
    fn stopping_and_verification_compose() {
        let cycle = AdaptiveCycle::new("t".into());
        assert_eq!(
            cycle.stopping_decision(0.8, 0.2, 0, false),
            TerminationState::Continue
        );
        let q = VerificationQuality {
            reproducibility: 0.9,
            evidence_strength: 0.9,
            impact_demonstration: 0.9,
            exploitability_evidence: 0.8,
            independence: 0.8,
            contradiction_level: 0.0,
            assumption_count: 0,
        };
        let d = FalsePositiveDefense::new();
        assert_eq!(
            AdaptiveCycle::verification_verdict(&q, &d),
            TerminationState::VerifiedReportable
        );
    }
}
