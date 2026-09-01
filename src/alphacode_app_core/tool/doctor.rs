//! Doctor tool — run system diagnostics and report health status.
//!
//! Exposes the `system_diagnostics` module to the agent so it can check
//! environment health, workspace integrity, and external tool availability
//! on demand. This gives the agent a proactive way to diagnose issues before
//! they cause failures.

use super::{Tool, ToolContext, ToolOutput};
use crate::alphacode_app_core::system_diagnostics;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

pub struct DoctorTool;

#[derive(Deserialize)]
struct DoctorInput {
    /// Action: "check" (default) runs diagnostics, "summary" gives a quick
    /// overview, "json" returns raw JSON, "fix" attempts auto-repair.
    #[serde(default = "default_action")]
    action: String,
    /// Optional specific category to check (e.g. "Rust Toolchain", "Git").
    #[serde(default)]
    category: Option<String>,
}

fn default_action() -> String {
    "check".to_string()
}

#[async_trait]
impl Tool for DoctorTool {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self) -> &str {
        "Run system diagnostics: check Rust toolchain, workspace, config, external tools, disk, and git status. Use action='check' for full report, 'summary' for quick overview, 'json' for structured output, 'fix' to attempt auto-repair of common issues."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["check", "summary", "json", "fix"],
                    "description": "Action: 'check' for full report, 'summary' for quick overview, 'json' for structured data, 'fix' to attempt auto-repair."
                },
                "category": {
                    "type": "string",
                    "description": "Optional: filter to a specific check category (e.g. 'Rust Toolchain', 'Git', 'System')."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: DoctorInput = serde_json::from_value(input)?;
        let work_dir = ctx.working_dir.as_deref();

        match params.action.as_str() {
            "fix" => self.execute_fix(work_dir, &params).await,
            "summary" => {
                let report = system_diagnostics::run_all(work_dir);
                let report = self.filter_by_category(report, &params.category);
                Ok(ToolOutput::new(self.format_summary(&report)).with_title("Doctor: Summary"))
            }
            "json" => {
                let report = system_diagnostics::run_all(work_dir);
                let report = self.filter_by_category(report, &params.category);
                Ok(ToolOutput::new(report.to_json()).with_title("Doctor: JSON Report"))
            }
            _ => {
                let report = system_diagnostics::run_all(work_dir);
                let report = self.filter_by_category(report, &params.category);
                Ok(ToolOutput::new(report.display()).with_title("Doctor: Full Report"))
            }
        }
    }
}

impl DoctorTool {
    fn filter_by_category(
        &self,
        report: system_diagnostics::DiagnosticReport,
        category: &Option<String>,
    ) -> system_diagnostics::DiagnosticReport {
        if let Some(cat) = category {
            let cat_lower = cat.to_lowercase();
            let filtered: Vec<_> = report.checks.into_iter()
                .filter(|c| c.category.to_lowercase().contains(&cat_lower))
                .collect();
            system_diagnostics::DiagnosticReport::from_checks(filtered)
        } else {
            report
        }
    }

    fn format_summary(&self, report: &system_diagnostics::DiagnosticReport) -> String {
        let status_icon = match report.overall {
            system_diagnostics::Severity::Ok => "✓",
            system_diagnostics::Severity::Warn => "⚠",
            system_diagnostics::Severity::Fail => "✗",
        };
        let mut output = format!(
            "{} System Health: {} ({} passed, {} warnings, {} failures)\n",
            status_icon,
            report.overall.label(),
            report.passed,
            report.warnings,
            report.failures
        );
        if report.failures > 0 {
            output.push_str("\nFailed checks:\n");
            for check in &report.checks {
                if check.severity == system_diagnostics::Severity::Fail {
                    output.push_str(&format!(
                        "  ✗ [{}] {} — {}\n",
                        check.category, check.name, check.detail
                    ));
                }
            }
        }
        if report.warnings > 0 {
            output.push_str("\nWarnings:\n");
            for check in &report.checks {
                if check.severity == system_diagnostics::Severity::Warn {
                    output.push_str(&format!(
                        "  ⚠ [{}] {} — {}\n",
                        check.category, check.name, check.detail
                    ));
                }
            }
        }
        output
    }

    async fn execute_fix(
        &self,
        work_dir: Option<&Path>,
        params: &DoctorInput,
    ) -> Result<ToolOutput> {
        let report = system_diagnostics::run_all(work_dir);
        let report = self.filter_by_category(report, &params.category);

        let mut fixes = Vec::new();
        let mut skipped = Vec::new();

        for check in &report.checks {
            if check.severity == system_diagnostics::Severity::Fail && check.auto_fixable {
                // Attempt to fix common issues
                let fix_result = attempt_fix(&check.name, work_dir).await;
                match fix_result {
                    Ok(msg) => fixes.push(format!("✓ {} — {}", check.name, msg)),
                    Err(msg) => skipped.push(format!("✗ {} — {}", check.name, msg)),
                }
            } else if check.severity == system_diagnostics::Severity::Warn && check.auto_fixable {
                let fix_result = attempt_fix(&check.name, work_dir).await;
                match fix_result {
                    Ok(msg) => fixes.push(format!("✓ {} — {}", check.name, msg)),
                    Err(msg) => skipped.push(format!("✗ {} — {}", check.name, msg)),
                }
            }
        }

        // Build response
        let mut output = String::new();

        if fixes.is_empty() && skipped.is_empty() {
            output.push_str("No auto-fixable issues found.\n\n");
            output.push_str("Run `doctor` with action='check' for full diagnostics.");
        } else {
            if !fixes.is_empty() {
                output.push_str("Fixed:\n");
                for fix in &fixes {
                    output.push_str(&format!("  {}\n", fix));
                }
            }
            if !skipped.is_empty() {
                output.push_str("\nSkipped (manual intervention needed):\n");
                for skip in &skipped {
                    output.push_str(&format!("  {}\n", skip));
                }
            }
            output.push_str(&format!(
                "\n{} fix(es) applied, {} skipped.",
                fixes.len(),
                skipped.len()
            ));
        }

        Ok(ToolOutput::new(output).with_title("Doctor: Auto-Fix"))
    }
}

/// Attempt to fix a specific diagnostic issue.
async fn attempt_fix(check_name: &str, work_dir: Option<&Path>) -> Result<String, String> {
    match check_name {
        "target/ size" => {
            // Clean target directory
            let dir = work_dir.unwrap_or_else(|| Path::new("."));
            let target = dir.join("target");
            if target.is_dir() {
                match std::fs::remove_dir_all(&target) {
                    Ok(_) => Ok("Cleaned target/ directory".to_string()),
                    Err(e) => Err(format!("Failed to clean target/: {}", e)),
                }
            } else {
                Err("target/ directory not found".to_string())
            }
        }
        "Disk write access" => {
            // Try to create .alphacode directory
            let dir = work_dir.unwrap_or_else(|| Path::new("."));
            let alpha_dir = dir.join(".alphacode");
            match std::fs::create_dir_all(&alpha_dir) {
                Ok(_) => Ok("Created .alphacode directory".to_string()),
                Err(e) => Err(format!("Failed to create .alphacode: {}", e)),
            }
        }
        "rustc version" | "cargo version" => {
            Err("Rust toolchain must be installed manually via rustup".to_string())
        }
        "clippy" => {
            Err("Install clippy: rustup component add clippy".to_string())
        }
        _ => Err(format!("No auto-fix available for '{}'", check_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tool_core::{ToolContext, ToolExecutionMode};

    fn test_ctx() -> ToolContext {
        ToolContext {
            session_id: "test".to_string(),
            message_id: "test".to_string(),
            tool_call_id: "test".to_string(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        }
    }

    #[tokio::test]
    async fn doctor_check_runs() {
        let tool = DoctorTool;
        let result = tool.execute(json!({"action": "check"}), test_ctx()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.output.contains("System Diagnostics"));
    }

    #[tokio::test]
    async fn doctor_summary_runs() {
        let tool = DoctorTool;
        let result = tool.execute(json!({"action": "summary"}), test_ctx()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.output.contains("System Health"));
    }

    #[tokio::test]
    async fn doctor_json_runs() {
        let tool = DoctorTool;
        let result = tool.execute(json!({"action": "json"}), test_ctx()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output.output).unwrap();
        assert!(parsed["checks"].is_array());
    }

    #[tokio::test]
    async fn doctor_fix_runs() {
        let tool = DoctorTool;
        let result = tool.execute(json!({"action": "fix"}), test_ctx()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain either fixes or "No auto-fixable issues"
        assert!(output.output.contains("fix") || output.output.contains("No auto-fixable"));
    }

    #[tokio::test]
    async fn doctor_category_filter() {
        let tool = DoctorTool;
        let result = tool.execute(json!({"action": "check", "category": "Rust"}), test_ctx()).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should only show Rust-related checks
        assert!(output.output.contains("rustc") || output.output.contains("cargo"));
    }
}
