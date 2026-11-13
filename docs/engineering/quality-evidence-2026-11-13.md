# Quality evidence: 2026-11-13

This record tracks the request to improve Alphacode across the categories used
in its comparison with Codex and Claude Code. A 10/10 score is not established
by feature counts, unit-test counts, or this document.

## Implemented improvements

- Goal verification ignores the shared top-level `intent` display label for
  native code tools while matching every execution argument exactly. Extension
  and MCP tool inputs remain exact, including `intent`. Failed relabeled reruns revoke old
  evidence. Nested `intent` fields remain execution data. Unit cases and the
  streamed-result recorder regression cover this behavior.
- SDK tool-output clipping uses the native head-and-tail implementation, keeping
  final diagnostics from large logs. Regression cases cover Unicode, bounded
  output, unchanged short output, and preservation of native metadata/images.
- Structured logging redacts password/passwd and cookie fields in addition to
  existing credential fields. Retry log URLs omit user information, query
  parameters, and fragments. These are targeted fixes, not a complete audit of
  every log sink or provider error body.
- CI now runs the GUI callback/diagnostic suite on Windows PowerShell 5.1, matching
  the desktop host's .NET Framework runtime. The suite covers callback recovery,
  fatal errors, privacy filters, failed log I/O, export, and rotation.
- Picker fixtures now distinguish unavailable routes from credentials expiring
  at selection, declare the remote server's actual route ownership, and isolate
  the preview catalog from persisted caches. The JPEG header-only regression uses
  allocation-limit rejection rather than assuming a tiny empty scan is rejected
  by the raster decoder.
- A task-evaluation runner copies fresh fixtures, invokes independent verifiers,
  checkpoints machine-readable outcomes, and enforces process timeouts. Its
  fourteen local tests pass, including descendant cleanup, literal command
  arguments, and rejection of premature zero-exit verifiers. Windows/Linux CI
  coverage is configured. The two bundled tasks
  are smoke fixtures, not a comparative coding benchmark. See
  [task evaluation](agent-task-evaluation.md).

## Local verification

Environment: Windows/MSVC, pinned Rust 1.94.1, default Cargo features. Build output
is outside OneDrive at `C:\Users\or0to\AppData\Local\Temp\alphacode-target`.

- Formatting and whitespace checks passed.
- `cargo clippy --lib -- -D warnings`, `cargo check --workspace`, and
  `cargo check --workspace --tests` passed, including the concluding reruns.
- Installer PATH tests: 46 passed, with no registry changes.
- GUI callback/diagnostic suite: passed under Windows PowerShell 5.1. Running it
  under PowerShell 7 fails because its .NET runtime differs from the shipped host.
- CI YAML parsed locally. Remote CI has not run on these changes.
- Initial library run with inherited `NO_COLOR=1`: 6,748 passed, 49 failed,
  15 ignored. The failures include color-dependent assertions and independently
  reproduced picker/JPEG fixture problems. This is a failed baseline, not a pass.
- A standalone probe linked to the installed `image` crate verified that the
  corrected JPEG fixture exposes its dimensions without raster allocation and
  that a full decode fails with an allocation-limit error.
- Direct compilation of the current goal-contract module passed all 9 tests.
  A second standalone harness combining the actual goal-contract source,
  evaluator policy source, and tool-name resolver passed all 16 tests. These
  probes do not include the `Agent` runtime or replace the full Cargo suite.
- With CI's color settings, the existing compiled style suite passed 140 tests,
  with 1 ignored. This narrows the color-related baseline failures; it is not a
  final-revision library-suite result.
- A full color-enabled run passed 6,790 tests, failed 8, and ignored 15. The
  remaining failures identified missing route ownership in mock remote catalogs
  and stale render expectations. Their assertions/fixtures have been corrected;
  the targeted reruns now pass: 158 picker tests and 39 tool-render tests.
  The full rerun passed: **6,798 passed, 0 failed, 15 ignored**.
- Local-model integration: 8 passed. Alphagrep: 45 unit and 6 integration tests
  passed. Evaluation runner: 14 passed.
- Live smoke evaluation: 2/2 tasks passed independent verification and completion
  receipts using `1.1.4-dev (20c9c833, dirty)`, a debug build, an isolated app home,
  and the Alphax Free automatic route. Agent times were 49.55 and 42.89 seconds;
  each verifier took about 0.15 seconds. Per-task timeout was 180 seconds. This
  is a two-task smoke result, not a general or comparative coding benchmark.
- Ignored tests and live terminal latency scenarios are not established by these
  results. Remote release CI is a separate, exact-revision publication gate.

## Evidence required for a stronger product rating

| Category | Required evidence beyond these fixes |
| --- | --- |
| Coding reliability | Matched bug-fix, feature, and refactoring tasks with independently checked outcomes; record failures and human interventions. |
| Provider flexibility | Exercise configured providers and local models, including malformed calls, interrupted streams, cooldowns, and fallback. |
| Responsiveness | Windows Terminal reader-to-frame-flush latency while idle, after model selection, and during streaming; report median and tail latency. |
| Browser and desktop workflows | Reproducible end-to-end tasks using the actual supported extension/host versions, including failure recovery. |
| Safety and privacy | Permission-boundary, cancellation, persistence, and redaction tests across actual entrypoints; dependency and release review. |
| Developer experience | Green checks on supported operating systems, clean install/update/uninstall exercises, and documentation checked against shipped behavior. |
| Competitive ranking | Same tasks, hardware, budgets, tool versions, and comparable model access; publish the method and raw outcomes. |

No comparative completion-rate, cost, memory, startup, or live-terminal latency
claim follows from the local checks above. Release publication is tracked by
GitHub Actions; these local checks alone do not establish publication.
