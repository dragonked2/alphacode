# Security Policy

Alphacode is an AI coding agent that runs arbitrary code, including shell
commands proposed by a language model. While the agent has built-in safety
gates for destructive commands (see `bash_destructive_gate.rs`), it is still a
powerful tool. Please use it responsibly and report vulnerabilities
responsibly.

## Supported versions

Only the latest release receives security fixes. Please upgrade before filing
a report.

## Reporting a vulnerability

**Please do not file a public GitHub issue for security bugs.**

Email: **tlbbeg313@gmail.com** with the subject prefix `[SECURITY]`.

Include:

1. A clear description of the vulnerability and its impact.
2. Reproduction steps (a transcript, a PoC repo, or a payload).
3. Your `alphacode --version` and platform info.
4. Whether you intend public disclosure and on what timeline.

You should receive an acknowledgement within 72 hours. We aim to ship a fix
within 14 days for high-severity issues.

## Out-of-scope reports

We are interested in real vulnerabilities. Please do **not** report:

- The AI model itself producing bad code (use your provider's safety channels).
- Hypothetical jailbreaks against the underlying model.
- "The agent could be tricked into running X" without a concrete repro.

## Built-in safety features

Alphacode ships with several layers designed to limit blast radius:

- **Destructive-command gate** (`bash_destructive_gate.rs`): refuses
  catastrophic targets (`/`, `$HOME`, credential stores, device nodes) and
  asks for human justification on ambiguous commands.
- **Risk assessment** (`alphacode_command_risk`): tokenizes shell commands to
  classify risk before they execute.
- **Permission prompts**: the TUI surfaces every risky action for confirmation.
- **Network URL safety**: outbound HTTP fetches are classified against SSRF
  and credential-leak heuristics.
- **Session crash recovery**: a panic or signal labels the session as
  `Crashed` and prints a resume command so the user never loses work.

If you find a way to bypass any of these, please report it.