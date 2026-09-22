use crate::alphacode_decision_core::engine::DecisionEngine;
use crate::alphacode_decision_core::telemetry::DecisionTrace;
use crate::alphacode_decision_core::types::*;
use std::sync::Arc;

/// Shadow mode runner: records what the Decision Plane WOULD have decided
/// without altering execution. Used for A/B comparison before activation.
pub struct ShadowRunner {
    engine: Arc<DecisionEngine>,
    trace: Arc<DecisionTrace>,
}

/// A shadow decision record: what the decision plane said vs what actually happened.
#[derive(Debug, Clone)]
pub struct ShadowRecord {
    pub question_id: String,
    pub decision_outcome: DecisionOutcome,
    pub decision_confidence: f32,
    pub actual_action: String,
    pub agreed: Option<bool>,
}

impl ShadowRunner {
    pub fn new(engine: Arc<DecisionEngine>, trace: Arc<DecisionTrace>) -> Self {
        Self { engine, trace }
    }

    /// Run a decision in shadow mode: records the decision but does not
    /// influence behavior. Returns the decision for logging purposes.
    pub async fn shadow_probability(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> DecisionResult {
        let start = std::time::Instant::now();
        let result = self
            .engine
            .decide_probability(question, state)
            .await
            .unwrap_or_else(|_| DecisionResult::Probability(ProbabilityDecision::abstained()));
        let latency = start.elapsed().as_millis() as u64;

        self.trace.record(
            question,
            &result,
            self.engine.config().provider.as_str(),
            self.engine.config().model.as_str(),
            latency,
            false,
            false,
        );

        result
    }

    /// Run a choice decision in shadow mode.
    pub async fn shadow_choice(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> DecisionResult {
        let start = std::time::Instant::now();
        let result = self
            .engine
            .decide_choice(question, state)
            .await
            .unwrap_or_else(|_| DecisionResult::Choice(ChoiceDecision::abstained()));
        let latency = start.elapsed().as_millis() as u64;

        self.trace.record(
            question,
            &result,
            self.engine.config().provider.as_str(),
            self.engine.config().model.as_str(),
            latency,
            false,
            false,
        );

        result
    }

    /// Run a score decision in shadow mode.
    pub async fn shadow_score(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> DecisionResult {
        let start = std::time::Instant::now();
        let result = self
            .engine
            .decide_score(question, state)
            .await
            .unwrap_or_else(|_| DecisionResult::Score(ScoreDecision::abstained()));
        let latency = start.elapsed().as_millis() as u64;

        self.trace.record(
            question,
            &result,
            self.engine.config().provider.as_str(),
            self.engine.config().model.as_str(),
            latency,
            false,
            false,
        );

        result
    }

    /// Compare shadow decisions against actual actions to measure agreement.
    pub fn compare_decisions(shadow: &[ShadowRecord]) -> ShadowComparison {
        let total = shadow.len();
        let agreed = shadow.iter().filter(|r| r.agreed == Some(true)).count();
        let disagreed = shadow.iter().filter(|r| r.agreed == Some(false)).count();
        let unknown = total - agreed - disagreed;

        let high_conf_agreed = shadow
            .iter()
            .filter(|r| r.agreed == Some(true) && r.decision_confidence >= 0.8)
            .count();
        let high_conf_total = shadow
            .iter()
            .filter(|r| r.decision_confidence >= 0.8)
            .count();

        ShadowComparison {
            total,
            agreed,
            disagreed,
            unknown,
            agreement_rate: if total > 0 {
                agreed as f32 / total as f32
            } else {
                0.0
            },
            high_confidence_agreement_rate: if high_conf_total > 0 {
                high_conf_agreed as f32 / high_conf_total as f32
            } else {
                0.0
            },
        }
    }
}

/// Summary of shadow mode comparison.
#[derive(Debug, Clone, Default)]
pub struct ShadowComparison {
    pub total: usize,
    pub agreed: usize,
    pub disagreed: usize,
    pub unknown: usize,
    pub agreement_rate: f32,
    pub high_confidence_agreement_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_decision_core::config::DecisionConfig;
    use crate::alphacode_decision_core::engine::HeuristicDecisionProvider;

    #[tokio::test]
    async fn shadow_runner_records_decisions() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::observe(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let trace = Arc::new(DecisionTrace::new(100));
        let runner = ShadowRunner::new(engine, trace.clone());

        let q = DecisionQuestion::probability("test.prob", "Is it?");
        let state = serde_json::json!({});

        let _ = runner.shadow_probability(&q, &state).await;
        assert_eq!(trace.len(), 1);
    }

    #[test]
    fn shadow_comparison_calculation() {
        let records = vec![
            ShadowRecord {
                question_id: "q1".into(),
                decision_outcome: DecisionOutcome::Decided,
                decision_confidence: 0.9,
                actual_action: "continue".into(),
                agreed: Some(true),
            },
            ShadowRecord {
                question_id: "q2".into(),
                decision_outcome: DecisionOutcome::Decided,
                decision_confidence: 0.85,
                actual_action: "retry".into(),
                agreed: Some(false),
            },
            ShadowRecord {
                question_id: "q3".into(),
                decision_outcome: DecisionOutcome::Abstained,
                decision_confidence: 0.2,
                actual_action: "defer".into(),
                agreed: None,
            },
        ];

        let comparison = ShadowRunner::compare_decisions(&records);
        assert_eq!(comparison.total, 3);
        assert_eq!(comparison.agreed, 1);
        assert_eq!(comparison.disagreed, 1);
        assert_eq!(comparison.unknown, 1);
        assert!((comparison.agreement_rate - 0.333).abs() < 0.01);
    }
}
