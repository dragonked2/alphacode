use crate::alphacode_decision_core::config::DecisionConfig;
use crate::alphacode_decision_core::types::*;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// The core trait for decision inference providers.
///
/// Implementors provide bounded judgment capabilities. The default implementation
/// uses an LLM-backed emulation via structured output. Future native decision
/// providers can implement this directly.
#[async_trait]
pub trait DecisionProvider: Send + Sync {
    /// Whether this provider supports native decision inference (not emulated via LLM).
    fn is_native(&self) -> bool {
        false
    }

    /// The provider name.
    fn provider_name(&self) -> &str;

    /// The model name.
    fn model_name(&self) -> &str;

    /// Make a probability decision.
    async fn decide_probability(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<ProbabilityDecision>;

    /// Make a choice decision.
    async fn decide_choice(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<ChoiceDecision>;

    /// Make a score decision.
    async fn decide_score(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<ScoreDecision>;
}

/// A deterministic fallback provider that uses heuristics when no LLM is available.
pub struct HeuristicDecisionProvider;

#[async_trait]
impl DecisionProvider for HeuristicDecisionProvider {
    fn provider_name(&self) -> &str {
        "heuristic"
    }

    fn model_name(&self) -> &str {
        "builtin"
    }

    async fn decide_probability(
        &self,
        _question: &DecisionQuestion,
        _state: &serde_json::Value,
    ) -> Result<ProbabilityDecision> {
        Ok(ProbabilityDecision::abstained())
    }

    async fn decide_choice(
        &self,
        question: &DecisionQuestion,
        _state: &serde_json::Value,
    ) -> Result<ChoiceDecision> {
        if let Some(first) = question.allowed_choices.first() {
            let mut probs = HashMap::new();
            let p = 1.0 / question.allowed_choices.len() as f32;
            for c in &question.allowed_choices {
                probs.insert(c.clone(), p);
            }
            Ok(ChoiceDecision::new(first.clone(), probs, 0.35))
        } else {
            Ok(ChoiceDecision::abstained())
        }
    }

    async fn decide_score(
        &self,
        _question: &DecisionQuestion,
        _state: &serde_json::Value,
    ) -> Result<ScoreDecision> {
        Ok(ScoreDecision::abstained())
    }
}

/// A cached decision entry.
#[derive(Debug, Clone)]
struct CacheEntry {
    result: DecisionResult,
    inserted_at: Instant,
}

/// The main Decision Engine.
///
/// Orchestrates decision requests through provider, fallback, caching, and
/// telemetry. The engine is stateless per-request; state is held in the
/// config and provider.
pub struct DecisionEngine {
    config: DecisionConfig,
    provider: Arc<dyn DecisionProvider>,
    fallback: Arc<HeuristicDecisionProvider>,
    cache: std::sync::RwLock<HashMap<String, CacheEntry>>,
    stats: std::sync::Mutex<DecisionStats>,
}

/// Aggregated statistics for the decision engine.
#[derive(Debug, Clone, Default)]
pub struct DecisionStats {
    pub total_requests: u64,
    pub decided: u64,
    pub abstained: u64,
    pub deferred: u64,
    pub needs_more_context: u64,
    pub cache_hits: u64,
    pub fallback_used: u64,
    pub errors: u64,
    pub total_latency_ms: u64,
}

impl DecisionStats {
    pub fn avg_latency_ms(&self) -> u64 {
        if self.total_requests == 0 {
            0
        } else {
            self.total_latency_ms / self.total_requests
        }
    }

    pub fn decision_rate(&self) -> f32 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.decided as f32 / self.total_requests as f32
        }
    }

    pub fn abstention_rate(&self) -> f32 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.abstained as f32 / self.total_requests as f32
        }
    }

    pub fn cache_hit_rate(&self) -> f32 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.cache_hits as f32 / self.total_requests as f32
        }
    }
}

impl DecisionEngine {
    /// Create a new decision engine with the given config and provider.
    pub fn new(config: DecisionConfig, provider: Arc<dyn DecisionProvider>) -> Self {
        Self {
            config,
            provider,
            fallback: Arc::new(HeuristicDecisionProvider),
            cache: std::sync::RwLock::new(HashMap::new()),
            stats: std::sync::Mutex::new(DecisionStats::default()),
        }
    }

    /// Create with heuristic-only provider (no LLM required).
    pub fn heuristic(config: DecisionConfig) -> Self {
        let fallback = Arc::new(HeuristicDecisionProvider);
        Self::new(config, fallback)
    }

    /// Get a snapshot of the current statistics.
    pub fn stats(&self) -> DecisionStats {
        self.stats.lock().unwrap().clone()
    }

    /// Get the current config.
    pub fn config(&self) -> &DecisionConfig {
        &self.config
    }

    /// Whether the engine can influence behavior.
    pub fn can_influence(&self) -> bool {
        self.config.mode.can_influence()
    }

    /// Clear the decision cache.
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Make a probability decision.
    pub async fn decide_probability(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<DecisionResult> {
        assert_eq!(question.decision_type, DecisionType::Probability);

        if !self.config.mode.is_enabled() {
            return Ok(DecisionResult::Probability(ProbabilityDecision::deferred()));
        }

        // Check cache
        if self.config.cache_enabled && question.cacheable {
            let cache_key = self.cache_key(question, state);
            if let Ok(cache) = self.cache.read()
                && let Some(entry) = cache.get(&cache_key)
                && (entry.inserted_at.elapsed().as_secs() < self.config.cache_ttl_secs
                    || self.config.cache_ttl_secs == 0)
            {
                self.stats.lock().unwrap().cache_hits += 1;
                self.stats.lock().unwrap().total_requests += 1;
                return Ok(entry.result.clone());
            }
        }

        let start = Instant::now();
        self.stats.lock().unwrap().total_requests += 1;

        let result = self
            .provider
            .decide_probability(question, state)
            .await
            .map(DecisionResult::Probability);

        let result = match result {
            Ok(r) => r,
            Err(e) => {
                if self.config.fallback_enabled {
                    self.stats.lock().unwrap().fallback_used += 1;
                    self.stats.lock().unwrap().errors += 1;
                    let dummy = serde_json::json!({});
                    DecisionResult::Probability(
                        self.fallback.decide_probability(question, &dummy).await?,
                    )
                } else {
                    self.stats.lock().unwrap().errors += 1;
                    return Err(e);
                }
            }
        };

        self.record_result(question, &result, start);
        self.maybe_cache(question, state, &result);
        Ok(result)
    }

    /// Make a choice decision.
    pub async fn decide_choice(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<DecisionResult> {
        assert_eq!(question.decision_type, DecisionType::Choice);

        if !self.config.mode.is_enabled() {
            return Ok(DecisionResult::Choice(ChoiceDecision::deferred()));
        }

        if self.config.cache_enabled && question.cacheable {
            let cache_key = self.cache_key(question, state);
            if let Ok(cache) = self.cache.read()
                && let Some(entry) = cache.get(&cache_key)
                && (entry.inserted_at.elapsed().as_secs() < self.config.cache_ttl_secs
                    || self.config.cache_ttl_secs == 0)
            {
                self.stats.lock().unwrap().cache_hits += 1;
                self.stats.lock().unwrap().total_requests += 1;
                return Ok(entry.result.clone());
            }
        }

        let start = Instant::now();
        self.stats.lock().unwrap().total_requests += 1;

        let result = self
            .provider
            .decide_choice(question, state)
            .await
            .map(DecisionResult::Choice);

        let result = match result {
            Ok(r) => r,
            Err(e) => {
                if self.config.fallback_enabled {
                    self.stats.lock().unwrap().fallback_used += 1;
                    self.stats.lock().unwrap().errors += 1;
                    let dummy = serde_json::json!({});
                    DecisionResult::Choice(self.fallback.decide_choice(question, &dummy).await?)
                } else {
                    self.stats.lock().unwrap().errors += 1;
                    return Err(e);
                }
            }
        };

        self.record_result(question, &result, start);
        self.maybe_cache(question, state, &result);
        Ok(result)
    }

    /// Make a score decision.
    pub async fn decide_score(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
    ) -> Result<DecisionResult> {
        assert_eq!(question.decision_type, DecisionType::Score);

        if !self.config.mode.is_enabled() {
            return Ok(DecisionResult::Score(ScoreDecision::deferred()));
        }

        if self.config.cache_enabled && question.cacheable {
            let cache_key = self.cache_key(question, state);
            if let Ok(cache) = self.cache.read()
                && let Some(entry) = cache.get(&cache_key)
                && (entry.inserted_at.elapsed().as_secs() < self.config.cache_ttl_secs
                    || self.config.cache_ttl_secs == 0)
            {
                self.stats.lock().unwrap().cache_hits += 1;
                self.stats.lock().unwrap().total_requests += 1;
                return Ok(entry.result.clone());
            }
        }

        let start = Instant::now();
        self.stats.lock().unwrap().total_requests += 1;

        let result = self
            .provider
            .decide_score(question, state)
            .await
            .map(DecisionResult::Score);

        let result = match result {
            Ok(r) => r,
            Err(e) => {
                if self.config.fallback_enabled {
                    self.stats.lock().unwrap().fallback_used += 1;
                    self.stats.lock().unwrap().errors += 1;
                    let dummy = serde_json::json!({});
                    DecisionResult::Score(self.fallback.decide_score(question, &dummy).await?)
                } else {
                    self.stats.lock().unwrap().errors += 1;
                    return Err(e);
                }
            }
        };

        self.record_result(question, &result, start);
        self.maybe_cache(question, state, &result);
        Ok(result)
    }

    // ========================================================================
    // Private Helpers
    // ========================================================================

    /// Record stats for a completed decision.
    fn record_result(&self, _question: &DecisionQuestion, result: &DecisionResult, start: Instant) {
        let latency = start.elapsed().as_millis() as u64;
        self.stats.lock().unwrap().total_latency_ms += latency;
        match result.outcome() {
            DecisionOutcome::Decided => self.stats.lock().unwrap().decided += 1,
            DecisionOutcome::Abstained => self.stats.lock().unwrap().abstained += 1,
            DecisionOutcome::Deferred => self.stats.lock().unwrap().deferred += 1,
            DecisionOutcome::NeedsMoreContext => self.stats.lock().unwrap().needs_more_context += 1,
        }
    }

    /// Cache the result if appropriate.
    fn maybe_cache(
        &self,
        question: &DecisionQuestion,
        state: &serde_json::Value,
        result: &DecisionResult,
    ) {
        if self.config.cache_enabled && question.cacheable && result.is_decided() {
            let cache_key = self.cache_key(question, state);
            if let Ok(mut cache) = self.cache.write() {
                if self.config.cache_max_entries > 0
                    && cache.len() >= self.config.cache_max_entries
                    && let Some(oldest_key) = cache
                        .iter()
                        .min_by_key(|(_, v)| v.inserted_at)
                        .map(|(k, _)| k.clone())
                {
                    cache.remove(&oldest_key);
                }
                cache.insert(
                    cache_key,
                    CacheEntry {
                        result: result.clone(),
                        inserted_at: Instant::now(),
                    },
                );
            }
        }
    }

    /// Compute a cache key from question + state hash.
    fn cache_key(&self, question: &DecisionQuestion, state: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        question.question_id.hash(&mut hasher);
        question.version.hash(&mut hasher);
        // Hash the state as a string representation (deterministic)
        let state_str = serde_json::to_string(state).unwrap_or_default();
        state_str.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl DecisionProvider for MockProvider {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock-v1"
        }
        async fn decide_probability(
            &self,
            _q: &DecisionQuestion,
            _s: &serde_json::Value,
        ) -> Result<ProbabilityDecision> {
            Ok(ProbabilityDecision::new(0.85, 0.9))
        }
        async fn decide_choice(
            &self,
            q: &DecisionQuestion,
            _s: &serde_json::Value,
        ) -> Result<ChoiceDecision> {
            let choice = q.allowed_choices.first().cloned().unwrap_or_default();
            let mut probs = HashMap::new();
            probs.insert(choice.clone(), 0.9);
            Ok(ChoiceDecision::new(choice, probs, 0.85))
        }
        async fn decide_score(
            &self,
            _q: &DecisionQuestion,
            _s: &serde_json::Value,
        ) -> Result<ScoreDecision> {
            Ok(ScoreDecision::new(0.8, 0.75))
        }
    }

    #[tokio::test]
    async fn engine_decides_with_mock_provider() {
        let engine = DecisionEngine::new(DecisionConfig::active(), Arc::new(MockProvider));
        let q = DecisionQuestion::probability("test.prob", "Is it?");
        let state = serde_json::json!({"key": "value"});
        let result = engine.decide_probability(&q, &state).await.unwrap();
        assert!(result.is_decided());
    }

    #[tokio::test]
    async fn engine_abstains_when_off() {
        let engine = DecisionEngine::heuristic(DecisionConfig::off());
        let q = DecisionQuestion::probability("test.prob", "Is it?");
        let state = serde_json::json!({});
        let result = engine.decide_probability(&q, &state).await.unwrap();
        assert!(result.is_deferred());
    }

    #[tokio::test]
    async fn engine_stats_tracking() {
        let engine = DecisionEngine::new(DecisionConfig::active(), Arc::new(MockProvider));
        let q = DecisionQuestion::probability("test.prob", "Is it?");
        let state = serde_json::json!({});

        let _ = engine.decide_probability(&q, &state).await;
        let _ = engine.decide_probability(&q, &state).await;

        let stats = engine.stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.decided, 2);
    }

    #[tokio::test]
    async fn engine_cache_hit() {
        let q = DecisionQuestion::probability("test.prob", "Is it?").cacheable();
        let state = serde_json::json!({"x": 1});

        let engine = DecisionEngine::new(DecisionConfig::active(), Arc::new(MockProvider));
        let _ = engine.decide_probability(&q, &state).await;
        let _ = engine.decide_probability(&q, &state).await;

        let stats = engine.stats();
        assert_eq!(stats.cache_hits, 1);
    }

    #[tokio::test]
    async fn engine_fallback_on_error() {
        struct FailingProvider;
        #[async_trait]
        impl DecisionProvider for FailingProvider {
            fn provider_name(&self) -> &str {
                "failing"
            }
            fn model_name(&self) -> &str {
                "fail-v1"
            }
            async fn decide_probability(
                &self,
                _: &DecisionQuestion,
                _: &serde_json::Value,
            ) -> Result<ProbabilityDecision> {
                Err(anyhow::anyhow!("provider unavailable"))
            }
            async fn decide_choice(
                &self,
                _: &DecisionQuestion,
                _: &serde_json::Value,
            ) -> Result<ChoiceDecision> {
                Err(anyhow::anyhow!("provider unavailable"))
            }
            async fn decide_score(
                &self,
                _: &DecisionQuestion,
                _: &serde_json::Value,
            ) -> Result<ScoreDecision> {
                Err(anyhow::anyhow!("provider unavailable"))
            }
        }

        let engine = DecisionEngine::new(DecisionConfig::active(), Arc::new(FailingProvider));
        let q = DecisionQuestion::choice("test.c", "Pick", vec!["a", "b"]);
        let state = serde_json::json!({});

        // Should fallback to heuristic, not error
        let result = engine.decide_choice(&q, &state).await;
        assert!(result.is_ok());
        let stats = engine.stats();
        assert_eq!(stats.fallback_used, 1);
    }

    #[test]
    fn heuristic_provider_abstains() {
        let hp = HeuristicDecisionProvider;
        let q = DecisionQuestion::probability("t", "d");
        let state = serde_json::json!({});

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let r = hp.decide_probability(&q, &state).await.unwrap();
            assert!(r.outcome == DecisionOutcome::Abstained);
        });
    }
}
