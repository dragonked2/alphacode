# AlphaCode Comprehensive Improvement Plan

**Status:** In progress; see `docs/engineering/improvement-backlog.md` for open gates
**Date:** 2026-10-29
**Scope:** Reliability, performance, correctness, UI/UX TUI, banner/ASCII art, security, testing, release quality
**Goal:** 100× better product — order-of-magnitude gains across workflows that matter most.

## 1. Executive intent

"100× better" = compounding gains:
- 10× less waiting (parallel work, caching, scheduling)
- 10× less repeated work (durable plans, memory, idempotency, recovery)
- 10× more verified outcomes (evidence gates, tests, self-review)

Result: starts instantly, stays responsive, reconnects by itself, uses strongest model per subtask, recovers from provider/tool failures, explains exactly what changed and why trustworthy.

## 2. North-star scorecard

| Dimension | User-visible outcome | Release gate |
|---|---|---|
| Reliability | Sessions survive crashes/disconnects/provider failures | Crash-free ≥99.95%; no process-fatal bg task |
| Recovery | Dropped server reconnects without restart | ≥99% reconnect within 30s under fault injection |
| Speed | Useful work begins quickly; independent work concurrent | p95 TTFP ≤2s; read-only tool phase ≥2× faster |
| Accuracy | Agent verifies changes, avoids repeating ineffective actions | ≥95% tasks include concrete verification |
| Efficiency | No duplicate side effects; bounded memory/context | Duplicate side effects = 0; bounded RSS/soak |
| UX | Interface explains state & next action | Every long-running state has progress/cancel/retry |
| Tool quality | Failures actionable & safe to retry | 100% common errors structured cause+next step |
| Extensibility | Skills/MCP discoverable, evaluable, scoped | Every skill has manifest+permissions+eval |
| UI/TUI | Modern, professional, max user-friendly | No layout shear, all tiers fit width |
| Banner | Correct ASCII art, no raw escapes | Brand tests pass; renders glyphs not source |

## 3. Delivery principles

1. Correctness before cleverness.
2. Smallest safe slice, independently shippable & reversible.
3. No hidden concurrency (declare ownership/ordering/cancellation/conflicts).
4. Idempotency by default.
5. Evidence over confidence.
6. Progressive disclosure.
7. Graceful degradation.
8. Privacy & safety are product features.

## 4. Workstream A — Reliability & crash resistance

### A1 Eliminate process-fatal background work
- Audit every tokio::spawn, std::thread::spawn, detached task.
- Convert expected runtime failures to structured results, not panics.
- Audit unsafe time arithmetic (Instant - Duration on fresh boot).
- Audit mutex poisoning, lock ordering, unbounded channels, blocking on async executors.
- Add panic/error boundary for reporters, watchdogs, bg jobs, plugin/tool workers.

Exit: no detached task terminates process in release; failures visible & recoverable.

### A2 Reconnect as explicit state machine
Connected → Disconnected → BackingOff → Reconnecting → Connected | Failed
Track generation, attempt count, backoff, last error, last good frame.

### A3 Session durability & idempotency
- Stable message IDs, dedupe on replay, resumable streams, transactional tool side effects.
- Checkpoint before risky operations; recover on startup.

### A4 Provider resilience
- Per-provider circuit breakers, retry with jitter, model fallback, capability-aware routing, streaming watchdog.

## 5. Workstream B — Performance & responsiveness

### B1 Startup
- Lazy module init, parallel config/provider probe, cache warmup, async disk, measure cold/warm.

### B2 Scheduling
- Separate read-only tool phase parallel execution; write tools serialized with locks; bounded concurrency; cancellation propagation.

### B3 Context & memory
- Structured compaction, token budgeting, prompt caching, dedup, memory indexing, bounded history.

### B4 Rendering
- Diff-based terminal redraw, batched frames, avoid full recompute, virtualized transcript, image caching.

## 6. Workstream C — Correctness & verification

### C1 Evidence gates
- Every completed task requires concrete verification (test/build/diff/state).
- Self-review pass before declaring done.

### C2 Tool quality
- Structured errors with cause + next step; retry safety; timeout; output truncation & summarization; sandboxing.

### C3 Skill/MCP
- Manifest, permission declaration, eval case, scoped access, discovery.

## 7. Workstream D — UI/UX TUI (max user-friendly, professional, modern)

### D1 Layout & responsiveness
- Adaptive tiers (already in brand_ux); avoid shear; min widths; graceful fallback to text label.
- Consistent spacing, alignment, borders, focus indicators.

### D2 Interaction
- Keyboard-first with discoverable shortcuts; mouse support where useful; command palette; autocomplete.
- Progress/cancel/retry for every long-running state.

### D3 Visual polish
- Gradient brand theme; syntax highlighting; diff rendering; status chips; tooltips; loading spinners.
- Dark/light theme support; accessibility (contrast, screen reader hints).

### D4 Error presentation
- Inline, actionable, non-blocking; redact secrets; copy-to-clipboard.

### D5 Onboarding
- First-run wizard, provider setup, doctor command, helpful empty states.

## 8. Workstream E — Banner & ASCII art (fix)

> **Status:** delivered. See `docs/engineering/workstream-e-banner.md` for the
> defects found, the changes, and the verification performed. Tier rules are in
> `docs/branding.md`. `cargo test`/`clippy` were not run for this slice; the
> recorded gate must be executed before release.

### Current state
- Brand module `src/alphacode_brand.rs` already centralizes `WORDMARK_BLOCK`, `WORDMARK_COMPACT`, `WORDMARK_MONOSPACE`, `WORDMARK_MINIMAL`, `WORDMARK_LADDER`, `wordmark_for`, `compact_wordmark_for`.
- Tests assert row widths, tier ordering, no raw `\u` escapes.
- `SplashScreen::wordmark_art` reads shared ladder.

### Issues / risks
- CLI startup may not render the shared art (grep showed no usage in src/cli) — verify CLI banner path.
- Ensure all surfaces (CLI, TUI header, TUI splash, window title) use shared brand module — single source of truth.
- Fix any ragged rows (width mismatch) — tests guard.
- Ensure narrow terminals degrade to text label not wrap.

### Actions
1. Audit every banner/splash/header site for shared brand usage; remove private copies.
2. Add CLI startup banner using `wordmark_for` with terminal width detection.
3. Add tests rendering banner at widths 0..200 asserting no panic and width bounds.
4. Verify raw escape absence in all const strings.
5. Add color gradient consistency across surfaces.
6. Document banner tiers in docs.

## 9. Workstream F — Security

### F1 Secrets
- Redact in logs/reports/screenshots; prefer env/secret store; remove temp secret artifacts.

### F2 Sandboxing
- Tool execution sandbox; path traversal guards; command risk classifier; network egress controls.

### F3 Auth
- Session expiry diagnosis; multi-account labeling; tenant boundaries.

### F4 Supply chain
- Dependency audit; pinned versions; SBOM; signature verification.

## 10. Workstream G — Testing & quality

### G1 Coverage
- Unit/integration/e2e; property-based for parsers; snapshot tests for UI.

### G2 Regression
- Deterministic workloads; recorded provider responses; soak tests.

### G3 CI
- fmt, clippy, tests, build matrix, perf gates.

## 11. Workstream H — Documentation & DX

- README, configuration, architecture, troubleshooting, contributing.
- Changelog discipline; release notes.

## 12. Execution roadmap (phased)

Phase 0 (1-2 weeks): Audit & baseline — metrics, crash inventory, perf baselines, test matrix.
Phase 1 (2-4 weeks): Reliability core — A1-A4, reconnect state machine, durability.
Phase 2 (2-4 weeks): Performance — B1-B4.
Phase 3 (2-4 weeks): UI/UX & banner — D*, E*.
Phase 4 (2-4 weeks): Security & tools — F*, C2-C3.
Phase 5 (ongoing): Testing, docs, release quality.

## 13. Issue inventory method

For each module:
1. Run cargo clippy, cargo test, cargo audit.
2. Grep TODO/FIXME/XXX/HACK/unwrap/panic/todo!.
3. Check git blame for recent regressions.
4. Run under sanitizers / miri where applicable.
5. Fuzz parsers and protocol frames.
6. Profile hot paths.

## 14. Immediate next actions

1. Run full test suite; capture failures.
2. Run clippy with -D warnings; capture.
3. Audit tokio::spawn sites count & owners.
4. Audit banner usage across surfaces; fix CLI.
5. Establish perf baselines.
6. Create issue tracker entries per workstream.

## 15. Success verification

- All tests pass; clippy clean; no unwrap in hot paths.
- Crash-free soak 24h.
- Banner renders correctly at widths 40/80/120/200.
- UI snapshot tests pass.
- Perf gates meet thresholds.

## 16. Execution record (2026-10-29)

- Shared brand art now renders as a centered CLI `run` banner on interactive
  terminals, with an ASCII fallback and no banner in quiet/JSON/NDJSON output.
- Explicit LM Studio/Ollama startup now discovers a live chat model when no
  model is specified. Direct local runtime construction no longer borrows a
  remote model name. Authenticated local model lists use the configured key.
- The library suite passed with 6,273 passed, 0 failed, 12 ignored when run
  serially with terminal color enabled. The parallel run exposed one
  session-restore test race; it passed alone and remains an open test-isolation
  issue.
- `cargo fmt --all -- --check`, `cargo check --workspace --tests`, and
  `cargo clippy --lib -- -D warnings` passed on Windows after the fixes.
- Release-profile performance gates, a 24-hour soak, crash-free/reconnect
  metrics, and a dependency audit have not been measured yet. `cargo audit`
  is not installed on this machine.
