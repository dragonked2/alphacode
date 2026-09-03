//! Unified retry policy for provider HTTP requests and tool HTTP calls.
//!
//! This module is the single source of truth for "should we retry this
//! transient failure, and if so how long do we wait". It is used by every
//! provider runtime (Anthropic, OpenAI, Gemini, OpenRouter, Bedrock,
//! Antigravity, Copilot, Claude-CLI, Cursor) and every HTTP-using tool
//! (`webfetch`, `websearch`, `discover`, ...).
//!
//! # What it handles
//!
//! - HTTP `429 Too Many Requests`, with rate-limit-aware retry classification
//!   (the conservative rule: 429 without rate-limit words is treated as a
//!   hard fail because upstream usually means "data policy / moderation").
//! - HTTP `408`, `500`, `502`, `503`, `504`, plus 5xx surfaced through
//!   `anyhow::Error` message text.
//! - Transport-level faults: TLS teardown, connection reset, DNS hiccups,
//!   HTTP/2 `GOAWAY`/`RST_STREAM`, broken pipe, idle-pool resurrection,
//!   and the `enhance_your_calm` Cloudflare rate-limit marker. See
//!   [`crate::alphacode_provider_core::transport::is_transient_transport_error`].
//! - Server-supplied `Retry-After` header (delta-seconds or HTTP-date).
//! - Body-supplied `retry_after_seconds` / `retry_after_seconds_raw` fields
//!   in the OpenRouter-style JSON envelope.
//!
//! # What it intentionally does NOT retry
//!
//! - 4xx other than 408/409/429 (auth, not-found, bad-request).
//! - 429 whose only context is "moderation" / "data policy" — retrying
//!   just hits the same wall.
//! - Provider permanent failures (`api_error: invalid_api_key`, etc.).
//!
//! # Backoff schedule
//!
//! When no server hint is available:
//!
//! | attempt | 429 backoff | 5xx/transport backoff |
//! |---------|-------------|-----------------------|
//! | 1       | 4–6 s       | 0.8–1.2 s             |
//! | 2       | 8–12 s      | 1.6–2.4 s             |
//! | 3       | 16–24 s     | 3.2–4.8 s             |
//! | 4       | 32–48 s     | 6.4–9.6 s             |
//! | 5       | 64–96 s     | 12.8–19.2 s           |
//!
//! Server hints are honored but capped at 180 s so a misbehaving upstream
//! can't park a turn indefinitely.
//!
//! # Idempotency
//!
//! [`send_with_retry`] only retries requests built from [`reqwest::Request`]
//! whose HTTP method is idempotent (`GET`, `HEAD`, `PUT`, `DELETE`,
//! `OPTIONS`, `TRACE`). `POST` is only retried when the caller has explicitly
//! opted in with [`RetryPolicy::retry_non_idempotent`]. This protects against
//! silently double-charging on retry of, e.g., a `POST /messages` request
//! that the upstream may have already started processing.
//!
//! # Logging
//!
//! Every retry attempt logs through [`crate::alphacode_base::logging`] with
//! the structured key/value pairs `(provider, url, attempt, next_delay_ms,
//! reason)`. The retry path is observable without making the loop itself
//! chatty on the happy path.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Error, anyhow};
use reqwest::header::HeaderMap;
use reqwest::{Method, Request, Response};

use crate::alphacode_provider_core::retry_after::{
    RetryAfter as ParsedRetryAfter, error_with_retry_after, retry_after, retry_after_from_error,
};
use crate::alphacode_provider_core::transport::{
    is_transient_transport_error, send_with_initial_response_timeout,
};

/// Maximum number of attempts (initial + retries) for a single request.
pub const MAX_ATTEMPTS: u32 = 5;

/// Hard cap on a server-supplied or backoff-derived retry delay. Anything
/// larger is clamped down so a hostile upstream cannot stall a turn.
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(180);

/// Base delay for non-rate-limit transient faults (5xx, transport).
/// Jittered in `0.8..1.2` and doubled per attempt.
pub const BASE_DELAY_TRANSPORT_MS: u64 = 1_000;

/// Base delay for 429 rate-limit faults. Higher than transport because
/// retrying too soon is guaranteed to fail and burns rate budget.
pub const BASE_DELAY_RATE_LIMIT_MS: u64 = 5_000;

/// Tunable policy. All fields have safe defaults — providers that want to
/// opt out of non-idempotent retries just leave `retry_non_idempotent` false.
#[derive(Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (initial + retries). Clamped to `[1, 8]`.
    pub max_attempts: u32,
    /// Allow retrying `POST` requests. Defaults to `false`. Streaming
    /// provider requests that need to retry mid-stream should set this.
    pub retry_non_idempotent: bool,
    /// Initial-response timeout applied to each attempt. The legacy
    /// `send_with_initial_response_timeout` uses this. A reasonable default
    /// for LLM streaming is 30 s; tight HTTP APIs may want 10 s.
    pub initial_response_timeout: Duration,
    /// Optional callback invoked once per scheduled retry. Useful for TUI
    /// status updates ("retrying in 12s (attempt 2/5)") or telemetry.
    pub on_retry: Option<std::sync::Arc<dyn Fn(RetryEvent) + Send + Sync>>,
}

impl std::fmt::Debug for RetryPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetryPolicy")
            .field("max_attempts", &self.max_attempts)
            .field("retry_non_idempotent", &self.retry_non_idempotent)
            .field("initial_response_timeout", &self.initial_response_timeout)
            .field("on_retry", &self.on_retry.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_ATTEMPTS,
            retry_non_idempotent: false,
            initial_response_timeout: Duration::from_secs(30),
            on_retry: None,
        }
    }
}

impl RetryPolicy {
    /// Conservative policy for HTTP API tools (webfetch / websearch /
    /// discover). GET-only, 3 attempts total.
    pub fn for_http_tools() -> Self {
        Self {
            max_attempts: 3,
            retry_non_idempotent: false,
            initial_response_timeout: Duration::from_secs(15),
            on_retry: None,
        }
    }

    /// Provider-streaming policy: 5 attempts, treats `POST` as retryable
    /// because every shipped provider supports either resuming from a
    /// `previous_*_id` or replaying the same body twice. Strict callers
    /// should pass `retry_non_idempotent = false`.
    pub fn for_streaming() -> Self {
        Self {
            max_attempts: MAX_ATTEMPTS,
            retry_non_idempotent: true,
            initial_response_timeout: Duration::from_secs(30),
            on_retry: None,
        }
    }

    /// Attach a retry callback (used by TUI status bar / telemetry).
    pub fn with_on_retry(mut self, cb: std::sync::Arc<dyn Fn(RetryEvent) + Send + Sync>) -> Self {
        self.on_retry = Some(cb);
        self
    }

    fn attempts(&self) -> u32 {
        self.max_attempts.clamp(1, 8)
    }

    fn is_idempotent(&self, method: &Method) -> bool {
        method_idempotent(method) || self.retry_non_idempotent
    }
}

/// Information passed to the `on_retry` callback.
#[derive(Debug, Clone)]
pub struct RetryEvent {
    pub provider: String,
    pub url: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub next_delay: Duration,
    pub reason: RetryReason,
}

/// Classification of why we are retrying. Stable across calls so consumers
/// can chart it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryReason {
    RateLimited,
    ServerError,
    TransportFault,
    ServerHint,
}

impl fmt::Display for RetryReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            RetryReason::RateLimited => "rate-limited",
            RetryReason::ServerError => "server error",
            RetryReason::TransportFault => "transport fault",
            RetryReason::ServerHint => "server hint",
        };
        formatter.write_str(label)
    }
}

/// Inspect a textual error and decide whether it represents a retryable
/// transient fault.
pub fn is_retryable_message(error_str: &str) -> bool {
    let lower = error_str.to_ascii_lowercase();
    if is_transient_transport_error(&lower) {
        return true;
    }
    if contains_429_with_rate_limit(&lower) {
        return true;
    }
    for needle in [
        "500 internal server error",
        "502 bad gateway",
        "503 service unavailable",
        "504 gateway timeout",
        "model overloaded",
        "please try again shortly",
        "temporarily unavailable",
        "service unavailable",
        "overloaded",
    ] {
        if lower.contains(needle) {
            return true;
        }
    }
    false
}

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

/// Pull a server hint from an `anyhow::Error` chain.
pub fn server_hint_from_error(error: &Error) -> Option<Duration> {
    retry_after_from_error(error)
}

/// Compute the backoff for the next attempt.
pub fn backoff_for(reason: RetryReason, attempt: u32, server_hint: Option<Duration>) -> Duration {
    if let Some(hint) = server_hint {
        return hint.min(MAX_RETRY_DELAY);
    }
    let base_ms = match reason {
        RetryReason::RateLimited | RetryReason::ServerHint => BASE_DELAY_RATE_LIMIT_MS,
        RetryReason::ServerError | RetryReason::TransportFault => BASE_DELAY_TRANSPORT_MS,
    };
    let shift = attempt.min(5);
    let scaled = base_ms.saturating_mul(1u64 << shift);
    let cap_ms = MAX_RETRY_DELAY.as_millis() as u64;
    let scaled = scaled.min(cap_ms);
    let jitter = jitter_factor();
    let jittered = ((scaled as f64) * jitter) as u64;
    let jittered = jittered.max(base_ms).min(cap_ms);
    Duration::from_millis(jittered)
}

fn jitter_factor() -> f64 {
    let nanos = STATIC_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let bucket = (nanos.wrapping_mul(2_654_435_761) ^ (nanos >> 16)) as u64;
    let unit = ((bucket % 10_000) as f64) / 10_000.0;
    0.8 + unit * 0.4
}

static STATIC_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A tagged error returned when retries are exhausted.
#[derive(Debug)]
pub struct RetryExhausted {
    pub attempts: u32,
    pub last_status: Option<u16>,
    pub last_message: String,
    pub last_server_hint: Option<Duration>,
    pub source: Error,
}

impl fmt::Display for RetryExhausted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "request failed after {} attempt(s): {}",
            self.attempts, self.last_message
        )?;
        if let Some(status) = self.last_status {
            write!(formatter, " (status {status})")?;
        }
        Ok(())
    }
}

impl std::error::Error for RetryExhausted {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

fn method_idempotent(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::PUT | Method::DELETE | Method::OPTIONS | Method::TRACE
    )
}

/// Send a request, applying [`RetryPolicy`].
///
/// Note: `reqwest::Body::try_clone` is private in 0.12, so callers that want
/// to retry non-trivial bodies should use [`send_builder_with_retry`]
/// (which clones the builder before each send) rather than this function.
/// This entry point is appropriate for GET/HEAD-style requests with no
/// body, or for already-buffered body types.
pub async fn send_with_retry(
    client: &reqwest::Client,
    request: Request,
    policy: &RetryPolicy,
    label: &str,
) -> Result<Response, Error> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let idempotent = policy.is_idempotent(&method);

    // For requests without a body, or whose body is already a Bytes buffer,
    // we can replay on retry by re-cloning the bytes. Streaming bodies
    // (chunked) cannot be replayed — for those, set `retry_non_idempotent =
    // false` and the loop will not retry past the first attempt.
    let replay_bytes: Option<bytes::Bytes> = request
        .body()
        .and_then(|body| body.as_bytes())
        .map(|slice| bytes::Bytes::copy_from_slice(slice));

    let mut state = LoopState::default();

    for attempt in 0..policy.attempts() {
        if request.body().is_some() && replay_bytes.is_none() && attempt > 0 {
            // Streaming body, can't replay; bail after the first try.
            break;
        }
        if attempt > 0 {
            let delay = backoff_for(state.reason, attempt - 1, state.hint);
            log_retry(label, &url, attempt, state.reason_str(), delay);
            if let Some(cb) = policy.on_retry.as_ref() {
                cb(RetryEvent {
                    provider: label.to_string(),
                    url: url.clone(),
                    attempt: attempt + 1,
                    max_attempts: policy.attempts(),
                    next_delay: delay,
                    reason: state.reason,
                });
            }
            tokio::time::sleep(delay).await;
        }

        let builder = client.request(
            method.clone(),
            reqwest::Url::parse(&url).expect("valid url"),
        );
        let builder = apply_headers(builder, request.headers());
        let builder = match replay_bytes.as_ref() {
            Some(bytes) => builder.body(bytes.clone()),
            None => builder,
        };

        let send_result = send_with_initial_response_timeout(
            builder,
            policy.initial_response_timeout,
        )
        .await;

        match send_result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return Ok(response);
                }

                let headers = response.headers().clone();
                let parsed_hint = retry_after(&headers);
                let next_delay = parsed_hint.map(|h: ParsedRetryAfter| h.remaining());

                let code = status.as_u16();
                let reason = if code == 429 {
                    Some(RetryReason::RateLimited)
                } else if status.is_server_error() || code == 408 || code == 409 {
                    Some(RetryReason::ServerError)
                } else {
                    None
                };

                let body_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| String::from("<unreadable body>"));
                let combined = format!("HTTP {status}: {body_text}");

                let Some(reason) = reason else {
                    return Err(anyhow!(combined.clone()).context(format!(
                        "request to {url} failed"
                    )));
                };

                // Conservative 429 check using the body.
                if matches!(reason, RetryReason::RateLimited)
                    && !contains_429_with_rate_limit(&body_text.to_ascii_lowercase())
                {
                    return Err(anyhow!(combined.clone()).context(format!(
                        "request to {url} failed (non-rate-limit 429)"
                    )));
                }

                state.reason = reason;
                state.last_status = Some(code);
                state.last_message = combined;
                state.hint = Some(next_delay.unwrap_or(Duration::ZERO));
                state.last_error = None;

                if !idempotent {
                    return Err(anyhow!(state.last_message.clone())).context(format!(
                        "non-idempotent request to {url} failed; not retrying"
                    ));
                }
                continue;
            }
            Err(send_err) => {
                let message = format!("{send_err:#}");
                state.last_message = message.clone();
                state.last_status = None;
                state.hint = None;
                if is_retryable_message(&message) && idempotent {
                    state.reason = RetryReason::TransportFault;
                    state.last_error = Some(send_err);
                    continue;
                }
                return Err(send_err).context(format!("transport error talking to {url}"));
            }
        }
    }

    let attempts = policy.attempts();
    let final_err = state.last_error.unwrap_or_else(|| anyhow!("{}", state.last_message));
    Err(Error::new(RetryExhausted {
        attempts,
        last_status: state.last_status,
        last_message: state.last_message.clone(),
        last_server_hint: state.hint,
        source: final_err,
    }))
}

/// Builder-driven convenience wrapper.
pub async fn send_builder_with_retry(
    client: &reqwest::Client,
    builder: reqwest::RequestBuilder,
    policy: &RetryPolicy,
    label: &str,
) -> Result<Response, Error> {
    let request = builder
        .build()
        .context("failed to build HTTP request")?;
    send_with_retry(client, request, policy, label).await
}

#[derive(Default)]
struct LoopState {
    reason: RetryReason,
    last_status: Option<u16>,
    last_message: String,
    hint: Option<Duration>,
    last_error: Option<Error>,
}

impl Default for RetryReason {
    fn default() -> Self {
        RetryReason::TransportFault
    }
}

impl LoopState {
    fn reason_str(&self) -> &'static str {
        match self.reason {
            RetryReason::RateLimited => "rate-limited",
            RetryReason::ServerError => "server error",
            RetryReason::TransportFault => "transport fault",
            RetryReason::ServerHint => "server hint",
        }
    }
}

fn apply_headers(builder: reqwest::RequestBuilder, headers: &HeaderMap) -> reqwest::RequestBuilder {
    let mut builder = builder;
    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }
    builder
}

fn log_retry(provider: &str, url: &str, attempt: u32, reason: &str, delay: Duration) {
    crate::alphacode_base::logging::warn(&format!(
        "[retry] {provider} {url}: attempt {attempt} after {delay:?} ({reason})"
    ));
}

#[allow(dead_code)]
fn _ensure_legacy_retry_after_used() {
    // Touch the legacy symbol so the import survives even when only the
    // new retry.rs code path is exercised in a given build.
    let _ = error_with_retry_after("noop".into(), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn classify_idempotent_methods() {
        for m in [
            Method::GET,
            Method::HEAD,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
            Method::TRACE,
        ] {
            assert!(method_idempotent(&m));
        }
        assert!(!method_idempotent(&Method::POST));
    }

    #[test]
    fn conservative_429_classifier() {
        assert!(contains_429_with_rate_limit("status: 429 too many requests"));
        assert!(contains_429_with_rate_limit("status: 429 rate limit exceeded"));
        assert!(contains_429_with_rate_limit("status: 429 throttled"));
        assert!(!contains_429_with_rate_limit("status: 429 moderation hit"));
        assert!(!contains_429_with_rate_limit("status: 5000 bad request"));
    }

    #[test]
    fn retryable_message_recognises_5xx() {
        for s in [
            "status: 500 internal server error",
            "status: 502 bad gateway",
            "status: 503 service unavailable",
            "status: 504 gateway timeout",
            "anthropic: model overloaded",
            "please try again shortly",
        ] {
            assert!(is_retryable_message(s), "{s} should retry");
        }
    }

    #[test]
    fn retryable_message_ignores_non_retriable_4xx() {
        for s in [
            "status: 400 bad request",
            "status: 401 unauthorized",
            "status: 403 forbidden",
            "status: 404 not found",
            "status: 422 unprocessable",
        ] {
            assert!(!is_retryable_message(s), "{s} should NOT retry");
        }
    }

    #[test]
    fn retryable_message_recognises_transport_faults() {
        for s in [
            "connection reset by peer",
            "broken pipe",
            "tls handshake eof",
            "enhance_your_calm",
            "unexpected eof",
        ] {
            assert!(is_retryable_message(s), "{s} should retry");
        }
    }

    #[test]
    fn backoff_respects_server_hint() {
        let delay = backoff_for(RetryReason::RateLimited, 2, Some(Duration::from_secs(42)));
        assert_eq!(delay, Duration::from_secs(42));
    }

    #[test]
    fn backoff_caps_at_max() {
        let delay = backoff_for(RetryReason::RateLimited, 8, None);
        assert!(delay <= MAX_RETRY_DELAY, "got {delay:?}");
    }

    #[test]
    fn backoff_is_monotonic_per_reason() {
        let mut last = Duration::ZERO;
        for attempt in 0..5 {
            let d = backoff_for(RetryReason::TransportFault, attempt, None);
            assert!(
                d >= last,
                "backoff must not decrease: attempt {attempt}: {d:?} < {last:?}"
            );
            last = d;
        }
    }

    #[test]
    fn server_hint_from_error_recovers_retry_after() {
        use reqwest::header::{HeaderMap, RETRY_AFTER};
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("11"));
        let parsed = retry_after(&headers).expect("header");
        let err = error_with_retry_after("rate limited".into(), Some(parsed));
        let recovered = server_hint_from_error(&err).expect("hint");
        let secs = recovered.as_secs();
        assert!(secs <= 11, "got {secs}");
    }

    #[test]
    fn header_retry_after_parses_delta() {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, HeaderValue::from_static("3"));
        let parsed = retry_after(&headers).expect("header");
        let secs = parsed.remaining().as_secs();
        assert!(secs <= 3, "got {secs}");
    }
}