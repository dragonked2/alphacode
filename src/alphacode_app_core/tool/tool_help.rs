//! `tool_help` — the promotion surface for progressive tool disclosure.
//!
//! Progressive disclosure advertises only the core + standard tool surface by
//! default (see [`super::disclosure`]). That keeps the static prompt prefix
//! small, but the model still needs a way to bring a specialized tool into the
//! advertised set when a task genuinely calls for it.
//!
//! `tool_help` is that path:
//!
//! * `tool_help {}` — list every tool that exists but is not currently
//!   advertised, grouped so the model can pick a family.
//! * `tool_help { "tools": ["sqlmap", "nuclei"] }` — promote those tools. From
//!   the next request on, their full schemas are advertised and the model can
//!   call them normally.
//! * `tool_help { "query": "sql" }` — promote every tool whose name matches.
//!
//! Promotion is sticky for the session. It is also cheap: the tool list snapshot
//! is unlocked once, which costs a single intentional prompt-cache miss — the
//! same trade the MCP late-registration path already makes.

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Tool, ToolContext, ToolOutput};

/// Session-scoped promoted tool names.
///
/// This deliberately mirrors the `SESSION_TOOL_POLICIES` pattern: a plain
/// process-global map keyed by session id, so tools can record state without
/// holding an `Agent` handle, and so promotion survives across the request
/// boundary that rebuilds definitions.
static SESSION_PROMOTED: std::sync::LazyLock<
    std::sync::RwLock<std::collections::HashMap<String, std::collections::HashSet<String>>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashMap::new()));

/// Promote `names` for `session_id`. Returns how many were newly promoted.
pub fn promote_tools<I, S>(session_id: &str, names: I) -> usize
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut guard = SESSION_PROMOTED
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = guard.entry(session_id.to_string()).or_default();
    names
        .into_iter()
        .filter(|name| entry.insert(name.as_ref().to_string()))
        .count()
}

/// Promoted tool names for a session.
pub fn promoted_tools(session_id: &str) -> std::collections::HashSet<String> {
    SESSION_PROMOTED
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(session_id)
        .cloned()
        .unwrap_or_default()
}

/// Drop promotion state for a session (called on teardown).
pub fn clear_session(session_id: &str) {
    SESSION_PROMOTED
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(session_id);
}

#[derive(Debug, Deserialize, Default)]
struct ToolHelpInput {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    tools: Option<Vec<String>>,
}

/// Lists and promotes tools that progressive disclosure keeps off the wire.
pub struct ToolHelpTool;

impl ToolHelpTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolHelpTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ToolHelpTool {
    fn name(&self) -> &str {
        "tool_help"
    }

    fn description(&self) -> &str {
        "Find and enable specialized tools. Only a core set of tool schemas is loaded \
         each turn to keep requests small; every other tool still exists and runs \
         once enabled. Call with no arguments to list the hidden tools, or pass \
         `tools` (exact names) / `query` (substring) to enable them — they are \
         advertised in the next model request."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "tools": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Exact tool names to enable, e.g. [\"sqlmap\",\"nuclei\"]."
                },
                "query": {
                    "type": "string",
                    "description": "Substring of tool names, e.g. \"sql\" or \"ffuf\". Enables every match."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: ToolHelpInput = serde_json::from_value(input).unwrap_or_default();

        // Everything the registry knows about, minus the always-advertised set.
        let session_policy = super::session_tool_policy(&ctx.session_id);
        let all: Vec<String> = super::known_tool_names()
            .into_iter()
            .filter(|name| {
                session_policy.as_ref().is_none_or(|policy| {
                    !policy.disabled_tools.contains(name)
                        && policy
                            .allowed_tools
                            .as_ref()
                            .is_none_or(|allowed| allowed.contains(name))
                })
            })
            .collect();

        let policy = crate::tool::disclosure::DisclosurePolicy::parse(
            &crate::config::config().tools.disclosure,
        );
        // A hidden tool is one this session has not promoted and that the
        // policy does not advertise by default.
        let already: std::collections::HashSet<String> = promoted_tools(&ctx.session_id);
        let forced = std::collections::HashSet::new();
        let selection = crate::tool::disclosure::DisclosureSelection::new();
        let hidden: Vec<String> = all
            .iter()
            .filter(|name| !selection.is_visible(name, policy, &forced, &already))
            .cloned()
            .collect();

        // --- Promotion path ---
        let mut requested: Vec<String> = params.tools.clone().unwrap_or_default();
        if let Some(query) = params.query.as_deref() {
            let needle = query.trim().to_ascii_lowercase();
            if !needle.is_empty() {
                requested.extend(
                    all.iter()
                        .filter(|name| name.to_ascii_lowercase().contains(&needle))
                        .cloned(),
                );
            }
        }
        if !requested.is_empty()
            || params
                .query
                .as_deref()
                .is_some_and(|q| !q.trim().is_empty())
        {
            // Only promote names that actually exist, so a typo cannot silently
            // create a phantom entry.
            let known: std::collections::HashSet<&String> = all.iter().collect();
            let valid: Vec<String> = requested
                .into_iter()
                .filter(|name| known.contains(name))
                .collect();
            let unknown: Vec<String> = params
                .tools
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter(|name| !known.contains(name))
                .collect();

            if valid.is_empty() {
                let mut msg = String::from("No matching tools to enable.");
                if !unknown.is_empty() {
                    msg.push_str(&format!(" Unknown: {}.", unknown.join(", ")));
                }
                if !hidden.is_empty() {
                    msg.push_str(&format!("\n\nHidden tools: {}", hidden.join(", ")));
                }
                return Ok(ToolOutput::new(msg));
            }

            let newly = promote_tools(&ctx.session_id, &valid);
            let mut msg = format!(
                "Enabled {} tool(s): {}.\nTheir schemas are advertised in the next model request; \
                 call them normally.",
                valid.len(),
                valid.join(", ")
            );
            if newly > 0 {
                msg.push_str(&format!("\n({newly} were newly enabled.)"));
            }
            if !unknown.is_empty() {
                msg.push_str(&format!("\nUnknown (ignored): {}.", unknown.join(", ")));
            }
            return Ok(ToolOutput::new(msg));
        }

        // --- Listing path ---
        if hidden.is_empty() {
            return Ok(ToolOutput::new(
                "Every available tool is already advertised. Nothing is hidden.".to_string(),
            ));
        }
        let mut msg = format!(
            "{} tool(s) are available but not advertised, to keep requests small. \
             Enable with tool_help {{\"tools\": [...]}} or {{\"query\": \"...\"}}:\n",
            hidden.len()
        );
        for name in &hidden {
            msg.push_str(&format!("- {name}\n"));
        }
        Ok(ToolOutput::new(msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ctx(session: &str, tools: &[&str]) -> ToolContext {
        // Seed the process-wide manifest the way `Registry::new` does.
        super::super::publish_tool_manifest(tools);
        ToolContext {
            session_id: session.to_string(),
            message_id: String::new(),
            tool_call_id: String::new(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: crate::tool::ToolExecutionMode::Direct,
        }
    }

    #[tokio::test]
    async fn lists_hidden_tools() {
        clear_session("th-list");
        let tool = ToolHelpTool::new();
        let out = tool
            .execute(
                json!({}),
                ctx("th-list", &["read", "bash", "sqlmap", "volatility"]),
            )
            .await
            .expect("execute");
        assert!(out.output.contains("sqlmap"), "{}", out.output);
        assert!(out.output.contains("volatility"), "{}", out.output);
        assert!(!out.output.contains("- read"), "{}", out.output);
    }

    #[tokio::test]
    async fn promotes_exact_names() {
        clear_session("th-promote");
        let tool = ToolHelpTool::new();
        tool.execute(
            json!({"tools": ["sqlmap"]}),
            ctx("th-promote", &["read", "sqlmap"]),
        )
        .await
        .expect("execute");
        let promoted = promoted_tools("th-promote");
        assert!(promoted.contains("sqlmap"));
        clear_session("th-promote");
    }

    #[tokio::test]
    async fn rejects_unknown_names() {
        clear_session("th-unknown");
        let tool = ToolHelpTool::new();
        let out = tool
            .execute(
                json!({"tools": ["not_a_real_tool"]}),
                ctx("th-unknown", &["read", "sqlmap"]),
            )
            .await
            .expect("execute");
        assert!(out.output.contains("No matching tools"), "{}", out.output);
        assert!(promoted_tools("th-unknown").is_empty());
        clear_session("th-unknown");
    }

    #[tokio::test]
    async fn query_promotes_matches() {
        clear_session("th-query");
        let tool = ToolHelpTool::new();
        tool.execute(
            json!({"query": "sql"}),
            ctx("th-query", &["read", "sqlmap", "bash"]),
        )
        .await
        .expect("execute");
        assert!(promoted_tools("th-query").contains("sqlmap"));
        clear_session("th-query");
    }

    #[tokio::test]
    async fn unmatched_query_reports_no_match() {
        clear_session("th-no-match");
        let out = ToolHelpTool::new()
            .execute(
                json!({"query": "no_such_tool_name"}),
                ctx("th-no-match", &["read"]),
            )
            .await
            .expect("execute");
        assert!(out.output.contains("No matching tools"));
        assert!(promoted_tools("th-no-match").is_empty());
        clear_session("th-no-match");
    }

    #[tokio::test]
    async fn disabled_tools_cannot_be_enabled_through_discovery() {
        let session = "th-disabled";
        super::super::set_session_tool_policy(
            session,
            None,
            ["sqlmap".to_string()].into_iter().collect(),
        );
        let out = ToolHelpTool::new()
            .execute(json!({"tools": ["sqlmap"]}), ctx(session, &["sqlmap"]))
            .await
            .expect("execute");
        assert!(out.output.contains("No matching tools"));
        assert!(promoted_tools(session).is_empty());
        super::super::clear_session_tool_policy(session);
    }
}
