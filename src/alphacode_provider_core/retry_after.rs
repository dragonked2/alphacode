//! Shared handling for provider `Retry-After` hints.
//!
//! Provider runtimes keep their own retry classification and request logic, but
//! parsing an untrusted server delay and carrying it through an `anyhow::Error`
//! should be consistent. Preserve long cooldowns: an automatic wait limit is
//! a reason to stop retrying, never permission to send before the deadline.

use anyhow::Error;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::fmt;
use std::time::{Duration, Instant, SystemTime};

/// Tracking horizon for malformed/overflowing hints (100 years).
/// Real provider cooldowns are preserved; automatic waits are bounded separately.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);
pub const MAX_AUTOMATIC_WAIT: Duration = Duration::from_secs(180);

/// Whether a retry would park a turn too long. Return the error to the user
/// while retaining the cooldown for subsequent requests and background work.
pub fn requires_manual_retry(error: &Error) -> bool {
    retry_after_from_error(error)
        .or_else(|| retry_after_seconds_from_text(&format!("{error:#}")).map(Duration::from_secs))
        .is_some_and(|hint| hint > MAX_AUTOMATIC_WAIT)
}

/// Read body hints without dropping long or overflowing positive values.
pub fn retry_after_seconds_from_text(error: &str) -> Option<u64> {
    for needle in ["retry_after_seconds_raw\":", "retry_after_seconds\":"] {
        for (position, _) in error.match_indices(needle) {
            let after = error[position + needle.len()..]
                .trim_start()
                .trim_start_matches('"');
            let digits = after.bytes().take_while(u8::is_ascii_digit);
            let seconds = digits.fold(0_u64, |seconds, byte| {
                seconds
                    .saturating_mul(10)
                    .saturating_add(u64::from(byte - b'0'))
            });
            if seconds > 0 {
                return Some(seconds.min(MAX_RETRY_AFTER.as_secs()));
            }
        }
    }
    None
}

/// Parse a `Retry-After` header as delta-seconds or an HTTP date.
///
/// Numeric values are parsed with saturation and then capped, so even an
/// arbitrarily long digit string is safe. Invalid values are ignored and let
/// the caller fall back to its normal exponential backoff.
pub fn retry_after(headers: &HeaderMap) -> Option<RetryAfter> {
    retry_after_delay_at(headers, SystemTime::now()).map(RetryAfter::new)
}

fn retry_after_delay_at(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }

    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        let max_secs = MAX_RETRY_AFTER.as_secs();
        let seconds = value.bytes().fold(0u64, |seconds, byte| {
            seconds
                .saturating_mul(10)
                .saturating_add(u64::from(byte - b'0'))
                .min(max_secs)
        });
        return Some(Duration::from_secs(seconds));
    }

    let retry_at = httpdate::parse_http_date(value).ok()?;
    Some(
        retry_at
            .duration_since(now)
            .unwrap_or(Duration::ZERO)
            .min(MAX_RETRY_AFTER),
    )
}

/// A bounded server retry hint represented as a monotonic deadline.
#[derive(Clone, Copy, Debug)]
pub struct RetryAfter {
    deadline: Instant,
}

impl RetryAfter {
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            deadline: Instant::now() + delay.min(MAX_RETRY_AFTER),
        }
    }

    /// Time still remaining on the hint. Time spent reading and classifying an
    /// error response counts toward the requested wait.
    pub fn remaining(self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

/// Error wrapper that preserves the provider's user-facing message while
/// carrying a parsed server retry deadline to the outer retry loop.
#[derive(Debug)]
struct RetryAfterError {
    message: String,
    retry_after: RetryAfter,
}

impl fmt::Display for RetryAfterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RetryAfterError {}

/// Build an error with an optional server retry hint without changing its
/// display text.
pub fn error_with_retry_after(message: String, retry_after: Option<RetryAfter>) -> Error {
    let body_hint = retry_after_seconds_from_text(&message).map(Duration::from_secs);
    let retry_after = match (retry_after, body_hint) {
        (Some(header), Some(body)) if body > header.remaining() => Some(RetryAfter::new(body)),
        (None, Some(body)) => Some(RetryAfter::new(body)),
        (header, _) => header,
    };
    match retry_after {
        Some(retry_after) => Error::new(RetryAfterError {
            message,
            retry_after,
        }),
        None => Error::msg(message),
    }
}

/// Recover a server retry hint from a provider error, including through anyhow
/// context layers.
pub fn retry_after_from_error(error: &Error) -> Option<Duration> {
    error
        .chain()
        .find_map(|source| source.downcast_ref::<RetryAfterError>())
        .map(|error| error.retry_after.remaining())
}

/// Select the delay before a retry, preferring a validated server hint over
/// the provider's normal jittered exponential backoff.
pub fn retry_delay(attempt: u32, base_ms: u64, server_hint: Option<Duration>) -> Duration {
    server_hint.unwrap_or_else(|| {
        crate::alphacode_provider_core::attempt_tracker::retry_backoff_delay(attempt, base_ms)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    fn headers(value: HeaderValue) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, value);
        headers
    }

    #[test]
    fn parses_delta_seconds_without_sleeping() {
        assert_eq!(
            retry_after_delay_at(
                &headers(HeaderValue::from_static("7")),
                SystemTime::UNIX_EPOCH,
            ),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn parses_http_date_relative_to_injected_clock() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let retry_at = now + Duration::from_secs(12);
        let value = HeaderValue::from_str(&httpdate::fmt_http_date(retry_at)).unwrap();
        assert_eq!(
            retry_after_delay_at(&headers(value), now),
            Some(Duration::from_secs(12))
        );
    }

    #[test]
    fn past_http_date_requests_no_additional_wait() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let value =
            HeaderValue::from_str(&httpdate::fmt_http_date(now - Duration::from_secs(30))).unwrap();
        assert_eq!(
            retry_after_delay_at(&headers(value), now),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn hour_long_http_date_is_preserved() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let value =
            HeaderValue::from_str(&httpdate::fmt_http_date(now + Duration::from_secs(3_600)))
                .unwrap();
        assert_eq!(
            retry_after_delay_at(&headers(value), now),
            Some(Duration::from_secs(3_600))
        );
    }

    #[test]
    fn malformed_retry_after_is_ignored() {
        assert_eq!(
            retry_after_delay_at(
                &headers(HeaderValue::from_static("not-a-delay")),
                SystemTime::UNIX_EPOCH,
            ),
            None
        );
    }

    #[test]
    fn tokenharbor_cooldown_survives_context_and_requires_manual_retry() {
        let hint = retry_after(&headers(HeaderValue::from_static("3541")));
        let error =
            error_with_retry_after("HTTP 429 rate limit".into(), hint).context("provider failed");
        let remaining = retry_after_from_error(&error).unwrap();
        assert!(remaining > Duration::from_secs(3540));
        assert!(requires_manual_retry(&error));
        assert_eq!(
            retry_after_seconds_from_text("\"retry_after_seconds\":3541"),
            Some(3541)
        );
        assert_eq!(
            retry_after_seconds_from_text(
                "\"retry_after_seconds_raw\":\"999999999999999999999999\""
            ),
            Some(MAX_RETRY_AFTER.as_secs())
        );
    }

    #[test]
    fn oversized_retry_after_is_capped_even_when_it_overflows_u64() {
        let value = HeaderValue::from_static("999999999999999999999999999999999999999999");
        assert_eq!(
            retry_after_delay_at(&headers(value), SystemTime::UNIX_EPOCH),
            Some(MAX_RETRY_AFTER)
        );
    }

    #[test]
    fn error_hint_round_trips_without_changing_message() {
        let error = error_with_retry_after(
            "rate limited".to_string(),
            Some(RetryAfter::new(Duration::from_secs(9))),
        )
        .context("request failed");
        assert_eq!(format!("{error:#}"), "request failed: rate limited");
        let remaining = retry_after_from_error(&error).unwrap();
        assert!(remaining <= Duration::from_secs(9));
        assert!(remaining > Duration::from_secs(8));
    }

    #[test]
    fn server_hint_replaces_backoff_without_sleeping() {
        assert_eq!(
            retry_delay(3, 10_000, Some(Duration::from_secs(4))),
            Duration::from_secs(4)
        );
    }

    #[test]
    fn elapsed_hint_does_not_add_another_wait() {
        let retry_after = RetryAfter {
            deadline: Instant::now() - Duration::from_secs(1),
        };
        let error = error_with_retry_after("rate limited".to_string(), Some(retry_after));
        assert_eq!(retry_after_from_error(&error), Some(Duration::ZERO));
    }
}
