# Comprehensive improvement backlog

This is the execution inventory for `docs/comprehensive-improvement-plan.md`.
Metrics in that plan are targets, not measured results. Do not claim a phase is
complete until its stated gate has been exercised.

| ID | Priority | Work | Evidence needed |
| --- | --- | --- | --- |
| A1 | High | Audit detached tasks and background threads for ownership, panic handling, and surfaced failure. Initial scan found 199 `tokio::spawn` and 77 thread-spawn references in `src` (includes tests/comments). Startup helper threads now log spawn failure and panic. | Inventory actual production spawn sites; fault injection for each owner. |
| A2 | High | Define and test the remote reconnect state machine, including attempt count and last error. | Disconnect/fault test showing reconnection within the stated 30-second target. |
| A3 | High | Make message replay and tool side effects idempotent. | Crash/replay tests with no duplicated side effect. |
| A4 | High | Verify provider retry, timeout, fallback, and model capability routing. | Recorded provider failures and recovery tests. |
| B1 | Medium | Measure cold and warm startup before changing more initialization paths. | p50/p95 startup and first-use timings on a named host/build. |
| B2 | Medium | Audit read-only tool concurrency and write serialization. | Throughput and cancellation tests with bounded concurrency. |
| B3 | Medium | Profile long sessions for context growth and memory retention. | Bounded-RSS soak and token-budget traces. |
| B4 | Medium | Profile rendering on large transcripts and resize. | Frame times and no layout shear at narrow/wide terminal sizes. |
| C1 | High | Require a concrete verification record before success messages. | Agent completion tests covering failed and skipped verification. |
| C2 | Medium | Standardize tool errors as cause, next step, and retry safety. | Common-error snapshots and redaction checks. |
| C3 | Medium | Inventory skill and MCP permissions, manifests, and evaluations. | One manifest and eval per shipped skill. |
| D1-D5 | High | Continue layout, keyboard, error, onboarding, and accessibility review. | TUI screenshots/snapshots at 40, 80, 120, and 200 columns; keyboard flows. |
| E | High | Use shared brand art across surfaces. CLI `run` banner, width tests, and tier docs are added. | Full TUI/CLI render check, including ASCII fallback and machine-readable output. |
| F1-F4 | High | Audit secret handling, tool paths, auth boundaries, and dependency supply chain. | Redaction/path tests, dependency audit, SBOM/signature procedure. |
| G1-G3 | High | Restore green full tests and CI quality gates. | `fmt`, `check`, `test`, `clippy`, and release perf gates on supported platforms. |
| H | Medium | Keep setup, troubleshooting, and release notes aligned with behavior. | Doc review against actual CLI output. |

The first library run on Windows reported 6,266 passed, six failed, and 12
ignored. The six failures have targeted fixes in progress; this is not yet a
green release gate. `cargo audit` is not installed on this machine.
