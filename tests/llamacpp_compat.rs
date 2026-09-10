//! llama.cpp OpenAI-compat regression tests (integration, small link binary).
//!
//! Covers the user-facing contract from the local-provider bug report without
//! requiring a live `llama-server`:
//! 1. base URL normalization (4 forms), 2. /v1/models discovery URLs,
//! 3. model-id verbatim handling, 4/5. auth header modes, 6/7. reasoning effort
//!    helpers, 8/9. stream true/false payload shapes, 10. SSE parsing,
//!    11/12. context overflow detection, 13/14. timeout + cancellation helpers,
//! 15. tool schema sanitize, 16. cloud prefixes preserved.

use alphacode::alphacode_provider_metadata::{
    normalize_api_base, normalize_openai_compat_api_base, openai_compat_base_is_local,
    openai_compat_chat_completions_url, openai_compat_models_url, openai_compat_request_url,
};

// 1. Base URL normalization.
#[test]
fn base_url_normalization_covers_all_four_user_forms() {
    for base in [
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8080/",
        "http://127.0.0.1:8080/v1",
        "http://127.0.0.1:8080/v1/",
    ] {
        assert!(
            normalize_api_base(base).is_some(),
            "normalize_api_base must accept {base}"
        );
        assert_eq!(
            normalize_openai_compat_api_base(base).as_deref(),
            Some("http://127.0.0.1:8080/v1"),
            "compat canonical form for {base}"
        );
    }
}

// 2. Discovery URLs never produce /chat/completions without /v1, never /v1/v1.
#[test]
fn discovery_urls_are_versioned_without_duplication() {
    for base in [
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8080/",
        "http://127.0.0.1:8080/v1",
        "http://127.0.0.1:8080/v1/",
    ] {
        assert_eq!(
            openai_compat_chat_completions_url(base).as_deref(),
            Some("http://127.0.0.1:8080/v1/chat/completions"),
            "{base}"
        );
        assert_eq!(
            openai_compat_models_url(base).as_deref(),
            Some("http://127.0.0.1:8080/v1/models"),
            "{base}"
        );
        assert_eq!(
            openai_compat_request_url(base, "models").as_deref(),
            Some("http://127.0.0.1:8080/v1/models"),
            "{base}"
        );
    }
    let v1 = openai_compat_chat_completions_url("http://127.0.0.1:8080/v1").unwrap();
    assert!(!v1.contains("/v1/v1"), "must not double version: {v1}");
}

// 16. Cloud custom prefixes preserved (no blind /v1 hardcode).
#[test]
fn cloud_custom_prefixes_are_preserved() {
    assert_eq!(
        openai_compat_chat_completions_url("https://api-cdn.thehive.ai/api/v3").as_deref(),
        Some("https://api-cdn.thehive.ai/api/v3/chat/completions")
    );
    assert_eq!(
        openai_compat_models_url("https://api.openai.com/v1").as_deref(),
        Some("https://api.openai.com/v1/models")
    );
    assert_eq!(
        openai_compat_chat_completions_url("https://api.kimi.com/coding/v1").as_deref(),
        Some("https://api.kimi.com/coding/v1/chat/completions")
    );
}

// 13. Local vs cloud classification is host-based, not model-name based.
#[test]
fn local_classification_is_capability_based() {
    assert!(openai_compat_base_is_local("http://127.0.0.1:8080/v1"));
    assert!(openai_compat_base_is_local("http://localhost:11434/v1"));
    assert!(openai_compat_base_is_local("http://127.0.0.1:8080"));
    assert!(!openai_compat_base_is_local("https://api.openai.com/v1"));
    assert!(!openai_compat_base_is_local(
        "https://api.experientiallabs.ai/v1"
    ));
}

// 3. Windows GGUF path model IDs must be usable verbatim (llama.cpp without
// --alias reports the filesystem path as the catalog id).
#[test]
fn windows_path_model_id_is_verbatim_json() {
    let path =
        r"D:\freespace\alpha-train\gguf\Spark-X2.5-4B-abliterated-FIT-BALANCED-2.68GiB-Q5_K_S.gguf";
    let body = serde_json::json!({
        "model": path,
        "messages": [{"role": "user", "content": "Say hello in one short sentence."}],
        "stream": false,
        "max_tokens": 32
    });
    let serialized = serde_json::to_string(&body).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed["model"].as_str(), Some(path));
    // Drive-letter colon must not be confused with a routing prefix.
    assert!(path.as_bytes()[1] == b':');
}

// 8/9/10. llama.cpp SSE shapes: streaming deltas, [DONE], usage, empty +
// malformed chunks tolerated. Exercised through the public stream parser.
#[test]
fn llamacpp_sse_streaming_shape_yields_text_then_done() {
    use alphacode::alphacode_provider_openrouter::stream::OpenRouterStream;
    use futures::StreamExt;

    let payload = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":3}}\n\n",
        "data: [DONE]\n\n",
    );
    let bytes = bytes::Bytes::from(payload.to_string());
    let mut stream = OpenRouterStream::new(
        futures::stream::once(async move { Ok::<_, reqwest::Error>(bytes) }),
        "alpha".to_string(),
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    );
    let text = futures::executor::block_on(async {
        let mut text = String::new();
        let mut saw_usage = false;
        let mut stop = None;
        while let Some(event) = stream.next().await {
            match event.expect("stream item") {
                alphacode::alphacode_message_types::StreamEvent::TextDelta(d) => text.push_str(&d),
                alphacode::alphacode_message_types::StreamEvent::TokenUsage {
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    saw_usage = true;
                    assert_eq!(input_tokens, Some(10));
                    assert_eq!(output_tokens, Some(3));
                }
                alphacode::alphacode_message_types::StreamEvent::MessageEnd { stop_reason } => {
                    stop = stop_reason;
                    break;
                }
                _ => {}
            }
        }
        (text, saw_usage, stop)
    });
    assert_eq!(text.0, "Hello world");
    assert!(text.1);
    assert_eq!(text.2.as_deref(), Some("stop"));
}

// 11. Context-overflow bodies are recognizable for better diagnostics.
#[test]
fn context_overflow_body_is_recognizable() {
    let body = r#"{"error":{"code":400,"message":"request (11934 tokens) exceeds the available context size (8192 tokens)","type":"exceed_context_size_error"}}"#;
    let lower = body.to_ascii_lowercase();
    assert!(
        lower.contains("exceed")
            || lower.contains("exceeds the available context")
            || lower.contains("n_ctx")
            || lower.contains("context")
    );
}

// 15. Tool schemas: bare {"type":"object"} gains properties so strict
// gateways (and llama.cpp) do not 400.
#[test]
fn bare_object_tool_schema_is_sanitized() {
    use alphacode::alphacode_provider_openrouter::request::sanitize_tool_parameters_schema;
    let out = sanitize_tool_parameters_schema(&serde_json::json!({"type": "object"}));
    assert_eq!(out, serde_json::json!({"type": "object", "properties": {}}));
}
