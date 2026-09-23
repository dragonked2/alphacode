use crate::alphacode_decision_core::engine::DecisionProvider;
use crate::alphacode_decision_core::types::*;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// An LLM-backed decision provider that emulates bounded decisions via
/// structured output generation.
///
/// This is the "emulated" path: a normal LLM is asked for constrained
/// structured output, validated, and converted to typed decisions.
pub struct LlmDecisionProvider {
    provider_name: String,
    model_name: String,
    /// The underlying LLM provider for generation.
    llm: Arc<dyn LlmCompletion>,
}

/// Trait for the underlying LLM completion capability.
/// This is a simplified interface for decision-specific completions.
#[async_trait]
pub trait LlmCompletion: Send + Sync {
    /// Send a prompt and get a structured JSON response.
    async fn complete_json(&self, system_prompt: &str, user_prompt: &str) -> Result<String>;
}

impl LlmDecisionProvider {
    pub fn new(provider_name: &str, model_name: &str, llm: Arc<dyn LlmCompletion>) -> Self {
        Self {
            provider_name: provider_name.to_string(),
            model_name: model_name.to_string(),
            llm,
        }
    }
}

#[async_trait]
impl DecisionProvider for LlmDecisionProvider {
    fn is_native(&self) -> bool {
        false // This is an emulated provider
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn decide_probability(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<ProbabilityDecision> {
        let system = format!(
            "You are a decision engine. Answer ONLY with valid JSON.\n\
             Question: {}\n\
             Type: probability\n\
             Output format: {{\"probability\": <0.0-1.0>, \"confidence\": <0.0-1.0>}}",
            question.description
        );
        let user = format!("State:\n{}", serde_json::to_string_pretty(state)?);

        let raw = self.llm.complete_json(&system, &user).await?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)?;

        let probability = parsed["probability"].as_f64().unwrap_or(0.5) as f32;
        let confidence = parsed["confidence"].as_f64().unwrap_or(0.5) as f32;

        Ok(ProbabilityDecision::new(probability, confidence))
    }

    async fn decide_choice(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<ChoiceDecision> {
        let choices_str = question.allowed_choices.join(", ");
        let system = format!(
            "You are a decision engine. Answer ONLY with valid JSON.\n\
             Question: {}\n\
             Type: choice\n\
             Allowed choices: {}\n\
             Output format: {{\"choice\": \"<choice>\", \"confidence\": <0.0-1.0>, \
             \"probabilities\": {{\"<choice>\": <0.0-1.0>, ...}}}}",
            question.description, choices_str
        );
        let user = format!("State:\n{}", serde_json::to_string_pretty(state)?);

        let raw = self.llm.complete_json(&system, &user).await?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)?;

        let choice = parsed["choice"].as_str().unwrap_or("").to_string();
        let confidence = parsed["confidence"].as_f64().unwrap_or(0.5) as f32;

        let mut probabilities = HashMap::new();
        if let Some(probs_obj) = parsed["probabilities"].as_object() {
            for (k, v) in probs_obj {
                if let Some(p) = v.as_f64() {
                    probabilities.insert(k.clone(), p as f32);
                }
            }
        }

        // Validate choice is in allowed set
        if !question.allowed_choices.is_empty() && !question.allowed_choices.contains(&choice) {
            // Fall back to first allowed choice
            if let Some(first) = question.allowed_choices.first() {
                return Ok(ChoiceDecision::new(
                    first.clone(),
                    probabilities,
                    confidence * 0.5,
                ));
            }
        }

        Ok(ChoiceDecision::new(choice, probabilities, confidence))
    }

    async fn decide_score(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<ScoreDecision> {
        let labels_str = question.score_labels.join(", ");
        let system = format!(
            "You are a decision engine. Answer ONLY with valid JSON.\n\
             Question: {}\n\
             Type: score\n\
             Score labels (low to high): {}\n\
             Output format: {{\"score\": <0.0-1.0>, \"label\": \"<label>\", \
             \"confidence\": <0.0-1.0>}}",
            question.description, labels_str
        );
        let user = format!("State:\n{}", serde_json::to_string_pretty(state)?);

        let raw = self.llm.complete_json(&system, &user).await?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)?;

        let score = parsed["score"].as_f64().unwrap_or(0.5) as f32;
        let confidence = parsed["confidence"].as_f64().unwrap_or(0.5) as f32;
        let label = parsed["label"].as_str().map(String::from);

        let mut result = ScoreDecision::new(score, confidence);
        result.label = label;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockLlm {
        call_count: AtomicUsize,
    }

    #[async_trait]
    impl LlmCompletion for MockLlm {
        async fn complete_json(&self, _system: &str, _user: &str) -> Result<String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(r#"{"probability": 0.85, "confidence": 0.92}"#.to_string())
        }
    }

    #[tokio::test]
    async fn llm_provider_decide_probability() {
        let llm = Arc::new(MockLlm {
            call_count: AtomicUsize::new(0),
        });
        let provider = LlmDecisionProvider::new("test", "test-v1", llm);
        let q = DecisionQuestion::probability("t", "d");
        let state = serde_json::json!({});

        let result = provider.decide_probability(&q, &state).await.unwrap();
        assert_eq!(result.probability, 0.85);
        assert_eq!(result.confidence, 0.92);
        assert!(!provider.is_native());
    }

    struct MockLlmChoice;

    #[async_trait]
    impl LlmCompletion for MockLlmChoice {
        async fn complete_json(&self, _: &str, _: &str) -> Result<String> {
            Ok(r#"{"choice": "inspect", "confidence": 0.81, "probabilities": {"inspect": 0.81, "edit": 0.19}}"#.to_string())
        }
    }

    #[tokio::test]
    async fn llm_provider_decide_choice() {
        let llm = Arc::new(MockLlmChoice);
        let provider = LlmDecisionProvider::new("test", "v1", llm);
        let q = DecisionQuestion::choice("t", "d", vec!["inspect", "edit"]);
        let state = serde_json::json!({});

        let result = provider.decide_choice(&q, &state).await.unwrap();
        assert_eq!(result.choice, "inspect");
        assert_eq!(result.probabilities.len(), 2);
    }

    struct MockLlmBadChoice;

    #[async_trait]
    impl LlmCompletion for MockLlmBadChoice {
        async fn complete_json(&self, _: &str, _: &str) -> Result<String> {
            Ok(
                r#"{"choice": "invalid_choice", "confidence": 0.5, "probabilities": {}}"#
                    .to_string(),
            )
        }
    }

    #[tokio::test]
    async fn llm_provider_validates_choice() {
        let llm = Arc::new(MockLlmBadChoice);
        let provider = LlmDecisionProvider::new("test", "v1", llm);
        let q = DecisionQuestion::choice("t", "d", vec!["a", "b"]);
        let state = serde_json::json!({});

        let result = provider.decide_choice(&q, &state).await.unwrap();
        // Should fall back to first allowed choice
        assert_eq!(result.choice, "a");
        assert!(result.confidence < 0.5); // Reduced confidence
    }
}
