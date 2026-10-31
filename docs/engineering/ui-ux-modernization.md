# UI/UX Modernization — Engineering Checklist

Durable task state for the TUI/UX polish effort. Update after each milestone;
recover from this file when session context is lost.

## Objectives and non-negotiables

- Audit, improve, and polish the AlphaCode TUI without regressing features.
- **AlphaX requirement (mandatory):** under the AlphaX provider
  (`alphax-free`, displayed "Alphax Free"), the user sees exactly one
  model-selection choice: `Automatic`. No individual free models, no model
  lists, no duplicated entries. Internal automatic routing must keep working.
- Preserve working features, public APIs, config formats, and other
  providers' behavior. No fake functionality; no broad rewrites.
- Validate with the commands in `AGENTS.md`; never report unverified success.

## Architecture (as inspected)

- Single Cargo package `alphacode` (manifest at repo root). Modules live in
  `src/alphacode_*` (not separate packages). TUI framework: `ratatui`-style
  widgets under `src/alphacode_tui/`, `src/alphacode_tui_*`.
- Model-selection flow:
  - Provider profiles: `src/alphacode_provider_metadata/catalog.rs`
    (`ALPHAX_FREE_PROFILE`, display name, default model via `route_cloak`).
  - Route identifiers are cloaked: `src/alphacode_provider_metadata/route_cloak.rs`
    (`free_route_default_model()` = the automatic routing alias).
  - Static per-profile model lists: `src/alphacode_base/provider_catalog.rs`
    → `openai_compatible_profile_static_models` ("alphax-free" pushes only
    the automatic alias); env plumbing in `apply_openai_compatible_profile_env_impl`
    disables the live catalog for alphax-free (`..._MODEL_CATALOG=0`).
  - Runtime model lists: `src/alphacode_provider_openrouter_runtime/openrouter_provider_impl.rs`
    → `available_models_display()`; `augment_free_pool_models()` strips every
    free-pool member and the alternate alias from all display paths
    (static, memory/disk cache, live catalog, current-model insertion).
  - Rotation: `src/alphacode_app_core/agent/free_pool_rotation.rs` +
    `src/alphacode_provider_metadata/free_pool.rs` (price-verified free pool;
    alias is the floor member).
  - UI labels: `src/alphacode_tui/tui/app/helpers/model_names.rs`
    (`pretty_model_display_name` prose, `pretty_known_model_family` picker).
  - Model cycling: `src/alphacode_tui/tui/app/model_context.rs::cycle_model`
    uses `available_models_for_switching()` (== display list).
  - Config: `src/alphacode_base/config/config_file.rs`
    `normalize_legacy_alphax_model_selection()` normalizes stale saved
    selections to the alias on load.
  - Post-login selection: `src/alphacode_base/auth/lifecycle.rs`
    (`alphax_free_flagship_order` = alias first).
- Picker surfaces: legacy inline picker
  (`src/alphacode_tui/tui/app/inline_interactive.rs`,
  `ui_inline_interactive.rs`) plus parallel faceted browser
  (`src/alphacode_tui/tui/model_browser*.rs`) built from `PickerOption`s
  derived from `model_routes()`.

## AlphaX automatic-only: implementation state

Done (working tree, verified by tests):

1. Static list exposes only the automatic alias
   (`openai_compatible_profile_static_models`).
2. `augment_free_pool_models` strips pool members / alternate alias from
   every selection surface, including current-model insertion; rotation
   still accepts member ids as live routing targets.
3. Live `/v1/models` catalog disabled for the profile; only the
   price-verified background refresh (rotation-only) reads it.
4. Legacy config normalization to the alias on load (no file rewrite).
5. Model cycling/switching reads the filtered list; other providers
   unaffected.
6. Regression tests: `provider_catalog_tests.rs`
   (`alphax_free_static_models_expose_only_the_automatic_route`,
   `other_profiles_keep_their_static_model_lists`),
   `openrouter_runtime/mod.rs`
   (`alphax_free_picker_exposes_only_the_automatic_alias`),
   `config_file.rs` (`legacy_alphax_free_model_selections_normalize_to_automatic`).

Done (this session):

7. The single choice is labeled `Automatic` everywhere:
   `internal_model_display_name` now returns "Automatic" for the alias
   (picker row, header/status, switch notices, diagnostics); picker suffix
   " (automatic)" removed (`model_names.rs`); tests updated.
8. Post-login messages no longer advertise manual model choice under
   AlphaX (`auth.rs`, two messages); copy now states automatic routing.
9. Header model line shows `Automatic` for the alias *and* rotated pool
   members (`ui_header.rs::header_model_display_name` maps free-route ids
   before the slashed-id branch, which previously printed fragments of the
   cloaked route); regression test added.
10. Status-bar/input compact label shows `Automatic` for free-route ids
    (`ui_status.rs::shorten_model_name` previously mashed the alias to a
    route fragment).
11. Info-widget model label no longer strips the `vendor/` prefix from
    namespaced catalog ids (`session_facts.rs::strip_path_prefix` now only
    stems genuine `.gguf`/`.bin` local-weights paths); the alias renders as
    `Automatic`; regression tests added.
12. `/model` autocomplete never suggests free-route ids: the alias
    collapses to a bare `/model` suggestion (opens the picker with the
    single `Automatic` entry; `/model Automatic` is not executable) and
    rotated pool members are suppressed (`state_ui_input_helpers.rs::
    model_switch_suggestion`, unit-tested). `@`-provider pinning
    suggestions are also suppressed for free-route ids (they would print
    the cloaked route and offer pinning the gateway does not support).
13. The Phase-2 faceted model browser (`model_browser.rs`, not yet wired
    into the live flow) masks internal virtual ids in `pretty_name()` so
    the future surface shows `Automatic` rather than the cloaked route;
    test added.

Remaining (AlphaX):

- One compile + focused test pass over the changed files (deferred while
  iterating; see Validation below).

## Other TUI work in flight (pre-existing, preserved)

- Streaming/redraw reliability: `turn_loops.rs`, `turn_streaming_mpsc.rs`,
  `response_recovery.rs`, `stream_buffer.rs`.
- Bundled security skills added under `src/alphacode_base/bundled_skills/`.
- System prompt slim-down (`prompt/system_prompt.md`).
- Legacy docs removed (`docs/*.md` deletions in working tree).

## Remaining work (beyond AlphaX)

- Theme token consolidation and layout polish (Phase 2+ of the master
  prompt) — audit only; no changes made yet this session.
- Narrow-terminal and streaming behavior spot checks.

## Validation commands and results

Environment: VS 2022 MSVC env loaded; `CARGO_TARGET_DIR=C:\Users\or0to\AppData\Local\Temp\alphacode-target`.

| Command | Result |
| --- | --- |
| `cargo check --workspace` | pass (after AlphaX label + copy changes) |
| `cargo check --workspace --tests` | pass (twice: baseline and after all display-surface fixes) |
| `cargo fmt --all` | applied (fixed a pre-existing unformatted in-flight `registry.rs`; repo gate `--check` was failing before) |
| focused `cargo test --lib` (alphax / pretty_names / pretty_model / model_switch_suggestion / pretty_name_masks / header_shows_automatic / header_model_display_name / legacy_alphax) | **pass — 26 passed, 0 failed, 1 ignored (by design), 6169 filtered out** |

Not yet run this session (per user direction to keep implementing; run before
release): full `cargo test --lib`, `cargo clippy --lib -- -D warnings`,
`cargo fmt --all -- --check` re-check.

## Known issues / constraints

- Working tree contains unrelated in-flight changes (see git status); they
  must be preserved, not reverted.
- Route identifiers must stay cloaked (never spell them out in source,
  tests, or docs).

## Product shell polish (2026-10-26)

- The primary experience remains the terminal workspace. The Windows floating
  companion is described as an optional secondary surface in onboarding and
  command discovery so it does not compete with the main TUI.
- Release linking now uses fat LTO with one codegen unit. This gives LLVM a
  whole-program view of the provider/TUI generics, reducing duplicate code and
  improving hot-path inlining in the shipped binary. The profile keeps symbol
  stripping and unwind-based rendering recovery enabled.
