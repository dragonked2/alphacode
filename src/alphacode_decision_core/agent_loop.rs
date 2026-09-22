use crate::alphacode_decision_core::config::{DecisionConfig, DecisionMode};
use crate::alphacode_decision_core::engine::DecisionEngine;
use crate::alphacode_decision_core::questions::*;
use crate::alphacode_decision_core::telemetry::DecisionTrace;
use crate::alphacode_decision_core::types::*;
use std::collections::VecDeque;
use std::sync::Arc;

/// Lightweight agent-loop integration for the Decision Plane.
///
/// Tracks agent state and provides decision methods at decision points.
/// Does NOT own the agent -- observes state and makes suggestions.
pub struct AgentDecisionLoop {
    engine: Arc<DecisionEngine>,
    trace: Arc<DecisionTrace>,
    recent_tools: VecDeque<String>,
    recent_errors: VecDeque<String>,
    turn_iteration: u32,
    total_turns: u32,
    continue_count: u32,
    max_continues: u32,
    mode: DecisionMode,
}

/// The action the agent should take after a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAction {
    Continue,
    Retry { strategy: String },
    AskUser,
    ChangeStrategy,
    Stop,
    DeferToModel,
}

impl AgentDecisionLoop {
    pub fn new(config: DecisionConfig) -> Self {
        let mode = config.mode;
        let engine = Arc::new(DecisionEngine::new(
            config,
            Arc::new(crate::alphacode_decision_core::engine::HeuristicDecisionProvider),
        ));
        let trace = Arc::new(DecisionTrace::new(4096));
        Self {
            engine,
            trace,
            recent_tools: VecDeque::with_capacity(32),
            recent_errors: VecDeque::with_capacity(16),
            turn_iteration: 0,
            total_turns: 0,
            continue_count: 0,
            max_continues: 20,
            mode,
        }
    }

    pub fn with_engine(engine: Arc<DecisionEngine>) -> Self {
        let mode = engine.config().mode;
        let trace = Arc::new(DecisionTrace::new(4096));
        Self {
            engine,
            trace,
            recent_tools: VecDeque::with_capacity(32),
            recent_errors: VecDeque::with_capacity(16),
            turn_iteration: 0,
            total_turns: 0,
            continue_count: 0,
            max_continues: 20,
            mode,
        }
    }

    pub fn engine(&self) -> &Arc<DecisionEngine> {
        &self.engine
    }

    pub fn trace(&self) -> &Arc<DecisionTrace> {
        &self.trace
    }

    // ========================================================================
    // State tracking
    // ========================================================================

    pub fn record_tool(&mut self, tool_name: &str) {
        self.recent_tools.push_back(tool_name.to_string());
        if self.recent_tools.len() > 32 {
            self.recent_tools.pop_front();
        }
        self.turn_iteration += 1;
    }

    pub fn record_error(&mut self, error: &str) {
        self.recent_errors.push_back(error.to_string());
        if self.recent_errors.len() > 16 {
            self.recent_errors.pop_front();
        }
    }

    pub fn record_turn_start(&mut self) {
        self.total_turns += 1;
        self.turn_iteration = 0;
        self.continue_count = 0;
        self.recent_tools.clear();
        self.recent_errors.clear();
    }

    pub fn recent_tools(&self) -> Vec<&str> {
        self.recent_tools.iter().map(|s| s.as_str()).collect()
    }

    pub fn recent_errors(&self) -> Vec<&str> {
        self.recent_errors.iter().map(|s| s.as_str()).collect()
    }

    pub fn turn_iteration(&self) -> u32 {
        self.turn_iteration
    }

    pub fn total_turns(&self) -> u32 {
        self.total_turns
    }

    // ========================================================================
    // Decision points
    // ========================================================================

    /// Decide whether to continue or escalate after all tools in a batch.
    pub async fn decide_continue_or_escalate(
        &mut self,
        current_task: &str,
        progress: &str,
        error_count: u32,
    ) -> AgentAction {
        if !self.mode.is_enabled() || self.mode == DecisionMode::Off {
            return AgentAction::Continue;
        }

        // Hard limit: force escalation if too many continues
        self.continue_count += 1;
        if self.continue_count >= self.max_continues {
            return AgentAction::AskUser;
        }

        let result = self
            .engine
            .decide_choice(
                &continue_or_escalate(),
                &serde_json::json!({
                    "current_task": current_task,
                    "progress_summary": progress,
                    "error_count": error_count,
                    "turn_count": self.total_turns,
                    "tool_iterations": self.turn_iteration,
                    "continue_count": self.continue_count,
                }),
            )
            .await;

        match result {
            Ok(DecisionResult::Choice(choice)) => match choice.choice.as_str() {
                "continue" => AgentAction::Continue,
                "ask_user" => AgentAction::AskUser,
                "change_strategy" => AgentAction::ChangeStrategy,
                "stop" => AgentAction::Stop,
                _ => AgentAction::Continue,
            },
            _ => AgentAction::DeferToModel,
        }
    }

    /// Classify a tool failure for retry strategy.
    pub async fn decide_retry_strategy(
        &self,
        tool_name: &str,
        error_message: &str,
        consecutive_failures: u32,
    ) -> AgentAction {
        if !self.mode.is_enabled() || self.mode == DecisionMode::Off {
            return AgentAction::DeferToModel;
        }

        let result = self
            .engine
            .decide_choice(
                &retry_classification(),
                &serde_json::json!({
                    "tool_name": tool_name,
                    "error_message": error_message,
                    "consecutive_failures": consecutive_failures,
                }),
            )
            .await;

        match result {
            Ok(DecisionResult::Choice(choice)) => match choice.choice.as_str() {
                "transient" => AgentAction::Retry {
                    strategy: "retry_same".to_string(),
                },
                "permanent" => AgentAction::ChangeStrategy,
                "degraded" => AgentAction::ChangeStrategy,
                _ => AgentAction::DeferToModel,
            },
            _ => AgentAction::DeferToModel,
        }
    }

    /// Detect if the agent is stuck in a loop.
    pub async fn decide_loop_detected(&self) -> bool {
        if !self.mode.is_enabled() || self.mode == DecisionMode::Off {
            // Fallback heuristic: same tool 3+ times in last 5
            return self.heuristic_loop_detected();
        }

        let result = self
            .engine
            .decide_probability(
                &agent_loop_detection(),
                &serde_json::json!({
                    "recent_tools": self.recent_tools(),
                    "recent_errors": self.recent_errors(),
                    "turn_count": self.total_turns,
                    "tool_iterations": self.turn_iteration,
                }),
            )
            .await;

        match result {
            Ok(DecisionResult::Probability(p)) => p.probability >= 0.7,
            _ => self.heuristic_loop_detected(),
        }
    }

    /// Decide if a tool result is sufficient to continue without more tool calls.
    pub async fn decide_tool_result_sufficient(
        &self,
        tool_name: &str,
        output_preview: &str,
        is_error: bool,
    ) -> bool {
        if !self.mode.is_enabled() || self.mode == DecisionMode::Off {
            return !is_error;
        }

        let result = self
            .engine
            .decide_probability(
                &tool_result_sufficient(),
                &serde_json::json!({
                    "tool_name": tool_name,
                    "output_preview": output_preview,
                    "is_error": is_error,
                    "turn_iteration": self.turn_iteration,
                }),
            )
            .await;

        match result {
            Ok(DecisionResult::Probability(p)) => p.probability >= 0.5,
            _ => !is_error,
        }
    }

    /// Decide if compaction would be beneficial.
    pub async fn decide_compaction_beneficial(
        &self,
        context_usage_pct: f32,
        last_compaction_turns_ago: u32,
    ) -> bool {
        if !self.mode.is_enabled() || self.mode == DecisionMode::Off {
            return context_usage_pct >= 0.85;
        }

        let result = self
            .engine
            .decide_probability(
                &compaction_beneficial(),
                &serde_json::json!({
                    "context_usage_pct": context_usage_pct,
                    "turn_count": self.total_turns,
                    "last_compaction_turns_ago": last_compaction_turns_ago,
                }),
            )
            .await;

        match result {
            Ok(DecisionResult::Probability(p)) => p.probability >= 0.6,
            _ => context_usage_pct >= 0.85,
        }
    }

    // ========================================================================
    // Heuristics (fallback when Decision Plane is off)
    // ========================================================================

    fn heuristic_loop_detected(&self) -> bool {
        if self.recent_tools.len() < 3 {
            return false;
        }
        // Check if the same tool appears 3+ times in the last 5
        let recent: Vec<&str> = self
            .recent_tools
            .iter()
            .rev()
            .take(5)
            .map(|s| s.as_str())
            .collect();
        if recent.len() < 3 {
            return false;
        }
        let first = recent[0];
        recent.iter().filter(|&&t| t == first).count() >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_loop_detection_catches_same_tool() {
        let mut loop_decision = AgentDecisionLoop::new(DecisionConfig::off());
        // Same tool 3 times
        loop_decision.record_tool("bash");
        loop_decision.record_tool("bash");
        loop_decision.record_tool("bash");
        assert!(loop_decision.heuristic_loop_detected());
    }

    #[test]
    fn heuristic_loop_detection_passes_diverse() {
        let mut loop_decision = AgentDecisionLoop::new(DecisionConfig::off());
        loop_decision.record_tool("bash");
        loop_decision.record_tool("read");
        loop_decision.record_tool("write");
        assert!(!loop_decision.heuristic_loop_detected());
    }

    #[test]
    fn agent_action_equality() {
        assert_eq!(AgentAction::Continue, AgentAction::Continue);
        assert_eq!(
            AgentAction::Retry {
                strategy: "a".into()
            },
            AgentAction::Retry {
                strategy: "a".into()
            }
        );
        assert_ne!(
            AgentAction::Retry {
                strategy: "a".into()
            },
            AgentAction::Retry {
                strategy: "b".into()
            }
        );
    }

    #[tokio::test]
    async fn off_mode_returns_continue() {
        let mut decision = AgentDecisionLoop::new(DecisionConfig::off());
        decision.record_tool("bash");
        let action = decision
            .decide_continue_or_escalate("task", "progress", 0)
            .await;
        assert_eq!(action, AgentAction::Continue);
    }

    #[tokio::test]
    async fn state_tracking_works() {
        let mut decision = AgentDecisionLoop::new(DecisionConfig::off());
        assert_eq!(decision.turn_iteration(), 0);

        decision.record_tool("bash");
        decision.record_tool("read");
        assert_eq!(decision.turn_iteration(), 2);
        assert_eq!(decision.recent_tools().len(), 2);
        assert_eq!(decision.recent_tools()[0], "bash");
        assert_eq!(decision.recent_tools()[1], "read");

        decision.record_error("timeout");
        assert_eq!(decision.recent_errors().len(), 1);

        decision.record_turn_start();
        assert_eq!(decision.turn_iteration(), 0);
        assert_eq!(decision.recent_tools().len(), 0);
        assert_eq!(decision.recent_errors().len(), 0);
        assert_eq!(decision.total_turns(), 1);
    }

    #[tokio::test]
    async fn max_continues_forces_escalation() {
        let mut decision = AgentDecisionLoop::new(DecisionConfig::active());
        decision.max_continues = 3;
        decision.continue_count = 3;
        let action = decision
            .decide_continue_or_escalate("task", "progress", 0)
            .await;
        assert_eq!(action, AgentAction::AskUser);
    }

    #[tokio::test]
    async fn retry_strategy_returns_valid_action() {
        let decision = AgentDecisionLoop::new(DecisionConfig::active());
        let action = decision
            .decide_retry_strategy("bash", "connection reset", 1)
            .await;
        // Heuristic provider abstains, so we get DeferToModel
        assert!(matches!(
            action,
            AgentAction::DeferToModel | AgentAction::Retry { .. } | AgentAction::ChangeStrategy
        ));
    }

    #[tokio::test]
    async fn loop_detection_heuristic_fallback() {
        let mut decision = AgentDecisionLoop::new(DecisionConfig::off());
        decision.record_tool("bash");
        decision.record_tool("bash");
        decision.record_tool("bash");
        // In Off mode, uses heuristic
        assert!(decision.decide_loop_detected().await);
    }
}
