use super::*;

impl Agent {
    fn parse_text_wrapped_tool_call(
        text: &str,
    ) -> Option<(String, String, serde_json::Value, String)> {
        let marker = "to=functions.";
        let marker_idx = text.find(marker)?;
        let after_marker = &text[marker_idx + marker.len()..];

        let mut tool_name_end = 0usize;
        for (idx, ch) in after_marker.char_indices() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                tool_name_end = idx + ch.len_utf8();
            } else {
                break;
            }
        }
        if tool_name_end == 0 {
            return None;
        }

        let tool_name = after_marker[..tool_name_end].to_string();
        let remaining = &after_marker[tool_name_end..];
        let mut fallback: Option<(String, String, serde_json::Value, String)> = None;

        for (brace_idx, ch) in remaining.char_indices() {
            if ch != '{' {
                continue;
            }
            let slice = &remaining[brace_idx..];
            let mut stream =
                serde_json::Deserializer::from_str(slice).into_iter::<serde_json::Value>();
            let parsed = match stream.next() {
                Some(Ok(value)) => value,
                Some(Err(_)) | None => continue,
            };
            let consumed = stream.byte_offset();
            if !parsed.is_object() {
                continue;
            }

            let prefix = text[..marker_idx].trim_end().to_string();
            let suffix = remaining[brace_idx + consumed..].trim().to_string();
            if suffix.is_empty() {
                return Some((prefix, tool_name.clone(), parsed, suffix));
            }
            if fallback.is_none() {
                fallback = Some((prefix, tool_name.clone(), parsed, suffix));
            }
        }

        fallback
    }

    pub(super) fn recover_text_wrapped_tool_call(
        &self,
        text_content: &mut String,
        tool_calls: &mut Vec<ToolCall>,
    ) -> bool {
        if !tool_calls.is_empty() || text_content.trim().is_empty() {
            return false;
        }

        let Some((prefix, tool_name, arguments, suffix)) =
            Self::parse_text_wrapped_tool_call(text_content)
        else {
            return false;
        };

        let mut sanitized = String::new();
        if !prefix.is_empty() {
            sanitized.push_str(&prefix);
        }
        if !suffix.is_empty() {
            if !sanitized.is_empty() {
                sanitized.push('\n');
            }
            sanitized.push_str(&suffix);
        }
        *text_content = sanitized;

        let call_id = format!("fallback_text_call_{}", id::new_id("call"));
        let recovered_total = RECOVERED_TEXT_WRAPPED_TOOL_CALLS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        logging::warn(&format!(
            "[agent] Recovered text-wrapped tool call for '{}' ({}, total={})",
            tool_name, call_id, recovered_total
        ));
        let intent = ToolCall::intent_from_input(&arguments);
        tool_calls.push(ToolCall {
            id: call_id,
            name: tool_name,
            input: arguments,
            intent,
            thought_signature: None,
        });

        true
    }

    pub(crate) fn should_continue_after_stop_reason(stop_reason: &str) -> bool {
        let reason = stop_reason.trim().to_ascii_lowercase();
        if reason.is_empty() {
            return false;
        }

        if matches!(reason.as_str(), "stop" | "end_turn" | "tool_use") {
            return false;
        }

        reason.contains("incomplete")
            || reason.contains("max_output_tokens")
            || reason.contains("max_tokens")
            || reason.contains("length")
            || reason.contains("trunc")
            || reason.contains("commentary")
    }

    /// True when the provider's stop reason indicates a model-side
    /// guardrail/safety stop (e.g. Anthropic `refusal`), as opposed to a
    /// normal end-of-turn or truncation.
    pub(crate) fn is_guardrail_stop_reason(stop_reason: Option<&str>) -> bool {
        let Some(reason) = stop_reason else {
            return false;
        };
        let reason = reason.trim().to_ascii_lowercase();
        matches!(reason.as_str(), "refusal" | "content_filter" | "safety")
            || reason.contains("guardrail")
            || reason.contains("policy_violation")
    }

    /// Builds the user-facing notice for a turn that ended with no visible
    /// assistant output (no text, no tool calls). Returns `None` when the turn
    /// looks normal and no notice should be surfaced.
    pub(crate) fn provider_guardrail_notice(
        stop_reason: Option<&str>,
        visible_text_empty: bool,
        had_reasoning: bool,
    ) -> Option<String> {
        let guardrail = Self::is_guardrail_stop_reason(stop_reason);
        if !guardrail && !visible_text_empty {
            return None;
        }
        let reason_label = stop_reason
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .unwrap_or("unknown");
        if guardrail {
            return Some(format!(
                "Provider guardrail stopped the response (stop_reason: {}). The model declined to answer this request. Rephrasing, narrowing the request, or providing more context may help.",
                reason_label
            ));
        }
        // Empty visible output with a non-guardrail stop reason: still surface,
        // since the user otherwise sees nothing at all. Do not assert a content
        // filter here: in practice this is usually a transient upstream failure
        // (a dropped or empty stream), not a provider guardrail (issue #672).
        let reasoning_hint = if had_reasoning {
            " after producing only internal reasoning"
        } else {
            ""
        };
        Some(format!(
            "The model ended its turn without any visible output{} (stop_reason: {}). The provider returned an empty response; this is usually a transient upstream failure rather than a content filter. Retrying the request may help.",
            reasoning_hint, reason_label
        ))
    }

    /// Log-event label for an empty final turn: real guardrail stops keep the
    /// `PROVIDER_GUARDRAIL` name, transient empty responses get their own so
    /// the two are separable in logs (issue #672).
    pub(crate) fn empty_turn_log_event(stop_reason: Option<&str>) -> &'static str {
        if Self::is_guardrail_stop_reason(stop_reason) {
            "PROVIDER_GUARDRAIL"
        } else {
            "PROVIDER_EMPTY_RESPONSE"
        }
    }
    /// Retry a whitespace-only final response that arrived right after tool
    /// results, by asking the model to produce the final answer. Shared by the
    /// non-streaming and streaming (mpsc) turn loops so their recovery
    /// behavior cannot drift (issue #672). Returns true when a continuation
    /// message was injected and the caller should re-issue the request.
    // The constant `MAX_EMPTY_POST_TOOL_CONTINUATION_ATTEMPTS` lives on the
    // impl block in `turn_loops.rs` so it has exactly one definition. We just
    // reference it through `Self::MAX_EMPTY_POST_TOOL_CONTINUATION_ATTEMPTS`
    // from this file, so a future change to the value lives in one place.
    /// Retries allowed when a turn's API call itself returns a transient
    /// provider / transport error (5xx, 429, network/EOF, TLS reset, ...).
    /// Bounded so a permanently broken provider cannot keep an autonomous
    /// session running forever, but generous enough to ride out a 120s rate
    /// limit (the OpenRouter free tier's worst case as of 2026-08).
    /// Increased from 4 to 6 with wider backoff to handle longer rate-limit
    /// windows without excessive retries.
    pub(crate) const MAX_PROVIDER_ERROR_CONTINUATION_ATTEMPTS: u32 = 6;
    pub(crate) fn maybe_continue_empty_post_tool_response(
        &mut self,
        visible_text_empty: bool,
        prompt_has_recent_tool_result: bool,
        stop_reason: Option<&str>,
        attempts: &mut u32,
    ) -> Result<bool> {
        if !visible_text_empty || !prompt_has_recent_tool_result {
            return Ok(false);
        }
        // A model-side refusal is deliberate; retrying it just burns tokens.
        if Self::is_guardrail_stop_reason(stop_reason) {
            return Ok(false);
        }
        if *attempts >= Self::MAX_EMPTY_POST_TOOL_CONTINUATION_ATTEMPTS {
            return Ok(false);
        }
        *attempts += 1;
        logging::warn(&format!(
            "Provider returned whitespace-only final response after tool results (stop_reason={:?}); requesting final answer continuation (attempt {}/{})",
            stop_reason,
            attempts,
            Self::MAX_EMPTY_POST_TOOL_CONTINUATION_ATTEMPTS
        ));
        self.add_message(
            Role::User,
            vec![ContentBlock::Text {
                text: "The previous provider response was empty after tool results. Please provide the final answer to the user's last request using the tool results above. Do not call more tools unless absolutely necessary.".to_string(),
                cache_control: None,
            }],
        );
        self.session.save()?;
        Ok(true)
    }

    /// Detect transient provider / transport errors that are worth retrying
    /// the same turn for. The classifier is intentionally lenient: a false
    /// positive costs one extra "continue" round-trip, a false negative
    /// strands a long-running autonomous session on a transient blip.
    ///
    /// What is "transient"?
    /// - Any 5xx status (server-side overload / deploys / upstream issues).
    /// - 429 *only* when the body indicates a *rate limit* (not auth/credit).
    /// - Network/transport errors: TLS, EOF, "connection reset", "stream
    ///   error", "no data received", etc.
    /// - Explicit "overloaded" / "temporarily unavailable" markers.
    ///
    /// What is *not* transient?
    /// - 4xx other than 429: bad request, auth failure, payment required,
    ///   model not found, etc. Retrying these just burns time and quota.
    /// - "Empty response" / "guardrail" / "refusal": those are handled by
    ///   the other `maybe_continue_*` helpers, not by retrying the call.
    pub(crate) fn is_transient_provider_error(error: &str) -> bool {
        let lower = error.to_ascii_lowercase();

        // Reuse the transport-layer classifier so the two paths agree on
        // what counts as retryable. That covers EOF, "stream error",
        // "connection reset", TLS BadRecordMac, etc.
        if crate::alphacode_provider_core::is_transient_transport_error(&lower) {
            return true;
        }

        // 5xx server errors: the upstream is failing, retry is appropriate.
        if contains_5xx_status(&lower) {
            return true;
        }

        // 429 with rate-limit language. The body in question typically reads
        // "Too Many Requests" or includes "rate limit" / "rate-limited". 429
        // without those words is more often an upstream-provider-specific
        // error code (e.g. OpenRouter's "data policy" or "moderation" 429)
        // where retrying the same call will fail identically.
        if contains_429_with_rate_limit(&lower) {
            return true;
        }

        // Free-form provider overload messages. These are common on
        // OpenRouter's free tier and Anthropic's overloaded models.
        if lower.contains("overloaded")
            || lower.contains("temporarily unavailable")
            || lower.contains("upstream provider")
            || lower.contains("try again")
            || (lower.contains("retry") && lower.contains("shortly"))
        {
            return true;
        }

        false
    }

    /// Auto-continue the current turn after a transient provider error.
    ///
    /// Injects a user-side "continue" reminder so the model resumes from the
    /// exact point the previous call failed at, and tells the caller to
    /// `continue` the turn loop. Bounded by
    /// [`Self::MAX_PROVIDER_ERROR_CONTINUATION_ATTEMPTS`] so a permanently
    /// broken provider cannot keep an autonomous session running forever.
    ///
    /// Returns `Ok(false)` (i.e. *do not* retry) for non-transient errors
    /// and once the attempt budget is exhausted. Both cases log a clear
    /// reason so a future operator can tell "the user hit a hard 4xx" from
    /// "the provider is broken beyond our budget".
    /// Returns `Ok(Some(delay_secs))` when the error is transient and a
    /// retry should happen after the given delay, `Ok(false)` when the error
    /// is non-transient or the budget is exhausted, and `Ok(true)` with a
    /// delay of 0 for immediate retries.
    pub(crate) fn maybe_continue_after_provider_error(
        &mut self,
        error: &str,
        attempts: &mut u32,
    ) -> Result<bool> {
        if !Self::is_transient_provider_error(error) {
            return Ok(false);
        }
        if *attempts >= Self::MAX_PROVIDER_ERROR_CONTINUATION_ATTEMPTS {
            logging::warn(&format!(
                "Transient provider error retry budget exhausted after {} attempts: {}",
                attempts, error
            ));
            return Ok(false);
        }
        *attempts += 1;
        let attempt = *attempts;
        let delay = retry_delay_for_error(error, attempt);
        logging::warn(&format!(
            "Transient provider error; waiting {}s before retrying (attempt {}/{}): {}",
            delay,
            attempt,
            Self::MAX_PROVIDER_ERROR_CONTINUATION_ATTEMPTS,
            error.lines().next().unwrap_or(error).trim()
        ));
        // Do NOT inject a continuation message for 429/rate-limit errors.
        // Injecting a user message forces a brand-new full-context API request,
        // which wastes tokens and can trigger more 429s. Instead, the caller
        // sleeps for `delay` seconds and retries the exact same request.
        // Continuation messages are only injected for non-rate-limit transient
        // errors where the model needs to pick up where it left off.
        let is_rate_limit = error.to_ascii_lowercase().contains("429")
            || error.to_ascii_lowercase().contains("rate limit")
            || error.to_ascii_lowercase().contains("too many requests")
            || error.to_ascii_lowercase().contains("throttl");
        if !is_rate_limit {
            self.add_message(
                Role::User,
                vec![ContentBlock::Text {
                    text: format!(
                        "[System reminder: the previous provider call failed with a transient error. The provider asked us to wait {} seconds. Please continue exactly where you left off -- the failure was on our side, not yours. Do not repeat completed work; if the next step is a tool call, emit the tool call now. Retry attempt {}/{}]",
                        delay,
                        attempt,
                        Self::MAX_PROVIDER_ERROR_CONTINUATION_ATTEMPTS,
                    ),
                    cache_control: None,
                }],
            );
        }
        self.session.save()?;
        Ok(true)
    }

    fn continuation_prompt_for_stop_reason(stop_reason: &str) -> String {
        format!(
            "[System reminder: your previous response ended before completion (stop_reason: {}). Continue exactly where you left off, do not repeat completed content, and if the next step is a tool call, emit the tool call now.]",
            stop_reason
        )
    }

    pub(crate) fn maybe_continue_incomplete_response(
        &mut self,
        stop_reason: Option<&str>,
        attempts: &mut u32,
    ) -> Result<bool> {
        let Some(stop_reason) = stop_reason
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
        else {
            return Ok(false);
        };

        if !Self::should_continue_after_stop_reason(stop_reason) {
            return Ok(false);
        }

        if *attempts >= Self::MAX_INCOMPLETE_CONTINUATION_ATTEMPTS {
            logging::warn(&format!(
                "Response ended with stop_reason='{}' after {} continuation attempts; returning partial output",
                stop_reason, attempts
            ));
            return Ok(false);
        }

        *attempts += 1;
        logging::warn(&format!(
            "Response ended with stop_reason='{}'; requesting continuation (attempt {}/{})",
            stop_reason,
            attempts,
            Self::MAX_INCOMPLETE_CONTINUATION_ATTEMPTS
        ));

        self.add_message(
            Role::User,
            vec![ContentBlock::Text {
                text: Self::continuation_prompt_for_stop_reason(stop_reason),
                cache_control: None,
            }],
        );
        self.session.save()?;
        Ok(true)
    }

    /// True when the provider said it stopped to call a tool but no tool call
    /// survived parsing.
    ///
    /// `stop_reason: tool_use` with zero tool calls is a contradiction: the
    /// model intended to act and the harness has nothing to run. Breaking out
    /// of the turn there strands the agent mid-task, which on a benchmark run
    /// looks like an ordinary "the agent stopped early" failure and silently
    /// discards all of its uncommitted work. Treat it like any other
    /// incomplete response and ask for a continuation instead.
    pub(crate) fn is_stranded_tool_use_stop(stop_reason: Option<&str>) -> bool {
        stop_reason
            .map(str::trim)
            .map(|reason| reason.eq_ignore_ascii_case("tool_use"))
            .unwrap_or(false)
    }

    pub(crate) fn maybe_continue_stranded_tool_use(
        &mut self,
        stop_reason: Option<&str>,
        attempts: &mut u32,
    ) -> Result<bool> {
        if !Self::is_stranded_tool_use_stop(stop_reason) {
            return Ok(false);
        }
        if *attempts >= Self::MAX_INCOMPLETE_CONTINUATION_ATTEMPTS {
            logging::warn(&format!(
                "Provider reported stop_reason='tool_use' with no parsed tool call after {} continuation attempts; ending turn",
                attempts
            ));
            return Ok(false);
        }
        *attempts += 1;
        logging::warn(&format!(
            "Provider reported stop_reason='tool_use' but no tool call was parsed; requesting continuation (attempt {}/{})",
            attempts,
            Self::MAX_INCOMPLETE_CONTINUATION_ATTEMPTS
        ));
        self.add_message(
            Role::User,
            vec![ContentBlock::Text {
                text: "[System reminder: your previous response ended with stop_reason \"tool_use\" but no tool call arrived. Nothing was executed. Re-issue the tool call you intended, do not repeat completed work, and continue the task.]"
                    .to_string(),
                cache_control: None,
            }],
        );
        self.session.save()?;
        Ok(true)
    }

    pub(super) fn filter_truncated_tool_calls(
        &mut self,
        stop_reason: Option<&str>,
        tool_calls: &mut Vec<ToolCall>,
        _assistant_message_id: Option<&String>,
    ) {
        let stop_reason = stop_reason.unwrap_or("");
        if !Self::should_continue_after_stop_reason(stop_reason) {
            return;
        }

        let before = tool_calls.len();
        tool_calls.retain(|tc| !tc.input.is_null());
        let discarded = before - tool_calls.len();
        if discarded > 0 && tool_calls.is_empty() {
            logging::warn(&format!(
                "Discarded {} tool call(s) with null input (truncated by {}); requesting continuation",
                discarded,
                if stop_reason.is_empty() {
                    "unknown"
                } else {
                    stop_reason
                }
            ));
        }
    }
}
/// Match an HTTP 5xx status embedded anywhere in the (already lowercased)
/// error string. The match is bounded by non-digit boundaries so model
/// versions like "gpt-5" or "claude-3-5" do not register as status codes.
fn contains_5xx_status(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let want: &[&[u8]] = &[b"500", b"501", b"502", b"503", b"504", b"507", b"520", b"521", b"522", b"523", b"524", b"525", b"526", b"527", b"529"];
    for needle in want {
        for (start, _) in lower.match_indices(std::str::from_utf8(needle).unwrap()) {
            let before_ok = start == 0 || !bytes[start - 1].is_ascii_digit();
            let end = start + needle.len();
            let after_ok = end == bytes.len() || !bytes[end].is_ascii_digit();
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// 429 with rate-limit language. The classifier must be conservative: 429
/// without rate-limit words usually means a different upstream error (data
/// policy, moderation), and the user has explicitly asked us not to retry
/// those.
fn contains_429_with_rate_limit(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut found_429 = false;
    for (start, _) in lower.match_indices("429") {
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_digit();
        let end = start + 3;
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_digit();
        if before_ok && after_ok {
            found_429 = true;
            break;
        }
    }
    if !found_429 {
        return false;
    }
    lower.contains("rate limit")
        || lower.contains("rate-limit")
        || lower.contains("rate_limited")
        || lower.contains("too many requests")
        || lower.contains("temporarily rate-limited")
        || lower.contains("retry shortly")
        || lower.contains("retry-after")
        || lower.contains("retry_after")
        || lower.contains("rate limits")
        || lower.contains("ratelimit")
        || lower.contains("throttl")
        || lower.contains("quota exceeded")
}

/// Extract `retry_after_seconds` from an OpenRouter-style error response.
///
/// OpenRouter embeds the retry delay in the JSON response body, e.g.
/// `"retry_after_seconds":60`. When present, the caller should sleep for
/// that duration before retrying to avoid burning through the retry budget
/// on retries that are guaranteed to fail.
///
/// Returns `None` when no explicit retry-after is found (caller should
/// use exponential backoff as a fallback).
pub(crate) fn extract_retry_after_secs(error: &str) -> Option<u64> {
    // Look for `"retry_after_seconds":N` or `"retry_after_seconds_raw":N`.
    // The value may appear as a bare integer or a quoted string.
    for needle in ["retry_after_seconds_raw\":", "retry_after_seconds\":"] {
        if let Some(pos) = error.find(needle) {
            let after = &error[pos + needle.len()..];
            // Skip optional whitespace and optional quote
            let after = after.trim_start();
            let after = after.trim_start_matches('"');
            // Parse the digits
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(secs) = digits.parse::<u64>()
                && secs > 0 && secs <= 300 {
                    return Some(secs);
                }
        }
    }
    None
}

/// Compute a retry delay for a transient provider error, preferring the
/// provider's explicit `retry_after_seconds` hint and falling back to
/// exponential backoff with jitter.
///
/// The delay is capped at 300 seconds to avoid stalling indefinitely.
/// Rate-limit (429) errors get a wider backoff than other transient errors
/// because retrying too soon is almost guaranteed to fail again and wastes
/// the rate-limit budget on retries.
///
/// Backoff schedules:
/// - Rate-limit (429): 10s, 20s, 40s, 80s, 160s, 300s (capped)
/// - Other transient:  5s, 10s, 20s, 40s, 80s, 160s (capped)
pub(crate) fn retry_delay_for_error(error: &str, attempt: u32) -> u64 {
    if let Some(secs) = extract_retry_after_secs(error) {
        // Provider told us how long to wait. Respect it but cap at 300s.
        return secs.min(300);
    }
    use rand::Rng;
    let is_rate_limit = {
        let lower = error.to_ascii_lowercase();
        lower.contains("429")
            || lower.contains("rate limit")
            || lower.contains("too many requests")
            || lower.contains("throttl")
    };
    // Rate-limit errors use a 10s base to give providers more breathing room.
    // Other transient errors use a 5s base.
    let base_secs: u64 = if is_rate_limit { 10 } else { 5 };
    let base = base_secs.saturating_mul(1u64 << attempt.min(5));
    let jittered = (base as f64 * rand::rng().random_range(0.8..1.2)) as u64;
    jittered.max(base_secs).min(300)
}

#[cfg(test)]
mod provider_error_tests {
    use super::*;

    #[test]
    fn recognises_5xx_as_transient() {
        for s in [
            "OpenAI-compatible chat request failed\n  status: 500 internal server error",
            "status: 502 bad gateway",
            "status: 503 service unavailable",
            "status: 504 gateway timeout",
        ] {
            assert!(Agent::is_transient_provider_error(s), "{s} should be transient");
        }
    }

    #[test]
    fn recognises_rate_limit_429_as_transient() {
        assert!(Agent::is_transient_provider_error(
            "OpenAI-compatible chat request failed\n  status: 429 Too Many Requests\n  response: {\"error\":{\"message\":\"Provider returned error\",\"code\":429,\"metadata\":{\"raw\":\"minimax/minimax-m3:free is temporarily rate-limited upstream. Please retry shortly, or add your own key to accumulate your rate limits\"}}}"
        ));
    }

    #[test]
    fn does_not_retry_4xx() {
        for s in [
            "status: 400 bad request",
            "status: 401 unauthorized",
            "status: 402 payment required",
            "status: 403 forbidden",
            "status: 404 not found",
        ] {
            assert!(
                !Agent::is_transient_provider_error(s),
                "{s} should NOT be retried"
            );
        }
    }

    #[test]
    fn does_not_retry_429_without_rate_limit_words() {
        // OpenRouter returns 429 for data-policy violations and moderation
        // hits. Retrying those with the same prompt just hits the same wall.
        assert!(!Agent::is_transient_provider_error(
            "status: 429 moderation hit"
        ));
    }

    #[test]
    fn recognises_overloaded_and_retry_shortly() {
        assert!(Agent::is_transient_provider_error("anthropic: model overloaded"));
        assert!(Agent::is_transient_provider_error("please try again shortly"));
    }

    #[test]
    fn does_not_match_status_codes_inside_model_versions() {
        // "gpt-5" contains "5" but must not register as a 5xx status.
        assert!(!Agent::is_transient_provider_error(
            "model gpt-5 does not exist"
        ));
    }

    #[test]
    fn extract_retry_after_from_openrouter_response() {
        let error = r#"status: 429 Too Many Requests
  response: {"error":{"metadata":{"retry_after_seconds":60,"retry_after_seconds_raw":60}}}"#;
        assert_eq!(extract_retry_after_secs(error), Some(60));
    }

    #[test]
    fn extract_retry_after_prefers_raw_field() {
        let error = r#"retry_after_seconds_raw":30,"retry_after_seconds":60"#;
        assert_eq!(extract_retry_after_secs(error), Some(30));
    }

    #[test]
    fn extract_retry_after_none_when_missing() {
        assert_eq!(extract_retry_after_secs("no retry info here"), None);
    }

    #[test]
    fn retry_delay_uses_provider_hint() {
        let error = r#"retry_after_seconds":45"#;
        assert_eq!(retry_delay_for_error(error, 1), 45);
    }

    #[test]
    fn retry_delay_caps_at_180s() {
        let error = r#"retry_after_seconds":300"#;
        assert_eq!(retry_delay_for_error(error, 1), 180);
    }

    #[test]
    fn retry_delay_uses_backoff_when_no_hint() {
        let d1 = retry_delay_for_error("no hint", 1);
        let d2 = retry_delay_for_error("no hint", 2);
        let d3 = retry_delay_for_error("no hint", 3);
        assert!(d1 >= 5, "minimum delay should be >= 5s, got {d1}");
        assert!(d1 < d2, "backoff should increase: {d1} < {d2}");
        assert!(d2 < d3, "backoff should increase: {d2} < {d3}");
    }

    #[test]
    fn contains_429_with_rate_limit_matches_common_patterns() {
        assert!(contains_429_with_rate_limit("status: 429 too many requests"));
        assert!(contains_429_with_rate_limit("status: 429 rate limit exceeded"));
        assert!(contains_429_with_rate_limit("status: 429 temporarily rate-limited"));
        assert!(contains_429_with_rate_limit("status: 429 throttled"));
        assert!(contains_429_with_rate_limit("status: 429 quota exceeded"));
        assert!(!contains_429_with_rate_limit("status: 429 moderation hit"));
    }
}
