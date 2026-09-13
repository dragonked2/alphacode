//! Quality Gate — validates that a phase is fully complete before accepting it.
//!
//! A phase is only considered complete if all of the following pass:
//! implementation complete, tests pass, build passes, documentation
//! updated, no critical issues, review approved, and checkpoint created.
//! If any check fails, the phase remains open and follow-up tasks are
//! generated.
//!
//! The gate supports **adaptive thresholds** that relax documentation
//! requirements for long-running autonomous sessions (where exhaustive
//! docs for every micro-change would be wasteful) and add extra scrutiny
//! for regression-prone phases (multiple failed attempts, high churn).
//!
//! **Quality Scoring** — Beyond pass/fail, the gate produces a weighted
//! quality score (0.0–1.0) that reflects how strongly each check passed.
//!
//! **Regression Detection** — Tracks quality history and flags when a
//! phase scores significantly below recent averages.
//!
//! **Fix Suggestions** — Maps common failure patterns to actionable
//! remediation steps.
//!
//! **Trend Tracking** — Maintains a rolling window of quality scores to
//! surface improving/degrading trends.

use super::QualityGateResult;
use std::collections::VecDeque;

// ═══════════════════════════════════════════════════════════════════════════
// Task Complexity
// ═══════════════════════════════════════════════════════════════════════════

/// High-level complexity estimate for the task being evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskComplexity {
    /// Small, isolated change (< 50 LOC, single module).
    Low,
    /// Moderate change (50–300 LOC, touches 1–3 modules).
    Medium,
    /// Large cross-cutting change (> 300 LOC, multi-module).
    High,
}

// ═══════════════════════════════════════════════════════════════════════════
// Quality Score
// ═══════════════════════════════════════════════════════════════════════════

/// Weighted quality score for a single evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityScore {
    /// Overall score between 0.0 (all failed) and 1.0 (all passed perfectly).
    pub overall: f64,
    /// Individual check scores (same order as the 7 gate checks).
    pub implementation: f64,
    pub tests: f64,
    pub build: f64,
    pub documentation: f64,
    pub issues: f64,
    pub review: f64,
    pub checkpoint: f64,
}

impl Default for QualityScore {
    fn default() -> Self {
        Self {
            overall: 0.0,
            implementation: 0.0,
            tests: 0.0,
            build: 0.0,
            documentation: 0.0,
            issues: 0.0,
            review: 0.0,
            checkpoint: 0.0,
        }
    }
}

impl QualityScore {
    /// Create a score from boolean results and a complexity-adjusted config.
    fn compute(result: &QualityGateResult, config: &QualityGateConfig) -> Self {
        let weights = config.check_weights();

        let score = |passed: bool| if passed { 1.0 } else { 0.0 };

        let implementation = score(result.implementation_complete);
        let tests = score(result.tests_pass);
        let build = score(result.build_passes);
        let documentation = score(result.documentation_updated);
        let issues = score(result.no_critical_issues);
        let review = score(result.review_approved);
        let checkpoint = score(result.checkpoint_created);

        let overall = implementation * weights.0
            + tests * weights.1
            + build * weights.2
            + documentation * weights.3
            + issues * weights.4
            + review * weights.5
            + checkpoint * weights.6;

        Self {
            overall,
            implementation,
            tests,
            build,
            documentation,
            issues,
            review,
            checkpoint,
        }
    }

    /// Returns `true` if the score meets the configured pass threshold.
    pub fn meets_threshold(&self, threshold: f64) -> bool {
        self.overall >= threshold
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Trend
// ═══════════════════════════════════════════════════════════════════════════

/// Direction of quality trend over the rolling window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    /// Quality is consistently improving.
    Improving,
    /// Quality is roughly stable.
    Stable,
    /// Quality is degrading — may need intervention.
    Degrading,
}

/// Summary of the quality trend over the rolling window.
#[derive(Debug, Clone)]
pub struct TrendSummary {
    pub direction: TrendDirection,
    /// Mean score over the rolling window.
    pub mean: f64,
    /// Standard deviation of scores in the window.
    pub stddev: f64,
    /// Number of samples in the window.
    pub sample_count: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Regression Detection
// ═══════════════════════════════════════════════════════════════════════════

/// Result of regression analysis.
#[derive(Debug, Clone)]
pub struct RegressionReport {
    /// Whether a regression was detected.
    pub detected: bool,
    /// How far below the rolling mean the current score dropped.
    pub deviation: f64,
    /// Human-readable explanation.
    pub message: String,
}

impl RegressionReport {
    fn none() -> Self {
        Self {
            detected: false,
            deviation: 0.0,
            message: String::new(),
        }
    }

    fn detected(deviation: f64, current: f64, mean: f64) -> Self {
        Self {
            detected: true,
            deviation,
            message: format!(
                "Regression detected: current score {current:.3} is {deviation:.3} below \
                 rolling mean {mean:.3}"
            ),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Fix Suggestions
// ═══════════════════════════════════════════════════════════════════════════

/// A single actionable fix suggestion.
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    /// Which check failed.
    pub check: &'static str,
    /// Brief description of the failure.
    pub failure: String,
    /// Ordered list of steps to remediate.
    pub steps: Vec<String>,
}

/// Map common failure patterns to fix suggestions.
pub fn suggest_fixes(result: &QualityGateResult) -> Vec<FixSuggestion> {
    let mut suggestions = Vec::new();

    if !result.implementation_complete {
        suggestions.push(FixSuggestion {
            check: "implementation",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Implementation"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Review the task spec and identify incomplete work items".into(),
                "Break remaining work into smaller sub-tasks".into(),
                "Implement missing functionality and verify locally".into(),
            ],
        });
    }

    if !result.tests_pass {
        suggestions.push(FixSuggestion {
            check: "tests",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Tests"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Run test suite in isolation to reproduce failures".into(),
                "Fix root cause — check for missing mocks or stale fixtures".into(),
                "Re-run tests to confirm the fix".into(),
                "Add a regression test if the bug was previously untested".into(),
            ],
        });
    }

    if !result.build_passes {
        suggestions.push(FixSuggestion {
            check: "build",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Build"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Run `cargo check` (or equivalent) and read compiler errors".into(),
                "Fix type mismatches, missing imports, or feature-gate issues".into(),
                "Ensure all dependencies are in Cargo.toml".into(),
                "Verify the build passes with `cargo build`".into(),
            ],
        });
    }

    if !result.documentation_updated {
        suggestions.push(FixSuggestion {
            check: "documentation",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Documentation"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Add or update doc comments for changed public APIs".into(),
                "Update README / module-level docs if behavior changed".into(),
                "Ensure examples still compile and demonstrate usage".into(),
            ],
        });
    }

    if !result.no_critical_issues {
        suggestions.push(FixSuggestion {
            check: "issues",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Critical"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Review linting and static analysis output".into(),
                "Fix any `unsafe` misuse, unwrap in production, or resource leaks".into(),
                "Re-run linters to confirm all issues resolved".into(),
            ],
        });
    }

    if !result.review_approved {
        suggestions.push(FixSuggestion {
            check: "review",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Review"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Address reviewer feedback on open PR or self-review".into(),
                "Ensure code follows project style and conventions".into(),
                "Re-request review after changes".into(),
            ],
        });
    }

    if !result.checkpoint_created {
        suggestions.push(FixSuggestion {
            check: "checkpoint",
            failure: result
                .failed_checks
                .iter()
                .find(|c| c.starts_with("Checkpoint"))
                .cloned()
                .unwrap_or_default(),
            steps: vec![
                "Ensure workspace state is clean (no untracked files)".into(),
                "Create checkpoint commit with descriptive message".into(),
                "Verify checkpoint hash is recorded in session state".into(),
            ],
        });
    }

    suggestions
}

// ═══════════════════════════════════════════════════════════════════════════
// Adaptive Configuration
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for adaptive quality gate behavior.
#[derive(Debug, Clone)]
pub struct QualityGateConfig {
    /// Number of consecutive phase failures — higher values tighten the gate.
    pub consecutive_failures: u32,
    /// Total number of phases completed so far.
    pub completed_phases: u32,
    /// Whether this is a long-running session (> 1 hour).
    pub long_running: bool,
    /// Estimated remaining phases.
    pub estimated_remaining: u32,
    /// Complexity of the current task.
    pub task_complexity: TaskComplexity,
    /// Rolling window of recent quality scores (most recent last).
    pub score_history: VecDeque<f64>,
    /// Rolling window size for trend / regression analysis.
    pub window_size: usize,
    /// Absolute score drop below rolling mean that triggers regression.
    pub regression_threshold: f64,
    /// Minimum overall score to pass the gate (0.0–1.0).
    pub pass_threshold: f64,
}

impl Default for QualityGateConfig {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            completed_phases: 0,
            long_running: false,
            estimated_remaining: 0,
            task_complexity: TaskComplexity::Medium,
            score_history: VecDeque::new(),
            window_size: 10,
            regression_threshold: 0.15,
            pass_threshold: 0.7,
        }
    }
}

impl QualityGateConfig {
    /// Determine whether documentation check can be relaxed.
    ///
    /// For long-running autonomous sessions with many completed phases,
    /// requiring full documentation on every micro-change is wasteful.
    /// The gate relaxes docs for completed phases > 5 in long-running
    /// sessions, but never relaxes for the first 3 phases (establishing
    /// good habits).
    pub fn docs_can_be_relaxed(&self) -> bool {
        self.long_running && self.completed_phases > 5 && self.consecutive_failures < 3
    }

    /// Whether to apply extra scrutiny after consecutive failures.
    pub fn is_regression_prone(&self) -> bool {
        self.consecutive_failures >= 2
    }

    /// Whether consecutive failures warrant escalation.
    pub fn should_escalate_after_failures(&self) -> bool {
        self.consecutive_failures >= 3
    }

    /// Return a 7-tuple of weights adjusted for task complexity.
    ///
    /// Weights sum to 1.0.  Higher complexity tasks weight implementation
    /// and tests more heavily; simpler tasks spread weight more evenly.
    fn check_weights(&self) -> (f64, f64, f64, f64, f64, f64, f64) {
        match self.task_complexity {
            TaskComplexity::Low => (0.15, 0.15, 0.15, 0.15, 0.15, 0.15, 0.10),
            TaskComplexity::Medium => (0.18, 0.18, 0.14, 0.12, 0.14, 0.14, 0.10),
            TaskComplexity::High => (0.22, 0.22, 0.14, 0.08, 0.14, 0.12, 0.08),
        }
    }

    /// Adaptive pass threshold — higher complexity demands higher quality,
    /// and regression-prone sessions need tighter gates.
    pub fn effective_threshold(&self) -> f64 {
        let base = self.pass_threshold;

        let complexity_adjustment = match self.task_complexity {
            TaskComplexity::Low => -0.05,
            TaskComplexity::Medium => 0.0,
            TaskComplexity::High => 0.05,
        };

        let regression_adjustment = if self.is_regression_prone() {
            0.05
        } else if self.consecutive_failures >= 1 {
            0.02
        } else {
            0.0
        };

        (base + complexity_adjustment + regression_adjustment).clamp(0.0, 1.0)
    }

    /// Push a score into the rolling history and trim to window size.
    fn record_score(&mut self, score: f64) {
        self.score_history.push_back(score);
        while self.score_history.len() > self.window_size {
            self.score_history.pop_front();
        }
    }

    /// Compute mean and standard deviation of the rolling window.
    fn window_stats(&self) -> Option<(f64, f64)> {
        if self.score_history.is_empty() {
            return None;
        }
        let n = self.score_history.len() as f64;
        let mean = self.score_history.iter().sum::<f64>() / n;
        let variance = self
            .score_history
            .iter()
            .map(|s| (s - mean).powi(2))
            .sum::<f64>()
            / n;
        Some((mean, variance.sqrt()))
    }

    /// Analyze the trend over the rolling window.
    pub fn trend(&self) -> Option<TrendSummary> {
        let (mean, stddev) = self.window_stats()?;
        let samples = self.score_history.len();
        if samples < 3 {
            return Some(TrendSummary {
                direction: TrendDirection::Stable,
                mean,
                stddev,
                sample_count: samples,
            });
        }

        // Simple linear regression over the window indices.
        let n = samples as f64;
        let indices: Vec<f64> = (0..samples).map(|i| i as f64).collect();
        let scores: Vec<f64> = self.score_history.iter().copied().collect();

        let sum_x: f64 = indices.iter().sum();
        let sum_y: f64 = scores.iter().sum();
        let sum_xy: f64 = indices.iter().zip(scores.iter()).map(|(x, y)| x * y).sum();
        let sum_x2: f64 = indices.iter().map(|x| x * x).sum();

        let denominator = n * sum_x2 - sum_x * sum_x;
        if denominator.abs() < 1e-12 {
            return Some(TrendSummary {
                direction: TrendDirection::Stable,
                mean,
                stddev,
                sample_count: samples,
            });
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denominator;

        let direction = if slope > 0.01 {
            TrendDirection::Improving
        } else if slope < -0.01 {
            TrendDirection::Degrading
        } else {
            TrendDirection::Stable
        };

        Some(TrendSummary {
            direction,
            mean,
            stddev,
            sample_count: samples,
        })
    }

    /// Detect whether the given score is a regression relative to history.
    pub fn detect_regression(&self, score: f64) -> RegressionReport {
        let Some((mean, _)) = self.window_stats() else {
            return RegressionReport::none();
        };

        let deviation = mean - score;
        if deviation >= self.regression_threshold {
            RegressionReport::detected(deviation, score, mean)
        } else {
            RegressionReport::none()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Evaluation
// ═══════════════════════════════════════════════════════════════════════════

/// Run the quality gate check for a phase.
///
/// Each argument is a closure that returns `(passed, detail)`.
/// The result is a `QualityGateResult` with all checks filled in.
pub fn evaluate(
    implementation_check: impl Fn() -> (bool, String),
    tests_check: impl Fn() -> (bool, String),
    build_check: impl Fn() -> (bool, String),
    docs_check: impl Fn() -> (bool, String),
    issues_check: impl Fn() -> (bool, String),
    review_check: impl Fn() -> (bool, String),
    checkpoint_check: impl Fn() -> (bool, String),
) -> QualityGateResult {
    evaluate_with_config(
        &QualityGateConfig::default(),
        implementation_check,
        tests_check,
        build_check,
        docs_check,
        issues_check,
        review_check,
        checkpoint_check,
    )
}

/// Run the quality gate with adaptive configuration.
///
/// For long-running sessions, this relaxes documentation requirements
/// after the first few phases while adding extra scrutiny when
/// consecutive failures indicate regression.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_with_config(
    config: &QualityGateConfig,
    implementation_check: impl Fn() -> (bool, String),
    tests_check: impl Fn() -> (bool, String),
    build_check: impl Fn() -> (bool, String),
    docs_check: impl Fn() -> (bool, String),
    issues_check: impl Fn() -> (bool, String),
    review_check: impl Fn() -> (bool, String),
    checkpoint_check: impl Fn() -> (bool, String),
) -> QualityGateResult {
    let mut result = QualityGateResult::pending();

    let (impl_ok, impl_detail) = implementation_check();
    result.implementation_complete = impl_ok;
    if !impl_ok {
        result
            .failed_checks
            .push(format!("Implementation: {impl_detail}"));
    }

    let (tests_ok, tests_detail) = tests_check();
    result.tests_pass = tests_ok;
    if !tests_ok {
        result.failed_checks.push(format!("Tests: {tests_detail}"));
    }

    let (build_ok, build_detail) = build_check();
    result.build_passes = build_ok;
    if !build_ok {
        result.failed_checks.push(format!("Build: {build_detail}"));
    }

    let (docs_ok, docs_detail) = docs_check();
    // Adaptive: relax documentation for long-running sessions after initial phases.
    result.documentation_updated = docs_ok || config.docs_can_be_relaxed();
    if !docs_ok && !result.documentation_updated {
        result
            .failed_checks
            .push(format!("Documentation: {docs_detail}"));
    }

    let (issues_ok, issues_detail) = issues_check();
    result.no_critical_issues = issues_ok;
    if !issues_ok {
        result
            .failed_checks
            .push(format!("Critical issues: {issues_detail}"));
    }

    let (review_ok, review_detail) = review_check();
    result.review_approved = review_ok;
    if !review_ok {
        result
            .failed_checks
            .push(format!("Review: {review_detail}"));
    }

    let (cp_ok, cp_detail) = checkpoint_check();
    result.checkpoint_created = cp_ok;
    if !cp_ok {
        result
            .failed_checks
            .push(format!("Checkpoint: {cp_detail}"));
    }

    result.evaluated_at = chrono::Utc::now();
    result
}

/// Generate follow-up tasks for failed quality gate checks.
pub fn follow_up_tasks(result: &QualityGateResult) -> Vec<String> {
    if result.all_pass() {
        return Vec::new();
    }
    result.failed_checks.clone()
}

/// Determine whether a failed quality gate should trigger retry
/// vs escalation.  Regression-prone phases (multiple consecutive
/// failures) should escalate to human review rather than retry.
pub fn should_escalate(config: &QualityGateConfig, result: &QualityGateResult) -> bool {
    if result.all_pass() {
        return false;
    }
    config.is_regression_prone() && result.failed_checks.len() >= 2
}

/// Full evaluation result bundle — combines the boolean gate, the
/// quality score, regression report, trend summary, and fix suggestions
/// into a single struct for downstream consumers.
#[derive(Debug, Clone)]
pub struct QualityGateReport {
    /// The underlying boolean gate result.
    pub result: QualityGateResult,
    /// Weighted quality score.
    pub score: QualityScore,
    /// Regression analysis.
    pub regression: RegressionReport,
    /// Trend over the rolling window (if enough history exists).
    pub trend: Option<TrendSummary>,
    /// Actionable fix suggestions for each failing check.
    pub suggestions: Vec<FixSuggestion>,
}

/// Convenience: compute the full `QualityGateReport` in one call.
///
/// This evaluates the gate, scores it, records the score into the
/// config's history, checks for regression, computes trend, and
/// generates fix suggestions.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_full(
    config: &mut QualityGateConfig,
    implementation_check: impl Fn() -> (bool, String),
    tests_check: impl Fn() -> (bool, String),
    build_check: impl Fn() -> (bool, String),
    docs_check: impl Fn() -> (bool, String),
    issues_check: impl Fn() -> (bool, String),
    review_check: impl Fn() -> (bool, String),
    checkpoint_check: impl Fn() -> (bool, String),
) -> QualityGateReport {
    let result = evaluate_with_config(
        config,
        implementation_check,
        tests_check,
        build_check,
        docs_check,
        issues_check,
        review_check,
        checkpoint_check,
    );

    let score = QualityScore::compute(&result, config);
    let regression = config.detect_regression(score.overall);
    let trend = config.trend();

    config.record_score(score.overall);

    let suggestions = suggest_fixes(&result);

    QualityGateReport {
        result,
        score,
        regression,
        trend,
        suggestions,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────

    fn config_defaults() -> QualityGateConfig {
        QualityGateConfig::default()
    }

    fn all_pass_check() -> (bool, String) {
        (true, "ok".into())
    }

    // ── original gate tests ─────────────────────────────────────────────

    #[test]
    fn test_all_pass() {
        let result = evaluate(
            || (true, "done".into()),
            || (true, "pass".into()),
            || (true, "ok".into()),
            || (true, "updated".into()),
            || (true, "none".into()),
            || (true, "approved".into()),
            || (true, "created".into()),
        );
        assert!(result.all_pass());
        assert!(result.failed_checks.is_empty());
        assert!(follow_up_tasks(&result).is_empty());
    }

    #[test]
    fn test_some_fail() {
        let result = evaluate(
            || (true, "done".into()),
            || (false, "3 tests failed".into()),
            || (true, "ok".into()),
            || (false, "no docs written".into()),
            || (true, "none".into()),
            || (true, "approved".into()),
            || (true, "created".into()),
        );
        assert!(!result.all_pass());
        assert_eq!(result.failed_checks.len(), 2);
        let tasks = follow_up_tasks(&result);
        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].contains("Tests:"));
        assert!(tasks[1].contains("Documentation:"));
    }

    #[test]
    fn test_adaptive_docs_relaxation() {
        let config = QualityGateConfig {
            long_running: true,
            completed_phases: 10,
            consecutive_failures: 0,
            ..Default::default()
        };
        let result = evaluate_with_config(
            &config,
            || (true, "done".into()),
            || (true, "pass".into()),
            || (true, "ok".into()),
            || (false, "no docs".into()),
            || (true, "none".into()),
            || (true, "approved".into()),
            || (true, "created".into()),
        );
        assert!(result.documentation_updated);
        assert!(result.all_pass());
    }

    #[test]
    fn test_no_docs_relaxation_for_new_sessions() {
        let config = QualityGateConfig {
            long_running: true,
            completed_phases: 2,
            consecutive_failures: 0,
            ..Default::default()
        };
        let result = evaluate_with_config(
            &config,
            || (true, "done".into()),
            || (true, "pass".into()),
            || (true, "ok".into()),
            || (false, "no docs".into()),
            || (true, "none".into()),
            || (true, "approved".into()),
            || (true, "created".into()),
        );
        assert!(!result.documentation_updated);
        assert!(!result.all_pass());
    }

    #[test]
    fn test_escalation_after_consecutive_failures() {
        let config = QualityGateConfig {
            consecutive_failures: 3,
            completed_phases: 10,
            ..Default::default()
        };
        let result = QualityGateResult {
            implementation_complete: true,
            tests_pass: false,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: false,
            checkpoint_created: true,
            failed_checks: vec!["Tests: failed".into(), "Review: rejected".into()],
            evaluated_at: chrono::Utc::now(),
        };
        assert!(should_escalate(&config, &result));
    }

    #[test]
    fn test_no_escalation_for_single_failure() {
        let config = QualityGateConfig {
            consecutive_failures: 1,
            completed_phases: 5,
            ..Default::default()
        };
        let result = QualityGateResult {
            implementation_complete: true,
            tests_pass: false,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: true,
            checkpoint_created: true,
            failed_checks: vec!["Tests: failed".into()],
            evaluated_at: chrono::Utc::now(),
        };
        assert!(!should_escalate(&config, &result));
    }

    // ── quality score tests ─────────────────────────────────────────────

    #[test]
    fn test_perfect_score() {
        let config = config_defaults();
        let result = QualityGateResult {
            implementation_complete: true,
            tests_pass: true,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: true,
            checkpoint_created: true,
            failed_checks: vec![],
            evaluated_at: chrono::Utc::now(),
        };
        let score = QualityScore::compute(&result, &config);
        assert!((score.overall - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_zero_score() {
        let config = config_defaults();
        let result = QualityGateResult {
            implementation_complete: false,
            tests_pass: false,
            build_passes: false,
            documentation_updated: false,
            no_critical_issues: false,
            review_approved: false,
            checkpoint_created: false,
            failed_checks: vec![],
            evaluated_at: chrono::Utc::now(),
        };
        let score = QualityScore::compute(&result, &config);
        assert!((score.overall).abs() < 1e-6);
    }

    #[test]
    fn test_high_complexity_weights_implementation_and_tests() {
        let medium = QualityGateConfig {
            task_complexity: TaskComplexity::Medium,
            ..Default::default()
        };
        let high = QualityGateConfig {
            task_complexity: TaskComplexity::High,
            ..Default::default()
        };
        let medium_weights = medium.check_weights();
        let high_weights = high.check_weights();
        // High complexity weights implementation and tests more than medium
        assert!(high_weights.0 > medium_weights.0);
        assert!(high_weights.1 > medium_weights.1);
    }

    #[test]
    fn test_meets_threshold() {
        let score = QualityScore {
            overall: 0.85,
            ..Default::default()
        };
        assert!(score.meets_threshold(0.7));
        assert!(!score.meets_threshold(0.9));
    }

    // ── regression tests ────────────────────────────────────────────────

    #[test]
    fn test_regression_detected() {
        let mut config = QualityGateConfig {
            window_size: 5,
            regression_threshold: 0.15,
            ..Default::default()
        };
        // Simulate a good history
        for s in [0.9, 0.85, 0.92, 0.88, 0.91] {
            config.record_score(s);
        }
        let report = config.detect_regression(0.6);
        assert!(report.detected);
        assert!(report.deviation > 0.15);
    }

    #[test]
    fn test_no_regression_within_normal_range() {
        let mut config = QualityGateConfig {
            window_size: 5,
            regression_threshold: 0.15,
            ..Default::default()
        };
        for s in [0.9, 0.85, 0.92, 0.88, 0.91] {
            config.record_score(s);
        }
        let report = config.detect_regression(0.82);
        assert!(!report.detected);
    }

    #[test]
    fn test_regression_no_history_returns_none() {
        let config = config_defaults();
        let report = config.detect_regression(0.5);
        assert!(!report.detected);
    }

    // ── trend tests ─────────────────────────────────────────────────────

    #[test]
    fn test_trend_improving() {
        let mut config = QualityGateConfig {
            window_size: 6,
            ..Default::default()
        };
        for s in [0.5, 0.6, 0.65, 0.7, 0.8, 0.85] {
            config.record_score(s);
        }
        let trend = config.trend().unwrap();
        assert_eq!(trend.direction, TrendDirection::Improving);
    }

    #[test]
    fn test_trend_degrading() {
        let mut config = QualityGateConfig {
            window_size: 6,
            ..Default::default()
        };
        for s in [0.9, 0.85, 0.8, 0.7, 0.6, 0.5] {
            config.record_score(s);
        }
        let trend = config.trend().unwrap();
        assert_eq!(trend.direction, TrendDirection::Degrading);
    }

    #[test]
    fn test_trend_stable() {
        let mut config = QualityGateConfig {
            window_size: 6,
            ..Default::default()
        };
        for s in [0.8, 0.81, 0.79, 0.8, 0.82, 0.8] {
            config.record_score(s);
        }
        let trend = config.trend().unwrap();
        assert_eq!(trend.direction, TrendDirection::Stable);
    }

    #[test]
    fn test_trend_insufficient_data_returns_stable() {
        let mut config = QualityGateConfig {
            window_size: 6,
            ..Default::default()
        };
        config.record_score(0.8);
        config.record_score(0.85);
        let trend = config.trend().unwrap();
        assert_eq!(trend.direction, TrendDirection::Stable);
    }

    // ── fix suggestion tests ────────────────────────────────────────────

    #[test]
    fn test_suggestions_for_all_failures() {
        let result = QualityGateResult {
            implementation_complete: false,
            tests_pass: false,
            build_passes: false,
            documentation_updated: false,
            no_critical_issues: false,
            review_approved: false,
            checkpoint_created: false,
            failed_checks: vec![
                "Implementation: incomplete".into(),
                "Tests: 3 failed".into(),
                "Build: compile error".into(),
                "Documentation: missing".into(),
                "Critical issues: unwrap in prod".into(),
                "Review: rejected".into(),
                "Checkpoint: hash mismatch".into(),
            ],
            evaluated_at: chrono::Utc::now(),
        };
        let suggestions = suggest_fixes(&result);
        assert_eq!(suggestions.len(), 7);
        let checks: Vec<&str> = suggestions.iter().map(|s| s.check).collect();
        assert!(checks.contains(&"implementation"));
        assert!(checks.contains(&"tests"));
        assert!(checks.contains(&"build"));
        assert!(checks.contains(&"documentation"));
        assert!(checks.contains(&"issues"));
        assert!(checks.contains(&"review"));
        assert!(checks.contains(&"checkpoint"));
    }

    #[test]
    fn test_no_suggestions_when_all_pass() {
        let result = QualityGateResult {
            implementation_complete: true,
            tests_pass: true,
            build_passes: true,
            documentation_updated: true,
            no_critical_issues: true,
            review_approved: true,
            checkpoint_created: true,
            failed_checks: vec![],
            evaluated_at: chrono::Utc::now(),
        };
        let suggestions = suggest_fixes(&result);
        assert!(suggestions.is_empty());
    }

    // ── adaptive threshold tests ────────────────────────────────────────

    #[test]
    fn test_effective_threshold_increases_with_complexity() {
        let low = QualityGateConfig {
            task_complexity: TaskComplexity::Low,
            ..Default::default()
        };
        let medium = QualityGateConfig {
            task_complexity: TaskComplexity::Medium,
            ..Default::default()
        };
        let high = QualityGateConfig {
            task_complexity: TaskComplexity::High,
            ..Default::default()
        };
        assert!(low.effective_threshold() < medium.effective_threshold());
        assert!(medium.effective_threshold() < high.effective_threshold());
    }

    #[test]
    fn test_effective_threshold_increases_with_failures() {
        let clean = QualityGateConfig {
            consecutive_failures: 0,
            ..Default::default()
        };
        let failing = QualityGateConfig {
            consecutive_failures: 3,
            ..Default::default()
        };
        assert!(clean.effective_threshold() < failing.effective_threshold());
    }

    // ── full evaluation test ────────────────────────────────────────────

    #[test]
    fn test_evaluate_full_produces_complete_report() {
        let mut config = QualityGateConfig {
            window_size: 5,
            ..Default::default()
        };
        // Seed history so regression/trend can work
        for s in [0.85, 0.9, 0.88, 0.92, 0.87] {
            config.record_score(s);
        }

        let report = evaluate_full(
            &mut config,
            all_pass_check,
            all_pass_check,
            all_pass_check,
            all_pass_check,
            all_pass_check,
            all_pass_check,
            all_pass_check,
        );

        assert!(report.result.all_pass());
        assert!((report.score.overall - 1.0).abs() < 1e-6);
        assert!(!report.regression.detected);
        assert!(report.suggestions.is_empty());
        // History should now contain 6 entries (5 seeded + 1 just computed)
        assert_eq!(config.score_history.len(), 6);
    }

    #[test]
    fn test_evaluate_full_records_regression() {
        let mut config = QualityGateConfig {
            window_size: 5,
            regression_threshold: 0.1,
            ..Default::default()
        };
        for s in [0.9, 0.92, 0.88, 0.91, 0.9] {
            config.record_score(s);
        }

        let report = evaluate_full(
            &mut config,
            all_pass_check,
            || (false, "unit tests failing".into()),
            all_pass_check,
            all_pass_check,
            all_pass_check,
            all_pass_check,
            all_pass_check,
        );

        // Score should be below 1.0 due to failing tests
        assert!(!report.result.all_pass());
        // Regression: score ~0.857 vs mean ~0.902 → deviation ~0.045
        // which is below the 0.1 threshold, so no regression
        // With a lower threshold we can trigger it
        assert!(!report.regression.detected);
        assert_eq!(report.suggestions.len(), 1);
        assert_eq!(report.suggestions[0].check, "tests");
    }
}
