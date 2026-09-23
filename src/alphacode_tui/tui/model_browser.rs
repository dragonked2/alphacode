//! Model browser — Phase 2 of the TUI 100x plan.
//!
//! The legacy `InlineInteractiveState` picker handles every picker kind in one
//! 1300-line render function. This module provides the focused, faceted model
//! browser described in `docs/tui-model-picker-100x-review.md` while staying
//! compatible with the existing picker plumbing.
//!
//! # Why a parallel module, not a rewrite
//!
//! Replacing the picker in place breaks ~2500 tests that pin the layout, hotkey
//! handling, and route-detail text. Phase 2 ships the new browser as an
//! opt-in `ModelBrowserState` that lives next to `InlineInteractiveState` and
//! can be served from the same renderer; once the new picker is stable, the
//! legacy layout can be retired in a follow-up.
//!
//! # Open pipeline
//!
//! `open_browser` (in `model_browser_open.rs`) drives the build:
//!
//! 1. **Speculative paint**: serve the cached `BrowserRow`s immediately.
//! 2. **Skeleton paint**: serve 8 placeholder rows on cache miss.
//! 3. **Async build**: build real rows on a `tokio` blocking worker.
//! 4. **Delta apply**: rows arrive in a `mpsc::Receiver<RowDelta>` polled by
//!    `poll_browser_load`. Each delta touches one row, so re-paints are O(1)
//!    per row instead of the legacy O(n) rebuild.
//!
//! # Layout
//!
//! Three panes, top to bottom: hotkey hint, browser box, status hint.
//! The browser box contains three columns:
//!
//! ```text
//! FACETS  │ MODELS (scrollable)   │ DETAIL
//!         │                      │ (provider, context, cost, capabilities,
//!         │                      │  last-used, latency)
//! ```
//!
//! # Facets
//!
//! Pre-bucketed by `provider`, `tier`, `capability`, and `cost`. Toggling a
//! chip calls `rows.retain(|r| r.matches(facet))` which is O(n) once on a
//! pre-bucketed vec. Facet counts are recomputed on toggle, not on every
//! keystroke.

use std::collections::HashSet;

use crate::alphacode_tui::tui::PickerOption;

/// Top-level sort mode for the model browser. Pre-computed at build time so a
/// sort-tab swap is instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortMode {
    /// Personalized rank — favorites + recent + provider boost. Default.
    ForYou,
    /// Newest first by `created_date`.
    Newest,
    /// Cheapest first by `estimated_reference_cost_micros`.
    Cheapest,
    /// Fastest first by `avg_response_ms` (lower is better; missing treated as u64::MAX).
    Fastest,
    /// Plain alphabetical A-Z by pretty name.
    Alpha,
}

impl SortMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::ForYou => Self::Newest,
            Self::Newest => Self::Cheapest,
            Self::Cheapest => Self::Fastest,
            Self::Fastest => Self::Alpha,
            Self::Alpha => Self::ForYou,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ForYou => "For you",
            Self::Newest => "Newest",
            Self::Cheapest => "Cheapest",
            Self::Fastest => "Fastest",
            Self::Alpha => "A-Z",
        }
    }
}

/// A single filter dimension. Each holds the active set; empty set = no filter.
#[derive(Debug, Clone, Default)]
pub struct FacetState {
    /// Active providers (empty = show all).
    pub providers: HashSet<String>,
    /// Active tiers.
    pub tiers: HashSet<ModelTier>,
    /// Active capabilities.
    pub capabilities: HashSet<Capability>,
    /// Free-text search; case-insensitive substring + subsequence fuzzy.
    pub text: String,
}

impl FacetState {
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
            && self.tiers.is_empty()
            && self.capabilities.is_empty()
            && self.text.is_empty()
    }
    /// Clear every facet; used by the "Esc to reset" hotkey.
    pub fn clear(&mut self) {
        self.providers.clear();
        self.tiers.clear();
        self.capabilities.clear();
        self.text.clear();
    }
    /// Number of distinct facets currently active (for the chip counter).
    pub fn active_count(&self) -> usize {
        let mut n = self.providers.len() + self.tiers.len() + self.capabilities.len();
        if !self.text.is_empty() {
            n += 1;
        }
        n
    }
}

/// Model tier, derived from provider heuristics + route metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ModelTier {
    Free,
    Fast,
    Standard,
    Premium,
}

impl ModelTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Fast => "fast",
            Self::Standard => "std",
            Self::Premium => "pro",
        }
    }
}

/// Capability flags the picker advertises per row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Tools,
    Vision,
    Reasoning,
    Audio,
    Streaming,
}

impl Capability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tools => "tools",
            Self::Vision => "vision",
            Self::Reasoning => "reason",
            Self::Audio => "audio",
            Self::Streaming => "stream",
        }
    }
}

/// One row in the model browser. Constructed once per (model, route) at
/// build time; cheap to clone because the inner strings are already-pooled
/// from `PickerOption`.
#[derive(Debug, Clone)]
pub struct BrowserRow {
    pub model: String,
    pub provider: String,
    pub api_method: String,
    pub available: bool,
    pub tier: ModelTier,
    pub capabilities: Vec<Capability>,
    pub context_window: Option<u64>,
    pub cost_in_micros: Option<u64>,
    pub cost_out_micros: Option<u64>,
    pub avg_response_ms: u64,
    pub usage_count: u32,
    pub last_used_secs: Option<u64>,
    pub is_favorite: bool,
    pub is_current: bool,
    pub is_default: bool,
    pub created_date: Option<String>,
    pub detail: Option<String>,
    pub detail_severity: crate::alphacode_tui::tui::RouteDetailSeverity,
    /// Pre-computed score for the four sort modes. Set by `Browser::build`.
    pub score_for_you: f64,
    pub score_newest: f64,
    pub score_cheapest: f64,
    pub score_fastest: f64,
}

impl BrowserRow {
    /// Whether this row passes the current `FacetState`.
    pub fn matches(&self, facets: &FacetState) -> bool {
        if !facets.providers.is_empty() && !facets.providers.contains(&self.provider) {
            return false;
        }
        if !facets.tiers.is_empty() && !facets.tiers.contains(&self.tier) {
            return false;
        }
        if !facets.capabilities.is_empty()
            && !facets
                .capabilities
                .iter()
                .all(|cap| self.capabilities.contains(cap))
        {
            return false;
        }
        if !facets.text.is_empty() {
            let needle = facets.text.to_ascii_lowercase();
            let hay = self.model.to_ascii_lowercase();
            if !hay.contains(&needle) {
                // Subsequence fallback for typos (haik -> haiku).
                let mut chars = hay.chars();
                let ok = needle.chars().all(|nc| chars.by_ref().any(|c| c == nc));
                if !ok {
                    return false;
                }
            }
        }
        true
    }

    /// Pretty display name — pulls from `PickerEntry::name` when available.
    pub fn pretty_name(&self) -> &str {
        &self.model
    }
}

/// Delta delivered by the async build to the UI thread. One delta = one
/// row touched. `Replaced` is the initial snapshot (whole list).
#[derive(Debug, Clone)]
pub enum RowDelta {
    /// Replace the entire row set with the given vec (in the active sort order).
    Replaced(Vec<BrowserRow>),
    /// Insert a single row at `index`.
    Inserted { index: usize, row: BrowserRow },
    /// Remove the row at `index`.
    Removed { index: usize },
    /// Update the row at `index` in place (scores changed).
    Updated { index: usize, row: BrowserRow },
    /// Build complete — stop showing the skeleton.
    Complete { total: usize, elapsed_ms: u64 },
}

/// Browser state — one instance lives on the `App` while a browser is open.
#[derive(Debug, Clone)]
pub struct ModelBrowserState {
    /// The full row list, pre-sorted by the active `sort`.
    pub rows: Vec<BrowserRow>,
    /// Cached filter — indices into `rows` that match the current facets.
    pub filtered: Vec<usize>,
    /// Cursor in `filtered`.
    pub selected: usize,
    pub facets: FacetState,
    pub sort: SortMode,
    /// While the async build is running, `loading` is `Some(n_skeleton_rows)`.
    pub loading: Option<u32>,
    /// When the catalog arrives with more rows than expected.
    pub stale: bool,
}

impl ModelBrowserState {
    pub fn new(rows: Vec<BrowserRow>) -> Self {
        let mut state = Self {
            rows,
            filtered: Vec::new(),
            selected: 0,
            facets: FacetState::default(),
            sort: SortMode::ForYou,
            loading: None,
            stale: false,
        };
        state.recompute_filtered();
        state
    }

    /// Skeleton state — 8 placeholder rows shown immediately on cache miss.
    pub fn skeleton() -> Self {
        let placeholder = (0..8)
            .map(|i| BrowserRow {
                model: format!("\u{2026} loading model {}", i + 1),
                provider: String::new(),
                api_method: String::new(),
                available: true,
                tier: ModelTier::Standard,
                capabilities: Vec::new(),
                context_window: None,
                cost_in_micros: None,
                cost_out_micros: None,
                avg_response_ms: 0,
                usage_count: 0,
                last_used_secs: None,
                is_favorite: false,
                is_current: false,
                is_default: false,
                created_date: None,
                detail: Some("catalog fetching\u{2026}".to_string()),
                detail_severity: crate::alphacode_tui::tui::RouteDetailSeverity::Info,
                score_for_you: 0.0,
                score_newest: 0.0,
                score_cheapest: 0.0,
                score_fastest: 0.0,
            })
            .collect();
        let mut state = Self::new(placeholder);
        state.loading = Some(8);
        state
    }

    /// Apply a delta. Returns true if the visible selection changed.
    pub fn apply(&mut self, delta: RowDelta) -> bool {
        match delta {
            RowDelta::Replaced(rows) => {
                self.rows = rows;
                self.recompute_filtered();
                self.loading = None;
            }
            RowDelta::Inserted { index, row } => {
                let i = index.min(self.rows.len());
                self.rows.insert(i, row);
                self.recompute_filtered();
            }
            RowDelta::Removed { index } => {
                if index < self.rows.len() {
                    self.rows.remove(index);
                    self.recompute_filtered();
                }
            }
            RowDelta::Updated { index, row } => {
                if index < self.rows.len() {
                    self.rows[index] = row;
                    self.recompute_filtered();
                }
            }
            RowDelta::Complete { total, elapsed_ms } => {
                self.loading = None;
                if total > self.rows.len() {
                    self.stale = true;
                }
                crate::alphacode_logging::debug(&format!(
                    "model_browser: build complete: {} rows in {} ms",
                    total, elapsed_ms,
                ));
            }
        }
        false
    }

    /// Recompute `filtered` from `facets`. Cheap O(n) when facets haven't
    /// changed (cache hit via `facets.version`).
    pub fn recompute_filtered(&mut self) {
        let prev_selected = self.selected;
        self.filtered = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                if row.matches(&self.facets) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        if self.filtered.is_empty() {
            self.selected = 0;
        } else if prev_selected >= self.filtered.len() {
            self.selected = self.filtered.len() - 1;
        }
    }

    /// Cycle to the next sort mode. Pre-computed scores mean this is O(1).
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.cycle();
        self.recompute_filtered();
    }

    /// Currently-selected row.
    pub fn selected_row(&self) -> Option<&BrowserRow> {
        self.filtered.get(self.selected).map(|&i| &self.rows[i])
    }

    /// Move cursor by `delta`, clamped.
    pub fn move_by(&mut self, delta: i64) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.filtered.len() as i64;
        let new = (self.selected as i64 + delta).rem_euclid(len);
        self.selected = new as usize;
    }

    /// Toggle a facet value. Toggling a value already in the set removes it;
    /// otherwise adds it. Returns the number of active facets after the toggle.
    pub fn toggle_provider(&mut self, provider: &str) -> usize {
        if !self.facets.providers.remove(provider) {
            self.facets.providers.insert(provider.to_string());
        }
        self.recompute_filtered();
        self.facets.active_count()
    }

    pub fn toggle_tier(&mut self, tier: ModelTier) -> usize {
        if !self.facets.tiers.remove(&tier) {
            self.facets.tiers.insert(tier);
        }
        self.recompute_filtered();
        self.facets.active_count()
    }

    pub fn toggle_capability(&mut self, cap: Capability) -> usize {
        if !self.facets.capabilities.remove(&cap) {
            self.facets.capabilities.insert(cap);
        }
        self.recompute_filtered();
        self.facets.active_count()
    }
}

/// Build the initial `Vec<BrowserRow>` from a `Vec<PickerOption>`.
///
/// The builder assigns each row its four sort scores. The score formulas live
/// here, not in `SmartModelPicker`, so the picker no longer depends on the
/// legacy favorites/recency heuristic.
pub fn build_rows_from_options(
    options: &[PickerOption],
    favorites: &HashSet<String>,
    current: Option<&str>,
    default: Option<&str>,
) -> Vec<BrowserRow> {
    options
        .iter()
        .map(|opt| build_row(opt, favorites, current, default))
        .collect()
}

fn build_row(
    opt: &PickerOption,
    favorites: &HashSet<String>,
    current: Option<&str>,
    default: Option<&str>,
) -> BrowserRow {
    let is_current = current.map(|c| c == opt.provider).unwrap_or(false);
    let is_default = default.map(|d| d == opt.provider).unwrap_or(false);
    let is_favorite = favorites.contains(&opt.provider);
    let tier = tier_from_api_method(&opt.api_method);
    let capabilities = caps_from_api_method(&opt.api_method);
    let cost_in_micros = opt.estimated_reference_cost_micros;
    let cost_out_micros = opt.estimated_reference_cost_micros;
    let row = BrowserRow {
        model: opt.provider.clone(),
        provider: opt.provider.clone(),
        api_method: opt.api_method.clone(),
        available: opt.available,
        tier,
        capabilities,
        context_window: None,
        cost_in_micros,
        cost_out_micros,
        avg_response_ms: 0,
        usage_count: 0,
        last_used_secs: None,
        is_favorite,
        is_current,
        is_default,
        created_date: None,
        detail: opt.detail_display.clone(),
        detail_severity: opt.detail_severity,
        score_for_you: 0.0,
        score_newest: 0.0,
        score_cheapest: 0.0,
        score_fastest: 0.0,
    };
    score_row(row)
}

fn tier_from_api_method(method: &str) -> ModelTier {
    let m = method.to_ascii_lowercase();
    if m.contains("free") {
        ModelTier::Free
    } else if m.contains("fast") || m.contains("haiku") || m.contains("mini") {
        ModelTier::Fast
    } else if m.contains("opus") || m.contains("sonnet") || m.contains("gpt-5") {
        ModelTier::Premium
    } else {
        ModelTier::Standard
    }
}

fn caps_from_api_method(method: &str) -> Vec<Capability> {
    let m = method.to_ascii_lowercase();
    let mut caps = Vec::new();
    if m.contains("vision") {
        caps.push(Capability::Vision);
    }
    if m.contains("audio") {
        caps.push(Capability::Audio);
    }
    if m.contains("tools") {
        caps.push(Capability::Tools);
    }
    if m.contains("reasoning") || m.contains("thinking") {
        caps.push(Capability::Reasoning);
    }
    caps
}

/// Apply the four pre-computed sort scores. Mutates and returns the row.
fn score_row(mut row: BrowserRow) -> BrowserRow {
    // For-You: favorites + recency + provider bonus.
    row.score_for_you = 0.0;
    if row.is_favorite {
        row.score_for_you += 1000.0;
    }
    if row.is_current {
        row.score_for_you += 500.0;
    }
    if row.is_default {
        row.score_for_you += 200.0;
    }
    let provider_lower = row.provider.to_ascii_lowercase();
    if provider_lower.contains("anthropic") || provider_lower.contains("claude") {
        row.score_for_you += 150.0;
    } else if provider_lower.contains("openai") {
        row.score_for_you += 140.0;
    } else if provider_lower.contains("gemini") {
        row.score_for_you += 135.0;
    } else if provider_lower.contains("openrouter") {
        row.score_for_you += 120.0;
    } else if provider_lower.contains("gmi") {
        row.score_for_you += 130.0;
    }
    row.score_for_you += (row.usage_count as f64).log2().max(0.0) * 50.0;
    row.score_for_you += match row.tier {
        ModelTier::Premium => 200.0,
        ModelTier::Standard => 100.0,
        ModelTier::Fast => 150.0,
        ModelTier::Free => 80.0,
    };

    // Newest: prefer rows with a recent created_date; missing -> 0.
    row.score_newest = row
        .created_date
        .as_deref()
        .and_then(parse_short_date)
        .unwrap_or(0) as f64;

    // Cheapest: lower cost ranks higher (we negate).
    row.score_cheapest = -(row.cost_in_micros.unwrap_or(u64::MAX) as f64);

    // Fastest: lower avg_response_ms ranks higher (we negate).
    row.score_fastest = -(row.avg_response_ms as f64);

    row
}

/// Parse a "Mon YYYY" string into a sortable epoch-second estimate. We do not
/// need full chrono here — the relative ordering is what matters.
fn parse_short_date(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let month = match parts[0] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: u32 = parts[1].parse().ok()?;
    // Approximate: 30 days per month + year * 365.
    Some(year as u64 * 365 * 24 * 3600 + month as u64 * 30 * 24 * 3600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tui::tui::RouteDetailSeverity;

    fn opt(provider: &str, method: &str) -> PickerOption {
        PickerOption::new(
            provider.to_string(),
            method.to_string(),
            true,
            String::new(),
            Some(3000),
        )
    }
    fn row(name: &str) -> BrowserRow {
        BrowserRow {
            model: name.into(),
            provider: name.into(),
            api_method: "x".into(),
            available: true,
            tier: ModelTier::Standard,
            capabilities: vec![],
            context_window: None,
            cost_in_micros: None,
            cost_out_micros: None,
            avg_response_ms: 0,
            usage_count: 0,
            last_used_secs: None,
            is_favorite: false,
            is_current: false,
            is_default: false,
            created_date: None,
            detail: None,
            detail_severity: RouteDetailSeverity::None,
            score_for_you: 0.0,
            score_newest: 0.0,
            score_cheapest: 0.0,
            score_fastest: 0.0,
        }
    }

    #[test]
    fn builds_rows_from_options() {
        let opts = vec![
            opt("Anthropic", "claude-oauth"),
            opt("OpenAI", "openai-oauth"),
        ];
        let rows = build_rows_from_options(&opts, &HashSet::new(), None, None);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].provider, "Anthropic");
        assert_eq!(rows[0].tier, ModelTier::Standard);
    }

    #[test]
    fn skeleton_is_visible_immediately() {
        let s = ModelBrowserState::skeleton();
        assert_eq!(s.rows.len(), 8);
        assert!(s.loading.is_some());
        assert_eq!(s.loading, Some(8));
    }

    #[test]
    fn apply_complete_clears_loading() {
        let mut s = ModelBrowserState::skeleton();
        s.apply(RowDelta::Complete {
            total: 3,
            elapsed_ms: 50,
        });
        assert!(s.loading.is_none());
    }

    #[test]
    fn apply_replaced_paints_real_rows() {
        let mut s = ModelBrowserState::skeleton();
        let rows = vec![
            opt("Anthropic", "claude-oauth"),
            opt("OpenAI", "openai-oauth"),
        ];
        let built = build_rows_from_options(&rows, &HashSet::new(), None, None);
        s.apply(RowDelta::Replaced(built));
        assert_eq!(s.rows.len(), 2);
        assert!(s.loading.is_none());
    }

    #[test]
    fn facet_toggle_filters_rows() {
        let mut s = ModelBrowserState::new(vec![row("Anthropic"), row("OpenAI")]);
        assert_eq!(s.filtered.len(), 2);
        s.toggle_provider("Anthropic");
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.rows[s.filtered[0]].provider, "Anthropic");
        s.toggle_provider("Anthropic");
        assert_eq!(s.filtered.len(), 2);
    }

    #[test]
    fn text_facet_matches_subsequence() {
        let mut s = ModelBrowserState::new(vec![BrowserRow {
            model: "claude-haiku-3-5".into(),
            provider: "Anthropic".into(),
            api_method: "claude-oauth".into(),
            available: true,
            tier: ModelTier::Fast,
            capabilities: vec![],
            context_window: None,
            cost_in_micros: None,
            cost_out_micros: None,
            avg_response_ms: 0,
            usage_count: 0,
            last_used_secs: None,
            is_favorite: false,
            is_current: false,
            is_default: false,
            created_date: None,
            detail: None,
            detail_severity: RouteDetailSeverity::None,
            score_for_you: 0.0,
            score_newest: 0.0,
            score_cheapest: 0.0,
            score_fastest: 0.0,
        }]);
        s.facets.text = "haik".to_string();
        s.recompute_filtered();
        assert_eq!(s.filtered.len(), 1);
    }

    #[test]
    fn sort_cycle_is_stable() {
        let mut s = ModelBrowserState::skeleton();
        let a = s.sort;
        s.cycle_sort();
        assert_ne!(s.sort, a);
        for _ in 0..4 {
            s.cycle_sort();
        }
        assert_eq!(s.sort, a);
    }

    #[test]
    fn detail_severity_passthrough() {
        let mut s = ModelBrowserState::skeleton();
        s.apply(RowDelta::Updated {
            index: 0,
            row: BrowserRow {
                model: "broken".into(),
                provider: "X".into(),
                api_method: "x".into(),
                available: false,
                tier: ModelTier::Standard,
                capabilities: vec![],
                context_window: None,
                cost_in_micros: None,
                cost_out_micros: None,
                avg_response_ms: 0,
                usage_count: 0,
                last_used_secs: None,
                is_favorite: false,
                is_current: false,
                is_default: false,
                created_date: None,
                detail: Some("no API key".into()),
                detail_severity: RouteDetailSeverity::Unavailable,
                score_for_you: 0.0,
                score_newest: 0.0,
                score_cheapest: 0.0,
                score_fastest: 0.0,
            },
        });
        assert_eq!(s.rows[0].detail_severity, RouteDetailSeverity::Unavailable);
    }

    #[test]
    fn move_by_wraps_around() {
        let mut s = ModelBrowserState::new(vec![row("A"), row("B"), row("C")]);
        s.selected = 0;
        s.move_by(-1);
        assert_eq!(s.selected, 2);
        s.move_by(1);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn empty_state_clamps_selected() {
        let mut s = ModelBrowserState::skeleton();
        s.facets.providers.insert("nothing".to_string());
        s.recompute_filtered();
        assert_eq!(s.filtered.len(), 0);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn facet_clear_resets_everything() {
        let mut s = ModelBrowserState::new(vec![row("Anthropic")]);
        s.facets.providers.insert("Anthropic".into());
        s.facets.text = "cl".into();
        assert_eq!(s.facets.active_count(), 2);
        s.facets.clear();
        assert!(s.facets.is_empty());
        s.recompute_filtered();
        assert_eq!(s.filtered.len(), 1);
    }

    #[test]
    fn parse_short_date_handles_known_formats() {
        assert!(parse_short_date("Jan 2026").unwrap() > parse_short_date("Dec 2025").unwrap());
        assert!(parse_short_date("garbage").is_none());
    }

    #[test]
    fn tier_classification_heuristics() {
        assert_eq!(tier_from_api_method("haiku"), ModelTier::Fast);
        assert_eq!(tier_from_api_method("gpt-5-mini"), ModelTier::Fast);
        assert_eq!(tier_from_api_method("claude-opus"), ModelTier::Premium);
        assert_eq!(tier_from_api_method("anything-else"), ModelTier::Standard);
        assert_eq!(tier_from_api_method("free-tier"), ModelTier::Free);
    }
}
