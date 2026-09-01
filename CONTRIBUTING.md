# Contributing to Alphacode

Thanks for helping make Alphacode better! This guide keeps contributions fast,
predictable, and mergeable.

## Quick start

```sh
git clone https://github.com/dragonked2/alphacode.git
cd alphacode
cargo build --release
cargo test --lib
cargo clippy --lib -- -D warnings
```

Requirements: **Rust 1.91+** (edition 2024) and a C toolchain matching your platform.

## Project layout

All first-party code lives under `src/`. The crate is intentionally monolithic
(fewer workspace rebuilds) and is split into focused module trees:

| Module | Responsibility |
| --- | --- |
| `alphacode_base` | Sessions, config, providers, memory, protocol |
| `alphacode_app_core` | Agent runtime, tools, server/protocol layer |
| `alphacode_tui*` | Terminal UI: rendering, widgets, style, animations |
| `alphacode_provider_*` | Per-provider runtimes (Anthropic, OpenAI, Gemini, …) |
| `alphacode_swarm_core` | Multi-agent coordination |
| `alphacode_modules` | High-level autonomous modules (planner, main_brain, …) |
| `cli` | Command-line entrypoint + subcommand dispatch |

## Pull request checklist

- [ ] `cargo build --release` passes locally.
- [ ] `cargo test --lib` passes locally (add unit tests when behavior changes).
- [ ] `cargo clippy --lib -- -D warnings` is clean.
- [ ] Public APIs have rustdoc comments.
- [ ] No new dependencies unless justified in the PR description.
- [ ] User-visible changes are documented in `IMPROVEMENTS.md`.

## Style

- 4-space indent, max line width 100.
- Prefer explicit error handling over `unwrap()` outside tests.
- Doc-comments explain *why*, not *what*.
- Tests live next to the code they exercise (`#[cfg(test)] mod tests`).

## Reporting bugs

Please use the **Bug report** issue template and include:

1. `alphacode --version` output
2. The exact command you ran
3. Expected vs. actual behavior
4. The minimal reproduction (a snippet, a repo, or a transcript)
5. Your terminal: `echo $TERM`, `echo $TERM_PROGRAM`

## Suggesting features

Open a **Feature request** issue. Briefly describe the user problem, your
proposed solution, and any alternatives you considered. Large changes are best
discussed before code is written.

## Security

See `SECURITY.md` for how to report vulnerabilities. Please do **not** file
public issues for security bugs.

## License

By contributing, you agree that your contributions are licensed under the MIT
License (see `LICENSE`).