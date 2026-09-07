# Changelog

All notable changes to Alphacode are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/).

## [1.0.25] - 2026-09-07

Major feature release: UI/UX improvements, performance optimizations, stability hardening, accuracy gains, harness API v1.1, and three new skill suites (CTF, frontend-dev, backend-dev) with expanded bug bounty coverage.

### Added

- **CTF solving skill suite** (`/ctf`): 8 subskills covering web exploitation, cryptanalysis, binary exploitation (pwn), reverse engineering, digital forensics, miscellaneous challenges, CTF methodology, and essential toolkit (pwntools, Ghidra, Burp Suite, etc.).
- **Frontend development skill suite** (`/frontend-dev`): 6 subskills covering React 19+, Next.js 15+, Tailwind CSS 4+, component systems, performance optimization, and testing (Vitest, Playwright, Storybook).
- **Backend development skill suite** (`/backend-dev`): 7 subskills covering Node.js/Express, Python/FastAPI, Rust/Axum, database patterns, authentication (JWT, OAuth2, RBAC), and caching strategies.
- **Bug bounty hunt-cors subskill** (`/bugbounty hunt-cors`): CORS misconfiguration hunting — origin reflection, null origin, pre-flight abuse, trust exploitation.
- **Bug bounty hunt-ssrf-advanced subskill** (`/bugbounty hunt-ssrf-advanced`): Advanced SSRF — cloud metadata, DNS rebinding, protocol smuggling, SSRF-to-RCE chains.
- **Bug bounty hunt-race subskill** (`/bugbounty hunt-race`): Race condition hunting — TOCTOU, double-spend, parallel request techniques.
- **Bug bounty hunt-desync subskill** (`/bugbounty hunt-desync`): HTTP request smuggling — CL.TE, TE.CL, TE.TE, H2.CL, prefix injection.
- **Bug bounty llm-injection subskill** (`/bugbounty llm-injection`): LLM/AI prompt injection — direct/indirect injection, jailbreak, data exfiltration, tool abuse.
- **Error toast expand/collapse** (`ui_error_toast`): Error toasts now support expand/collapse — press Enter to expand a toast to see the full message and hint at wider width (100 cells). Collapsed toasts show "▸" indicator and "press Enter to expand" hint. Esc dismisses all visible toasts.
- **Harness API v1.1 events** (`alphacode_harness_api`): Added `ToolProgress` (batch execution progress), `MemoryWarning` (pre-compaction alert), and `HealthUpdate` (periodic resource metrics) events. Backward compatible.
- **Time-based session auto-save** (`turn_loops`): Sessions now auto-save every 60 seconds regardless of tool iteration count, preventing data loss during long-running single-tool operations.

### Changed

- **Streaming reveal rate increased** (`stream_buffer`): Base reveal rate 180 → 240 chars/sec, backlog gain 3.0 → 4.0, max reveal rate 960 → 1440 chars/sec. Text now appears 33% faster during streaming.
- **HTTP connection pool expanded** (`provider_core`): `pool_max_idle_per_host` increased from 2 to 4, improving connection reuse for parallel requests (catalog refresh, health checks).
- **Context compaction quality improved** (`compaction_core`): `RECENT_TURNS_TO_KEEP` increased from 15 to 20, `EMERGENCY_TOOL_RESULT_MAX_CHARS` increased from 8000 to 10000. More context preserved during compaction reduces information loss on multi-step tasks.

### Fixed

- **install.ps1 version verification fails with empty warning** (`scripts/install.ps1`): The installer suppressed stderr with `2>$null`, discarding the actual error when the binary failed to run (e.g., Windows Defender/SmartScreen block). Replaced with `Start-Process` + `RedirectStandardError` to capture and display the real error. Added 500ms delay for Windows SmartScreen unsealing. Warnings now show actionable diagnostic text instead of a bare colon.

## [1.0.24] - 2026-09-07

Patch release. Adds automatic GitHub star feature and bumps version.

### Added

- **Automatic GitHub star on first run** (`alphacode_github_star`): when a user has the `gh` CLI installed and authenticated, alphacode now silently stars the repository (`dragonked2/alphacode`) in a background thread on first launch. The star attempt is recorded in `~/.alphacode/.github_starred` so it only runs once. This helps grow the open-source community around the project with zero user friction.

## [1.0.23] - 2026-09-06

Patch release. Fixes three install-script failures on Windows and Linux.

### Fixed

- **`alphacode` (and all subcommands) fail with `unrecognized subcommand '<exe path>'` on Windows** (`cli::startup::maybe_reexec_to_fresher_binary`): when a newer binary was found during startup, the re-exec code passed `std::env::args()` (which includes `argv[0]`, the program path) to `ProcessCommand::args()`. The new process then saw its own executable path as an extra positional argument, which clap interpreted as an unrecognized subcommand. This broke bare `alphacode` invocation and every subcommand on Windows. Fixed by using `args().skip(1)` to exclude `argv[0]`.

- **install.ps1 / install.sh version verification fails** (`scripts/install.ps1`, `scripts/install.sh`): both installers called `alphacode version` (a subcommand) to verify the binary works. This triggered the same re-exec bug above. Switched to `--version` (a clap flag handled before any application logic), which always succeeds.

- **uninstall.ps1 aborts with `Cannot overwrite variable HOME`** (`scripts/uninstall.ps1`): the script assigned `$Home = $env:USERPROFILE`, but PowerShell variable names are case-insensitive and `$HOME` is a read-only automatic variable. Renamed to `$UserProfile` and added defensive error handling throughout.

## [1.0.20] - 2026-09-06

Patch release. Removes the puzzle-game stego stubs that were accidentally committed in v1.0.19, deletes `test_update.zip` (no longer needed), and applies `cargo fmt` to four provider files so CI stops failing on `cargo fmt --check`.

### Removed

- **111 puzzle-game stego artifacts accidentally shipped in v1.0.19**: 46 Python scripts (`creative_search.py`, `bitplane_lsb.py`, `advanced_analyzer.py`, `deep_*`, `scan_*`, `*_analyzer.py`, `*_investigation.py`, etc.) and 65 PNG outputs (`bitplane_*.png`, `alpha_*.png`, `threshold_*.png`, `enhanced_*.png`, `reshaped_*.png`, `rotated_*.png`, `transposed.png`, `high_contrast.png`, `puzzle_image.png`). These were leftovers from a side stego investigation and have no relation to the alphacode project.
- **`test_update.zip`**: stale release-test fixture, no longer used.

### Fixed

- **`cargo fmt --check` CI failure on v1.0.19 release**: rustfmt drift in `src/alphacode_provider_core/mod.rs`, `src/alphacode_provider_env/mod.rs`, `src/alphacode_provider_metadata/catalog.rs`, `src/alphacode_provider_metadata/mod.rs`. Formatted to match `stable` rustfmt defaults so CI passes again.

## [Unreleased]

### Added

- **Experiential Labs provider — Free Gift from Alphacode**
  (`alphacode_provider_metadata::catalog::EXPLABS_PROFILE`,
  `cli::provider_init::ProviderChoice::Explabs`): a first-class OpenAI-compatible
  provider pointing at `https://api.experientiallabs.ai/v1` so the gateway's free
  models (qwen3.8-27b at launch, plus experimental GPT6 / Astra / Fable 5.1 etc. as
  the platform-funded lane grows) show up in the model picker as a single labeled
  entry. Auth is a bearer key minted at
  https://platform.experientiallabs.ai/settings/api-keys (`xpl_<40 hex>`,
  `EXPLABS_API_KEY`). The login picker and `/provider list` show it as
  "Experiential Labs (Free Gift from Alphacode)" and it is `recommended: true` so
  first-time setup surfaces the free lane before the paid gateways. Aliases:
  `experiential-labs`, `experientiallabs`, `experiential`, `xpl`,
  `alphacode-gift`, `alphacode-free`, `free-gift`.
- **Bundled demo key for the Free Gift from Alphacode lane**
  (`alphacode_provider_metadata::catalog::EXPLABS_BUNDLED_API_KEY`,
  `alphacode_provider_env::load_api_key_from_env_or_config`): when the user
  has not set their own `EXPLABS_API_KEY` (env var or `explabs.env`), the
  runtime falls back to a bundled `xpl_` bearer so the lane works out of
  the box with zero setup. Users who bring their own key still win — the
  env var and `explabs.env` are consulted before the bundled fallback. The
  bundled key is shared, so set `EXPLABS_API_KEY` for dedicated rate limits
  and credit balance.
- **Curated free-model list for the Free Gift from Alphacode lane**
  (`ALL_EXPLABS_MODELS`): top-down free slugs are `gpt-6-astra`,
  `claude-fable-5.1`, `gpt-5.6-luna`, `qwen3.8-27b`, `deepseek-v4-flash`.
  The post-login flagship picker consults this list so the free models
  rank above the live catalog's first random row, and
  `EXPLABS_PROFILE.default_model` is now `gpt-6-astra` so first-login
  auto-selects the strongest free tier.

### Fixed

- **OpenAI-compatible gateways no longer reject the whole chat request when one tool's description is over the provider's hard cap** (`alphacode_provider_core::tool_description::sanitize_tool_description`, `alphacode_provider_openrouter::request::sanitize_tool_description`, `alphacode_provider_openai::request::build_tools`, `alphacode_provider_openrouter_runtime::openrouter_provider_impl::complete`): strict gateways such as Experiential Labs (`https://api.experientiallabs.ai/v1`) were returning HTTP 400 `Invalid value for 'tools.30.function.description'` because the swarm tool's description inlines the full user-tunable `swarm-prompt.md` (~16 KB / ~4k tokens), which is far over the OpenAI-documented 1024-character limit most strict gateways enforce. The single over-long tool bricked the entire request, locking the user out of *every* tool call. The new canonical helper caps each tool's description at 1024 chars by default (configurable per-launch via `ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS`, with a 256-char safety floor so a misconfigured env var cannot strip descriptions to nothing), truncates on UTF-8 char boundaries, and appends a `[description truncated to fit provider limit; full prompt loaded separately]` marker so the model knows the text was clipped. The same helper is wired into both the openrouter-compat runtime path (used by every OpenAI-compatible gateway incl. explabs) and the native OpenAI provider path.

## [1.0.18] - 2026-09-05

Patch release. Two correctness fixes plus the lockfile bump CI demanded.

### Fixed

- **`/update` reload no longer re-execs the old binary**
  (`build::update_reload_candidate`, `cli::hot_exec::hot_reload`,
  `alphacode_build_support::paths`): when a release was freshly
  installed by `/update`, the post-install reload used to fall back to
  the local repo build (`target/{selfdev,release}/alphacode`) whenever
  its mtime happened to be newer than the channel binary. Self-dev
  users running from inside the repo, or anyone whose filesystem mtime
  resolution is coarser than the install stamp, saw `/update` succeed
  and then reload into the old binary ("stays with the old version").
  `/update` now always re-execs the freshly installed `current` channel
  binary. `/reload` and `/rebuild` keep the old "prefer the local
  build" behavior because those commands are explicitly about picking
  up a freshly built local binary. Regression test:
  `alphacode_build_support::paths::update_reload_candidate_prefers_freshly_installed_release_over_newer_repo_build`.
- **CI build lockfile drift** (`Cargo.lock`): the checked-in lockfile
  was 53 packages behind `Cargo.toml`, which made every CI matrix
  build (`ubuntu-latest / dev`, `macos-latest / dev`,
  `windows-latest / dev`) fail with "the lock file needs to be updated
  but --locked was passed" (see run 33969058675). The lockfile is now
  regenerated; `cargo build --release --locked` succeeds locally and
  on CI.

## [1.0.9] - 2026-09-03

Security-research usability release: simplify the deterministic command
gate and remove the URL-secret reflection prompt that was blocking
authorized bug-bounty work without actually catching abuse.

### Changed

- **Destructive-command gate** (`alphacode_command_risk::gate`,
  `tool::bash_destructive_gate`): the `Confirm` tier and its reflection
  prompt have been removed. The gate now collapses to two outcomes -
  `Allow` for everything except the catastrophic tier, and `Deny` for
  the catastrophic tier. Routine authorized security tooling (`nmap`,
  `subfinder`, `nuclei`, `httpx`, `ffuf`, `gobuster`, `curl` against an
  in-scope target, `cat`-on-an-output-file, etc.) used to be held for a
  reflection turn because every such tool reaches outside the working
  directory by definition; it now runs immediately. The catastrophic
  tier still refuses `rm -rf ~`, `rm -rf ~/.ssh`, `rm -rf /etc`,
  recursive destruction of system paths, and direct writes to device
  nodes. The `justification` field on the `bash` tool schema is
  preserved for backwards compatibility but is no longer consumed.
- **Windows URL-safety gate** (`tool::bash`): the FDJ-style
  `findstr`/`powershell` URL-with-`&` heuristic has been removed from
  the bash tool. The detector still exists as a pure function in
  `alphacode_command_risk::shell_url_safety` for callers that want to
  invoke it explicitly, but no longer pre-empts every bash call.
- **Webfetch URL-secret gate** (`tool::webfetch`): the previous
  scan-for-pasted-bearer/cookie/AWS/Stripe/key-in-URL reflection prompt
  has been removed. The webfetch tool now fetches any valid `http(s)://`
  URL as supplied. Authorized security testing that needs your own
  `Authorization` header against an in-scope target is now supported
  end-to-end. The `web_safety` module and its tests have been deleted.

### Security

- The `bash` tool still refuses `rm -rf /`, `$HOME` wipes, device-node
  writes, and similar catastrophic targets.

## [1.0.7] - 2026-09-02

Patch release. TUI polish round: the idle splash now renders the
`alphacode` wordmark with a per-character gradient over a breathing
box-drawing separator, the tip header swaps to a ✨ sparkle, the
todo-changes widget shows a percent-aware progress bar with clearer
status glyphs (✔, ✖), and the OpenRouter/gmicloud active-provider
fallback in `MultiProvider::resolve_active` now also consults the
config's `default_provider` key (not just the env-var-driven display
name). Also includes a one-off rustfmt sweep against five files
whose layout had drifted from the 1.91 toolchain.

### Added

- **Gradient idle wordmark** (`ui_animations.rs::render_idle_wordmark`):
  each character of the splash wordmark is now colored from a 7-stop
  gradient (`HeaderIcon → Info → Accent → ModelName → Warning →
  Success → HeaderIcon`) with `Modifier::BOLD` applied per cell, so
  the splash reads as a single visual unit instead of a flat string.
- **Breathing separator** (`ui_animations.rs`): a third splash line
  composed of varied box-drawing characters (─ ┄ ─ ┈ ─) cycled to
  reservation width, gradient-blended teal → blue → purple across
  the row. Hidden on reservations `<= 2` rows tall to preserve the
  wordmark's vertical center.

### Changed

- **Tip header glyph** (`info_widget_tips.rs`): the splash tip header
  now uses ✨ (U+2728) and the brand-gradient index `gradient[7]`
  instead of 💡 (U+1F4A1) at `gradient[9]`. Body text color bumped
  one step warmer (`rgb(175,180,200)` from `rgb(165,170,190)`).
- **Todo-change progress** (`ui_todo_changes.rs::progress_span`): the
  `(completed/total)` span now appends a five-cell bar
  (`░░░░░ NN%`) and a percent-aware foreground color
  (green `rgb(120,230,160)` at 100%, blue `rgb(148,188,255)` at
  50%+, purple `rgb(200,170,255)` below).
- **Todo-change status icons** (`ui_todo_changes.rs::status_icon`):
  `completed` glyph bumped from `✓` (U+2713) to `✔` (U+2714),
  `cancelled` glyph bumped from `✗` (U+2717) to `✖` (U+2716); all
  four palette colors lightened for better contrast against the
  default TUI background.

### Fixed

- **OpenRouter/gmicloud takeover** (`provider/startup.rs`): the
  fallback that promotes `OpenRouter` to the active provider when
  Claude is otherwise selected and the gmicloud endpoint is the
  configured OpenAI-compatible target now also triggers when
  `provider_state.default_provider_key()` returns `gmicloud` —
  previously only the env-var-driven display name was checked,
  which missed shells that set env vars in a different order than
  `apply_openai_compatible_profile_env` does.

### Housekeeping

- **rustfmt sweep** (`chore(fmt)`): re-runs rustfmt against
  `bundled_skills.rs`, `alphacode_base/mod.rs`, `skill.rs`,
  `alphacode_setup_hints/mod.rs`, and `windows_setup.rs`, whose
  layout had drifted from the 1.91 toolchain. No semantic changes;
  `cargo fmt --check` is clean.

## [1.0.6] - 2026-09-02

Patch release. Bundles the `bugbounty` skill directly into the binary so
`/bugbounty` (and its 16 subskill references) is always available,
regardless of the working directory or `$HOME`. Also removes the first-run
"Set up global keys to launch alphacode?" and "Alacritty: the fastest
terminal for alphacode" prompts, and fixes the macOS build that previously
errored on unused `libproc` constants.

### Added

- **Bundled `bugbounty` skill** (`src/alphacode_base/bundled_skills/`):
  the full bug-bounty methodology skill - including its 16 subskills
  (recon, hunt-sqli, hunt-xss, hunt-ssrf, hunt-idor, hunt-graphql,
  hunt-oauth, hunt-api, hunt-memory, llm-redteam, web3-audit,
  credential-attack, client-reverse, security-arsenal,
  advanced-techniques, report) - is now embedded at compile time via
  `include_str!`. The top-level `SKILL.md` is registered as `/bugbounty`;
  the subskill bodies are exposed as named `reference_files` so they
  remain available contextually when the skill is invoked. Users can
  still override the embedded copy by placing a same-named skill in
  `~/.alphacode/skills/` or `./.alphacode/skills/` - the on-disk overlay
  always wins, mirroring the historical load order. As a result,
  `/bugbounty` now shows up identically whether the user launches
  `alphacode` from a fresh shell with cwd outside the project tree or
  from inside the repo.

### Removed

- **First-run Windows setup nudges** (`windows_setup.rs`): the
  "Set up global keys to launch alphacode?" and "Alacritty: the fastest
  terminal for alphacode" prompts no longer interrupt the first launches
  of every Windows user. Users who still want either setup can run
  `alphacode setup-hotkey` (existing explicit path) or
  `alphacode setup-launcher`. The underlying helpers are retained
  (`#[allow(dead_code)]`) so future re-enablement is a one-line change.

### Fixed

- **macOS dead-code errors** (`stdin_detect.rs`): the `libproc`
  constants `PROC_PIDFDVNODEPATHINFO`, `PROC_PIDFDSOCKETINFO`, and
  `PROC_PIDFDPIPEINFO`, plus the `proc_pidfdinfo` FFI binding, are now
  annotated with `#[allow(dead_code)]` so `cargo check` no longer fails
  with `error: constant ... is never used` on macOS targets.
- **`RELOAD_HANDOFF_EVENT_POLL_MS` cfg gate** (`reload_state.rs`):
  scoped the constant to `#[cfg(target_os = "linux")]` (it is only
  consumed from the Linux-gated `wait_for_reload_handoff_event_blocking`
  helper). macOS no longer sees an unused-symbol warning.

## [1.0.5] — 2026-09-02

Patch release. TUI polish round: new commands, smarter model picker,
and tightened markdown rendering. No user-facing config, protocol, or
behavior changes.

### Added

- **New TUI commands** in `src/alphacode_tui/tui/app/commands.rs`
  (+127 LOC): additional slash commands the agent can call inline.
  See the in-TUI `/help` for the current list.
- **Smart model picker** (`smart_model_picker.rs`, +80 LOC): the
  model picker now ranks models by recency of use and groups by
  provider, so the model you actually use most is the one you
  reach for with the fewest keystrokes.
- **Info widget tip rotation** (`info_widget_tips.rs`): the splash
  hint line now rotates through a small set of tips instead of
  pinning one.

### Changed

- **Markdown rendering** (`markdown_render_full.rs`, +50 LOC):
  table styling, inline-interactive prompts, and message layout
  re-tuned. The renderer's visible behavior matches the v1.0.2
  palette commitments (high-contrast code/math/links/headings on
  dark backgrounds) but with sharper edges and a tighter
  per-message rhythm.
- **Inline-interactive prompts** (`ui_inline_interactive.rs`):
  the prompt chrome around inline choices is simpler and easier
  to skim.
- **Menubar** (`cli/commands/menubar.rs`): one minor behavioral
  fix and a small status-item update.

## [1.0.4] — 2026-09-02

Patch release. Removes the only production `unwrap()` in the codebase
that could fire from a real user action, found during a 1,157-call
`unwrap` audit. No user-facing config, protocol, or behavior changes.

### Fixed

- `MemoryManager` no longer implements `Default`. The previous impl
  called `Self::new(".").unwrap()`, and `MemoryManager::new` runs
  `Path::canonicalize()` and `fs::create_dir_all()` — both of which
  can fail on a read-only filesystem, a missing parent directory, or
  in any environment where `"."` is not writable. A blanket
  `Default::default()` would have panicked on any of those failure
  modes the first time anything reached for a default
  `MemoryManager`. There were zero callers in production, so the
  safest fix was to drop the impl and force callers to be explicit
  about the path they want. A short comment stands in for the
  removed impl so a future contributor who reaches for
  `Default::default()` understands why it is not there.

## [1.0.3] — 2026-09-02

Patch release. Fixes three install-script failures and cleans the
embedded `alphacode --version` string on tagged releases. No
user-facing config or protocol changes.

### Fixed

- **install.sh / install.ps1**: drop `--locked` from the source-build
  fallback. The committed `Cargo.lock` does not list every
  platform-conditional dep (e.g. `core-graphics` for macOS), and
  CI itself builds with `locked: false` — the installer was
  diverging from CI and would `error: ... Cargo.lock needs to be
  updated to use it` on first run for any platform that wasn't
  pre-warmed. The flag is gone, with a comment so the next person
  doesn't add it back.
- **install.sh**: `curl | grep | sed` race against the GitHub
  `/releases/latest` API. As soon as `grep -m1` matched `tag_name`
  and exited, the pipe closed and `curl` got `SIGPIPE` (exit 23,
  "Failure writing output to destination") on its next write. The
  ~15 KB JSON got cut off mid-stream, and on a slow / rate-limited
  connection the captured fragment could be missing `tag_name`
  entirely — so `VERSION` ended up empty and the script fell
  through to a 5–30 minute source build even though a release was
  right there. Fix: download the API response to a tempfile, then
  parse.
- **install.sh**: the previous fix used `local _api_tmp` at the
  top level of the script, which bash correctly rejects with
  `bash: line N: local: can only be used in a function`. The
  `local` is gone; the variable name is unique enough that global
  scope is harmless.

### Changed

- `.github/workflows/release.yml`: set `ALPHACODE_RELEASE_BUILD=1`
  and `ALPHACODE_BUILD_GIT_DIRTY=0` on the build step. Without
  these, the embedded version string took the dev-tagged form
  `v{version}-dev ({git_hash}, dirty)` even on a tagged release —
  `alphacode --version` reported e.g. `v1.0.2-dev (579910d, dirty)`
  on the v1.0.2 release binaries. With both env vars set, tagged
  releases embed the clean `v{version} ({git_hash})` form.
- `Cargo.toml`: bump version 1.0.2 → 1.0.3.
- `Cargo.lock`: `1.0.1` → `1.0.3` in the `alphacode` package entry
  (was previously drifted in the working tree but not committed; now
  matches `Cargo.toml`).

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
