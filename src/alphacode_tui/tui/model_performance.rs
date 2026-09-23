use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Model performance cache with TTL-based invalidation
pub struct ModelPerformanceCache {
    /// Cached model data
    cache: Arc<Mutex<HashMap<String, CachedModelData>>>,
    /// Cache TTL (time to live) in seconds
    ttl: Duration,
    /// Maximum cache size
    max_size: usize,
}

#[derive(Clone, Debug)]
struct CachedModelData {
    /// Cached model routes
    routes: Vec<CachedModelRoute>,
    /// When this data was cached
    cached_at: Instant,
}

#[derive(Clone, Debug)]
pub struct CachedModelRoute {
    pub model: String,
    pub provider: String,
    pub api_method: String,
    pub available: bool,
    pub detail: String,
    pub context_window: Option<u64>,
    pub supports_tools: bool,
    pub last_used: Option<u64>,
    pub usage_count: u32,
}

impl ModelPerformanceCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(300), // 5 minutes default
            max_size: 1000,
        }
    }

    /// Create cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        let mut cache = Self::new();
        cache.ttl = ttl;
        cache
    }

    /// Get cached model routes if valid
    pub fn get(&self, key: &str) -> Option<Vec<CachedModelRoute>> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(key)?;

        // Check if cache entry is still valid
        if entry.cached_at.elapsed() < self.ttl {
            Some(entry.routes.clone())
        } else {
            None // Cache expired
        }
    }

    /// Store model routes in cache
    pub fn set(&self, key: &str, routes: Vec<CachedModelRoute>) {
        let mut cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => return,
        };

        // Evict old entries if at capacity
        if cache.len() >= self.max_size {
            self.evict_oldest(&mut cache);
        }

        cache.insert(
            key.to_string(),
            CachedModelData {
                routes,
                cached_at: Instant::now(),
            },
        );
    }

    /// Invalidate cache entry
    pub fn invalidate(&self, key: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(key);
        }
    }

    /// Invalidate all cache entries
    pub fn invalidate_all(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(_) => return CacheStats::default(),
        };

        let mut valid_entries = 0;
        let mut expired_entries = 0;

        for entry in cache.values() {
            if entry.cached_at.elapsed() < self.ttl {
                valid_entries += 1;
            } else {
                expired_entries += 1;
            }
        }

        CacheStats {
            total_entries: cache.len(),
            valid_entries,
            expired_entries,
            max_size: self.max_size,
        }
    }

    /// Evict oldest cache entries
    fn evict_oldest(&self, cache: &mut HashMap<String, CachedModelData>) {
        let now = Instant::now();
        let mut entries: Vec<(String, Duration)> = cache
            .iter()
            .map(|(key, data)| (key.clone(), now.duration_since(data.cached_at)))
            .collect();

        // Sort by age (oldest first)
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove oldest 10% of entries
        let to_remove = (entries.len() / 10).max(1);
        for (key, _) in entries.into_iter().take(to_remove) {
            cache.remove(&key);
        }
    }

    /// Update cache TTL
    pub fn set_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }

    /// Update max cache size
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
    }
}

impl Default for ModelPerformanceCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub max_size: usize,
}

/// Lazy model loader for deferred initialization
pub struct LazyModelLoader {
    /// Models to load lazily
    pending_models: Arc<Mutex<Vec<String>>>,
    /// Already loaded models
    loaded_models: Arc<Mutex<HashMap<String, bool>>>,
}

impl LazyModelLoader {
    pub fn new() -> Self {
        Self {
            pending_models: Arc::new(Mutex::new(Vec::new())),
            loaded_models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Schedule model for lazy loading
    pub fn schedule_load(&self, model: &str) {
        let mut pending = match self.pending_models.lock() {
            Ok(pending) => pending,
            Err(_) => return,
        };

        // Don't schedule if already loaded or pending
        if self.is_loaded(model) || pending.contains(&model.to_string()) {
            return;
        }

        pending.push(model.to_string());
    }

    /// Check if model is loaded
    pub fn is_loaded(&self, model: &str) -> bool {
        self.loaded_models
            .lock()
            .ok()
            .and_then(|loaded| loaded.get(model).copied())
            .unwrap_or(false)
    }

    /// Mark model as loaded
    pub fn mark_loaded(&self, model: &str) {
        if let Ok(mut loaded) = self.loaded_models.lock() {
            loaded.insert(model.to_string(), true);
        }
    }

    /// Get pending models count
    pub fn pending_count(&self) -> usize {
        self.pending_models.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Get loaded models count
    pub fn loaded_count(&self) -> usize {
        self.loaded_models.lock().map(|l| l.len()).unwrap_or(0)
    }

    /// Clear all pending and loaded models
    pub fn clear(&self) {
        if let Ok(mut pending) = self.pending_models.lock() {
            pending.clear();
        }
        if let Ok(mut loaded) = self.loaded_models.lock() {
            loaded.clear();
        }
    }
}

impl Default for LazyModelLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance metrics for model operations
pub struct ModelPerformanceMetrics {
    /// Track model load times
    load_times: Arc<Mutex<HashMap<String, Duration>>>,
    /// Track model switch times
    switch_times: Arc<Mutex<Vec<Duration>>>,
    /// Track cache hit/miss rates
    cache_hits: Arc<Mutex<u64>>,
    cache_misses: Arc<Mutex<u64>>,
}

impl ModelPerformanceMetrics {
    pub fn new() -> Self {
        Self {
            load_times: Arc::new(Mutex::new(HashMap::new())),
            switch_times: Arc::new(Mutex::new(Vec::new())),
            cache_hits: Arc::new(Mutex::new(0)),
            cache_misses: Arc::new(Mutex::new(0)),
        }
    }

    /// Record model load time
    pub fn record_load_time(&self, model: &str, duration: Duration) {
        if let Ok(mut times) = self.load_times.lock() {
            times.insert(model.to_string(), duration);
        }
    }

    /// Record model switch time
    pub fn record_switch_time(&self, duration: Duration) {
        if let Ok(mut times) = self.switch_times.lock() {
            times.push(duration);
            // Keep only last 100 measurements
            if times.len() > 100 {
                times.remove(0);
            }
        }
    }

    /// Record cache hit
    pub fn record_cache_hit(&self) {
        if let Ok(mut hits) = self.cache_hits.lock() {
            *hits += 1;
        }
    }

    /// Record cache miss
    pub fn record_cache_miss(&self) {
        if let Ok(mut misses) = self.cache_misses.lock() {
            *misses += 1;
        }
    }

    /// Get average model load time
    pub fn average_load_time(&self) -> Duration {
        let times = match self.load_times.lock() {
            Ok(times) => times,
            Err(_) => return Duration::ZERO,
        };

        if times.is_empty() {
            return Duration::ZERO;
        }

        let total: Duration = times.values().sum();
        total / times.len() as u32
    }

    /// Get average model switch time
    pub fn average_switch_time(&self) -> Duration {
        let times = match self.switch_times.lock() {
            Ok(times) => times,
            Err(_) => return Duration::ZERO,
        };

        if times.is_empty() {
            return Duration::ZERO;
        }

        let total: Duration = times.iter().sum();
        total / times.len() as u32
    }

    /// Get cache hit rate (0.0 to 1.0)
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.lock().map(|h| *h).unwrap_or(0);
        let misses = self.cache_misses.lock().map(|m| *m).unwrap_or(0);
        let total = hits + misses;
        if total == 0 {
            return 0.0;
        }
        hits as f64 / total as f64
    }

    /// Get performance summary
    pub fn summary(&self) -> PerformanceSummary {
        PerformanceSummary {
            average_load_time: self.average_load_time(),
            average_switch_time: self.average_switch_time(),
            cache_hit_rate: self.cache_hit_rate(),
            total_loads: self.load_times.lock().map(|t| t.len()).unwrap_or(0),
            total_switches: self.switch_times.lock().map(|t| t.len()).unwrap_or(0),
        }
    }
}

impl Default for ModelPerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct PerformanceSummary {
    pub average_load_time: Duration,
    pub average_switch_time: Duration,
    pub cache_hit_rate: f64,
    pub total_loads: usize,
    pub total_switches: usize,
}

impl std::fmt::Display for PerformanceSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Model Performance: avg_load={:.1}ms, avg_switch={:.1}ms, cache_hit_rate={:.1}%, loads={}, switches={}",
            self.average_load_time.as_secs_f64() * 1000.0,
            self.average_switch_time.as_secs_f64() * 1000.0,
            self.cache_hit_rate * 100.0,
            self.total_loads,
            self.total_switches,
        )
    }
}

/// Global performance cache instance
static GLOBAL_PERFORMANCE_CACHE: std::sync::OnceLock<ModelPerformanceCache> =
    std::sync::OnceLock::new();

/// Get global performance cache
pub fn global_performance_cache() -> &'static ModelPerformanceCache {
    GLOBAL_PERFORMANCE_CACHE.get_or_init(ModelPerformanceCache::new)
}

/// Global performance metrics instance
static GLOBAL_PERFORMANCE_METRICS: std::sync::OnceLock<ModelPerformanceMetrics> =
    std::sync::OnceLock::new();

/// Get global performance metrics
pub fn global_performance_metrics() -> &'static ModelPerformanceMetrics {
    GLOBAL_PERFORMANCE_METRICS.get_or_init(ModelPerformanceMetrics::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_cache_creation() {
        let cache = ModelPerformanceCache::new();
        let stats = cache.stats();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.valid_entries, 0);
    }

    #[test]
    fn test_cache_set_and_get() {
        let cache = ModelPerformanceCache::new();
        let routes = vec![CachedModelRoute {
            model: "test-model".to_string(),
            provider: "test".to_string(),
            api_method: "test".to_string(),
            available: true,
            detail: String::new(),
            context_window: Some(1000000),
            supports_tools: true,
            last_used: None,
            usage_count: 0,
        }];

        cache.set("test-key", routes.clone());
        let cached = cache.get("test-key");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = ModelPerformanceCache::new();
        cache.set("test-key", vec![]);
        assert!(cache.get("test-key").is_some());

        cache.invalidate("test-key");
        assert!(cache.get("test-key").is_none());
    }

    #[test]
    fn test_performance_metrics() {
        let metrics = ModelPerformanceMetrics::new();
        metrics.record_load_time("model1", Duration::from_millis(100));
        metrics.record_load_time("model2", Duration::from_millis(200));

        let avg = metrics.average_load_time();
        assert_eq!(avg, Duration::from_millis(150));
    }

    #[test]
    fn test_cache_hit_rate() {
        let metrics = ModelPerformanceMetrics::new();
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let rate = metrics.cache_hit_rate();
        assert!((rate - 0.666).abs() < 0.01);
    }
}
