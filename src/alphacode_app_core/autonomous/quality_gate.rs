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

use super::QualityGateResult;

/// Configuration for adaptive quality gate behavior.
#[derive(Debug, Clone)]
pub struct QualityGateConfig {
    /// Number of consecutive phase failures — higher values tighten the gate.
    pub consecutive_failures: u32,
    /// Total number of phases completed so far.
    pub completed_phases: u32,
    /// Whether this is a long-running session (>1 hour).
    pub long_running: bool,
    /// Estimated remaining phases.
    pub estimated_remaining: u32,
}

impl Default for QualityGateConfig {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            completed_phases: 0,
            long_running: false,
            estimated_remaining: 0,
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
}

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
        result.failed_checks.push(format!("Implementation: {impl_detail}"));
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
        result.failed_checks.push(format!("Documentation: {docs_detail}"));
    }

    let (issues_ok, issues_detail) = issues_check();
    result.no_critical_issues = issues_ok;
    if !issues_ok {
        result.failed_checks.push(format!("Critical issues: {issues_detail}"));
    }

    let (review_ok, review_detail) = review_check();
    result.review_approved = review_ok;
    if !review_ok {
        result.failed_checks.push(format!("Review: {review_detail}"));
    }

    let (cp_ok, cp_detail) = checkpoint_check();
    result.checkpoint_created = cp_ok;
    if !cp_ok {
        result.failed_checks.push(format!("Checkpoint: {cp_detail}"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // Docs check failed, but adaptive relaxation should pass it.
        assert!(result.documentation_updated);
        assert!(result.all_pass());
    }

    #[test]
    fn test_no_docs_relaxation_for_new_sessions() {
        let config = QualityGateConfig {
            long_running: true,
            completed_phases: 2, // Too few phases
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
}
