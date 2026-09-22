use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// The outcome of a decision attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DecisionOutcome {
    /// A confident decision was made.
    Decided,
    /// The engine abstained due to insufficient confidence.
    Abstained,
    /// The decision was deferred to a higher-level system (e.g., generative model).
    Deferred,
    /// More context is needed before a decision can be made.
    NeedsMoreContext,
}

impl fmt::Display for DecisionOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decided => write!(f, "DECIDED"),
            Self::Abstained => write!(f, "ABSTAINED"),
            Self::Deferred => write!(f, "DEFERRED"),
            Self::NeedsMoreContext => write!(f, "NEEDS_MORE_CONTEXT"),
        }
    }
}

/// A probability judgment: "How likely is X?"
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbabilityDecision {
    /// The estimated probability [0.0, 1.0].
    pub probability: f32,
    /// Confidence in the probability estimate [0.0, 1.0].
    pub confidence: f32,
    /// The overall outcome of the decision attempt.
    pub outcome: DecisionOutcome,
}

impl ProbabilityDecision {
    pub fn new(probability: f32, confidence: f32) -> Self {
        let probability = clamp_f32(probability);
        let confidence = clamp_f32(confidence);
        let outcome = if confidence >= 0.6 {
            DecisionOutcome::Decided
        } else if confidence >= 0.3 {
            DecisionOutcome::Abstained
        } else {
            DecisionOutcome::NeedsMoreContext
        };
        Self {
            probability,
            confidence,
            outcome,
        }
    }

    pub fn abstained() -> Self {
        Self {
            probability: 0.5,
            confidence: 0.0,
            outcome: DecisionOutcome::Abstained,
        }
    }

    pub fn deferred() -> Self {
        Self {
            probability: 0.5,
            confidence: 0.0,
            outcome: DecisionOutcome::Deferred,
        }
    }
}

/// A choice from a finite set of allowed options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceDecision {
    /// The selected choice (must be one of the declared allowed_choices).
    pub choice: String,
    /// Probability distribution over all allowed choices.
    pub probabilities: HashMap<String, f32>,
    /// Confidence in the choice [0.0, 1.0].
    pub confidence: f32,
    /// The overall outcome.
    pub outcome: DecisionOutcome,
}

impl ChoiceDecision {
    pub fn new(choice: String, probabilities: HashMap<String, f32>, confidence: f32) -> Self {
        let confidence = clamp_f32(confidence);
        let outcome = if confidence >= 0.6 {
            DecisionOutcome::Decided
        } else if confidence >= 0.3 {
            DecisionOutcome::Abstained
        } else {
            DecisionOutcome::NeedsMoreContext
        };
        Self {
            choice,
            probabilities,
            confidence,
            outcome,
        }
    }

    pub fn abstained() -> Self {
        Self {
            choice: String::new(),
            probabilities: HashMap::new(),
            confidence: 0.0,
            outcome: DecisionOutcome::Abstained,
        }
    }

    pub fn deferred() -> Self {
        Self {
            choice: String::new(),
            probabilities: HashMap::new(),
            confidence: 0.0,
            outcome: DecisionOutcome::Deferred,
        }
    }
}

/// An ordinal score with a defined range and optional labels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreDecision {
    /// The numeric score.
    pub score: f32,
    /// The ordinal label (if provided).
    pub label: Option<String>,
    /// Probability distribution across score levels (if provided).
    pub distribution: Option<HashMap<String, f32>>,
    /// Confidence in the score [0.0, 1.0].
    pub confidence: f32,
    /// The overall outcome.
    pub outcome: DecisionOutcome,
}

impl ScoreDecision {
    pub fn new(score: f32, confidence: f32) -> Self {
        let confidence = clamp_f32(confidence);
        let outcome = if confidence >= 0.6 {
            DecisionOutcome::Decided
        } else if confidence >= 0.3 {
            DecisionOutcome::Abstained
        } else {
            DecisionOutcome::NeedsMoreContext
        };
        Self {
            score: clamp_f32(score),
            label: None,
            distribution: None,
            confidence,
            outcome,
        }
    }

    pub fn abstained() -> Self {
        Self {
            score: 0.0,
            label: None,
            distribution: None,
            confidence: 0.0,
            outcome: DecisionOutcome::Abstained,
        }
    }

    pub fn deferred() -> Self {
        Self {
            score: 0.0,
            label: None,
            distribution: None,
            confidence: 0.0,
            outcome: DecisionOutcome::Deferred,
        }
    }
}

/// The type of decision being requested.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DecisionType {
    Probability,
    Choice,
    Score,
}

impl fmt::Display for DecisionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Probability => write!(f, "probability"),
            Self::Choice => write!(f, "choice"),
            Self::Score => write!(f, "score"),
        }
    }
}

/// A typed decision question with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionQuestion {
    /// Unique identifier for this question type (e.g., "agent.tool_result.sufficient").
    pub question_id: String,
    /// Human-readable description.
    pub description: String,
    /// The type of decision.
    pub decision_type: DecisionType,
    /// Allowed choices (for Choice decisions).
    pub allowed_choices: Vec<String>,
    /// Score range labels (for Score decisions).
    pub score_labels: Vec<String>,
    /// Question schema version.
    pub version: u32,
    /// Whether this question can be cached.
    pub cacheable: bool,
}

impl DecisionQuestion {
    pub fn probability(question_id: &str, description: &str) -> Self {
        Self {
            question_id: question_id.to_string(),
            description: description.to_string(),
            decision_type: DecisionType::Probability,
            allowed_choices: vec![],
            score_labels: vec![],
            version: 1,
            cacheable: false,
        }
    }

    pub fn choice(question_id: &str, description: &str, choices: Vec<&str>) -> Self {
        Self {
            question_id: question_id.to_string(),
            description: description.to_string(),
            decision_type: DecisionType::Choice,
            allowed_choices: choices.into_iter().map(String::from).collect(),
            score_labels: vec![],
            version: 1,
            cacheable: false,
        }
    }

    pub fn score(question_id: &str, description: &str, labels: Vec<&str>) -> Self {
        Self {
            question_id: question_id.to_string(),
            description: description.to_string(),
            decision_type: DecisionType::Score,
            allowed_choices: vec![],
            score_labels: labels.into_iter().map(String::from).collect(),
            version: 1,
            cacheable: false,
        }
    }

    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn cacheable(mut self) -> Self {
        self.cacheable = true;
        self
    }
}

/// The result of a decision request, wrapping any decision type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DecisionResult {
    Probability(ProbabilityDecision),
    Choice(ChoiceDecision),
    Score(ScoreDecision),
}

impl DecisionResult {
    pub fn outcome(&self) -> &DecisionOutcome {
        match self {
            Self::Probability(d) => &d.outcome,
            Self::Choice(d) => &d.outcome,
            Self::Score(d) => &d.outcome,
        }
    }

    pub fn confidence(&self) -> f32 {
        match self {
            Self::Probability(d) => d.confidence,
            Self::Choice(d) => d.confidence,
            Self::Score(d) => d.confidence,
        }
    }

    pub fn is_decided(&self) -> bool {
        matches!(self.outcome(), DecisionOutcome::Decided)
    }

    pub fn is_abstained(&self) -> bool {
        matches!(self.outcome(), DecisionOutcome::Abstained)
    }

    pub fn is_deferred(&self) -> bool {
        matches!(self.outcome(), DecisionOutcome::Deferred)
    }
}

/// Metadata about how a decision was produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionMeta {
    /// The question that was asked.
    pub question: DecisionQuestion,
    /// Which provider produced this decision.
    pub provider: String,
    /// Which model produced this decision.
    pub model: String,
    /// Question version used.
    pub question_version: u32,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Whether this was a cache hit.
    pub cache_hit: bool,
    /// Whether this was produced by the native decision engine or emulated via LLM.
    pub native: bool,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// A complete decision record: result + metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub result: DecisionResult,
    pub meta: DecisionMeta,
}

fn clamp_f32(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probability_clamps_and_selects_outcome() {
        let d = ProbabilityDecision::new(1.5, 0.95);
        assert_eq!(d.probability, 1.0);
        assert_eq!(d.confidence, 0.95);
        assert_eq!(d.outcome, DecisionOutcome::Decided);

        let d = ProbabilityDecision::new(0.5, 0.4);
        assert_eq!(d.outcome, DecisionOutcome::Abstained);

        let d = ProbabilityDecision::new(0.5, 0.1);
        assert_eq!(d.outcome, DecisionOutcome::NeedsMoreContext);
    }

    #[test]
    fn probability_nan_becomes_zero() {
        let d = ProbabilityDecision::new(f32::NAN, f32::NAN);
        assert_eq!(d.probability, 0.0);
        assert_eq!(d.confidence, 0.0);
    }

    #[test]
    fn choice_validates_and_selects_outcome() {
        let mut probs = HashMap::new();
        probs.insert("a".into(), 0.7);
        probs.insert("b".into(), 0.3);
        let d = ChoiceDecision::new("a".into(), probs, 0.85);
        assert_eq!(d.choice, "a");
        assert_eq!(d.outcome, DecisionOutcome::Decided);
    }

    #[test]
    fn choice_abstained_has_empty_choice() {
        let d = ChoiceDecision::abstained();
        assert!(d.choice.is_empty());
        assert_eq!(d.outcome, DecisionOutcome::Abstained);
    }

    #[test]
    fn score_clamps_and_selects_outcome() {
        let d = ScoreDecision::new(0.5, 0.9);
        assert_eq!(d.score, 0.5);
        assert_eq!(d.outcome, DecisionOutcome::Decided);

        let d = ScoreDecision::new(-0.5, 0.5);
        assert_eq!(d.score, 0.0);
    }

    #[test]
    fn decision_result_wrappers() {
        let p = DecisionResult::Probability(ProbabilityDecision::new(0.9, 0.8));
        assert!(p.is_decided());
        assert!(!p.is_abstained());
        assert!(!p.is_deferred());
        assert_eq!(p.confidence(), 0.8);

        let c = DecisionResult::Choice(ChoiceDecision::abstained());
        assert!(!c.is_decided());
        assert!(c.is_abstained());
    }

    #[test]
    fn decision_question_constructors() {
        let q = DecisionQuestion::probability("test.prob", "Is it?");
        assert_eq!(q.decision_type, DecisionType::Probability);
        assert!(q.allowed_choices.is_empty());

        let q = DecisionQuestion::choice("test.choice", "Pick one", vec!["a", "b", "c"]);
        assert_eq!(q.allowed_choices.len(), 3);

        let q = DecisionQuestion::score("test.score", "Rate it", vec!["low", "high"]);
        assert_eq!(q.score_labels.len(), 2);
    }

    #[test]
    fn decision_question_version_and_cacheable() {
        let q = DecisionQuestion::probability("t", "d")
            .with_version(3)
            .cacheable();
        assert_eq!(q.version, 3);
        assert!(q.cacheable);
    }
}
