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
//! * `tool_help { "action": "search", "query": "binary analysis" }` — inspect
//!   names, descriptions and families without promoting schemas.
//! * `tool_help { "query": "sql" }` — promote a small matching set. Broad
//!   queries ask the model to choose exact tools instead of loading everything.
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
    action: Option<String>,
    #[serde(default)]
    tools: Option<Vec<String>>,
}

/// Lists and promotes tools that progressive disclosure keeps off the wire.
type LiveTools = tokio::sync::RwLock<std::collections::HashMap<String, std::sync::Arc<dyn Tool>>>;

pub struct ToolHelpTool {
    live: Option<std::sync::Weak<LiveTools>>,
}

impl ToolHelpTool {
    pub fn new() -> Self {
        Self { live: None }
    }

    pub(crate) fn for_registry(registry: &super::Registry) -> Self {
        Self {
            live: Some(std::sync::Arc::downgrade(&registry.tools)),
        }
    }

    async fn catalog(&self) -> Vec<(String, String)> {
        if let Some(live) = &self.live {
            let Some(tools) = live.upgrade() else {
                return Vec::new();
            };
            let tools = tools.read().await;
            let mut catalog: Vec<_> = tools
                .iter()
                .map(|(name, tool)| (name.clone(), tool.description().chars().take(512).collect()))
                .collect();
            catalog.sort_by(|a, b| a.0.cmp(&b.0));
            return catalog;
        }
        // Compatibility for standalone callers; production instances use a live registry.
        super::known_tool_names()
            .into_iter()
            .map(|name| (name, String::new()))
            .collect()
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
        "Discover tools in this session. Use action=search with a task query to match names, \
         descriptions, and capability families without loading schemas. Then pass tools with exact \
         names to enable only what is useful. With no arguments, list hidden tools. Registration \
         does not guarantee an installed binary, authenticated account, or running backend."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string", "enum": ["list", "search", "enable"],
                    "description": "search inspects matching tools without promoting them; enable loads selected schemas. Omitting action preserves automatic promotion for tools/query."
                },
                "tools": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Exact tool names to enable, e.g. [\"sqlmap\",\"nuclei\"]."
                },
                "query": {
                    "type": "string",
                    "description": "Search names, descriptions, or capability families. Use action=search to inspect matches without enabling them."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: ToolHelpInput = serde_json::from_value(input)?;
        let action = params.action.as_deref().unwrap_or("auto");
        anyhow::ensure!(
            matches!(action, "auto" | "list" | "search" | "enable"),
            "unknown tool_help action; use list, search, or enable"
        );

        // Everything the registry knows about, minus the always-advertised set.
        let session_policy = super::session_tool_policy(&ctx.session_id);
        let catalog: Vec<_> = self
            .catalog()
            .await
            .into_iter()
            .filter(|(name, _)| {
                session_policy.as_ref().is_none_or(|policy| {
                    !policy.disabled_tools.contains(name)
                        && policy
                            .allowed_tools
                            .as_ref()
                            .is_none_or(|allowed| allowed.contains(name))
                })
            })
            .collect();
        let all: Vec<String> = catalog.iter().map(|(name, _)| name.clone()).collect();
        {
            let mut sessions = SESSION_PROMOTED
                .write()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(promoted) = sessions.get_mut(&ctx.session_id) {
                promoted.retain(|name| all.binary_search(name).is_ok());
            }
        }

        if action == "search" {
            let query = params.query.as_deref().unwrap_or_default().trim();
            anyhow::ensure!(
                !query.is_empty(),
                "tool_help search requires a non-empty query"
            );
            let matches: Vec<_> = catalog
                .iter()
                .filter(|(name, description)| {
                    super::capabilities::matches_query(name, description, query)
                })
                .cloned()
                .collect();
            return Ok(ToolOutput::new(format_catalog(
                &matches,
                "Search results (not enabled)",
            )));
        }

        let policy = crate::tool::disclosure::DisclosurePolicy::parse(
            &crate::config::config().tools.disclosure,
        );
        // A hidden tool is one this session has not promoted and that the
        // policy does not advertise by default.
        let already: std::collections::HashSet<String> = promoted_tools(&ctx.session_id);
        let forced = session_policy
            .as_ref()
            .and_then(|policy| policy.allowed_tools.clone())
            .unwrap_or_default();
        let selection = crate::tool::disclosure::DisclosureSelection::new();
        let hidden: Vec<String> = all
            .iter()
            .filter(|name| !selection.is_visible(name, policy, &forced, &already))
            .cloned()
            .collect();

        // --- Promotion path ---
        if action == "list" {
            let entries: Vec<_> = catalog
                .iter()
                .filter(|(name, _)| hidden.contains(name))
                .cloned()
                .collect();
            return Ok(ToolOutput::new(format_catalog(&entries, "Hidden tools")));
        }
        let mut requested: Vec<String> = params
            .tools
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|name| super::Registry::resolve_tool_name(&name).to_string())
            .collect();
        if let Some(query) = params.query.as_deref() {
            let needle = query.trim().to_ascii_lowercase();
            if !needle.is_empty() {
                requested.extend(
                    catalog
                        .iter()
                        .filter(|(name, description)| {
                            super::capabilities::matches_query(name, description, &needle)
                        })
                        .map(|(name, _)| name.clone()),
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
                .filter(|name| {
                    !known.contains(&super::Registry::resolve_tool_name(name).to_string())
                })
                .collect();

            if valid.is_empty() {
                let mut msg = String::from("No matching tools to enable.");
                if !unknown.is_empty() {
                    msg.push_str(&format!(" Unknown: {}.", unknown.join(", ")));
                }
                if !hidden.is_empty() {
                    let entries: Vec<_> = catalog
                        .iter()
                        .filter(|(name, _)| hidden.contains(name))
                        .cloned()
                        .collect();
                    msg.push_str(&format!("\n\n{}", format_catalog(&entries, "Hidden tools")));
                }
                return Ok(ToolOutput::new(msg));
            }

            let valid: Vec<_> = valid
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if valid.len() > 8 && params.tools.as_ref().is_none_or(Vec::is_empty) {
                let entries: Vec<_> = catalog
                    .iter()
                    .filter(|(name, _)| valid.contains(name))
                    .cloned()
                    .collect();
                return Ok(ToolOutput::new(format!(
                    "Broad query matched {} tools. No schemas loaded; choose exact tools to enable.\n{}",
                    valid.len(),
                    format_catalog(&entries, "Matches")
                )));
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
        anyhow::ensure!(
            action != "enable",
            "tool_help enable requires tools or a query"
        );
        let entries: Vec<_> = catalog
            .into_iter()
            .filter(|(name, _)| hidden.contains(name))
            .collect();
        Ok(ToolOutput::new(format_catalog(
            &entries,
            "Tools available but not advertised",
        )))
    }
}

fn format_catalog(entries: &[(String, String)], heading: &str) -> String {
    let mut output = format!(
        "{heading}: {} tool(s). Enable exact names with tool_help tools=[...].\n",
        entries.len()
    );
    let mut shown = 0;
    for (name, description) in entries.iter().take(40) {
        let hint: String = description
            .split_whitespace()
            .flat_map(|word| word.chars().chain(std::iter::once(' ')))
            .take(120)
            .collect();
        let line = format!(
            "- {} [{}]: {}\n",
            serde_json::to_string(name).unwrap_or_default(),
            super::capabilities::family(name),
            serde_json::to_string(&hint).unwrap_or_default()
        );
        if output.len() + line.len() > 8000 {
            break;
        }
        output.push_str(&line);
        shown += 1;
    }
    if shown < entries.len() {
        output.push_str("More matches omitted; narrow the query.\n");
    }
    output
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
    async fn live_catalog_tracks_registration_removal_and_registry_isolation() {
        let first = super::super::Registry::empty();
        let second = super::super::Registry::empty();
        let help = ToolHelpTool::for_registry(&first);
        let context = ctx("th-live", &[]);
        second
            .register(
                "mcp__other__private_tool".into(),
                std::sync::Arc::new(ToolHelpTool::new()),
            )
            .await;
        first
            .register("ghidra".into(), std::sync::Arc::new(ToolHelpTool::new()))
            .await;
        let output = help
            .execute(
                json!({"action":"search", "query":"binary analysis"}),
                context.clone(),
            )
            .await
            .unwrap();
        assert!(output.output.contains("ghidra"));
        assert!(!output.output.contains("private_tool"));
        assert!(promoted_tools("th-live").is_empty());
        help.execute(json!({"tools":["ghidra","ghidra"]}), context.clone())
            .await
            .unwrap();
        assert_eq!(promoted_tools("th-live").len(), 1);
        first.unregister("ghidra").await;
        let output = help
            .execute(json!({"tools":["ghidra"]}), context)
            .await
            .unwrap();
        assert!(output.output.contains("No matching tools"));
        assert!(!promoted_tools("th-live").contains("ghidra"));
        clear_session("th-live");
    }

    #[tokio::test]
    async fn invalid_requests_do_not_silently_list_or_promote_tools() {
        let tool = ToolHelpTool::new();
        let context = ctx("th-invalid", &["ghidra"]);
        assert!(
            tool.execute(json!({"tools":12}), context.clone())
                .await
                .is_err()
        );
        assert!(
            tool.execute(json!({"action":"search"}), context)
                .await
                .is_err()
        );
        assert!(promoted_tools("th-invalid").is_empty());
    }

    #[tokio::test]
    async fn search_respects_policy_and_alias_promotion_is_canonical() {
        let registry = super::super::Registry::empty();
        registry
            .register("ghidra".into(), std::sync::Arc::new(ToolHelpTool::new()))
            .await;
        registry
            .register("bash".into(), std::sync::Arc::new(ToolHelpTool::new()))
            .await;
        let help = ToolHelpTool::for_registry(&registry);
        let context = ctx("th-policy-search", &[]);
        super::super::set_session_tool_policy(
            "th-policy-search",
            Some(["bash".into()].into_iter().collect()),
            std::collections::HashSet::new(),
        );
        let output = help
            .execute(
                json!({"action":"search","query":"binary analysis"}),
                context.clone(),
            )
            .await
            .unwrap();
        assert!(!output.output.contains("ghidra"));
        let output = help
            .execute(json!({"tools":["shell_exec"]}), context)
            .await
            .unwrap();
        assert!(promoted_tools("th-policy-search").contains("bash"));
        assert!(!output.output.contains("Unknown"));
        super::super::clear_session_tool_policy("th-policy-search");
    }

    #[test]
    fn catalog_output_is_bounded_for_large_names_and_descriptions() {
        let entries = vec![("mcp__large".into(), "x".repeat(10_000)); 100];
        assert!(format_catalog(&entries, "Matches").len() < 8100);
    }

    #[tokio::test]
    async fn broad_queries_inspect_instead_of_promoting_every_schema() {
        let registry = super::super::Registry::empty();
        for index in 0..12 {
            registry
                .register(
                    format!("mcp__sample__{index}"),
                    std::sync::Arc::new(ToolHelpTool::new()),
                )
                .await;
        }
        let help = ToolHelpTool::for_registry(&registry);
        let output = help
            .execute(json!({"query":"integration"}), ctx("th-broad", &[]))
            .await
            .unwrap();
        assert!(output.output.contains("No schemas loaded"));
        assert!(promoted_tools("th-broad").is_empty());
    }

    #[tokio::test]
    async fn lists_hidden_tools() {
        clear_session("th-list");
        let registry = super::super::Registry::empty();
        for name in ["read", "bash", "sqlmap", "volatility"] {
            registry
                .register(name.into(), std::sync::Arc::new(ToolHelpTool::new()))
                .await;
        }
        let tool = ToolHelpTool::for_registry(&registry);
        let out = tool
            .execute(
                json!({}),
                ctx("th-list", &["read", "bash", "sqlmap", "volatility"]),
            )
            .await
            .expect("execute");
        assert!(out.output.contains("sqlmap"), "{}", out.output);
        assert!(out.output.contains("volatility"), "{}", out.output);
        assert!(!out.output.contains("- \"read\" ["), "{}", out.output);
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
