//! Local per-model token and cost ledger, persisted for the lifetime of the
//! install.
//!
//! Account endpoints (see `usage.rs`) report limits, not what alphacode itself
//! spent, and the Alphax Free gateway exposes no per-model usage API. So every
//! completed API response is recorded here, keyed by the local calendar day
//! and the model that served it, and both `/usage` and the session widget read
//! from this one source.
//!
//! The ledger persists to `~/.alphacode/usage_ledger.json` (debounced), so
//! `/usage` can report everything since alphacode was installed — total tokens,
//! requests, estimated cost, and a per-day breakdown sorted newest first —
//! instead of only the current process's session.
//!
//! Costs: Alphax Free pool models use the flat $10/1M-token rate the user
//! configured for the gateway; other models use the models.dev catalog when a
//! price is known, and report `None` (unknown) rather than inventing a number.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// USD per one million tokens for the Alphax Free pool.
pub const ALPHAX_FREE_USD_PER_MTOK: f64 = 10.0;

/// Minimum gap between debounced disk writes. Recording happens on every API
/// response (several per minute in tool-heavy turns); rewriting a few-KB JSON
/// file each time is wasteful, so writes are coalesced. `/usage` and
/// [`flush`] force a write so read paths always see exact totals.
const FLUSH_MIN_INTERVAL: Duration = Duration::from_secs(5);

const LEDGER_FILE: &str = "usage_ledger.json";
const LEDGER_VERSION: u32 = 1;

/// Token totals for a single model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
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

    fn merge(&mut self, other: &ModelUsage) {
        self.requests = self.requests.saturating_add(other.requests);
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(other.cache_write_tokens);
    }
}

/// One local calendar day of usage across all models.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyRow {
    /// Local calendar day, `YYYY-MM-DD`.
    pub day: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Estimated USD cost for the day; `None` when no price is known for the
    /// models used that day.
    pub cost_usd: Option<f64>,
}

impl DailyRow {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }
}

/// On-disk shape: version-tagged map of day -> model -> totals.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedLedger {
    version: u32,
    days: BTreeMap<String, BTreeMap<String, ModelUsage>>,
}

/// In-memory ledger state, re-resolved whenever the home directory changes
/// (tests, multi-home setups), so state never leaks across paths.
struct LedgerState {
    path: PathBuf,
    days: BTreeMap<String, BTreeMap<String, ModelUsage>>,
    dirty: bool,
    last_flush: Option<Instant>,
}

fn ledger() -> &'static Mutex<Option<LedgerState>> {
    static LEDGER: Mutex<Option<LedgerState>> = Mutex::new(None);
    &LEDGER
}

fn ledger_path() -> Option<PathBuf> {
    Some(crate::storage::alphacode_dir().ok()?.join(LEDGER_FILE))
}

/// Run `f` against the loaded state for the current home directory, loading
/// the persisted file on first use (or after the home directory changed).
fn with_state<T>(f: impl FnOnce(&mut LedgerState) -> T) -> Option<T> {
    let path = ledger_path()?;
    let mut guard = ledger()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let needs_reload = guard
        .as_ref()
        .map(|state| state.path != path)
        .unwrap_or(true);
    if needs_reload {
        let persisted: PersistedLedger = crate::storage::read_json(&path).unwrap_or_default();
        *guard = Some(LedgerState {
            path,
            days: persisted.days,
            dirty: false,
            last_flush: None,
        });
    }
    let state = guard.as_mut()?;
    Some(f(state))
}

fn flush_locked(state: &mut LedgerState) {
    if !state.dirty {
        return;
    }
    let persisted = PersistedLedger {
        version: LEDGER_VERSION,
        days: state.days.clone(),
    };
    match crate::storage::write_json(&state.path, &persisted) {
        Ok(()) => {
            state.dirty = false;
            state.last_flush = Some(Instant::now());
        }
        Err(error) => {
            crate::logging::warn(&format!(
                "Failed to persist usage ledger to {}: {}",
                state.path.display(),
                error
            ));
            // Retry on the next record; leave `dirty` set.
            state.last_flush = Some(Instant::now());
        }
    }
}

/// Force any pending ledger changes to disk. Read paths (`/usage`) call this
/// so the report always shows exact lifetime totals, and shutdown hooks may
/// call it to close the debounce window.
pub fn flush() {
    with_state(flush_locked);
}

/// Local calendar day key used for bucketing.
fn today_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
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
    let entry = ModelUsage {
        requests: 1,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    };
    with_state(|state| {
        state
            .days
            .entry(today_key())
            .or_default()
            .entry(model.to_string())
            .or_default()
            .merge(&entry);
        state.dirty = true;
        let overdue = state
            .last_flush
            .map(|last| last.elapsed() >= FLUSH_MIN_INTERVAL)
            .unwrap_or(true);
        if overdue {
            flush_locked(state);
        }
    });
}

/// Per-model lifetime totals across every recorded day, most-used first.
pub fn snapshot() -> Vec<(String, ModelUsage)> {
    with_state(|state| {
        let mut merged: BTreeMap<String, ModelUsage> = BTreeMap::new();
        for models in state.days.values() {
            for (model, usage) in models {
                merged.entry(model.clone()).or_default().merge(usage);
            }
        }
        let mut rows: Vec<(String, ModelUsage)> = merged.into_iter().collect();
        rows.sort_by(|a, b| {
            b.1.total_tokens()
                .cmp(&a.1.total_tokens())
                .then_with(|| a.0.cmp(&b.0))
        });
        rows
    })
    .unwrap_or_default()
}

/// Lifetime totals across all models: `(total_tokens, total_requests)`.
pub fn totals() -> (u64, u64) {
    snapshot().iter().fold((0u64, 0u64), |(t, r), (_, u)| {
        (
            t.saturating_add(u.total_tokens()),
            r.saturating_add(u.requests),
        )
    })
}

/// Per-day rows, newest first. Cost is estimated per model (catalog price for
/// priced providers, flat gateway rate for the Alphax Free pool) and summed;
/// days whose models have no known price report `None`.
pub fn daily_usage() -> Vec<DailyRow> {
    with_state(|state| {
        let mut rows: Vec<DailyRow> = state
            .days
            .iter()
            .rev() // BTreeMap iterates keys ascending; newest day first here.
            .map(|(day, models)| {
                let mut row = DailyRow {
                    day: day.clone(),
                    requests: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cost_usd: None,
                };
                let mut cost_sum = 0.0f64;
                let mut any_cost_known = false;
                for (model, usage) in models {
                    row.requests = row.requests.saturating_add(usage.requests);
                    row.input_tokens = row.input_tokens.saturating_add(usage.input_tokens);
                    row.output_tokens = row.output_tokens.saturating_add(usage.output_tokens);
                    row.cache_read_tokens = row
                        .cache_read_tokens
                        .saturating_add(usage.cache_read_tokens);
                    row.cache_write_tokens = row
                        .cache_write_tokens
                        .saturating_add(usage.cache_write_tokens);
                    if let Some(cost) = estimate_cost_usd(model, usage) {
                        cost_sum += cost;
                        any_cost_known = true;
                    }
                }
                row.cost_usd = any_cost_known.then_some(cost_sum);
                row
            })
            .collect();
        rows.sort_by(|a, b| b.day.cmp(&a.day));
        rows
    })
    .unwrap_or_default()
}

/// Earliest recorded local calendar day, if anything was ever recorded.
pub fn first_recorded_day() -> Option<String> {
    with_state(|state| state.days.keys().next().cloned()).flatten()
}

/// Estimated USD cost for one model's usage, or `None` when the price is
/// unknown (never invents a number).
pub fn estimate_cost_usd(model: &str, usage: &ModelUsage) -> Option<f64> {
    if crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(model).is_some()
    {
        return Some(usage.cost_usd_at(ALPHAX_FREE_USD_PER_MTOK));
    }
    let provider = crate::alphacode_provider_core::models::provider_for_model(model)?;
    let cost = crate::model_pricing::lookup(provider, model)?;
    Some(
        (usage.input_tokens as f64 * cost.input_usd_per_mtok
            + usage.output_tokens as f64 * cost.output_usd_per_mtok
            + usage.cache_read_tokens as f64 * cost.cache_read_usd_per_mtok.unwrap_or(0.0)
            + usage.cache_write_tokens as f64 * cost.cache_write_usd_per_mtok.unwrap_or(0.0))
            / 1_000_000.0,
    )
}

/// One user-facing row of the `/usage` model breakdown.
///
/// `cost_usd` is the sum over the row's members for which a price *is* known;
/// a row mixing priced and unpriced members keeps `Some` so its priced share
/// still shows instead of dropping the whole row to "cost n/a".
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    /// User-facing model label (raw model id, or the shared product name for
    /// pool members).
    pub label: String,
    pub usage: ModelUsage,
    pub cost_usd: Option<f64>,
}

/// Collapse ledger model ids into user-facing rows.
///
/// Every model the Alphax Free pool serves presents under the single product
/// name ("Alphax Free"), regardless of which upstream the gateway happened to
/// route a given turn to. Raw alias rows would leak server-side routing —
/// `kilo-auto/free` is a routing *alias*, not a model the user selected — and
/// splitting one product across several pool ids would also split its spend
/// across several rows. All pool members share the flat rate, so merging is
/// exact, not an approximation.
///
/// Cost is accumulated per *group* here because the flat rate applies per
/// member id; estimating against the merged label alone would lose pricing
/// (the label itself is not a pool member) and show "cost n/a" for the exact
/// rows the user cares about.
fn aggregate_display_rows(rows: Vec<(String, ModelUsage)>) -> Vec<UsageRow> {
    let mut merged: BTreeMap<String, (ModelUsage, f64, bool)> = BTreeMap::new();
    for (model, usage) in rows {
        // Cost is estimated against the *member* id — the flat pool rate is
        // defined per pool model and catalog pricing is keyed by real model
        // ids — then folded into the group's display row. Estimating against
        // the merged label alone would lose pricing (the product label is not
        // itself a pool member) and render "cost n/a" for the exact rows the
        // user cares about.
        let member_cost = estimate_cost_usd(&model, &usage);
        let label =
            crate::alphacode_app_core::agent::free_pool_rotation::free_pool_display_name(&model)
                .map(str::to_string)
                .unwrap_or(model);
        let entry = merged
            .entry(label)
            .or_insert((ModelUsage::default(), 0.0, false));
        entry.0.merge(&usage);
        if let Some(cost) = member_cost {
            entry.1 += cost;
            entry.2 = true;
        }
    }
    let mut out: Vec<UsageRow> = merged
        .into_iter()
        .map(|(label, (usage, cost_sum, any_known))| UsageRow {
            cost_usd: any_known.then_some(cost_sum),
            label,
            usage,
        })
        .collect();
    // Same ordering contract as `snapshot`'s rows: most-used first, label as
    // the deterministic tie-break.
    out.sort_by(|a, b| {
        b.usage
            .total_tokens()
            .cmp(&a.usage.total_tokens())
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}

/// Build a `/usage` report entry from the local lifetime ledger: totals,
/// per-model breakdown, and a per-day history sorted newest first. Returns
/// `None` until at least one response has been recorded.
pub fn usage_report() -> Option<crate::alphacode_usage_types::ProviderUsage> {
    // Make sure the report reflects everything recorded so far, even inside
    // the debounce window.
    flush();
    let rows = aggregate_display_rows(snapshot());
    if rows.is_empty() {
        return None;
    }
    // Aggregation preserves every counter, so lifetime totals are identical
    // whether they are summed over raw member ids or display rows.
    let (total_tokens, total_requests) = totals();

    let mut known_cost = 0.0f64;
    let mut any_cost_known = false;
    for row in &rows {
        if let Some(cost) = row.cost_usd {
            known_cost += cost;
            any_cost_known = true;
        }
    }

    let mut extra_info: Vec<(String, String)> = Vec::new();
    let since = first_recorded_day();
    extra_info.push((
        "Since".to_string(),
        since.clone().unwrap_or_else(|| "this session".to_string()),
    ));
    let total_line = if any_cost_known {
        format!(
            "{} tokens · {} requests · est. ${:.2}",
            format_count(total_tokens),
            total_requests,
            known_cost
        )
    } else {
        format!(
            "{} tokens · {} requests (no pricing known for the models used)",
            format_count(total_tokens),
            total_requests
        )
    };
    extra_info.push(("Total (lifetime)".to_string(), total_line));

    extra_info.push((String::new(), String::new()));
    extra_info.push((
        "Models (lifetime)".to_string(),
        format!("{} model(s), newest usage first", rows.len()),
    ));
    // Pool members are already collapsed under the shared product label, so a
    // gateway alias like `kilo-auto/free` can never surface here: the user
    // selected "Alphax Free", and that is the name every row reports.
    for row in &rows {
        let cost_label = match row.cost_usd {
            Some(value) => format!("est. ${value:.4}"),
            None => "cost n/a".to_string(),
        };
        extra_info.push((
            row.label.clone(),
            format!(
                "{} tokens · in {} · out {} · cache r/w {}/{} · {} req · {cost_label}",
                format_count(row.usage.total_tokens()),
                format_count(row.usage.input_tokens),
                format_count(row.usage.output_tokens),
                format_count(row.usage.cache_read_tokens),
                format_count(row.usage.cache_write_tokens),
                row.usage.requests,
            ),
        ));
    }

    let days = daily_usage();
    if !days.is_empty() {
        extra_info.push((String::new(), String::new()));
        extra_info.push((
            "Daily history (newest first)".to_string(),
            format!("{} day(s) recorded", days.len()),
        ));
        // Bound the detail pane: the most recent 60 days keep the panel
        // readable; older days still count toward the lifetime totals above.
        for day in days.iter().take(60) {
            let cost_label = match day.cost_usd {
                Some(value) => format!("est. ${value:.2}"),
                None => "cost n/a".to_string(),
            };
            extra_info.push((
                day.day.clone(),
                format!(
                    "{} tokens · {} req · {cost_label}",
                    format_count(day.total_tokens()),
                    day.requests,
                ),
            ));
        }
    }

    Some(crate::alphacode_usage_types::ProviderUsage {
        provider_name: "Alphacode usage (since install)".to_string(),
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

    #[test]
    fn merging_usage_sums_every_counter() {
        let mut a = ModelUsage {
            requests: 1,
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 30,
            cache_write_tokens: 40,
        };
        a.merge(&ModelUsage {
            requests: 2,
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 300,
            cache_write_tokens: 400,
        });
        assert_eq!(a.requests, 3);
        assert_eq!(a.input_tokens, 110);
        assert_eq!(a.output_tokens, 220);
        assert_eq!(a.cache_read_tokens, 330);
        assert_eq!(a.cache_write_tokens, 440);
        assert_eq!(a.total_tokens(), 1100);
    }

    #[test]
    fn persisted_round_trip_preserves_days() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ledger.json");
        let mut days: BTreeMap<String, BTreeMap<String, ModelUsage>> = BTreeMap::new();
        days.entry("2026-10-01".to_string()).or_default().insert(
            "model-a".to_string(),
            ModelUsage {
                requests: 4,
                input_tokens: 1_000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            },
        );
        let persisted = PersistedLedger {
            version: LEDGER_VERSION,
            days,
        };
        crate::storage::write_json(&path, &persisted).expect("write");
        let loaded: PersistedLedger = crate::storage::read_json(&path).expect("read");
        assert_eq!(loaded.version, LEDGER_VERSION);
        let usage = loaded
            .days
            .get("2026-10-01")
            .and_then(|models| models.get("model-a"))
            .expect("day/model row");
        assert_eq!(usage.requests, 4);
        assert_eq!(usage.total_tokens(), 1_500);
    }

    #[test]
    fn format_count_is_compact() {
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_234), "1.2K");
        assert_eq!(format_count(5_600_000), "5.6M");
    }

    #[test]
    fn unknown_prices_report_none_not_zero() {
        let usage = ModelUsage {
            input_tokens: 1_000_000,
            ..Default::default()
        };
        // A model id that matches no catalog entry and no free-pool route must
        // not fabricate a $0.00 cost.
        assert_eq!(estimate_cost_usd("totally-unknown-model-xyz", &usage), None);
    }

    #[test]
    fn pool_models_present_under_the_product_label_not_the_router_alias() {
        // `/usage` must never show the raw gateway alias (`kilo-auto/free` is
        // server-side routing, not a model the user picked). The pool serves
        // its turns behind the "Alphax Free" product name, and that is what
        // the report row is keyed by.
        let usage = ModelUsage {
            requests: 5,
            input_tokens: 3_000_000,
            output_tokens: 5_500,
            cache_read_tokens: 2_400_000,
            cache_write_tokens: 0,
        };
        let out = aggregate_display_rows(vec![("kilo-auto/free".to_string(), usage.clone())]);
        assert_eq!(out.len(), 1, "one member id -> one display row");
        assert_eq!(out[0].label, "Alphax Free");
        assert_eq!(out[0].usage, usage);
        assert_eq!(
            out[0].cost_usd,
            estimate_cost_usd("kilo-auto/free", &usage),
            "group row keeps the member's priced cost"
        );
    }

    #[test]
    fn unknown_models_keep_their_raw_label_in_display_rows() {
        // Only the Alphax Free pool may borrow the product name; everything
        // else must keep showing the model the user actually served with.
        let out = aggregate_display_rows(vec![(
            "claude-opus-4-8".to_string(),
            ModelUsage {
                requests: 1,
                input_tokens: 10,
                ..Default::default()
            },
        )]);
        assert_eq!(out[0].label, "claude-opus-4-8");
    }
}
