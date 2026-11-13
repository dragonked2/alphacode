# Quality evidence: 2026-11-13

This record tracks the request to improve Alphacode across the categories used
in its comparison with Codex and Claude Code. A 10/10 score is not established
by feature counts, unit-test counts, or this document.

## Implemented improvements

- Goal verification ignores the shared top-level `intent` display label while
  matching every execution argument exactly. Failed relabeled reruns revoke old
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
  at selection, use a model accepted by the Copilot classifier, and isolate the
  preview catalog from persisted caches. The JPEG header-only regression uses
  allocation-limit rejection rather than assuming a tiny empty scan is rejected
  by the raster decoder.

## Local verification

Environment: Windows/MSVC, pinned Rust 1.94.1, default Cargo features. Build output
is outside OneDrive at `C:\Users\or0to\AppData\Local\Temp\alphacode-target`.

- Formatting and whitespace checks passed.
- `cargo check --workspace --tests` passed before the final logging additions;
  final-revision checks are still pending.
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
- Full library tests and final check/lint validation are in progress.

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
claim follows from the local checks above. No release or deployment was made.
