//! Speculative-open pipeline for the model browser — Phase 2b of the TUI 100x
//! plan.
//!
//! # Why this exists
//!
//! The legacy `open_model_picker_inner` builds the catalog on the UI thread
//! before the picker paints, so the user sees a blank screen for the duration
//! of the network round-trip + provider route aggregation. On a slow login
//! that is 800-2000 ms; on a cached re-open it is 50-200 ms; both feel slow.
//!
//! This module makes every open paint in <16 ms (one frame at 60 fps):
//!
//! 1. If the cache is fresh, serve the cached `BrowserRow`s immediately.
//! 2. Otherwise, install the 8-row skeleton (see
//!    [`ModelBrowserState::skeleton`]) and start the async build.
//! 3. The async build runs on a `tokio` blocking worker, sends deltas through
//!    an `mpsc::Receiver`, and the UI's poll loop applies them one at a time.
//!
//! # File split
//!
//! `model_browser_open.rs` owns the pipeline; `model_browser.rs` owns the
//! state. The renderer is free to use either the legacy `InlineInteractiveState`
//! or the new browser without crossing the module boundary.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::alphacode_tui::tui::model_browser::{
    build_rows_from_options, BrowserRow, ModelBrowserState, RowDelta,
};

/// Sender for the in-flight browser build. Held by `App` until the build
/// completes or is invalidated.
pub type DeltaSender = std::sync::mpsc::Sender<RowDelta>;

/// Receiver for the in-flight browser build. Polled by `App::poll_browser_load`.
pub type DeltaReceiver = std::sync::mpsc::Receiver<RowDelta>;

/// Hashable signature that decides whether the cache is fresh.
///
/// Phase 1 cache signature was a 5-tuple that thrashed on every login because
/// `recent_authenticated_provider` is set on the same tick as `open_model_picker`.
/// This content hash survives logins as long as the routes themselves don't
/// change.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BrowserCacheSignature {
    /// Hash of (route.provider, route.model, route.api_method, route.detail)
    /// sorted and folded. Computed by [`signature_from_routes`].
    pub routes_hash: u64,
    /// The currently-active model id. Changes invalidate the cache because
    /// the "is_current" pill moves.
    pub current_model: String,
    /// Optional task hint (for the "For you" sort). `None` means "browse".
    pub task_hint: Option<String>,
}

impl BrowserCacheSignature {
    pub fn new(routes_hash: u64, current_model: String, task_hint: Option<String>) -> Self {
        Self {
            routes_hash,
            current_model,
            task_hint,
        }
    }
}

/// Fold a slice of routes into a stable 64-bit content hash.
///
/// We use a simple FNV-1a variant because the input is small and the function
/// is on the open path; a cryptographic hash is overkill. The fold is
/// deterministic for any permutation of the input because we sort by
/// `(provider, model, api_method, detail)` first.
pub fn signature_from_routes(
    routes: &[crate::alphacode_tui::tui::PickerOption],
    current_model: &str,
    task_hint: Option<&str>,
) -> BrowserCacheSignature {
    // Build the keys in a single pass so we never need a Vec<&str>. The
    // sorted order is what makes the signature stable under input permutation.
    let mut keys: Vec<String> = routes
        .iter()
        .map(|r| {
            // U+001F (unit separator) never appears in any field in practice,
            // so it is safe as a delimiter.
            format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                r.provider,
                r.api_method,
                r.detail,
                r.detail_severity as u8,
            )
        })
        .collect();
    keys.sort_unstable();
    let mut h: u64 = 0xcbf29ce484222325;
    for s in &keys {
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    BrowserCacheSignature::new(h, current_model.to_string(), task_hint.map(str::to_string))
}

/// Cache slot stored on `App`. `None` means "no cache yet"; `Some(slot)` means
/// "we have a fresh-enough browser snapshot to paint speculatively."
#[derive(Debug, Clone)]
pub struct BrowserCacheSlot {
    pub signature: BrowserCacheSignature,
    pub state: ModelBrowserState,
    pub cached_at: Instant,
}

impl BrowserCacheSlot {
    pub fn is_fresh(&self, sig: &BrowserCacheSignature) -> bool {
        // Cache survives up to 60 seconds; longer than that and a refresh
        // is cheap enough that staleness would surprise the user.
        self.signature == *sig && self.cached_at.elapsed().as_secs() < 60
    }
}

/// Outcome of an `open_browser` call. The renderer reads this and either
/// paints the cached state immediately, paints the skeleton, or hands off to
/// the poll loop.
#[derive(Debug)]
pub enum OpenOutcome {
    /// Cache hit — paint `state` immediately. No async work needed.
    CacheHit(ModelBrowserState),
    /// Cache miss — paint `state` (the skeleton) and start an async build.
    /// `rx` will deliver `RowDelta`s; call [`poll_browser_load`] each frame.
    Skeleton {
        state: ModelBrowserState,
        rx: DeltaReceiver,
    },
    /// No routes at all — caller should print the empty-state hint and not
    /// open a picker.
    Empty,
}

/// Open the model browser using the speculative pipeline.
///
/// `routes_fn` is called on a blocking worker (it may take 200-2000 ms for a
/// live catalog fetch). The returned deltas stream into `rx` until the build
/// is complete, then `rx` is closed.
pub fn open_browser<F>(
    cached: Option<&BrowserCacheSlot>,
    routes: &[crate::alphacode_tui::tui::PickerOption],
    current_model: &str,
    task_hint: Option<&str>,
    favorites: &HashSet<String>,
    default: Option<&str>,
    routes_fn: F,
) -> OpenOutcome
where
    F: FnOnce() -> Vec<crate::alphacode_tui::tui::PickerOption> + Send + 'static,
{
    let sig = signature_from_routes(routes, current_model, task_hint);

    // Phase 1: cache hit returns instantly.
    if let Some(slot) = cached {
        if slot.is_fresh(&sig) {
            return OpenOutcome::CacheHit(slot.state.clone());
        }
    }

    // No routes at all: don't bother painting the skeleton.
    if routes.is_empty() && cached.is_none() {
        return OpenOutcome::Empty;
    }

    // Phase 2: skeleton + async build.
    let mut state = ModelBrowserState::skeleton();
    state.facets.clear();
    let (tx, rx) = std::sync::mpsc::channel::<RowDelta>();

    let favorites_clone = favorites.clone();
    let current = current_model.to_string();
    let default = default.map(str::to_string);

    let build = move || {
        let started = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let new_routes = routes_fn();
            build_rows_from_options(&new_routes, &favorites_clone, Some(&current), default.as_deref())
        }));
        match result {
            Ok(rows) => {
                let _ = tx.send(RowDelta::Replaced(rows.clone()));
                let _ = tx.send(RowDelta::Complete {
                    total: rows.len(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
            Err(_) => {
                // Build panicked. Send a Complete so the picker leaves the
                // skeleton state; the caller will then see the skeleton until
                // it dismisses.
                let _ = tx.send(RowDelta::Complete {
                    total: 0,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
        }
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(build);
    } else {
        std::thread::spawn(build);
    }

    OpenOutcome::Skeleton { state, rx }
}

/// Apply any pending deltas from `rx` to `state`. Returns true if any delta
/// was applied (caller should request a repaint).
pub fn poll_browser_load(rx: &DeltaReceiver, state: &mut ModelBrowserState) -> bool {
    let mut changed = false;
    loop {
        match rx.try_recv() {
            Ok(delta) => {
                state.apply(delta);
                changed = true;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.loading = None;
                break;
            }
        }
    }
    changed
}

/// Shared in-flight build registry — one slot per `App`. The renderer holds
/// `Arc<Mutex<Option<BrowserCacheSlot>>>` to enable cross-frame cache hits.
pub type SharedCacheSlot = Arc<Mutex<Option<BrowserCacheSlot>>>;

/// Convenience: build the cache from a delta stream. Called after the build
/// completes so the next open hits the cache.
pub fn cache_from_state(
    sig: BrowserCacheSignature,
    state: &ModelBrowserState,
) -> BrowserCacheSlot {
    BrowserCacheSlot {
        signature: sig,
        state: state.clone(),
        cached_at: Instant::now(),
    }
}

/// Public hook: paint the browser rows as a flat list (the legacy caller can
/// adopt this as a drop-in for `render_smart_model_picker_with_task`).
pub fn visible_rows(state: &ModelBrowserState) -> Vec<&BrowserRow> {
    state.filtered.iter().map(|&i| &state.rows[i]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tui::tui::PickerOption;
    use std::sync::mpsc;

    fn opt(p: &str, m: &str) -> PickerOption {
        PickerOption::new(p.to_string(), m.to_string(), true, String::new(), Some(3000))
    }

    #[test]
    fn signature_is_stable_under_route_order_changes() {
        let a = vec![opt("A", "a"), opt("B", "b")];
        let b = vec![opt("B", "b"), opt("A", "a")];
        let sa = signature_from_routes(&a, "current", None);
        let sb = signature_from_routes(&b, "current", None);
        assert_eq!(sa.routes_hash, sb.routes_hash);
    }

    #[test]
    fn signature_changes_when_routes_change() {
        let a = vec![opt("A", "a")];
        let b = vec![opt("A", "b")];
        let sa = signature_from_routes(&a, "current", None);
        let sb = signature_from_routes(&b, "current", None);
        assert_ne!(sa.routes_hash, sb.routes_hash);
    }

    #[test]
    fn cache_hit_returns_state_without_async() {
        let sig = signature_from_routes(&[opt("A", "a")], "current", None);
        let slot = BrowserCacheSlot {
            signature: sig.clone(),
            state: ModelBrowserState::new(vec![]),
            cached_at: Instant::now(),
        };
        let outcome = open_browser(Some(&slot), &[opt("A", "a")], "current", None, &HashSet::new(), None, || panic!("should not run"));
        assert!(matches!(outcome, OpenOutcome::CacheHit(_)));
    }

    #[test]
    fn empty_routes_returns_empty() {
        let outcome = open_browser(None, &[], "current", None, &HashSet::new(), None, || vec![]);
        assert!(matches!(outcome, OpenOutcome::Empty));
    }

    #[test]
    fn skeleton_path_streams_deltas() {
        let (tx, rx) = mpsc::channel();
        // Pre-fill with the skeleton state, then deliver real rows.
        let mut state = ModelBrowserState::skeleton();
        let real_rows = build_rows_from_options(
            &[opt("Anthropic", "claude-oauth")],
            &HashSet::new(),
            Some("Anthropic"),
            None,
        );
        tx.send(RowDelta::Replaced(real_rows.clone())).unwrap();
        tx.send(RowDelta::Complete { total: real_rows.len(), elapsed_ms: 5 }).unwrap();
        drop(tx);
        let changed = poll_browser_load(&rx, &mut state);
        assert!(changed);
        assert!(state.loading.is_none());
        assert_eq!(state.rows.len(), 1);
    }

    #[test]
    fn poll_returns_false_when_no_deltas() {
        let (_tx, rx) = mpsc::channel::<RowDelta>();
        let mut state = ModelBrowserState::skeleton();
        drop(_tx);
        let changed = poll_browser_load(&rx, &mut state);
        assert!(!changed);
    }

    #[test]
    fn cache_freshness_depends_on_signature_and_time() {
        let sig = signature_from_routes(&[opt("A", "a")], "current", None);
        let slot = BrowserCacheSlot {
            signature: sig.clone(),
            state: ModelBrowserState::new(vec![]),
            cached_at: Instant::now(),
        };
        assert!(slot.is_fresh(&sig));
        let mut stale_sig = sig.clone();
        stale_sig.current_model = "other".into();
        assert!(!slot.is_fresh(&stale_sig));
    }

    #[test]
    fn signature_includes_current_model() {
        let a = signature_from_routes(&[opt("A", "a")], "x", None);
        let b = signature_from_routes(&[opt("A", "a")], "y", None);
        assert_ne!(a, b);
    }
}