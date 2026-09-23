//! Performance gates for the model browser — Phase 5 of the TUI 100x plan.
//!
//! These tests assert concrete time budgets for the browser hot paths. They
//! run in `cargo test --release --test perf_gates` so the timings reflect
//! the build that ships to users, not the slower debug build. Each test
//! passes when the operation completes under its budget on this hardware;
//! CI hardware may need its budgets re-tuned. The budgets here target a
//! recent x86-64 laptop running the release profile.
//!
//! # Budgets (release profile)
//!
//! | operation                       | budget  |
//! |---------------------------------|---------|
//! | speculative-open (cache hit)    | <16 ms  |
//! | speculative-open (cold)         | <100 ms |
//! | one paint with 400 rows         | <4 ms   |
//! | facet toggle on 400 rows        | <2 ms   |
//! | sort-mode cycle                 | <0.5 ms |
//!
//! # Tuning
//!
//! If a budget is consistently failing on the CI host, adjust the budget
//! by editing the constant; do not silently let the gate lapse. Each gate
//! is paired with a comment explaining what it protects.

#![cfg(test)]

use std::collections::HashSet;
use std::time::{Duration, Instant};

use alphacode::tui::PickerOption;
use alphacode::tui::model_browser::ModelBrowserState;
use alphacode::tui::model_browser_open::{OpenOutcome, open_browser};

const PERF_BUDGET_CACHE_HIT: Duration = Duration::from_millis(16);
const PERF_BUDGET_COLD_OPEN: Duration = Duration::from_millis(100);
const PERF_BUDGET_PAINT_400: Duration = Duration::from_millis(4);
const PERF_BUDGET_FACET_TOGGLE: Duration = Duration::from_millis(2);
// 1000 sort cycles on 400 rows must stay under 5 ms (5 us each). The
// pre-computed score design keeps the per-cycle cost constant.
const PERF_BUDGET_SORT_CYCLE: Duration = Duration::from_millis(5);

fn make_options(n: usize) -> Vec<PickerOption> {
    (0..n)
        .map(|i| {
            PickerOption::new(
                format!("provider-{}", i % 12),
                format!("method-{}", i),
                i % 7 != 0,
                String::new(),
                Some(3000),
            )
        })
        .collect()
}

#[test]
fn cache_hit_under_16ms() {
    let options = make_options(50);
    let cached = open_browser(
        None,
        &options,
        "current-model",
        None,
        &HashSet::new(),
        None,
        || panic!("cache hit must not invoke the build closure"),
    );
    let cached = match cached {
        OpenOutcome::Skeleton { state, .. } => state,
        _ => panic!("expected skeleton"),
    };
    let slot = alphacode::tui::model_browser_open::BrowserCacheSlot {
        signature: alphacode::tui::model_browser_open::signature_from_routes(
            &options,
            "current-model",
            None,
        ),
        state: cached,
        cached_at: Instant::now(),
    };

    let started = Instant::now();
    let outcome = open_browser(
        Some(&slot),
        &options,
        "current-model",
        None,
        &HashSet::new(),
        None,
        || panic!("cache hit must not invoke the build closure"),
    );
    let elapsed = started.elapsed();
    assert!(
        matches!(outcome, OpenOutcome::CacheHit(_)),
        "expected cache hit"
    );
    assert!(
        elapsed < PERF_BUDGET_CACHE_HIT,
        "cache hit took {elapsed:?}, budget {PERF_BUDGET_CACHE_HIT:?}"
    );
}

#[test]
fn cold_open_skeleton_under_100ms() {
    let options = make_options(400);
    let options_for_closure = options.clone();
    let started = Instant::now();
    let outcome = open_browser(
        None,
        &options,
        "current-model",
        None,
        &HashSet::new(),
        None,
        move || options_for_closure.clone(),
    );
    let elapsed = started.elapsed();
    let rx = match outcome {
        OpenOutcome::Skeleton { rx, .. } => rx,
        _ => panic!("expected skeleton"),
    };
    assert!(
        elapsed < PERF_BUDGET_COLD_OPEN,
        "cold open took {elapsed:?}, budget {PERF_BUDGET_COLD_OPEN:?}"
    );
    // Drain any pending deltas so the build worker thread can exit.
    while rx.try_recv().is_ok() {}
}

#[test]
fn paint_400_rows_under_4ms() {
    use alphacode::tui::model_browser::build_rows_from_options;
    use alphacode::tui::model_browser_render::render_browser;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let options = make_options(400);
    let rows = build_rows_from_options(&options, &HashSet::new(), Some("provider-0"), None);
    let state = ModelBrowserState::new(rows);

    let backend = TestBackend::new(160, 48);
    let mut terminal = Terminal::new(backend).unwrap();

    let started = Instant::now();
    terminal
        .draw(|f| render_browser(&state, f.area(), f.buffer_mut()))
        .unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < PERF_BUDGET_PAINT_400,
        "paint 400 rows took {elapsed:?}, budget {PERF_BUDGET_PAINT_400:?}"
    );
}

#[test]
fn facet_toggle_400_rows_under_2ms() {
    use alphacode::tui::model_browser::build_rows_from_options;

    let options = make_options(400);
    let rows = build_rows_from_options(&options, &HashSet::new(), Some("provider-0"), None);
    let mut state = ModelBrowserState::new(rows);

    let started = Instant::now();
    for _ in 0..10 {
        state.toggle_provider("provider-3");
        state.toggle_provider("provider-7");
        state.toggle_tier(alphacode::tui::model_browser::ModelTier::Standard);
        state.toggle_capability(alphacode::tui::model_browser::Capability::Tools);
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < PERF_BUDGET_FACET_TOGGLE,
        "facet toggles took {elapsed:?}, budget {PERF_BUDGET_FACET_TOGGLE:?}"
    );
}

#[test]
fn sort_cycle_under_500us() {
    let options = make_options(400);
    let mut state = ModelBrowserState::new(alphacode::tui::model_browser::build_rows_from_options(
        &options,
        &HashSet::new(),
        Some("provider-0"),
        None,
    ));

    let started = Instant::now();
    for _ in 0..1000 {
        state.cycle_sort();
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < PERF_BUDGET_SORT_CYCLE,
        "1000 sort cycles took {elapsed:?}, budget {:?}",
        PERF_BUDGET_SORT_CYCLE * 1000
    );
}
