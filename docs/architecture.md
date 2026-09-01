# Architecture

A high-level map of the codebase so a new contributor can find their way
around quickly.

## Workspace shape

The crate is a single Cargo workspace with one binary (`alphacode`) and many
library crates. The library crates live in `src/`. Each one is small and
single-purpose; the binary just wires them together.

```
src/
├── alphacode_core           # provider-agnostic trait types and shared state
├── alphacode_app_core       # the agent loop, prompts, tools, autonomy, server
│   ├── agent/               # turn execution, streaming, compaction, recovery
│   ├── autonomous/          # Main Brain, plan/phase, quality gate, checkpoints
│   ├── ambient/             # background directives (cron, schedules, watchdog)
│   ├── memory_manager/      # STATE.json, GOALS.md, PLAN.md, project index
│   ├── server/              # client/session protocol, swarm task DAG
│   └── tool/                # 40+ tools: edit, bash, grep, web, browser, …
├── alphacode_base           # the system prompt, prompt builder, capability enum
├── alphacode_tui            # the terminal UI (ratatui-based)
│   └── tui/                 # widgets, render passes, input handling, onboarding
├── alphacode_modules        # orchestrator glue: startup, shutdown, wiring
├── alphacode_cli            # the `alphacode` CLI entrypoint and arg parsing
├── alphacode_harness_api    # the harness API the agent exposes to itself
├── alphacode_auth_*         # per-provider OAuth flows (OpenAI, Anthropic, …)
├── alphacode_logging        # structured logging
├── alphacode_pdf            # PDF text extraction
├── alphacode_plan           # the master execution plan persisted to PLAN.md
└── alphacode_* (many more)  # small, focused utility crates
```

The naming convention is intentional: every crate is `alphacode_*`. A new
contributor can find what they're looking for by file path without learning
the layout first.

## The agent loop

A single turn of the agent lives in
[`src/alphacode_app_core/agent/`](../src/alphacode_app_core/agent/mod.rs):

```
                ┌──────────────────────────────┐
                │  build_system_prompt_split  │  static + dynamic parts, cached
                └──────────────┬───────────────┘
                               │
                ┌──────────────▼───────────────┐
                │   Provider::stream_chat      │  any model: Claude, GPT, …
                └──────────────┬───────────────┘
                               │
            tool_call ◄────────┴────────► text / reasoning blocks
                │
                ▼
   ┌────────────────────────────┐
   │  Tool::execute(input, ctx) │  edit, bash, grep, webfetch, …
   └──────────────┬─────────────┘
                  │ ToolOutput
                  ▼
   ┌────────────────────────────┐
   │  cap_tool_output_for_…      │  head/tail truncation, prompt-cache-safe
   └──────────────┬─────────────┘
                  │
                  ▼
   back into the conversation as a tool_result message
```

Loop invariants:
- The static part of the system prompt is **stable across turns** so the
  provider's prompt cache hits.
- Every tool call carries an `intent` parameter — the UI uses it as a
  short label so the user sees *why* the agent did each thing.
- Every tool call is **persisted before it runs**, so killing the process
  never loses a step.

## The system prompt

The prompt that drives the agent is plain Markdown, kept at
[`src/alphacode_base/prompt/system_prompt.md`](../src/alphacode_base/prompt/system_prompt.md).
It is `include_str!`-ed into the binary at compile time so there is no
runtime fetch, no template engine, and no place for drift.

The prompt is split into two halves for cache-friendliness:

| Part | Lives in | Cacheable? |
| --- | --- | --- |
| Static | `system_prompt.md` (always present) | Yes — same across turns |
| Dynamic | per-turn memory, AGENTS.md, current task, system reminder | No |

## The autonomous layer

Long-horizon work goes through a separate orchestrator in
[`src/alphacode_app_core/autonomous/`](../src/alphacode_app_core/autonomous/mod.rs):

- **Main Brain** owns the plan, the phases, and the merge.
- **Child agents** are spawned as fresh sessions with their own context.
- Each child reports back via an `AgentReport` (with `files_modified`,
  `files_created`, `problems`, `confidence`, `potential_conflicts`).
- The Main Brain detects file conflicts (`detect_file_conflicts`) and
  refuses to advance a phase until the **quality gate** passes.
- Every phase completion creates a **checkpoint** so the project can roll
  back without losing state.

## The TUI

The terminal UI is built on [ratatui](https://ratatui.rs). The renderer is
event-driven and lazy: widgets compute their layout on every frame and only
redraw dirty regions. The session transcript is persisted to disk, not held
in memory, so a week-long session does not bloat the process.

The onboarding screen lives at
[`src/alphacode_tui/tui/ui_onboarding.rs`](../src/alphacode_tui/tui/ui_onboarding.rs).
Every guided phase has an `Esc to skip` escape so a first-run user is never
trapped.

## Tools

Tools implement a single trait
([`src/alphacode_tool_core/mod.rs`](../src/alphacode_tool_core/mod.rs)).
The trait is intentionally tiny (`name`, `description`, `parameters_schema`,
`execute`) so adding a new tool is mostly a matter of describing its input
schema. Every tool's input schema is rewritten at registration time to
inject the shared `intent` parameter — that is why every tool call carries
a human-readable label.

## Coding-quality contract

The system prompt encodes the contract a user can hold Alphacode to:

1. **Smallest change** — never bundle unrelated edits into the same call.
2. **Anti-regression** — every change must leave previously-passing tests
   in a passing state.
3. **Self-critique** — before reporting done, the agent runs a 5-point
   checklist (objective covered, evidence-backed, no regressions, scoped
   diff, edge cases considered).
4. **Structured output** — every state-changing turn ends with
   *What changed / What was verified / What remains*.

If you are tuning the agent's behavior, edit the prompt. If you are
adding capability, add a tool. If you are changing orchestration, touch
the autonomous layer. The boundaries are intentional.