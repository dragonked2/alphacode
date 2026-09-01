# Changelog

All notable changes to Alphacode are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/).

## [1.0.2] — 2026-09-01

Patch release. Polishes the TUI / markdown rendering and the bash
destructive-command gate. No user-facing config or protocol changes.

### Changed

- **Splash screen copy**: "AI-Powered Coding Assistant" → "Your AI
  coding companion"; "Ready when you are" → "Type to get started";
  the ready-state pill now reads "is ready ✨". The idle wordmark
  gained the same ✨ glyph and the hint text is slightly longer.
- **Splash feature chips**: replaced `doctor / vuln / tokio` with
  `memory / multi / open` to match the framing in the README.
- **Markdown palette**: code blocks, math, links, headings, and dim
  text are re-tuned for higher contrast on dark backgrounds. Code
  separators, blockquote gutters, and `┌─ math` headers are now
  blue-tinted instead of generic dim grey.
- **Markdown tables**: separators and cell dividers use a dedicated
  blue-grey tone (was sharing `table_color`); headers are underlined
  bold for clearer scan.
- **Bash destructive gate**: log lines for denied / confirmation
  commands are summarized at 512 chars to keep long pipelines from
  flooding the log; the JSON schema is reformatted without changing
  any field.

### Fixed

- Removed two pieces of dead code that produced `#[warn(dead_code)]`
  on non-test builds: the unused `MATH_INLINE_FOREGROUND` constant
  and the unused `table_color()` helper in the markdown renderer.
- `bash_destructive_gate.rs` no longer ends without a trailing
  newline.

## [1.0.1] — 2026-09-01

Patch release. Ships the new coding-quality contract and the
repo-polish pass without disturbing the existing v1.0.0 install base.

### Coding-quality contract

The agent's prompt is hardened with four guardrails on every
code-changing turn:

1. **Smallest Change** — never bundle unrelated edits; report deeper
   issues separately instead of fixing them silently.
2. **Anti-Regression** — every change must leave previously-passing
   tests in a passing state; new warnings are treated as failures.
3. **Self-Critique Loop** — a 5-point checklist runs before any task
   is reported complete (objective covered, evidence-backed, no
   regressions, scoped diff, edge cases considered).
4. **Structured Output After Tool Calls** — every state-changing turn
   ends with *What changed / What was verified / What remains*.

The tool descriptions for `edit`, `write`, `multiedit`, and `bash`
were tightened in the same change so the prompts the user sees
reinforce the prompt the agent runs on. The system prompt also gains
a *Tool-Use Best Practices* table and three worked examples
(good-vs-bad shape) for the most common coding-task patterns.

Source of truth:
[`src/alphacode_base/prompt/system_prompt.md`](src/alphacode_base/prompt/system_prompt.md).

### Added

- Onboarding trust line (`quality_guarantees_line`) shown on the
  Suggestions phase so first-run users see the contract they can
  hold Alphacode to.
- `rust-toolchain.toml` pinning Rust 1.91 (matches the CI matrix).
- `docs/configuration.md`, `docs/architecture.md`, `CODE_OF_CONDUCT.md`.
- `src/alphacode_app_core/session_watchdog.rs` — Session Health
  Watchdog for stall detection, memory-leak mitigation, connection
  refresh, and crash recovery.
- `src/cli/args/tests.rs` — CLI arg parsing tests.

### Changed

- README: new tagline; honest Performance section (no marketing
  benchmarks); new **Coding-quality contract** section.
- CONTRIBUTING: fixed broken `IMPROVEMENTS.md` reference; updated
  module table to match the actual 90+ crates; links to
  `docs/architecture.md`.
- Cargo.toml: package description aligned with the new tone.
- `.gitignore`: ignores `scripts/.[a-zA-Z0-9_-]*.ps1` (local
  hardcoded debug helpers).

### Fixed

- `autonomous/mod.rs`: stale test assertion (8/3/4 → 16/5/12) so
  `cargo test` matches the current `AgentLimits::default`.

## [1.0.0] — 2026-09-01

The first stable release. Ready for daily driving. The binaries
tagged `v1.0.0` contain the original v1.0.0 code; users on
`v1.0.0` who want the coding-quality contract should upgrade to
`v1.0.1` (the install script does this automatically).

### Highlights

- **40+ models** out of the box: Anthropic Claude, OpenAI GPT, Google
  Gemini, AWS Bedrock, Azure, OpenRouter, GitHub Copilot, Cursor,
  Antigravity, and any OpenAI-compatible endpoint.
- **Swarm mode** with parallel sub-agents, task DAG, deep / light modes.
- **Rich TUI** built on ratatui: animations, syntax highlighting, image
  previews, Mermaid diagrams, multi-pane transcripts.
- **40+ tools** including file editing, shell, search, browser automation,
  memory, skills, scheduled tasks, and the autonomous module set.
- **Built-in safety**: destructive-command gate, risk classification,
  permission prompts before risky runs, SSRF + credential-leak heuristics.
- **Crash-safe sessions**: every message is persisted synchronously, so
  killing Alphacode never corrupts a session.
- **Coding-quality contract**: the system prompt enforces *smallest change*,
  *anti-regression*, *self-critique*, and *structured output* across every
  turn. See `src/alphacode_base/prompt/system_prompt.md`.

### Coding-quality contract (new in 1.0)

The agent's prompt has been hardened with four guardrails that apply to
every code-changing turn:

1. **Smallest Change** — never bundle unrelated edits; report deeper
   issues separately instead of fixing them silently.
2. **Anti-Regression** — every change must leave previously-passing tests
   in a passing state; new warnings are treated as failures.
3. **Self-Critique Loop** — a 5-point checklist runs before any task is
   reported complete (objective covered, evidence-backed, no regressions,
   scoped diff, edge cases considered).
4. **Structured Output After Tool Calls** — every state-changing turn ends
   with *What changed / What was verified / What remains*.

The tool descriptions for `edit`, `write`, `multiedit`, and `bash` were
tightened in the same change so the prompts the user sees reinforce the
prompt the agent runs on.

### Added

- Swarm task-DAG (`deep` and `light` modes) with per-task gates.
- Session Health Watchdog that detects and auto-recovers from stale states.
- New install scripts (curl-bash on Unix, `iwr | iex` on Windows) with
  checksum verification.
- Onboarding wizard with telemetry choice, model defaults, and key-binding
  cheat-sheet. Every guided phase has `Esc to skip`.
- `quality_guarantees_line` in the welcome screen, telegraphing the
  smallest-change / verified-not-assumed / no-silent-regressions contract
  to first-run users.
- Per-project `AGENTS.md` injected into the system prompt as
  `## Project Agents`.
- Session resume (`alphacode --resume`) with crash-state recovery.

### Changed

- MSRV bumped to **1.91** (mdwright-latex 0.1.3 needs 1.91). CI tests
  against 1.88, 1.91, and stable.
- Release workflow uses locked `cargo` profile and matches the asset names
  the install scripts expect (`alphacode-{version}-{target}.{ext}`).
- The default HTTP client pool was tightened from 4 → 2 idle connections
  per host to protect free-tier provider rate-limit budgets.

### Fixed

- Malformed pwsh step in the `installer-smoke` CI job.
- Invalid `args` input in `release.yml` replaced with `locked+profile`.
- Release asset names now match what the install scripts download.

### Security

- The `bash` tool refuses `rm -rf /`, `$HOME` wipes, device-node writes,
  and similar catastrophic targets.
- Ambiguous destructive commands (e.g. `find / -delete`) surface a
  justification prompt before the model retries.
- All risky actions go through the TUI permission prompt.
- Network fetches pass through SSRF and credential-leak heuristics.

---

## Pre-1.0 history

The `Initial commit: Alphacode v1.0.0` snapshot established the workspace
shape: 90+ small `alphacode_*` crates, a single binary, ratatui-based TUI,
the autonomous module set, and the swarm task-DAG. Everything after that
snapshot is captured above.