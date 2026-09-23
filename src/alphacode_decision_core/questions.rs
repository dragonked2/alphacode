use crate::alphacode_decision_core::types::DecisionQuestion;

// Predefined decision questions for common AlphaCode decision points.
//
// These are versioned, typed, and testable. Each question represents a
// bounded judgment that can potentially be answered by the Decision Plane
// instead of requiring a full generative model call.
// ============================================================================
// Agent Loop Decision Points
// ============================================================================

/// "Is the current tool result sufficient to continue without another tool call?"
pub fn tool_result_sufficient() -> DecisionQuestion {
    DecisionQuestion::probability(
        "agent.tool_result.sufficient",
        "Is the tool result sufficient to proceed without gathering more information?",
    )
    .with_version(1)
}

/// "Should we retry this failed tool call?"
pub fn retry_classification() -> DecisionQuestion {
    DecisionQuestion::choice(
        "agent.retry.classify",
        "How should this tool failure be classified?",
        vec!["transient", "permanent", "degraded", "unknown"],
    )
    .with_version(1)
}

/// "Is the agent trajectory stuck in a loop?"
pub fn agent_loop_detection() -> DecisionQuestion {
    DecisionQuestion::probability(
        "agent.trajectory.stuck",
        "Is the current agent trajectory likely stuck in a repetitive loop?",
    )
    .with_version(1)
}

/// "Should the agent continue or ask the user?"
pub fn continue_or_escalate() -> DecisionQuestion {
    DecisionQuestion::choice(
        "agent.control.continue_or_escalate",
        "Should the agent continue autonomously or escalate to the user?",
        vec!["continue", "ask_user", "change_strategy", "stop"],
    )
    .with_version(1)
}

/// "Which tool category is most appropriate for the current need?"
pub fn tool_category_selection() -> DecisionQuestion {
    DecisionQuestion::choice(
        "agent.tool.category",
        "Which tool category best addresses the current need?",
        vec![
            "read", "write", "edit", "search", "execute", "browser", "memory", "swarm", "ask_user",
        ],
    )
    .with_version(1)
}

// ============================================================================
// Context Management Decision Points
// ============================================================================

/// "Is compaction likely beneficial given current context state?"
pub fn compaction_beneficial() -> DecisionQuestion {
    DecisionQuestion::probability(
        "context.compaction.beneficial",
        "Would compacting the context materially improve subsequent reasoning?",
    )
    .with_version(1)
}

/// "Would additional project memory materially improve this decision?"
pub fn memory_retrieval_useful() -> DecisionQuestion {
    DecisionQuestion::probability(
        "context.memory.useful",
        "Would retrieving project memory materially improve the current decision?",
    )
    .with_version(1)
}

// ============================================================================
// Autonomous Orchestration Decision Points
// ============================================================================

/// "Is this task decomposable into independent subtasks?"
pub fn task_decomposable() -> DecisionQuestion {
    DecisionQuestion::probability(
        "autonomous.task.decomposable",
        "Is this task meaningfully decomposable into independent subtasks?",
    )
    .with_version(1)
}

/// "How many independent workstreams are justified?"
pub fn parallelism_justified() -> DecisionQuestion {
    DecisionQuestion::score(
        "autonomous.parallelism.how_many",
        "How many independent parallel workstreams are justified by this task?",
        vec!["none", "few_2_3", "moderate_4_6", "many_7_plus"],
    )
    .with_version(1)
}

/// "Is this phase complete?"
pub fn phase_complete() -> DecisionQuestion {
    DecisionQuestion::probability(
        "autonomous.phase.complete",
        "Does the evidence indicate this autonomous phase is complete?",
    )
    .with_version(1)
}

/// "Should the autonomous planner replan?"
pub fn should_replan() -> DecisionQuestion {
    DecisionQuestion::choice(
        "autonomous.planner.action",
        "What should the autonomous planner do next?",
        vec!["continue", "replan", "delegate", "complete", "ask_user"],
    )
    .with_version(1)
}

// ============================================================================
// Quality Gate Decision Points
// ============================================================================

/// "Is this child-agent report trustworthy enough?"
pub fn report_trustworthy() -> DecisionQuestion {
    DecisionQuestion::probability(
        "quality.report.trustworthy",
        "Is this child-agent report trustworthy enough to accept?",
    )
    .with_version(1)
}

/// "Should the quality gate fail?"
pub fn quality_gate_verdict() -> DecisionQuestion {
    DecisionQuestion::choice(
        "quality.gate.verdict",
        "What is the quality gate verdict?",
        vec!["pass", "conditional_pass", "fail", "needs_review"],
    )
    .with_version(1)
}

// ============================================================================
// Swarm Decision Points
// ============================================================================

/// "Is this merge result coherent?"
pub fn merge_coherent() -> DecisionQuestion {
    DecisionQuestion::probability(
        "swarm.merge.coherent",
        "Is the merged result from parallel agents coherent and conflict-free?",
    )
    .with_version(1)
}

/// "Should this agent report be merged or kept separate?"
pub fn merge_strategy() -> DecisionQuestion {
    DecisionQuestion::choice(
        "swarm.merge.strategy",
        "How should these agent reports be combined?",
        vec!["merge", "keep_separate", "reject_one", "request_revision"],
    )
    .with_version(1)
}

// ============================================================================
// Model Routing Decision Points
// ============================================================================

/// "Does this task require deeper reasoning than the current model provides?"
pub fn needs_deeper_reasoning() -> DecisionQuestion {
    DecisionQuestion::probability(
        "routing.needs_deeper_reasoning",
        "Does this task require deeper reasoning than the current model provides?",
    )
    .with_version(1)
}

/// "Is the current model adequate for this task?"
pub fn model_adequate() -> DecisionQuestion {
    DecisionQuestion::probability(
        "routing.model_adequate",
        "Is the current model adequate for this task's complexity?",
    )
    .with_version(1)
}

// ============================================================================
// Adaptive Intelligence Decision Points (info-gain, hierarchy, stopping)
// ============================================================================

/// "Which hypothesis most deserves attention right now?"
pub fn hypothesis_to_test() -> DecisionQuestion {
    DecisionQuestion::choice(
        "adaptive.hypothesis.which",
        "Which competing hypothesis deserves attention given uncertainty and evidence?",
        vec!["h1", "h2", "h3", "defer"],
    )
    .with_version(1)
}

/// "What evidence would discriminate between these hypotheses?"
pub fn evidence_needed() -> DecisionQuestion {
    DecisionQuestion::choice(
        "adaptive.evidence.needed",
        "What evidence would best discriminate between competing hypotheses?",
        vec![
            "reproduction",
            "control_comparison",
            "auth_comparison",
            "impact_demo",
            "independent_verify",
        ],
    )
    .with_version(1)
}

/// "Which action has the highest expected information value?"
pub fn action_value() -> DecisionQuestion {
    DecisionQuestion::score(
        "adaptive.action.value",
        "Rate this action's expected information value per unit cost.",
        vec!["low", "medium", "high", "critical"],
    )
    .with_version(1)
}

/// "How deeply should this be investigated?"
pub fn investigation_depth() -> DecisionQuestion {
    DecisionQuestion::choice(
        "adaptive.depth.level",
        "How deeply should this hypothesis be investigated?",
        vec![
            "reconnaissance",
            "initial",
            "focused",
            "deep",
            "adversarial",
            "final",
        ],
    )
    .with_version(1)
}

/// "Is independent verification required before reporting?"
pub fn verification_needed() -> DecisionQuestion {
    DecisionQuestion::probability(
        "adaptive.verification.needed",
        "Does this candidate require independent adversarial verification?",
    )
    .with_version(1)
}

/// "Should investigation continue or stop?"
pub fn adaptive_stop() -> DecisionQuestion {
    DecisionQuestion::choice(
        "adaptive.control.stop_or_continue",
        "Should investigation continue, change strategy, delegate, or stop?",
        vec![
            "continue",
            "change_strategy",
            "delegate",
            "stop_confirmed",
            "stop_disproven",
            "stop_insufficient",
            "stop_low_value",
            "stop_blocked",
        ],
    )
    .with_version(1)
}

/// "Which agent contribution should be trusted?"
pub fn arbitrate_contribution() -> DecisionQuestion {
    DecisionQuestion::score(
        "adaptive.swarm.arbitrate",
        "Rate this agent contribution's evidence quality and trustworthiness.",
        vec!["reject", "weak", "acceptable", "strong"],
    )
    .with_version(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_decision_core::types::DecisionType;

    #[test]
    fn all_questions_have_valid_structure() {
        let questions = vec![
            tool_result_sufficient(),
            retry_classification(),
            agent_loop_detection(),
            continue_or_escalate(),
            tool_category_selection(),
            compaction_beneficial(),
            memory_retrieval_useful(),
            task_decomposable(),
            parallelism_justified(),
            phase_complete(),
            should_replan(),
            report_trustworthy(),
            quality_gate_verdict(),
            merge_coherent(),
            merge_strategy(),
            needs_deeper_reasoning(),
            model_adequate(),
            hypothesis_to_test(),
            evidence_needed(),
            action_value(),
            investigation_depth(),
            verification_needed(),
            adaptive_stop(),
            arbitrate_contribution(),
        ];

        for q in &questions {
            assert!(!q.question_id.is_empty());
            assert!(!q.description.is_empty());
            assert!(q.version >= 1);

            match q.decision_type {
                DecisionType::Probability => {
                    assert!(q.allowed_choices.is_empty());
                }
                DecisionType::Choice => {
                    assert!(
                        !q.allowed_choices.is_empty(),
                        "Choice question '{}' must have allowed_choices",
                        q.question_id
                    );
                }
                DecisionType::Score => {
                    assert!(
                        !q.score_labels.is_empty(),
                        "Score question '{}' must have score_labels",
                        q.question_id
                    );
                }
            }
        }
    }

    #[test]
    fn question_ids_are_unique() {
        let questions = vec![
            tool_result_sufficient(),
            retry_classification(),
            agent_loop_detection(),
            continue_or_escalate(),
            tool_category_selection(),
            compaction_beneficial(),
            memory_retrieval_useful(),
            task_decomposable(),
            parallelism_justified(),
            phase_complete(),
            should_replan(),
            report_trustworthy(),
            quality_gate_verdict(),
            merge_coherent(),
            merge_strategy(),
            needs_deeper_reasoning(),
            model_adequate(),
            hypothesis_to_test(),
            evidence_needed(),
            action_value(),
            investigation_depth(),
            verification_needed(),
            adaptive_stop(),
            arbitrate_contribution(),
        ];

        let mut ids: Vec<_> = questions.iter().map(|q| &q.question_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), questions.len(), "Duplicate question IDs found");
    }
}
