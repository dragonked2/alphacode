# Alphacode 10× Improvement Plan (Full Audit)

**Scope:** performance, speed, accuracy, UI/UX, TUI, output, input, animations, content, agents, harness, logic, tools.

**Basis:** read-through of the agent turn loop (`turn_loops.rs`, `turn_execution.rs`, `turn_streaming_mpsc.rs`, `agent.rs`), tool registry (`tool/mod.rs`), TUI run loop (`app/run_shell.rs`, `redraw_schedule.rs`, `ui_smoothness.rs`, `stream_buffer.rs`, `ui_animations.rs`, `improved_input.rs`), config defaults (`alphacode_config_types/display.rs`, `alphacode_base/config/default_file.rs`), prompt content (`system_prompt.md`, `swarm_prompt.md`), swarm core, and the existing `docs/tui-model-picker-100x-review.md`.

**Honest framing first:** Alphacode already ships unusually good engineering in several of these areas — partial-repaint fast paths, a semantic stream buffer, perf tiers, smart output truncation, prefix-hash cache tracking, smoothness recording, health monitoring. Several "obvious 10×" wins are already taken. The plan below targets what is *actually* still slow, inaccurate, or friction-y, verified against code that exists today. Each item lists evidence, expected gain, and effort. Nothing requires new dependencies.

---

## TL;DR — the 12 changes with the best impact × effort

| # | Change | Area | Impact | Effort |
|---|---|---|---|---|
| 1 | Parallel execution of independent tool calls in a single assistant message | Speed | 🟢 huge | M |
| 2 | Fix contradictory animation-FPS config (default 20 vs docs "default: 60") | Config truth | 🟢 high | S |
| 3 | One shared `truncate_chars` helper; kill 11 divergent `.chars().take(n).collect()` sites | Correctness | 🟢 high | S |
| 4 | Log-rate limiting: demote per-iteration INFO logs to TRACE in hot loops | Speed | 🟡 medium | S |
| 5 | `Session::save()` coalescing in the tool loop (save once per iteration, not per tool) | Speed / disk | 🟢 high | S |
| 6 | Add `Esc Esc` / `Ctrl+C` "soft cancel" that keeps partial assistant text | Input / UX | 🟢 high | M |
| 7 | Slash-command palette (`/` opens ranked menu with descriptions) | Input / UX | 🟢 huge | M |
| 8 | Surface tool-output truncation visibly (`… truncated: show with /tool <id>`) | Output / trust | 🟡 medium | M |
| 9 | Opt-in safe parallel `read`/`ls`/`glob` in `batch` by default with per-tool concurrency caps | Speed | 🟡 medium | S |
| 10 | Error-message linter: every `anyhow!` shown to the model gets a "next step" suffix | Accuracy | 🟢 high | M |
| 11 | Animation governance: single `AnimationBudget` guard so decorative + status + spinner never exceed N cells/frame | Animations | 🟡 medium | M |
| 12 | Help content overhaul: `/help` grouped by intent with keybinding column, generated from a single source of truth | Content | 🟡 medium | M |

---

# Part A — Performance & speed

## A1. Parallelize independent tool calls in one assistant message

**Evidence.** `Agent::run_turn` (`src/alphacode_app_core/agent/turn_loops.rs:966+`) executes `tool_calls` strictly sequentially:

```rust
for tc in tool_calls {
    ...
    let result = self.registry.execute(&tc.name, tc.input.clone(), ctx).await;
    ...
}
```

The same sequential loop exists in the streaming path (`turn_streaming_mpsc.rs`). When a model emits 3–5 parallel reads/greps (modern providers do this routinely), each 200–500 ms tool call stacks into 1–2.5 s of wall time that should be ~max(single call).

**Design.** Not all tools are safe to run concurrently (bash mutates state; edits depend on ordering). Introduce a per-tool concurrency class in `ToolRegistry`:

- `ReadOnly` (read, ls, glob/grep/agentgrep, session_search, conversation_search, webfetch, websearch) → run concurrently, capped at `min(4, calls.len())`.
- `Mutating` (write, edit, multiedit, apply_patch, patch, bash, browser, computer) → keep sequential, preserve exact current ordering.
- `Background` (bg, cron, schedule) → fire-and-forget, already non-blocking.

Implementation sketch: partition the `tool_calls` vec into consecutive read-only runs; for each maximal run of `ReadOnly` tools, `futures::future::join_all` with a `Semaphore` permit per call; emit `ToolUpdated` events per call as each finishes (the bus is already per-call, so the TUI needs no changes). Results are appended to history **in the original call order** regardless of completion order — this is critical because providers correlate tool results by `tool_use_id`, but some (Anthropic) are stricter about result ordering matching call ordering.

Add a config kill-switch: `[agent] parallel_readonly_tools = true` (default on), and a `debug-parallel-tools` metric comparing sequential vs parallel elapsed time per run.

**Gain.** 2–4× wall-clock reduction on the read-heavy phase of every turn — which is most of a coding session. This is the single biggest *speed* win available.

**Risks.** Tools sharing global mutable state (e.g. read.rs's file cache) need a quick audit for `RwLock` vs `Mutex`; the `ToolContext` is already cloned per call. The `health::SLOW_OP_THRESHOLD` reporting stays valid because each tool is individually timed.

## A2. Coalesce `Session::save()` calls inside the tool loop

**Evidence.** Today the loop saves per tool result via `add_message_with_duration` → `session.save()` internally (see `turn_execution.rs:add_manual_tool_result` pattern) *and* again at the end of the batch (`if tool_results_dirty { self.session.save()?; }`). Session persistence measures itself and already logs slow saves (`session/persistence.rs:453` logs total ms with delta breakdowns — the instrumentation exists because this is a known cost).

**Design.** Make `add_message*` in the tool path mark dirty instead of saving; keep the existing end-of-batch save plus the periodic auto-save (every 5 iterations / 60 s, already in `turn_loops.rs`). Worst-case loss window is already bounded by `AUTO_SAVE_INTERVAL`; this doesn't change it.

**Gain.** Removes N-1 fsync-class writes per multi-tool turn; directly reduces the "slow save" tail the persistence layer already tracks.

## A3. Log-rate limiting in hot loops

**Evidence.** Per-iteration `logging::info` calls in `messages_for_provider` (`agent.rs:692`, `agent.rs:725`) fire on **every** turn-loop iteration, formatting and counting messages every time. `Tool starting:` / `Tool finished:` pairs, `API call starting:`, and `Bus::publish(SubagentStatus)` per tool call all run even when nothing consumes them.

**Design.** Two rules:
1. Message-count summaries move to `trace_enabled()` gates (they're already computed cheaply; the format! is the cost).
2. Add a `logging::info_throttled(key, secs)` helper (a small `Mutex<HashMap<&'static str, Instant>>`) used for per-tool start/finish pairs at ≥1 s granularity; the *finish* line keeps full precision since it carries the timing.

**Gain.** Removes hundreds of allocs/sec during streaming turns; measurable on low-end hardware and SSH where the tier is Reduced/Minimal.

## A4. Config truth: animation FPS contradictions

**Evidence.** Three sources disagree:
- `alphacode_config_types/display.rs:170` — `animation_fps: 20` (the real default).
- `display.rs:73` doc comment — "(default: 60)".
- `alphacode_base/config/default_file.rs:243` — "Animation FPS (idle animation): 1-120 (default: 60)" and a commented `animation_fps = 60`, while 30 lines above it claims "The shipped default is 20 FPS".
- `redraw_schedule.rs` caps decorative animation at 30 fps with measured data; the calm-defaults regression test (`default_file.rs:717`) asserts ≤30.

Users reading the generated config file are told the wrong default, and if they uncomment `animation_fps = 60` they get *worse* smoothness than the cap silently allows (cap 30) — a config value that lies.

**Design.** Fix the doc comments to 20; when a user sets `animation_fps > 30`, log one line: "animation_fps capped to 30 for decorative animations (measured CPU data in redraw_schedule.rs); functional animations still use your value." Make the cap visible in `/settings` output via `display_summary.rs`.

**Gain.** Correctness of the single most user-visible perf knob.

## A5. `batch` tool: safe defaults for read-only fan-out

**Evidence.** `tool/batch.rs` exists but the model has no nudge to use it for parallel reads; the system prompt doesn't mention batching read-only calls. Combined with A1, this multiplies.

**Design.** In `prompt/system_prompt.md` tool guidance, add one sentence: "When you need ≥3 independent reads (files, greps, listings), issue them as parallel tool calls or one `batch` call — they execute concurrently." Keep the current `SINGLE_OUTPUT_MAX_FRACTION` guard.

**Gain.** Teaches the model to structure calls the harness can execute fast. Cost: nothing.

## A6. First-response latency: pre-warm the static prompt

**Evidence.** `turn_loops.rs` caches the static prompt per turn (`cached_static_prompt`), but the *first* turn of a session still builds the full 11 KB prompt (the comment in the code says trivial greetings get ~1.5 KB via tiering — good) and the tool list (~40 tools × JSON schema) on the UI-blocking path before the first API call. `startup_profile.rs` exists but doesn't cover this.

**Design.** After `Agent` construction, `tokio::spawn` a pre-warm that calls `tool_definitions()` and `build_system_prompt_split(None, None)` once and stashes the result in an `Arc<OnceLock>` the turn loop consults first. Invalidate on `is_canary` changes or selfdev registration (the two events that mutate the tool set).

**Gain.** Removes 50–150 ms from the first token of every session; the "first message feels slow" README troubleshooting entry shrinks.

---

# Part B — Accuracy & agent logic

## B1. Every model-facing error gets a next step

**Evidence.** Tool errors are surfaced as `format!("Error: {}", e)` (`turn_loops.rs:~1310`) with no recovery guidance. The tool registry's unknown-tool error is the counter-example — it lists available tools and "Did you mean" suggestions (`tool/mod.rs:595-603`), and the code comments that this stopped hallucination spirals (#104). Most other errors didn't get the same treatment.

**Design.** Add a small `agent_facing_error(tool_name, error) -> String` in `tool/mod.rs` that appends tool-specific hints:

- `edit`: "old_string not found — re-read the file section first; match whitespace exactly."
- `bash`: "command failed with exit N — check the command's own stderr above; do not retry identical input."
- `read`: "file is binary/oversized — use offset/limit ranges."
- Unknown-tool case: unchanged (already good).

Keep hints ≤2 lines. Measure via `telemetry::waste_metrics` whether repeated-identical-failure counts drop.

**Gain.** Directly targets "smarter": models recover in one retry instead of two or three, which is also a latency win (fewer round-trips).

## B2. Recovery paths are good — extend them, don't rebuild

**Evidence.** `response_recovery.rs` already recovers text-wrapped tool calls, truncates malformed ones, and the turn loop has bounded retries for context limits (5), incomplete continuations (3), empty-post-tool (5), and provider errors. `repair_missing_tool_outputs` self-heals transcript gaps. This is solid; the gap is that none of these counters are *visible*.

**Design.** Surface `provider_error_continuations` / recovery counters in the TUI status detail line and `/stats` (`usage_display.rs`): "2 auto-recovered provider errors this session". Cheap, and it converts invisible resilience into user trust.

## B3. Todo-gate noise budget

**Evidence.** `alphacode_base/todo.rs` continuation messages are long, repeated on every failing gate, and the digest prefix (`TODO_GATE_DIGEST_PREFIX`) plus completion gate (max 5 attempts) plus spike detection can chain into multiple consecutive automated messages. On small models this produces observable loops (the code itself caps attempts, which is the tell).

**Design.** Track per-gate repeat count; after 2 identical gate messages in a row, inject a *short* variant ("completion confidence still low: name the concrete evidence per todo, 1 line each") instead of the full paragraph. Keep the first message full-length.

**Gain.** Fewer wasted round-trips on the accuracy gates; big models unaffected, small models stop burning tokens on re-reading their own nudge.

## B4. Swarm worker prompts: parallel-first

**Evidence.** `swarm_core/mod.rs:383-413` injects a deep-node marker telling workers they *may* decompose, but the worker prompt (`prompt/swarm_prompt.md`) doesn't require declaring read-only vs mutating steps, so workers serialize their own tool calls even when independent.

**Design.** One added line to the worker prompt: "Issue independent reads as parallel tool calls." (Same sentence as A5; shared constant.)

**Gain.** Multiplies A1's win across swarm workers.

---

# Part C — TUI, input, output, animations

## C1. Soft-cancel that preserves partial output

**Evidence.** The README documents `Ctrl+C` as "pause the current response (session is kept)". In the code, interrupts (`agent/interrupts.rs`) handle cancel robustly, but partial assistant text handling on double-interrupt still discards the streamed prefix in the common path; users retyping "continue" lose the model's in-flight paragraph.

**Design.** On first interrupt during streaming: keep streaming into a "cancelled" message marked partial, show "Esc to keep / Ctrl+C again to discard". Second interrupt within 2 s discards as today. The stream buffer already segments reasoning vs text, so salvaging text-only is a small change in `stream_buffer.rs` consumption.

**Gain.** The single most-requested cancel UX in terminal agents; removes retyping and re-generation cost.

## C2. Slash-command palette

**Evidence.** There are 100+ slash commands (`commands.rs`, `commands_improve.rs`, debug commands). Discovery today = `/help` (a long list) or knowing the name. Fuzzy matching exists (`alphacode_fuzzy`, with typo tolerance for `/conifg` → `/config`) but only *after* the user types a full candidate. The keybinding-hints system (`shortcut_hints.rs`) already teaches shortcuts after the fact — it proves the appetite for teachable UI.

**Design.** Typing `/` with an empty query opens a ranked overlay (rank: recently used > alphabet), each row showing name + one-line description + shortcut if any. Reuse the existing session-picker overlay rendering (arrow keys, fuzzy filter, Enter). The overlay infra (`session_picker_overlay`, `account_picker`) is general enough; `fuzzy_match_positions` already returns highlight indices.

**Gain.** Turns 100+ commands from hidden knowledge into discoverable UI. This is the biggest *UX* win on the list.

## C3. Visible output truncation

**Evidence.** Two silent truncation layers: `cap_tool_output_for_history` (1 MB, smart head/tail — good) and `guard_context_overflow` (30% budget fraction). Neither leaves a user-visible marker; when a `read` of a huge file comes back short, users can't tell truncation from a short file.

**Design.** When either guard fires, append a `ToolOutput` title/annotation: `⌄ truncated 812k chars — Esc to expand`. Render the annotation in `ui_tools.rs`; expand-on-demand re-reads from a bounded spill file (the session journal already persists full outputs until the next checkpoint).

**Gain.** Restores trust in tool output; helps users diagnose "the model didn't see my whole file."

## C4. Input: paste + history polish (already strong — two gaps left)

**Evidence.** Paste handling is genuinely excellent: `paste_guard.rs` scales the Enter-swallow window up to 8× for slow SSH, `paste_buffer.rs` handles burst joins, bracketed-paste detection covers `array[0]` false positives. Ctrl+R fuzzy history search exists cross-session. Remaining gaps:

1. **No `Alt+.` last-arg insert.** Shell muscle memory; trivially served from prompt history.
2. **No inline ghost completion.** The onboarding suggestion cache (`state_ui_input_helpers.rs:1663`, TTL'd) proves the pattern; extend it: after 3+ chars, if the exact string is a unique prefix of a history entry, render the remainder dim after the cursor; `Tab`/`→` accepts. Zero network cost, history is already loaded for Ctrl+R.

**Gain.** Measurable typing-throughput improvement for repetitive prompts (the "run tests again" case).

## C5. Animation governance

**Evidence.** Good budget already exists per-animation: decorative cap 30 fps with measured CPU data, spinner-only single-cell fast path, partial-repaint tracking with `draw-stats` observability, fragile-glyph-cache policy for macOS 26. The gap: the *sum* of concurrent animation surfaces (status spinner + swarm strip spinner + session-picker spinner + streaming reveal + notification line) has no shared cap; each is individually justified, jointly they can each hold full-frame redraw reasons simultaneously (`FULL_FRAME_REDRAW_REASONS` shows `swarm_spinner`, `session_picker_spinner`, `status_animation` as separate full-frame reasons).

**Design.** One `AnimationBudget` atomic: full-frame animation reasons register/deregister; when >2 concurrent, the lowest-priority ones (decorative first, then picker spinners) demote to the partial-repaint path for a 500 ms window. Wire the existing `note_idle_animation_fast_path_blocked(reason)` tally to report demotions.

**Gain.** Bounds worst-case redraw churn on busy screens (swarm + streaming is the heavy case); the draw-stats infra makes it verifiable.

## C6. Markdown rendering cache: extend to streaming text

**Evidence.** Caches exist for side-panel markdown, mermaid, pinned tables. The *transcript* markdown for the currently-streaming message re-renders the visible suffix every frame (inherent to streaming), but completed messages above it re-render too unless the viewport caching in `ui_messages_cache.rs` already covers them — verify; if the completed-message path still re-highlights syntax per frame, route it through the same cache keyed by (message_id, width).

**Gain.** On long transcripts this is the difference between 60 fps scroll and 20 fps scroll. Measure with the existing `smoothness` report before/after.

---

# Part D — Content & user friendliness

## D1. `/help` grouped by intent

Today's `/help` is a flat wall. Regroup: **Core** (model, sessions, todos, diff), **Workflow** (swarm, plan, memory, skills), **Diagnostics** (doctor, stats, debug), **Settings**. Single source of truth: a `HelpEntry { name, args, summary, group, shortcut }` table that `/help`, the C2 palette, and `keymap_overview.rs` all render from — three surfaces, one table, no drift.

## D2. Error messages for humans

The README troubleshooting section is good; mirror it in-app. The 6 most common failure states (no provider configured, 401/403, rate-limit, no network, binary file read, WSL terminal quirks) should render a short human line + the exact fix command (e.g. "Run `alphacode login` or set `ALPHACODE_OPENAI_API_KEY`"). `setup_hints.rs` already has this pattern for terminals — extend to provider errors.

## D3. Onboarding: show, don't tell

The onboarding flow exists (`ui_onboarding.rs`, flow control, simulator). Add one thing: after the first successful task completes, show a one-time card: "What just happened: 1 prompt → N tool calls (list them) → verified. Press Ctrl+Y anytime to watch agents live." Turning the first success into a mental model is the highest-leverage content change for new users.

---

# Recommended execution order

1. **A1 parallel tools** (biggest speed win; M effort, isolated in the turn loop)
2. **C2 slash palette** (biggest UX win; M effort, reuses overlay infra)
3. **A2 + A3 + A4** (small, independent, immediate)
4. **C1 soft-cancel** (high user-visible value; M)
5. **B1 error next-steps** (accuracy; M, localized)
6. **C3 truncation visibility** (trust; M)
7. **C5 animation budget** (needs draw-stats verification loop)
8. **D1–D3 content** (S each, do alongside)
9. **B3, C4, C6, A5, A6, B2** (polish round)

Each item is independently shippable and testable; none blocks another. The existing regression-gate culture (calm-defaults test, draw-stats, smoothness report, waste metrics) gives every change a before/after measurement — which is how "10×" gets proven instead of claimed.
