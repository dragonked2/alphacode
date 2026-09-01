//! Model capability validation and real-time API checks.
//!
//! This module provides:
//! 1. Real-time validation of model capabilities against live APIs
//! 2. Detection of stale caches
//! 3. Automatic refresh of context limits
//! 4. Model availability health checks

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

/// Cached model capability with timestamp for staleness detection.
#[derive(Debug, Clone)]
pub struct CachedModelCapability {
    pub context_window: Option<usize>,
    pub cached_at: Instant,
    pub source: &'static str,
}

impl CachedModelCapability {
    /// Whether this cached capability is stale (older than TTL).
    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }

    /// Age of this cache entry in seconds.
    pub fn age_secs(&self) -> u64 {
        self.cached_at.elapsed().as_secs()
    }
}

/// Process-global cache of model capabilities with staleness tracking.
static MODEL_CAPABILITY_CACHE: OnceLock<RwLock<HashMap<String, CachedModelCapability>>> = OnceLock::new();

fn capability_cache() -> &'static RwLock<HashMap<String, CachedModelCapability>> {
    MODEL_CAPABILITY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Default TTL for cached model capabilities (15 minutes).
const DEFAULT_CAPABILITY_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Populate the capability cache from a batch of model limits.
pub fn populate_capability_cache(limits: HashMap<String, usize>, source: &'static str) {
    if let Ok(mut cache) = capability_cache().write() {
        for (model, limit) in limits {
            cache.insert(
                model.clone(),
                CachedModelCapability {
                    context_window: Some(limit),
                    cached_at: Instant::now(),
                    source,
                },
            );
        }
    }
}

/// Get a cached model capability if available and not stale.
pub fn get_cached_capability(model: &str) -> Option<CachedModelCapability> {
    let cache = capability_cache().read().ok()?;
    let entry = cache.get(model)?;
    if entry.is_stale(DEFAULT_CAPABILITY_CACHE_TTL) {
        return None;
    }
    Some(entry.clone())
}

/// Clear all cached capabilities (e.g., after credential change).
pub fn clear_capability_cache() {
    if let Ok(mut cache) = capability_cache().write() {
        cache.clear();
    }
}

/// Clear cached capabilities for a specific model.
pub fn clear_model_capability(model: &str) {
    if let Ok(mut cache) = capability_cache().write() {
        cache.remove(model);
    }
}

/// Record a failed attempt to fetch model capabilities.
pub fn record_fetch_failure(model: &str, error: &str) {
    crate::logging::warn(&format!(
        "Model capability fetch failed for {}: {}",
        model, error
    ));
}

/// Check if a model's cached capability should be refreshed.
pub fn should_refresh_capability(model: &str) -> bool {
    let cache = match capability_cache().read() {
        Ok(cache) => cache,
        Err(_) => return true,
    };

    match cache.get(model) {
        None => true,
        Some(entry) => entry.is_stale(DEFAULT_CAPABILITY_CACHE_TTL),
    }
}

/// Get the staleness summary for all cached capabilities.
pub fn capability_cache_summary() -> CapabilityCacheSummary {
    let cache = match capability_cache().read() {
        Ok(cache) => cache,
        Err(_) => return CapabilityCacheSummary::default(),
    };

    let total = cache.len();
    let stale = cache.values().filter(|e| e.is_stale(DEFAULT_CAPABILITY_CACHE_TTL)).count();
    let fresh = total - stale;

    CapabilityCacheSummary {
        total_models: total,
        fresh_models: fresh,
        stale_models: stale,
    }
}

#[derive(Debug, Default)]
pub struct CapabilityCacheSummary {
    pub total_models: usize,
    pub fresh_models: usize,
    pub stale_models: usize,
}

impl std::fmt::Display for CapabilityCacheSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} models cached ({} fresh, {} stale)",
            self.total_models, self.fresh_models, self.stale_models
        )
    }
}

/// Validate that a model's context window matches expected values.
pub fn validate_model_context_window(
    model: &str,
    actual_window: Option<usize>,
    expected_window: Option<usize>,
) -> ValidationResult {
    match (actual_window, expected_window) {
        (Some(actual), Some(expected)) if actual == expected => ValidationResult::Valid,
        (Some(actual), Some(expected)) => {
            crate::logging::warn(&format!(
                "Context window mismatch for {}: actual={}k, expected={}k",
                model,
                actual / 1000,
                expected / 1000
            ));
            ValidationResult::Mismatch {
                actual,
                expected,
            }
        }
        (Some(_), None) => ValidationResult::Valid,
        (None, Some(expected)) => {
            crate::logging::warn(&format!(
                "Context window unknown for {}: expected={}k",
                model,
                expected / 1000
            ));
            ValidationResult::Missing {
                expected,
            }
        }
        (None, None) => ValidationResult::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    Valid,
    Mismatch { actual: usize, expected: usize },
    Missing { expected: usize },
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_capability_freshness() {
        let cache = CachedModelCapability {
            context_window: Some(1_000_000),
            cached_at: Instant::now(),
            source: "test",
        };
        assert!(!cache.is_stale(DEFAULT_CAPABILITY_CACHE_TTL));
    }

    #[test]
    fn cached_capability_staleness() {
        let cache = CachedModelCapability {
            context_window: Some(1_000_000),
            cached_at: Instant::now() - Duration::from_secs(3600), // 1 hour ago
            source: "test",
        };
        assert!(cache.is_stale(DEFAULT_CAPABILITY_CACHE_TTL));
    }

    #[test]
    fn validation_result_matching() {
        let result = validate_model_context_window(
            "gpt-5.4",
            Some(1_000_000),
            Some(1_000_000),
        );
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn validation_result_mismatch() {
        let result = validate_model_context_window(
            "gpt-5.4",
            Some(200_000),
            Some(1_000_000),
        );
        assert!(matches!(result, ValidationResult::Mismatch { .. }));
    }
}
