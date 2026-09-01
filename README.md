# Alphacode

> **Possibly the greatest coding agent ever built.**
> Blazing-fast terminal UI, multi-model orchestration, intelligent swarm coordination, and 40+ tools working together.

<p align="center">
  <a href="#install">Install</a> · <a href="#quick-start">Quick start</a> · <a href="#what-can-it-do">Features</a> · <a href="#commands">Commands</a> · <a href="docs/">Docs</a> · <a href="CONTRIBUTING.md">Contributing</a>
</p>

<p align="center">
  <strong>Status:</strong> v1.0.0 — stable, ready for daily driving.<br>
  <strong>License:</strong> MIT · <strong>Platforms:</strong> Linux · macOS · Windows
</p>

---

## What is Alphacode?

Alphacode is an **AI coding agent that lives in your terminal**. You describe what
you want — in plain English — and it plans, edits files, runs commands, runs
tests, opens pull requests, and stays running until the task is done.

- 🧠 **40+ models out of the box** — Anthropic Claude, OpenAI GPT, Google
  Gemini, AWS Bedrock, Azure, OpenRouter, GitHub Copilot, Cursor, Antigravity,
  plus any OpenAI-compatible endpoint.
- 🐝 **Swarm mode** — spin up parallel sub-agents that each take a slice of
  the work and merge their results.
- 🖥️ **Rich terminal UI** — animations, syntax highlighting, image previews,
  Mermaid diagrams, multi-pane transcripts.
- 🛠️ **40+ tools** — file editing, shell, search, web fetch, browser automation,
  memory, skills, scheduled tasks, autonomous modules, and more.
- 🛡️ **Built-in safety** — destructive-command gate, risk classification, and
  permission prompts before anything risky runs.
- 🧵 **Resilient** — sessions crash-safely, every conversation is resumable,
  every change is revertable.

---

## Install

You don't need to install anything to *use* Alphacode — pick the one-liner for
your platform and run it in any terminal.

### macOS · Linux

```sh
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
```

That's it. The script downloads the latest release, verifies its checksum,
drops the `alphacode` binary in `~/.local/bin`, and prints PATH instructions if
you don't already have it.

### Windows (PowerShell)

```powershell
iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
```

It installs to `%LOCALAPPDATA%\Programs\alphacode\bin\` and (if you want) adds
it to your user PATH.

### From source (Rust developers)

```sh
git clone https://github.com/dragonked2/alphacode.git
cd alphacode
cargo build --release
./target/release/alphacode --version
```

Requirements: **Rust 1.91+** (edition 2024) and a C toolchain matching your
platform. The default build skips heavy optional stacks (Bedrock, embeddings,
PDF, Mermaid) so cold builds stay fast. Opt in when you need them:

```sh
cargo build --release --features bedrock,embeddings,pdf,renderer
```

### Verify the install

```sh
alphacode --version       # print version
alphacode doctor          # check providers, terminal, paths
alphacode login           # connect a model
alphacode                 # launch the TUI
```

---

## Quick start

You don't need an account to look around, but you need at least one model
configured before Alphacode can do anything useful.

```sh
# 1. Run the doctor — it tells you what's missing.
alphacode doctor

# 2. Connect a model. Pick whichever is easiest:
alphacode login                       # interactive picker (Claude, ChatGPT, …)
alphacode login --provider openai     # specific provider
ALPHACODE_OPENAI_API_KEY=sk-... alphacode   # env-var auth, no command needed

# 3. Launch the TUI.
alphacode
```

First time you launch, you'll see a friendly onboarding wizard:
telemetry choice, model defaults, key bindings cheat-sheet. Every step has an
**Esc to skip** so you can never get trapped.

A few things to try once you're in:

| Action | How |
| --- | --- |
| Refactor this repo | `> refactor src/cli/startup.rs to use a builder pattern` |
| Explain a file | `> walk me through src/alphacode_swarm_core` |
| Add tests | `> add unit tests for the new helper` |
| Search the web | `> look up the latest ratatui release notes` |
| Run multiple agents | `> /swarm "split this into 4 parallel tasks"` |
| Resume a session | `> alphacode --resume` (or `--resume <id>`) |

Press **F1** in the TUI for the full keymap, **Ctrl+T** to toggle the model
picker, **Ctrl+Y** for the agent panel, **Ctrl+C** to interrupt the current
turn (your session is preserved).

---

## What can it do?

### Models & providers

| Provider | Auth | Notes |
| --- | --- | --- |
| Anthropic (Claude) | OAuth or API key | First-class; default for most users |
| OpenAI (GPT-4, o1, o3, …) | OAuth, API key, ChatGPT browser | Reasoning + vision + tools |
| Gemini | OAuth or API key | Google's lineup |
| GitHub Copilot | OAuth | Uses your existing subscription |
| Cursor | OAuth | Reuses Cursor's session |
| Bedrock | AWS creds | `--features bedrock` |
| Azure | Azure AD / API key | `--features azure-auth` |
| OpenRouter | API key | Aggregator, free tier |
| Any OpenAI-compatible | API key | `alphacode provider add` |
| GMI Cloud | built-in default key | Works out of the box |

### Tools

Editing (read, write, patch, multi-edit), search (regex, fuzzy, AST), shell
(risky commands gated), web (fetch, search), browser (automation via Chrome
DevTools Protocol), memory, skills, scheduled/ambient tasks, sessions,
checkpoints, image rendering, Mermaid diagrams, PDF text extraction, OAuth
flows, and the autonomous modules (planner, project_analyzer, self-review,
quality_gate, resource_monitor, etc.).

### Swarm

Drop `/swarm <goal>` and the orchestrator splits the goal into a DAG of
sub-tasks, dispatches them across parallel agents, monitors each branch, and
merges the results. The whole plan is visualised in the TUI.

### Safety

- The `bash` tool refuses `rm -rf /`, `$HOME` wipes, device-node writes, and
  similar catastrophic targets (`src/alphacode_app_core/tool/bash_destructive_gate.rs`).
- Ambiguous commands (e.g. `find / -delete`) are held back and surface a
  justification prompt before the model retries.
- Every risky action goes through the TUI permission prompt.
- Network fetches go through SSRF + credential-leak heuristics.
- Session crash-recovery labels a crashed session and prints a resume command.

### Resilience

- `alphacode --resume` reopens any past conversation. The session list is
  searchable.
- Panics, signals, and dropped SSH connections all mark the session as
  `Crashed` instead of corrupting it.
- A month-of-uptime health monitor records RSS, slow ops, error rates, and
  per-subsystem liveness in `health.json` for monitoring.

---

## Commands

```
alphacode                          # launch the TUI (default)
alphacode login                    # authenticate a provider
alphacode doctor                   # check providers, terminal, paths
alphacode provider list            # list configured providers
alphacode provider add <name>      # add a custom OpenAI-compatible endpoint
alphacode provider current         # print the active provider + model
alphacode model list               # list models on the current provider
alphacode run "fix the failing test"   # one-shot, non-interactive
alphacode repl                      # simple REPL (no TUI)
alphacode sessions list             # list past sessions
alphacode --resume                  # search-and-resume picker
alphacode --resume <id>             # resume a specific session
alphacode update                    # self-update
alphacode --version                 # print version
alphacode --help                    # full help

# Inside the TUI:
/help       # full command list
/agents     # spawn parallel sub-agents
/compact    # condense long context
/memory     # browse long-term memory
/skills     # browse and edit skills
/diff       # open the change viewer
/exit       # leave the TUI (session is preserved)
```

---

## Configuration

Alphacode stores config and sessions under platform-standard locations:

| OS | Config | Sessions | Logs |
| --- | --- | --- | --- |
| Linux | `~/.config/alphacode/` | `~/.local/share/alphacode/sessions/` | `~/.local/share/alphacode/logs/` |
| macOS | `~/Library/Application Support/alphacode/` | same | same |
| Windows | `%APPDATA%\alphacode\` | `%LOCALAPPDATA%\alphacode\sessions\` | `%LOCALAPPDATA%\alphacode\logs\` |

Everything else is documented in `docs/configuration.md`.

---

## Performance

The release build is tuned for speed:

- `opt-level = 3`, `lto = "thin"`, `codegen-units = 16` keeps cold build time
  fast while keeping hot runtime paths fast.
- Hot third-party crates (reqwest, hyper, tokio, rustls, ratatui, crossterm)
  compile as a single codegen unit so cross-crate inlining + LTO can fold them
  tightly into the binary.
- The HTTP client pool uses 2 idle connections per host — enough for
  sequential chat completions plus one parallel background catalog refresh,
  but it stops free-tier providers from burning rate-limit budget.
- `cargo test --lib` is the only required test lane; full integration tests
  are opt-in to keep CI fast.

---

## Uninstall

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.sh | bash
# also remove config + sessions:
curl -fsSL ... | bash -s -- --purge    # via env: ALPHACODE_PURGE=1

# Windows
iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.ps1 | iex
iwr -useb ... | iex -Purge
```

---

## Contributing

See `CONTRIBUTING.md` for the workflow and `SECURITY.md` for how to report
vulnerabilities. Pull requests welcome — the codebase is large but the module
boundaries are intentional; new providers, new tools, and new TUI widgets
each fit in well-isolated slots.

---

## Acknowledgements

Built on top of outstanding open-source work: [ratatui], [crossterm],
[tokio], [reqwest], [rustls], [clap], [pulldown-cmark], [syntect],
[resvg], and dozens more. See `Cargo.lock` for the full list.

[ratatui]: https://ratatui.rs
[crossterm]: https://github.com/crossterm-rs/crossterm
[tokio]: https://tokio.rs
[reqwest]: https://github.com/seanmonstar/reqwest
[rustls]: https://github.com/rustls/rustls

---

<p align="center">
  <sub>Made with care by <a href="https://github.com/dragonked2">Ali Essam</a> · MIT licensed</sub>
</p>