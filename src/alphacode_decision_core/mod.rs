//! Decision Plane for AlphaCode.
//!
//! Provides bounded, typed judgment capabilities that complement the existing
//! generative reasoning system. The Decision Plane handles small, repeated,
//! measurable decisions without requiring a full LLM generation cycle.
//!
//! # Architecture
//!
//! ```text
//!                    ALPHACODE
//!                        │
//!          ┌─────────────┴─────────────┐
//!          │                           │
//!          ▼                           ▼
//!   GENERATIVE PLANE              DECISION PLANE
//!          │                           │
//!   reasoning/model             fast bounded judgment
//!          │                           │
//!   code generation             probability
//!   investigation               choice
//!   planning                    score
//!   synthesis                   confidence
//!          │                           │
//!          └─────────────┬─────────────┘
//!                        ▼
//!                 AGENT ORCHESTRATOR
//! ```
//!
//! # Operating Modes
//!
//! - **Off**: Decision Plane disabled, existing behavior preserved
//! - **Observe**: Records decisions without influencing behavior
//! - **Shadow**: Records decisions for A/B comparison
//! - **Active**: Decisions can influence approved decision points
//!
//! # Decision Types
//!
//! - **Probability**: "How likely is X?" (0.0-1.0 + confidence)
//! - **Choice**: "Which of these options?" (finite set + probabilities)
//! - **Score**: "Rate this on a scale" (ordinal + confidence)

pub mod agent_loop;
pub mod config;
pub mod engine;
pub mod integration;
pub mod llm_provider;
pub mod questions;
pub mod selection;
pub mod shadow;
pub mod telemetry;
pub mod types;

pub use agent_loop::{AgentAction, AgentDecisionLoop};
pub use config::{DecisionConfig, DecisionMode, DecisionThresholds};
pub use engine::{DecisionEngine, DecisionProvider, DecisionStats, HeuristicDecisionProvider};
pub use integration::DecisionIntegration;
pub use llm_provider::{LlmCompletion, LlmDecisionProvider};
pub use questions::*;
pub use selection::{
    CounterfactualEvaluation, HierarchyLevel, InvestigationLevel, select_best_evaluation,
    suggest_depth,
};
pub use shadow::{ShadowComparison, ShadowRunner};
pub use telemetry::{DecisionTelemetrySummary, DecisionTrace, DecisionTraceEvent};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_smoke_test() {
        let config = DecisionConfig::default();
        assert_eq!(config.mode, DecisionMode::Observe);

        let engine = DecisionEngine::heuristic(config);
        let stats = engine.stats();
        assert_eq!(stats.total_requests, 0);
    }

    #[test]
    fn all_question_definitions_valid() {
        let q = tool_result_sufficient();
        assert_eq!(q.decision_type, DecisionType::Probability);

        let q = retry_classification();
        assert_eq!(q.decision_type, DecisionType::Choice);
        assert!(!q.allowed_choices.is_empty());

        let q = parallelism_justified();
        assert_eq!(q.decision_type, DecisionType::Score);
        assert!(!q.score_labels.is_empty());
    }

    #[test]
    fn decision_config_serialization_roundtrip() {
        let config = DecisionConfig::active();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: DecisionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn decision_mode_serialization() {
        let modes = vec![
            DecisionMode::Off,
            DecisionMode::Observe,
            DecisionMode::Shadow,
            DecisionMode::Active,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: DecisionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, parsed);
        }
    }

    #[tokio::test]
    async fn full_decision_flow() {
        use std::sync::Arc;
        let engine = Arc::new(DecisionEngine::heuristic(DecisionConfig::active()));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        // Probability decision
        let result = integration
            .decide_tool_result_sufficient("bash", "command executed", false, 1)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());

        // Choice decision
        let result = integration
            .decide_retry_classification("bash", "timeout", 1)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());

        // Score decision
        let result = integration
            .decide_phase_complete("implement", 3, 5, 0.7)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());

        // Check trace recorded all decisions
        assert_eq!(integration.trace().len(), 3);
    }
}
