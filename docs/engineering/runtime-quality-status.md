# Runtime quality work

The historical notes below describe earlier implementation work. Current local
checks and remaining gaps are recorded in [the 2026-11-13 quality evidence](quality-evidence-2026-11-13.md).
That evidence does not establish that every live recovery or terminal scenario
listed here has been exercised.

This is an implementation status record, not a benchmark or a perfect-quality
rating. The user requested that checks be skipped. Builds, regression tests,
interactive terminal exercises, and recovery scenarios below remain unexecuted.

## Implemented changes

| Concern | Change | Evidence still needed |
| --- | --- | --- |
| Completion evidence | Tool/input-bound metadata checks replace automatic text matches; per-criterion provenance accumulates across calls and failed reruns revoke passes. | Execute unit and recorder integration cases; exercise real provider paths. |
| Runtime consistency | Shared goal initialization, admission, observation, and native-tool execution across streaming and non-streaming loops; streamed serial and parallel calls share result recording. | Exercise ordered batches, terminal rejection, SDK-native callbacks, cancellation, and reconnect. |
| Tool timing | Serial execution passes its measured duration; parallel tasks capture completion duration independently of ordered result collection. | Compare stored durations with controlled slow/fast concurrent tools. |
| Coordinator state | Shared phase-plan types live in `alphacode_task_types::project_plan`; `ProjectState.plan` persists the machine-readable plan and hydrates it on restart. `PLAN.md` is a derived view. | Reopen, corrupt/missing-state recovery, failed writes, and old-snapshot compatibility. |
| Phase transitions | Commit phase statuses, active phase, completion summary, and phase statistics in one state replacement. Skip finished phases, preserve blocked next phases, and avoid repeating a committed transition after a projection/checkpoint failure. | Fault injection at each persistence boundary and phase advancement regressions. |
| Checkpoint semantics | Crash recovery preserves readable newer state; explicit restore uses the selected snapshot. Rollback keeps later checkpoints available. | Execute snapshot, corruption, rollback-history, and index-reconciliation cases. |
| Concurrent state updates | State read-modify-write and recovery share a stable OS sidecar lock with a bounded acquisition wait. Initialization uses create-new writes. Checkpoint index reconciliation re-reads current state under ownership. | Execute concurrent-manager and failed-transaction cases; add and run independent-process/fault-injection scenarios. |
| Stale coordinator writes | Plan commits compare the current plan/objective with the coordinator's last committed view. Markdown publication checks the source again under ownership. | Execute stale-plan/stale-projection regressions, including cross-process writers. |
| Worker handoffs | Shared assignment/report types persist in ProjectState. Registration reports commit errors, reserves capacity, and records phase ownership; reports commit before summary projection, reject unknown/conflicting outcomes, and replay idempotently across restarts. | Execute persistence failure, restart, report replay/conflict, unfinished-phase, and capacity regressions. |
| Run ownership | New objectives receive a run ID, including repeated identical objectives. Plan, assignment, report, and derived-plan writes fence obsolete coordinators. Proposals use stable per-phase definition IDs and exclude previously assigned work. | Exercise concurrent objective replacement and late worker callbacks through real runtime wiring. |
| Live swarm outcomes | Explicitly watched missing members remain unknown instead of implicitly completed/stopped. Owner notifications cover stale/streaming/queued outcomes and a changed report arriving after the status transition; identical reports do not notify twice. | Execute await observation, status matrix, and late-report fanout regressions. |
| Live swarm labels | Shared UTF-8-safe short IDs replace byte slicing in status, await, headless, message, and client-action paths. | Execute Unicode label and late-report regressions through attached clients. |
| Swarm-wide wait cohorts | Await records persist observed members independently of the dynamic membership query. Removed unfinished members remain unknown, observed matching outcomes survive removal, late joiners extend the cohort, and retry/expired-resume paths reconcile the same history. | Execute removal/reload, late-joiner, deadline, and compatibility regressions. |
| Await finalization | Per-request daemon locks serialize observations, preferences, and final results. Stale preference saves retain the observation ledger and cannot reopen a committed result. Competing finalization paths use the first stored outcome and suppress duplicate in-process wake publication. | Execute finalization races and storage-failure scenarios; audit cross-process ownership and notification delivery. |
| TUI foundations | Shared header/footer label fitting, grapheme-aware compact status clipping, sanitized titles/previews, corrected reload cursors, and bounded image geometry. | Windows Terminal idle/model-selected/streaming latency, narrow layouts, Unicode editing, and terminal image protocols. |
| Styled Unicode output | Shared clipping coalesces graphemes split by style boundaries before measuring, including suffix budgets; the starting span supplies the glyph's style. Memory tiles wrap complete graphemes while retaining exact source prefixes, and zero-width copy-badge content emits no ellipsis. | Execute cross-span emoji/combining/flag/variation-selector, memory wrapping, suffix-width, and badge regressions; inspect terminal buffers and latency. |
| Image preparation | Bounded prewarm workers can be recreated after disconnect, back off failed thread creation, and isolate recoverable per-image panics. Channel failure no longer triggers synchronous pixel decoding/cache writes/scaling in rendering. Long-header dimension fallback reads container metadata rather than decompressing pixels. | Execute worker failure/backoff/capacity/panic and header-only JPEG regressions; exercise actual image protocols and input latency. |
| Recoverable panic handling | The terminal crash hook honors thread-local recoverable-panic suppression regardless of hook installation order. Image jobs reuse the existing suppression scope rather than swapping global hooks per request. | Exercise decoder panics under the real terminal hook; confirm terminal modes and session status remain correct. |
| Terminal initialization | Fallible paste/focus/mouse setup precedes keyboard-stack pushes. Failed inherited setup attempts pop the owned keyboard state, restore modes, and clear stale handoff metadata. Redirected streams are rejected before theme queries. | Execute injected setup-order regressions; exercise partial I/O failure, exec handoff, and terminal restoration on Windows and Unix. |
| Live parallel handoff | Parallel collection removes completed handles before an Alt+B handoff, adopts every remaining task through the background runner, and records admitted outcomes in original call order. Handoff/reload notices are separate from executed results and invalidate stale verification. | Reproduce Alt+B after a partial parallel completion; verify one result per call, background task IDs, and actual wake delivery. |
| Tool task ownership | Turn-owned spawned tool futures abort on owner drop; an explicit transfer releases the raw handle to the background runner. Applied to streamed serial/parallel tasks and non-streamed parallel tasks. | Execute cancellation/transfer regressions and exercise real turn cancellation while tools run. |

## Boundaries that remain

- These are focused changes, not a complete TUI redesign or an AGI claim.
- Text width remains subject to terminal/font behavior. A grapheme wider than a
  memory tile's wrap budget stays atomic for the renderer to clip; zero-width
  wrapping keeps one source chunk rather than inventing a display fragment.
  Cross-style Unicode normalization has not been profiled in the live TUI.
- Image dimension lookup can still decode a full base64 container for unusually
  long headers; it does not decompress raster pixels in that fallback. Worker
  restart and per-job panic isolation do not guarantee recovery from poisoned
  third-party caches, allocation aborts, or every terminal-protocol failure.
- Aborting an owned Tokio task cancels its async future. It does not establish
  that every child process, blocking operation, or remote side effect has stopped;
  those require tool-specific cleanup and reconciliation. Parallel handoff changes
  have not been exercised end to end in the running terminal.
- A source search currently finds `MainBrain::new` calls only in that module's
  tests. Coordinator persistence/assignment improvements are subsystem work;
  they do not establish that the daemon or live swarm uses this coordinator.
  Production integration must follow actual server/agent entrypoints rather
  than treating the presence of a coordinator module as runtime wiring.
- Goal evidence is a point-in-time observation. It does not automatically expire
  after unrelated file changes, and a passing command does not establish test
  adequacy. See [goal evidence](goal-evidence.md) for configuration and migration.
- A serializable criterion ledger is not yet a durable execution journal wired
  through every session persistence and recovery path.
- Coordinator persistence restores phase plans, recorded worker specs, and accepted
  reports. Unfinished assignments reserve capacity but do not establish process
  liveness. There is no automatic redispatch after restart; process/task handles
  still need explicit reconciliation. The separate swarm DAG and external side
  effects are not restored by this state ledger.
- State transaction locking covers cooperating writers. Explicit `save_state`
  intentionally replaces a whole snapshot; external programs that ignore the
  sidecar protocol are not coordinated. Separate index/document writers and
  live worker execution still need their own concurrency audit.
- Report acceptance validates structure, assignment/run ownership, and replay
  consistency. Report prose is not proof that tests passed or work was correct;
  executable quality gates and the runtime evidence system remain necessary.
- Await observation history persists in the existing runtime-state location;
  it is not guaranteed to survive a reboot that clears that directory. Legacy
  records cannot reconstruct members that disappeared before observation
  history existed. The daemon's per-request lock does not coordinate a separate
  process writing the same await files. Delivery is not an exactly-once external
  notification journal, and a final-result write failure still needs a broader
  persistence/delivery audit.
- Older states without a typed plan deserialize with an empty plan; existing
  Markdown is not silently parsed into executable phase state.
- Checkpoint state restore does not restore repository files. Returned Git
  instructions remain separate from execution.

## Quality evidence required before a high rating

Use a pinned revision and task set. Record the model/provider, environment,
permissions, budgets, and exact acceptance criteria. Measure verified task success,
false completion, cost per verified task, input-to-flush median and tail latency,
recovery without lost or duplicate actions, and bounded resource growth. Compare
against the same model under a simpler harness. Add components only when measured
outcomes justify their complexity.

No numerical improvement or 10/10 rating has been demonstrated by this work.

Background adoption now acquires its registry lock before publishing running
status or spawning its monitor. An abort guard owns the incoming async task while
registration waits; cancellation at that point aborts it and publishes no phantom
background task. A regression case holds the registry lock, polls adoption, then
cancels it and observes task destruction. This case is unexecuted. Dropping an
adoption future before its first poll, child-process cleanup, and durable delivery
remain outside this guarantee.

The adopted task's abort guard now moves into its monitor and remains alive
through result handling. Cancelling that monitor therefore also requests abort
of its underlying async tool future, including when the monitor has not yet been
polled. A regression case observes the tool future's drop after user cancellation
(unexecuted). Adopted tasks retain their display name in the live registry, so
cancellation no longer erases it. Monitor termination does not await underlying
future destruction; the existing reload wait is not proof of child cleanup.

Background waits no longer fabricate a Completed outcome from a progress event
when status storage is unavailable. The fallback retains Running, session and
display name, and leaves terminal fields unset even at 100% reported progress.
Completion-event waits now use the terminal event if disk still says Running.
Added a progress-fallback regression assertion; no checks were run. These changes
do not make missing storage durable or reconstruct missed terminal events.

Fresh background spawns now acquire registry ownership before status publication
and task launch, matching the adoption registration invariant. There is no await
between launch and registry insertion. Added an unexecuted regression case that
cancels registration under lock contention and asserts no execution, live entry,
or status file. The registry is still held during the existing synchronous
initial status write; asynchronous storage and durable registration remain future
work. Source inspection also found that direct background execution panics can
bypass terminal publication; that path has not yet been repaired.

Direct background execution now catches unwinding panics from callback creation
and future polling, converts them to failed results, and proceeds through normal
terminal status/event publication and registry pruning. Panic-hook suppression is
scoped to each poll, never across an await. Added unexecuted regressions for a
panic after yielding and restoration of the thread-local suppression scope between
polls. Panics in finalization/destructors, aborting panics, storage failures, and
external side effects are not covered by this recovery boundary. No checks ran.

Direct background results are normalized before terminal publication and return:
nonzero exit codes, error-bearing Completed results, and returned Running results
become Failed. Superseded remains a separate terminal outcome. Added unexecuted
normalization cases and an integration case for nonzero exit persistence. This
is an execution-status invariant, not verification that a successful tool achieved
the user's objective. Checks remain skipped.

Background status publication now uses same-directory temporary files, synced
contents, and replacement instead of in-place truncating writes. All background
manager status paths use this shared helper; async paths offload persistence to
the blocking pool. Added unexecuted replacement and failed-scratch cleanup cases.
Atomic document publication does not serialize competing logical updates, fsync
the parent directory, or make cancellation of blocking writes transactional.
Persistence errors are logged; callers still do not receive a durable-commit
acknowledgement. No checks ran.

Late progress for an already-terminal task is now ignored. Shared status
publication also serializes its write-time terminal fence within this process:
a stale Running snapshot cannot replace Completed, Failed, or Superseded JSON.
Added an unexecuted regression covering all three terminal outcomes. This fence
is process-local, does not serialize full read/modify/write transactions, and
cannot prevent stale progress events/responses or lost delivery-flag updates.
It also does not arbitrate competing terminal outcomes. Checks remain skipped.
