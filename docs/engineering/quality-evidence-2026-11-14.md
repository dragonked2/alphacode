# Quality verification: 2026-11-14

## Failure investigation

The checkout starts at `92a4eb86d69480763046150478ce8679c8ee6ac3`, with
existing uncommitted security/reasoning work preserved. GitHub CI run
[43560755940](https://github.com/dragonked2/alphacode/actions/runs/43560755940)
and release run
[43560804641](https://github.com/dragonked2/alphacode/actions/runs/43560804641)
are successful on that committed revision. Those results do not cover the
current uncommitted changes.

The earlier CI scheduler failure asserted that a process-global startup flag
was unset even though parallel tests could initialize it. The starting revision
already contains its isolation fix.

The local review's library run inherited `NO_COLOR=1`: 6,839 passed, 38 failed,
18 ignored. Color and animation assertions assumed a color-enabled terminal.
Changing the host environment made targeted failures pass, but required callers
to know an undocumented fixture prerequisite.

## Changes

- Unit buffer fixtures now use a deterministic truecolor, motion-enabled terminal
  profile, independent of host accessibility preferences. Production builds still
  detect the actual environment and honor the existing accessibility settings.
- `tests/render_preferences.rs` exercises the production library in ten isolated
  child processes. It checks monochrome output, empty/nonempty `NO_COLOR`, the
  app-specific override, reduced-motion flags, truecolor, and 256-color fallback.
  Isolation also exercises the real startup caches without mutating environment
  variables while other tests are running. The ignored helper is executed by the
  parent test; it is not an unverified test scenario.
- CI runs those production preference checks on Windows, macOS, and Linux.
- `scripts/dev-check.ps1` provides the required Windows/MSVC setup and a single
  fail-fast entry point for formatting, compilation, library/workspace-member
  tests, integration tests, and warnings-as-errors linting. Build output stays
  under `%LOCALAPPDATA%\Temp\alphacode-target`, outside OneDrive. It preserves
  the caller's accessibility settings.

Run the Windows checks from the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-check.ps1
```

`-Gate fmt|check|test|integration|clippy` selects a subset. ExecutionPolicy Bypass
applies to that process only; it does not change the machine's execution policy.

## Verification

Checks use Windows/MSVC, Rust 1.94.1, default features, and the mandated temporary
target directory. The full library run retained the host's `NO_COLOR=1`.
The following checks have completed:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace` and `cargo check --workspace --tests`: passed.
- `cargo test --lib`: **6,877 passed, 0 failed, 18 ignored**, in 185.53 seconds.
  All 38 failures from the earlier review run are resolved under the same
  inherited color setting. Ignored live/optional tests remain unverified.
- Python tool tests: 38 passed.
- Windows PowerShell 5.1 GUI callback/diagnostic suite: passed.
- Installer PATH tests: 46 passed, without changing the registry.
- PowerShell script parsing, the new runner's formatting gate, and diff
  whitespace checks: passed.
- The new runner also returned a nonzero exit after a deliberately failed Cargo
  check in an isolated temporary directory without a manifest.
- Production render preferences: all ten child-process scenarios passed (the
  parent test passed; its helper is marked ignored for direct discovery).
- Local-model compatibility integration: 8 passed.
- Alphagrep: 45 unit tests and 6 integration tests passed.
- `cargo clippy --lib -- -D warnings` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
  The latter ran through the new PowerShell runner.
- The rebuilt CLI's `--version` and `--help` succeeded; it identifies itself as
  `v1.1.6-dev (92a4eb86, dirty)`.

All checks above apply to the local working tree. The new CI step has not run
remotely, and no commit, push, tag, or release publication was performed.

## Remaining evidence

Passing gates establishes specific tested behavior, not a perfect product score.
The outstanding product evidence remains representative coding-task evaluations,
live provider/browser/desktop recovery exercises, and Windows Terminal
console-reader-to-frame-flush median and tail latency while idle, after selecting
a model, and during streaming. Large central modules still need focused
refactoring backed by their behavior tests. No live latency, competitive ranking,
complete security audit, or 10/10 rating follows from this change.
