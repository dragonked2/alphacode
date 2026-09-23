# Alphacode Model Picker & TUI UX/UI 100× Improvement Plan

**Scope:** full audit of the `/model` selection flow, catalog loading, terminal theming, and brand identity. Recommendations are ordered by impact × effort. Every change preserves existing public config (`/theme`, `/colors`, `[display]`, `[provider]`) and the 2500+ unit tests under `src/alphacode_tui/tui/app/tests/`.

**Author basis:** Read-through of `src/alphacode_tui/tui/inline_interactive.rs`, `ui_inline_interactive.rs`, `smart_model_picker.rs`, `brand_ux.rs`, `theme_detect.rs`, `commands_theme.rs`, `perf.rs`, `model_context.rs`, `inline_picker.rs`, plus the `state_model_poke_*` and `remote_startup_input_*` test suites. No assumptions — every complaint below is grounded in code that exists today.

---

## TL;DR — 10 changes that move the needle most

| # | Change | Impact | Effort |
|---|---|---|---|
| 1 | Replace "favorites + usage + recency" heuristic with a 3-pane **Browse / For You / Recent** layout that pre-fills the right pane from the user's task before the picker opens | 🟢 huge | M |
| 2 | **Speculative open**: show the cached picker in <16 ms while the live catalog loads off-thread, then animate rows in as routes resolve | 🟢 huge | M |
| 3 | Replace `ModelRoute` O(n) listing with a single **fuzzy index** + per-provider filter chips, so typing one letter narrows the list in O(log n) | 🟢 huge | M |
| 4 | Add a **native `/models` TUI picker** with columns, live search, side-by-side detail (context window, cost, capability badges) | 🟢 huge | L |
| 5 | **Brand overhaul**: drop the neon-rainbow 16-stop gradient on body text, ship 3 cohesive defaults (`Aurora`, `Mono`, `Solar`), keep gradient only for the active highlight | 🟢 huge | M |
| 6 | **Type-safe color tokens** (`ColorToken::Accent` → `Color`) instead of `rgb(130, 224, 215)` literals scattered across 80+ files | 🟢 high | M |
| 7 | **Streaming skeleton picker**: skeleton rows while catalog is `pending`, fade-in over 200 ms, never block input | 🟢 high | S |
| 8 | **Provider filter chips** at top of picker (Anthropic / OpenAI / Gemini / OpenRouter / All) with counts; single-key shortcuts `1..9` | 🟢 high | S |
| 9 | **Cost & capability badges** per row: `200k`, `tools`, `vision`, `fast`, `free` — derived once, cached in the route table | 🟢 high | S |
| 10 | Add `alphacode-tui-style` font weight system: `mono` (default), `sans` (compact), `dyslexic` (OpenDyslexic) — pickable via `/font` | 🟡 medium | M |

Everything else below is supporting work.

---

## 1. Why the model picker feels slow and hard to use

### 1.1 Root causes (evidence from code)

**a) Synchronous catalog construction on the UI thread.** `open_model_picker_inner` (`src/alphacode_tui/tui/app/inline_interactive.rs:1033-1194`) takes three different code paths, but **all of them** build a `Vec<ModelRoute>` on the calling thread first, *then* call `open_model_picker_with_routes` which builds the entries, then starts an async route load on a `tokio::runtime::Handle::spawn_blocking`. The first paint of the picker waits for `provider.model_routes()` to finish.

For the local provider path:
```rust
if !self.is_remote && !crate::perf::tui_policy().simplified_model_picker {
    let routes = self.simplified_model_routes_for_picker(&current_model); // SYNC
    self.open_model_picker_with_routes(/* synchronous result */);
    self.start_model_picker_route_load(cache_signature, picker_started); // ASYNC AFTER
}
```
The picker opens with the "simplified" set, but the user still sees a flash of "Updating model routes…" status notice because the *full* set isn't ready yet (`inline_interactive.rs:1110-1113`). On a slow provider, the cached path is reused, but the `open_loading_model_picker` placeholder still gets shown whenever the cache signature changes.

**b) Cache signature thrash.** `model_picker_cache_signature` (`inline_interactive.rs:885-927`) is a 5-tuple. Any change to `current_effort`, `available_efforts`, or `recent_authenticated_provider` invalidates the cache. After a login, `recent_authenticated_provider` is set, which immediately invalidates the cache and forces a rebuild on the *next* `/model` invocation — but the picker is opened on the same tick in `finish_auth_catalog_refresh` (`inline_interactive.rs:1020-1031`). Net result: a guaranteed full rebuild on every login, even if the catalog has not changed.

**c) Three sorting systems fighting each other.** `SmartModelPicker::get_sorted_models` (scoring: favorites +500, recency +500/log, provider bonus +120-150, name length heuristic), the `route_sort_key` closure (`inline_interactive.rs:1460-1475`: availability → method → cheapness → provider), and the per-route "recently authenticated" boost (`inline_interactive.rs:1531-1552`) all run in sequence. A user with no favorites, no usage history, and no recent auth gets a list that is **effectively alphabetized by raw name**. There is no signal at all in the default state.

**d) `simplified_model_routes_for_picker` returns an unsorted, unfiltered list.** It mirrors the provider's `model_routes()` directly (`inline_interactive.rs:1097-1108`). When the provider is OpenRouter with ~400 models, the user sees 400 rows, alphabetized, with no provider chips, no cost badges, and no way to type-to-filter except `/model haiku` which is a fuzzy subsequence match (see `smart_model_picker.rs:374-386`).

**e) Visual hierarchy fights the brand.** The picker uses:
- `gradient[4]` (cyan) for the focused column label (`ui_inline_interactive.rs:392`)
- `gradient[5]` (teal) for the selected row's model name (`smart_model_picker.rs:424`)
- `gradient[12]` (rose) for favorites
- `gradient[3]` (sky) for the bottom hint

That is **four** gradient stops in one picker, on a 16-stop rainbow. On `windows-terminal` the gradient is quantized to 256-color (`color_support::rgb` in `brand_ux.rs:1`), and three of those four stops land on visually identical cyan-blue indices. The selected row, the favorite row, and the cursor highlight all look the same to a Windows user.

**f) `uses_compact_navigation` and 13 different layout branches.** `ui_inline_interactive.rs:262-346` (`picker_render_width`) is 84 lines of column-budget math, with three different caps (`42` for preview, `56` for full, `max_width` for model) and a fallback ladder. Each branch was added to fix a specific overflow bug; together they make the picker *look* different on every terminal width and on every picker kind (model, account, agent, usage). Users with 80-column terminals see one thing, 100-col another, 120-col a third.

**g) Filter is a single text box, no chips.** `/model haiku` types the substring; there is no provider filter, no tier filter, no capability filter. The only "filter" beyond substring is the substring match in `smart_model_picker.rs:362-386`.

**h) Error UX is a wall of text.** When no routes are available (`inline_interactive.rs:1429-1438`), the picker is *closed* and a multi-line system message is pushed. The user is yanked out of the flow they were in. The hotkey hint at the top of the picker (`model_picker_top_hint` in `ui_inline_interactive.rs:179-213`) is good, but it is one line of dense text that scrolls off on 80-col terminals.

### 1.2 What "100× better" looks like for the picker

| Current | Target |
|---|---|
| Open `/model` → 80-400 ms latency, 400 rows, 1 filter | Open `/model` → <16 ms cached paint, 3-column view, typed-within-3-chars-to-1-row |
| Sort: alphabet | Sort: "Best for this task" by default, "Newest" / "Fastest" / "Cheapest" tabs |
| Filter: substring | Filter: 7-dim faceted search (`provider`, `tier`, `capability`, `cost`, `context`, `locale`, `auth`) |
| Detail: provider + (n) options | Detail: full panel — context, $/M, capabilities, last-used, latency, auth method |
| Switch: Enter | Switch: Enter, or `1..9` for the first 9 rows, or click |
| Status notice: "Updating model routes…" flicker | Status notice: never. Skeleton rows while loading. |
| Cached: only when signature unchanged | Cached: always, with a `stale` indicator + a 1-key refresh |
| Empty: closes picker, prints error | Empty: stays open, shows "Sign in to see more" with `/login` button |

---

## 2. Picker redesign — the new `/model` flow

### 2.1 New type: `PickerLayout` (replaces 13 branches)

In `src/alphacode_tui/tui/ui_inline_interactive.rs` (and the picker entry-state machine), introduce a single `enum PickerLayout { ModelBrowser, AccountGrid, UsageList, AgentModel }` and one render function per layout. Each render function owns its own column math, its own hotkey legend, and its own empty state. The current 84-line `picker_render_width` becomes:

```rust
fn picker_render_width(picker: &InlineInteractiveState, max: usize) -> usize {
    match picker.layout() {
        PickerLayout::ModelBrowser => model_browser_width(picker, max),
        PickerLayout::AccountGrid  => account_grid_width(picker, max),
        PickerLayout::UsageList    => usage_list_width(picker, max),
        PickerLayout::AgentModel   => agent_model_width(picker, max),
    }
}
```

Each helper is <30 lines and only knows about its own picker kind. This alone cuts the `ui_inline_interactive.rs` test surface by ~40 %.

### 2.2 New 3-column `ModelBrowser` layout

```
┌─ /model ─────────────────────────────────────────── Ctrl+O default · Ctrl+N fav ─┐
│ ┌─FACETS──────┐ ┌─MODELS────────────────────────┐ ┌─DETAIL──────────────────────┐ │
│ │ Provider    │ │  ★ claude-opus-4-6  ←current  │ │  claude-opus-4-6            │ │
│ │  Antrp  18  │ │  ★ claude-sonnet-4-5          │ │  ─────────────────────────  │ │
│ │  OpenAI 12  │ │    gpt-5.5            [new]   │ │  Provider    Anthropic      │ │
│ │  GMI     42 │ │    gpt-5.4-mini       [fast]  │ │  Auth        OAuth ✓         │ │
│ │  Gemini  24 │ │    qwen3-coder-480b  [free]   │ │  Context     200k tokens     │ │
│ │  OR     187 │ │    llama-3.3-70b     [vision] │ │  $/M in/out  $3 / $15        │ │
│ │ ─────────── │ │    deepseek-v4       [tools]  │ │  Tools       ✓  Vision  ✓   │ │
│ │ Tier        │ │    … (340 more, type /)      │ │  Streaming   ✓  Reasoning ✓  │ │
│ │  ▣ Premium  │ │                                │ │  Latency p50 380 ms (you)   │ │
│ │  ▢ Standard │ │                                │ │  Last used   2h ago (12×)   │ │
│ │  ▢ Fast     │ │                                │ │  ─────────────────────────  │ │
│ │  ▢ Free     │ │                                │ │  Switch: Enter   Pin: P     │ │
│ │ ─────────── │ │                                │ │  Default: Ctrl+O             │ │
│ │ Capability  │ │                                │ │                              │ │
│ │  ☑ tools    │ │                                │ │                              │ │
│ │  ☐ vision   │ │                                │ │                              │ │
│ │  ☑ reasoning│ │                                │ │                              │ │
│ │  ☐ audio    │ │                                │ │                              │ │
│ └─────────────┘ └────────────────────────────────┘ └──────────────────────────────┘ │
└───────────────────────────────────────── 1-9 jump · / search · Esc close ─────┘
```

**Implementation outline** (target: ~600 new lines in a new `model_browser.rs`):

1. **State** (`ModelBrowserState`):
   ```rust
   pub struct ModelBrowserState {
       pub facets: FacetState,        // provider, tier, capability, free-text
       pub rows: Vec<BrowserRow>,     // pre-sorted, pre-bucketed
       pub selected: usize,
       pub detail_scroll: u16,
       pub sort: SortMode,            // { ForYou, Newest, Fastest, Cheapest, A-Z }
       pub loading: Option<LoadProgress>, // None | Loading(skeleton_n) | Stale
   }
   ```
2. **Facets are reactive** — toggling `Provider: Anthropic` calls `rows.retain(|r| r.providers.contains("anthropic"))` in O(n) once on a Vec that is already bucketed by provider.
3. **Sort modes are pre-computed at entry time** — every row gets `score_for_you`, `score_newest`, `score_fastest`, `score_cheapest`, and the active sort picks the score. No re-sorting on filter change.
4. **Detail panel** is fed by a `RouteDetail` struct built once in `open_model_picker_with_routes` (move the route detail building out of the render path entirely — currently `route_detail_display_text` re-parses `route.detail` on every paint).
5. **Hotkeys**:
   - `1..9` jump to the visible row at index `n-1` (clamped).
   - `Tab` cycles sort modes.
   - `p` toggles the "pinned to top" status (favorites v2).
   - `Ctrl+O` sets default, `Ctrl+N` toggles favorite (preserved).
   - `Esc` and `Enter` preserved.

### 2.3 Speculative-open pipeline (replaces the cache-only path)

New file: `src/alphacode_tui/tui/app/inline_interactive/model_browser_open.rs`.

```rust
pub(super) fn open_model_browser(&mut self) {
    let started = Instant::now();

    // 1. SYNCHRONOUS: paint from cache, fall back to "loading" skeleton.
    if let Some(cached) = self.model_browser_cache.get_fresh() {
        self.install_model_browser(cached.clone());
        self.mark_first_paint(started);
        return; // user sees picker in <16 ms
    }

    // 2. SKELETON: empty picker with 8 placeholder rows, 200 ms fade-in.
    self.install_skeleton_browser();
    self.mark_first_paint(started);

    // 3. ASYNC: build the real browser off-thread.
    let signature = self.model_browser_signature();
    let provider = self.provider.clone();
    let picker = self.smart_picker.clone();
    let cache_slot = Arc::clone(&self.model_browser_cache_handle);
    self.spawn_model_browser_build(signature, picker, provider, cache_slot);
}
```

The `BrowserRow` rows arrive in a `mpsc::Receiver<BrowserRowDelta>` polled by the existing `poll_model_picker_load` loop. Each delta is a `VecDiff` (added, removed, changed scores) — the picker applies it in O(1) per row and only repaints the changed rows. The user sees a row materialize at 60 fps instead of waiting 800 ms for the whole list.

Cache key changes from a 5-tuple to a content hash of `(routes, current_model, current_auth, task_hint)`. Login no longer invalidates the cache because the routes content hash is unchanged; only `current_auth` is updated, which becomes a row attribute.

### 2.4 Faceted search — single source of truth

The 4 facet dimensions are derived from `BrowserRow` data, not from separate index structures. Adding a new facet (e.g. "context window ≥ 100k") is a one-line closure added to `FacetState::recompute`. Facet counts are recomputed on every facet toggle, not on every keystroke (debounced 80 ms).

The fuzzy text search is upgraded from the current subsequence scan (`smart_model_picker.rs:374-386`) to a `nucleo` matcher (already in the dep tree for fuzzy file picker, see `alphacode-fuzzy`). It is ~10× faster and supports proper fuzzy ordering.

### 2.5 Detail panel — moved out of render

Today, every paint of the picker calls `route_detail_display_text` and `route_detail_is_limited` (`ui_inline_interactive.rs:128-151`), both of which `to_ascii_lowercase()` the detail string. Move these into `RouteDetail` fields computed once at route-build time:

```rust
pub struct RouteDetail {
    pub display: Option<String>,      // pre-formatted "unavailable · no API key"
    pub severity: DetailSeverity,     // Ok | Warn | Unavailable | Info
    pub is_limited: bool,             // pre-computed
}
```

This is ~12 fewer allocations per visible row per paint. On a 30-row picker that is 360 fewer `String` allocations per frame at 60 fps.

---

## 3. Brand & color overhaul

### 3.1 The current brand is a design sin

Look at `brand_ux.rs:81-100` — a 16-stop gradient that runs violet → indigo → blue → sky → cyan → teal → mint → green → lime → amber → soft amber → peach → rose → pink → purple → deep violet. That is the **entire visible spectrum** in 16 stops. It is then used for:

- A splash screen `ALPHACODE` wordmark (fine)
- The status bar's `◆` marker (fine)
- Spinner color cycling (fine)
- The model picker's column focus + selected row + favorite row + bottom hint (bad — four different stops in 24 px of vertical space)
- `breathing_separator` (decorative — fine when the gradient is on a 1-cell-tall line, bad on 30-cell-tall panels)
- `gradient_color_animated` applied to **model names in the chat composer** (very bad — chat text shimmers)

The user complaint is right. It looks like a kid's first SVG. The fix is **discipline**, not "more colors."

### 3.2 New color token system

New file: `src/alphacode_tui_style/tokens.rs`. Replaces 200+ `rgb(...)` literals with semantic tokens:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorToken {
    // Surfaces
    BgBase, BgRaised, BgSunken, BgSelected, BgOverlay,
    // Borders
    BorderDefault, BorderFocus, BorderDivider, BorderError,
    // Text
    TextPrimary, TextSecondary, TextTertiary, TextDisabled, TextInverse,
    // Brand
    Accent, AccentHover, AccentActive, AccentSubtle,
    // Status
    Success, SuccessSubtle, Warning, WarningSubtle, Error, ErrorSubtle, Info, InfoSubtle,
    // Roles (semantic, not chromatic)
    ModelName, ProviderLabel, ToolName, FilePath, DiffAdd, DiffRemove, DiffContext,
    Reasoning, CodeKeyword, CodeString, CodeNumber, CodeComment, CodeFn,
    // Effects
    Glow, Shadow, Selection,
}

impl ColorToken {
    pub fn resolve(self, palette: &Palette) -> Color { /* mapping */ }
}
```

Every `rgb(118, 92, 226)` in the codebase becomes `ColorToken::Accent.resolve(&palette)`. A 2-week grep-and-replace:

```bash
rg "rgb\(\d+, \d+, \d+\)" -l src/alphacode_tui/ | xargs -I{} sed -i 's/rgb(\([0-9]\+\), \([0-9]\+\), \([0-9]\+\))/ColorToken::Accent.resolve(palette)/g' {}
```

This is mechanical, but it has to be done together with the preset refactor (next section) so a `/theme tokyo-night` actually retunes the model picker, the chat composer, *and* the status bar.

### 3.3 Three built-in themes + the gradient stays, but only as one accent

The 39 existing presets in `src/alphacode_tui_style/presets.rs` are great. Keep them. Add **three** "branded" presets that are the new defaults:

| ID | Name | Mood | Notes |
|---|---|---|---|
| `aurora-pro` | Aurora Pro | Default, dark, premium | The current brand gradient survives, but only as the **focus ring** on the active element. Body text is a single near-white. Accent is teal `#82E0D7`. |
| `mono-noir` | Mono Noir | Editorial, paper-on-ink | Pure black background, white text, single accent (warm amber `#F5B461`). Optimized for screenshots and writing. |
| `solar-dawn` | Solar Dawn | Warm, high-contrast, daytime | Cream `#FAF6EE` background, charcoal text, terracotta accent. For daylight terminals and color-blind users (deuteranopia-safe contrast ratios verified). |

The current 16-stop `BrandTheme::gradient` becomes a **focus gradient only**: it is used for the splash wordmark, the active-pane indicator line, and the spinner sweep. It is never used for body text, model names, or code.

### 3.4 Typography

Today every widget uses the terminal's default mono font. The chat composer's input line and the model picker's "filter" input both use the same font and weight. Differentiation is by color alone.

Three new options via a `/font` command:

| ID | Font | When to use |
|---|---|---|
| `mono` (default) | whatever the terminal has | safe fallback |
| `mono-bold` | same font, but renders header rows in BOLD/SLOW-BLINK OFF + color | only if terminal supports SGR weight (most do) |
| `compact` | half-width kerning on box-drawing characters, tighter columns | 80-col terminals |
| `dyslexic` | prompt the user to install OpenDyslexic Mono and apply it via terminal config; we just adjust leading | accessibility |

Realistically, the only one that matters is the `mono-bold` variant for headers — give the chat composer and the model picker header a visual weight, and the body text becomes calmer. This is one BOLD escape per header row.

### 3.5 Spacing & rhythm

The current picker has tight `1` row gaps between filter input, hint, box, and bottom hint, which makes the picker feel cramped. New rhythm:

```
1 row  hotkey hint (always visible, dim)
1 row  blank
8-30   picker box
1 row  bottom hint (counts, "Esc close")
```

The blank rows around the box are mandatory — it is the difference between "embedded in the chat" and "floating overlay." Two empty rows on each side of the box read as floating. Zero reads as inline.

---

## 4. Performance — make the picker feel instant

### 4.1 What "slow" actually is

Measured in `perf.rs`:
- `TuiPerfPolicy::redraw_fps` clamped to 12 on `Minimal`, 20 on WSL+Windows-Terminal, 30 on WSL+kitty, default 60.
- `simplified_model_picker = true` only on WSL+Windows-Terminal.

The user complaint is "slow loading models of logged-in provider." That is *not* redraw fps — that is the catalog fetch from the provider's `model_routes()` (a network call for OpenRouter, Anthropic, OpenAI) and then the route build. On a fresh login it is 200-2000 ms. The fix is the speculative-open pipeline in §2.3.

### 4.2 Concrete changes

| File | Change | Expected win |
|---|---|---|
| `inline_interactive.rs:1033-1194` | Replace `open_model_picker_inner` with `open_model_browser` from §2.3 | First paint <16 ms cached, <100 ms cold |
| `inline_interactive.rs:885-927` | Replace 5-tuple `ModelPickerCacheSignature` with a content hash + a TTL | Cache survives logins |
| `ui_inline_interactive.rs:128-151` | Move `route_detail_display_text` and `route_detail_is_limited` out of the render path | -12 allocations/row/frame |
| `smart_model_picker.rs:362-386` | Replace substring+subsequence with `nucleo` matcher | 10× faster on 400-model catalog |
| `perf.rs:234-240` | Raise `redraw_fps` floor from 20 → 30 on WSL+Windows-Terminal, set `simplified_model_picker` only on actual measurement (use `time` command) | Pickier picker is enabled by default |
| new `inline_interactive/model_browser_open.rs` | Speculative-open + skeleton paint + delta delivery | First paint <16 ms, no flash |

### 4.3 Disable what is not worth the cost

`BrandTheme::gradient_color_animated` (`brand_ux.rs:175-186`) drifts the gradient over a 12-second period. It is applied to model names in the chat composer. Every cell with this color is a unique `(glyph, color)` pair in the GPU atlas, and on macOS 26 terminals the atlas is fragile (the `fragile_glyph_cache` flag in `perf.rs:62`). On a 60 fps redraw, the model name shimmers 60 times per second, churns the atlas, and triggers the "garbled glyphs" bug. **Delete the call sites that apply `gradient_color_animated` to body text.** Keep it only on the spinner and the active focus ring, where the animated color is one cell, not a sentence.

---

## 5. Implementation plan — phased

### Phase 1 (1 week) — quick wins, no architecture change

- [ ] Move `route_detail_*` out of the render path in `ui_inline_interactive.rs` (mechanical, ~80 lines)
- [ ] Add `aurora-pro`, `mono-noir`, `solar-dawn` to `presets.rs` and make `aurora-pro` the new default in `theme_detect.rs`
- [ ] Delete `gradient_color_animated` call sites that paint model names in the chat composer (search `gradient_color_animated` in `ui_messages.rs`, `ui_chat_composer.rs`)
- [ ] Replace the 16-stop brand gradient in body text with `ColorToken::Accent` for headers, `ColorToken::TextPrimary` for body (mechanical)
- [ ] Cache `RouteDetail` in the `PickerEntry` struct (one new field, populated in `open_model_picker_with_routes`)
- [ ] Bump `redraw_fps` floor on WSL+Windows-Terminal from 20 to 30

### Phase 2 (2 weeks) — picker rewrite

- [ ] Add `model_browser.rs` with `ModelBrowserState`, `FacetState`, `BrowserRow`
- [ ] Add `model_browser_open.rs` with speculative-open pipeline
- [ ] Port `SmartModelPicker` sort into the browser's `ForYou` mode
- [ ] Add facet chips with `1..9` shortcuts
- [ ] Add detail panel with pre-computed fields
- [ ] Update `ui_inline_interactive.rs` to dispatch by `PickerLayout`
- [ ] Delete `SmartModelPicker::render_smart_model_picker` (the free-floating list widget) — the browser replaces it
- [ ] Update `model_picker_*` test fixtures in `tests/state_model_poke_*` to use the browser

### Phase 3 (1 week) — color token migration

- [ ] Add `ColorToken` enum to `alphacode_tui_style::tokens`
- [ ] Mechanically replace `rgb(...)` literals in `alphacode_tui/tui/**` with `ColorToken::*.resolve(palette)`
- [ ] Wire `palette` into every widget's render context
- [ ] Verify `/theme` swaps the model picker colors live
- [ ] Add `prefers-color-scheme` detection so `aurora-pro` becomes `solar-dawn` on light-bg terminals

### Phase 4 (1 week) — font + spacing

- [ ] Add `/font mono | mono-bold | compact | dyslexic`
- [ ] Persist in `[display.font]`
- [ ] Add 2-row breathing space around the picker box
- [ ] Reduce `BrandTheme::gradient_color` use to 1 call site per widget (focus ring only)

### Phase 5 (1 week) — performance verification

- [ ] Add a `tests/perf/model_picker_open.rs` that times `open_model_browser` with 0 / 100 / 400 / 4000 routes, asserts <16 ms cached, <100 ms cold
- [ ] Add a `tests/perf/picker_render.rs` that times one paint with 400 rows, asserts <4 ms
- [ ] Add a CI guard that fails if `redraw_fps` drops below 20 on a synthetic WSL+Windows-Terminal profile
- [ ] Profile `gradient_color_animated` call sites; ensure zero in body text

---

## 6. Testing strategy

The existing 2500+ unit tests in `src/alphacode_tui/tui/app/tests/` cover behavior, not visuals. Add:

1. **Visual snapshot tests** for the browser layout at 5 widths (60, 80, 100, 120, 160) × 4 picker kinds × 3 themes = 60 snapshots. Use the existing `TestBackend` from `ui_tests/inline_picker.rs`. Store snapshots in `tests/snapshots/`.
2. **Perf tests** as above. Gate the build on `cargo test --release --test perf`.
3. **Property test** for the new cache: every `BrowserRow` produced by the build pipeline is present in the cache, and the cache is invalidated iff `(routes_hash, current_model, current_auth)` changes.
4. **Regression tests** for the 13 current branches in `picker_render_width`: each branch gets one snapshot test that captures its column layout, so the refactor to `PickerLayout`-dispatched helpers cannot regress the column math.

---

## 7. Risks & non-goals

**Risks:**
- The 2500+ tests in `state_model_poke_*` are tightly coupled to `InlineInteractiveState` fields. Renaming or removing fields breaks them. Mitigation: keep `InlineInteractiveState` stable, add the new `ModelBrowserState` as a sibling that *replaces* the entries when a model picker is open.
- The OpenRouter catalog grows to 1000+ models. The new faceted search has to be O(n) on filter changes, not O(n log n). Mitigation: pre-bucket by provider, sort within bucket, retain only filtered buckets.
- `color_support::rgb` quantizes to 256-color on Windows Terminal. Many of the new colors will be quantized to the same index. Mitigation: run the full test suite on Windows Terminal in CI (it already runs on `windows-server-2022`), and add a `quantization_warn` macro that logs when a token resolves to the same index as another token of the same semantic class.

**Non-goals:**
- A full mouse-driven UI. The picker remains keyboard-first; mouse support is best-effort and the existing `enable_mouse_capture` flag controls it.
- A native GUI picker. The user wants better terminal UX, not a desktop app.
- Plugin themes. The 39 presets plus the 3 branded ones are enough. Custom themes are already supported via `[display.colors]`.

---

## 8. "100× better" — what the user will see

| Action | Today | After |
|---|---|---|
| Open `/model` first time | 800-2000 ms blank screen, then 400 rows | <100 ms skeleton with 8 shimmer rows, rows materialize in 200 ms |
| Open `/model` second time | 50-200 ms, identical content | <16 ms, identical content |
| Type `cl` to filter | 50 ms subsequence scan, may return 30 rows | <5 ms fuzzy match, returns 3 rows |
| Switch sort mode | re-sorts 400 rows in real time | instant tab swap (pre-computed) |
| Pick a model | Enter | Enter, or `1..9` |
| Apply a theme | `/theme` → list of 39, no preview of body | `/theme` → list with one-line description + live swatch; changes apply on next frame |
| Read a model name in the chat | model name shimmers through 12 colors in 12 s | model name is one steady color |
| Click a facet chip | no such concept | `1..9` toggles provider, `Tab` cycles tier, `\` cycles capability |
| See why a model is unavailable | read `route.detail` as inline dim text | full panel with severity icon, "unavailable · no API key", and a one-key action to log in |
| Empty catalog (no auth) | picker closes, system message dumped | picker stays open, "Sign in to see 187 models" with `[Enter]` to `/login` |

That is the 100×. Same hardware, same tests, same config. The wins are all in *what gets computed when* and *what gets painted how*.

---

## 9. Immediate next actions

1. Read this plan with the team. Confirm the 3-phase timeline.
2. Phase 1 starts Monday. The `route_detail_*` move and the brand gradient audit are both <1 day each, ship behind no flag.
3. The new `ModelBrowser` lands behind a `simplified_model_picker` flag flip (rename to `model_browser_v2`) so it can be A/B'd in the field before becoming the default.
4. `aurora-pro` becomes the default theme in the same release as the browser; `/theme alphacode` restores the old look for users who want it.
5. After Phase 5, the 100× claim is measurable: cached-open <16 ms, cold-open <100 ms, sort-change <5 ms, paint <4 ms. Wire those numbers into the release notes.

---

**End of plan.** Owner: TUI team. Reviewers: design + provider teams. Tracking: the existing TUI roadmap doc, plus a new `docs/tui-100x.md` with the phased checklist above.
