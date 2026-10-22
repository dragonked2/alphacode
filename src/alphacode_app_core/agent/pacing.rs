//! Inter-call pacing between provider requests within a single turn.
//!
//! # The problem
//! The turn loop used to `sleep(500ms)` unconditionally after every tool
//! batch, on every provider, including the final iteration of the turn. The
//! comment justified it with "free models on OpenRouter have strict
//! per-minute limits (~10 RPM)" — but the sleep was paid by paid providers
//! with no such limit, and by the last iteration of a turn where there is no
//! next request to pace.
//!
//! On a 10-iteration turn that is 5 seconds of pure dead time, on every
//! provider.
//!
//! # The mechanism
//! [`inter_call_delay`] returns the delay to apply *before the next provider
//! request*:
//!
//! - `None` — no delay. Paid providers, and the final iteration of a turn.
//! - `Some(duration)` — the pacing delay for providers that are actually
//!   rate-limited (the free pool).
//!
//! The free pool is the only provider family AlphaCode knows to be metered
//! per-minute, and it is already identified by
//! [`is_free_tier_model_id`]. We use that rather than a provider-name
//! guesswork so the pacing decision follows the same rule the free-pool
//! rotation and failover logic already use.

use std::time::Duration;

use crate::alphacode_provider_metadata::free_pool::is_free_tier_model_id;

/// The delay applied between provider requests on a rate-limited provider.
///
/// 500ms keeps a 10-RPM free model under its cap with headroom, matching the
/// delay this module replaces. It is now applied *only* where it is needed.
pub const FREE_TIER_INTER_CALL_DELAY: Duration = Duration::from_millis(500);

/// How long to wait before the next provider request in this turn.
///
/// `model` is the model id that will serve the next request. `has_more_requests`
/// is false on the last iteration of the turn, where there is no next request
/// to pace and the sleep would be pure dead time.
pub fn inter_call_delay(model: Option<&str>, has_more_requests: bool) -> Option<Duration> {
    if !has_more_requests {
        return None;
    }
    let model = model?;
    if is_free_tier_model_id(model) {
        Some(FREE_TIER_INTER_CALL_DELAY)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paid_models_are_not_paced() {
        assert_eq!(inter_call_delay(Some("gpt-5.5"), true), None);
        assert_eq!(inter_call_delay(Some("claude-opus-4-8"), true), None);
        assert_eq!(
            inter_call_delay(Some("anthropic.claude-3-5-sonnet-20241022-v2:0"), true),
            None
        );
    }

    #[test]
    fn free_tier_models_are_paced() {
        assert_eq!(
            inter_call_delay(Some("kilo-auto/free"), true),
            Some(FREE_TIER_INTER_CALL_DELAY)
        );
        assert_eq!(
            inter_call_delay(Some("deepseek-v4-flash:free"), true),
            Some(FREE_TIER_INTER_CALL_DELAY)
        );
    }

    #[test]
    fn the_final_iteration_is_never_paced() {
        // Even a free-tier model must not sleep before the turn ends.
        assert_eq!(inter_call_delay(Some("kilo-auto/free"), false), None);
    }

    #[test]
    fn an_unknown_model_is_not_paced() {
        assert_eq!(inter_call_delay(None, true), None);
        assert_eq!(inter_call_delay(Some(""), true), None);
    }
}
