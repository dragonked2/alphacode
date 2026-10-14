//! Local per-model token and cost ledger.
//!
//! Account endpoints (see `usage.rs`) report limits, not what alphacode itself
//! spent, and the Alphax Free gateway exposes no per-model usage API. So every
//! completed API response is recorded here, keyed by the model that served it,
//! and both `/usage` and the session widget read from this one source.
//!
//! Costs use the rate the user configured for Alphax Free: $10 per 1M tokens,
//! applied to input + output (+ cache reads and writes) alike. Other providers
//! fall back to the models.dev catalog where available.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// USD per one million tokens for the Alphax Free pool.
pub const ALPHAX_FREE_USD_PER_MTOK: f64 = 10.0;

/// Token totals for a single model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelUsage {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl ModelUsage {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }

    /// Cost in USD at a flat per-million-token rate.
    pub fn cost_usd_at(&self, usd_per_mtok: f64) -> f64 {
        self.total_tokens() as f64 / 1_000_000.0 * usd_per_mtok
    }
}

fn ledger() -> &'static Mutex<BTreeMap<String, ModelUsage>> {
    static LEDGER: OnceLock<Mutex<BTreeMap<String, ModelUsage>>> = OnceLock::new();
    LEDGER.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Record one completed API response served by `model`.
pub fn record_response(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) {
    let model = model.trim();
    if model.is_empty() {
        return;
    }
    if input_tokens == 0 && output_tokens == 0 && cache_read_tokens == 0 && cache_write_tokens == 0
    {
        return;
    }
    let Ok(mut map) = ledger().lock() else {
        return;
    };
    let entry = map.entry(model.to_string()).or_default();
    entry.requests += 1;
    entry.input_tokens = entry.input_tokens.saturating_add(input_tokens);
    entry.output_tokens = entry.output_tokens.saturating_add(output_tokens);
    entry.cache_read_tokens = entry.cache_read_tokens.saturating_add(cache_read_tokens);
    entry.cache_write_tokens = entry.cache_write_tokens.saturating_add(cache_write_tokens);
}

/// Snapshot of every model seen this process, most-used first.
pub fn snapshot() -> Vec<(String, ModelUsage)> {
    let Ok(map) = ledger().lock() else {
        return Vec::new();
    };
    let mut rows: Vec<(String, ModelUsage)> =
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    rows.sort_by(|a, b| {
        b.1.total_tokens()
            .cmp(&a.1.total_tokens())
            .then_with(|| a.0.cmp(&b.0))
    });
    rows
}

/// Totals across all models: `(total_tokens, total_requests)`.
pub fn totals() -> (u64, u64) {
    snapshot().iter().fold((0u64, 0u64), |(t, r), (_, u)| {
        (
            t.saturating_add(u.total_tokens()),
            r.saturating_add(u.requests),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_million_tokens_costs_ten_dollars() {
        let usage = ModelUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        };
        assert!((usage.cost_usd_at(ALPHAX_FREE_USD_PER_MTOK) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn cache_tokens_count_toward_total() {
        let usage = ModelUsage {
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 300,
            cache_write_tokens: 400,
            ..Default::default()
        };
        assert_eq!(usage.total_tokens(), 1000);
    }
}
/// Build a `/usage` report entry from the local ledger so per-model tokens and
/// cost show even when no account API reports anything for the gateway.
/// Returns `None` until at least one response has been recorded.
pub fn usage_report() -> Option<crate::alphacode_usage_types::ProviderUsage> {
    let rows = snapshot();
    if rows.is_empty() {
        return None;
    }
    let (total_tokens, total_requests) = totals();
    let total_cost: f64 = rows
        .iter()
        .map(|(_, u)| u.cost_usd_at(ALPHAX_FREE_USD_PER_MTOK))
        .sum();

    let mut extra_info = vec![(
        "Total".to_string(),
        format!(
            "{} tokens · {} requests · ${:.2} (at ${}/1M)",
            format_count(total_tokens),
            total_requests,
            total_cost,
            ALPHAX_FREE_USD_PER_MTOK
        ),
    )];
    for (model, usage) in rows {
        extra_info.push((
            model,
            format!(
                "{} tokens · in {} · out {} · {} req · ${:.2}",
                format_count(usage.total_tokens()),
                format_count(usage.input_tokens),
                format_count(usage.output_tokens),
                usage.requests,
                usage.cost_usd_at(ALPHAX_FREE_USD_PER_MTOK),
            ),
        ));
    }

    Some(crate::alphacode_usage_types::ProviderUsage {
        provider_name: "Alphax Free (this session)".to_string(),
        limits: Vec::new(),
        extra_info,
        hard_limit_reached: false,
        error: None,
        last_used_unix_secs: None,
    })
}

/// Compact human count: 1234 -> "1.2K", 5_600_000 -> "5.6M".
pub fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Tokens and USD cost for models that belong to the Alphax Free pool.
/// Other providers are excluded so no rate is applied to them.
pub fn alphax_pool_totals() -> (u64, f64) {
    snapshot()
        .iter()
        .filter(|(model, _)| {
            crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(model)
                .is_some()
        })
        .fold((0u64, 0.0f64), |(tokens, cost), (_, u)| {
            (
                tokens.saturating_add(u.total_tokens()),
                cost + u.cost_usd_at(ALPHAX_FREE_USD_PER_MTOK),
            )
        })
}
