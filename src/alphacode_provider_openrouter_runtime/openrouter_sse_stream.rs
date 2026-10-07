use super::*;
use crate::alphacode_provider_openrouter::stream::OpenRouterStream;

fn local_endpoint_troubleshooting_hint(api_base: &str, model: &str) -> &'static str {
    let lower = api_base.to_ascii_lowercase();
    if lower.contains("localhost:11434") || lower.contains("127.0.0.1:11434") {
        return "Ollama hint: make sure `ollama serve` is running, the model is installed with `ollama pull <model>`, and run alphacode with an installed model, for example `alphacode --provider ollama --model llama3.2 run 'hello'`.";
    }

    if lower.contains("localhost:1234") || lower.contains("127.0.0.1:1234") {
        return "LM Studio hint: start the Local Server in LM Studio, load a chat model, and run alphacode with the exact model id shown by LM Studio's /v1/models endpoint.";
    }

    if lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]") {
        return "Local endpoint hint: make sure the server is running, the base URL includes /v1, the selected model is loaded, and the server supports streaming POST /chat/completions.";
    }

    let _ = model;
    "Hint: check network connectivity, DNS/TLS, that the base URL includes the API version (usually /v1), and that the model exists on the provider."
}

fn is_alphax_free_gateway(api_base: &str) -> bool {
    url::Url::parse(api_base).is_ok_and(|url| {
        url.host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("api.kilo.ai"))
            && url.path().starts_with("/api/gateway")
    })
}

fn user_facing_endpoint_label(api_base: &str, endpoint: &str) -> String {
    if is_alphax_free_gateway(api_base) {
        "Alphax Free endpoint".to_string()
    } else {
        endpoint.to_string()
    }
}

fn redact_alphax_gateway_brand(api_base: &str, detail: &str) -> String {
    if !is_alphax_free_gateway(api_base) {
        return detail.to_string();
    }

    static GATEWAY_URL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)(?:https?://)?(?:api\.)?kilo\.ai(?:/[^\s"'<>),]*)?"#)
            .expect("valid gateway URL redaction pattern")
    });
    static ROUTE_ALIAS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)kilo-auto/[a-z0-9._/-]+")
            .expect("valid gateway route redaction pattern")
    });
    static BRAND_NAME: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\bkilo(?:code)?\b").expect("valid gateway brand redaction pattern")
    });

    let without_url = GATEWAY_URL.replace_all(detail, "Alphax Free service");
    let without_route = ROUTE_ALIAS.replace_all(&without_url, "automatic free route");
    BRAND_NAME
        .replace_all(&without_route, "the service")
        .into_owned()
}

/// Status-aware hint for a completed-but-failed HTTP response. The generic
/// network/DNS hint above only applies to *send* failures (connection never
/// established); once the server answered with an error status, blaming DNS
/// misleads the user — name what the status actually means instead.
fn http_status_troubleshooting_hint(
    status: reqwest::StatusCode,
    api_base: &str,
    model: &str,
    request_estimate: usize,
) -> String {
    // Local servers keep their server-specific advice (model not loaded, etc).
    let lower = api_base.to_ascii_lowercase();
    let is_local =
        lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]");
    if is_local {
        return local_endpoint_troubleshooting_hint(api_base, model).to_string();
    }
    match status.as_u16() {
        400 => format!(
            "Hint: the provider rejected the request (HTTP 400). The prompt (~{} tokens) may exceed this model's context window, or a request field is unsupported — try /compact to shrink context, or switch model (/model). The provider's exact reason is in the response body above.",
            request_estimate
        ),
        401 | 403 => format!(
            "Hint: authentication failed (HTTP {}). Run /login or check the API key for this provider.",
            status.as_u16()
        ),
        404 => "Hint: endpoint or model not found (HTTP 404) — check that the base URL includes the API version (usually /v1) and that the model exists on the provider (/model).".to_string(),
        402 => "Hint: payment or quota required (HTTP 402) — this model needs credits on the provider account; switch model (/model) or top up.".to_string(),
        429 => format!(
            "Hint: rate limited (HTTP 429) — wait before retrying, or switch model (/model). Context estimate ~{} tokens.",
            request_estimate
        ),
        s if (500..600).contains(&s) => format!(
            "Hint: provider-side server error (HTTP {}) — retry shortly, or switch model (/model).",
            s
        ),
        _ => format!(
            "Hint: the provider returned HTTP {} — see the response body above for the reason.",
            status.as_u16()
        ),
    }
}

// ============================================================================
// SSE Stream Parser
// ============================================================================

#[expect(
    clippy::too_many_arguments,
    reason = "stream helpers thread transport, auth, request, event channel, and pin state explicitly"
)]
pub(super) async fn run_stream_with_retries(
    client: Client,
    api_base: String,
    auth: ProviderAuth,
    send_openrouter_headers: bool,
    request: Value,
    tx: mpsc::Sender<Result<StreamEvent>>,
    provider_pin: Arc<Mutex<Option<ProviderPin>>>,
    model: String,
) {
    let mut last_error = None;
    let mut next_retry_delay = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            let delay = crate::alphacode_provider_core::retry_after::retry_delay(
                attempt,
                RETRY_BASE_DELAY_MS,
                next_retry_delay.take(),
            );
            tokio::time::sleep(delay).await;
            crate::alphacode_base::logging::info(&format!(
                "Retrying API request using {} (attempt {}/{})",
                auth.label(),
                attempt + 1,
                MAX_RETRIES
            ));
        }

        crate::alphacode_base::logging::info(&format!(
            "API stream attempt {}/{} over HTTPS transport (model: {}, endpoint: {}, auth: {})",
            attempt + 1,
            MAX_RETRIES,
            model,
            api_base,
            auth.label()
        ));

        // Track whether this attempt streams replay-visible output so a
        // mid-stream transport fault can roll the partial output back on the
        // consumer before the retry replays the response from the top.
        let (attempt_tx, attempt_guard) =
            crate::alphacode_provider_core::attempt_tracker::track_attempt_output(tx.clone());

        // Retries use a fresh unpooled client: the fault that broke attempt N
        // (e.g. TLS BadRecordMac from a corrupting middlebox) may also have
        // poisoned other idle pooled connections opened through the same path,
        // so reusing the shared pool can fail identically. A fresh client
        // guarantees a brand-new TCP+TLS connection.
        let attempt_client = if attempt == 0 {
            client.clone()
        } else {
            crate::alphacode_provider_core::fresh_transport_client()
        };

        match stream_response(
            attempt_client,
            api_base.clone(),
            auth.clone(),
            send_openrouter_headers,
            request.clone(),
            attempt_tx,
            Arc::clone(&provider_pin),
            model.clone(),
        )
        .await
        {
            Ok(()) => {
                let _ = attempt_guard.finish().await;
                return;
            }
            Err(e) => {
                let saw_output = attempt_guard.finish().await;
                // Full anyhow chain ({:#}) so a `.context(...)`-wrapped transport
                // cause (e.g. TLS BadRecordMac) is visible to the classifier.
                let error_str = format!("{e:#}").to_lowercase();
                if is_retryable_error(&error_str) && attempt + 1 < MAX_RETRIES {
                    if saw_output {
                        // Partial output already reached the consumer; tell it
                        // to discard the partial attempt so the retried
                        // response replays cleanly instead of duplicating.
                        crate::alphacode_base::logging::warn(&format!(
                            "Transient API error after partial output; rolling back partial attempt and retrying: {}",
                            e
                        ));
                        let _ = tx
                            .send(Ok(StreamEvent::RetryRollback {
                                attempt: attempt + 2,
                                max: MAX_RETRIES,
                            }))
                            .await;
                    } else {
                        crate::alphacode_base::logging::info(&format!(
                            "Transient API error, will retry: {}",
                            e
                        ));
                    }
                    next_retry_delay =
                        crate::alphacode_provider_core::retry_after::retry_after_from_error(&e);
                    last_error = Some(e);
                    continue;
                }

                let _ = tx.send(Err(e)).await;
                return;
            }
        }
    }

    if let Some(e) = last_error {
        let _ = tx
            .send(Err(anyhow::anyhow!(
                "Failed after {} retries: {}",
                MAX_RETRIES,
                e
            )))
            .await;
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "stream helpers thread transport, auth, request, event channel, and pin state explicitly"
)]
async fn stream_response(
    client: Client,
    api_base: String,
    auth: ProviderAuth,
    send_openrouter_headers: bool,
    request: Value,
    tx: mpsc::Sender<Result<StreamEvent>>,
    provider_pin: Arc<Mutex<Option<ProviderPin>>>,
    model: String,
) -> Result<()> {
    use crate::alphacode_message_types::ConnectionPhase;
    let _ = tx
        .send(Ok(StreamEvent::ConnectionPhase {
            phase: ConnectionPhase::SendingRequest,
        }))
        .await;
    let connect_start = std::time::Instant::now();
    let stream_idle_timeout = super::effective_stream_idle_timeout(&api_base);
    // User-facing diagnostics show the product name; the raw routing id stays
    // in logs and in the outbound request body only.
    let model_label = crate::alphacode_provider_metadata::internal_model_display_name(&model)
        .unwrap_or(&model)
        .to_string();

    let invalid_endpoint_label = user_facing_endpoint_label(&api_base, &api_base);
    let url = super::resolve_chat_completions_url(&api_base).ok_or_else(|| {
        anyhow::anyhow!(
            "OpenAI-compatible chat request failed\n  endpoint: invalid base '{}'\n  model: {}\n  auth: {}\n  mode: streaming",
            invalid_endpoint_label,
            model_label,
            auth.label()
        )
    })?;
    let endpoint_label = user_facing_endpoint_label(&api_base, url.as_str());
    let mut req = apply_kimi_coding_agent_headers(
        auth.apply(
            client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("Accept-Encoding", "identity"),
        )
        .await?,
        &api_base,
        Some(&model),
    );

    if send_openrouter_headers {
        req = req
            .header(
                "HTTP-Referer",
                crate::alphacode_provider_core::ALPHACODE_HOMEPAGE,
            )
            .header("X-Title", "Alphacode");
    }

    req = super::apply_opencode_provider_headers(req, &api_base);

    // Diagnostics: estimated prompt size, timeout, and mode. Never includes secrets.
    let request_estimate = super::estimate_chat_request_tokens(&request);
    let suppress_upstream_error_details = is_alphax_free_gateway(&api_base);
    let response = crate::alphacode_provider_core::transport::send_with_initial_response_timeout(
        req.json(&request),
        stream_idle_timeout,
    )
    .await
    .map_err(|error| {
        let hint = local_endpoint_troubleshooting_hint(&api_base, &model);
        let message = format!(
            "Failed to send OpenAI-compatible chat request\n  endpoint: {}\n  model: {}\n  auth: {}\n  mode: streaming\n  context_estimate: ~{} tokens\n  timeout: {}s\n{}",
            endpoint_label,
            model_label,
            auth.label(),
            request_estimate,
            stream_idle_timeout.as_secs(),
            hint
        );
        if suppress_upstream_error_details {
            // reqwest's source chain may embed the upstream URL. Keep useful
            // status and troubleshooting context while preventing that
            // implementation detail from leaking into user-visible errors.
            anyhow::anyhow!(message)
        } else {
            error.context(message)
        }
    })?;

    let connect_ms = connect_start.elapsed().as_millis();
    crate::alphacode_base::logging::info(&format!(
        "HTTP connection established in {}ms (status={})",
        connect_ms,
        response.status()
    ));

    if !response.status().is_success() {
        let status = response.status();
        let retry_after =
            crate::alphacode_provider_core::retry_after::retry_after(response.headers());
        let body = crate::alphacode_base::util::http_error_body(response, "HTTP error").await;
        let body = redact_alphax_gateway_brand(&api_base, &body);
        let mut hint =
            http_status_troubleshooting_hint(status, &api_base, &model, request_estimate);
        if super::response_body_reports_context_overflow(&body) {
            // The server just told us its real context window. Persist it
            // so the pre-flight guard and the compaction budget stop
            // trusting the model-family default (which is the model's
            // trained window, not the window this server was started
            // with) for every later request on this endpoint.
            let learned = super::parse_reported_context_tokens(&body);
            if let Some(limit) = learned {
                super::record_learned_context_limit(&api_base, &model, limit);
            }
            hint = match learned {
                Some(limit) => format!(
                    "{}\nContext hint: the prompt (~{} tokens) exceeded the server context. This endpoint reported a {}-token window (n_ctx), which is now used for prompt budgeting. Compact the session (/compact), drop large tool outputs, or restart the server with a larger context.",
                    hint, request_estimate, limit
                ),
                None => format!(
                    "{}\nContext hint: the prompt (~{} tokens) exceeded the server context. Compact the session (/compact), drop large tool outputs, or restart llama-server with a larger `-c` (e.g. `-c 16384`).",
                    hint, request_estimate
                ),
            };
        }
        return Err(
            crate::alphacode_provider_core::retry_after::error_with_retry_after(
                format!(
                    "OpenAI-compatible chat request failed\n  endpoint: {}\n  model: {}\n  auth: {}\n  mode: streaming\n  context_estimate: ~{} tokens\n  timeout: {}s\n  status: {}\n  response: {}\n{}",
                    endpoint_label,
                    model_label,
                    auth.label(),
                    request_estimate,
                    stream_idle_timeout.as_secs(),
                    status,
                    body,
                    hint
                ),
                retry_after,
            ),
        );
    }

    let _ = tx
        .send(Ok(StreamEvent::ConnectionPhase {
            phase: ConnectionPhase::WaitingForResponse,
        }))
        .await;

    let mut stream = OpenRouterStream::new(response.bytes_stream(), model.clone(), provider_pin);

    // Idle timeout between streamed chunks. Configurable so slow reasoning
    // models (e.g. DeepSeek) that think silently for minutes before emitting
    // tokens don't trip a premature timeout (issue #196). Resolved from
    // `[provider] stream_idle_timeout_secs` / `ALPHACODE_STREAM_IDLE_TIMEOUT_SECS`,
    // defaulting to 180s. Shared with the native provider paths (issue #434).
    let idle_timeout_secs = stream_idle_timeout.as_secs();

    loop {
        let event = match tokio::time::timeout(stream_idle_timeout, stream.next()).await {
            Ok(Some(Ok(event))) => event,
            Ok(Some(Err(error))) => {
                let detail = if suppress_upstream_error_details {
                    "the automatic route returned an invalid stream".to_string()
                } else {
                    error.to_string()
                };
                anyhow::bail!(
                    "OpenAI-compatible stream error\n  endpoint: {}\n  model: {}\n  auth: {}\n  mode: streaming\n  error: {}",
                    endpoint_label,
                    model_label,
                    auth.label(),
                    detail
                );
            }
            Ok(None) => break, // stream ended normally
            Err(_) => {
                crate::alphacode_base::logging::warn(&format!(
                    "OpenRouter SSE stream timed out (no data for {}s)",
                    idle_timeout_secs
                ));
                anyhow::bail!(
                    "OpenAI-compatible stream timeout\n  endpoint: {}\n  model: {}\n  auth: {}\n  mode: streaming\n  timeout: no data received for {} seconds\n{}",
                    endpoint_label,
                    model_label,
                    auth.label(),
                    idle_timeout_secs,
                    local_endpoint_troubleshooting_hint(&api_base, &model)
                );
            }
        };
        let event = if suppress_upstream_error_details {
            match event {
                StreamEvent::Error {
                    message,
                    retry_after_secs,
                } => StreamEvent::Error {
                    message: redact_alphax_gateway_brand(&api_base, &message),
                    retry_after_secs,
                },
                StreamEvent::UpstreamProvider { .. } => StreamEvent::UpstreamProvider {
                    provider: "Alphax Free".to_string(),
                },
                StreamEvent::StatusDetail { detail } => StreamEvent::StatusDetail {
                    detail: redact_alphax_gateway_brand(&api_base, &detail),
                },
                event => event,
            }
        } else {
            event
        };
        if tx.send(Ok(event)).await.is_err() {
            return Ok(());
        }
    }

    Ok(())
}

/// Extract the HTTP status code reported in a formatted provider error string.
///
/// Error strings produced in this module embed the status as `status: <code>`
/// (e.g. `status: 402 Payment Required`). The input may be lowercased before
/// it reaches here, so matching is case-insensitive.
fn parsed_http_status(error_str: &str) -> Option<u16> {
    let lower = error_str.to_ascii_lowercase();
    let idx = lower.find("status:")?;
    let rest = lower[idx + "status:".len()..].trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() == 3 {
        digits.parse().ok()
    } else {
        None
    }
}

/// Check if an error is transient and should be retried.
///
/// Delegates the heavy lifting to the unified
/// [`crate::alphacode_provider_core::retry::is_retryable_message`] classifier so
/// every provider agrees on what counts as transient. We keep the explicit
/// non-retryable HTTP-status list (400, 401, 402, 403, 404, 405, 406, 422, 429)
/// because 429 is handled by the failover system rather than the in-flight
/// retry loop.
fn is_retryable_error(error_str: &str) -> bool {
    if let Some(400 | 401 | 402 | 403 | 404 | 405 | 406 | 422 | 429) = parsed_http_status(error_str)
    {
        return false;
    }
    let lower = error_str.to_ascii_lowercase();
    crate::alphacode_provider_core::retry::is_retryable_message(&lower)
        || lower.contains("stream error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphax_error_details_hide_gateway_brand_and_routing_ids() {
        let detail = "Request to https://api.kilo.ai/api/gateway/v1 failed for kilo-auto/free: KiloCode is unavailable";
        let redacted = redact_alphax_gateway_brand("https://api.kilo.ai/api/gateway", detail);

        assert!(redacted.contains("Alphax Free service"));
        assert!(redacted.contains("automatic free route"));
        assert!(!redacted.to_ascii_lowercase().contains("kilo"));
    }

    #[test]
    fn other_provider_error_details_are_unchanged() {
        let detail = "https://api.kilo.ai is shown as a user supplied proxy URL";
        assert_eq!(
            redact_alphax_gateway_brand("https://api.openrouter.ai/api/v1", detail),
            detail
        );
    }

    #[test]
    fn local_endpoint_hint_mentions_ollama_actions() {
        let hint = local_endpoint_troubleshooting_hint("http://localhost:11434/v1", "llama3.2");
        assert!(hint.contains("ollama serve"));
        assert!(hint.contains("ollama pull"));
        assert!(hint.contains("--provider ollama"));
    }

    #[test]
    fn local_endpoint_hint_mentions_lm_studio_server() {
        let hint = local_endpoint_troubleshooting_hint("http://127.0.0.1:1234/v1", "local-model");
        assert!(hint.contains("LM Studio"));
        assert!(hint.contains("Local Server"));
        assert!(hint.contains("/v1/models"));
    }

    #[test]
    fn parsed_http_status_extracts_code() {
        assert_eq!(
            parsed_http_status("status: 402 payment required"),
            Some(402)
        );
        assert_eq!(parsed_http_status("  status:404 not found"), Some(404));
        assert_eq!(parsed_http_status("no status here"), None);
        // Embedded numbers elsewhere must not be misread as a status.
        assert_eq!(parsed_http_status("you requested 65536 tokens"), None);
    }

    #[test]
    fn payment_required_is_not_retryable() {
        let err = "openai-compatible chat request failed\n  endpoint: \
            https://openrouter.ai/api/v1/chat/completions\n  model: openai/gpt-5.4\n  \
            auth: openrouter_api_key\n  status: 402 payment required\n  response: \
            {\"error\":{\"message\":\"this request requires more credits, or fewer \
            max_tokens. you requested up to 65536 tokens, but can only afford 34424\"}}";
        assert!(!is_retryable_error(err));
    }

    #[test]
    fn client_errors_are_not_retryable() {
        for status in [400u16, 401, 402, 403, 404, 405, 406, 422] {
            let err = format!("chat request failed\n  status: {status} client error");
            assert!(
                !is_retryable_error(&err),
                "status {status} should not be retryable"
            );
        }
    }

    #[test]
    fn server_errors_remain_retryable() {
        assert!(is_retryable_error(
            "chat request failed\n  status: 503 service unavailable"
        ));
        assert!(is_retryable_error(
            "chat request failed\n  status: 500 internal server error"
        ));
        // Provider overload messages should still be retried.
        assert!(is_retryable_error("overloaded"));
    }

    #[test]
    fn http_429_is_not_retryable_at_transport_level() {
        // 429 is NOT retryable at the transport level. The failover system
        // handles it by marking the route unavailable and trying a fallback.
        // Retrying in the transport loop wastes extra API requests that count
        // against the rate limit.
        assert!(!is_retryable_error(
            "chat request failed\n  status: 429 unknown\n  response: {}"
        ));
        assert!(!is_retryable_error(
            "chat request failed\n  status: 429 Too Many Requests"
        ));
    }

    #[test]
    fn http_status_hint_blames_the_status_not_the_network() {
        use reqwest::StatusCode;
        let remote = "https://api.kilo.ai/api/gateway";
        let hint = http_status_troubleshooting_hint(
            StatusCode::BAD_REQUEST,
            remote,
            "kilo-auto/free",
            163_116,
        );
        assert!(
            !hint.contains("network connectivity") && !hint.contains("DNS"),
            "HTTP 400 must not suggest a network fault, got: {hint}"
        );
        assert!(hint.contains("HTTP 400"), "hint names the status: {hint}");
        assert!(hint.contains("/compact"), "hint suggests shrinking context");

        let auth = http_status_troubleshooting_hint(StatusCode::UNAUTHORIZED, remote, "m", 10);
        assert!(auth.contains("/login"), "401 points at /login: {auth}");

        let not_found = http_status_troubleshooting_hint(StatusCode::NOT_FOUND, remote, "m", 10);
        assert!(
            not_found.contains("/v1"),
            "404 still mentions the base-URL form: {not_found}"
        );

        let server =
            http_status_troubleshooting_hint(StatusCode::SERVICE_UNAVAILABLE, remote, "m", 10);
        assert!(server.contains("HTTP 503"), "5xx named: {server}");

        // Local endpoints keep their server-specific advice regardless of status.
        let local = http_status_troubleshooting_hint(
            StatusCode::BAD_REQUEST,
            "http://localhost:11434/v1",
            "llama3.2",
            10,
        );
        assert!(local.contains("Ollama"), "local hint preserved: {local}");
    }
}
