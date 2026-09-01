# Configuration

Alphacode stores its config, sessions, logs, and other state under
platform-standard locations so it never litters your project directory.

## Locations

| OS | Config | Sessions | Logs |
| --- | --- | --- | --- |
| Linux | `~/.config/alphacode/` | `~/.local/share/alphacode/sessions/` | `~/.local/share/alphacode/logs/` |
| macOS | `~/Library/Application Support/alphacode/` | same | same |
| Windows | `%APPDATA%\alphacode\` | `%LOCALAPPDATA%\alphacode\sessions\` | `%LOCALAPPDATA%\alphacode\logs\` |

## Top-level config file

The main config lives at `config.toml` inside the config directory. It is
created the first time you run `alphacode login`. The schema is intentionally
flat and self-documenting:

```toml
# Default model used when a session does not override it.
default_model = "claude-api:claude-fable-5"

# Telemetry: "everything" | "no_content" | "nothing"
telemetry = "no_content"

# Maximum number of child agents the orchestrator may spawn.
max_child_agents = 16

# Reasoning effort: "none" | "low" | "medium" | "high" | "swarm" | "swarm-deep"
reasoning_effort = "low"

[providers.openai]
kind = "openai"
api_key = "env:ALPHACODE_OPENAI_API_KEY"

[providers.anthropic]
kind = "anthropic"
api_key = "env:ALPHACODE_ANTHROPIC_API_KEY"
```

`api_key = "env:..."` is the recommended form: the key is read from the named
environment variable at use time, never persisted to disk.

## Environment variables

| Variable | Purpose |
| --- | --- |
| `ALPHACODE_OPENAI_API_KEY` | OpenAI API key |
| `ALPHACODE_ANTHROPIC_API_KEY` | Anthropic API key |
| `ALPHACODE_GEMINI_API_KEY` | Google Gemini API key |
| `ALPHACODE_BEDROCK_*` | AWS Bedrock credentials (when `--features bedrock`) |
| `ALPHACODE_NO_TELEMETRY=1` | Force telemetry off regardless of config |
| `ALPHACODE_ROUNDED_PILLS=on\|off` | Override the rounded-pill glyph detector |
| `ALPHACODE_SCRATCH_DIR` | Where scratch / venv / worktree files go (defaults to a per-user dir) |
| `ALPHACODE_PURGE=1` | Used by the uninstall script to also remove config + sessions |

## Per-project overrides

Drop an `AGENTS.md` file in your project's working directory to add
project-specific guidance. Alphacode reads it on every session start and
injects the contents into the system prompt as a `## Project Agents` section.

Prepend `@/path/to/file.md` to reference additional files.

## Sessions are crash-safe

Every message, every tool call, every checkpoint is persisted to disk
synchronously as it happens. Killing Alphacode with `Ctrl+C`, dropping your
SSH connection, or a kernel panic all leave the session in a `Crashed`
state — never a corrupted one. Resume with `alphacode --resume`.

## Resetting

To start fresh without uninstalling:

```sh
# Linux / macOS
rm -rf ~/.config/alphacode ~/.local/share/alphacode

# Windows (PowerShell)
Remove-Item -Recurse -Force "$env:APPDATA\alphacode"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\alphacode"
```

Sessions removed this way are not recoverable.