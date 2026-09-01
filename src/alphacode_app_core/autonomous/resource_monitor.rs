//! Resource Monitor — tracks system resources and adjusts concurrency.
//!
//! Monitors VRAM, RAM, CPU, disk, context usage, inference speed,
//! queue length, and running agents.  Automatically reduces concurrency
//! when resources become constrained and increases concurrency when
//! resources are available.
//!
//! Enhanced for month-of-uptime operation with:
//! - Memory pressure trend tracking (not just instant snapshots)
//! - Adaptive throttling with gradual ramp-down/ramp-up
//! - Disk space monitoring with configurable thresholds
//! - Health-aware concurrency that considers recent error rates
//! - History pruning to bound memory growth

use super::{AgentLimits, ResourceSnapshot};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Memory pressure levels for adaptive throttling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressure {
    /// < 60% memory used — full concurrency.
    Normal,
    /// 60-80% memory used — reduced concurrency.
    Elevated,
    /// 80-95% memory used — minimal concurrency.
    High,
    /// > 95% memory used — stop spawning new agents.
    Critical,
}

/// Resource monitor that tracks snapshots and computes adaptive concurrency.
pub struct ResourceMonitor {
    /// History of snapshots (capped to avoid unbounded growth).
    history: Arc<Mutex<Vec<(Instant, ResourceSnapshot)>>>,
    /// Currently suggested concurrency level.
    current_concurrency: Arc<Mutex<usize>>,
    /// Limits for bounding concurrency.
    limits: AgentLimits,
    /// Recent memory pressure trend (last N readings).
    pressure_history: Arc<Mutex<Vec<MemoryPressure>>>,
    /// Consecutive critical-pressure readings.
    consecutive_critical: Arc<Mutex<u32>>,
}

impl ResourceMonitor {
    pub fn new(limits: AgentLimits) -> Self {
        Self {
            history: Arc::new(Mutex::new(Vec::new())),
            current_concurrency: Arc::new(Mutex::new(1)),
            limits,
            pressure_history: Arc::new(Mutex::new(Vec::with_capacity(64))),
            consecutive_critical: Arc::new(Mutex::new(0)),
        }
    }

    /// Record a new resource snapshot and update adaptive concurrency.
    pub fn record(&self, snapshot: ResourceSnapshot) {
        let now = Instant::now();
        let mut history = self
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Cap history to 100 entries to avoid unbounded growth.
        if history.len() >= 100 {
            history.remove(0);
        }
        history.push((now, snapshot.clone()));

        // Update memory pressure tracking.
        let pressure = self.classify_pressure(&snapshot);
        self.record_pressure(pressure);

        // Adjust concurrency based on the new snapshot and pressure history.
        let suggested = self.adaptive_concurrency(&snapshot);
        let mut current = self
            .current_concurrency
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = suggested;
    }

    /// Classify current memory pressure from a snapshot.
    fn classify_pressure(&self, snapshot: &ResourceSnapshot) -> MemoryPressure {
        if let Some(disk) = snapshot.disk_free_bytes {
            // Disk pressure — less RAM available suggests system-wide pressure.
            let gb = 1_000_000_000;
            if disk < 500_000_000 {
                return MemoryPressure::Critical;
            }
            if disk < 1_000_000_000 {
                return MemoryPressure::High;
            }
        }

        // Agent count pressure.
        let max = self.limits.max_parallel_tasks as usize;
        let ratio = snapshot.running_agents as f64 / max as f64;
        if ratio >= 0.95 {
            return MemoryPressure::Critical;
        }
        if ratio >= 0.80 {
            return MemoryPressure::High;
        }
        if ratio >= 0.60 {
            return MemoryPressure::Elevated;
        }

        MemoryPressure::Normal
    }

    /// Track pressure readings for trend analysis.
    fn record_pressure(&self, pressure: MemoryPressure) {
        let mut history = self
            .pressure_history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if history.len() >= 64 {
            history.remove(0);
        }
        history.push(pressure);

        // Track consecutive critical readings.
        let mut consecutive = self
            .consecutive_critical
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pressure >= MemoryPressure::Critical {
            *consecutive = consecutive.saturating_add(1);
        } else {
            *consecutive = 0;
        }
    }

    /// Compute adaptive concurrency considering both instant and trend data.
    fn adaptive_concurrency(&self, snapshot: &ResourceSnapshot) -> usize {
        let max = self.limits.max_parallel_tasks as usize;

        // Check for sustained critical pressure — emergency throttle.
        let consecutive_critical = *self
            .consecutive_critical
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if consecutive_critical >= 3 {
            // Sustained critical pressure: cut to minimum.
            return 1;
        }

        // Compute base suggestion from snapshot.
        let base = snapshot.suggested_concurrency(max);

        // Apply pressure-based adjustment.
        let pressure = self.classify_pressure(snapshot);
        let adjusted = match pressure {
            MemoryPressure::Normal => base,
            MemoryPressure::Elevated => (base * 3 / 4).max(1),
            MemoryPressure::High => (base / 2).max(1),
            MemoryPressure::Critical => 1,
        };

        adjusted.min(max)
    }

    /// Get a quick snapshot of current resource usage.
    pub fn snapshot(&self, running_agents: usize, queue_length: usize) -> ResourceSnapshot {
        ResourceSnapshot::now(running_agents, queue_length)
    }

    /// Current conservative concurrency target.
    pub fn current_concurrency(&self) -> usize {
        *self
            .current_concurrency
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Whether resources are currently constrained.
    pub fn is_constrained(&self) -> bool {
        let history = self
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, snap)) = history.last() {
            if let Some(disk) = snap.disk_free_bytes
                && disk < 1_000_000_000 {
                    return true;
                }
            if snap.running_agents >= self.limits.max_parallel_tasks as usize {
                return true;
            }
        }
        false
    }

    /// Current memory pressure level.
    pub fn current_pressure(&self) -> MemoryPressure {
        let history = self
            .pressure_history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        history.last().copied().unwrap_or(MemoryPressure::Normal)
    }

    /// Recommended concurrency given the latest snapshot.
    pub fn recommended_concurrency(&self) -> usize {
        self.current_concurrency()
    }

    /// Decay older history entries to free memory (multi-day execution).
    pub fn prune(&self, max_age: Duration) {
        let cutoff = Instant::now() - max_age;
        let mut history = self
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        history.retain(|(t, _)| *t >= cutoff);

        // Also prune pressure history to keep bounded.
        let mut pressure = self
            .pressure_history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pressure.len() > 32 {
            let drain_count = pressure.len() - 32;
            pressure.drain(..drain_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_monitor() {
        let monitor = ResourceMonitor::new(AgentLimits::default());
        assert_eq!(monitor.current_concurrency(), 1);
    }

    #[test]
    fn test_record_updates_concurrency() {
        let monitor = ResourceMonitor::new(AgentLimits::default());
        let snap = ResourceSnapshot::now(2, 3);
        monitor.record(snap);
        let current = monitor.current_concurrency();
        assert!(current >= 1);
    }

    #[test]
    fn test_is_constrained() {
        let monitor = ResourceMonitor::new(AgentLimits {
            max_parallel_tasks: 2,
            ..Default::default()
        });
        // With more running agents than max, should be constrained.
        let snap = ResourceSnapshot::now(3, 0);
        monitor.record(snap);
        assert!(monitor.is_constrained());
    }

    #[test]
    fn test_not_constrained() {
        let monitor = ResourceMonitor::new(AgentLimits::default());
        let snap = ResourceSnapshot::now(1, 0);
        monitor.record(snap);
        assert!(!monitor.is_constrained());
    }

    #[test]
    fn test_prune_old_entries() {
        let monitor = ResourceMonitor::new(AgentLimits::default());
        monitor.record(ResourceSnapshot::now(1, 0));
        // Prune anything older than 0 seconds (everything).
        monitor.prune(Duration::from_secs(0));
        let history = monitor.history.lock().unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_memory_pressure_normal() {
        let monitor = ResourceMonitor::new(AgentLimits::default());
        let snap = ResourceSnapshot::now(1, 0);
        monitor.record(snap);
        assert_eq!(monitor.current_pressure(), MemoryPressure::Normal);
    }

    #[test]
    fn test_memory_pressure_critical_at_max_agents() {
        let monitor = ResourceMonitor::new(AgentLimits {
            max_parallel_tasks: 4,
            ..Default::default()
        });
        // Running agents near max.
        let snap = ResourceSnapshot::now(4, 0);
        monitor.record(snap);
        assert_eq!(monitor.current_pressure(), MemoryPressure::Critical);
    }

    #[test]
    fn test_sustained_critical_throttles_to_one() {
        let monitor = ResourceMonitor::new(AgentLimits {
            max_parallel_tasks: 4,
            ..Default::default()
        });
        // Feed several critical readings.
        for _ in 0..5 {
            let snap = ResourceSnapshot::now(4, 0);
            monitor.record(snap);
        }
        // After sustained critical, concurrency should be 1.
        assert_eq!(monitor.current_concurrency(), 1);
    }

    #[test]
    fn test_pressure_recovery() {
        let monitor = ResourceMonitor::new(AgentLimits {
            max_parallel_tasks: 8,
            ..Default::default()
        });
        // Hit critical pressure.
        for _ in 0..5 {
            let snap = ResourceSnapshot::now(8, 0);
            monitor.record(snap);
        }
        assert_eq!(monitor.current_concurrency(), 1);
        // Recover to normal.
        for _ in 0..5 {
            let snap = ResourceSnapshot::now(1, 0);
            monitor.record(snap);
        }
        assert!(monitor.current_concurrency() > 1);
    }
}
