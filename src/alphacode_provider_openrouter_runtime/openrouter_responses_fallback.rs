//! Fallback from `/chat/completions` to the OpenAI **Responses** API
//! (`/v1/responses`) for OpenAI-compatible gateways whose models are
//! Responses-only.
//!
//! AgentRouter (and other `new-api` deployments) reject their GPT
//! `*-sol`/`*-astra` models on `/v1/chat/completions` with HTTP 400:
//!
//! ```text
//! {"error":{"message":"模型 gpt-5.6-sol 不支持 /v1/chat/completions 接口，请改用
//! /v1/responses（OpenAI Responses API）。Model gpt-5.6-sol does not support
//! /v1/chat/completions, please use /v1/responses instead.","type":"new_api_error"}}
//! ```
//!
//! The gateway routes those models to upstream servers that only speak the
//! Responses wire protocol. When that exact rejection arrives the runtime
//! rebuilds the same turn as a Responses request — reusing the shared builders
//! the native OpenAI runtime uses ([`build_responses_input`], [`build_tools`])
//! — POSTs it to `{base}/responses`, and remembers the model so later turns
//! skip the doomed chat attempt instead of paying one wasted round trip each.

use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use crate::alphacode_message_types::{Message, ToolDefinition};

/// The original turn inputs, kept alongside the chat-completions body so the
/// fallback can rebuild the request on the Responses wire without trying to
/// reverse-convert the (already transformed) chat JSON.
#[derive(Debug, Clone)]
pub(crate) struct ResponsesFallbackContext {
    /// The effective message list `complete()` was called with (post
    /// cache-breakpoint annotation — `cache_control` never reaches the wire).
    pub messages: Vec<Message>,
    /// The merged system prompt (`instructions` in the Responses payload).
    pub system: String,
    /// Tool definitions as declared by the agent, converted to the flat
    /// Responses tool shape by [`build_tools`].
    pub tools: Vec<ToolDefinition>,
}

/// Does this HTTP error response body mean "this model needs `/v1/responses"?
///
/// Marker-based rather than host-based so other `new-api`-powered OpenAI
/// compatible gateways get the same repair for free. The rejection body names
/// both endpoint paths (the gateway prints the English sentence even when the
/// surrounding message is Chinese), and 400 is the only status the
/// `new-api` router emits for this condition.
pub(crate) fn body_reports_chat_completions_unsupported(
    status: reqwest::StatusCode,
    body: &str,
) -> bool {
    if status.as_u16() != 400 {
        return false;
    }
    let lower = body.to_ascii_lowercase();
    lower.contains("/v1/chat/completions")
        && lower.contains("/v1/responses")
        && (lower.contains("does not support") || lower.contains("不支持"))
}

/// The static (endpoint, model) → "Requires the Responses API" set.
///
/// The provider runtime instance lives across turns, but the stream helpers are
/// free functions called from spawned tasks, so the memo must be process-global.
/// Keyed by endpoint too: a model name can mean different things on different
/// gateways.
static RESPONSES_ONLY_ROUTES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn responses_only_routes() -> &'static Mutex<HashSet<String>> {
    RESPONSES_ONLY_ROUTES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn responses_only_key(api_base: &str, model: &str) -> String {
    format!(
        "{}|{}",
        api_base.trim_end_matches('/').to_ascii_lowercase(),
        model.trim().to_ascii_lowercase()
    )
}

/// Whether this (endpoint, model) pairing should be sent to `/v1/responses`
/// directly, because the gateway already rejected `/chat/completions` for it.
pub(crate) fn responses_required(api_base: &str, model: &str) -> bool {
    responses_only_routes()
        .lock()
        .map(|routes| routes.contains(&responses_only_key(api_base, model)))
        .unwrap_or(false)
}

/// Record that this (endpoint, model) pairing must use the Responses API.
fn remember_responses_required(api_base: &str, model: &str) {
    if let Ok(mut routes) = responses_only_routes().lock() {
        routes.insert(responses_only_key(api_base, model));
    }
}

/// Build a Responses request from the original turn inputs when the chat
/// attempt failed with the "use `/v1/responses`" rejection.
///
/// Returns `None` (and falls through to the ordinary error path) when either
/// the rejection did not match, or the fallback context cannot produce a
/// Responses body (missing model id).
pub(crate) fn fallback_request_from_chat_rejection(
    status: reqwest::StatusCode,
    body: &str,
    api_base: &str,
    model: &str,
    fallback: &ResponsesFallbackContext,
    chat_request: &Value,
) -> Option<Value> {
    if !body_reports_chat_completions_unsupported(status, body) {
        return None;
    }
    let built = build_responses_request(fallback, chat_request)?;
    remember_responses_required(api_base, model);
    crate::alphacode_base::logging::info(&format!(
        "Gateway rejects /v1/chat/completions for model '{}' (Responses-only); retrying the same turn via {}",
        model,
        crate::alphacode_provider_metadata::openai_compat_request_url(api_base, "responses")
            .unwrap_or_else(|| format!("{}/responses", api_base.trim_end_matches('/'))),
    ));
    Some(built)
}

/// Convert the chat-completions body into an OpenAI Responses API body.
///
/// Mirrors the native OpenAI runtime's `build_response_request` payload shape
/// (minus ChatGPT-backend-only extras): flat `input` items and flat function
/// tool declarations, `store: false`, reasoning effort carried over from the
/// chat body when present.
pub(crate) fn build_responses_request(
    fallback: &ResponsesFallbackContext,
    chat_request: &Value,
) -> Option<Value> {
    let model = chat_request.get("model")?.as_str()?.to_string();

    let input =
        crate::alphacode_base::provider::openai_request::build_responses_input(&fallback.messages);
    let api_tools = crate::alphacode_base::provider::openai_request::build_tools(&fallback.tools);

    let mut request = serde_json::json!({
        "model": model,
        "instructions": fallback.system,
        "input": input,
        "tools": api_tools,
        "tool_choice": "auto",
        "parallel_tool_calls": false,
        "stream": true,
        "store": false,
        // Replay reasoning state across turns the same way the native OpenAI
        // runtime does; `build_responses_input` emits reasoning items with
        // `encrypted_content` when the conversation carries them.
        "include": ["reasoning.encrypted_content"],
    });

    // Carry the reasoning effort the chat request was built with
    // (`reasoning_effort` on direct-OpenAI-style gateways, or the unified
    // `reasoning.effort` object) into the Responses `reasoning` field.
    let effort = chat_request
        .get("reasoning_effort")
        .and_then(Value::as_str)
        .or_else(|| {
            chat_request
                .get("reasoning")
                .and_then(|reasoning| reasoning.get("effort"))
                .and_then(Value::as_str)
        });
    if let Some(effort) = effort {
        // `summary` is mandatory on the Responses stream: without it the model
        // can stay silent for minutes while thinking and trip the idle timeout.
        request["reasoning"] = serde_json::json!({ "effort": effort, "summary": "auto" });
    }

    // The chat body's `max_tokens` cap maps to Responses `max_output_tokens`.
    if let Some(max_tokens) = chat_request.get("max_tokens") {
        request["max_output_tokens"] = max_tokens.clone();
    }

    Some(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_message_types::{ContentBlock, Role};

    const AGENTROUTER_400_BODY: &str = "{\"error\":{\"message\":\"模型 gpt-5.6-sol 不支持 /v1/chat/completions 接口，请改用 /v1/responses（OpenAI Responses API）。Model gpt-5.6-sol does not support /v1/chat/completions, please use /v1/responses instead. (request id: 20261023125334755361499fq6gxUt3WJvtO)\",\"type\":\"new_api_error\"}}";

    fn ctx(messages: Vec<Message>) -> ResponsesFallbackContext {
        ResponsesFallbackContext {
            messages,
            system: "You are a helpful assistant.".to_string(),
            tools: vec![ToolDefinition {
                name: "ls".to_string(),
                description: "list files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                }),
            }],
        }
    }

    fn chat_request(effort: Option<&str>) -> Value {
        let mut request = serde_json::json!({
            "model": "gpt-5.6-sol",
            "messages": [
                { "role": "system", "content": "system text" },
                { "role": "user", "content": [{ "type": "text", "text": "hi" }] }
            ],
            "stream": true,
            "tools": [],
        });
        if let Some(effort) = effort {
            request["reasoning_effort"] = serde_json::json!(effort);
        }
        request
    }

    #[test]
    fn agentrouter_rejection_body_is_recognized() {
        assert!(body_reports_chat_completions_unsupported(
            reqwest::StatusCode::BAD_REQUEST,
            AGENTROUTER_400_BODY
        ));
        // Status is required to be 400: other rejections are not this bug.
        assert!(!body_reports_chat_completions_unsupported(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            AGENTROUTER_400_BODY
        ));
        // A 400 that merely mentions context windows is not this bug.
        assert!(!body_reports_chat_completions_unsupported(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":{\"message\":\"context length exceeded\"}}"
        ));
        // A 400 that names both endpoints but for auth is not this bug.
        assert!(!body_reports_chat_completions_unsupported(
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":{\"message\":\"/v1/chat/completions and /v1/responses both require keys\"}}"
        ));
    }

    #[test]
    fn fallback_request_maps_chat_body_to_responses_shape() {
        let messages = vec![Message::user("hi")];
        let fallback = ctx(messages);
        let built = build_responses_request(&fallback, &chat_request(Some("max")))
            .expect("fallback request must build");

        assert_eq!(built["model"], "gpt-5.6-sol");
        assert_eq!(built["instructions"], "You are a helpful assistant.");
        assert_eq!(built["stream"], true);
        assert_eq!(built["store"], false);
        assert_eq!(built["tool_choice"], "auto");
        // Effort carried over from the chat body, summaries enabled.
        assert_eq!(built["reasoning"]["effort"], "max");
        assert_eq!(built["reasoning"]["summary"], "auto");

        // Input items: user message becomes a Responses message item.
        let input = built["input"].as_array().expect("input array");
        assert!(input.iter().any(|item| item["role"] == "user"
            && item["type"] == "message"
            && item["content"][0]["type"] == "input_text"
            && item["content"][0]["text"] == "hi"),);

        // Tools are flat function declarations (no nested `function` object).
        let tools = built["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["name"], "ls");
        assert!(tools[0].get("parameters").is_some());
        assert!(tools[0].get("function").is_none());

        // Chat-only keys never leak into the Responses body.
        assert!(built.get("messages").is_none());
        assert!(built.get("reasoning_effort").is_none());
    }

    #[test]
    fn fallback_request_omits_reasoning_when_chat_body_has_no_effort() {
        let fallback = ctx(vec![Message::user("hi")]);
        let built = build_responses_request(&fallback, &chat_request(None)).unwrap();
        assert!(built.get("reasoning").is_none());
    }

    #[test]
    fn fallback_request_maps_max_tokens_to_max_output_tokens() {
        let fallback = ctx(vec![Message::user("hi")]);
        let mut chat = chat_request(None);
        chat["max_tokens"] = serde_json::json!(4096);
        let built = build_responses_request(&fallback, &chat).unwrap();
        assert_eq!(built["max_output_tokens"], 4096);
    }

    #[test]
    fn fallback_from_rejection_only_fires_on_the_marker_error() {
        let fallback = ctx(vec![Message::user("hi")]);
        let chat = chat_request(None);

        let built = fallback_request_from_chat_rejection(
            reqwest::StatusCode::BAD_REQUEST,
            AGENTROUTER_400_BODY,
            "https://agentrouter.org/v1",
            "gpt-5.6-sol",
            &fallback,
            &chat,
        )
        .expect("the AgentRouter rejection must trigger the fallback");
        assert_eq!(built["model"], "gpt-5.6-sol");

        // Memoized: the same (endpoint, model) now routes directly.
        assert!(responses_required(
            "https://agentrouter.org/v1/",
            "GPT-5.6-Sol"
        ));

        // A different model on the same endpoint is not memoized.
        assert!(!responses_required("https://agentrouter.org/v1", "glm-5.3"));

        // A non-matching body does not memoize anything.
        assert!(!responses_required("https://other.example/v1", "glm-5.3"));
        assert!(
            fallback_request_from_chat_rejection(
                reqwest::StatusCode::BAD_REQUEST,
                "{\"error\":{\"message\":\"bad schema\"}}",
                "https://other.example/v1",
                "glm-5.3",
                &fallback,
                &chat,
            )
            .is_none()
        );

        // Assistant-history messages convert too (regression guard for the
        // tool-call replay path in later turns).
        let history = vec![Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_1".to_string(),
                name: "ls".to_string(),
                input: serde_json::json!({ "path": "." }),
                thought_signature: None,
            }],
            timestamp: None,
            tool_duration_ms: None,
        }];
        let history_ctx = ctx(history);
        let built_history = build_responses_request(&history_ctx, &chat)
            .expect("history fallback request must build");
        let input = built_history["input"].as_array().expect("input array");
        assert!(
            input
                .iter()
                .any(|item| item["type"] == "function_call" && item["name"] == "ls")
        );
    }
}
