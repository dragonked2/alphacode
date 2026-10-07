#![cfg_attr(test, allow(clippy::await_holding_lock))]

use super::*;
use crate::alphacode_app_core::message::{Message, ToolDefinition};
use crate::alphacode_app_core::provider::{EventStream, Provider};
use async_trait::async_trait;
use serde_json::{Value, json};

/// The bug that prompted this ladder: a bare `from_value` rejected every
/// argument shape a model actually produces, and the resulting
/// "missing field `url`" cost a whole turn.
#[test]
fn coerce_url_arg_recovers_the_shapes_models_emit() {
    let coerce = |v: Value| coerce_url_arg(&v, "webfetch").unwrap();

    // Documented key.
    assert_eq!(
        coerce(json!({"url": "https://example.com/a"})),
        "https://example.com/a"
    );
    // Aliases, in the casing models actually use.
    for key in ["uri", "target", "href", "link", "URL", "Target"] {
        assert_eq!(
            coerce(json!({ key: "https://example.com/x" })),
            "https://example.com/x",
            "alias {key} was not recovered"
        );
    }
    // A key outside the alias list still resolves when its value is a URL.
    assert_eq!(
        coerce(json!({"full_link": "https://example.com/y"})),
        "https://example.com/y"
    );
    // Bare URL string where an object was expected.
    assert_eq!(
        coerce(Value::String("https://example.com/b".into())),
        "https://example.com/b"
    );
    // Single-element array wrapper.
    assert_eq!(
        coerce(json!([{"url": "https://example.com/c"}])),
        "https://example.com/c"
    );
    // Quoting characters models add when echoing a URL back from context.
    assert_eq!(
        coerce(json!({"url": "  `https://example.com/d` "})),
        "https://example.com/d"
    );
}

#[test]
fn coerce_url_arg_reports_an_actionable_error() {
    let err = coerce_url_arg(&json!({}), "webfetch")
        .unwrap_err()
        .to_string();
    // The `missing field` wording keeps the repeat guard from counting a
    // malformed call as an execution failure.
    assert!(err.contains("missing field `url`"), "{err}");
    // The error must show the whole contract, not one field at a time.
    assert!(err.contains("https://example.com"), "{err}");

    let wrong_key = coerce_url_arg(&json!({"adress": "example.com"}), "webfetch")
        .unwrap_err()
        .to_string();
    assert!(
        wrong_key.contains("`adress`"),
        "keys not listed: {wrong_key}"
    );

    // A non-string value cannot be a target.
    assert!(coerce_url_arg(&json!({"url": 42}), "webfetch").is_err());
    assert!(coerce_url_arg(&Value::Null, "webfetch").is_err());
}

/// A value that merely *contains* a URL must not be mistaken for one: `url`
/// is accepted verbatim, but an alias is only honoured when it really is an
/// http(s) target, or a `path` field would be fetched as a URL.
#[test]
fn coerce_url_arg_does_not_treat_arbitrary_strings_as_targets() {
    assert!(coerce_url_arg(&json!({"path": "/api/v1/users"}), "webfetch").is_err());
    assert!(coerce_url_arg(&json!({"host": "example.com"}), "webfetch").is_err());
}

#[test]
fn coerce_host_arg_recovers_bare_hosts() {
    let coerce = |v: Value| coerce_host_arg(&v, "nmap", "target").unwrap();
    // Documented key.
    assert_eq!(coerce(json!({"target": "example.com"})), "example.com");
    // Aliases, including the ones specific to recon tools.
    for key in ["host", "domain", "ip", "t", "d", "hostname"] {
        assert_eq!(
            coerce(json!({ key: "example.com" })),
            "example.com",
            "alias {key} was not recovered"
        );
    }
    // A scheme is unwrapped: these tools take a host, and `validate_hostname`
    // rejects both the colon and the path.
    assert_eq!(
        coerce(json!({"target": "https://example.com/admin"})),
        "example.com"
    );
    // Bare string and array wrapper.
    assert_eq!(coerce(Value::String("10.0.0.1".into())), "10.0.0.1");
    assert_eq!(
        coerce(json!([{"host": "example.com:8443"}])),
        "example.com:8443"
    );
    // Wildcards are legitimate recon input.
    assert_eq!(coerce(json!({"target": "*.example.com"})), "*.example.com");
}

#[test]
fn coerce_host_arg_rejects_non_hosts() {
    // A bare word with no dot, port or wildcard is not a host.
    assert!(coerce_host_arg(&json!({"target": "localhost"}), "nmap", "target").is_err());
    assert!(coerce_host_arg(&json!({}), "nmap", "target").is_err());
    let err = coerce_host_arg(&json!({}), "nmap", "target")
        .unwrap_err()
        .to_string();
    // The whole contract is stated at once, not one field per retry.
    assert!(err.contains("missing field `target`"), "{err}");
    assert!(err.contains("nmap"), "{err}");
}

struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> anyhow::Result<EventStream> {
        Err(anyhow::anyhow!(
            "Mock provider should not be used for streaming completions in tool registry tests"
        ))
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(MockProvider)
    }
}

#[test]
fn input_validation_errors_are_not_repeatable_execution_failures() {
    let missing = anyhow::anyhow!("missing field `command`. Received keys: ");
    let runtime = anyhow::anyhow!("command exited with status 1");
    assert!(super::is_input_validation_error(&missing));
    assert!(!super::is_input_validation_error(&runtime));
}

/// An empty call is the most common malformed shape, and the old
/// copy-pasted formatter rendered it as the useless
/// "missing field `file_path`. Received keys: ." — which named no fields and
/// gave the model nothing to correct.
#[test]
fn empty_arguments_are_described_actionably() {
    let described = super::describe_received_arguments(&json!({}));
    assert!(described.contains("no arguments at all"), "{described}");
    assert!(
        !described.contains("Received keys: ."),
        "must not render the garbled empty list: {described}"
    );

    // Keys present: list them so a typo is visible.
    let keys = super::describe_received_arguments(&json!({"filepath": "a.txt", "body": "x"}));
    assert!(keys.contains("`body`"), "{keys}");
    assert!(keys.contains("`filepath`"), "{keys}");

    // Wrong top-level type: say what arrived instead.
    let wrong = super::describe_received_arguments(&json!("just a string"));
    assert!(wrong.contains("a string"), "{wrong}");

    let null = super::describe_received_arguments(&Value::Null);
    assert!(null.contains("null"), "{null}");
}

/// End-to-end proof that the reported symptom is fixed: the same malformed
/// `write` — empty argument object — used to fail forever, because a validation
/// error called `clear_failure` and therefore erased its own streak.
///
/// This drives the real `Registry::execute`, not the guard in isolation, because
/// the bug was in the wiring: the guard was capable of refusing the call, it was
/// just never told about the failure.
#[tokio::test]
async fn an_identical_malformed_call_eventually_stops_repeating() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let session = format!("test-malformed-loop-{}", std::process::id());
    let ctx = |session: &str| ToolContext {
        session_id: session.to_string(),
        message_id: "message".to_string(),
        tool_call_id: "tool".to_string(),
        working_dir: None,
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let mut saw_refusal = false;
    let mut dispatched = 0usize;
    // More attempts than any limit allows: the loop must become bounded well
    // before this runs out.
    for _ in 0..(super::repeat_guard::MALFORMED_CALL_LIMIT + 4) {
        match registry.execute("write", json!({}), ctx(&session)).await {
            Err(error) => {
                let message = error.to_string();
                if message.contains("Refusing to run `write` again") {
                    saw_refusal = true;
                    break;
                }
                // An ordinary "missing field" rejection: the tool ran and said
                // no, which is the expected first-N behaviour.
                assert!(
                    message.contains("missing field `file_path`"),
                    "unexpected error: {message}"
                );
            }
            Ok(_) => panic!("an empty write must never succeed"),
        }
        dispatched += 1;
    }

    assert!(
        saw_refusal,
        "the identical malformed call was never refused after {dispatched} attempts"
    );
    assert!(
        dispatched <= super::repeat_guard::MALFORMED_CALL_LIMIT as usize,
        "took {dispatched} attempts to stop the loop; the limit is {}",
        super::repeat_guard::MALFORMED_CALL_LIMIT
    );
    clear_session_tool_policy(&session);
}

/// A model that fixes the call must still get through, and must not need any
/// explicit forgiveness -- a corrected call has a different input and so a
/// different streak.
#[tokio::test]
async fn a_corrected_malformed_call_is_still_dispatched() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let session = format!("test-malformed-fix-{}", std::process::id());
    let ctx = || ToolContext {
        session_id: session.clone(),
        message_id: "message".to_string(),
        tool_call_id: "tool".to_string(),
        working_dir: None,
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    // Burn through the whole malformed allowance on a call that cannot work.
    for _ in 0..super::repeat_guard::MALFORMED_CALL_LIMIT {
        assert!(registry.execute("write", json!({}), ctx()).await.is_err());
    }

    // Now the corrected shape. It must be dispatched -- i.e. produce a
    // *different* error rather than the guard's refusal.
    if let Err(error) = registry
        .execute("bash", json!({"command": "echo ok"}), ctx())
        .await
    {
        let message = error.to_string();
        assert!(
            !message.contains("Refusing to run `write` again")
                && !message.contains("Refusing to run `bash` again"),
            "the corrected call was refused by the loop guard: {message}"
        );
    }
    clear_session_tool_policy(&session);
}

#[test]
fn schema_call_hint_names_every_required_field_with_a_usable_example() {
    let schema = json!({
        "type": "object",
        "required": ["file_path", "content"],
        "properties": {
            "file_path": {"type": "string"},
            "content": {"type": "string"},
            "append": {"type": "boolean"},
        }
    });
    let hint = super::schema_call_hint(&schema).expect("hint");
    assert!(hint.contains("\"file_path\": \"...\""), "{hint}");
    assert!(hint.contains("\"content\": \"...\""), "{hint}");
    assert!(
        !hint.contains("append"),
        "optional fields must not be demanded: {hint}"
    );

    let mixed = json!({
        "type": "object",
        "required": ["action", "limit", "all_frames"],
        "properties": {
            "action": {"type": "string"},
            "limit": {"type": "integer"},
            "all_frames": {"type": "boolean"},
        }
    });
    let hint = super::schema_call_hint(&mixed).expect("hint");
    assert!(hint.contains("\"limit\": 0"), "{hint}");
    assert!(hint.contains("\"all_frames\": false"), "{hint}");

    // A schema with no required list must not produce a misleading hint.
    assert!(super::schema_call_hint(&json!({"type": "object"})).is_none());
}

/// End-to-end: the very first malformed call must state the tool's whole
/// required contract, not one field at a time.
#[tokio::test]
async fn a_malformed_call_reports_the_tools_full_required_contract() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let session = format!("schema-hint-{}", std::process::id());
    let ctx = ToolContext {
        session_id: session.clone(),
        message_id: "message".to_string(),
        tool_call_id: "tool".to_string(),
        working_dir: None,
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let error = registry
        .execute("write", json!({}), ctx)
        .await
        .expect_err("empty write call should fail validation");
    let message = error.to_string();
    assert!(message.contains("file_path"), "{message}");
    assert!(
        message.contains("\"content\""),
        "both required fields must be named at once: {message}"
    );
    clear_session_tool_policy(&session);
}

#[tokio::test]
async fn repeated_missing_bash_command_stays_correctable_until_arguments_change() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    // The repeat guard is process-global; do not let parallel tests share its
    // malformed-call streak accidentally.
    let session_id = format!("bash-validation-{}", uuid::Uuid::new_v4());
    let ctx = ToolContext {
        session_id: session_id.clone(),
        message_id: "message".to_string(),
        tool_call_id: "tool".to_string(),
        working_dir: None,
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };
    for _ in 0..3 {
        let error = registry
            .execute("bash", json!({}), ctx.clone())
            .await
            .expect_err("empty bash call should fail validation");
        assert!(
            error.to_string().contains("missing field `command`"),
            "empty bash call should report a correctable missing command: {error}"
        );
    }
    let corrected = registry
        .execute("bash", json!({"command": "echo ok"}), ctx)
        .await
        .expect("corrected bash call should execute");
    assert!(corrected.output.contains("ok"));
    clear_session_tool_policy(&session_id);
}

#[test]
fn required_argument_contract_names_mandatory_fields_once() {
    let schema = json!({
        "type": "object",
        "required": ["file_path", "content"],
        "properties": {"file_path": {}, "content": {}}
    });
    let contract = super::required_argument_contract(&schema, "Create or overwrite a file.")
        .expect("contract");
    assert!(contract.contains("`file_path`"), "{contract}");
    assert!(contract.contains("`content`"), "{contract}");
    assert!(
        contract.starts_with(' '),
        "must append, not replace: {contract}"
    );

    // A tool with no required fields gets nothing appended.
    let optional = json!({"type": "object", "properties": {"path": {}}});
    assert!(super::required_argument_contract(&optional, "List a directory.").is_none());

    // No repetition when the prose already names every required field.
    let already = "Write takes file_path and content.";
    assert!(super::required_argument_contract(&schema, already).is_none());

    // Partially-named prose still gets the full contract, since the model
    // needs to see the one it is missing.
    let partial =
        super::required_argument_contract(&schema, "Write takes file_path.").expect("contract");
    assert!(partial.contains("`content`"), "{partial}");
}

/// The prompt budget is hard: a description that already fills it must not be
/// made any longer, because every request pays for it.
#[test]
fn required_argument_contract_yields_to_the_prompt_budget() {
    let schema = json!({
        "type": "object",
        "required": ["command"],
        "properties": {"command": {"type": "string"}}
    });
    // `estimate_tokens` is chars/4, so this lands exactly on the cap.
    let filler = "x".repeat(super::TOOL_DESCRIPTION_TOKEN_CAP * 4);
    assert_eq!(
        crate::util::estimate_tokens(&filler),
        super::TOOL_DESCRIPTION_TOKEN_CAP
    );
    assert!(
        super::required_argument_contract(&schema, &filler).is_none(),
        "must not append past the cap"
    );
}

#[tokio::test]
async fn tool_definitions_state_their_required_arguments() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let defs = registry.definitions(None).await;

    let write = defs
        .iter()
        .find(|def| def.name == "write")
        .expect("write definition");
    assert!(
        write.description.contains("`file_path`") && write.description.contains("`content`"),
        "the model must see the contract: {}",
        write.description
    );

    // The schema itself must keep its `required` array untouched.
    assert!(write.input_schema.get("required").is_some());
}

/// Generating the contract must never breach the always-on prompt budget, so
/// a tool whose own guidance already fills the cap keeps its schema contract
/// and gains no extra prose.
#[tokio::test]
async fn generated_contracts_never_breach_the_description_budget() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    for def in registry.definitions(None).await {
        let exempt = matches!(def.name.as_str(), "discover_tools" | "swarm");
        if exempt {
            continue;
        }
        assert!(
            def.description_token_estimate() <= super::TOOL_DESCRIPTION_TOKEN_CAP,
            "{} is at {} tokens, over the {} cap: {}",
            def.name,
            def.description_token_estimate(),
            super::TOOL_DESCRIPTION_TOKEN_CAP,
            def.description
        );
    }
}

/// Prompt caching breaks if the definition text varies between calls, so the
/// generated contract has to be stable.
#[tokio::test]
async fn generated_tool_contracts_are_deterministic() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let first = registry.definitions(None).await;
    let second = registry.definitions(None).await;
    let first: Vec<_> = first.iter().map(|d| d.description.clone()).collect();
    let second: Vec<_> = second.iter().map(|d| d.description.clone()).collect();
    assert_eq!(first, second);
}

#[tokio::test]
async fn test_tool_definitions_are_sorted() {
    // Create registry with mock provider
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;

    // Get definitions multiple times and verify they're always in the same order
    let defs1 = registry.definitions(None).await;
    let defs2 = registry.definitions(None).await;

    // Should have the same order
    assert_eq!(defs1.len(), defs2.len());
    for (d1, d2) in defs1.iter().zip(defs2.iter()) {
        assert_eq!(d1.name, d2.name);
    }

    // Verify they're sorted alphabetically
    let names: Vec<&str> = defs1.iter().map(|d| d.name.as_str()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort();
    assert_eq!(
        names, sorted_names,
        "Tool definitions should be sorted alphabetically"
    );
}

#[test]
fn test_resolve_skill_aliases_to_skill_manage() {
    assert_eq!(Registry::resolve_tool_name("skill"), "skill_manage");
    assert_eq!(Registry::resolve_tool_name("Skill"), "skill_manage");
    assert_eq!(Registry::resolve_tool_name("skill_manage"), "skill_manage");
}

#[tokio::test]
async fn test_discover_tools_not_registered_when_sponsors_disabled() {
    // sponsors.enabled defaults to false; the discovery tool must not exist.
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let names = registry.tool_names().await;
    if crate::config::config().sponsors.enabled {
        assert!(names.iter().any(|n| n == "discover_tools"));
    } else {
        assert!(
            !names.iter().any(|n| n == "discover_tools"),
            "discover_tools must not be registered when sponsors are disabled"
        );
    }
}

#[tokio::test]
async fn subagent_tool_is_not_registered() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;

    assert!(
        !registry
            .tool_names()
            .await
            .iter()
            .any(|name| name == "subagent"),
        "the deprecated direct subagent tool must not be exposed; use swarm instead"
    );
}

struct BareSchemaTool;

#[async_trait]
impl Tool for BareSchemaTool {
    fn name(&self) -> &str {
        "bare_schema"
    }

    fn description(&self) -> &str {
        "Test tool without an explicit intent property."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string"}
            }
        })
    }

    async fn execute(&self, _input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::new("ok"))
    }
}

/// `to_definition` injects an optional `intent` property into every
/// object-shaped tool schema so a tool that omits `intent` from its
/// own `parameters_schema` still advertises it. The property is NOT
/// added to `required` because every tool deserializes it as
/// `Option<String>` with `#[serde(default)]`.
#[test]
fn tool_definitions_auto_inject_optional_intent() {
    let def = BareSchemaTool.to_definition();
    assert_eq!(def.input_schema["properties"]["intent"]["type"], "string");
    let required = def.input_schema["required"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !required.iter().any(|value| value == "intent"),
        "intent must NOT be required after central injection: {required:?}"
    );
    assert!(
        required.iter().any(|value| value == "command"),
        "injection must preserve the tool's own required fields: {required:?}"
    );
}

#[tokio::test]
async fn first_party_tool_definitions_have_optional_intent_with_display_only_docs() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    registry.register_ambient_tools().await;
    registry.register_selfdev_tools().await;

    let defs = registry.definitions(None).await;
    assert!(!defs.is_empty());

    for def in defs {
        let schema = &def.input_schema;
        if schema["type"] != "object" {
            continue;
        }

        assert_eq!(
            schema["properties"]["intent"]["type"], "string",
            "{} should explicitly define optional intent in its schema",
            def.name
        );
        assert!(
            schema["properties"]["intent"]["description"]
                .as_str()
                .unwrap_or_default()
                .contains("shown in the UI"),
            "{} intent description should say it is UI-display-only",
            def.name
        );
        let required = schema["required"].as_array().cloned().unwrap_or_default();
        assert!(
            !required.iter().any(|value| value == "intent"),
            "{} must NOT require intent (it is optional at runtime)",
            def.name
        );
    }
}

#[test]
fn test_resolve_tool_name_oauth_aliases() {
    assert_eq!(Registry::resolve_tool_name("file_read"), "read");
    assert_eq!(Registry::resolve_tool_name("file_write"), "write");
    assert_eq!(Registry::resolve_tool_name("file_edit"), "edit");
    assert_eq!(Registry::resolve_tool_name("shell_exec"), "bash");
    assert_eq!(Registry::resolve_tool_name("shell"), "bash");
    assert_eq!(Registry::resolve_tool_name("read_file"), "read");
    assert_eq!(Registry::resolve_tool_name("write_file"), "write");
    assert_eq!(Registry::resolve_tool_name("edit_file"), "edit");
    assert_eq!(Registry::resolve_tool_name("task_runner"), "subagent");
    assert_eq!(Registry::resolve_tool_name("task"), "subagent");
    assert_eq!(Registry::resolve_tool_name("launch"), "open");
    assert_eq!(Registry::resolve_tool_name("grep"), "agentgrep");
    assert_eq!(Registry::resolve_tool_name("file_grep"), "agentgrep");
    assert_eq!(Registry::resolve_tool_name("todo_read"), "todo");
    assert_eq!(Registry::resolve_tool_name("todo_write"), "todo");
    assert_eq!(Registry::resolve_tool_name("todoread"), "todo");
    assert_eq!(Registry::resolve_tool_name("todowrite"), "todo");
    assert_eq!(Registry::resolve_tool_name("bash"), "bash");
    assert_eq!(Registry::resolve_tool_name("functions.bash"), "bash");
    assert_eq!(Registry::resolve_tool_name("functions.shell_exec"), "bash");
    assert_eq!(Registry::resolve_tool_name("batch"), "batch");
    assert_eq!(Registry::resolve_tool_name("memory"), "memory");
}

#[tokio::test]
async fn test_batch_resolves_function_namespaced_tools() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let ctx = ToolContext {
        session_id: "test-batch-function-namespace".to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: Some(std::env::temp_dir()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let result = registry
        .execute(
            "batch",
            serde_json::json!({
                "tool_calls": [
                    {"tool": "functions.bash", "command": "true"},
                    {"tool": "functions.shell_exec", "command": "true"}
                ]
            }),
            ctx,
        )
        .await
        .expect("namespaced batch subcalls should execute");

    assert!(result.output.contains("Completed: 2 succeeded, 0 failed"));
    assert!(!result.output.contains("Unknown tool"));
    assert!(result.output.contains("--- [1] bash ---"));
    assert!(result.output.contains("--- [2] bash ---"));
    assert!(!result.output.contains("functions."));
}

#[tokio::test]
async fn test_batch_rejects_function_namespaced_batch_recursion() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let ctx = ToolContext {
        session_id: "test-batch-function-namespace-recursion".to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: Some(std::env::temp_dir()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let error = registry
        .execute(
            "batch",
            serde_json::json!({
                "tool_calls": [{"tool": "functions.batch", "tool_calls": []}]
            }),
            ctx,
        )
        .await
        .expect_err("namespaced batch recursion should be rejected");

    assert!(error.to_string().contains("Cannot batch the 'batch' tool"));
}

#[tokio::test]
async fn test_batch_resolves_oauth_names() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let temp_dir = std::env::temp_dir();

    let ctx = ToolContext {
        session_id: "test".to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: Some(temp_dir),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let result = registry
        .execute("shell_exec", serde_json::json!({"command": "true"}), ctx)
        .await;
    assert!(result.is_ok(), "shell_exec should resolve to bash tool");
}

#[tokio::test]
async fn registry_execute_enforces_session_tool_policy_after_alias_resolution() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let temp_dir = std::env::temp_dir();
    let session_id = "test-policy-deny";
    set_session_tool_policy(session_id, None, HashSet::from(["bash".to_string()]));

    let ctx = ToolContext {
        session_id: session_id.to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: Some(temp_dir.clone()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let result = registry
        .execute("shell_exec", serde_json::json!({"command": "true"}), ctx)
        .await;

    clear_session_tool_policy(session_id);
    assert!(result.is_err(), "deny-list should block aliased bash calls");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Tool 'bash' is disabled")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn registry_execute_pre_tool_hook_blocks_and_allows() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = crate::storage::lock_test_env();
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let temp = tempfile::TempDir::new().expect("temp dir");

    // Policy script: block bash calls whose input mentions "secret".
    let policy = temp.path().join("policy.sh");
    std::fs::write(
        &policy,
        "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in\n  *secret*) echo \"no secrets\" >&2; exit 2 ;;\nesac\nexit 0\n",
    )
    .expect("write policy");
    std::fs::set_permissions(&policy, std::fs::Permissions::from_mode(0o755))
        .expect("chmod policy");

    let prev = std::env::var_os("ALPHACODE_HOOK_PRE_TOOL");
    crate::alphacode_core::env::set_var(
        "ALPHACODE_HOOK_PRE_TOOL",
        policy.to_string_lossy().to_string(),
    );
    // alphacode-base is compiled without cfg(test) here, so the config cache only
    // re-checks env every 500ms; force a reload so the hook is visible now.
    crate::config::invalidate_config_cache();

    let ctx = || ToolContext {
        session_id: "test-pre-tool-hook".to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: Some(std::env::temp_dir()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let blocked = registry
        .execute(
            "bash",
            serde_json::json!({
                "command": "echo secret"
            }),
            ctx(),
        )
        .await;
    let allowed = registry
        .execute(
            "bash",
            serde_json::json!({
                "command": "true"
            }),
            ctx(),
        )
        .await;

    match prev {
        Some(value) => crate::alphacode_core::env::set_var("ALPHACODE_HOOK_PRE_TOOL", value),
        None => crate::alphacode_core::env::remove_var("ALPHACODE_HOOK_PRE_TOOL"),
    }
    crate::config::invalidate_config_cache();

    let error = blocked.expect_err("pre_tool hook should block matching input");
    assert!(
        error.to_string().contains("no secrets"),
        "hook stderr should surface in the error: {error}"
    );
    assert!(allowed.is_ok(), "non-matching input should pass the gate");
}

#[tokio::test]
async fn test_definitions_keep_batch_schema_generic() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;

    let defs = registry.definitions(None).await;
    let batch_def = defs
        .iter()
        .find(|def| def.name == "batch")
        .expect("batch definition should exist");

    assert!(batch_def.input_schema["properties"]["tool_calls"]["items"]["oneOf"].is_null());
    assert!(
        batch_def.input_schema["properties"]["tool_calls"]["items"]["required"]
            .as_array()
            .map(|required| required.iter().any(|value| value == "tool"))
            .unwrap_or(false)
    );
    assert!(
        batch_def.input_schema["properties"]["tool_calls"]["items"]["properties"]["parameters"]
            .is_null()
    );
}

#[test]
fn resolve_tool_name_maps_communicate_to_swarm() {
    assert_eq!(Registry::resolve_tool_name("communicate"), "swarm");
}

#[tokio::test]
#[ignore]
async fn print_tool_definition_token_report() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let mut defs = registry.definitions(None).await;
    defs.sort_by_key(|def| std::cmp::Reverse(def.prompt_token_estimate()));

    println!("name,total_tokens,description_tokens");
    for def in defs {
        println!(
            "{},{},{}",
            def.name,
            def.prompt_token_estimate(),
            def.description_token_estimate()
        );
    }
}

/// Tool descriptions are always-on prompt cost, so they are capped at a
/// modest estimated-token budget. The cap is intentionally generous because
/// the most-used tools (`bash`, `webfetch`, ...) have cross-platform or
/// security guidance that is genuinely cheaper to keep in the description
/// than to repeat across parameter docs and runtime errors. Tightening the
/// cap would push that detail into error strings, where it cannot help the
/// model choose between tools in the first place. Exemptions must be
/// justified inline.
#[tokio::test]
async fn tool_descriptions_stay_under_token_cap() {
    // 250 tokens covers `bash`'s cross-platform shell guidance (POSIX syntax,
    // Git Bash on Windows, anti-cmd.exe/PowerShell confusion) and
    // `webfetch`'s authorization context without losing the operational
    // prompts that make the tools hard to misuse. Shared with the generator
    // that appends the required-argument contract, so the two cannot drift.
    const DESCRIPTION_TOKEN_CAP: usize = super::TOOL_DESCRIPTION_TOKEN_CAP;
    // discover_tools keeps a deliberate second sentence disclosing that catalog
    // entries are vetted/partnered integrations.
    // swarm appends the user-tunable swarm-prompt.md by design.
    const EXEMPT: &[&str] = &["discover_tools", "swarm"];

    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let over_cap: Vec<String> = registry
        .definitions(None)
        .await
        .into_iter()
        .filter(|def| !EXEMPT.contains(&def.name.as_str()))
        .filter(|def| def.description_token_estimate() > DESCRIPTION_TOKEN_CAP)
        .map(|def| {
            format!(
                "{} (~{} tokens): {}",
                def.name,
                def.description_token_estimate(),
                def.description
            )
        })
        .collect();
    assert!(
        over_cap.is_empty(),
        "tool descriptions over the {DESCRIPTION_TOKEN_CAP}-token cap:\n{}",
        over_cap.join("\n")
    );
}

fn collect_param_descriptions(schema: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match schema {
        Value::Object(map) => {
            if path != "$"
                && let Some(Value::String(description)) = map.get("description")
            {
                out.push((path.to_string(), description.clone()));
            }
            for (key, value) in map {
                if key == "description" {
                    continue;
                }
                collect_param_descriptions(value, &format!("{path}.{key}"), out);
            }
        }
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                collect_param_descriptions(item, &format!("{path}[{idx}]"), out);
            }
        }
        _ => {}
    }
}

/// Parameter descriptions inside tool schemas are also always-on prompt cost,
/// so each is capped. Longer guidance belongs in runtime error messages, docs,
/// or the system prompt (the todo calibration rubrics, for example, live in
/// the gate continuation messages in alphacode-base::todo).
#[tokio::test]
async fn tool_parameter_descriptions_stay_under_token_cap() {
    // 100 tokens covers `bash`'s cross-platform `command` parameter guidance
    // (POSIX syntax, Git Bash on Windows, anti-cmd.exe/PowerShell confusion,
    // ~76 tokens) and keeps room for short imperative guidance on other
    // parameters without becoming a hiding place for full prose docs.
    const PARAM_DESCRIPTION_TOKEN_CAP: usize = 100;

    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let mut over_cap: Vec<String> = Vec::new();
    for def in registry.definitions(None).await {
        let mut descriptions = Vec::new();
        collect_param_descriptions(&def.input_schema, "$", &mut descriptions);
        for (path, description) in descriptions {
            let tokens = crate::util::estimate_tokens(&description);
            if tokens > PARAM_DESCRIPTION_TOKEN_CAP {
                over_cap.push(format!(
                    "{} {} (~{} tokens): {}",
                    def.name, path, tokens, description
                ));
            }
        }
    }
    assert!(
        over_cap.is_empty(),
        "{} parameter descriptions over the {PARAM_DESCRIPTION_TOKEN_CAP}-token cap:\n{}",
        over_cap.len(),
        over_cap.join("\n")
    );
}

fn schema_type_includes(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(value)) => value == expected,
        Some(Value::Array(values)) => values
            .iter()
            .any(|value| value.as_str().is_some_and(|value| value == expected)),
        _ => false,
    }
}

fn collect_schema_errors(schema: &Value, path: &str, errors: &mut Vec<String>) {
    match schema {
        Value::Object(map) => {
            if schema_type_includes(schema, "array") && !map.contains_key("items") {
                errors.push(format!("{path}: array schema missing items"));
            }

            // Gemini validates `required` against the same object's `properties`
            // and rejects the entire request when a name is missing, which broke
            // every tool-enabled Gemini call (issue #655). Objects without a
            // local `properties` map are exempt: there is nothing to check
            // against, and Gemini accepts those.
            if let (Some(Value::Array(required)), Some(Value::Object(properties))) =
                (map.get("required"), map.get("properties"))
            {
                for name in required {
                    let Some(name) = name.as_str() else {
                        errors.push(format!("{path}.required: entries must be strings"));
                        continue;
                    };
                    if !properties.contains_key(name) {
                        errors.push(format!(
                            "{path}.required: '{name}' is not defined in the same object's properties"
                        ));
                    }
                }
            }

            for keyword in ["anyOf", "oneOf", "allOf"] {
                let Some(branches) = map.get(keyword) else {
                    continue;
                };
                let Some(branches) = branches.as_array() else {
                    errors.push(format!("{path}.{keyword}: must be an array"));
                    continue;
                };
                for (idx, branch) in branches.iter().enumerate() {
                    let branch_path = format!("{path}.{keyword}[{idx}]");
                    match branch {
                        Value::Object(branch_map) => {
                            if !branch_map.contains_key("type") {
                                errors.push(format!("{branch_path}: schema missing type"));
                            }
                        }
                        _ => errors.push(format!("{branch_path}: schema branch must be an object")),
                    }
                }
            }

            for (key, value) in map {
                collect_schema_errors(value, &format!("{path}.{key}"), errors);
            }
        }
        Value::Array(values) => {
            for (idx, value) in values.iter().enumerate() {
                collect_schema_errors(value, &format!("{path}[{idx}]"), errors);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn test_tool_definitions_do_not_expose_invalid_array_schemas() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;

    let defs = registry.definitions(None).await;
    let mut errors = Vec::new();
    for def in &defs {
        collect_schema_errors(
            &def.input_schema,
            &format!("tool `{}`", def.name),
            &mut errors,
        );
    }

    assert!(
        errors.is_empty(),
        "tool definitions must not expose invalid schemas:\n{}",
        errors.join("\n")
    );
}

#[test]
fn test_schema_validator_rejects_any_of_branches_without_type() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "status_filter": {
                "anyOf": [
                    { "enum": ["running", "completed"] },
                    { "type": "array", "items": { "type": "string" } }
                ]
            }
        }
    });

    let mut errors = Vec::new();
    collect_schema_errors(&schema, "tool `test`", &mut errors);

    assert!(
        errors
            .iter()
            .any(|error| error.contains("status_filter.anyOf[0]: schema missing type")),
        "expected missing type error, got: {errors:?}"
    );
}

#[tokio::test]
async fn test_context_guard_small_output_passes_through() {
    let compaction = Arc::new(RwLock::new(CompactionManager::new().with_budget(200_000)));
    let registry = Registry {
        tools: Arc::new(RwLock::new(HashMap::new())),
        skills: Arc::new(RwLock::new(crate::skill::SkillRegistry::default())),
        compaction,
    };

    let output = ToolOutput::new("small output");
    let result = registry.guard_context_overflow("test", output).await;
    assert_eq!(result.output, "small output");
}

#[tokio::test]
async fn test_context_guard_truncates_huge_single_output() {
    let compaction = Arc::new(RwLock::new(CompactionManager::new().with_budget(1000)));
    let registry = Registry {
        tools: Arc::new(RwLock::new(HashMap::new())),
        skills: Arc::new(RwLock::new(crate::skill::SkillRegistry::default())),
        compaction,
    };

    // 30% of 1000 = 300 tokens = 1200 chars max for a single output
    // Create output that's way larger
    let big_output = "x".repeat(8000); // 2000 tokens, well over 30% of 1000
    let output = ToolOutput::new(big_output.clone());
    let result = registry.guard_context_overflow("test", output).await;
    assert!(
        result.output.len() < big_output.len(),
        "Output should be truncated"
    );
    assert!(
        result.output.contains("TRUNCATED"),
        "Should contain truncation warning"
    );
}

#[tokio::test]
async fn test_context_guard_truncates_when_context_nearly_full() {
    let compaction = Arc::new(RwLock::new(CompactionManager::new().with_budget(10_000)));
    {
        let mut mgr = compaction.write().await;
        mgr.update_observed_input_tokens(9500); // 95% full
    }
    let registry = Registry {
        tools: Arc::new(RwLock::new(HashMap::new())),
        skills: Arc::new(RwLock::new(crate::skill::SkillRegistry::default())),
        compaction,
    };

    // Even a modest output should get truncated when context is 95% full
    let output = ToolOutput::new("x".repeat(4000)); // 1000 tokens
    let result = registry.guard_context_overflow("test", output).await;
    assert!(
        result.output.contains("TRUNCATED") || result.output.contains("CONTEXT LIMIT"),
        "Should warn about context limits when nearly full"
    );
}

#[tokio::test]
async fn test_context_guard_zero_budget_passes_through() {
    let compaction = Arc::new(RwLock::new(CompactionManager::new().with_budget(0)));
    let registry = Registry {
        tools: Arc::new(RwLock::new(HashMap::new())),
        skills: Arc::new(RwLock::new(crate::skill::SkillRegistry::default())),
        compaction,
    };

    let output = ToolOutput::new("x".repeat(100_000));
    let result = registry.guard_context_overflow("test", output).await;
    assert_eq!(
        result.output.len(),
        100_000,
        "Zero budget should pass through"
    );
}

#[tokio::test]
async fn test_request_permission_is_ambient_only() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;

    let defs = registry.definitions(None).await;
    assert!(
        !defs.iter().any(|d| d.name == "request_permission"),
        "request_permission should not be available in normal sessions"
    );

    registry.register_ambient_tools().await;
    let defs_after = registry.definitions(None).await;
    assert!(
        defs_after.iter().any(|d| d.name == "request_permission"),
        "request_permission should be available after ambient tool registration"
    );
}

#[test]
fn closest_tool_names_suggests_near_misses() {
    let available = ["todo", "end_ambient_cycle", "bash", "read", "write", "edit"];
    // Exact-ish prefix/typo cases the ambient agent hit (#104).
    let s = Registry::closest_tool_names("todos", &available);
    assert_eq!(s.first().map(String::as_str), Some("todo"));

    let s = Registry::closest_tool_names("end_ambient_cyle", &available);
    assert!(s.iter().any(|n| n == "end_ambient_cycle"), "got {s:?}");

    // Case-insensitive containment.
    let s = Registry::closest_tool_names("Bash", &available);
    assert_eq!(s.first().map(String::as_str), Some("bash"));

    // A wildly unrelated name should yield no confident suggestion.
    let s = Registry::closest_tool_names("xyzzy_quux", &available);
    assert!(s.is_empty(), "got {s:?}");
}

#[tokio::test]
async fn unknown_tool_error_lists_available_tools_and_suggestions() {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    registry.register_ambient_tools().await;

    let ctx = ToolContext {
        session_id: "test-unknown-tool".to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: None,
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };
    let err = registry
        .execute("ToolSearch", serde_json::json!({}), ctx)
        .await
        .expect_err("ToolSearch is not a real tool");
    let msg = err.to_string();
    assert!(msg.contains("Unknown tool: ToolSearch"), "got: {msg}");
    assert!(
        msg.contains("Available tools:"),
        "error must list available tools so the model can recover (#104): {msg}"
    );
    assert!(
        msg.contains("end_ambient_cycle"),
        "available list should include registered ambient tools: {msg}"
    );
}

#[tokio::test]
async fn gemini_build_tools_from_registry_definitions_omits_const_keywords() {
    // Moved from alphacode-base/src/provider/gemini_tests.rs: this is the one test
    // that needs the upper-layer tool::Registry, so it lives here instead of
    // forcing a base -> app-core dev-dependency cycle.
    fn schema_contains_key(schema: &serde_json::Value, key: &str) -> bool {
        match schema {
            serde_json::Value::Object(map) => {
                map.contains_key(key) || map.values().any(|value| schema_contains_key(value, key))
            }
            serde_json::Value::Array(items) => {
                items.iter().any(|value| schema_contains_key(value, key))
            }
            _ => false,
        }
    }

    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider).await;
    let defs = registry.definitions(None).await;

    let built = crate::provider::gemini::build_tools(&defs).expect("gemini tools");
    let parameters = &built[0].function_declarations;

    assert!(!schema_contains_key(
        &serde_json::json!(parameters),
        "const"
    ));

    // Gemini rejects the whole generateContent request when any `required` entry
    // names a property the same object does not declare, which made every
    // tool-enabled Gemini call fail (issue #655). Assert on the *converted*
    // declarations: the pre-conversion sweep in
    // `test_tool_definitions_do_not_expose_invalid_array_schemas` cannot prove
    // the adapter output is clean, and the adapter is what Gemini actually sees.
    let mut dangling = Vec::new();
    for declaration in parameters {
        collect_dangling_required(
            &declaration.parameters,
            &format!("tool `{}`", declaration.name),
            &mut dangling,
        );
    }
    assert!(
        dangling.is_empty(),
        "converted Gemini function declarations still require undeclared properties:\n{}",
        dangling.join("\n")
    );
}

/// Collect `required` entries that name a property absent from the same
/// object's `properties` map. Objects without a local `properties` map are
/// exempt, matching what Gemini validates.
fn collect_dangling_required(schema: &Value, path: &str, errors: &mut Vec<String>) {
    match schema {
        Value::Object(map) => {
            if let (Some(Value::Array(required)), Some(Value::Object(properties))) =
                (map.get("required"), map.get("properties"))
            {
                for name in required {
                    if let Some(name) = name.as_str()
                        && !properties.contains_key(name)
                    {
                        errors.push(format!("{path}.required: '{name}' is not declared here"));
                    }
                }
            }
            for (key, value) in map {
                collect_dangling_required(value, &format!("{path}.{key}"), errors);
            }
        }
        Value::Array(values) => {
            for (idx, value) in values.iter().enumerate() {
                collect_dangling_required(value, &format!("{path}[{idx}]"), errors);
            }
        }
        _ => {}
    }
}
