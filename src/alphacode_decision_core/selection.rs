use serde::{Deserialize, Serialize};

/// Investigation depth — adaptive escalation, not always-maximum.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationLevel {
    Reconnaissance,
    InitialAnalysis,
    FocusedInvestigation,
    DeepInvestigation,
    AdversarialVerification,
    FinalConfirmation,
}

impl InvestigationLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reconnaissance => "reconnaissance",
            Self::InitialAnalysis => "initial_analysis",
            Self::FocusedInvestigation => "focused_investigation",
            Self::DeepInvestigation => "deep_investigation",
            Self::AdversarialVerification => "adversarial_verification",
            Self::FinalConfirmation => "final_confirmation",
        }
    }

    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Reconnaissance => Some(Self::InitialAnalysis),
            Self::InitialAnalysis => Some(Self::FocusedInvestigation),
            Self::FocusedInvestigation => Some(Self::DeepInvestigation),
            Self::DeepInvestigation => Some(Self::AdversarialVerification),
            Self::AdversarialVerification => Some(Self::FinalConfirmation),
            Self::FinalConfirmation => None,
        }
    }

    pub fn max_cost(&self) -> f32 {
        match self {
            Self::Reconnaissance => 0.3,
            Self::InitialAnalysis => 0.4,
            Self::FocusedInvestigation => 0.6,
            Self::DeepInvestigation => 0.8,
            Self::AdversarialVerification => 0.9,
            Self::FinalConfirmation => 1.0,
        }
    }

    pub fn max_risk(&self) -> f32 {
        match self {
            Self::Reconnaissance => 0.1,
            Self::InitialAnalysis => 0.15,
            Self::FocusedInvestigation => 0.25,
            Self::DeepInvestigation => 0.35,
            Self::AdversarialVerification => 0.3,
            Self::FinalConfirmation => 0.2,
        }
    }
}

/// Hierarchical decision levels — never a single "what next?" question.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyLevel {
    Mission,
    Objective,
    Investigation,
    Hypothesis,
    EvidenceRequirement,
    Action,
    ExecutionDepth,
    Verification,
    StopContinue,
}

impl HierarchyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mission => "mission",
            Self::Objective => "objective",
            Self::Investigation => "investigation",
            Self::Hypothesis => "hypothesis",
            Self::EvidenceRequirement => "evidence_requirement",
            Self::Action => "action",
            Self::ExecutionDepth => "execution_depth",
            Self::Verification => "verification",
            Self::StopContinue => "stop_continue",
        }
    }
}

/// Counterfactual evaluation of one candidate action before paying its cost.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CounterfactualEvaluation {
    pub action_id: String,
    /// What we learn if it succeeds.
    pub if_success_learn: String,
    /// What we learn if it fails (failures are information too).
    pub if_fail_learn: String,
    /// Which hypothesis pairs it distinguishes.
    pub distinguishes: Vec<(String, String)>,
    /// Uncertainty that would remain afterwards.
    pub residual_uncertainty: f32,
    /// Cheaper action with similar information, if any.
    pub cheaper_alternative: Option<String>,
    /// Could this produce a misleading result?
    pub misleading_risk: f32,
    pub expected_cost: f32,
    pub expected_risk: f32,
    /// value = info_gain + goal_progress + discrimination - cost - risk - misleading.
    pub value: f32,
}

impl CounterfactualEvaluation {
    // Arity is part of the public decision API; keep the explicit parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        action_id: String,
        info_gain: f32,
        goal_progress: f32,
        distinguishes: Vec<(String, String)>,
        expected_cost: f32,
        expected_risk: f32,
        misleading_risk: f32,
        residual_uncertainty: f32,
        cheaper_alternative: Option<String>,
    ) -> Self {
        let discrimination = (distinguishes.len() as f32 * 0.25).min(0.75);
        let value = info_gain + goal_progress + discrimination
            - expected_cost
            - expected_risk
            - misleading_risk;
        Self {
            action_id,
            if_success_learn: format!("gain={info_gain:.2} discrimination={discrimination:.2}"),
            if_fail_learn: "failure eliminates this path; try alternative".to_string(),
            distinguishes,
            residual_uncertainty,
            cheaper_alternative,
            misleading_risk,
            expected_cost,
            expected_risk,
            value: value.clamp(-3.0, 3.0),
        }
    }

    /// Prefer actions that teach us something even on failure and do not mislead.
    pub fn is_worthwhile(&self, min_value: f32) -> bool {
        self.value >= min_value && self.misleading_risk < 0.7 && self.expected_risk < 0.6
    }
}

/// Pick the best evaluation deterministically. No model call.
pub fn select_best_evaluation(
    evals: &[CounterfactualEvaluation],
) -> Option<&CounterfactualEvaluation> {
    evals
        .iter()
        .filter(|e| e.is_worthwhile(-1.0))
        .max_by(|a, b| a.value.total_cmp(&b.value))
}

/// Escalate depth only on signal; de-escalate on weak evidence.
/// Pure heuristic: corroborated / reproduced evidence escalates.
pub fn suggest_depth(
    current: &InvestigationLevel,
    corroborated_count: u32,
    reproduced: bool,
    open_contradictions: u32,
) -> InvestigationLevel {
    if reproduced {
        return InvestigationLevel::AdversarialVerification;
    }
    if open_contradictions > 0 {
        return InvestigationLevel::FocusedInvestigation;
    }
    match (current, corroborated_count) {
        (InvestigationLevel::Reconnaissance, 1..) => InvestigationLevel::InitialAnalysis,
        (InvestigationLevel::InitialAnalysis, 1..) => InvestigationLevel::FocusedInvestigation,
        (InvestigationLevel::FocusedInvestigation, 2..) => InvestigationLevel::DeepInvestigation,
        (InvestigationLevel::DeepInvestigation, 2..) => InvestigationLevel::AdversarialVerification,
        _ => current.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_costs_increase() {
        assert!(
            InvestigationLevel::Reconnaissance.max_cost()
                < InvestigationLevel::DeepInvestigation.max_cost()
        );
    }

    #[test]
    fn counterfactual_value_prefers_discriminating() {
        let plain = CounterfactualEvaluation::evaluate(
            "scan".into(),
            0.3,
            0.2,
            vec![],
            0.4,
            0.1,
            0.1,
            0.8,
            None,
        );
        let sharp = CounterfactualEvaluation::evaluate(
            "auth_compare".into(),
            0.5,
            0.3,
            vec![("h1".into(), "h2".into())],
            0.3,
            0.1,
            0.05,
            0.4,
            None,
        );
        assert!(sharp.value > plain.value);
        assert!(sharp.is_worthwhile(-1.0));
    }

    #[test]
    fn misleading_actions_rejected() {
        let risky = CounterfactualEvaluation::evaluate(
            "x".into(),
            0.9,
            0.9,
            vec![],
            0.1,
            0.1,
            0.9,
            0.1,
            None,
        );
        assert!(!risky.is_worthwhile(0.0));
    }

    #[test]
    fn depth_escalates_on_reproduction() {
        let d = suggest_depth(&InvestigationLevel::InitialAnalysis, 0, true, 0);
        assert_eq!(d, InvestigationLevel::AdversarialVerification);
    }
}
