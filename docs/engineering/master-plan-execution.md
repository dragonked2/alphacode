# Master plan execution record

This document records work that is implemented in the repository and the checks
that support it. It intentionally separates measured results from targets.

## Implemented product work

- The TUI uses shared spacing, frame, palette, transition, and accessibility
  tokens across its major surfaces.
- Streaming output has incremental buffering, redraw coalescing, paced remote
  delivery, cancellation recovery, and bounded previews for long tool output.
- Provider selection has cached catalogs, explicit fallback routes, retry and
  failover classification, capability checks, and safe model display names.
- The Alphax Free route exposes one automatic selection while keeping internal
  rotation private to the routing layer.
- The agent runtime has bounded phase budgets, parallel read-only tool plans,
  verification gates, resumable background work, and transcript recovery.
- Command execution and filesystem paths pass through risk and workspace
  boundary checks; sensitive values are redacted from structured diagnostics.
- A Windows desktop GUI, bridge, host build, debug transcript/session helpers,
  bundled workflow skills, icons, and documentation are present in the working
  tree. See `docs/gui.md` for launch instructions and current limitations.

## Verification

An earlier execution record reports the following checks with the Windows
target directory outside OneDrive:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo check --workspace --tests
cargo clippy --lib -- -D warnings
cargo test --lib
```

That record reports passing compile and lint checks and timing-sensitive
library-test failures. Those results have not been independently reproduced
for the current changes; see the current audit findings below. No benchmark
comparison against Codex is claimed.

## Measurement policy

`tests/perf_gates.rs` and the release performance audit provide reproducible
micro-benchmarks for search, rendering, and scaling behavior. Startup, memory,
network time-to-first-token, and end-to-end provider latency require a fixed
machine and credentials, so they must be measured by the release operator and
reported with the workload and environment.

## Current audit findings

- The client frame reader formerly called `read_until` before applying its
  limit, allowing an unterminated frame to grow without bound. Its local buffer
  was also lost if a `select!` branch cancelled a partial read. The main and
  debug sockets now share a bounded, cancellation-safe reader and reject
  invalid UTF-8. Focused regression tests cover both cases.
- The desktop has a local fake-bridge smoke test, but that is not a full
  end-to-end verification of a real server, all actions, or accessibility.
- No new startup, memory, CPU, or end-to-end performance measurements have
  been established in this audit. Previous measurement targets are not results.

## Launch

```powershell
$env:CARGO_TARGET_DIR = "C:\Users\or0to\AppData\Local\Temp\alphacode-target"
cargo run --release
```
