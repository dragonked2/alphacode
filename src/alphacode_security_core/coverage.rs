use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Tracks what has been tested and what remains uncovered.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CoverageTracker {
    /// Test results keyed by (endpoint, method, vuln_class).
    pub test_results: HashMap<String, TestRecord>,
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
        let previous = self.test_results.insert(key, record.clone());

        // Decrement stats for the overwritten result so re-tests don't inflate counts.
        if let Some(prev) = previous {
            match prev.result {
                TestResult::ConfirmedVulnerability => {
                    self.stats.confirmed_findings = self.stats.confirmed_findings.saturating_sub(1)
                }
                TestResult::PotentialVulnerability => {
                    self.stats.potential_findings = self.stats.potential_findings.saturating_sub(1)
                }
                TestResult::Error => self.stats.errors = self.stats.errors.saturating_sub(1),
                _ => {}
            }
        } else {
            self.stats.total_tests_run += 1;
        }

        let TestRecord {
            endpoint,
            method,
            vuln_class,
            result,
            timestamp,
            ..
        } = record;
        let is_positive = result.is_positive();
        let ep_cov = self
            .endpoint_coverage
            .entry(endpoint.clone())
            .or_insert_with(|| EndpointCoverage {
                endpoint,
                methods_tested: HashSet::new(),
                vuln_classes_tested: HashSet::new(),
                vuln_classes_with_signal: HashSet::new(),
                highest_severity: None,
                last_tested: None,
            });
        ep_cov.methods_tested.insert(method);
        ep_cov.vuln_classes_tested.insert(vuln_class.clone());
        ep_cov.last_tested = Some(timestamp);

        if is_positive {
            ep_cov.vuln_classes_with_signal.insert(vuln_class);
        }

        // Update stats
        match result {
            TestResult::ConfirmedVulnerability => self.stats.confirmed_findings += 1,
            TestResult::PotentialVulnerability => self.stats.potential_findings += 1,
            TestResult::Error => self.stats.errors += 1,
            _ => {}
        }
        self.stats.endpoints_tested = self.endpoint_coverage.len();
        self.stats.vuln_classes_tested = self
            .test_results
            .values()
            .map(|r| r.vuln_class.to_lowercase())
            .collect::<HashSet<_>>()
            .len();
    }

    /// Check if a specific test has already been performed.
    pub fn has_tested(&self, endpoint: &str, method: &str, vuln_class: &str) -> bool {
        let key = format!("{endpoint}:{method}:{vuln_class}");
        self.test_results.contains_key(&key)
    }

    /// Get all endpoints with confirmed vulnerabilities.
    pub fn confirmed_vulns(&self) -> Vec<&TestRecord> {
        self.test_results
            .values()
            .filter(|r| r.result == TestResult::ConfirmedVulnerability)
            .collect()
    }

    /// Get all endpoints with potential vulnerabilities.
    pub fn potential_vulns(&self) -> Vec<&TestRecord> {
        self.test_results
            .values()
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

    /// Identify which vuln classes have NOT been tested across all endpoints.
    pub fn untested_vuln_classes(&self, all_classes: &[String]) -> Vec<String> {
        let tested: HashSet<String> = self
            .test_results
            .values()
            .map(|r| r.vuln_class.to_lowercase())
            .collect();
        all_classes
            .iter()
            .filter(|c| !tested.contains(&c.to_lowercase()))
            .cloned()
            .collect()
    }

    /// Identify high-value endpoints that deserve deeper investigation:
    /// endpoints with signal, or with broad untested surface.
    pub fn high_value_endpoints(&self) -> Vec<&EndpointCoverage> {
        self.endpoint_coverage
            .values()
            .filter(|c| !c.vuln_classes_with_signal.is_empty() && c.vuln_classes_tested.len() < 5)
            .collect()
    }

    /// Update discovered endpoint count.
    pub fn set_discovered_endpoints(&mut self, count: usize) {
        self.stats.total_endpoints_discovered = count;
        self.stats.endpoints_tested = self.endpoint_coverage.len();
    }

    /// Update available vuln class count.
    pub fn set_vuln_classes(&mut self, count: usize) {
        self.stats.total_vuln_classes = count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
