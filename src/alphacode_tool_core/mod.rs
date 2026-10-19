use crate::alphacode_agent_runtime::InterruptSignal;
use crate::alphacode_message_types::ToolDefinition;
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const TOOL_INTENT_DESCRIPTION: &str = "Optional short label shown in the UI: why this call is being made. It never substitutes for the tool's required arguments.";

pub fn intent_schema_property() -> Value {
    serde_json::json!({
        "type": "string",
        "description": TOOL_INTENT_DESCRIPTION,
    })
}

/// Ensure a tool parameter schema declares the shared `intent` property.
/// The property is optional (not added to `required`) because every tool
/// deserializes it as `Option<String>` with `#[serde(default)]`. Marking it
/// required in the schema confuses models into sending malformed calls
/// (e.g. empty objects or missing required fields like `command`/`file_path`).
pub fn ensure_intent_in_schema(mut schema: Value) -> Value {
    let Some(object) = schema.as_object_mut() else {
        return schema;
    };
    // Only touch object-shaped parameter schemas.
    let is_object_schema = object
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "object")
        .unwrap_or_else(|| object.contains_key("properties"));
    if !is_object_schema {
        return schema;
    }

    let properties = object
        .entry("properties")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(properties) = properties.as_object_mut() {
        properties
            .entry("intent")
            .or_insert_with(intent_schema_property);
    }

    schema
}

/// A request for stdin input from a running command.
pub struct StdinInputRequest {
    pub request_id: String,
    pub prompt: String,
    pub is_password: bool,
    pub response_tx: tokio::sync::oneshot::Sender<String>,
}

#[derive(Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub message_id: String,
    pub tool_call_id: String,
    pub working_dir: Option<PathBuf>,
    pub stdin_request_tx: Option<tokio::sync::mpsc::UnboundedSender<StdinInputRequest>>,
    pub graceful_shutdown_signal: Option<InterruptSignal>,
    pub execution_mode: ToolExecutionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    AgentTurn,
    Direct,
}

/// Scheduler-visible side-effect class for a tool.
///
/// The default is deliberately conservative. Tools only opt into
/// [`ReadOnly`](Self::ReadOnly) after verifying that concurrent calls do not
/// mutate shared state or produce external side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionClass {
    /// Safe to run concurrently when the scheduler has an available slot.
    ReadOnly,
    /// Changes local state; mutating calls remain ordered by default.
    Mutating,
    /// Requires exclusive access to a session-scoped resource.
    Exclusive,
    /// Starts work and returns a job handle.
    Background,
    /// Changes a remote system or otherwise consumes an external side effect.
    ExternalEffect,
}

impl ToolContext {
    pub fn for_subcall(&self, tool_call_id: String) -> Self {
        Self {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id,
            working_dir: self.working_dir.clone(),
            stdin_request_tx: self.stdin_request_tx.clone(),
            graceful_shutdown_signal: self.graceful_shutdown_signal.clone(),
            execution_mode: self.execution_mode,
        }
    }

    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ref base) = self.working_dir {
            base.join(path)
        } else {
            path.to_path_buf()
        }
    }

    /// Resolve `path` and prove it stays inside a protected location.
    ///
    /// [`Self::resolve_path`] is a *purely lexical* join: it does not collapse
    /// `..`, does not resolve symlinks, and does not check containment. Handing
    /// its output straight to `fs::write` meant
    /// `write("..\\..\\..\\..\\.ssh\\authorized_keys")` reached
    /// `C:\Users\<user>\.ssh\authorized_keys`. Only the two patch tools consulted
    /// `is_catastrophic_target`; `write` / `edit` / `multiedit` / `read` / `ls`
    /// had no path check at all, and the sole mitigation was prose in the tool
    /// description asking the model to behave.
    ///
    /// The check here is mechanical, and there are two independent parts
    /// because either alone is insufficient:
    ///
    /// 1. **Catastrophic targets are refused outright** — the same policy the
    ///    patch tools already apply, now applied consistently.
    /// 2. **Symlinks are resolved before the check.** Canonicalizing only the
    ///    deepest *existing* ancestor and re-appending the not-yet-created tail
    ///    means a symlink inside the working directory that points at
    ///    `~/.ssh` or `C:\Windows` is caught; a purely lexical check cannot see
    ///    it. This is why canonicalization cannot simply be skipped for paths
    ///    that do not exist yet, which is the normal case for `write`.
    pub fn resolve_path_guarded(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let joined = self.resolve_path(path);

        // Resolve the deepest existing ancestor so a symlinked parent is
        // followed, then re-attach the segments that do not exist yet.
        let mut existing: &Path = joined.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while !existing.exists() {
            match (existing.file_name(), existing.parent()) {
                (Some(name), Some(parent)) => {
                    tail.push(name.to_os_string());
                    existing = parent;
                }
                // Reached the filesystem root without finding anything that
                // exists; use what we have.
                _ => break,
            }
        }
        let real = existing
            .canonicalize()
            .unwrap_or_else(|_| existing.to_path_buf());
        let mut resolved = real;
        for segment in tail.into_iter().rev() {
            resolved.push(segment);
        }

        let risk_ctx =
            crate::alphacode_command_risk::RiskContext::from_env(self.working_dir.clone());
        if crate::alphacode_command_risk::is_catastrophic_target(&resolved, &risk_ctx) {
            anyhow::bail!(
                "Refusing to touch {:?}: it resolves to {}, which is a protected \
                 system, credential, or home path. Writing outside the project \
                 is not permitted.",
                path.display(),
                resolved.display()
            );
        }
        Ok(resolved)
    }
}

/// A tool that can be executed by the agent.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (must match what's sent to the API).
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema for the input parameters.
    fn parameters_schema(&self) -> Value;

    /// Scheduler-visible side-effect class for this specific input. Unknown and
    /// third-party tools stay serialized unless they explicitly opt into a
    /// different contract.
    fn execution_class(&self, _input: &Value) -> ToolExecutionClass {
        ToolExecutionClass::Mutating
    }

    /// Execute the tool with the given input.
    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput>;

    /// Convert to API tool definition.
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: ensure_intent_in_schema(self.parameters_schema()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_intent_adds_property_without_required() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string"}
            }
        });
        let out = ensure_intent_in_schema(schema);
        assert!(out["properties"]["intent"].is_object());
        // intent must NOT be added to required — it is Option<String> at runtime
        let required: Vec<_> = out["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"command"));
        assert!(!required.contains(&"intent"));
    }

    #[test]
    fn ensure_intent_does_not_create_required_array() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        let out = ensure_intent_in_schema(schema);
        // No required array should be created just for intent
        assert!(out.get("required").is_none());
    }

    #[test]
    fn ensure_intent_preserves_existing_intent_property() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["intent"],
            "properties": {
                "intent": {"type": "string", "description": "custom"}
            }
        });
        let out = ensure_intent_in_schema(schema);
        assert_eq!(out["properties"]["intent"]["description"], "custom");
        // If a tool already has intent in required, we don't duplicate it
        assert_eq!(
            out["required"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.as_str() == Some("intent"))
                .count(),
            1
        );
    }

    #[test]
    fn ensure_intent_skips_non_object_schemas() {
        let schema = serde_json::json!({"type": "string"});
        let out = ensure_intent_in_schema(schema.clone());
        assert_eq!(out, schema);
    }
}
