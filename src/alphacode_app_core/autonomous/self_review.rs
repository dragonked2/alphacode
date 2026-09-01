//! Self-Review — independent review pass after each major task.
//!
//! After each major task, the system runs an independent review pass.
//! It checks: logic, performance, security, correctness, style,
//! documentation, and architecture.  If problems are found, new tasks
//! are created automatically.

use super::{ReviewCategory, ReviewCheck, ReviewResult};
use chrono::Utc;

/// Run a self-review pass on the given item.
///
/// `item` is a label for what was reviewed (e.g. "src/main.rs").
/// `checks` is a list of `(category, passed, detail)` tuples.
///
/// In a full system, the LLM performs the review.  Here we provide a
/// structure for organising review checks and creating follow-up tasks
/// from failures.
pub fn run_review(
    item: impl Into<String>,
    checks: Vec<(ReviewCategory, bool, String)>,
) -> ReviewResult {
    let item = item.into();
    let review_checks: Vec<ReviewCheck> = checks
        .into_iter()
        .map(|(cat, passed, detail)| ReviewCheck {
            category: cat,
            passed,
            detail,
        })
        .collect();

    let overall_pass = review_checks.iter().all(|c| c.passed);

    // Auto-create tasks for failed checks, ordered by severity so security and
    // correctness failures are scheduled before style and documentation.
    let mut failed: Vec<&ReviewCheck> = review_checks.iter().filter(|c| !c.passed).collect();
    failed.sort_by_key(|c| c.category.priority());
    let tasks_created: Vec<String> = failed
        .into_iter()
        .map(|c| format!("Fix {} issue in {}: {}", c.category.label(), item, c.detail))
        .collect();

    ReviewResult {
        reviewed_item: item,
        checks: review_checks,
        overall_pass,
        tasks_created,
        created_at: Utc::now(),
    }
}

/// Run a full review pass covering all categories.
///
/// This is a convenience method that runs all 7 review categories.
/// Each check function receives the item label and returns `(passed, detail)`.
pub fn run_full_review<F>(item: &str, check: F) -> ReviewResult
where
    F: Fn(ReviewCategory) -> (bool, String),
{
    let mut checks = Vec::new();
    for &cat in ReviewCategory::all() {
        let (passed, detail) = check(cat);
        checks.push((cat, passed, detail));
    }
    run_review(item, checks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_pass() {
        let result = run_review(
            "src/main.rs",
            vec![
                (ReviewCategory::Logic, true, "ok".into()),
                (ReviewCategory::Performance, true, "ok".into()),
            ],
        );
        assert!(result.overall_pass);
        assert!(result.tasks_created.is_empty());
    }

    #[test]
    fn test_creates_tasks_for_failures() {
        let result = run_review(
            "src/main.rs",
            vec![
                (ReviewCategory::Logic, true, "ok".into()),
                (ReviewCategory::Security, false, "SQL injection risk".into()),
                (ReviewCategory::Style, false, "Inconsistent naming".into()),
            ],
        );
        assert!(!result.overall_pass);
        assert_eq!(result.tasks_created.len(), 2);
        assert!(result.tasks_created[0].contains("security"));
        assert!(result.tasks_created[1].contains("style"));
    }

    #[test]
    fn failed_checks_are_prioritized_by_severity() {
        let result = run_review(
            "src/main.rs",
            vec![
                (ReviewCategory::Style, false, "naming".into()),
                (ReviewCategory::Security, false, "injection risk".into()),
                (ReviewCategory::Performance, false, "slow loop".into()),
            ],
        );
        assert_eq!(result.tasks_created.len(), 3);
        assert!(result.tasks_created[0].contains("security"));
        assert!(result.tasks_created[1].contains("performance"));
        assert!(result.tasks_created[2].contains("style"));
    }

    #[test]
    fn test_full_review_all_categories() {
        let result = run_full_review("module.rs", |_| (true, "looks good".to_string()));
        assert!(result.overall_pass);
        assert_eq!(result.checks.len(), 7);
    }

    #[test]
    fn test_full_review_with_failures() {
        let result = run_full_review("module.rs", |cat| {
            if cat == ReviewCategory::Security {
                (false, "dangerous eval".to_string())
            } else {
                (true, "fine".to_string())
            }
        });
        assert!(!result.overall_pass);
        assert_eq!(result.tasks_created.len(), 1);
        assert!(result.tasks_created[0].contains("dangerous eval"));
    }
}
