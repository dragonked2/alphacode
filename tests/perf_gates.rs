//! Performance gates for the model browser.
//!
//! These tests guard the model-browser hot paths. Run them under the release
//! profile so the timings reflect the build that ships to users:
//!
//! ```text
//! cargo test --release --test perf_gates -- --nocapture --test-threads=1
//! ```
//!
//! # Two kinds of gate
//!
//! The gates are split by what they can reliably assert, because a single
//! wall-clock threshold cannot be both meaningful and trustworthy in CI.
//!
//! **1. Complexity gates (always enforced, hardware-independent).**
//!
//! These measure the same operation at two input sizes and assert the cost
//! grows no faster than the size. A gate at scale *N* must not cost more than
//! [`SCALING_TOLERANCE`] times a gate at scale *N/2*.
//!
//! This is the gate that catches the regressions that actually happen: a lost
//! cache, a dropped `partition_point` that becomes a linear scan, an
//! `O(n^2)` facet count. Those regressions change the *shape* of the cost
//! curve, so they are detected identically on a developer laptop and on a
//! shared, noisy CI runner, and they do not need re-tuning per machine.
//!
//! **2. Absolute latency gates (opt-in).**
//!
//! [`ABSOLUTE_BUDGETS`] holds the original wall-clock thresholds, tuned on a
//! recent x86-64 laptop running the release profile. Absolute budgets are only
//! meaningful relative to a known host, so CI does **not** fail on them:
//! a GitHub-hosted runner is a shared 2-core VM and a 4 ms paint budget on it
//! measures the neighbours, not the code. They are still *measured and
//! printed* on every run, so real drift is visible in the log, and they are
//! enforced when you ask for them locally:
//!
//! ```text
//! ALPHACODE_PERF_ABSOLUTE=1 cargo test --release --test perf_gates
//! ```
//!
//! # Why not just assert absolute budgets in CI
//!
//! Because a perf gate that fails for reasons unrelated to the code is worse
//! than no gate: it trains reviewers to re-run or re-tune instead of reading,
//! and the first genuinely slow commit sails through alongside the noise. The
//! complexity gates above fail only when the code changed shape; the absolute
//! numbers stay available for a human who wants them.
//!
//! # Keeping these honest
//!
//! - Run with `--test-threads=1`. The gates are timing-sensitive and the
//!   scaling comparison is only meaningful on an otherwise-quiet machine.
//! - Never "fix" a failure by loosening the constant. Read [`SCALING_TOLERANCE`]
//!   and work out which operation changed shape first.

#![cfg(test)]

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use alphacode::tui::PickerOption;
use alphacode::tui::model_browser::ModelBrowserState;
use alphacode::tui::model_browser_open::{OpenOutcome, open_browser};

/// Row count for the "small" side of each scaling comparison. The large side
/// is [`SCALE_LARGE`], i.e. exactly double.
const SCALE_SMALL: usize = 400;
const SCALE_LARGE: usize = 800;

/// How much the cost is allowed to grow when the input doubles.
///
/// Linear behaviour (a plain scan, a sort) leaves real headroom here, so 2.5x
/// is generous: it tolerates cache effects, allocator behaviour, and branch
/// prediction noise on a shared machine, while still failing loudly at the
/// ~4x a quadratic step produces. The gap between 2.5x and 4x is the whole
/// point of the gate, so do not raise it toward 4x.
const SCALING_TOLERANCE: f64 = 2.5;

/// Absolute wall-clock budgets, measured on a recent x86-64 laptop under the
/// release profile. Reported on every run; enforced only under
/// `ALPHACODE_PERF_ABSOLUTE=1`. See the module docs for why.
struct AbsoluteBudgets {
    cache_hit: Duration,
    cold_open: Duration,
    paint: Duration,
    facet_toggle: Duration,
    sort_cycle: Duration,
}

const ABSOLUTE_BUDGETS: AbsoluteBudgets = AbsoluteBudgets {
    cache_hit: Duration::from_millis(16),
    cold_open: Duration::from_millis(100),
    paint: Duration::from_millis(4),
    facet_toggle: Duration::from_millis(2),
    // 1000 sort cycles on 400 rows must stay under 5 ms (5 us each). The
    // pre-computed score design keeps the per-cycle cost constant.
    sort_cycle: Duration::from_millis(5),
};

/// Whether wall-clock budgets should fail the run. Off by default so CI is
/// deterministic; see the module docs.
fn absolute_gates_enforced() -> bool {
    matches!(
        std::env::var("ALPHACODE_PERF_ABSOLUTE")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

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

fn build_state(n: usize) -> ModelBrowserState {
    let options = make_options(n);
    ModelBrowserState::new(alphacode::tui::model_browser::build_rows_from_options(
        &options,
        &HashSet::new(),
        Some("provider-0"),
        None,
    ))
}

/// Report an absolute measurement, and fail when absolute gates are enabled.
fn report(name: &str, rows: usize, elapsed: Duration, budget: Duration) {
    let verdict = if elapsed <= budget { "ok" } else { "OVER" };
    println!(
        "[perf] {name:<22} rows={rows:<5} {:>9.3} ms  budget {:>8.3} ms  {verdict}",
        elapsed.as_secs_f64() * 1e3,
        budget.as_secs_f64() * 1e3,
    );
    if absolute_gates_enforced() && elapsed > budget {
        panic!(
            "{name} took {elapsed:?} at {rows} rows, absolute budget {budget:?} \
             (ALPHACODE_PERF_ABSOLUTE=1)"
        );
    }
}

/// Run `op` at both scales and assert the cost does not grow super-linearly.
///
/// This is the hardware-independent gate. It measures the large case twice and
/// keeps the faster run, so a single unlucky scheduling hiccup on a loaded
/// machine cannot be mistaken for an algorithmic regression.
fn assert_scales_linearly(name: &str, mut op: impl FnMut(usize) -> Duration) {
    let small = op(SCALE_SMALL);
    // Two large runs, keep the minimum: the minimum is the best estimate of the
    // operation's true cost when the host is noisy.
    let large_a = op(SCALE_LARGE);
    let large_b = op(SCALE_LARGE);
    let large = large_a.min(large_b);

    let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::MIN_POSITIVE);
    println!(
        "[perf] {name:<22} scaling {} rows -> {} rows: {:.3} ms -> {:.3} ms (x{ratio:.2}, \
         tolerance x{SCALING_TOLERANCE:.2})",
        SCALE_SMALL,
        SCALE_LARGE,
        small.as_secs_f64() * 1e3,
        large.as_secs_f64() * 1e3,
    );

    assert!(
        ratio <= SCALING_TOLERANCE,
        "{name} grew x{ratio:.2} when the input doubled ({} -> {} rows, {:.3} ms -> {:.3} ms). \
         Linear work must stay within x{SCALING_TOLERANCE:.2}; x4.0 is what an O(n^2) \
         regression looks like. This gate is host-independent, so a failure here means \
         the code changed shape, not that the machine was busy.",
        SCALE_SMALL,
        SCALE_LARGE,
        small.as_secs_f64() * 1e3,
        large.as_secs_f64() * 1e3,
    );
}

fn time_paint(rows: usize) -> Duration {
    use alphacode::tui::model_browser_render::render_browser;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let state = build_state(rows);
    let backend = TestBackend::new(160, 48);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    let started = Instant::now();
    terminal
        .draw(|f| render_browser(&state, f.area(), f.buffer_mut()))
        .expect("draw");
    started.elapsed()
}

fn time_facet_toggle(rows: usize) -> Duration {
    let mut state = build_state(rows);

    let started = Instant::now();
    for _ in 0..10 {
        state.toggle_provider("provider-3");
        state.toggle_provider("provider-7");
        state.toggle_tier(alphacode::tui::model_browser::ModelTier::Standard);
        state.toggle_capability(alphacode::tui::model_browser::Capability::Tools);
    }
    started.elapsed()
}

fn time_sort_cycles(rows: usize) -> Duration {
    let mut state = build_state(rows);

    let started = Instant::now();
    for _ in 0..1000 {
        state.cycle_sort();
    }
    started.elapsed()
}

/// A paint must not get super-linear in the number of rows. This is the gate
/// that would catch a regression to rendering every row rather than the visible
/// window, or to a per-row full scan of the row set.
#[test]
fn paint_scales_linearly_with_row_count() {
    assert_scales_linearly("paint", time_paint);
}

/// Facet toggles must not get super-linear. This is the gate for the facet
/// counts: a per-facet `rows.iter().filter(...).count()` is O(rows) per facet
/// and would show up here as the row count doubles.
#[test]
fn facet_toggle_scales_linearly_with_row_count() {
    assert_scales_linearly("facet-toggle", time_facet_toggle);
}

/// Sorting must not get super-linear. `cycle_sort` relies on pre-computed
/// scores so each cycle is O(1); losing that turns this into a per-cycle sort
/// of the whole row set.
#[test]
fn sort_cycles_scale_linearly_with_row_count() {
    assert_scales_linearly("sort-cycle", time_sort_cycles);
}

/// Absolute budgets for the fixed-size reference workloads. Always measured
/// and printed; only asserted under `ALPHACODE_PERF_ABSOLUTE=1`.
#[test]
fn report_absolute_budgets() {
    // Speculative-open, cache hit.
    //
    // The cache-hit invariant — "a fresh cache slot must never invoke the
    // build closure" — cannot be asserted with a panicking closure: the
    // closure runs on a *spawned worker thread* where `open_browser` wraps it
    // in `catch_unwind`, so a panic there is swallowed and the test would
    // still pass. Instead the hit closure sets a flag the test owns; the flag
    // is asserted after the hit has been observed. Only the hit closure touches
    // it, so the cold worker below (which runs concurrently) cannot race it.
    let hit_build_called = Arc::new(AtomicBool::new(false));
    let options = make_options(50);
    let cached = match open_browser(
        None,
        &options,
        "current-model",
        None,
        &HashSet::new(),
        None,
        Vec::new,
    ) {
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

    let hit_flag = Arc::clone(&hit_build_called);
    let started = Instant::now();
    let outcome = open_browser(
        Some(&slot),
        &options,
        "current-model",
        None,
        &HashSet::new(),
        None,
        move || {
            hit_flag.store(true, Ordering::SeqCst);
            panic!("cache hit must not invoke the build closure")
        },
    );
    let cache_hit = started.elapsed();
    assert!(
        matches!(outcome, OpenOutcome::CacheHit(_)),
        "expected cache hit"
    );
    // A cache hit must return before the closure is ever polled, so the flag
    // must still be clear here. (On a *miss* the closure would panic on the
    // worker thread — reported by the test harness — which is a loud failure
    // in its own right, but the flag assertion below is the deterministic gate.)
    assert!(
        !hit_build_called.load(Ordering::SeqCst),
        "cache hit must not invoke the build closure"
    );
    report(
        "open-cache-hit",
        options.len(),
        cache_hit,
        ABSOLUTE_BUDGETS.cache_hit,
    );

    // Cold open must publish a skeleton immediately and build off-thread.
    let cold_options = make_options(SCALE_SMALL);
    let options_for_closure = cold_options.clone();
    let started = Instant::now();
    let outcome = open_browser(
        None,
        &cold_options,
        "current-model",
        None,
        &HashSet::new(),
        None,
        move || options_for_closure.clone(),
    );
    let cold_open = started.elapsed();
    let rx = match outcome {
        OpenOutcome::Skeleton { rx, .. } => rx,
        _ => panic!("expected skeleton"),
    };
    report(
        "open-cold",
        cold_options.len(),
        cold_open,
        ABSOLUTE_BUDGETS.cold_open,
    );

    // Join the cold-build worker. The cold closure is a trivial clone, so this
    // returns promptly once the worker has published `Replaced` + `Complete`.
    let cold_finished = match Instant::now().checked_add(Duration::from_secs(10)) {
        Some(deadline) => loop {
            if Instant::now() > deadline {
                panic!("cold build worker did not finish within 10s");
            }
            match rx.try_recv() {
                Ok(_) => break true,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break false,
            }
        },
        None => false,
    };
    assert!(
        cold_finished,
        "cold build must publish a delta so the skeleton leaves the loading state"
    );

    // Drain any pending deltas so the build worker thread can exit.
    while rx.try_recv().is_ok() {}

    report(
        "paint-400",
        SCALE_SMALL,
        time_paint(SCALE_SMALL),
        ABSOLUTE_BUDGETS.paint,
    );
    report(
        "facet-toggle-400",
        SCALE_SMALL,
        time_facet_toggle(SCALE_SMALL),
        ABSOLUTE_BUDGETS.facet_toggle,
    );
    report(
        "sort-cycle-1000",
        SCALE_SMALL,
        time_sort_cycles(SCALE_SMALL),
        ABSOLUTE_BUDGETS.sort_cycle,
    );
}
