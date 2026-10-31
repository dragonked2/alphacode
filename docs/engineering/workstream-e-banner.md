# Workstream E — banner and ASCII art: execution record

Source plan: `docs/comprehensive-improvement-plan.md` §8.
Scope delivered here: E1 (single source of truth), E2 (CLI/REPL banner), E3
(width-sweep tests), E5 (gradient consistency), E6 (documentation).
Tier rules themselves are documented in `docs/branding.md`.

## What was wrong

The plan's premise — that surfaces "may not render the shared art" — was still
true in three places, and one of them was worse than drift:

1. **A dead, ragged private banner in core.** `alphacode_core::output_style`
   carried `startup_banner` and `compact_header` with their own hand-assembled
   box-drawing wordmark. Measured, its three art rows were 33, 32 and 41 cells
   wide: it spelled nothing, and would have rendered as `╔═╗╔═╗╔╦╗` garbage.
   Both functions had **zero callers** — the defect the shared brand module was
   created to prevent, left behind as dead code.
2. **The TUI header kept a private tier list** (`ALPHA_BANNERS`) and re-ran the
   width comparison itself instead of calling the shared selector.
3. **The interactive REPL showed no wordmark at all**, only
   `AlphaCode - Coding Agent`.

Three more surfaces had drifted on the *name* rather than the art, all producing
`AlphaCode · alphacode Fox` — the product named twice, the second time in the
lowercase slug: the resume-window path, the live TUI window-title sync, and the
TUI's own private copy of `resumed_window_title`. The window title is the one
piece of chrome that escapes the terminal (tab bars, window switchers, menubar),
so the typo had the widest reach of any string in the product.

Separately, the splash tagline overran the terminal at every width from 17 to
24 and disappeared below 17: the short tagline is 27 cells and was gated on a
fixed `> 55`/else split, while the minimal wordmark needs only 13.

## What changed

- Deleted `compact_header` and `startup_banner` from core along with the five
  colour constants only they used, and removed `compact_header` from the
  `alphacode_base` re-export list.
- `core::output_style::startup_banner` is now the **only** banner implementation
  (with `startup_banner_with_unicode` as its testable seam). `cli::output`
  re-exports it, so the `run` banner, the REPL and any future entry point share
  one renderer. The `app_core` REPL calls core directly — `app_core` must not
  depend on `cli`.
- The REPL prints the shared banner, falling back to
  `AlphaCode - Coding Agent` when no tier fits.
- `build_alpha_banner` calls `alphacode_brand::compact_wordmark_for`.
- Added `process_title::terminal_fallback_label`, which deliberately omits the
  product name because `terminal_window_title` already adds it. The resume path,
  the live TUI sync and the TUI helper all call it; the TUI's duplicate
  `resumed_window_title` was deleted in favour of the shared one.
- Window titles for new sessions and desktop notification titles use
  `alphacode_brand::DISPLAY_NAME`.
- The header wordmark now uses `BrandTheme::gradient_rows` (the splash's
  continuous ramp) instead of sampling the 16-stop array per row, which gave a
  multi-row logo only a few distinct colours and drew hard bands.
- Splash accessory rows are gated on their **measured** width
  (`full_tagline_width`, `compact_tagline_width`, `elite_feature_row_width`).
  Below the short tagline's width the splash renders the wordmark alone.

## Tests added or corrected

- `alphacode_brand`: `wordmark_for` and `compact_wordmark_for` never return a
  tier wider than requested, swept over every width 0..=200; the persistent
  selector never exceeds the splash selection; existing ladder tests unchanged.
- `brand_ux`: `splash_rows_fit_every_width_from_0_to_200` and
  `splash_is_empty_below_the_minimal_wordmark` — the first would have caught the
  tagline overflow, which the previous sample list (30, 40, 55, 75, 80, 120)
  skipped.
- `process_title`: the fallback label never repeats the product name, and the
  composed title names the product once in branded case.
- `ui_header`: the tier-consistency test now asserts against the shared ladder
  rather than the deleted private list.
- `turn_notify`: three assertions updated to the branded title.

## Verification performed

Syntax and formatting of every edited file were checked with
`rustfmt --edition 2024 --check <file>`. The checker was first validated against
a deliberately malformed file to confirm it exits non-zero on parse errors.

```text
rustfmt --edition 2024 --check src/alphacode_brand.rs                                  exit=0
rustfmt --edition 2024 --check src/alphacode_core/output_style.rs                      exit=0
rustfmt --edition 2024 --check src/alphacode_base/output_style.rs                      exit=0
rustfmt --edition 2024 --check src/alphacode_base/process_title.rs                     exit=0
rustfmt --edition 2024 --check src/alphacode_app_core/session_launch.rs                exit=0
rustfmt --edition 2024 --check src/alphacode_app_core/agent/turn_execution.rs          exit=0
rustfmt --edition 2024 --check src/alphacode_tui/tui/app/helpers.rs                    exit=0
rustfmt --edition 2024 --check src/alphacode_tui/tui/app/tui_lifecycle_runtime.rs      exit=0
rustfmt --edition 2024 --check src/alphacode_tui/tui/app/turn_notify.rs                exit=0
rustfmt --edition 2024 --check src/alphacode_tui/tui/ui_header.rs                      exit=0
rustfmt --edition 2024 --check src/alphacode_tui/tui/brand_ux.rs                       exit=0
rustfmt --edition 2024 --check src/cli/output.rs                                       exit=0
```

Static consistency sweeps confirmed no remaining reference to `compact_header`,
`ALPHA_BANNERS`, or the deleted colour constants, and no compile-affecting
change to any symbol outside this workstream.

## Not verified (must be run before release)

`cargo test`, `cargo clippy -- -D warnings` and a build were **not** run for this
change, at the operator's instruction to avoid long-running checks. The following
remain unconfirmed by execution:

- that the workspace compiles (rustfmt only proves each file parses);
- that the new tests pass, in particular the 0..=200 sweeps;
- clippy cleanliness, including whether the loosened `alphacode_base`
  re-export list leaves any unused import.

Recommended gate before merge:

```text
cargo fmt --all -- --check
cargo check --workspace --tests
cargo test --lib alphacode_brand brand_ux process_title ui_header turn_notify
cargo clippy --lib -- -D warnings
```

## Known remaining work in this workstream

- Window titles passed to terminal emulators (`setup_hints/windows_setup.rs`,
  and two test fixtures in `terminal_launch`) still use the lowercase slug;
  those are process/window-manager labels rather than in-app branding and were
  left alone deliberately.
- `process_title::initial_title` keeps the `alphacode:*` process-title prefixes
  in lowercase. Those are protocol values parsed elsewhere, not display chrome.