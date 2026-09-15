//! Structured error types for desktop control operations.
//!
//! These errors are designed to be informative for the LLM agent, providing
//! enough context to recover from failures without exposing internal details.

use thiserror::Error;

/// Desktop control error types.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum DesktopError {
    #[error("ElementNotFound: {message}")]
    ElementNotFound { message: String },

    #[error("AmbiguousElement: {count} elements matched. {hint}")]
    AmbiguousElement { count: usize, hint: String },

    #[error("StaleElement: {message}")]
    StaleElement { message: String },

    #[error("PermissionDenied: {instructions}")]
    PermissionDenied { instructions: String },

    #[error("UnsupportedPlatform: {feature} is not available on this platform")]
    UnsupportedPlatform { feature: String },

    #[error("UnsupportedAction: {action} is not supported on {role}")]
    UnsupportedAction { action: String, role: String },

    #[error("ApplicationNotFound: {name}")]
    ApplicationNotFound { name: String },

    #[error("WindowNotFound: {message}")]
    WindowNotFound { message: String },

    #[error("InputBlocked: {reason}")]
    InputBlocked { reason: String },

    #[error("Timeout: operation exceeded {secs:.1}s limit")]
    Timeout { secs: f64 },

    #[error("Cancelled: {reason}")]
    Cancelled { reason: String },

    #[error("BackendUnavailable: {reason}")]
    BackendUnavailable { reason: String },

    #[error("EmergencyStop: desktop control operations are stopped")]
    EmergencyStop,
}

impl DesktopError {
    /// Convert to a string suitable for LLM consumption.
    #[allow(dead_code)]
    pub fn to_llm_message(&self) -> String {
        match self {
            Self::ElementNotFound { message } => {
                format!(
                    "{message}\nHint: Run desktop_snapshot or desktop_find with a broader selector."
                )
            }
            Self::AmbiguousElement { count, hint } => {
                format!("Found {count} matching elements. {hint}")
            }
            Self::StaleElement { message } => {
                format!("{message}\nHint: Re-run desktop_find to get a fresh element reference.")
            }
            Self::PermissionDenied { instructions } => {
                format!("Permission denied. {instructions}")
            }
            Self::ApplicationNotFound { name } => {
                format!(
                    "Application '{name}' not found. Use desktop_list_windows to see available applications."
                )
            }
            Self::Timeout { secs } => {
                format!(
                    "Operation timed out after {secs:.1}s. Try a simpler operation or increase timeout_ms."
                )
            }
            other => other.to_string(),
        }
    }
}
