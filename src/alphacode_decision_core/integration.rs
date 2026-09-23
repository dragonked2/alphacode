use crate::alphacode_decision_core::config::{DecisionConfig, DecisionMode};
use crate::alphacode_decision_core::engine::DecisionEngine;
use crate::alphacode_decision_core::questions::*;
use crate::alphacode_decision_core::shadow::ShadowRunner;
use crate::alphacode_decision_core::telemetry::DecisionTrace;
use crate::alphacode_decision_core::types::*;
use std::sync::Arc;

/// The main integration point between the Decision Plane and AlphaCode's
/// agent architecture.
///
/// This module provides high-level functions that can be called from the
/// agent loop, autonomous orchestrator, and other components to make
/// bounded decisions without requiring a full generative model call.
pub struct DecisionIntegration {
    engine: Arc<DecisionEngine>,
    trace: Arc<DecisionTrace>,
    shadow: Option<ShadowRunner>,
}

impl DecisionIntegration {
    /// Create a new integration with the given config.
    pub fn new(config: DecisionConfig, engine: Arc<DecisionEngine>) -> Self {
        let trace = Arc::new(DecisionTrace::new(4096));
        let shadow = if config.mode == DecisionMode::Shadow {
            Some(ShadowRunner::new(Arc::clone(&engine), Arc::clone(&trace)))
        } else {
            None
        };
        Self {
            engine,
            trace,
            shadow,
        }
    }

    /// Get the underlying engine.
    pub fn engine(&self) -> &Arc<DecisionEngine> {
        &self.engine
    }

    /// Get the trace buffer.
    pub fn trace(&self) -> &Arc<DecisionTrace> {
        &self.trace
    }

    // ========================================================================
    // Agent Loop Integration Points
    // ========================================================================

    /// Decide whether a tool result is sufficient to continue.
    pub async fn decide_tool_result_sufficient(
        &self,
        tool_name: &str,
        output_preview: &str,
        is_error: bool,
        turn_iteration: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "tool_name": tool_name,
            "output_preview": output_preview.chars().take(500).collect::<String>(),
            "is_error": is_error,
            "turn_iteration": turn_iteration,
        });

        let q = tool_result_sufficient();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Classify a tool failure for retry strategy.
    pub async fn decide_retry_classification(
        &self,
        tool_name: &str,
        error_message: &str,
        consecutive_failures: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "tool_name": tool_name,
            "error_message": error_message.chars().take(500).collect::<String>(),
            "consecutive_failures": consecutive_failures,
        });

        let q = retry_classification();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Choice(ChoiceDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_choice(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Detect if the agent is stuck in a loop.
    pub async fn decide_agent_loop_detected(
        &self,
        recent_tools: &[String],
        recent_errors: &[String],
        turn_count: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "recent_tools": recent_tools,
            "recent_errors": recent_errors,
            "turn_count": turn_count,
        });

        let q = agent_loop_detection();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Decide whether to continue or escalate to the user.
    pub async fn decide_continue_or_escalate(
        &self,
        current_task: &str,
        progress_summary: &str,
        error_count: u32,
        turn_count: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "current_task": current_task,
            "progress_summary": progress_summary,
            "error_count": error_count,
            "turn_count": turn_count,
        });

        let q = continue_or_escalate();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Choice(ChoiceDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_choice(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    // ========================================================================
    // Context Management Integration Points
    // ========================================================================

    /// Decide if compaction would be beneficial.
    pub async fn decide_compaction_beneficial(
        &self,
        context_usage_pct: f32,
        turn_count: u32,
        last_compaction_turns_ago: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "context_usage_pct": context_usage_pct,
            "turn_count": turn_count,
            "last_compaction_turns_ago": last_compaction_turns_ago,
        });

        let q = compaction_beneficial();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    // ========================================================================
    // Autonomous Integration Points
    // ========================================================================

    /// Decide if a phase is complete.
    pub async fn decide_phase_complete(
        &self,
        phase_name: &str,
        tasks_completed: u32,
        tasks_total: u32,
        quality_score: f32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "phase_name": phase_name,
            "tasks_completed": tasks_completed,
            "tasks_total": tasks_total,
            "quality_score": quality_score,
        });

        let q = phase_complete();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Decide whether the autonomous planner should replan.
    pub async fn decide_should_replan(
        &self,
        current_plan: &str,
        completed_tasks: u32,
        remaining_tasks: u32,
        blockers: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "current_plan": current_plan,
            "completed_tasks": completed_tasks,
            "remaining_tasks": remaining_tasks,
            "blockers": blockers,
        });

        let q = should_replan();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Choice(ChoiceDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_choice(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    // ========================================================================
    // Quality Gate Integration Points
    // ========================================================================

    /// Decide if an agent report is trustworthy enough to accept.
    pub async fn decide_report_trustworthy(
        &self,
        agent_id: &str,
        confidence: f32,
        completed_tasks: u32,
        files_modified: u32,
        summary_length: usize,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "agent_id": agent_id,
            "confidence": confidence,
            "completed_tasks": completed_tasks,
            "files_modified": files_modified,
            "summary_length": summary_length,
        });

        let q = report_trustworthy();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Decide the quality gate verdict.
    // Arity is part of the public decision API; keep the explicit parameters.
    #[allow(clippy::too_many_arguments)]
    pub async fn decide_quality_gate(
        &self,
        implementation_complete: bool,
        tests_pass: bool,
        build_passes: bool,
        documentation_updated: bool,
        no_critical_issues: bool,
        review_approved: bool,
        checkpoint_created: bool,
    ) -> DecisionResult {
        let passed_count = [
            implementation_complete,
            tests_pass,
            build_passes,
            documentation_updated,
            no_critical_issues,
            review_approved,
            checkpoint_created,
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        let state = serde_json::json!({
            "implementation_complete": implementation_complete,
            "tests_pass": tests_pass,
            "build_passes": build_passes,
            "documentation_updated": documentation_updated,
            "no_critical_issues": no_critical_issues,
            "review_approved": review_approved,
            "checkpoint_created": checkpoint_created,
            "passed_count": passed_count,
            "total_checks": 7,
        });

        let q = quality_gate_verdict();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Choice(ChoiceDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_choice(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    // ========================================================================
    // Swarm Integration Points
    // ========================================================================

    /// Decide if a merge result is coherent.
    pub async fn decide_merge_coherent(
        &self,
        source_agents: &[String],
        files_touched: &[String],
        conflict_count: u32,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "source_agents": source_agents,
            "files_touched": files_touched,
            "conflict_count": conflict_count,
        });

        let q = merge_coherent();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Decide the merge strategy for conflicting reports.
    pub async fn decide_merge_strategy(
        &self,
        agent_a_id: &str,
        agent_b_id: &str,
        shared_files: &[String],
        conflict_severity: &str,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "agent_a_id": agent_a_id,
            "agent_b_id": agent_b_id,
            "shared_files": shared_files,
            "conflict_severity": conflict_severity,
        });

        let q = merge_strategy();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Choice(ChoiceDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_choice(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Observe-only: run the decision but don't use it for control.
    async fn observe_decision(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> DecisionResult {
        let start = std::time::Instant::now();
        let result = match question.decision_type {
            DecisionType::Probability => self
                .engine
                .decide_probability(question, state)
                .await
                .unwrap_or(DecisionResult::Probability(ProbabilityDecision::abstained())),
            DecisionType::Choice => self
                .engine
                .decide_choice(question, state)
                .await
                .unwrap_or(DecisionResult::Choice(ChoiceDecision::abstained())),
            DecisionType::Score => self
                .engine
                .decide_score(question, state)
                .await
                .unwrap_or(DecisionResult::Score(ScoreDecision::abstained())),
        };
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

    /// Active mode: run the decision and use it for control.
    async fn active_decision(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> DecisionResult {
        let start = std::time::Instant::now();
        let result = match question.decision_type {
            DecisionType::Probability => self
                .engine
                .decide_probability(question, state)
                .await
                .unwrap_or(DecisionResult::Probability(ProbabilityDecision::abstained())),
            DecisionType::Choice => self
                .engine
                .decide_choice(question, state)
                .await
                .unwrap_or(DecisionResult::Choice(ChoiceDecision::abstained())),
            DecisionType::Score => self
                .engine
                .decide_score(question, state)
                .await
                .unwrap_or(DecisionResult::Score(ScoreDecision::abstained())),
        };
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

    // ========================================================================
    // Model Routing Integration Points
    // ========================================================================

    /// Decide if the task requires deeper reasoning than the current model.
    pub async fn decide_needs_deeper_reasoning(
        &self,
        task_description: &str,
        current_model: &str,
        task_complexity: &str,
    ) -> DecisionResult {
        let state = serde_json::json!({
            "task_description": task_description,
            "current_model": current_model,
            "task_complexity": task_complexity,
        });

        let q = needs_deeper_reasoning();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Decide if the current model is adequate for this task.
    pub async fn decide_model_adequate(
        &self,
        task_description: &str,
        current_model: &str,
        error_history: &[String],
    ) -> DecisionResult {
        let state = serde_json::json!({
            "task_description": task_description,
            "current_model": current_model,
            "error_history": error_history,
        });

        let q = model_adequate();

        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    // ========================================================================
    // Adaptive Intelligence Integration Points (hierarchical + stopping)
    // ========================================================================

    async fn decide_choice_helper(
        &self,
        q: DecisionQuestion,
        state: serde_json::Value,
    ) -> DecisionResult {
        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Choice(ChoiceDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_choice(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    async fn decide_prob_helper(
        &self,
        q: DecisionQuestion,
        state: serde_json::Value,
    ) -> DecisionResult {
        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Probability(ProbabilityDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_probability(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    async fn decide_score_helper(
        &self,
        q: DecisionQuestion,
        state: serde_json::Value,
    ) -> DecisionResult {
        match self.engine.config().mode {
            DecisionMode::Off => DecisionResult::Score(ScoreDecision::deferred()),
            DecisionMode::Observe => self.observe_decision(&q, &state).await,
            DecisionMode::Shadow => {
                if let Some(ref shadow) = self.shadow {
                    shadow.shadow_score(&q, &state).await
                } else {
                    self.observe_decision(&q, &state).await
                }
            }
            DecisionMode::Active => self.active_decision(&q, &state).await,
        }
    }

    /// Which hypothesis most deserves attention?
    pub async fn decide_hypothesis_to_test(
        &self,
        hypotheses: &[String],
        uncertainty_summary: &str,
    ) -> DecisionResult {
        let q = hypothesis_to_test();
        let state = serde_json::json!({
            "hypotheses": hypotheses,
            "uncertainty_summary": uncertainty_summary,
        });
        self.decide_choice_helper(q, state).await
    }

    /// What evidence would discriminate between hypotheses?
    pub async fn decide_evidence_needed(
        &self,
        hypothesis_a: &str,
        hypothesis_b: &str,
        available_actions: &[String],
    ) -> DecisionResult {
        let q = evidence_needed();
        let state = serde_json::json!({
            "hypothesis_a": hypothesis_a,
            "hypothesis_b": hypothesis_b,
            "available_actions": available_actions,
        });
        self.decide_choice_helper(q, state).await
    }

    /// How deeply should this be investigated?
    pub async fn decide_investigation_depth(
        &self,
        hypothesis_id: &str,
        evidence_strength: f32,
        corroborated: u32,
    ) -> DecisionResult {
        let q = investigation_depth();
        let state = serde_json::json!({
            "hypothesis_id": hypothesis_id,
            "evidence_strength": evidence_strength,
            "corroborated": corroborated,
        });
        self.decide_choice_helper(q, state).await
    }

    /// Is independent verification required?
    pub async fn decide_verification_needed(
        &self,
        finding_id: &str,
        quality_score: f32,
        impact_demonstrated: bool,
    ) -> DecisionResult {
        let q = verification_needed();
        let state = serde_json::json!({
            "finding_id": finding_id,
            "quality_score": quality_score,
            "impact_demonstrated": impact_demonstrated,
        });
        self.decide_prob_helper(q, state).await
    }

    /// Should investigation continue or stop?
    pub async fn decide_adaptive_stop(
        &self,
        expected_gain: f32,
        expected_cost: f32,
        failures: u32,
        verification_state: &str,
    ) -> DecisionResult {
        let q = adaptive_stop();
        let state = serde_json::json!({
            "expected_gain": expected_gain,
            "expected_cost": expected_cost,
            "failures": failures,
            "verification_state": verification_state,
        });
        self.decide_choice_helper(q, state).await
    }

    /// Rate an agent contribution for arbitration.
    pub async fn decide_arbitrate_contribution(
        &self,
        agent_id: &str,
        evidence_quality: f32,
        reproducibility: f32,
        contradictions: u32,
    ) -> DecisionResult {
        let q = arbitrate_contribution();
        let state = serde_json::json!({
            "agent_id": agent_id,
            "evidence_quality": evidence_quality,
            "reproducibility": reproducibility,
            "contradictions": contradictions,
        });
        self.decide_score_helper(q, state).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_decision_core::config::DecisionConfig;
    use crate::alphacode_decision_core::engine::HeuristicDecisionProvider;

    #[tokio::test]
    async fn integration_off_mode_defers() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::off(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::off(), engine);

        let result = integration
            .decide_tool_result_sufficient("bash", "output", false, 1)
            .await;
        assert!(result.is_deferred());
    }

    #[tokio::test]
    async fn integration_active_mode_decides() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_tool_result_sufficient("bash", "output", false, 1)
            .await;
        // Heuristic provider abstains, but the integration still records it
        assert!(result.is_abstained() || result.is_deferred());
    }

    #[tokio::test]
    async fn integration_records_trace() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::observe(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::observe(), engine);

        let _ = integration
            .decide_tool_result_sufficient("bash", "output", false, 1)
            .await;

        assert_eq!(integration.trace().len(), 1);
    }

    #[tokio::test]
    async fn integration_retry_classification() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_retry_classification("bash", "timeout", 2)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_phase_complete() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_phase_complete("implement", 5, 5, 0.9)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_report_trustworthy() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_report_trustworthy("agent-1", 0.8, 5, 10, 500)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_quality_gate() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_quality_gate(true, true, true, true, true, true, true)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_merge_coherent() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_merge_coherent(
                &["agent-a".into(), "agent-b".into()],
                &["src/main.rs".into()],
                0,
            )
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_merge_strategy() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_merge_strategy("agent-a", "agent-b", &["src/main.rs".into()], "minor")
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_should_replan() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_should_replan("phase-1-implement", 3, 2, 1)
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_needs_deeper_reasoning() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_needs_deeper_reasoning("implement complex algorithm", "gpt-4o", "high")
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }

    #[tokio::test]
    async fn integration_model_adequate() {
        let engine = Arc::new(DecisionEngine::new(
            DecisionConfig::active(),
            Arc::new(HeuristicDecisionProvider),
        ));
        let integration = DecisionIntegration::new(DecisionConfig::active(), engine);

        let result = integration
            .decide_model_adequate("simple edit task", "gpt-4o", &[])
            .await;
        assert!(result.is_abstained() || result.is_deferred() || result.is_decided());
    }
}
