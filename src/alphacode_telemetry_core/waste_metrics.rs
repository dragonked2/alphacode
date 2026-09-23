//! Waste metrics per goal contract.
//!
//! Tracks redundant calls, post-goal calls, phase overruns, and other waste
//! signals that describe how efficiently a task was executed. These metrics
//! feed into the TUI run summary and the benchmark suite.

use std::sync::Mutex;

/// Per-contract waste metrics, accumulated during a turn and flushed at the end.
#[derive(Debug, Clone, Default)]
pub struct WasteMetrics {
    /// Identical (tool, params) calls rejected by the dedup guard.
    pub redundant_calls: u32,
    /// Calls made after the highest-tier evidence arrived (the key waste signal).
    pub post_goal_calls: u32,
    /// Actual phase duration / budgeted phase duration.
    pub phase_overrun_ratio: f64,
    /// Diagnostic tools called after goal was already satisfied.
    pub diagnostics_after_success: u32,
    /// Auxiliary tool retries after goal was achieved.
    pub auxiliary_retries_post_goal: u32,
    /// Total tool calls in this contract.
    pub total_calls: u32,
    /// Call number when the goal was first achieved (if any).
    pub goal_achieved_at_call: Option<u32>,
    /// Wall-clock time from first call to goal achievement.
    pub time_to_goal: Option<std::time::Duration>,
    /// Wall-clock time from goal achievement to final report.
    pub time_post_goal: Option<std::time::Duration>,
}

impl WasteMetrics {
    /// The waste ratio: post_goal_calls / total_calls. 0.0 = perfect, 1.0 = all waste.
    pub fn waste_ratio(&self) -> f64 {
        if self.total_calls == 0 {
            return 0.0;
        }
        self.post_goal_calls as f64 / self.total_calls as f64
    }

    /// One-line summary for the TUI run summary.
    pub fn summary_line(&self) -> String {
        match self.goal_achieved_at_call {
            Some(achieved) => {
                format!(
                    "goal in {} calls; {} post-goal waste (ratio: {:.0}%), {} total",
                    achieved,
                    self.post_goal_calls,
                    self.waste_ratio() * 100.0,
                    self.total_calls
                )
            }
            None => {
                format!(
                    "{} calls total (no terminal evidence detected)",
                    self.total_calls
                )
            }
        }
    }

    /// Detailed multi-line summary for debugging.
    pub fn detailed_summary(&self) -> String {
        let mut lines = vec![
            format!("Total calls: {}", self.total_calls),
            format!("Redundant calls: {}", self.redundant_calls),
            format!("Post-goal calls: {}", self.post_goal_calls),
            format!("Waste ratio: {:.1}%", self.waste_ratio() * 100.0),
        ];

        if let Some(achieved) = self.goal_achieved_at_call {
            lines.push(format!("Goal achieved at call: #{}", achieved));
        }
        if let Some(duration) = self.time_to_goal {
            lines.push(format!("Time to goal: {:.1}s", duration.as_secs_f64()));
        }
        if let Some(duration) = self.time_post_goal {
            lines.push(format!("Time post-goal: {:.1}s", duration.as_secs_f64()));
        }
        if self.diagnostics_after_success > 0 {
            lines.push(format!(
                "Diagnostics after success: {}",
                self.diagnostics_after_success
            ));
        }
        if self.auxiliary_retries_post_goal > 0 {
            lines.push(format!(
                "Auxiliary retries post-goal: {}",
                self.auxiliary_retries_post_goal
            ));
        }
        if self.phase_overrun_ratio > 1.0 {
            lines.push(format!("Phase overrun: {:.1}x", self.phase_overrun_ratio));
        }

        lines.join("\n")
    }
}

/// Process-global waste metrics for the current session.
static SESSION_WASTE: Mutex<Option<WasteMetrics>> = Mutex::new(None);

/// Initialize waste metrics for a new contract.
pub fn begin_contract() {
    if let Ok(mut guard) = SESSION_WASTE.lock() {
        *guard = Some(WasteMetrics::default());
    }
}

/// Record a redundant call.
pub fn record_redundant_call() {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
    {
        metrics.redundant_calls += 1;
        metrics.total_calls += 1;
    }
}

/// Record a post-goal call.
pub fn record_post_goal_call() {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
    {
        metrics.post_goal_calls += 1;
        metrics.total_calls += 1;
    }
}

/// Record a diagnostic after success.
pub fn record_diagnostic_after_success() {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
    {
        metrics.diagnostics_after_success += 1;
    }
}

/// Record an auxiliary retry post-goal.
pub fn record_auxiliary_retry_post_goal() {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
    {
        metrics.auxiliary_retries_post_goal += 1;
    }
}

/// Record a tool call.
pub fn record_tool_call() {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
    {
        metrics.total_calls += 1;
    }
}

/// Mark the goal as achieved.
pub fn mark_goal_achieved(call_number: u32) {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
        && metrics.goal_achieved_at_call.is_none()
    {
        metrics.goal_achieved_at_call = Some(call_number);
        metrics.time_to_goal = Some(std::time::Instant::now().elapsed());
    }
}

/// Set the phase overrun ratio.
pub fn set_phase_overrun(ratio: f64) {
    if let Ok(mut guard) = SESSION_WASTE.lock()
        && let Some(metrics) = guard.as_mut()
    {
        metrics.phase_overrun_ratio = ratio;
    }
}

/// Get the current waste metrics (snapshot).
pub fn snapshot() -> Option<WasteMetrics> {
    SESSION_WASTE.lock().ok()?.clone()
}

/// Finalize and return the waste metrics, clearing the session state.
pub fn finalize() -> Option<WasteMetrics> {
    SESSION_WASTE.lock().ok()?.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waste_ratio_calculation() {
        let metrics = WasteMetrics {
            total_calls: 10,
            post_goal_calls: 3,
            ..Default::default()
        };
        assert!((metrics.waste_ratio() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn waste_ratio_zero_total() {
        let metrics = WasteMetrics::default();
        assert_eq!(metrics.waste_ratio(), 0.0);
    }

    #[test]
    fn summary_line_with_goal() {
        let metrics = WasteMetrics {
            total_calls: 7,
            post_goal_calls: 2,
            goal_achieved_at_call: Some(5),
            ..Default::default()
        };
        let summary = metrics.summary_line();
        assert!(summary.contains("goal in 5 calls"));
        assert!(summary.contains("2 post-goal waste"));
    }

    #[test]
    fn summary_line_without_goal() {
        let metrics = WasteMetrics {
            total_calls: 7,
            ..Default::default()
        };
        let summary = metrics.summary_line();
        assert!(summary.contains("7 calls total"));
        assert!(summary.contains("no terminal evidence"));
    }

    #[test]
    fn session_waste_lifecycle() {
        begin_contract();
        record_tool_call();
        record_tool_call();
        mark_goal_achieved(2);
        record_post_goal_call();

        let metrics = snapshot().unwrap();
        assert_eq!(metrics.total_calls, 3);
        assert_eq!(metrics.post_goal_calls, 1);
        assert_eq!(metrics.goal_achieved_at_call, Some(2));

        let finalized = finalize();
        assert!(finalized.is_some());
        assert!(snapshot().is_none());
    }
}
