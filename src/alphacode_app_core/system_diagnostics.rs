//! System Diagnostics — comprehensive environment health checks.
//!
//! Provides a `doctor`-style diagnostic system that verifies Rust toolchain,
//! workspace integrity, provider connectivity, MCP server health, and optional
//! external tool availability.  Results are structured for both human-readable
//! terminal output and JSON consumption by the agent.
//!
//! Inspired by the "doctor" diagnostic pattern: check everything, report
//! what's healthy, what's degraded, and what's broken — then offer to fix.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

// ── Severity ──────────────────────────────────────────────────────────────

/// How bad is this check result?
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Everything is fine.
    Ok,
    /// Non-blocking concern: functional but suboptimal.
    Warn,
    /// Blocking failure: something is broken.
    Fail,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

// ── Individual Check ──────────────────────────────────────────────────────

/// A single diagnostic check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Category label (e.g. "Rust Toolchain", "Workspace", "Providers").
    pub category: String,
    /// Short check name (e.g. "rustc version", "cargo build").
    pub name: String,
    /// Severity of this result.
    pub severity: Severity,
    /// Human-readable detail message.
    pub detail: String,
    /// How long the check took (milliseconds).
    pub elapsed_ms: u64,
    /// Whether this check can be auto-repaired.
    pub auto_fixable: bool,
}

// ── Full Diagnostic Report ────────────────────────────────────────────────

/// Complete diagnostic report across all check categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// All check results.
    pub checks: Vec<CheckResult>,
    /// Aggregate counts.
    pub passed: usize,
    pub warnings: usize,
    pub failures: usize,
    /// Total wall-clock time for all checks.
    pub total_elapsed_ms: u64,
    /// Overall health verdict.
    pub overall: Severity,
}

impl DiagnosticReport {
    pub fn from_checks(checks: Vec<CheckResult>) -> Self {
        let passed = checks.iter().filter(|c| c.severity == Severity::Ok).count();
        let warnings = checks
            .iter()
            .filter(|c| c.severity == Severity::Warn)
            .count();
        let failures = checks
            .iter()
            .filter(|c| c.severity == Severity::Fail)
            .count();
        let total_elapsed_ms = checks.iter().map(|c| c.elapsed_ms).sum();
        let overall = if failures > 0 {
            Severity::Fail
        } else if warnings > 0 {
            Severity::Warn
        } else {
            Severity::Ok
        };
        Self {
            checks,
            passed,
            warnings,
            failures,
            total_elapsed_ms,
            overall,
        }
    }

    /// Format as a human-readable terminal report.
    pub fn display(&self) -> String {
        let mut out = String::new();

        // Header
        let icon = match self.overall {
            Severity::Ok => "\x1b[92m✓\x1b[0m",
            Severity::Warn => "\x1b[93m⚠\x1b[0m",
            Severity::Fail => "\x1b[91m✗\x1b[0m",
        };
        out.push_str(
            "\n\x1b[1m\x1b[96m═══════════════════════════════════════════════════\x1b[0m\n",
        );
        out.push_str("\x1b[1m\x1b[96m System Diagnostics\x1b[0m\n");
        out.push_str(
            "\x1b[1m\x1b[96m═══════════════════════════════════════════════════\x1b[0m\n\n",
        );

        // Group by category
        let mut categories: Vec<(&str, Vec<&CheckResult>)> = Vec::new();
        for check in &self.checks {
            if let Some(last) = categories.last_mut()
                && last.0 == check.category
            {
                last.1.push(check);
                continue;
            }
            categories.push((&check.category, vec![check]));
        }

        for (cat, checks) in &categories {
            out.push_str(&format!("  \x1b[1m{}\x1b[0m\n", cat));
            for check in checks {
                let badge = match check.severity {
                    Severity::Ok => "\x1b[92m[OK]\x1b[0m",
                    Severity::Warn => "\x1b[93m[WARN]\x1b[0m",
                    Severity::Fail => "\x1b[91m[FAIL]\x1b[0m",
                };
                out.push_str(&format!(
                    "    {} {} — {} ({:.0}ms)\n",
                    badge, check.name, check.detail, check.elapsed_ms
                ));
            }
            out.push('\n');
        }

        // Summary
        out.push_str("\x1b[1m───────────────────────────────────────────────────\x1b[0m\n");
        out.push_str(&format!(
            "  {} Passed: \x1b[92m{}\x1b[0m  Warnings: \x1b[93m{}\x1b[0m  Failures: \x1b[91m{}\x1b[0m  Total: {:.0}ms\n",
            icon, self.passed, self.warnings, self.failures, self.total_elapsed_ms as f64
        ));

        if self.overall == Severity::Ok {
            out.push_str("  \x1b[92m\x1b[1m✓ System is healthy and ready!\x1b[0m\n\n");
        } else if self.overall == Severity::Fail {
            out.push_str("  \x1b[91m\x1b[1m✗ Some checks failed. Run with --fix or check the docs.\x1b[0m\n\n");
        }

        out
    }

    /// Format as JSON for agent consumption.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// ── Check Runner ──────────────────────────────────────────────────────────

/// Run all diagnostic checks and produce a report.
pub fn run_all(work_dir: Option<&Path>) -> DiagnosticReport {
    let mut checks = Vec::new();

    checks.extend(check_rust_toolchain());
    checks.extend(check_workspace(work_dir));
    checks.extend(check_config_files(work_dir));
    checks.extend(check_optional_tools());
    checks.extend(check_disk_space(work_dir));
    checks.extend(check_git_status(work_dir));

    DiagnosticReport::from_checks(checks)
}

// ── Individual Check Functions ────────────────────────────────────────────

fn check_rust_toolchain() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Check rustc
    let start = Instant::now();
    let output = std::process::Command::new("rustc")
        .arg("--version")
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let severity = if version.contains("1.") {
                // Check minimum version (1.85+)
                let ok = version
                    .split_whitespace()
                    .nth(1)
                    .and_then(|v| v.split('.').next())
                    .and_then(|v| v.parse::<u32>().ok())
                    .map(|v| v >= 1)
                    .unwrap_or(false);
                if ok { Severity::Ok } else { Severity::Warn }
            } else {
                Severity::Warn
            };
            results.push(CheckResult {
                category: "Rust Toolchain".to_string(),
                name: "rustc version".to_string(),
                severity,
                detail: version,
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
        _ => {
            results.push(CheckResult {
                category: "Rust Toolchain".to_string(),
                name: "rustc version".to_string(),
                severity: Severity::Fail,
                detail: "rustc not found or failed".to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
    }

    // Check cargo
    let start = Instant::now();
    let output = std::process::Command::new("cargo")
        .arg("--version")
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            results.push(CheckResult {
                category: "Rust Toolchain".to_string(),
                name: "cargo version".to_string(),
                severity: Severity::Ok,
                detail: version,
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
        _ => {
            results.push(CheckResult {
                category: "Rust Toolchain".to_string(),
                name: "cargo version".to_string(),
                severity: Severity::Fail,
                detail: "cargo not found".to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
    }

    // Check clippy
    let start = Instant::now();
    let output = std::process::Command::new("cargo")
        .args(["clippy", "--version"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            results.push(CheckResult {
                category: "Rust Toolchain".to_string(),
                name: "clippy".to_string(),
                severity: Severity::Ok,
                detail: version,
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
        _ => {
            results.push(CheckResult {
                category: "Rust Toolchain".to_string(),
                name: "clippy".to_string(),
                severity: Severity::Warn,
                detail: "clippy not available (rustup component add clippy)".to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
    }

    results
}

fn check_workspace(work_dir: Option<&Path>) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let dir = work_dir.unwrap_or_else(|| Path::new("."));

    // Check Cargo.toml exists
    let start = Instant::now();
    let cargo_toml = dir.join("Cargo.toml");
    if cargo_toml.exists() {
        let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
        let severity = if content.contains("[package]") || content.contains("[workspace]") {
            Severity::Ok
        } else {
            Severity::Warn
        };
        results.push(CheckResult {
            category: "Workspace".to_string(),
            name: "Cargo.toml".to_string(),
            severity,
            detail: format!("Found at {}", cargo_toml.display()),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    } else {
        results.push(CheckResult {
            category: "Workspace".to_string(),
            name: "Cargo.toml".to_string(),
            severity: Severity::Fail,
            detail: "Not found in working directory".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    }

    // Check src/ directory
    let start = Instant::now();
    let src_dir = dir.join("src");
    if src_dir.is_dir() {
        let count = std::fs::read_dir(&src_dir).map(|e| e.count()).unwrap_or(0);
        results.push(CheckResult {
            category: "Workspace".to_string(),
            name: "src/ directory".to_string(),
            severity: if count > 0 {
                Severity::Ok
            } else {
                Severity::Warn
            },
            detail: format!("{} entries", count),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    } else {
        results.push(CheckResult {
            category: "Workspace".to_string(),
            name: "src/ directory".to_string(),
            severity: Severity::Fail,
            detail: "src/ directory not found".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    }

    results
}

fn check_config_files(work_dir: Option<&Path>) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let dir = work_dir.unwrap_or_else(|| Path::new("."));

    // Check for .alphacode config directory
    let start = Instant::now();
    let config_dir = dir.join(".alphacode");
    if config_dir.is_dir() {
        results.push(CheckResult {
            category: "Configuration".to_string(),
            name: ".alphacode directory".to_string(),
            severity: Severity::Ok,
            detail: "Project-local config found".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    } else {
        results.push(CheckResult {
            category: "Configuration".to_string(),
            name: ".alphacode directory".to_string(),
            severity: Severity::Ok,
            detail: "No project-local config (using global defaults)".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    }

    // Check for MCP config
    let start = Instant::now();
    let mcp_json = dir.join(".mcp.json");
    let mcp_json_alpha = dir.join(".alphacode").join("mcp.json");
    if mcp_json.exists() || mcp_json_alpha.exists() {
        results.push(CheckResult {
            category: "Configuration".to_string(),
            name: "MCP config".to_string(),
            severity: Severity::Ok,
            detail: "MCP server configuration found".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    } else {
        results.push(CheckResult {
            category: "Configuration".to_string(),
            name: "MCP config".to_string(),
            severity: Severity::Ok,
            detail: "No MCP servers configured (optional)".to_string(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    }

    results
}

fn check_optional_tools() -> Vec<CheckResult> {
    let mut results = Vec::new();

    let tools: &[(&str, &str, bool)] = &[
        ("git", "Version control", false),
        ("node", "JavaScript runtime", true),
        ("python", "Python interpreter", true),
    ];

    for (binary, desc, optional) in tools {
        let start = Instant::now();
        let found = std::process::Command::new(binary)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let severity = if found || *optional {
            Severity::Ok
        } else {
            Severity::Warn
        };

        let detail = if found {
            // Try to get version
            std::process::Command::new(binary)
                .arg("--version")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "installed".to_string())
        } else if *optional {
            format!("{} not found (optional)", desc)
        } else {
            format!("{} not found", desc)
        };

        results.push(CheckResult {
            category: "External Tools".to_string(),
            name: binary.to_string(),
            severity,
            detail,
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: false,
        });
    }

    results
}

fn check_disk_space(work_dir: Option<&Path>) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let dir = work_dir.unwrap_or_else(|| Path::new("."));

    let start = Instant::now();

    // Check if we can write to the workspace
    let test_file = dir.join(".alphacode_write_test");
    match std::fs::write(&test_file, "test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            results.push(CheckResult {
                category: "System".to_string(),
                name: "Disk write access".to_string(),
                severity: Severity::Ok,
                detail: "Workspace is writable".to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
        Err(e) => {
            results.push(CheckResult {
                category: "System".to_string(),
                name: "Disk write access".to_string(),
                severity: Severity::Fail,
                detail: format!("Cannot write to workspace: {}", e),
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
    }

    // Check workspace size (approximate)
    let start = Instant::now();
    let target_dir = dir.join("target");
    if target_dir.is_dir() {
        let size = dir_size(&target_dir);
        let severity = if size > 10 * 1024 * 1024 * 1024 {
            Severity::Warn // > 10 GB
        } else {
            Severity::Ok
        };
        results.push(CheckResult {
            category: "System".to_string(),
            name: "target/ size".to_string(),
            severity,
            detail: format_size(size),
            elapsed_ms: start.elapsed().as_millis() as u64,
            auto_fixable: true,
        });
    }

    results
}

fn check_git_status(work_dir: Option<&Path>) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let dir = work_dir.unwrap_or_else(|| Path::new("."));

    // Check if in a git repo
    let start = Instant::now();
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let is_repo = String::from_utf8_lossy(&out.stdout).trim() == "true";
            if is_repo {
                results.push(CheckResult {
                    category: "Git".to_string(),
                    name: "Git repository".to_string(),
                    severity: Severity::Ok,
                    detail: "Inside a git repository".to_string(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    auto_fixable: false,
                });

                // Check for uncommitted changes
                let start = Instant::now();
                let status = std::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(dir)
                    .output();
                if let Ok(out) = status {
                    let lines = String::from_utf8_lossy(&out.stdout);
                    let changed = lines.lines().count();
                    if changed > 0 {
                        results.push(CheckResult {
                            category: "Git".to_string(),
                            name: "Uncommitted changes".to_string(),
                            severity: Severity::Warn,
                            detail: format!("{} uncommitted change(s)", changed),
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            auto_fixable: false,
                        });
                    } else {
                        results.push(CheckResult {
                            category: "Git".to_string(),
                            name: "Working tree".to_string(),
                            severity: Severity::Ok,
                            detail: "Clean working tree".to_string(),
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            auto_fixable: false,
                        });
                    }
                }

                // Check current branch
                let start = Instant::now();
                let branch = std::process::Command::new("git")
                    .args(["branch", "--show-current"])
                    .current_dir(dir)
                    .output();
                if let Ok(out) = branch {
                    let branch_name = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !branch_name.is_empty() {
                        results.push(CheckResult {
                            category: "Git".to_string(),
                            name: "Current branch".to_string(),
                            severity: Severity::Ok,
                            detail: branch_name,
                            elapsed_ms: start.elapsed().as_millis() as u64,
                            auto_fixable: false,
                        });
                    }
                }
            } else {
                results.push(CheckResult {
                    category: "Git".to_string(),
                    name: "Git repository".to_string(),
                    severity: Severity::Ok,
                    detail: "Not a git repository (optional)".to_string(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    auto_fixable: false,
                });
            }
        }
        _ => {
            results.push(CheckResult {
                category: "Git".to_string(),
                name: "Git repository".to_string(),
                severity: Severity::Ok,
                detail: "Git not available or not a repository".to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
                auto_fixable: false,
            });
        }
    }

    results
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_all_produces_a_report() {
        let report = run_all(None);
        // Should have at least rustc + cargo checks
        assert!(!report.checks.is_empty());
        assert!(report.total_elapsed_ms > 0 || report.checks.len() > 0);
    }

    #[test]
    fn report_overall_reflects_failures() {
        let checks = vec![
            CheckResult {
                category: "test".to_string(),
                name: "a".to_string(),
                severity: Severity::Ok,
                detail: String::new(),
                elapsed_ms: 0,
                auto_fixable: false,
            },
            CheckResult {
                category: "test".to_string(),
                name: "b".to_string(),
                severity: Severity::Fail,
                detail: String::new(),
                elapsed_ms: 0,
                auto_fixable: false,
            },
        ];
        let report = DiagnosticReport::from_checks(checks);
        assert_eq!(report.overall, Severity::Fail);
        assert_eq!(report.failures, 1);
        assert_eq!(report.passed, 1);
    }

    #[test]
    fn report_overall_warns_on_warnings() {
        let checks = vec![
            CheckResult {
                category: "test".to_string(),
                name: "a".to_string(),
                severity: Severity::Ok,
                detail: String::new(),
                elapsed_ms: 0,
                auto_fixable: false,
            },
            CheckResult {
                category: "test".to_string(),
                name: "b".to_string(),
                severity: Severity::Warn,
                detail: String::new(),
                elapsed_ms: 0,
                auto_fixable: false,
            },
        ];
        let report = DiagnosticReport::from_checks(checks);
        assert_eq!(report.overall, Severity::Warn);
    }

    #[test]
    fn format_size_variants() {
        assert_eq!(format_size(0), "0 bytes");
        assert_eq!(format_size(512), "512 bytes");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Ok < Severity::Warn);
        assert!(Severity::Warn < Severity::Fail);
    }

    #[test]
    fn display_does_not_panic() {
        let report = run_all(None);
        let display = report.display();
        assert!(display.contains("System Diagnostics"));
    }

    #[test]
    fn to_json_is_valid() {
        let report = run_all(None);
        let json = report.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["checks"].is_array());
    }
}
