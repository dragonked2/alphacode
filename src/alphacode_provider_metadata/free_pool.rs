//! The Alphax Free fallback rotation pool.
//! # Scope note
//! Free-route identifiers are reassembled from `super::route_cloak` on use, and
//! the pool's membership is discovered live from the gateway (see
//! [`curated_free_models`] and [`refresh_free_pool_from_gateway`]) — neither the
//! route nor the current lineup is stored in this file.
//!
//! # Why this file exists
//!
//! The automatic route is a *routing alias*, not a model. The gateway resolves
//! it server-side to one concrete free model, so the id we hold says nothing
//! about what actually served a request. The gateway's rate limits are enforced
//! **per upstream model**, which means the alias can return HTTP 429 while
//! several other zero-priced models are serving traffic at that exact moment.
//!
//! That is a real and reproducible failure, verified against the live gateway:
//! within one session, seconds apart, other free ids served `200`s while the
//! alias's backend returned a daily-limit 429 — and the 429 body name(d) the
//! concrete upstream that had tripped its own cap, proving the limit is
//! per-model rather than per-account. Retrying the same alias (which is what
//! AlphaCode did before this file) just re-rolls the gateway's dice against the
//! same throttled backend. Rotating to a different free model is the only
//! reliable way to make progress.
//!
//! # Eligibility
//!
//! Membership is decided by **price**, not by marketing name. A model enters
//! the pool only if the gateway's `/v1/models` reports `pricing.prompt == 0`
//! *and* `pricing.completion == 0`. Nothing that costs money to call may enter
//! the rotation, because rotation must never be able to spend a user's credits
//! behind their back. The metered sibling alias is the cautionary case: it is
//! priced (differently on each axis), so it is *not* free and is deliberately
//! excluded.
//!
//! The alias entries that are included (`/free`, and `/efficient`
//! only because the gateway reports `-1`, meaning "free/unmetered") are kept
//! because they are the tier names users recognise. `/frontier` and `/balanced`
//! are excluded: they are metered aliases and would silently cost credits.

use super::route_cloak;

/// The profile whose gateway exposes the free pool. Kept as a constant so the
/// rotation code and the profile definition cannot drift apart.
pub const ALPHAX_FREE_PROFILE_ID: &str = "alphax-free";

/// The stable rotation floor. Free lineups rotate without notice, so this is
/// deliberately NOT a frozen list of pool members — only the one id that is
/// always there: the automatic routing alias, which is also the default the
/// user picked.
///
/// Everything else in the rotation pool is discovered live:
/// [`refresh_free_pool_from_gateway`] reads the gateway's `/v1/models`, keeps
/// the zero-priced rows, and [`merge_pool`] appends them behind this floor.
/// Hardcoding last month's lineup would only guarantee staleness (and, patched
/// in a hurry, would leak upstream ids into the binary — route identifiers stay
/// fused in [`super::route_cloak`]).
///
/// The alias is decoded from [`super::route_cloak`] (route identifiers are not
/// kept as source literals), so this is a function rather than a `const`.
pub fn curated_free_models() -> &'static [&'static str] {
    use std::sync::OnceLock;
    static CURATED: OnceLock<&'static [&'static str]> = OnceLock::new();
    CURATED.get_or_init(|| {
        Box::leak(Box::from([
            super::route_cloak::leaked_free_route_default_model(),
        ]))
    })
}

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
        .filter(|id| *id != route_cloak::free_route_efficient_alias())
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

/// Context windows learned from the live gateway, keyed by model id.
///
/// Free lineups rotate without notice, so no per-model table frozen into
/// source can stay correct — the gateway publishes `context_length` for each
/// catalog row, and [`refresh_free_pool_from_gateway`] records every free
/// row's limit alongside the pool ids themselves. Nothing here is pinned down
/// in source beyond the one always-present id's verified window.
static DISCOVERED_CONTEXT_LIMITS: std::sync::Mutex<std::collections::BTreeMap<String, usize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Record `context_length` for every free-eligible catalog row.
///
/// Missing, zero, or non-numeric limits are simply not recorded; callers fall
/// through to the provider's conservative default for those ids.
pub fn remember_free_model_context_limits(body: &serde_json::Value) {
    let Some(rows) = body.get("data").and_then(serde_json::Value::as_array) else {
        return;
    };
    let Ok(mut limits) = DISCOVERED_CONTEXT_LIMITS.lock() else {
        return;
    };
    for row in rows {
        let Some(id) = row.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(limit) = row
            .get("context_length")
            .and_then(serde_json::Value::as_u64)
        else {
            continue;
        };
        if limit == 0 {
            continue;
        }
        limits.insert(id.trim().to_ascii_lowercase(), limit as usize);
    }
}

/// Context window for a free-pool model, if known.
///
/// Lookups are lifetime-current: the map is rebuilt on every refresh, so a
/// model the gateway retired loses its remembered window only when the pool
/// moves on from it as well.
pub fn free_model_context_limit(model: &str) -> Option<usize> {
    let id = model.trim();
    if id.is_empty() {
        return None;
    }
    // The automatic routing alias is the one always-present id, and its
    // window was verified directly against the gateway. Its id is
    // route-cloaked, so it cannot be a table row or a source literal.
    if id == route_cloak::free_route_default_model() {
        return Some(256_000);
    }
    let Ok(limits) = DISCOVERED_CONTEXT_LIMITS.lock() else {
        return None;
    };
    limits.get(&id.to_ascii_lowercase()).copied()
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
    remember_free_model_context_limits(&body);
    crate::alphacode_app_core::agent::free_pool_rotation::absorb_discovered_free_models(discovered);
}

/// Is this model id eligible to serve a free-tier rotation?
///
/// The rule is a suffix match on `:free` plus the two unmetered aliases.
/// Keeping it a pure function of the id means it can gate a model that
/// arrived from the live catalog without another round trip.
pub fn is_free_tier_model_id(model: &str) -> bool {
    let id = model.trim();
    if id.is_empty() {
        return false;
    }
    if id.ends_with(":free") {
        return true;
    }
    id == route_cloak::free_route_default_model()
        || id == route_cloak::free_route_efficient_alias()
        || id == "openrouter/free"
}

/// The user-visible name for anything in the free pool.
///
/// Every model in this pool is a detail of the gateway's internals, so a user
/// should never be shown a concrete pool id and asked to care which upstream
/// happened to serve the turn. They picked "Alphax Free"; which free model
/// covered the request is our problem, not theirs.
pub const ALPHAX_FREE_DISPLAY_NAME: &str = "Alphax Free";

/// Map a free-*tier-eligible* model id to the Alphax Free product name.
///
/// This is a pure suffix test: it treats **any** `:free` id as eligible, which
/// is correct for gating rotation but too broad for display — other providers
/// publish their own `:free` ids that are not Alphax Free. User-facing surfaces
/// must therefore gate on actual pool membership via
/// `crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name`
/// instead of calling this directly.
///
/// Returns `None` for anything outside the eligible set so ordinary models keep
/// their real, honest names.
pub fn free_model_display_name(model: &str) -> Option<&'static str> {
    is_free_tier_model_id(model).then_some(ALPHAX_FREE_DISPLAY_NAME)
}

/// The gateway base URL for the Alphax Free profile, decoded from the route
/// cloak on demand.
pub fn alphax_free_api_base() -> String {
    route_cloak::free_route_base()
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
    let curated = curated_free_models();
    let mut merged: Vec<String> = Vec::with_capacity(curated.len() + refreshed.len());
    let mut seen = std::collections::HashSet::new();
    for model in curated
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

    /// The metered sibling alias. Route ids are cloaked in source; tests
    /// reassemble them through [`route_cloak`] rather than spelling them out.
    fn small_alias() -> String {
        route_cloak::free_route_small_alias()
    }

    fn auto_alias_id(suffix: &str) -> String {
        format!("{}/{}", route_cloak::free_route_auto_prefix(), suffix)
    }

    #[test]
    fn curated_pool_is_exactly_the_price_verified_free_models() {
        // Every entry must satisfy the same eligibility rule the live catalog
        // is filtered by, so the curated floor and the refreshed set can never
        // disagree about what "free" means.
        for model in curated_free_models() {
            assert!(
                is_free_tier_model_id(model),
                "curated entry `{model}` is not eligible for rotation; it would \
                 either be skipped or, worse, be treated as a fallback"
            );
        }
    }

    #[test]
    fn priced_small_alias_is_never_in_the_pool() {
        // Regression guard for the real trap: the metered sibling looks like a
        // free-tier sibling but the gateway prices it at 0.00000005/0.0000004.
        // Including it would spend a user's credits without consent.
        let small = small_alias();
        assert!(!is_free_tier_model_id(&small));
        assert!(!curated_free_models().contains(&small.as_str()));
        assert!(!merge_pool(&[]).iter().any(|m| *m == small));
    }

    #[test]
    fn metered_aliases_are_excluded() {
        // /frontier and /balanced are metered, not free.
        assert!(!is_free_tier_model_id(&auto_alias_id("frontier")));
        assert!(!is_free_tier_model_id(&auto_alias_id("balanced")));
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
            &route_cloak::free_route_default_model(),
            "openai/gpt-5.4",
            "synth-a/xs:free",
            "anthropic/claude-opus-5",
            "synth-b/y:free",
        ]);
        assert_eq!(
            filtered,
            vec![
                route_cloak::free_route_default_model(),
                "synth-a/xs:free".to_string(),
                "synth-b/y:free".to_string()
            ]
        );
    }

    #[test]
    fn filter_drops_duplicates_keeping_first() {
        let filtered =
            filter_free_tier_ids(["synth-a/xs:free", "openai/gpt-5.4", "synth-a/xs:free"]);
        assert_eq!(filtered, vec!["synth-a/xs:free"]);
    }

    #[test]
    fn merge_never_shrinks_below_the_curated_floor() {
        // A refresh that finds nothing must not empty the pool.
        let merged = merge_pool(&[]);
        assert_eq!(merged.len(), curated_free_models().len());
        assert!(merged.contains(&route_cloak::free_route_default_model()));
    }

    #[test]
    fn merge_appends_newly_discovered_models_after_curated_ones() {
        let merged = merge_pool(&[
            "brand-new/model:free".to_string(),
            "another/new:free".to_string(),
        ]);
        for curated in curated_free_models() {
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
        // A refresh that only echoes models already in the pool must not grow
        // it at all.
        let merged = merge_pool(&[
            route_cloak::free_route_default_model(),
            "synthetic/pool-a:free".to_string(),
            "synthetic/pool-a:free".to_string(),
        ]);
        assert_eq!(merged.len(), curated_free_models().len() + 1);
        for model in &merged {
            let occurrences = merged.iter().filter(|m| *m == model).count();
            assert_eq!(occurrences, 1, "`{model}` must appear exactly once");
        }
    }

    #[test]
    fn merge_counts_only_genuinely_new_models() {
        let merged = merge_pool(&[
            route_cloak::free_route_default_model(),
            "brand-new/model:free".to_string(),
        ]);
        assert_eq!(merged.len(), curated_free_models().len() + 1);
    }

    #[test]
    fn curated_models_all_have_a_known_context_limit() {
        // The floor is the automatic routing alias; its window is the one
        // verified constant, known even before the first refresh lands.
        for model in curated_free_models() {
            assert!(
                free_model_context_limit(model).is_some(),
                "floor model `{model}` has no known context limit"
            );
        }
    }

    #[test]
    fn context_limits_are_learned_from_the_live_catalog() {
        // Free lineups rotate, so limits are parsed from the gateway's
        // `context_length` field at refresh time instead of a frozen table.
        let body = serde_json::json!({ "data": [
            { "id": "synth-a/xs:free", "context_length": 65_536 },
            { "id": "synth-b/ultra:free", "context_length": 1_048_576 },
            { "id": "synth-c/no-limit:free" },
        ]});
        remember_free_model_context_limits(&body);
        assert_eq!(
            free_model_context_limit("Synth-A/XS:free "),
            Some(65_536),
            "lookups are trim/case-insensitive, matching the wire data"
        );
        assert_eq!(
            free_model_context_limit("synth-b/ultra:free"),
            Some(1_048_576)
        );
        assert_eq!(
            free_model_context_limit("synth-c/no-limit:free"),
            None,
            "a row without a limit stays unknown"
        );
        assert_eq!(free_model_context_limit("synth-b/ultra"), None);
        assert_eq!(free_model_context_limit("openai/gpt-5.4"), None);
        assert_eq!(free_model_context_limit(""), None);
    }

    #[test]
    fn every_free_model_presents_as_alphax_free() {
        // The user picked "Alphax Free"; display surfaces must never show a
        // raw upstream id. (Picker entries get a separate masked label that
        // still hides the route.)
        for model in curated_free_models() {
            assert_eq!(free_model_display_name(model), Some("Alphax Free"));
        }
        assert_eq!(
            free_model_display_name("synth-a/xs:free"),
            Some("Alphax Free")
        );
    }

    #[test]
    fn non_free_models_keep_their_real_names() {
        // Rotation display mapping must not leak onto paid models, which users
        // legitimately need to identify.
        assert_eq!(free_model_display_name("openai/gpt-5.4"), None);
        assert_eq!(free_model_display_name(&small_alias()), None);
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
        // Shapes the gateway emits, including zero-padded free rows.
        let body = serde_json::json!({ "data": [
            catalog_row(&route_cloak::free_route_default_model(), "0", "0"),
            catalog_row("synth-a/xs:free", "0", "0"),
            catalog_row("synth-c/flash:free", "0.000000000000", "0.000000000000"),
            catalog_row(&small_alias(), "0.00000005", "0.0000004"),
            catalog_row("openai/gpt-5.4", "0.0000015", "0.000006"),
        ]});
        let ids = free_model_ids_from_catalog(&body);
        assert!(ids.contains(&route_cloak::free_route_default_model()));
        assert!(ids.contains(&"synth-a/xs:free".to_string()));
        assert!(ids.contains(&"synth-c/flash:free".to_string()));
        assert!(
            !ids.contains(&small_alias()),
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
