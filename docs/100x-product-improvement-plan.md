# AlphaCode 100× Product and Engineering Plan

**Status:** Active execution plan  
**Date:** 2026-09-25  
**Scope:** reliability, speed, accuracy, tools, skills, providers, multi-agent execution, TUI/UX, security, testing, and release quality

## 1. Executive intent

“100× better” must not mean an unfocused promise that every workflow becomes exactly one hundred times faster. A professional interpretation is **order-of-magnitude improvement in the workflows that matter most**, achieved by compounding several smaller gains:

- **10× less waiting** through parallel work, caching, speculative execution, and better scheduling.
- **10× less repeated work** through durable plans, structured memory, idempotency, and recovery.
- **10× more verified outcomes** through evidence gates, targeted tests, and self-review.

The result should feel like a different class of product: it starts instantly, stays responsive, reconnects by itself, uses the strongest available model for each subtask, recovers from provider/tool failures, and can explain exactly what changed and why it is trustworthy.

This document is the product-level plan. The detailed implementation designs in [`10x-improvement-plan.md`](10x-improvement-plan.md) and [`tui-model-picker-100x-review.md`](tui-model-picker-100x-review.md) remain the technical references.

---

## 2. North-star outcomes and scorecard

Every proposal must improve at least one measurable outcome without materially harming another.

| Dimension | User-visible outcome | Release gate |
| --- | --- | --- |
| Reliability | Sessions survive crashes, disconnects, provider failures, and malformed events | Crash-free sessions ≥ 99.95%; no process-fatal background task |
| Recovery | A dropped server reconnects without restarting AlphaCode | ≥ 99% reconnect success within 30 seconds under fault injection |
| Speed | Useful work begins quickly and independent work runs concurrently | p95 local time-to-first-progress ≤ 2 s; independent read-only tool phase ≥ 2× faster |
| Accuracy | The agent verifies its changes and avoids repeating ineffective actions | ≥ 95% completed tasks include concrete verification evidence |
| Efficiency | No duplicate side effects and bounded memory/context growth | Duplicate side effects = 0; RSS and history growth bounded in soak tests |
| UX | The interface always explains state and next action | Every long-running state has progress, cancel, retry, and recovery behavior |
| Tool quality | Tool failures are actionable and safe to retry | 100% of common tool errors include a structured cause and next step |
| Extensibility | Skills and MCP tools are discoverable, evaluable, and safely scoped | Every skill has a manifest, permission declaration, and eval case |

### Measurement rules

1. Measure local scheduler/tool overhead separately from provider and network latency.
2. Report p50, p95, and p99; averages alone are insufficient.
3. Record cold start, warm start, and long-session behavior separately.
4. Use deterministic workloads and recorded provider responses for regression tests.
5. Never collect source content or secrets in default telemetry.
6. Every optimization requires a correctness benchmark as well as a speed benchmark.

---

## 3. Delivery principles

1. **Correctness before cleverness.** A fast wrong answer is worse than a slower reliable one.
2. **Smallest safe slice.** Each change must be independently shippable and reversible.
3. **No hidden concurrency.** Every parallel path declares ownership, ordering, cancellation, and conflict semantics.
4. **Idempotency by default.** Retries must not duplicate messages, edits, commands, jobs, or network side effects.
5. **Evidence over confidence.** “Probably fixed” is not a completion state; tests or equivalent evidence are.
6. **Progressive disclosure.** Advanced options remain available without cluttering the normal path.
7. **Graceful degradation.** Missing tools, offline models, poor terminals, and unavailable providers produce useful partial results.
8. **Privacy and safety are product features.** Secret redaction, scoped permissions, and auditable actions are first-class.

---

## 4. Workstream A — Reliability and crash resistance

### A1. Eliminate process-fatal background work

- Audit every `tokio::spawn`, `std::thread::spawn`, and detached task.
- Convert expected runtime failures into structured results rather than panics.
- Audit unsafe time arithmetic, especially `Instant - Duration` on freshly booted systems.
- Audit mutex poisoning, lock ordering, unbounded channels, and blocking work on async executors.
- Add a panic/error boundary for reporters, watchdog tasks, background jobs, and plugin/tool workers.

**Exit criteria:** no detached task can terminate the process in release mode; failures are visible and recoverable.

### A2. Reconnect as an explicit state machine

Model connection lifecycle rather than treating reconnection as a side effect:

```text
Connected
  -> Disconnected
  -> BackingOff
  -> Reconnecting
  -> Resynchronizing
  -> Connected
```

Required behavior:

- Exponential backoff with jitter and an explicit maximum delay.
- A new connection attempt after every terminal socket error.
- Resume token and event-sequence reconciliation after reconnect.
- Deduplication of replayed events.
- Preserve queued user messages and in-flight work when safe.
- Surface attempt number and next retry in the TUI.
- Stop retrying after a deliberate quit or unrecoverable authentication failure.
- Fault tests for disconnect during streaming, tool execution, session mutation, and shutdown.

### A3. Transactional persistence

- Assign monotonically increasing IDs to messages, tool calls, and tool results.
- Persist intent before execution and result after completion.
- Make replay idempotent.
- Validate journal and checkpoint checksums on resume.
- Add crash-injection tests at every persistence boundary.

### A4. Cancellation and shutdown

- One cancellation token tree per turn, tool batch, provider request, and child agent.
- Cancellation must be bounded and must not strand locks, channels, or child processes.
- Partial output handling must be explicit: keep, discard, or resume.
- Shutdown must close servers, browser sessions, MCP children, background tasks, and telemetry in a deterministic order.

---

## 5. Workstream B — Execution speed and scheduling

### B1. Tool execution classes

Add capability metadata to every tool:

- `ReadOnly`: safe to run concurrently if it has no hidden shared-state side effects.
- `Mutating`: serialized unless an explicit dependency graph proves isolation.
- `Exclusive`: requires session-level execution, such as shell control, browser control, or interactive input.
- `Background`: returns a job ID and continues asynchronously.
- `ExternalEffect`: sends messages, modifies remote systems, or consumes paid quota.

The scheduler must preserve provider-required result ordering even when completion order differs.

### B2. Parallel read-only execution

- Run maximal consecutive read-only groups concurrently.
- Default cap: four tasks; configurable by performance tier.
- Keep mutating operations ordered.
- Publish progress as each task finishes.
- Assemble tool results in original call order.
- Add cancellation and per-tool timeouts to the whole batch.
- Benchmark against sequential execution using realistic repository workloads.

### B3. Dependency-aware execution

Represent a turn as a small DAG:

```text
plan -> [read A, read B, search C] -> merge evidence -> edit -> test -> review
```

- Scheduler starts only nodes whose dependencies are satisfied.
- Independent branches can run in parallel.
- Mutating nodes touching the same resource serialize or conflict-check automatically.
- The UI shows the active critical path, not just a pile of running tools.

### B4. Reduce latency overhead

- Pre-warm the immutable system prompt and tool schemas off the first-request path.
- Coalesce redundant session writes while preserving crash-loss bounds.
- Cache provider/model metadata and route catalogs with explicit invalidation.
- Move blocking filesystem/network operations off UI-critical async paths.
- Add hot-path log sampling and structured timing spans.
- Bound transcript indexing and model-catalog work by repository size and tier.

### B5. Context and cache efficiency

- Stable prompt prefixes remain byte-stable for provider caching.
- Dynamic memory is summarized and deduplicated.
- Tool outputs are stored once and referenced by content hash.
- Expired or irrelevant context is evicted by semantic utility, not only recency.
- Prompt-cache hit rate and wasted-token ratio become release metrics.

---

## 6. Workstream C — Accuracy, verification, and self-correction

### C1. Evidence-backed completion

A task cannot be reported complete until it has:

1. A stated objective and acceptance criteria.
2. Evidence that the relevant code path was inspected.
3. A minimal change consistent with project conventions.
4. Tests or an explicitly documented alternative verification.
5. A review for unintended files, secrets, debug output, and regressions.

### C2. Failure taxonomy and recovery budget

Classify failures as transient, authentication, rate-limit, capability, input, policy, conflict, deterministic code, or unknown. Each class gets a bounded retry policy and a recovery action.

- Never repeat an identical failed tool call unchanged.
- Track whether a recovery attempt changed the inputs or strategy.
- Stop when expected value of another attempt is below its cost/risk.
- Surface unresolved uncertainty rather than fabricating success.

### C3. Change confidence and verification planning

Before editing, produce an internal verification plan:

- Which behavior can regress?
- Which existing tests cover it?
- What is the smallest targeted test command?
- What independent evidence can confirm the result?
- Which files or subsystems are outside scope?

### C4. Review independence

- Reviewer agents receive the objective, diff, and evidence—not the author’s rationale alone.
- Reviewers search for correctness, security, concurrency, compatibility, and missing tests.
- High-risk changes require a second independent reviewer or deterministic verifier.
- Findings are deduplicated and resolved with evidence.

### C5. Deterministic evaluation suite

Maintain representative tasks for:

- Single-file bug fixes.
- Cross-file refactors.
- Build/test failures.
- API/schema migrations.
- Async and reconnect faults.
- Security-sensitive changes.
- UI rendering and keyboard-only operation.

Track first-pass success, recovery success, false completion, unnecessary retries, and human edits.

---

## 7. Workstream D — Provider intelligence and model capability

### D1. Task-aware model routing

Route each subtask using actual requirements:

- Coding, tool use, reasoning, long context, vision, speed, cost, and privacy.
- Provider/model health, latency, rate-limit state, and recent task success.
- Explicit user policy overrides automatic routing.

Routing decisions must be inspectable in `/stats` and provider diagnostics.

### D2. Provider reliability

- Circuit breakers with half-open probes.
- Retry only transient and idempotent failures.
- Respect `Retry-After` and provider-specific rate-limit headers.
- Preserve partial output and conversation state across failover.
- Prevent retry storms through shared health state and jitter.
- Never silently switch to a provider with different privacy or cost expectations.

### D3. Capability truth

- Validate context windows, tool support, reasoning controls, and streaming modes.
- Invalidate stale model metadata after provider changes.
- Surface unknown capabilities as unknown, never optimistic.
- Keep normalized URLs and model IDs exact for local/OpenAI-compatible servers.

### D4. Cost and quota control

- Per-task token and cost budgets.
- Expected-value routing between fast, cheap, and strong models.
- Background-work discounts where safe.
- A dry-run estimate for unusually expensive swarms.
- Clear reporting of cost, retries, and fallback reasons.

---

## 8. Workstream E — TUI and user experience

### E1. One honest application state

The TUI should derive a single state from the runtime:

- Idle, thinking, streaming, tool-running, waiting-for-approval, reconnecting, compacting, recovering, and failed.

Every state needs:

- A concise label.
- Progress or elapsed time when relevant.
- A safe cancel action.
- A retry/recover action when applicable.
- Detail available without flooding the transcript.

### E2. Responsive, accessible interaction

- Correct behavior at 60, 80, 100, 120, and 160 columns.
- Full keyboard operation with visible focus and discoverable shortcuts.
- High-contrast and monochrome themes.
- No meaning conveyed by color alone.
- Respect reduced-motion and low-performance terminal policies.
- Avoid animated text and glyph-cache churn.

### E3. Faster discovery

- Command palette ranked by recency, frequency, and task relevance.
- Grouped help by intent.
- Model/account/tool pickers with search, facets, cached first paint, and useful empty states.
- Inline “why this is unavailable” details with an actionable next step.

### E4. Trustworthy output

- Clearly mark truncated, summarized, cached, or partially restored output.
- Provide a safe way to inspect the full source/output.
- Distinguish model claims, tool evidence, test output, and user input.
- Keep diffs and verification results adjacent to the relevant change.

### E5. Onboarding and diagnostics

- Reach a first successful verified task in under five minutes.
- `/doctor` explains provider, terminal, tool, browser, MCP, skill, and server health in plain language.
- Setup flows validate the credential with a real bounded probe.
- Errors provide the exact next command and preserve context.

---

## 9. Workstream F — Tools

### F1. Structured tool contracts

Every tool exposes:

- Stable name and version.
- JSON schema with strict validation.
- Capability class and required permissions.
- Idempotency behavior.
- Cancellation and timeout contract.
- Bounded input/output sizes.
- Structured error code, retryability, and remediation.
- Deterministic local test where possible.

### F2. Safer mutations

- Prefer dry-run/plan execution for risky tools.
- Use transactional writes with rollback or atomic replacement.
- Re-read and version-check files before edits.
- Detect overlapping edits from concurrent agents before commit.
- Keep audit records for destructive or external actions.

### F3. Better native tooling

Prioritize tools that reduce model uncertainty:

- Structured code and repository search.
- HTTP request/response capture.
- Browser state plus screenshots and DOM/accessibility snapshots.
- Desktop state with explicit action verification.
- Test discovery, targeted execution, and failure parsing.
- Diff-aware review and static analysis.
- CTF/security workflow support with strict authorized-scope controls.

### F4. Batch composition

- Support bounded, typed batches rather than shell-only composition.
- Return per-item status, timing, stdout/stderr, truncation, and artifact references.
- Share one browser/HTTP/network context where safe.
- Preserve original item order and provider tool-call correlation.

### F5. MCP and extension lifecycle

- Discover, health-check, authenticate, and version-lock MCP servers.
- Apply per-tool permissions and timeouts.
- Isolate crashes and malformed schemas.
- Show provenance and trust state in the UI.
- Cache schemas with bounded invalidation.

---

## 10. Workstream G — Skills

### G1. Skill manifests

Add a machine-readable manifest for every skill:

- Name, version, description, tags, compatibility, and dependencies.
- Required tools and permissions.
- Inputs, outputs, risk level, and expected artifacts.
- Example tasks and evaluation cases.
- Optional model/tool capability requirements.

### G2. Progressive disclosure

- Router sees compact metadata first.
- Full instructions load only when confidence and task relevance justify it.
- Large references are loaded by named subsection.
- Every loaded skill reports token cost and why it was selected.

### G3. Skill evaluation

Each bundled or user skill has automated cases measuring:

- Routing precision.
- Instruction adherence.
- Tool selection.
- Output/evidence quality.
- Token and wall-time cost.
- Regression against the no-skill baseline.

### G4. Safe composition

- Detect conflicting skill instructions.
- Merge permissions explicitly.
- Support versioned dependencies and local overrides.
- Never let a skill silently broaden filesystem, network, shell, or messaging permissions.

### G5. Learning from failures

Turn recurring failures into:

1. A structured failure pattern.
2. A routing or prompt improvement.
3. A regression evaluation.
4. A user-visible capability when appropriate.

Do not self-modify production behavior without tests, review, and rollback.

---

## 11. Workstream H — Multi-agent and swarm execution

### H1. Role planner

Select agents by required capability and marginal value. A second agent must have an independent task, tool set, or verification role; duplicate workers are waste.

### H2. Shared blackboard and DAG

Workers publish typed artifacts, evidence, assumptions, conflicts, and file claims to a shared state. The coordinator consumes a stable snapshot, not ad-hoc chat transcripts.

### H3. Budgets and termination

- Wall-time, token, cost, tool-call, memory, and depth budgets.
- Progress-based continuation rather than fixed fan-out.
- Detect duplicate work, stalled workers, and dependency cycles.
- Cancel descendants when a parent task is no longer useful.

### H4. Merge safety

- File-level and semantic conflict detection before merge.
- One owner per mutable artifact.
- Verifier role for high-risk phases.
- Checkpoint before destructive transitions and automatic rollback where possible.

---

## 12. Workstream I — Security and privacy

- Capability-scoped permissions for shell, filesystem, network, browser, desktop, messaging, and providers.
- Workspace boundary enforcement with explicit user-approved escapes.
- SSRF defense for all URL-capable tools.
- Secret detection and redaction in logs, transcripts, telemetry, and model context.
- Dependency and release artifact verification.
- Optional sandbox profiles with filesystem/network process isolation.
- Tamper-evident audit records for privileged actions.
- User-visible command showing exactly what data leaves the machine.
- No hidden telemetry content; redaction before serialization, not after.

---

## 13. Workstream J — Engineering quality and release gates

### Required test layers

1. **Unit:** pure state machines, parsers, policies, schedulers, and formatting.
2. **Component:** tool, provider, persistence, and TUI widget boundaries.
3. **Integration:** real files, local HTTP/WebSocket servers, subprocesses, and MCP fixtures.
4. **Fault injection:** disconnects, malformed frames, disk-full behavior, cancellation, timeouts, and panic boundaries.
5. **Property/fuzz:** protocol parsers, state transitions, persistence replay, and path handling.
6. **Performance:** cold start, tool scheduling, picker paint, transcript render, memory, and soak tests.
7. **Visual snapshots:** narrow/wide terminals, themes, accessibility modes, and high-DPI Unicode.

### Release requirements

- `cargo fmt --check`
- Locked build for supported feature sets and operating systems
- Clippy with warnings denied
- Unit, integration, and protocol tests
- Performance gates on controlled runners
- Optional-feature compile checks
- Installer/update/rollback smoke tests
- Dependency and secret scans
- Crash and reconnect soak test
- Changelog and user-facing migration notes

---

## 14. Phased execution plan

### Phase 0 — Reliability foundation (current)

**Goal:** no known process crash or manual-restart recovery loop.

- [x] Fix health reporter `Instant` underflow.
- [x] Add watchdog trend regression coverage.
- [x] Fix related production underflow paths.
- [x] Synchronize `Cargo.lock`.
- [x] Implement automatic server reconnect and resynchronization.
- [x] Add protocol-bootstrap timeout, delayed-history, missing-session, and reload lifecycle fault tests.
- [ ] Pass complete unit, integration, formatting, and Clippy gates.

**Exit criteria:** the reported crash and reconnect scenarios pass automated fault tests and a manual end-to-end run.

### Phase 1 — Faster execution loop

- [x] Add tool capability classes and conservative unknown-tool defaults.
- [x] Parallelize maximal consecutive read-only groups at a cap of four while serializing mutations/external effects.
- [ ] Add dependency-aware scheduling and progress events.
- Add performance workloads comparing sequential and parallel execution.
- Coalesce redundant persistence writes.
- Pre-warm immutable prompt/tool data.

**Exit criteria:** ≥2× faster read-heavy turns with identical ordered results and no mutation regressions.

### Phase 2 — Accuracy and recovery

- Structured failure taxonomy and retry budgets.
- Evidence-backed completion gate.
- Targeted verification planner.
- Deterministic task evaluation suite.
- Independent review and finding deduplication.

**Exit criteria:** measurable gains in first-pass success and a major reduction in repeated ineffective actions.

### Phase 3 — TUI trust and usability

- Unified runtime state model.
- Complete command palette and grouped help.
- Reconnect, cancellation, truncation, and verification affordances.
- Responsive/accessibility snapshot matrix.
- Faster model/account/tool pickers.

**Exit criteria:** all core workflows keyboard-complete, understandable at 80 columns, and recoverable without process restart.

### Phase 4 — Skills, tools, and MCP ecosystem

- Skill manifests, router precision, progressive loading, and evals.
- Structured tool errors, dry-run, idempotency, and transaction support.
- MCP lifecycle, provenance, and health.
- Expanded native repository/test/browser diagnostics.

**Exit criteria:** adding a skill/tool no longer requires editing central dispatch code and unsafe behavior is blocked by default.

### Phase 5 — Multi-agent scale and provider intelligence

- Role planner, blackboard, DAG scheduler, budgets, and merge safety.
- Task-aware provider routing, health-aware fallback, and cost controls.
- Long-running soak and recovery tests.

**Exit criteria:** swarm work scales in useful throughput rather than token spend, with bounded cost and conflict rate.

### Phase 6 — Continuous optimization

- Maintain benchmark fixtures, production privacy-safe telemetry, and regression dashboards.
- Promote only measured improvements.
- Quarterly dependency/security/toolchain reviews.
- Remove obsolete paths after migration and compatibility windows.

---

## 15. Current execution queue

The next changes should be made in this order:

1. ~~Finish and verify server reconnect recovery.~~ Complete locally; retain the locked fault tests as the release gate.
2. Clear all full-suite and Clippy failures.
3. ~~Add tool execution classification and strict metadata.~~ Conservative classes are implemented; expand contracts as tools are audited.
4. ~~Implement bounded parallel read-only execution.~~ Consecutive groups run at cap four with ordered results.
5. Add deterministic scheduling/performance tests.
6. Continue fault-injection coverage for disconnect, cancellation, and replay.
7. Implement skill manifests/evals and tool error contracts.
8. Continue TUI and provider work from the detailed plans above.

## 16. Final definition of “100×”

AlphaCode has achieved the 100× goal when a representative user can:

- Start and receive useful progress almost immediately.
- Complete read-heavy coding tasks several times faster.
- Recover from provider, server, and tool failures without restarting or repeating work.
- Trust completion because every result includes evidence.
- Discover advanced tools and skills without memorizing internals.
- Use the system comfortably by keyboard, on narrow terminals, and with assistive rendering.
- Scale from one local task to many specialized agents without losing control of cost, safety, or correctness.

That is a durable product improvement. It is more valuable—and more defensible—than a single benchmark claiming an arbitrary multiplier.
