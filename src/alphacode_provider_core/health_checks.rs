//! Model availability health checks and automatic context limit refresh.
//!
//! This module provides:
//! 1. Model availability health checks
//! 2. Automatic context limit refresh on model switch
//! 3. Model capability diff detection for stale caches

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

/// Model health status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelHealthStatus {
    /// Model is healthy and available.
    Healthy,
    /// Model is degraded (high latency, partial failures).
    Degraded { reason: String },
    /// Model is unavailable.
    Unavailable { reason: String },
    /// Model health is unknown.
    Unknown,
}

/// Model health check result.
#[derive(Debug, Clone)]
pub struct ModelHealthCheck {
    pub model: String,
    pub status: ModelHealthStatus,
    pub checked_at: Instant,
    pub latency_ms: Option<u64>,
}

impl ModelHealthCheck {
    /// Whether this health check is stale (older than TTL).
    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.checked_at.elapsed() > ttl
    }
}

/// Process-global model health cache.
static MODEL_HEALTH_CACHE: OnceLock<RwLock<HashMap<String, ModelHealthCheck>>> = OnceLock::new();

fn health_cache() -> &'static RwLock<HashMap<String, ModelHealthCheck>> {
    MODEL_HEALTH_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Default TTL for health checks (5 minutes).
const HEALTH_CHECK_TTL: Duration = Duration::from_secs(5 * 60);

/// Record a model health check result.
pub fn record_health_check(check: ModelHealthCheck) {
    if let Ok(mut cache) = health_cache().write() {
        cache.insert(check.model.clone(), check);
    }
}

/// Get a model's health status.
pub fn get_model_health(model: &str) -> ModelHealthStatus {
    let cache = match health_cache().read() {
        Ok(cache) => cache,
        Err(_) => return ModelHealthStatus::Unknown,
    };

    match cache.get(model) {
        Some(check) if !check.is_stale(HEALTH_CHECK_TTL) => check.status.clone(),
        _ => ModelHealthStatus::Unknown,
    }
}

/// Clear all health checks (e.g., after credential change).
pub fn clear_health_cache() {
    if let Ok(mut cache) = health_cache().write() {
        cache.clear();
    }
}

/// Clear health checks for a specific model.
pub fn clear_model_health(model: &str) {
    if let Ok(mut cache) = health_cache().write() {
        cache.remove(model);
    }
}

/// Context limit refresh state for automatic refresh on model switch.
#[derive(Debug, Clone)]
pub struct ContextLimitRefreshState {
    pub model: String,
    pub last_refreshed: Instant,
    pub refresh_count: u32,
}

impl ContextLimitRefreshState {
    /// Whether this state needs refresh (older than TTL).
    pub fn needs_refresh(&self, ttl: Duration) -> bool {
        self.last_refreshed.elapsed() > ttl
    }
}

/// Process-global context limit refresh state.
static CONTEXT_LIMIT_REFRESH_STATE: OnceLock<RwLock<HashMap<String, ContextLimitRefreshState>>> = OnceLock::new();

fn context_limit_state() -> &'static RwLock<HashMap<String, ContextLimitRefreshState>> {
    CONTEXT_LIMIT_REFRESH_STATE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Default TTL for context limit refresh (10 minutes).
const CONTEXT_LIMIT_REFRESH_TTL: Duration = Duration::from_secs(10 * 60);

/// Record a context limit refresh for a model.
pub fn record_context_limit_refresh(model: &str) {
    if let Ok(mut state) = context_limit_state().write() {
        let entry = state.entry(model.to_string()).or_insert_with(|| ContextLimitRefreshState {
            model: model.to_string(),
            last_refreshed: Instant::now(),
            refresh_count: 0,
        });
        entry.last_refreshed = Instant::now();
        entry.refresh_count += 1;
    }
}

/// Check if a model's context limit needs refresh.
pub fn context_limit_needs_refresh(model: &str) -> bool {
    let state = match context_limit_state().read() {
        Ok(state) => state,
        Err(_) => return true,
    };

    match state.get(model) {
        Some(entry) => entry.needs_refresh(CONTEXT_LIMIT_REFRESH_TTL),
        None => true,
    }
}

/// Get the context limit refresh state for a model.
pub fn get_context_limit_refresh_state(model: &str) -> Option<ContextLimitRefreshState> {
    let state = context_limit_state().read().ok()?;
    state.get(model).cloned()
}

/// Clear all context limit refresh states.
pub fn clear_context_limit_refresh_states() {
    if let Ok(mut state) = context_limit_state().write() {
        state.clear();
    }
}

/// Model capability diff for detecting stale caches.
#[derive(Debug, Clone)]
pub struct ModelCapabilityDiff {
    pub model: String,
    pub field: &'static str,
    pub old_value: Option<usize>,
    pub new_value: Option<usize>,
    pub detected_at: Instant,
}

/// Process-global model capability diffs.
static MODEL_CAPABILITY_DIFFS: OnceLock<RwLock<Vec<ModelCapabilityDiff>>> = OnceLock::new();

fn capability_diffs() -> &'static RwLock<Vec<ModelCapabilityDiff>> {
    MODEL_CAPABILITY_DIFFS.get_or_init(|| RwLock::new(Vec::new()))
}

/// Record a model capability diff.
pub fn record_capability_diff(diff: ModelCapabilityDiff) {
    if let Ok(mut diffs) = capability_diffs().write() {
        // Keep only the most recent diffs (max 1000)
        if diffs.len() > 1000 {
            diffs.drain(0..500);
        }
        diffs.push(diff);
    }
}

/// Get recent model capability diffs.
pub fn get_recent_capability_diffs(max_age: Duration) -> Vec<ModelCapabilityDiff> {
    let diffs = match capability_diffs().read() {
        Ok(diffs) => diffs,
        Err(_) => return Vec::new(),
    };

    diffs
        .iter()
        .filter(|d| d.detected_at.elapsed() < max_age)
        .cloned()
        .collect()
}

/// Clear all model capability diffs.
pub fn clear_capability_diffs() {
    if let Ok(mut diffs) = capability_diffs().write() {
        diffs.clear();
    }
}

/// Detect capability changes between two sets of model capabilities.
pub fn detect_capability_changes(
    old_capabilities: &HashMap<String, usize>,
    new_capabilities: &HashMap<String, usize>,
) -> Vec<ModelCapabilityDiff> {
    let mut diffs = Vec::new();

    // Check for changed or new capabilities
    for (model, &new_limit) in new_capabilities {
        match old_capabilities.get(model) {
            Some(&old_limit) if old_limit != new_limit => {
                diffs.push(ModelCapabilityDiff {
                    model: model.clone(),
                    field: "context_window",
                    old_value: Some(old_limit),
                    new_value: Some(new_limit),
                    detected_at: Instant::now(),
                });
            }
            None => {
                diffs.push(ModelCapabilityDiff {
                    model: model.clone(),
                    field: "context_window",
                    old_value: None,
                    new_value: Some(new_limit),
                    detected_at: Instant::now(),
                });
            }
            _ => {}
        }
    }

    // Check for removed capabilities
    for (model, &old_limit) in old_capabilities {
        if !new_capabilities.contains_key(model) {
            diffs.push(ModelCapabilityDiff {
                model: model.clone(),
                field: "context_window",
                old_value: Some(old_limit),
                new_value: None,
                detected_at: Instant::now(),
            });
        }
    }

    diffs
}

/// Health check summary for reporting.
#[derive(Debug, Default)]
pub struct HealthCheckSummary {
    pub total_models: usize,
    pub healthy_models: usize,
    pub degraded_models: usize,
    pub unavailable_models: usize,
    pub unknown_models: usize,
}

impl std::fmt::Display for HealthCheckSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Health: {}/{} healthy, {} degraded, {} unavailable, {} unknown",
            self.healthy_models,
            self.total_models,
            self.degraded_models,
            self.unavailable_models,
            self.unknown_models
        )
    }
}

/// Get a summary of all model health checks.
pub fn health_check_summary() -> HealthCheckSummary {
    let cache = match health_cache().read() {
        Ok(cache) => cache,
        Err(_) => return HealthCheckSummary::default(),
    };

    let total = cache.len();
    let mut healthy = 0;
    let mut degraded = 0;
    let mut unavailable = 0;
    let mut unknown = 0;

    for check in cache.values() {
        if check.is_stale(HEALTH_CHECK_TTL) {
            unknown += 1;
            continue;
        }
        match check.status {
            ModelHealthStatus::Healthy => healthy += 1,
            ModelHealthStatus::Degraded { .. } => degraded += 1,
            ModelHealthStatus::Unavailable { .. } => unavailable += 1,
            ModelHealthStatus::Unknown => unknown += 1,
        }
    }

    HealthCheckSummary {
        total_models: total,
        healthy_models: healthy,
        degraded_models: degraded,
        unavailable_models: unavailable,
        unknown_models: unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_check_staleness() {
        let check = ModelHealthCheck {
            model: "gpt-5.4".to_string(),
            status: ModelHealthStatus::Healthy,
            checked_at: Instant::now(),
            latency_ms: Some(100),
        };
        assert!(!check.is_stale(HEALTH_CHECK_TTL));
    }

    #[test]
    fn health_check_stale() {
        let check = ModelHealthCheck {
            model: "gpt-5.4".to_string(),
            status: ModelHealthStatus::Healthy,
            checked_at: Instant::now() - Duration::from_secs(3600), // 1 hour ago
            latency_ms: Some(100),
        };
        assert!(check.is_stale(HEALTH_CHECK_TTL));
    }

    #[test]
    fn detect_capability_changes_basic() {
        let mut old = HashMap::new();
        old.insert("gpt-5.4".to_string(), 1_000_000);
        old.insert("claude-opus-5".to_string(), 1_000_000);

        let mut new = HashMap::new();
        new.insert("gpt-5.4".to_string(), 1_000_000); // unchanged
        new.insert("claude-opus-5".to_string(), 2_000_000); // changed
        new.insert("claude-opus-6".to_string(), 1_000_000); // new

        let diffs = detect_capability_changes(&old, &new);
        assert_eq!(diffs.len(), 2);

        let claude_diff = diffs.iter().find(|d| d.model == "claude-opus-5").unwrap();
        assert_eq!(claude_diff.old_value, Some(1_000_000));
        assert_eq!(claude_diff.new_value, Some(2_000_000));

        let new_diff = diffs.iter().find(|d| d.model == "claude-opus-6").unwrap();
        assert_eq!(new_diff.old_value, None);
        assert_eq!(new_diff.new_value, Some(1_000_000));
    }
}
