# Alphacode improvement roadmap

Build toward an agent that finishes complex work with verifiable results, runs for days through interruptions, and stays responsive on Windows Terminal. Give small local and free models the best chance of succeeding by moving repeatable decisions into the harness and showing the model only the context it needs. Benchmark frontier-model and human performance on the same tasks so any parity or advantage claim has a defined scope.

This roadmap builds on the [existing product plan](100x-product-improvement-plan.md), [prompt budget](prompt-budget-architecture.md), [swarm audit](engineering/swarm-tools-audit-2026-11-01.md), and [TUI performance audit](engineering/tui-performance-follow-up-2026-11-01.md). Those documents contain prior measurements. Establish a fresh baseline before using the targets below to judge improvements.

## 0. Baseline and regression harness

- Pin a reference revision, browser extension and bridge versions, model IDs and settings, and a Windows reference machine. Run identical workloads through the interactive TUI and headless entry points. Keep deterministic provider, browser, and filesystem fixtures for CI, with separate live-model and real-Firefox runs.
- Cover quick edits, multi-file fixes, ambiguous failures, a long multi-stage migration, conflicts between swarm workers, and browser workflows with slow pages, authentication, popups, and forms. Add hidden acceptance checks and record completion, false success, regressions, manual interventions, and cases where progress stalls. Score partial work explicitly.
- Measure time to usable composer, first visible output, verified completion, and cancellation. Separate provider wait and inference from serialization, queueing, tools, and painting. Track input, output, cached and retry tokens, model calls, browser operations, RSS, handles, queue depth, and disk growth. Use paired runs and report medians, tail latency, and uncertainty.

## 1. Fix the Firefox tab leak and target selection

[browser.rs](../src/alphacode_app_core/tool/browser.rs) currently defaults an untargeted `open` to `newTab: true`. Its session pin stores the most recent target, so successive opens can leave older tabs behind. The bundled [Firefox extension](../AlphaCode-Browser-Agent-1.6.4.xpi) creates a tab before waiting for readiness. If that wait times out, the Rust fallback can resend `newTab: true` without a tab ID and create another. These paths are visible in source and still need reproduction against a running browser.

1. **Prove both paths.** Reproduce repeated untargeted opens, then force a readiness timeout after tab creation and count Firefox tabs before and after. Add a Rust fake-bridge regression and a real-Firefox fixture with a user tab, two agent sessions, and a slow page.
2. **Reuse a safe default.** Make implicit `open` reuse the session's owned working tab in the same profile, window, and container. Create a background tab on first use and keep explicit `new_tab: true` for parallel work. Keep selected user tabs distinct from owned tabs. Validate stale targets against the current browser instance and keep agent operations off the user's active tab when a target disappears.
3. **Split tab allocation from readiness.** Return the tab ID before waiting, or return it with a structured timeout. Give allocation an operation ID the bridge can reconcile when a response is lost. Wait or inspect the known tab after a timeout. For an older bridge, use explicit allocation followed by targeted navigation where supported and reconcile ambiguous allocations before retrying. Test protocol compatibility before changing the default.
4. **Track and release owned resources.** Record all agent-created tabs, including popups and forks when ownership can be attributed. Distinguish temporary, retained, and borrowed tabs. Clean up temporary tabs after completion, failure, or cancellation, reconcile after a restart, and expire abandoned leases only after verifying ownership. Preserve user tabs and tabs explicitly retained for review. Start with a configurable cap of four managed tabs per session and a shared concurrency cap. At capacity, reuse an idle owned tab or return a concise recovery action. Serialize competing allocations and writes to the same tab.
5. **Ship the hotfix independently.** Require zero temporary tab leaks and zero user-tab changes in the fixture. Exercise 100 sequential navigations, lost replies, cancelled runs, parallel sessions, container separation, Firefox restart, and manual tab closure. The first patch should ship as soon as safe reuse and retry behavior pass on the supported bridge versions.

## 2. Make multi-day runs recoverable

Extend the [checkpoint manager](../src/alphacode_app_core/autonomous/checkpoint_manager.rs), session persistence, and [swarm persistence](../src/alphacode_app_core/server/swarm_persistence.rs). The project checkpoint currently writes JSON and updates the state index in separate steps. Treat crash recovery across those steps as a test case.

- Persist a versioned run record with the user's exact objective, constraints, acceptance criteria, approved scope, task dependency graph, completed evidence, artifact references, workspace revision, and pending actions. Keep a bounded prompt view over this record. Store the transcript and large outputs separately with searchable references and retention limits.
- Write checkpoint generations atomically, verify the latest generation on load, and recover from a damaged tail. Record intent and outcome around external mutations using stable operation IDs. If a tool cannot confirm whether a side effect happened, reconcile from tool-specific evidence or mark that operation as needing review. Resume independent work that remains safe.
- Recover each failure at its own boundary. Reattach the TUI from the last acknowledged event, recover stalled workers from leases, and handle provider disconnects and quota cooldowns with bounded retries. Persist the next retry time and surface the last meaningful progress, current blocker, and next action. Keep execution attached to the daemon across TUI exits.
- Exercise hard kills between intent and execution and between execution and outcome persistence. Inject sleep/wake, provider 429s and outages, bridge disconnects, disk-full and partial writes, and worker crashes. Start with a short repeatable recovery suite, then run 24-hour and 72-hour mixed-workload soaks before promoting defaults.

## 3. Lower token cost and improve small-model results

Extend [adaptive tool selection](../src/alphacode_app_core/agent/turn_execution.rs) and [compaction](../src/alphacode_app_core/agent/compaction.rs). Alphacode already has short prompts and bounded schema selection for small local contexts. Apply the same budgeting to providers based on observed capabilities, including hosted free models.

- Count the assembled request by component before sending it. Reserve output and tool-result headroom, preserve stable cacheable prefixes, and audit every repeated schema, instruction, and tool result. Use provider token counts when available, calibrate estimates against usage, and track effective context size separately from billed cache usage.
- Keep user constraints, the active task, and recent verified state in the working prompt. Retrieve older evidence by file, symbol, task, or artifact ID. Store large logs, screenshots, diffs, and DOM results once and give the model a typed summary with targeted retrieval. Deduplicate repeated results while retaining errors, counterexamples, and evidence provenance. Test that facts needed late in a run survive compaction and can be recovered after restart.
- Probe structured output, native tools, streaming, and actual context limits. Offer a compact validated action format when native tool calling is unreliable. Check types, permissions, and preconditions locally. Permit a bounded repair attempt and report a specific validation error. Run repeated procedures through existing tools and deterministic workflow steps.
- Add a free-only routing mode that keeps all planning, repair, verification, and fallback within the selected model budget. Evaluate 4K and 8K context profiles, with and without reliable native tool calling. Route any optional stronger-model assistance only within the configured policy and record its cost. Require budget and quota exhaustion to produce a resumable run state.
- Compare three configurations on the same held-out tasks with the same permissions and tool environment. Run the small model on the current harness, the small model on the improved harness, and a frontier model on the improved harness. Count planner, reviewer, retry, and compaction calls in the totals. Report the quality gap per task family and tokens per verified success alongside pass rates.

## 4. Improve strategy, task execution, and swarms

- Extend [GoalContract](../src/alphacode_task_types/goal_contract.rs), the [DAG scheduler](../src/alphacode_plan/dag/schedule.rs), and the [swarm server](../src/alphacode_app_core/server/swarm.rs). For complex tasks, plan a short dependency graph around verifiable outcomes. Give each node inputs, ownership, acceptance checks, a retry budget, and a compact output contract. Keep small requests on a direct execution path.
- Move scheduling and progress checks into code. Dispatch independent nodes concurrently within provider and tool quotas. Protect shared files and browser state, isolate conflicting code edits in worktrees, and integrate through diff review and cross-node tests. Use structured worker reports with evidence references. Retrieve full worker transcripts only when diagnosing a failure.
- Separate strategy from verification. Let the planner record assumptions and alternatives where they affect the next decision. Run executable checks and reconcile contradictions before advancing a milestone. Use a focused second reviewer for high-impact work when the budget supports it. Detect repeated actions with unchanged state, then re-plan the blocked node or try a bounded alternative supported by the evidence.
- Require final completion to match the original acceptance criteria. Persist checks, exact outcomes, and unresolved gaps. Compare swarms against a single worker at the same total token budget and at comparable latency. Enable automatic fan-out only for task classes where it improves verified success or completion time.

## 5. Improve browser skill and throughput

- Make the default loop **observe → act → verify**. Produce compact snapshots with accessible names, roles, document-scoped element references, visible state, and frame identifiers. Invalidate references on navigation or page change and re-observe the affected region after stale or ambiguous results. Use viewport images for canvas, visual ambiguity, and layout verification.
- Add condition-based waits for visible or enabled elements, route changes, and expected results. Exercise iframes, virtual lists, shadow DOM, popups, dialogs, downloads, uploads, consent, and authentication handoffs. Verify outcomes from browser state after writes and submit operations. Keep page content outside the instruction and permission boundary.
- Measure native-process startup and JSON transfer in the current command-per-process bridge. If they dominate, move to persistent, versioned IPC with capability negotiation, deadlines, cancellation, and reconnect. Batch independent observations and compact repetitive DOM data. Serialize dependent actions and put uncertain side effects through the same reconciliation used for multi-day runs.
- Build real Firefox scenarios for research, dashboard extraction, multi-step data entry, and web QA with known end states. Compare against human operators on the same tasks, including warm-up conditions and time spent on retries or handoffs. Claim superiority only for scenarios where measured completion, accuracy, and time support it.

## 6. Improve runtime speed and polish the TUI

- Profile release builds before changing hot paths. Use the existing [performance tests](../tests/perf_gates.rs) for model-picker scaling and extend measurements to cold/warm startup, tool queueing, stream dispatch, persistence, long transcripts, and memory. Defer nonessential startup work. Use bounded background queues and move blocking work off input and rendering paths. Retain ordered results and cancellation across both agent entry points.
- Keep Ratatui and Crossterm. Use a transcript-first layout with a persistent multiline composer and a compact model, run, and connection status row. Add a keyboard-accessible details pane for tasks, workers, browser tabs, and verification, with an overlay at narrow widths. Use a searchable command palette and inline actions for resume, retry, copy, and opening evidence.
- Present tool results as concise status rows with expandable output, readable diffs, and selectable text. Show meaningful progress during long operations. Preserve drafts, scroll position, and partial output across reconnects and cancellations. Support discoverable key bindings, undo, history search, asynchronous paste, visible focus, terminal-cell-aware layout, and no-color operation. Keep headless output scriptable.
- Reuse the [Windows input reader](../src/alphacode_tui/tui/terminal_events.rs), bounded ordered queue, and existing reader-to-flush instrumentation. Keep ordinary typing independent of rendering, daemon work, and paste timers. Coalesce stream redraws and virtualize old transcript content while preserving selection and scroll anchors. Review compact and wide layouts, resize, Unicode, terminal restoration, and idle/working/recovering/error states. Update buffer assertions with visible wording changes.

## Release gates

The numerical targets are proposed starting gates. Calibrate them on the pinned baseline and record the fixture, machine, model, and sample size beside each result.

| Area | Proposed gate |
| --- | --- |
| Browser ownership | Zero leaked temporary tabs, duplicate allocations after lost replies, or unintended user-tab changes across the Firefox regression suite. |
| Long runs | Pass a 72-hour soak with zero unhandled crashes, lost committed milestones, or duplicate externally visible effects. Recover after each injected fault or identify the exact external blocker. Enforce explicit memory, queue, handle, and disk budgets with no sustained resource growth under steady load. |
| Token efficiency | Reduce median total tokens per verified success by at least 40% on the fixed short and medium suites with no decline in verified pass rate. Report p95, per-model breakdown, and cache hits separately. |
| Small and free models | Improve held-out verified pass rate by at least 15 percentage points over the same model on the baseline, with zero paid calls in free-only runs. Record the frontier gap by task family. |
| Correctness | Zero false completions in deterministic fixtures. Track false success, regressions, and intervention rate on live evaluations. |
| Speed | Target 25% lower p95 time to verified completion on fixed local/tool-heavy workloads. Report harness time and provider time separately. |
| Console and TUI | On the pinned Windows Terminal setup, target reader-to-frame-flush p95 ≤ 33 ms and p99 ≤ 66 ms while idle, after model selection, and while streaming. Require zero lost or reordered inputs in controlled burst tests. |

Run `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo check --workspace --tests`, `cargo test --lib`, and `cargo clippy --lib -- -D warnings`. On Windows, load VS 2022 MSVC in the build PowerShell process and set `CARGO_TARGET_DIR` to `C:\Users\or0to\AppData\Local\Temp\alphacode-target`. Keep timing gates on controlled runners and verify CI on the exact tagged revision before publishing.

## Implementation order

1. Open the baseline and browser regression work together. Ship safe tab reuse and retry handling first.
2. Land durable run state and recovery, then build the token-budget path on its evidence and artifact references. Start the short fault-injection runs immediately.
3. Enable evidence-gated complex task execution and swarm scheduling on top of durable runs. Compare free-only, frontier, single-worker, and swarm results before changing routing defaults.
4. Improve browser observation and action handling once tab ownership and recovery are reliable. Start TUI profiling and layout prototypes early, then connect the final status and details views to the verified runtime state.
5. Run the long soaks, Windows Terminal checks, and browser/model evaluations. Roll out behavior changes incrementally and retain compatibility with existing sessions and supported browser versions.

## Implementation and verification log

### 2026-11-11 reference setup

- Source reference: `7dd13fe459661f66db016c06727e49d8e7cf2351`. The checkout contained in-flight edits, so the bridge reproduction below isolates protocol behavior rather than claiming a clean end-to-end build of that revision.
- Windows 11 Pro build 26300, Intel Core i7-13700HX, Windows Terminal 1.24.12741.0, Rust 1.94.1, VS 2022 MSVC.
- Firefox 159.0.1, installed AlphaCode Browser Agent 1.6.3, protocol 1.3.0, native bridge CLI 1.0.0. Bridge binary SHA-256: `2bf7bebb25c4ab8cbcf88776a64a3c7143f4d7d00e134ca26d7620ee8703ac8b`. Bundled 1.6.4 XPI SHA-256: `1704f213867004091c0a59a58ee9eeff3e1dda01b63b60385a50e19a0e490692`.
- One local HTTP fixture run reproduced both paths using the original wire requests. Two untargeted opens produced two background tabs. A one-second readiness timeout reported the ID of the tab it had already allocated. Replaying the request created a second tab for the same slow URL. Firefox reported both tabs after they finished loading, and the fixture tabs were removed.
- Deterministic browser regressions cover 100 sequential opens, a borrowed tab, a readiness timeout, and a lost allocation reply. The opt-in Firefox check uses the same Rust navigation code and a loopback server. On Windows, point `ALPHACODE_HOME` at the installed bridge home for this opt-in test, then run `cargo test --lib firefox_working_tabs_stay_isolated_across_sessions -- --ignored --nocapture`.
- The opt-in Rust Firefox fixture passed on the installed 1.6.3 extension with two sessions, a loopback readiness timeout, and explicit cleanup.
- A separate live bridge run allocated one background tab for each of two sessions, observed a slow request in progress at the readiness deadline, and navigated both tabs repeatedly using their IDs. Explicit fixture cleanup restored the original tab inventory and selection. A subsequent protocol run completed 100 targeted navigations across two fixture tabs. After closing those two tabs, the original tab IDs, URLs, and active flags matched the starting snapshot.
- The managed-open tab limit defaults to four per session. Set `ALPHACODE_BROWSER_MANAGED_TAB_LIMIT` to a positive integer to change it. Allocation markers and ownership are process-local. The bridge connection timestamp is used conservatively to reject reuse after a reconnect. Automatic cleanup and cross-process ownership still require durable leases and a browser-side ownership identifier.
- Checkpoint tests simulate the crash window after committing a generation and before replacing its state index, a damaged generation, and recovery when the project state file cannot be parsed. These tests exercise filesystem recovery and leave external mutation reconciliation open.
- The paired model and TUI workloads, controlled timing baselines, browser compatibility matrix, and long soaks remain pending. No release gate is calibrated from this sample.

### Follow-up fixes

- Firefox working-tab selection tries the remaining owned tabs if the last one was closed outside Alphacode. At the managed limit, allocation checks the live tab inventory and frees capacity for closed tabs. Fake-bridge regressions cover both paths, including refusal when every slot is still occupied.
- Checkpoint restore recovers the saved project objective when `STATE.json` is missing or empty. A restart regression removes the file after committing a checkpoint and verifies the restored objective and index.
- Alphax Free handles an opaque HTTP 400 with one alternate on the configured free profile. A second rejection restores the composer and leaves the route-switch action available. The gateway redacted the underlying rejection reason.
- The LM Studio picker uses endpoint-scoped catalogs and the advertised wire model ID. A live request to the user's local server returned `qwen3.8-9b-heretic-uncensored`. The server streamed reasoning deltas before its final usage sample.
- CTF probe and goal-phase thresholds now prompt checkpoints without rejecting an authorized call or forcing a report. Both turn entry points continue actionable CTF todos after a premature response and avoid retrying unchanged evidence.
- The composer status shows the active generation average. It marks estimates from deltas with `~` until provider usage arrives. Terminal rendering tests cover the rate on wide and narrow layouts. Release-build reader-to-frame-flush measurements remain pending.
