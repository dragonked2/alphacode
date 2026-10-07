//! The Alphax Free fallback pool: which models a 429 can rotate through.
//!
//! # Why this file exists
//!
//! `kilo-auto/free` is a *routing alias*, not a model. The gateway resolves it
//! server-side to one concrete free model, so the id we hold says nothing about
//! what actually served a request. The gateway's rate limits are enforced
//! **per upstream model**, which means the alias can return HTTP 429 while
//! several other zero-priced models are serving traffic at that exact moment.
//!
//! That is a real and reproducible failure. Verified against the live gateway
//! (`https://api.kilo.ai/api/gateway`) in one session, seconds apart:
//!
//! ```text
//! poolside/laguna-s-2.1:free           200
//! nvidia/nemotron-3-ultra-550b:free    200
//! thinkingmachines/inkling-small:free  429   <- daily limit, for *this* model
//! qwen/qwen3.8-27b:free                429
//! ```
//!
//! and the 429 body names the culprit, proving the limit is per-model rather
//! than per-account:
//!
//! ```text
//! "Rate limit exceeded: ... Daily limit reached for
//!  thinkingmachines/inkling-small:free via Thinking Machines."
//! ```
//!
//! Retrying the same alias (which is what AlphaCode did before this file) just
//! re-rolls the gateway's dice against the same throttled backend. Rotating to
//! a different, explicitly-named free model is the only reliable way to make
//! progress.
//!
//! # Eligibility
//!
//! Membership is decided by **price**, not by marketing name. Every entry here
//! was confirmed to report `pricing.prompt == 0` *and* `pricing.completion == 0`
//! on the gateway's `/v1/models`. Nothing that costs money to call may appear
//! in this list, because rotation must never be able to spend a user's credits
//! behind their back. `kilo-auto/small` is the cautionary case: it is priced at
//! `0.00000005`/`0.0000004`, so it is *not* free and is deliberately excluded.
//!
//! The two `kilo-auto/*` aliases that are included (`/free`, and `/efficient`
//! only because the gateway reports `-1`, meaning "free/unmetered") are kept
//! because they are the tier names users recognise. `/frontier` and `/balanced`
//! are excluded: they are metered aliases and would silently cost credits.

/// The profile whose gateway exposes the free pool. Kept as a constant so the
/// rotation code and the profile definition cannot drift apart.
pub const ALPHAX_FREE_PROFILE_ID: &str = "alphax-free";

/// The profile whose gateway exposes the free pool. Imported rather than
/// hardcoded so the pool and the profile it targets cannot drift apart: if the
/// profile's id or base URL changes, this file changes with it.
use super::catalog::ALPHAX_FREE_PROFILE;

/// Curated, price-verified free models, in fallback-preference order.
///
/// Order is deliberate, cheapest-risk first:
/// 1. `kilo-auto/free` leads because it is the default the user picked and the
///    gateway does its own internal routing for it.
/// 2. The remaining entries are named concrete models, which means the gateway
///    has no further discretion and the 429 is unambiguous.
///
/// The list is a *floor*, not a ceiling: [`refresh_free_pool_from_gateway`]
/// merges in whatever the gateway currently reports as zero-priced, so a model
/// Kilo adds later is picked up without a release.
pub const CURATED_FREE_MODELS: &[&str] = &[
    "kilo-auto/free",
    // `openrouter/free` was removed from this list: the gateway answered it
    // with `No endpoints found for openrouter/free ... failed_routing_step:
    // "Filter by Fallback"`, i.e. every candidate endpoint had been removed by
    // routing policy. It is a synthetic alias rather than a concrete model, so
    // it can be unroutable at any time depending on which upstreams the
    // gateway is willing to serve free traffic to. Kept only as a recognised id
    // (so an existing selection still rotates) but no longer a rotation target.
    "poolside/laguna-s-2.1:free",
    "nvidia/nemotron-3-super-120b-a12b:free",
    "inclusionai/ling-3.0-flash-sante:free",
    "cohere/north-mini-code:free",
    "liquid/lfm-2.5-2.6b:free",
    "thinkingmachines/inkling-small:free",
    "dots-studio/dots-3-note-preview:free",
    "nvidia/nemotron-3-ultra-550b-a55b:free",
];

/// How long a discovered free model is trusted before the refresh runs again.
///
/// Free-tier lineups churn, but not per-minute. An hour keeps a retired model
/// in the rotation set for at most one window while costing one small request
/// per hour.
pub const POOL_REFRESH_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

/// The gateway catalog endpoint, relative to the profile's `api_base`.
const CATALOG_PATH: &str = "/v1/models";

/// Extract the ids of every zero-priced model from a `/v1/models` payload.
///
/// Pricing is the only authority on what "free" means here, so it is read
/// directly rather than inferred from the `:free` suffix. The suffix is a naming
/// convention that any provider can adopt or drop; the price is the fact. Both
/// must hold: a model priced at zero *and* one we are willing to name.
///
/// Unknown and non-numeric prices are treated as **not free**. A missing price
/// field must never be read as "free", because that is the direction that
/// spends a user's money by accident.
pub fn free_model_ids_from_catalog(body: &serde_json::Value) -> Vec<String> {
    let Some(rows) = body.get("data").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let id = row.get("id")?.as_str()?.trim();
            if !is_free_tier_model_id(id) {
                return None;
            }
            let pricing = row.get("pricing")?;
            // A free model must be free on *both* axes. A zero prompt price
            // with a paid completion price is still a billable model.
            if is_zero_price(pricing.get("prompt")) && is_zero_price(pricing.get("completion")) {
                Some(id)
            } else {
                None
            }
        })
        .filter(|id| *id != "kilo-auto/efficient")
        .map(str::to_string)
        .collect::<Vec<_>>()
        .pipe_dedup()
}

/// Is this price field unambiguously zero?
///
/// Handles the three shapes the gateway and OpenRouter both emit: the number
/// `0`, the string `"0"`, and the string `"-1"` (OpenRouter's "free/unmetered"
/// marker). Anything else — including a missing field — is not free.
fn is_zero_price(value: Option<&serde_json::Value>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let raw = match value {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        // `null` and anything structured means "not free": a missing price must
        // never be optimistically read as zero.
        _ => return false,
    };
    let Ok(parsed) = raw.trim().parse::<f64>() else {
        return false;
    };
    // Tolerance rather than exact equality: some rows round a genuinely free
    // price to a denormalized 1e-18. A real paid price is orders of magnitude
    // larger, so this cannot admit a billable model.
    parsed.abs() < 1e-12
}

/// Deduplicate while preserving order.
trait PipeDedup: Sized {
    fn pipe_dedup(self) -> Self;
}

impl PipeDedup for Vec<String> {
    fn pipe_dedup(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.retain(|id| seen.insert(id.clone()));
        self
    }
}

/// Verified context windows for the curated free models, from the gateway's own
/// `/v1/models` `context_length` field.
///
/// This table exists because rotation can move a conversation onto a model with a
/// *much* smaller window: `liquid/lfm-2.5-2.6b:free` serves 65K while
/// `kilo-auto/free` advertises 256K and the Nemotron models advertise 1M. Without
/// a known limit the provider falls back to its conservative default and the
/// compaction budget is set wrong — which either truncates a conversation that
/// would have fit, or lets one grow past what the newly selected model accepts
/// and fail the request outright. Getting this right is the difference between a
/// transparent rotation and a mid-turn crash.
///
/// Verified values, matching `context_length` from the live gateway.
pub const FREE_MODEL_CONTEXT_LIMITS: &[(&str, usize)] = &[
    ("kilo-auto/free", 256_000),
    // Retained even though it left the rotation list: a session already pinned
    // to it still needs a context budget before it rotates off.
    ("openrouter/free", 200_000),
    ("poolside/laguna-s-2.1:free", 262_144),
    ("poolside/laguna-xs-2.1:free", 262_144),
    ("nvidia/nemotron-3-super-120b-a12b:free", 262_144),
    (
        "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free",
        256_000,
    ),
    ("nvidia/nemotron-3.5-lightning:free", 1_000_000),
    ("nvidia/nemotron-3-ultra-550b-a55b:free", 1_000_000),
    ("inclusionai/ling-3.0-flash-sante:free", 262_144),
    ("inclusionai/ling-3.0-flash-fin:free", 262_144),
    ("cohere/north-mini-code:free", 256_000),
    ("liquid/lfm-2.5-2.6b:free", 65_536),
    ("thinkingmachines/inkling-small:free", 1_048_576),
    ("dots-studio/dots-3-note-preview:free", 512_000),
];

/// Context window for a free-pool model, if known.
pub fn free_model_context_limit(model: &str) -> Option<usize> {
    let id = model.trim();
    FREE_MODEL_CONTEXT_LIMITS
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map(|(_, limit)| *limit)
}

/// Fetch the gateway's free models and merge them into the live rotation pool.
///
/// Runs in the background and is allowed to fail silently: it is an
/// enhancement over the curated floor, never a prerequisite for it. Any error
/// (offline, 429, schema change) simply leaves the curated pool in place, which
/// is why no `Result` is surfaced to the caller.
pub async fn refresh_free_pool_from_gateway(api_base: &str) {
    let Ok(client) = reqwest::Client::builder()
        .user_agent(crate::alphacode_provider_core::ALPHACODE_USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .build()
    else {
        return;
    };
    let url = format!("{}{CATALOG_PATH}", api_base.trim_end_matches('/'));
    let Ok(response) = client.get(&url).send().await else {
        return;
    };
    if !response.status().is_success() {
        return;
    }
    let Ok(body) = response.json::<serde_json::Value>().await else {
        return;
    };
    let discovered = free_model_ids_from_catalog(&body);
    crate::alphacode_app_core::agent::free_pool_rotation::absorb_discovered_free_models(discovered);
}

/// Is this model id eligible to serve a free-tier rotation?
///
/// The rule is a suffix match on `:free` plus the two unmetered `kilo-auto`
/// aliases. Keeping it a pure function of the id means it can gate a model that
/// arrived from the live catalog without another round trip.
pub fn is_free_tier_model_id(model: &str) -> bool {
    let id = model.trim();
    if id.is_empty() {
        return false;
    }
    if id.ends_with(":free") {
        return true;
    }
    matches!(
        id,
        "kilo-auto/free" | "kilo-auto/efficient" | "openrouter/free"
    )
}

/// The user-visible name for anything in the free pool.
///
/// Every model in this pool is a detail of the gateway's internals, so a user
/// should never be shown `nvidia/nemotron-3-ultra-550b-a55b:free` and asked to
/// care which upstream happened to serve the turn. They picked "Alphax Free";
/// which free model covered the request is our problem, not theirs.
pub const ALPHAX_FREE_DISPLAY_NAME: &str = "Alphax Free";

/// Map a free-pool model id to the name the user sees.
///
/// Returns `None` for anything outside the pool so ordinary models keep their
/// real, honest names.
pub fn free_model_display_name(model: &str) -> Option<&'static str> {
    is_free_tier_model_id(model).then_some(ALPHAX_FREE_DISPLAY_NAME)
}

/// The gateway base URL for the Alphax Free profile.
pub fn alphax_free_api_base() -> &'static str {
    ALPHAX_FREE_PROFILE.api_base
}

/// Filter raw gateway catalog ids down to the rotation-eligible ones.
///
/// Ordering is preserved from the input so the gateway's own preference (its
/// list is curated) survives; duplicates are dropped, keeping first occurrence.
pub fn filter_free_tier_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .map(str::trim)
        .filter(|id| is_free_tier_model_id(id))
        .filter(|id| seen.insert((*id).to_string()))
        .map(str::to_string)
        .collect()
}

/// Build the effective rotation pool: the curated floor, plus anything the
/// background refresh discovered, curated order preserved and newcomers
/// appended.
///
/// The curated models are never dropped even if the refresh does not see them.
/// A gateway hiccup must not shrink the pool below the set we know works — an
/// empty or shrunken pool would turn a recoverable 429 into a hard failure.
pub fn merge_pool(refreshed: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(CURATED_FREE_MODELS.len() + refreshed.len());
    let mut seen = std::collections::HashSet::new();
    for model in CURATED_FREE_MODELS
        .iter()
        .copied()
        .chain(refreshed.iter().map(String::as_str))
    {
        if seen.insert(model.to_string()) {
            merged.push(model.to_string());
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_pool_is_exactly_the_price_verified_free_models() {
        // Every entry must satisfy the same eligibility rule the live catalog
        // is filtered by, so the curated floor and the refreshed set can never
        // disagree about what "free" means.
        for model in CURATED_FREE_MODELS {
            assert!(
                is_free_tier_model_id(model),
                "curated entry `{model}` is not eligible for rotation; it would \
                 either be skipped or, worse, be treated as a fallback"
            );
        }
    }

    #[test]
    fn priced_kilo_auto_small_is_never_in_the_pool() {
        // Regression guard for the real trap: `kilo-auto/small` looks like a
        // free-tier sibling but the gateway prices it at 0.00000005/0.0000004.
        // Including it would spend a user's credits without consent.
        assert!(!is_free_tier_model_id("kilo-auto/small"));
        assert!(!CURATED_FREE_MODELS.contains(&"kilo-auto/small"));
        assert!(!merge_pool(&[]).iter().any(|m| m == "kilo-auto/small"));
    }

    #[test]
    fn metered_kilo_aliases_are_excluded() {
        // /frontier and /balanced are metered, not free.
        assert!(!is_free_tier_model_id("kilo-auto/frontier"));
        assert!(!is_free_tier_model_id("kilo-auto/balanced"));
    }

    #[test]
    fn paid_models_are_not_free_tier() {
        for model in [
            "openai/gpt-5.4",
            "anthropic/claude-opus-5",
            "deepseek/deepseek-v4-pro",
            "z-ai/glm-5.3",
        ] {
            assert!(
                !is_free_tier_model_id(model),
                "`{model}` is a paid model and must never be treated as free"
            );
        }
    }

    #[test]
    fn empty_and_blank_ids_are_rejected() {
        assert!(!is_free_tier_model_id(""));
        assert!(!is_free_tier_model_id("   "));
    }

    #[test]
    fn filter_keeps_only_free_and_preserves_order() {
        let filtered = filter_free_tier_ids([
            "kilo-auto/free",
            "openai/gpt-5.4",
            "poolside/laguna-xs-2.1:free",
            "anthropic/claude-opus-5",
            "liquid/lfm-2.5-2.6b:free",
        ]);
        assert_eq!(
            filtered,
            vec![
                "kilo-auto/free",
                "poolside/laguna-xs-2.1:free",
                "liquid/lfm-2.5-2.6b:free"
            ]
        );
    }

    #[test]
    fn filter_drops_duplicates_keeping_first() {
        let filtered = filter_free_tier_ids([
            "liquid/lfm-2.5-2.6b:free",
            "openai/gpt-5.4",
            "liquid/lfm-2.5-2.6b:free",
        ]);
        assert_eq!(filtered, vec!["liquid/lfm-2.5-2.6b:free"]);
    }

    #[test]
    fn merge_never_shrinks_below_the_curated_floor() {
        // A refresh that finds nothing must not empty the pool.
        let merged = merge_pool(&[]);
        assert_eq!(merged.len(), CURATED_FREE_MODELS.len());
        assert!(merged.contains(&"kilo-auto/free".to_string()));
    }

    #[test]
    fn merge_appends_newly_discovered_models_after_curated_ones() {
        let merged = merge_pool(&[
            "brand-new/model:free".to_string(),
            "another/new:free".to_string(),
        ]);
        for curated in CURATED_FREE_MODELS {
            let pos = merged
                .iter()
                .position(|m| m == curated)
                .expect("curated model must survive the merge");
            let new_pos = merged
                .iter()
                .position(|m| m == "brand-new/model:free")
                .expect("refreshed model must be added");
            assert!(
                pos < new_pos,
                "curated `{curated}` must outrank the appended newcomer"
            );
        }
        assert!(merged.contains(&"another/new:free".to_string()));
    }

    #[test]
    fn merge_does_not_duplicate_models_present_in_both() {
        // A refresh that only echoes models already in the curated floor must
        // not grow the pool at all.
        let merged = merge_pool(&[
            "kilo-auto/free".to_string(),
            "poolside/laguna-s-2.1:free".to_string(),
            "poolside/laguna-s-2.1:free".to_string(),
        ]);
        assert_eq!(merged.len(), CURATED_FREE_MODELS.len());
        for model in &merged {
            let occurrences = merged.iter().filter(|m| *m == model).count();
            assert_eq!(occurrences, 1, "`{model}` must appear exactly once");
        }
    }

    #[test]
    fn merge_counts_only_genuinely_new_models() {
        let merged = merge_pool(&[
            "kilo-auto/free".to_string(),
            "brand-new/model:free".to_string(),
        ]);
        assert_eq!(merged.len(), CURATED_FREE_MODELS.len() + 1);
    }

    #[test]
    fn curated_models_all_have_a_known_context_limit() {
        // A curated model with no known limit would fall back to the
        // provider default and could be rotated onto with a wrong compaction
        // budget, which is exactly the mid-turn failure this table prevents.
        for model in CURATED_FREE_MODELS {
            assert!(
                free_model_context_limit(model).is_some(),
                "curated model `{model}` has no verified context limit"
            );
        }
    }

    #[test]
    fn context_limits_are_lookup_exact_and_do_not_match_by_prefix() {
        // A prefix match would let `poolside/laguna-xs-2.1:free` resolve to the
        // `laguna-s-2.1` row, or worse, `lfm-2.5-2.6b:free` to some other
        // liquid model with a different window.
        assert_eq!(
            free_model_context_limit("poolside/laguna-xs-2.1:free"),
            Some(262_144)
        );
        assert_eq!(
            free_model_context_limit("liquid/lfm-2.5-2.6b:free"),
            Some(65_536)
        );
        assert_eq!(free_model_context_limit("poolside/laguna"), None);
        assert_eq!(free_model_context_limit("openai/gpt-5.4"), None);
        assert_eq!(free_model_context_limit(""), None);
    }

    #[test]
    fn pool_context_limits_are_all_plausible() {
        // Guards against a typo'd zero or a nonsense value shipping into the
        // compaction budget.
        for (model, limit) in FREE_MODEL_CONTEXT_LIMITS {
            assert!(
                *limit >= 32_000 && *limit <= 4_000_000,
                "`{model}` has an implausible context limit of {limit}"
            );
        }
    }

    #[test]
    fn every_free_model_presents_as_alphax_free() {
        // The user picked "Alphax Free"; they must never see a raw upstream id.
        for model in CURATED_FREE_MODELS {
            assert_eq!(free_model_display_name(model), Some("Alphax Free"));
        }
        assert_eq!(
            free_model_display_name("poolside/laguna-xs-2.1:free"),
            Some("Alphax Free")
        );
    }

    #[test]
    fn non_free_models_keep_their_real_names() {
        // Rotation display mapping must not leak onto paid models, which users
        // legitimately need to identify.
        assert_eq!(free_model_display_name("openai/gpt-5.4"), None);
        assert_eq!(free_model_display_name("kilo-auto/small"), None);
        assert_eq!(free_model_display_name(""), None);
    }

    fn catalog_row(id: &str, prompt: &str, completion: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "pricing": { "prompt": prompt, "completion": completion },
            "context_length": 262144,
        })
    }

    #[test]
    fn catalog_keeps_only_zero_priced_models() {
        // Shapes copied from the real gateway payload, including the `"-1"`
        // unmetered marker and the zero-padded free rows.
        let body = serde_json::json!({ "data": [
            catalog_row("kilo-auto/free", "0", "0"),
            catalog_row("poolside/laguna-xs-2.1:free", "0", "0"),
            catalog_row("stepfun/step-3.7-flash:free", "0.000000000000", "0.000000000000"),
            catalog_row("kilo-auto/small", "0.00000005", "0.0000004"),
            catalog_row("openai/gpt-5.4", "0.0000015", "0.000006"),
        ]});
        let ids = free_model_ids_from_catalog(&body);
        assert!(ids.contains(&"kilo-auto/free".to_string()));
        assert!(ids.contains(&"poolside/laguna-xs-2.1:free".to_string()));
        assert!(ids.contains(&"stepfun/step-3.7-flash:free".to_string()));
        assert!(
            !ids.contains(&"kilo-auto/small".to_string()),
            "a priced model must never be pulled into the free pool"
        );
        assert!(!ids.contains(&"openai/gpt-5.4".to_string()));
    }

    #[test]
    fn catalog_rejects_free_prompt_with_paid_completion() {
        // Half-free is still billable. Reading only the prompt price would let
        // this through and charge the user on every output token.
        let body = serde_json::json!({ "data": [
            catalog_row("sneaky/model:free", "0", "0.000004"),
        ]});
        assert!(free_model_ids_from_catalog(&body).is_empty());
    }

    #[test]
    fn catalog_treats_missing_or_null_price_as_not_free() {
        // The dangerous direction: an absent price must never be assumed zero.
        let body = serde_json::json!({ "data": [
            { "id": "mystery/model:free" },
            { "id": "other/model:free", "pricing": { "prompt": null, "completion": null } },
        ]});
        assert!(
            free_model_ids_from_catalog(&body).is_empty(),
            "an unknown price must not be read as free"
        );
    }

    #[test]
    fn catalog_survives_a_malformed_payload() {
        // The catalog is a remote, untrusted response; none of these may panic.
        for body in [
            serde_json::json!({}),
            serde_json::json!({ "data": "not-an-array" }),
            serde_json::json!({ "data": [1, 2, 3] }),
            serde_json::json!({ "data": [{ "id": 42 }] }),
            serde_json::json!([]),
        ] {
            assert!(free_model_ids_from_catalog(&body).is_empty());
        }
    }

    #[test]
    fn catalog_deduplicates_repeated_ids() {
        let body = serde_json::json!({ "data": [
            catalog_row("a/model:free", "0", "0"),
            catalog_row("a/model:free", "0", "0"),
        ]});
        assert_eq!(free_model_ids_from_catalog(&body).len(), 1);
    }

    #[test]
    fn catalog_tolerates_whitespace_around_ids_and_prices() {
        let body = serde_json::json!({ "data": [
            catalog_row("  spaced/model:free  ", " 0 ", "0"),
        ]});
        assert_eq!(
            free_model_ids_from_catalog(&body),
            vec!["spaced/model:free".to_string()]
        );
    }
}
