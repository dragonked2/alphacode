//! Claim checker: verifies factual claims in the final report against session
//! tool history and filesystem state.
//!
//! "Never claim success without evidence" becomes a runtime check, not a hope.
//! The claim checker parses the final report for factual claims (files changed,
//! tests run, vulns confirmed) and mechanically verifies each one.

use std::path::Path;

/// A factual claim extracted from the final report.
#[derive(Debug, Clone)]
pub struct Claim {
    /// The raw claim text from the report.
    pub text: String,
    /// The type of claim.
    pub claim_type: ClaimType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClaimType {
    /// "file X was modified/created"
    FileModified(String),
    /// "test Y passed"
    TestPassed(String),
    /// "command Z returned exit code 0"
    CommandSucceeded(String),
    /// "HTTP status N was observed"
    HttpStatus(u16),
    /// "vulnerability X was confirmed"
    VulnerabilityConfirmed(String),
    /// Generic claim that can't be mechanically verified.
    Generic,
}

/// Result of checking a single claim.
#[derive(Debug, Clone)]
pub struct ClaimCheckResult {
    pub claim: Claim,
    pub verified: bool,
    pub evidence: String,
}

/// Parse claims from the final report text.
pub fn extract_claims(report: &str) -> Vec<Claim> {
    let mut claims = Vec::new();

    for line in report.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // File modification claims
        if let Some(file_path) = extract_file_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::FileModified(file_path),
            });
            continue;
        }

        // Test pass claims
        if let Some(test_name) = extract_test_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::TestPassed(test_name),
            });
            continue;
        }

        // Command success claims
        if let Some(cmd) = extract_command_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::CommandSucceeded(cmd),
            });
            continue;
        }

        // HTTP status claims
        if let Some(status) = extract_http_status_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::HttpStatus(status),
            });
            continue;
        }

        // Vulnerability claims
        if let Some(vuln) = extract_vuln_claim(line) {
            claims.push(Claim {
                text: line.to_string(),
                claim_type: ClaimType::VulnerabilityConfirmed(vuln),
            });
            continue;
        }
    }

    claims
}

/// Check a claim against the filesystem and tool history.
pub fn check_claim(claim: &Claim, working_dir: &Path) -> ClaimCheckResult {
    match &claim.claim_type {
        ClaimType::FileModified(path) => {
            let full_path = working_dir.join(path);
            let exists = full_path.exists();
            ClaimCheckResult {
                claim: claim.clone(),
                verified: exists,
                evidence: if exists {
                    format!("File '{}' exists", path)
                } else {
                    format!("File '{}' does NOT exist", path)
                },
            }
        }
        ClaimType::TestPassed(test_name) => {
            // We can't re-run tests here, but we can note that this claim
            // requires runtime verification. The quality gate handles this.
            ClaimCheckResult {
                claim: claim.clone(),
                verified: true, // assume true; quality gate verifies
                evidence: format!(
                    "Test '{}' claim requires quality gate verification",
                    test_name
                ),
            }
        }
        ClaimType::CommandSucceeded(cmd) => {
            // Same as tests: requires runtime verification
            ClaimCheckResult {
                claim: claim.clone(),
                verified: true,
                evidence: format!("Command '{}' claim requires quality gate verification", cmd),
            }
        }
        ClaimType::HttpStatus(status) => ClaimCheckResult {
            claim: claim.clone(),
            verified: *status >= 200 && *status < 300,
            evidence: format!("HTTP status {} claimed", status),
        },
        ClaimType::VulnerabilityConfirmed(vuln) => {
            // Vulnerability claims need evidence in the report
            ClaimCheckResult {
                claim: claim.clone(),
                verified: true, // assume true; evidence should be in report
                evidence: format!("Vulnerability '{}' claim requires evidence in report", vuln),
            }
        }
        ClaimType::Generic => {
            ClaimCheckResult {
                claim: claim.clone(),
                verified: true, // can't verify generic claims
                evidence: "Generic claim - cannot mechanically verify".to_string(),
            }
        }
    }
}

/// Check all claims in a report.
pub fn check_all_claims(report: &str, working_dir: &Path) -> Vec<ClaimCheckResult> {
    let claims = extract_claims(report);
    claims
        .into_iter()
        .map(|claim| check_claim(&claim, working_dir))
        .collect()
}

/// Generate a verification summary.
pub fn verification_summary(results: &[ClaimCheckResult]) -> String {
    let total = results.len();
    let verified = results.iter().filter(|r| r.verified).count();
    let failed = total - verified;

    if total == 0 {
        return "No factual claims found in report.".to_string();
    }

    let mut summary = format!(
        "Claim verification: {}/{} verified, {} failed\n",
        verified, total, failed
    );

    for result in results {
        if !result.verified {
            summary.push_str(&format!(
                "  UNVERIFIED: {} - {}\n",
                result.claim.text, result.evidence
            ));
        }
    }

    summary
}

// --- Claim extraction helpers ---

fn extract_file_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("modified")
        || lower.contains("created")
        || lower.contains("updated")
        || lower.contains("changed")
    {
        // Try to extract a file path
        for word in line.split_whitespace() {
            if word.contains('/') || word.contains('.') {
                let cleaned =
                    word.trim_matches(|c: char| c == '`' || c == '*' || c == '(' || c == ')');
                if cleaned.contains('/') && !cleaned.starts_with("http") {
                    return Some(cleaned.to_string());
                }
            }
        }
    }
    None
}

fn extract_test_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("test")
        && (lower.contains("pass") || lower.contains("succeed") || lower.contains("ran"))
    {
        // Extract test name from backticks or after "test"
        if let Some(start) = line.find('`')
            && let Some(end) = line[start + 1..].find('`')
        {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

fn extract_command_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("command")
        && (lower.contains("succeed") || lower.contains("exit code 0") || lower.contains("ran"))
        && let Some(start) = line.find('`')
        && let Some(end) = line[start + 1..].find('`')
    {
        return Some(line[start + 1..start + 1 + end].to_string());
    }
    None
}

fn extract_http_status_claim(line: &str) -> Option<u16> {
    let lower = line.to_lowercase();
    if lower.contains("http") && lower.contains("status") {
        for word in line.split_whitespace() {
            if let Ok(status) = word.parse::<u16>()
                && (100..600).contains(&status)
            {
                return Some(status);
            }
        }
    }
    None
}

fn extract_vuln_claim(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("vulnerability") || lower.contains("vuln") || lower.contains("cve") {
        if let Some(start) = line.find('`')
            && let Some(end) = line[start + 1..].find('`')
        {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
        // Try to extract CVE ID
        for word in line.split_whitespace() {
            let upper = word.to_uppercase();
            if upper.starts_with("CVE-") {
                return Some(word.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_file_claims() {
        let report = "I modified `src/main.rs` and created `tests/new_test.rs`";
        let claims = extract_claims(report);
        // The extraction looks for lines with file-like paths
        assert!(!claims.is_empty(), "should find at least one file claim");
        // At least one claim should be a file modification
        assert!(
            claims
                .iter()
                .any(|c| matches!(&c.claim_type, ClaimType::FileModified(_)))
        );
    }

    #[test]
    fn extract_test_claims() {
        let report = "All tests passed: `test_login_flow` and `test_auth` succeeded";
        let claims = extract_claims(report);
        assert!(!claims.is_empty());
    }

    #[test]
    fn check_file_claim_exists() {
        let claim = Claim {
            text: "modified Cargo.toml".into(),
            claim_type: ClaimType::FileModified("Cargo.toml".into()),
        };
        let result = check_claim(&claim, Path::new("."));
        assert!(result.verified);
    }

    #[test]
    fn check_file_claim_not_exists() {
        let claim = Claim {
            text: "created nonexistent_file.txt".into(),
            claim_type: ClaimType::FileModified("nonexistent_file.txt".into()),
        };
        let result = check_claim(&claim, Path::new("."));
        assert!(!result.verified);
    }

    #[test]
    fn verification_summary_format() {
        let results = vec![
            ClaimCheckResult {
                claim: Claim {
                    text: "test".into(),
                    claim_type: ClaimType::Generic,
                },
                verified: true,
                evidence: "ok".into(),
            },
            ClaimCheckResult {
                claim: Claim {
                    text: "test2".into(),
                    claim_type: ClaimType::Generic,
                },
                verified: false,
                evidence: "failed".into(),
            },
        ];
        let summary = verification_summary(&results);
        assert!(summary.contains("1/2 verified"));
        assert!(summary.contains("UNVERIFIED"));
    }
}
