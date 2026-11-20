//! Structured error types for the bug bounty system.
//!
//! This module provides rich, actionable error types that replace generic
//! `anyhow::Error` usage throughout the bug bounty workflow. Each error
//! variant includes context and recovery hints to enable automated retry
//! logic and clear user-facing messages.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for bug bounty operations.
pub type BountyResult<T> = Result<T, BountyError>;

/// Comprehensive error type for bug bounty operations.
#[derive(Debug, Error)]
pub enum BountyError {
    /// Tool execution failed.
    #[error("Tool '{tool}' execution failed: {reason}")]
    ToolExecution {
        /// Name of the tool that failed.
        tool: String,
        /// Human-readable reason for the failure.
        reason: String,
        /// Whether this error is transient and a retry might succeed.
        #[source]
        source: Option<anyhow::Error>,
        /// Suggested recovery action.
        recovery_hint: Option<String>,
    },

    /// Tool binary not found in PATH.
    #[error("Tool '{tool}' not found in PATH")]
    ToolNotFound {
        /// Name of the missing tool.
        tool: String,
        /// Suggested installation command or hint.
        install_hint: Option<String>,
        /// Available alternatives that could be used instead.
        alternatives: Vec<String>,
    },

    /// Tool timed out.
    #[error("Tool '{tool}' timed out after {timeout_secs}s")]
    ToolTimeout {
        /// Name of the tool that timed out.
        tool: String,
        /// Timeout duration in seconds.
        timeout_secs: u64,
        /// Whether partial output was captured.
        partial_output: Option<String>,
    },

    /// Tool produced invalid or unparseable output.
    #[error("Tool '{tool}' produced invalid output: {reason}")]
    InvalidToolOutput {
        /// Name of the tool.
        tool: String,
        /// Reason the output was invalid.
        reason: String,
        /// Raw output for debugging.
        raw_output: Option<String>,
    },

    /// Target is out of scope.
    #[error("Target '{target}' is out of scope: {reason}")]
    OutOfScope {
        /// The target that was rejected.
        target: String,
        /// Reason for rejection.
        reason: String,
        /// The scope policy that was violated.
        policy_violated: Option<String>,
    },

    /// Insufficient authorization for operation.
    #[error("Insufficient authorization for {operation}: {reason}")]
    Authorization {
        /// Operation that was attempted.
        operation: String,
        /// Reason authorization failed.
        reason: String,
        /// Required permissions that are missing.
        required_permissions: Vec<String>,
    },

    /// State persistence failed.
    #[error("Failed to {operation} state: {reason}")]
    StatePersistence {
        /// Operation that failed (save/load).
        operation: String,
        /// Reason for failure.
        reason: String,
        /// Path that was being accessed.
        path: Option<PathBuf>,
    },

    /// Task dependency cycle detected.
    #[error("Task dependency cycle detected involving: {tasks:?}")]
    DependencyCycle {
        /// Tasks involved in the cycle.
        tasks: Vec<String>,
    },

    /// Task prerequisite not met.
    #[error("Task '{task}' prerequisite not met: {prerequisite}")]
    PrerequisiteNotMet {
        /// Task that cannot proceed.
        task: String,
        /// Missing prerequisite.
        prerequisite: String,
    },

    /// Resource exhausted (rate limits, file descriptors, etc.).
    #[error("Resource exhausted: {resource} ({details})")]
    ResourceExhausted {
        /// Type of resource exhausted.
        resource: String,
        /// Details about the exhaustion.
        details: String,
        /// Suggested recovery action.
        recovery_hint: Option<String>,
    },

    /// Network error during tool execution.
    #[error("Network error during {operation}: {reason}")]
    NetworkError {
        /// Operation that failed.
        operation: String,
        /// Reason for the network failure.
        reason: String,
        /// Whether the error is transient.
        is_transient: bool,
    },

    /// Configuration error.
    #[error("Configuration error: {reason}")]
    Configuration {
        /// Description of the configuration issue.
        reason: String,
        /// Path to the configuration file (if applicable).
        config_path: Option<PathBuf>,
    },

    /// Validation error for finding or evidence.
    #[error("Validation failed for {item}: {reason}")]
    Validation {
        /// Item that failed validation.
        item: String,
        /// Reason for validation failure.
        reason: String,
        /// Expected format or criteria.
        expected: Option<String>,
    },

    /// Generic internal error with context.
    #[error("Internal error in {context}: {reason}")]
    Internal {
        /// Context where the error occurred.
        context: String,
        /// Reason for the error.
        reason: String,
        /// Underlying error source.
        #[source]
        source: Option<anyhow::Error>,
    },
}

impl BountyError {
    /// Create a tool execution error.
    pub fn tool_execution(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ToolExecution {
            tool: tool.into(),
            reason: reason.into(),
            source: None,
            recovery_hint: None,
        }
    }

    /// Create a tool execution error with source.
    pub fn tool_execution_with_source(
        tool: impl Into<String>,
        reason: impl Into<String>,
        source: anyhow::Error,
    ) -> Self {
        Self::ToolExecution {
            tool: tool.into(),
            reason: reason.into(),
            source: Some(source),
            recovery_hint: None,
        }
    }

    /// Create a tool not found error.
    pub fn tool_not_found(
        tool: impl Into<String>,
        install_hint: Option<String>,
        alternatives: Vec<String>,
    ) -> Self {
        Self::ToolNotFound {
            tool: tool.into(),
            install_hint,
            alternatives,
        }
    }

    /// Create a tool timeout error.
    pub fn tool_timeout(
        tool: impl Into<String>,
        timeout_secs: u64,
        partial_output: Option<String>,
    ) -> Self {
        Self::ToolTimeout {
            tool: tool.into(),
            timeout_secs,
            partial_output,
        }
    }

    /// Create an out of scope error.
    pub fn out_of_scope(
        target: impl Into<String>,
        reason: impl Into<String>,
        policy_violated: Option<String>,
    ) -> Self {
        Self::OutOfScope {
            target: target.into(),
            reason: reason.into(),
            policy_violated,
        }
    }

    /// Create an authorization error.
    pub fn authorization(
        operation: impl Into<String>,
        reason: impl Into<String>,
        required_permissions: Vec<String>,
    ) -> Self {
        Self::Authorization {
            operation: operation.into(),
            reason: reason.into(),
            required_permissions,
        }
    }

    /// Create a state persistence error.
    pub fn state_persistence(
        operation: impl Into<String>,
        reason: impl Into<String>,
        path: Option<PathBuf>,
    ) -> Self {
        Self::StatePersistence {
            operation: operation.into(),
            reason: reason.into(),
            path,
        }
    }

    /// Create a dependency cycle error.
    pub fn dependency_cycle(tasks: Vec<String>) -> Self {
        Self::DependencyCycle { tasks }
    }

    /// Create a prerequisite not met error.
    pub fn prerequisite_not_met(task: impl Into<String>, prerequisite: impl Into<String>) -> Self {
        Self::PrerequisiteNotMet {
            task: task.into(),
            prerequisite: prerequisite.into(),
        }
    }

    /// Create a resource exhausted error.
    pub fn resource_exhausted(
        resource: impl Into<String>,
        details: impl Into<String>,
        recovery_hint: Option<String>,
    ) -> Self {
        Self::ResourceExhausted {
            resource: resource.into(),
            details: details.into(),
            recovery_hint,
        }
    }

    /// Create a network error.
    pub fn network_error(
        operation: impl Into<String>,
        reason: impl Into<String>,
        is_transient: bool,
    ) -> Self {
        Self::NetworkError {
            operation: operation.into(),
            reason: reason.into(),
            is_transient,
        }
    }

    /// Create a configuration error.
    pub fn configuration(reason: impl Into<String>, config_path: Option<PathBuf>) -> Self {
        Self::Configuration {
            reason: reason.into(),
            config_path,
        }
    }

    /// Create a validation error.
    pub fn validation(
        item: impl Into<String>,
        reason: impl Into<String>,
        expected: Option<String>,
    ) -> Self {
        Self::Validation {
            item: item.into(),
            reason: reason.into(),
            expected,
        }
    }

    /// Create an internal error.
    pub fn internal(context: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Internal {
            context: context.into(),
            reason: reason.into(),
            source: None,
        }
    }

    /// Create an internal error with source.
    pub fn internal_with_source(
        context: impl Into<String>,
        reason: impl Into<String>,
        source: anyhow::Error,
    ) -> Self {
        Self::Internal {
            context: context.into(),
            reason: reason.into(),
            source: Some(source),
        }
    }

    /// Check if this error is transient and a retry might succeed.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::ToolExecution { source, .. } => {
                // Check if the underlying error is transient
                source.as_ref().is_some_and(|e| {
                    let msg = e.to_string().to_lowercase();
                    msg.contains("timeout")
                        || msg.contains("connection reset")
                        || msg.contains("connection refused")
                        || msg.contains("temporarily unavailable")
                        || msg.contains("resource temporarily unavailable")
                        || msg.contains("would block")
                        || msg.contains("broken pipe")
                        || msg.contains("unexpected eof")
                        || msg.contains("dns")
                })
            }
            Self::ToolTimeout { .. } => true,
            Self::NetworkError { is_transient, .. } => *is_transient,
            Self::ResourceExhausted { .. } => true,
            _ => false,
        }
    }

    /// Get a user-friendly recovery hint.
    pub fn recovery_hint(&self) -> Option<String> {
        match self {
            Self::ToolExecution { recovery_hint, .. } => recovery_hint.clone(),
            Self::ToolNotFound {
                install_hint,
                alternatives,
                ..
            } => {
                let mut hints = Vec::new();
                if let Some(hint) = install_hint {
                    hints.push(format!("Install with: {}", hint));
                }
                if !alternatives.is_empty() {
                    hints.push(format!(
                        "Alternatives available: {}",
                        alternatives.join(", ")
                    ));
                }
                if hints.is_empty() {
                    None
                } else {
                    Some(hints.join("; "))
                }
            }
            Self::ToolTimeout { .. } => Some("Increase timeout or reduce input size".to_string()),
            Self::OutOfScope { .. } => Some("Verify target is in scope configuration".to_string()),
            Self::Authorization {
                required_permissions,
                ..
            } => Some(format!(
                "Required permissions: {}",
                required_permissions.join(", ")
            )),
            Self::StatePersistence {
                operation, path, ..
            } => Some(format!(
                "Check {} permissions for {}",
                operation,
                path.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unknown path".to_string())
            )),
            Self::ResourceExhausted { recovery_hint, .. } => recovery_hint.clone(),
            Self::NetworkError { is_transient, .. } => {
                if *is_transient {
                    Some("Retry after a short delay".to_string())
                } else {
                    Some("Check network connectivity and target availability".to_string())
                }
            }
            Self::Configuration { config_path, .. } => Some(format!(
                "Check configuration at {}",
                config_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unknown path".to_string())
            )),
            Self::Validation { expected, .. } => expected.clone(),
            _ => None,
        }
    }

    /// Get the error category for metrics/alerting.
    pub fn category(&self) -> &'static str {
        match self {
            Self::ToolExecution { .. } => "tool_execution",
            Self::ToolNotFound { .. } => "tool_not_found",
            Self::ToolTimeout { .. } => "tool_timeout",
            Self::InvalidToolOutput { .. } => "invalid_output",
            Self::OutOfScope { .. } => "out_of_scope",
            Self::Authorization { .. } => "authorization",
            Self::StatePersistence { .. } => "state_persistence",
            Self::DependencyCycle { .. } => "dependency_cycle",
            Self::PrerequisiteNotMet { .. } => "prerequisite_not_met",
            Self::ResourceExhausted { .. } => "resource_exhausted",
            Self::NetworkError { .. } => "network_error",
            Self::Configuration { .. } => "configuration",
            Self::Validation { .. } => "validation",
            Self::Internal { .. } => "internal",
        }
    }
}

/// Convert from anyhow::Error to BountyError with context.
impl From<anyhow::Error> for BountyError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal {
            context: "anyhow conversion".to_string(),
            reason: err.to_string(),
            source: Some(err),
        }
    }
}

/// Extension trait for adding context to results.
pub trait BountyResultExt<T> {
    /// Add context to an error.
    fn with_context(self, context: &str) -> BountyResult<T>;

    /// Add tool context to an error.
    fn with_tool_context(self, tool: &str) -> BountyResult<T>;
}

impl<T, E> BountyResultExt<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    fn with_context(self, context: &str) -> BountyResult<T> {
        self.map_err(|e| BountyError::internal(context, e.into().to_string()))
    }

    fn with_tool_context(self, tool: &str) -> BountyResult<T> {
        self.map_err(|e| {
            BountyError::tool_execution_with_source(tool, "execution failed", e.into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_execution_error() {
        let err = BountyError::tool_execution("nuclei", "template not found");
        assert!(err.to_string().contains("nuclei"));
        assert!(err.to_string().contains("template not found"));
        assert!(!err.is_transient());
    }

    #[test]
    fn test_tool_not_found_error() {
        let err = BountyError::tool_not_found(
            "subfinder",
            Some("go install ...".to_string()),
            vec!["assetfinder".to_string()],
        );
        assert!(err.to_string().contains("subfinder"));
        assert!(err.recovery_hint().is_some());
    }

    #[test]
    fn test_tool_timeout_error() {
        let err = BountyError::tool_timeout("katana", 60, Some("partial".to_string()));
        assert!(err.is_transient());
        assert!(err.to_string().contains("60s"));
    }

    #[test]
    fn test_out_of_scope_error() {
        let err = BountyError::out_of_scope(
            "evil.com",
            "not in allowlist",
            Some("domain:example.com".to_string()),
        );
        assert!(!err.is_transient());
    }

    #[test]
    fn test_authorization_error() {
        let err = BountyError::authorization("scan", "no API key", vec!["api_key".to_string()]);
        assert!(err.recovery_hint().unwrap().contains("api_key"));
    }

    #[test]
    fn test_transient_detection() {
        let err = BountyError::tool_execution_with_source(
            "httpx",
            "timeout",
            anyhow::anyhow!("connection timeout"),
        );
        assert!(err.is_transient());

        let err = BountyError::tool_execution("nuclei", "invalid template");
        assert!(!err.is_transient());
    }

    #[test]
    fn test_category() {
        assert_eq!(
            BountyError::tool_execution("x", "y").category(),
            "tool_execution"
        );
        assert_eq!(
            BountyError::tool_not_found("x", None, vec![]).category(),
            "tool_not_found"
        );
        assert_eq!(
            BountyError::tool_timeout("x", 10, None).category(),
            "tool_timeout"
        );
        assert_eq!(
            BountyError::out_of_scope("x", "y", None).category(),
            "out_of_scope"
        );
        assert_eq!(
            BountyError::authorization("x", "y", vec![]).category(),
            "authorization"
        );
    }

    #[test]
    fn test_result_ext() {
        let result: Result<i32, std::io::Error> = Err(std::io::Error::other("failed"));
        let bounty_result = result.with_context("test operation");
        assert!(bounty_result.is_err());
        let err = bounty_result.unwrap_err();
        assert!(matches!(err, BountyError::Internal { .. }));
    }
}
