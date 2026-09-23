//! Self-Review — independent review pass after each major task.
//!
//! After each major task, the system runs an independent review pass.
//! It checks: logic, performance, security, correctness, style,
//! documentation, architecture, accessibility, internationalization,
//! testing, maintainability, and readability.
//!
//! If problems are found, new tasks are created automatically with
//! severity-aware scheduling and automated fix suggestions.

use super::{
    AutomatedFix, ReviewCategory, ReviewCheck, ReviewHistory, ReviewHistoryEntry, ReviewResult,
    ReviewSeverity, ReviewTemplate,
};
use chrono::Utc;

/// Default review templates for common code patterns.
pub fn default_templates() -> Vec<ReviewTemplate> {
    vec![
        ReviewTemplate {
            name: "sql_query".into(),
            category: ReviewCategory::Security,
            pattern: r"(?i)(execute|query|exec)\s*\(".into(),
            checks: vec![
                (
                    ReviewCategory::Security,
                    "SQL query detected — check for parameterized queries".into(),
                ),
                (
                    ReviewCategory::Performance,
                    "Check for N+1 query patterns".into(),
                ),
            ],
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Security,
                severity: ReviewSeverity::High,
                description: "Use parameterized queries instead of string interpolation".into(),
                file_path: String::new(),
                line_range: None,
                replacement: None,
                confidence: 0.9,
            }],
        },
        ReviewTemplate {
            name: "unwrap_chain".into(),
            category: ReviewCategory::Correctness,
            pattern: r"\.unwrap\(\)".into(),
            checks: vec![(
                ReviewCategory::Correctness,
                "unwrap() may panic — consider using ? or expect()".into(),
            )],
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Correctness,
                severity: ReviewSeverity::Medium,
                description: "Replace unwrap() with proper error handling".into(),
                file_path: String::new(),
                line_range: None,
                replacement: None,
                confidence: 0.85,
            }],
        },
        ReviewTemplate {
            name: "todo_fixme".into(),
            category: ReviewCategory::Documentation,
            pattern: r"(?i)(TODO|FIXME|HACK|XXX)\b".into(),
            checks: vec![
                (
                    ReviewCategory::Documentation,
                    "Unresolved TODO/FIXME comment found".into(),
                ),
                (
                    ReviewCategory::Maintainability,
                    "Incomplete code should be tracked as a task".into(),
                ),
            ],
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Documentation,
                severity: ReviewSeverity::Low,
                description: "Convert TODO to a tracked task".into(),
                file_path: String::new(),
                line_range: None,
                replacement: None,
                confidence: 0.95,
            }],
        },
        ReviewTemplate {
            name: "long_function".into(),
            category: ReviewCategory::Maintainability,
            pattern: r"(?m)^(pub\s+)?fn\s+\w+".into(),
            checks: vec![
                (
                    ReviewCategory::Maintainability,
                    "Functions over 50 lines should be refactored".into(),
                ),
                (
                    ReviewCategory::Readability,
                    "Long functions reduce readability".into(),
                ),
            ],
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Maintainability,
                severity: ReviewSeverity::Medium,
                description: "Consider splitting long function into smaller helpers".into(),
                file_path: String::new(),
                line_range: None,
                replacement: None,
                confidence: 0.7,
            }],
        },
        ReviewTemplate {
            name: "magic_number".into(),
            category: ReviewCategory::Readability,
            pattern: r"\b\d{2,}\b".into(),
            checks: vec![(
                ReviewCategory::Readability,
                "Magic number detected — extract to named constant".into(),
            )],
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Readability,
                severity: ReviewSeverity::Low,
                description: "Extract magic number to a named constant".into(),
                file_path: String::new(),
                line_range: None,
                replacement: None,
                confidence: 0.6,
            }],
        },
        ReviewTemplate {
            name: "hardcoded_string".into(),
            category: ReviewCategory::Internationalization,
            pattern: r#""[A-Z][a-z]+(?:\s+[a-z]+)+"#.into(),
            checks: vec![
                (
                    ReviewCategory::Internationalization,
                    "Hardcoded user-facing string — use i18n".into(),
                ),
                (
                    ReviewCategory::Accessibility,
                    "Ensure strings are translatable for accessibility".into(),
                ),
            ],
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Internationalization,
                severity: ReviewSeverity::Low,
                description: "Replace hardcoded string with i18n key".into(),
                file_path: String::new(),
                line_range: None,
                replacement: None,
                confidence: 0.5,
            }],
        },
    ]
}

/// Compute a severity score for a set of checks.
///
/// Returns 0 (all pass) to 100 (critical failures).
pub fn compute_severity_score(checks: &[ReviewCheck]) -> u8 {
    if checks.is_empty() {
        return 0;
    }
    if checks.iter().all(|c| c.passed) {
        return 0;
    }
    let total: u32 = checks.iter().map(|c| c.severity.score() as u32).sum();
    let max_possible = checks.len() as u32 * 100;
    ((total * 100) / max_possible) as u8
}

/// Generate automated fixes for a set of review checks.
pub fn generate_fixes(
    item: &str,
    checks: &[ReviewCheck],
    templates: &[ReviewTemplate],
) -> Vec<AutomatedFix> {
    let mut fixes: Vec<AutomatedFix> = checks
        .iter()
        .filter(|c| !c.passed)
        .flat_map(|c| {
            c.auto_fixes
                .iter()
                .map(move |f| {
                    let mut fix = f.clone();
                    fix.file_path = item.to_string();
                    fix
                })
                .chain(
                    templates
                        .iter()
                        .filter(|t| t.category == c.category)
                        .flat_map(|t| {
                            t.auto_fixes.iter().map(move |f| {
                                let mut fix = f.clone();
                                fix.file_path = item.to_string();
                                fix
                            })
                        }),
                )
        })
        .collect();

    fixes.sort_by(|a, b| b.severity.cmp(&a.severity));
    fixes.dedup_by(|a, b| a.description == b.description && a.file_path == b.file_path);
    fixes
}

/// Build a history entry from a review result.
pub fn build_history_entry(result: &ReviewResult) -> ReviewHistoryEntry {
    let mut severity_scores = std::collections::HashMap::new();
    for check in &result.checks {
        severity_scores
            .entry(check.category.label().to_string())
            .and_modify(|e: &mut u8| *e = (*e).max(check.severity.score()))
            .or_insert(check.severity.score());
    }

    ReviewHistoryEntry {
        reviewed_item: result.reviewed_item.clone(),
        overall_pass: result.overall_pass,
        total_checks: result.checks.len(),
        failed_checks: result.checks.iter().filter(|c| !c.passed).count(),
        severity_scores,
        auto_fixes_generated: result.auto_fixes.len(),
        tasks_created: result.tasks_created.len(),
        created_at: result.created_at,
    }
}

/// Run a self-review pass on the given item.
///
/// `item` is a label for what was reviewed (e.g. "src/main.rs").
/// `checks` is a list of `(category, passed, severity, detail)` tuples.
/// `auto_fixes` are optional automated fix suggestions.
///
/// In a full system, the LLM performs the review.  Here we provide a
/// structure for organising review checks and creating follow-up tasks
/// from failures.
pub fn run_review(
    item: impl Into<String>,
    checks: Vec<(ReviewCategory, bool, ReviewSeverity, String)>,
    auto_fixes: Vec<AutomatedFix>,
) -> ReviewResult {
    let item = item.into();
    let review_checks: Vec<ReviewCheck> = checks
        .into_iter()
        .map(|(cat, passed, severity, detail)| ReviewCheck {
            category: cat,
            passed,
            severity,
            detail,
            auto_fixes: Vec::new(),
        })
        .collect();

    let overall_pass = review_checks.iter().all(|c| c.passed);
    let severity_score = compute_severity_score(&review_checks);

    // Auto-create tasks for failed checks, ordered by severity so security and
    // correctness failures are scheduled before style and documentation.
    let mut failed: Vec<&ReviewCheck> = review_checks.iter().filter(|c| !c.passed).collect();
    failed.sort_by_key(|c| (c.severity, c.category.priority()));
    let tasks_created: Vec<String> = failed
        .into_iter()
        .map(|c| {
            format!(
                "[{}] Fix {} issue in {}: {}",
                c.severity.label(),
                c.category.label(),
                item,
                c.detail
            )
        })
        .collect();

    let result = ReviewResult {
        reviewed_item: item,
        checks: review_checks,
        overall_pass,
        tasks_created,
        created_at: Utc::now(),
        auto_fixes,
        severity_score,
        history_entry: None,
    };

    // Attach history entry
    let mut result = result;
    result.history_entry = Some(build_history_entry(&result));
    result
}

/// Run a self-review pass with default severity.
pub fn run_review_default_severity(
    item: impl Into<String>,
    checks: Vec<(ReviewCategory, bool, String)>,
) -> ReviewResult {
    let upgraded: Vec<(ReviewCategory, bool, ReviewSeverity, String)> = checks
        .into_iter()
        .map(|(cat, passed, detail)| (cat, passed, ReviewSeverity::Medium, detail))
        .collect();
    run_review(item, upgraded, Vec::new())
}

/// Run a full review pass covering all categories.
///
/// This is a convenience method that runs all 12 review categories.
/// Each check function receives the item label and returns `(passed, detail)`.
pub fn run_full_review<F>(item: &str, check: F) -> ReviewResult
where
    F: Fn(ReviewCategory) -> (bool, String),
{
    let mut checks = Vec::new();
    for &cat in ReviewCategory::all() {
        let (passed, detail) = check(cat);
        checks.push((cat, passed, ReviewSeverity::Medium, detail));
    }
    let review_checks: Vec<ReviewCheck> = checks
        .iter()
        .map(|(cat, passed, severity, detail)| ReviewCheck {
            category: *cat,
            passed: *passed,
            severity: *severity,
            detail: detail.clone(),
            auto_fixes: Vec::new(),
        })
        .collect();
    let auto_fixes = generate_fixes(item, &review_checks, &default_templates());
    run_review(item, checks, auto_fixes)
}

/// Run a full review pass with custom severity per category.
pub fn run_full_review_with_severity<F>(item: &str, check: F) -> ReviewResult
where
    F: Fn(ReviewCategory) -> (bool, ReviewSeverity, String),
{
    let mut checks = Vec::new();
    for &cat in ReviewCategory::all() {
        let (passed, severity, detail) = check(cat);
        checks.push((cat, passed, severity, detail));
    }
    let review_checks: Vec<ReviewCheck> = checks
        .iter()
        .map(|(cat, passed, severity, detail)| ReviewCheck {
            category: *cat,
            passed: *passed,
            severity: *severity,
            detail: detail.clone(),
            auto_fixes: Vec::new(),
        })
        .collect();
    let auto_fixes = generate_fixes(item, &review_checks, &default_templates());
    run_review(item, checks, auto_fixes)
}

/// Run a targeted review using templates for specific patterns.
pub fn run_template_review(item: &str, source: &str) -> ReviewResult {
    let templates = default_templates();
    let mut checks = Vec::new();
    let mut all_fixes = Vec::new();

    for template in &templates {
        let regex = regex::Regex::new(&template.pattern);
        if let Ok(re) = regex {
            let matches = re.find_iter(source).count();
            if matches > 0 {
                for (cat, detail) in &template.checks {
                    checks.push((
                        *cat,
                        false,
                        ReviewSeverity::Medium,
                        format!("{} ({} occurrences)", detail, matches),
                    ));
                }
                all_fixes.extend(template.auto_fixes.iter().map(|f| {
                    let mut fix = f.clone();
                    fix.file_path = item.to_string();
                    fix
                }));
            }
        }
    }

    if checks.is_empty() {
        for &cat in ReviewCategory::all() {
            checks.push((cat, true, ReviewSeverity::Info, "no issues found".into()));
        }
    }

    run_review(item, checks, all_fixes)
}

/// Compute aggregate statistics from review history.
pub fn compute_history_stats(history: &ReviewHistory) -> HistoryStats {
    let category_counts = history.category_failure_counts();
    let most_common_failure = category_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(cat, _)| cat.clone());

    let recent = history.recent_failures(10);
    let trend = if recent.len() >= 2 {
        let recent_pass_rate =
            recent.iter().filter(|e| e.overall_pass).count() as f32 / recent.len() as f32;
        if recent_pass_rate > history.overall_pass_rate {
            HistoryTrend::Improving
        } else if recent_pass_rate < history.overall_pass_rate {
            HistoryTrend::Worsening
        } else {
            HistoryTrend::Stable
        }
    } else {
        HistoryTrend::InsufficientData
    };

    HistoryStats {
        total_reviews: history.total_reviews,
        overall_pass_rate: history.overall_pass_rate,
        avg_severity_score: history.avg_severity_score,
        most_common_failure_category: most_common_failure,
        trend,
    }
}

/// Statistics derived from review history.
#[derive(Debug, Clone)]
pub struct HistoryStats {
    pub total_reviews: usize,
    pub overall_pass_rate: f32,
    pub avg_severity_score: f32,
    pub most_common_failure_category: Option<String>,
    pub trend: HistoryTrend,
}

/// Trend direction for review history.
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryTrend {
    Improving,
    Stable,
    Worsening,
    InsufficientData,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_pass() {
        let result = run_review(
            "src/main.rs",
            vec![
                (
                    ReviewCategory::Logic,
                    true,
                    ReviewSeverity::Info,
                    "ok".into(),
                ),
                (
                    ReviewCategory::Performance,
                    true,
                    ReviewSeverity::Info,
                    "ok".into(),
                ),
            ],
            Vec::new(),
        );
        assert!(result.overall_pass);
        assert!(result.tasks_created.is_empty());
        assert_eq!(result.severity_score, 0);
    }

    #[test]
    fn test_creates_tasks_for_failures() {
        let result = run_review(
            "src/main.rs",
            vec![
                (
                    ReviewCategory::Logic,
                    true,
                    ReviewSeverity::Info,
                    "ok".into(),
                ),
                (
                    ReviewCategory::Security,
                    false,
                    ReviewSeverity::Critical,
                    "SQL injection risk".into(),
                ),
                (
                    ReviewCategory::Style,
                    false,
                    ReviewSeverity::Low,
                    "Inconsistent naming".into(),
                ),
            ],
            Vec::new(),
        );
        assert!(!result.overall_pass);
        assert_eq!(result.tasks_created.len(), 2);
        assert!(result.tasks_created[0].contains("[critical]"));
        assert!(result.tasks_created[0].contains("security"));
        assert!(result.tasks_created[1].contains("[low]"));
        assert!(result.tasks_created[1].contains("style"));
    }

    #[test]
    fn failed_checks_are_prioritized_by_severity() {
        let result = run_review(
            "src/main.rs",
            vec![
                (
                    ReviewCategory::Style,
                    false,
                    ReviewSeverity::Low,
                    "naming".into(),
                ),
                (
                    ReviewCategory::Security,
                    false,
                    ReviewSeverity::Critical,
                    "injection risk".into(),
                ),
                (
                    ReviewCategory::Performance,
                    false,
                    ReviewSeverity::Medium,
                    "slow loop".into(),
                ),
            ],
            Vec::new(),
        );
        assert_eq!(result.tasks_created.len(), 3);
        assert!(result.tasks_created[0].contains("[critical]"));
        assert!(result.tasks_created[1].contains("[medium]"));
        assert!(result.tasks_created[2].contains("[low]"));
    }

    #[test]
    fn test_full_review_all_categories() {
        let result = run_full_review("module.rs", |_| (true, "looks good".to_string()));
        assert!(result.overall_pass);
        assert_eq!(result.checks.len(), 12);
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

    #[test]
    fn test_severity_score_computation() {
        let checks = vec![
            ReviewCheck {
                category: ReviewCategory::Security,
                passed: false,
                severity: ReviewSeverity::Critical,
                detail: "critical issue".into(),
                auto_fixes: Vec::new(),
            },
            ReviewCheck {
                category: ReviewCategory::Style,
                passed: true,
                severity: ReviewSeverity::Info,
                detail: "ok".into(),
                auto_fixes: Vec::new(),
            },
        ];
        let score = compute_severity_score(&checks);
        assert_eq!(score, 52);
    }

    #[test]
    fn test_severity_score_empty() {
        let checks = Vec::new();
        let score = compute_severity_score(&checks);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_history_tracking() {
        let mut history = ReviewHistory::new();
        assert_eq!(history.total_reviews, 0);

        let entry = ReviewHistoryEntry {
            reviewed_item: "src/main.rs".into(),
            overall_pass: true,
            total_checks: 10,
            failed_checks: 0,
            severity_scores: std::collections::HashMap::new(),
            auto_fixes_generated: 0,
            tasks_created: 0,
            created_at: Utc::now(),
        };
        history.add_entry(entry);
        assert_eq!(history.total_reviews, 1);
        assert_eq!(history.overall_pass_rate, 1.0);
    }

    #[test]
    fn test_history_recent_failures() {
        let mut history = ReviewHistory::new();
        for i in 0..5 {
            let entry = ReviewHistoryEntry {
                reviewed_item: format!("file_{}.rs", i),
                overall_pass: i % 2 == 0,
                total_checks: 10,
                failed_checks: if i % 2 == 0 { 0 } else { 2 },
                severity_scores: std::collections::HashMap::new(),
                auto_fixes_generated: 0,
                tasks_created: if i % 2 == 0 { 0 } else { 2 },
                created_at: Utc::now(),
            };
            history.add_entry(entry);
        }
        let failures = history.recent_failures(3);
        assert_eq!(failures.len(), 2);
    }

    #[test]
    fn test_history_category_counts() {
        let mut history = ReviewHistory::new();
        let mut scores = std::collections::HashMap::new();
        scores.insert("security".to_string(), 100);
        scores.insert("style".to_string(), 25);
        history.add_entry(ReviewHistoryEntry {
            reviewed_item: "a.rs".into(),
            overall_pass: false,
            total_checks: 12,
            failed_checks: 2,
            severity_scores: scores,
            auto_fixes_generated: 1,
            tasks_created: 2,
            created_at: Utc::now(),
        });
        let counts = history.category_failure_counts();
        assert_eq!(counts.get("security"), Some(&1));
        assert_eq!(counts.get("style"), Some(&1));
    }

    #[test]
    fn test_history_stats() {
        let mut history = ReviewHistory::new();
        for i in 0..3 {
            let entry = ReviewHistoryEntry {
                reviewed_item: format!("file_{}.rs", i),
                overall_pass: true,
                total_checks: 10,
                failed_checks: 0,
                severity_scores: std::collections::HashMap::new(),
                auto_fixes_generated: 0,
                tasks_created: 0,
                created_at: Utc::now(),
            };
            history.add_entry(entry);
        }
        let stats = compute_history_stats(&history);
        assert_eq!(stats.total_reviews, 3);
        assert_eq!(stats.overall_pass_rate, 1.0);
        assert_eq!(stats.trend, HistoryTrend::InsufficientData);
    }

    #[test]
    fn test_auto_fix_generation() {
        let item = "src/db.rs";
        let checks = vec![ReviewCheck {
            category: ReviewCategory::Security,
            passed: false,
            severity: ReviewSeverity::High,
            detail: "SQL injection".into(),
            auto_fixes: vec![AutomatedFix {
                category: ReviewCategory::Security,
                severity: ReviewSeverity::High,
                description: "Use parameterized query".into(),
                file_path: String::new(),
                line_range: Some((10, 15)),
                replacement: Some("query(\"SELECT * FROM users WHERE id = ?\", &[id])".into()),
                confidence: 0.9,
            }],
        }];
        let fixes = generate_fixes(item, &checks, &default_templates());
        assert!(!fixes.is_empty());
        assert_eq!(fixes[0].file_path, item);
    }

    #[test]
    fn test_default_severity_upgrade() {
        let result = run_review_default_severity(
            "src/lib.rs",
            vec![
                (ReviewCategory::Logic, true, "ok".into()),
                (ReviewCategory::Security, false, "bad".into()),
            ],
        );
        assert!(!result.overall_pass);
        assert!(result.tasks_created.iter().any(|t| t.contains("[medium]")));
    }

    #[test]
    fn test_template_review() {
        let source = r#"
            let query = format!("SELECT * FROM users WHERE name = '{}'", name);
            let value = some_number.unwrap();
            // TODO: fix this later
        "#;
        let result = run_template_review("src/db.rs", source);
        assert!(!result.overall_pass);
        assert!(result.checks.len() >= 3);
    }

    #[test]
    fn test_severity_labels() {
        assert_eq!(ReviewSeverity::Critical.label(), "critical");
        assert_eq!(ReviewSeverity::High.label(), "high");
        assert_eq!(ReviewSeverity::Medium.label(), "medium");
        assert_eq!(ReviewSeverity::Low.label(), "low");
        assert_eq!(ReviewSeverity::Info.label(), "info");
    }

    #[test]
    fn test_severity_scores() {
        assert_eq!(ReviewSeverity::Critical.score(), 100);
        assert_eq!(ReviewSeverity::High.score(), 75);
        assert_eq!(ReviewSeverity::Medium.score(), 50);
        assert_eq!(ReviewSeverity::Low.score(), 25);
        assert_eq!(ReviewSeverity::Info.score(), 5);
    }

    #[test]
    fn test_category_count() {
        assert_eq!(ReviewCategory::all().len(), 12);
    }

    #[test]
    fn test_category_labels() {
        assert_eq!(ReviewCategory::Accessibility.label(), "accessibility");
        assert_eq!(
            ReviewCategory::Internationalization.label(),
            "internationalization"
        );
        assert_eq!(ReviewCategory::Testing.label(), "testing");
        assert_eq!(ReviewCategory::Maintainability.label(), "maintainability");
        assert_eq!(ReviewCategory::Readability.label(), "readability");
    }

    #[test]
    fn test_history_entry_attached() {
        let result = run_review(
            "src/main.rs",
            vec![(
                ReviewCategory::Logic,
                true,
                ReviewSeverity::Info,
                "ok".into(),
            )],
            Vec::new(),
        );
        assert!(result.history_entry.is_some());
        let entry = result.history_entry.unwrap();
        assert!(entry.overall_pass);
        assert_eq!(entry.total_checks, 1);
        assert_eq!(entry.failed_checks, 0);
    }

    #[test]
    fn test_full_review_with_severity() {
        let result = run_full_review_with_severity("module.rs", |cat| match cat {
            ReviewCategory::Security => (false, ReviewSeverity::Critical, "eval usage".into()),
            ReviewCategory::Style => (false, ReviewSeverity::Low, "naming".into()),
            _ => (true, ReviewSeverity::Info, "ok".into()),
        });
        assert!(!result.overall_pass);
        assert_eq!(result.tasks_created.len(), 2);
        assert!(result.tasks_created[0].contains("[critical]"));
        assert!(result.tasks_created[1].contains("[low]"));
    }
}
