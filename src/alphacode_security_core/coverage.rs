use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Tracks what has been tested and what remains uncovered.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CoverageTracker {
    /// Test results keyed by (endpoint, method, vuln_class).
    pub test_results: HashMap<String, TestRecord>,
    /// Earlier positive observations awaiting a completed retest. An error or
    /// skip cannot refute evidence, even though the latest attempt is uncovered.
    #[serde(default)]
    pub retained_signals: HashMap<String, TestRecord>,
    /// Endpoint coverage: which endpoints have been tested for which vuln classes.
    pub endpoint_coverage: HashMap<String, EndpointCoverage>,
    /// Technology coverage: which tech-specific attack classes have been attempted.
    pub tech_coverage: HashMap<String, TechCoverage>,
    /// Overall statistics.
    pub stats: CoverageStats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestRecord {
    pub endpoint: String,
    pub method: String,
    pub vuln_class: String,
    pub result: TestResult,
    pub confidence: f64,
    pub agent: Option<String>,
    pub skill_used: Option<String>,
    pub timestamp: String,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestResult {
    NotTested,
    NoVulnerability,
    PotentialVulnerability,
    ConfirmedVulnerability,
    Error,
    Skipped,
}

impl TestResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotTested => "not_tested",
            Self::NoVulnerability => "no_vulnerability",
            Self::PotentialVulnerability => "potential_vulnerability",
            Self::ConfirmedVulnerability => "confirmed_vulnerability",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }

    pub fn is_positive(&self) -> bool {
        matches!(
            self,
            Self::PotentialVulnerability | Self::ConfirmedVulnerability
        )
    }

    /// The target was actually exercised and produced an interpretable result.
    /// This says nothing about other actors, fixtures, or target revisions.
    pub fn is_completed(&self) -> bool {
        matches!(
            self,
            Self::NoVulnerability | Self::PotentialVulnerability | Self::ConfirmedVulnerability
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EndpointCoverage {
    pub endpoint: String,
    pub methods_tested: HashSet<String>,
    pub vuln_classes_tested: HashSet<String>,
    pub vuln_classes_with_signal: HashSet<String>,
    pub highest_severity: Option<String>,
    pub last_tested: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TechCoverage {
    pub technology: String,
    pub attack_classes_attempted: Vec<String>,
    pub attack_classes_remaining: Vec<String>,
    pub relevant_cves_checked: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CoverageStats {
    pub total_endpoints_discovered: usize,
    pub endpoints_tested: usize,
    pub total_vuln_classes: usize,
    pub vuln_classes_tested: usize,
    pub total_tests_run: usize,
    pub confirmed_findings: usize,
    pub potential_findings: usize,
    pub errors: usize,
}

impl CoverageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a test result.
    pub fn record_test(&mut self, record: TestRecord) {
        let key = format!(
            "{}:{}:{}",
            record.endpoint, record.method, record.vuln_class
        );
        let previous = self.test_results.insert(key.clone(), record.clone());
        if record.result.is_completed() {
            self.retained_signals.remove(&key);
        } else if let Some(prior) = previous.as_ref().filter(|r| r.result.is_positive()) {
            self.retained_signals.insert(key, prior.clone());
        }

        // Attempt counters describe the latest retained result for each test.
        if let Some(prev) = previous {
            if prev.result == TestResult::Error {
                self.stats.errors = self.stats.errors.saturating_sub(1);
            }
        } else {
            self.stats.total_tests_run += 1;
        }

        let endpoint = &record.endpoint;
        let ep_cov = self
            .endpoint_coverage
            .entry(endpoint.clone())
            .or_insert_with(|| EndpointCoverage {
                endpoint: endpoint.clone(),
                methods_tested: HashSet::new(),
                vuln_classes_tested: HashSet::new(),
                vuln_classes_with_signal: HashSet::new(),
                highest_severity: None,
                last_tested: None,
            });
        // Rebuild from current results for THIS endpoint. A sibling method's
        // signal survives a negative retest; another endpoint's signal cannot
        // keep a refuted candidate alive here. Errors/skips remain coverage gaps.
        ep_cov.methods_tested.clear();
        ep_cov.vuln_classes_tested.clear();
        ep_cov.vuln_classes_with_signal.clear();
        ep_cov.last_tested = None;
        for retained in self
            .test_results
            .values()
            .filter(|r| &r.endpoint == endpoint)
        {
            if retained.result.is_completed() {
                ep_cov.methods_tested.insert(retained.method.clone());
                ep_cov
                    .vuln_classes_tested
                    .insert(retained.vuln_class.clone());
                if ep_cov
                    .last_tested
                    .as_ref()
                    .is_none_or(|last| retained.timestamp > *last)
                {
                    ep_cov.last_tested = Some(retained.timestamp.clone());
                }
            }
            if retained.result.is_positive() {
                ep_cov
                    .vuln_classes_with_signal
                    .insert(retained.vuln_class.clone());
            }
        }

        for retained in self
            .retained_signals
            .values()
            .filter(|r| &r.endpoint == endpoint)
        {
            ep_cov
                .vuln_classes_with_signal
                .insert(retained.vuln_class.clone());
        }

        // Update stats
        if record.result == TestResult::Error {
            self.stats.errors += 1;
        }
        self.stats.confirmed_findings = 0;
        self.stats.potential_findings = 0;
        for signal in self
            .test_results
            .values()
            .chain(self.retained_signals.values())
        {
            match signal.result {
                TestResult::ConfirmedVulnerability => self.stats.confirmed_findings += 1,
                TestResult::PotentialVulnerability => self.stats.potential_findings += 1,
                _ => {}
            }
        }
        self.stats.endpoints_tested = self
            .endpoint_coverage
            .values()
            .filter(|c| !c.methods_tested.is_empty())
            .count();
        self.stats.vuln_classes_tested = self
            .test_results
            .values()
            .filter(|r| r.result.is_completed())
            .map(|r| r.vuln_class.to_lowercase())
            .collect::<HashSet<_>>()
            .len();
    }

    /// Whether a specific test produced an interpretable result. Failed,
    /// skipped and merely planned work must remain eligible for investigation.
    pub fn has_tested(&self, endpoint: &str, method: &str, vuln_class: &str) -> bool {
        let key = format!("{endpoint}:{method}:{vuln_class}");
        self.test_results
            .get(&key)
            .is_some_and(|r| r.result.is_completed())
    }

    /// Get all endpoints with confirmed vulnerabilities.
    pub fn confirmed_vulns(&self) -> Vec<&TestRecord> {
        self.test_results
            .values()
            .chain(self.retained_signals.values())
            .filter(|r| r.result == TestResult::ConfirmedVulnerability)
            .collect()
    }

    /// Get all endpoints with potential vulnerabilities.
    pub fn potential_vulns(&self) -> Vec<&TestRecord> {
        self.test_results
            .values()
            .chain(self.retained_signals.values())
            .filter(|r| r.result == TestResult::PotentialVulnerability)
            .collect()
    }

    /// Get endpoints that have produced signal (candidates for deeper testing).
    pub fn shallow_coverage(&self) -> Vec<&EndpointCoverage> {
        self.endpoint_coverage
            .values()
            .filter(|c| !c.vuln_classes_with_signal.is_empty())
            .collect()
    }

    /// Classes with no completed test anywhere. This is not per-endpoint or
    /// per-actor coverage and must not be interpreted as exhaustive assessment.
    pub fn untested_vuln_classes(&self, all_classes: &[String]) -> Vec<String> {
        let tested: HashSet<String> = self
            .test_results
            .values()
            .filter(|r| r.result.is_completed())
            .map(|r| r.vuln_class.to_lowercase())
            .collect();
        all_classes
            .iter()
            .filter(|c| !tested.contains(&c.to_lowercase()))
            .cloned()
            .collect()
    }

    /// Endpoints with unresolved signals, regardless of how many classes have
    /// already been tested. Test count cannot dismiss a live candidate.
    pub fn high_value_endpoints(&self) -> Vec<&EndpointCoverage> {
        self.endpoint_coverage
            .values()
            .filter(|c| !c.vuln_classes_with_signal.is_empty())
            .collect()
    }

    /// Update discovered endpoint count.
    pub fn set_discovered_endpoints(&mut self, count: usize) {
        self.stats.total_endpoints_discovered = count;
    }

    /// Update available vuln class count.
    pub fn set_vuln_classes(&mut self, count: usize) {
        self.stats.total_vuln_classes = count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(endpoint: &str, method: &str, class: &str, result: TestResult) -> TestRecord {
        TestRecord {
            endpoint: endpoint.into(),
            method: method.into(),
            vuln_class: class.into(),
            result,
            confidence: 0.8,
            agent: None,
            skill_used: None,
            timestamp: "2026-11-04T10:00:00Z".into(),
            notes: None,
        }
    }

    #[test]
    fn failed_and_skipped_work_remains_uncovered() {
        for result in [
            TestResult::Error,
            TestResult::Skipped,
            TestResult::NotTested,
        ] {
            let mut tracker = CoverageTracker::new();
            tracker.record_test(record("/api", "GET", "idor", result));
            assert!(!tracker.has_tested("/api", "GET", "idor"));
            assert_eq!(tracker.stats.endpoints_tested, 0);
            assert_eq!(tracker.stats.vuln_classes_tested, 0);
            assert_eq!(
                tracker.untested_vuln_classes(&["idor".into()]),
                vec!["idor"]
            );
            tracker.set_discovered_endpoints(10);
            assert_eq!(tracker.stats.endpoints_tested, 0);
            tracker.record_test(record("/api", "GET", "idor", TestResult::NoVulnerability));
            assert!(tracker.has_tested("/api", "GET", "idor"));
            assert_eq!(tracker.stats.endpoints_tested, 1);
            assert_eq!(tracker.stats.errors, 0);
        }
    }

    #[test]
    fn signals_are_local_and_survive_sibling_method_retests() {
        let mut tracker = CoverageTracker::new();
        tracker.record_test(record(
            "/a",
            "GET",
            "idor",
            TestResult::PotentialVulnerability,
        ));
        tracker.record_test(record(
            "/b",
            "GET",
            "idor",
            TestResult::PotentialVulnerability,
        ));
        tracker.record_test(record(
            "/a",
            "POST",
            "idor",
            TestResult::ConfirmedVulnerability,
        ));
        tracker.record_test(record("/a", "GET", "idor", TestResult::NoVulnerability));
        assert!(
            tracker.endpoint_coverage["/a"]
                .vuln_classes_with_signal
                .contains("idor")
        );
        tracker.record_test(record("/a", "POST", "idor", TestResult::NoVulnerability));
        assert!(
            tracker.endpoint_coverage["/a"]
                .vuln_classes_with_signal
                .is_empty()
        );
        assert!(
            tracker.endpoint_coverage["/b"]
                .vuln_classes_with_signal
                .contains("idor")
        );
        assert_eq!(tracker.stats.confirmed_findings, 0);
        assert_eq!(tracker.stats.potential_findings, 1);
    }

    #[test]
    fn broad_testing_does_not_discard_live_signal() {
        let mut tracker = CoverageTracker::new();
        for class in ["idor", "xss", "sqli", "ssrf", "csrf", "ssti"] {
            tracker.record_test(record(
                "/api",
                "GET",
                class,
                TestResult::PotentialVulnerability,
            ));
        }
        assert_eq!(tracker.high_value_endpoints().len(), 1);
        tracker.record_test(record("/api", "GET", "idor", TestResult::Error));
        assert!(!tracker.has_tested("/api", "GET", "idor"));
        assert_eq!(tracker.stats.errors, 1);
        assert!(
            !tracker.endpoint_coverage["/api"]
                .vuln_classes_tested
                .contains("idor")
        );
        assert_eq!(tracker.potential_vulns().len(), 6);
        assert!(
            tracker.endpoint_coverage["/api"]
                .vuln_classes_with_signal
                .contains("idor")
        );
        tracker.record_test(record("/api", "GET", "idor", TestResult::Skipped));
        assert_eq!(tracker.potential_vulns().len(), 6);
        let serialized = serde_json::to_string(&tracker).unwrap();
        let mut tracker: CoverageTracker = serde_json::from_str(&serialized).unwrap();
        assert_eq!(tracker.potential_vulns().len(), 6);
        tracker.record_test(record("/api", "GET", "idor", TestResult::NoVulnerability));
        assert_eq!(tracker.potential_vulns().len(), 5);
        assert!(
            !tracker.endpoint_coverage["/api"]
                .vuln_classes_with_signal
                .contains("idor")
        );
    }

    #[test]
    fn coverage_tracker_records_tests() {
        let mut tracker = CoverageTracker::new();
        tracker.record_test(TestRecord {
            endpoint: "/api/users".to_string(),
            method: "GET".to_string(),
            vuln_class: "idor".to_string(),
            result: TestResult::PotentialVulnerability,
            confidence: 0.6,
            agent: None,
            skill_used: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            notes: None,
        });
        assert!(tracker.has_tested("/api/users", "GET", "idor"));
        assert_eq!(tracker.potential_vulns().len(), 1);
    }

    #[test]
    fn coverage_tracks_endpoint_signal() {
        let mut tracker = CoverageTracker::new();
        tracker.record_test(TestRecord {
            endpoint: "/api/users".to_string(),
            method: "GET".to_string(),
            vuln_class: "idor".to_string(),
            result: TestResult::PotentialVulnerability,
            confidence: 0.6,
            agent: None,
            skill_used: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            notes: None,
        });
        let ep = tracker.endpoint_coverage.get("/api/users").unwrap();
        assert!(ep.vuln_classes_with_signal.contains("idor"));
    }

    #[test]
    fn untested_classes() {
        let mut tracker = CoverageTracker::new();
        tracker.record_test(TestRecord {
            endpoint: "/api".to_string(),
            method: "GET".to_string(),
            vuln_class: "xss".to_string(),
            result: TestResult::NoVulnerability,
            confidence: 0.8,
            agent: None,
            skill_used: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            notes: None,
        });
        let all = vec!["xss".to_string(), "sqli".to_string(), "ssrf".to_string()];
        let untested = tracker.untested_vuln_classes(&all);
        assert_eq!(untested, vec!["sqli".to_string(), "ssrf".to_string()]);
    }
}
