//! Provider error classification shared by the agent, TUI and transports.
//!
//! Destructive context recovery needs an actual input-overflow signal, not a
//! diagnostic token estimate or generic troubleshooting advice.

fn error_evidence(error: &str) -> String {
    error
        .lines()
        .map(str::trim)
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            ![
                "hint:",
                "context hint:",
                "context_estimate:",
                "context_window:",
                "context estimate:",
                "context window:",
                "endpoint:",
                "model:",
                "auth:",
                "timeout:",
            ]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase()
}

/// A Responses history item belongs to another upstream OpenAI resource.
/// This is neither an input overflow nor a reason to discard conversation turns.
pub fn is_responses_resource_mismatch(error: &str) -> bool {
    let lower = error_evidence(error);
    lower.contains("requested item")
        && ((lower.contains("created under a different") && lower.contains("resource"))
            || lower.contains("same resource that created the item"))
}

/// Whether the provider explicitly rejected the input/context length.
/// Output caps, quotas and request diagnostics cannot establish this condition.
pub fn is_context_limit_error(error: &str) -> bool {
    let lower = error_evidence(error);

    // Explicit provider error codes remain authoritative even when the error
    // also describes the requested output allowance (e.g. max_tokens).
    if [
        "context_length_exceeded",
        "context_window_exceeded",
        "exceed_context_size",
        "request exceeds context",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }

    if [
        "max_tokens",
        "max_completion_tokens",
        "max_output_tokens",
        "max output tokens",
        "output token limit",
        "output length",
        "too many output tokens",
        "response too long",
        "finish_reason",
        "tokens for completion",
        "completion is",
        "tokens allowed",
        "tokens per minute",
        "tokens_per_minute",
        "rate limit",
        "rate_limit",
        "quota",
        "credits",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || is_responses_resource_mismatch(error)
    {
        return false;
    }

    [
        "maximum context length",
        "maximum context window",
        "too many tokens",
        "prompt is too long",
        "input is too long",
        "exceeds the available context",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || (["context", "n_ctx", "input token", "prompt token"]
            .iter()
            .any(|marker| lower.contains(marker))
            && ["exceed", "too long", "too large", "longer than"]
                .iter()
                .any(|marker| lower.contains(marker)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESOURCE_ERROR: &str = r#"{"error":{"message":"OpenAI Responses bad request: The requested item was created under a different *** OpenAI resource. Use the same resource that created the item to access it.","type":"invalid_request_error","param":"","code":null}}"#;

    #[test]
    fn regression_1094_diagnostics_and_generic_hints_are_not_overflows() {
        for body in [
            RESOURCE_ERROR,
            r#"{"error":{"message":"Unsupported parameter: temperature"}}"#,
            r#"{"error":{"message":"Invalid API key"}}"#,
            "rate limit exceeded; tokens per minute exhausted",
        ] {
            let error = format!(
                "OpenAI Responses request failed\n  context_estimate: ~11780 tokens\n  context_window: 128000 tokens\n  status: 400 Bad Request\n  response: {body}\nHint: The prompt may exceed this model's context window — try /compact."
            );
            assert!(!is_context_limit_error(&error), "false overflow: {error}");
        }
        assert!(!is_context_limit_error(
            "Failed to send request\n  context_estimate: ~3000 tokens\nconnection refused"
        ));
    }

    #[test]
    fn regression_1094_real_input_overflows_are_still_detected() {
        for error in [
            "provider response: context_length_exceeded",
            "context_window_exceeded",
            "OpenAI-compatible request exceeds context\n  context_window: 8192 tokens",
            "This model's maximum context length is 200000 tokens",
            "prompt is too long: 250000 tokens > 200000 maximum",
            "input is too long for the context window",
            "request too large: prompt is too long for context window",
            "too many tokens in request",
            "context limit exceeded",
            r#"{"error":{"type":"exceed_context_size_error","n_ctx":4096}}"#,
            "context_length_exceeded: input plus max_tokens exceeds context",
        ] {
            assert!(is_context_limit_error(error), "missed overflow: {error}");
        }
    }

    #[test]
    fn regression_1094_output_and_payload_limits_do_not_drop_turns() {
        for error in [
            "max_tokens is greater than the maximum number of tokens allowed",
            "This model's maximum tokens for completion is 16384",
            "output token limit exceeded",
            "response too long: finish_reason=length",
            "max_completion_tokens must be <= 128000",
            "max_output_tokens exceeds the output token limit",
            "HTTP 413 Payload Too Large: request too large",
            "input[*].encrypted_content: string_above_max_length",
            "context_estimate: ~11780 tokens",
        ] {
            assert!(!is_context_limit_error(error), "false overflow: {error}");
        }
    }

    #[test]
    fn regression_1094_resource_mismatch_is_specific() {
        assert!(is_responses_resource_mismatch(RESOURCE_ERROR));
        assert!(!is_context_limit_error(RESOURCE_ERROR));
        for error in [
            "resource not found",
            "requested item exceeds the context window",
            "resource quota exceeded",
            "invalid_request_error",
        ] {
            assert!(!is_responses_resource_mismatch(error));
        }
    }
}
