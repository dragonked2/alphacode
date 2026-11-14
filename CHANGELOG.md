# Changelog

All notable changes to Alphacode are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/).

## [1.1.6] - 2026-11-14

### Added

- Durable engagement runtime (`engagement_runtime`) that tracks session scope,
  policy, and investigation intelligence across the orchestrator, agent,
  proof reviewer, and exports; the bug-bounty executor now derives a stable
  session id from the target and workdir and writes `engagement.json`.
- Authorization regression suite (`authz_regression_tests`) covering the
  run-local `AccessIndex` paths for denied controls, first-declaration
  identity/resource resolution, and exchange-backed assessments.
- New `authorization_review` and `investigation` tools exposing the durable
  engagement state to the agent turn.
- `reasoning-cases` bundled skill for moving from an interesting observation
  to a causal test (ownership vs. public preview, failed repetitions,
  revocation, interpreter evidence, server-side callbacks, business-state
  races, chain compatibility, and source-to-deployment differences).

### Changed

- Bug-bounty scanner tasks now execute through the same registry gates
  (`Arc<dyn Tool>`) as ordinary agent calls instead of a hand-rolled match,
  so every task benefits from the same output caps, timeouts, and input
  validation; `dalfox` joins the supported task tool set.
- Authorization engine (`authz.rs`) builds a run-local `AccessIndex` per
  case and exposes `analyze_access_inner` for reuse across assessments,
  removing per-call linear scans of identities, resources, and observations.
- The `MultiProvider` routes memo is now `Arc<RoutesMemoEntry>`, so forks on
  a shared server reuse one build instead of each rebuilding their own.
- `httpflow` bot-protection detection is now challenge-specific: Cloudflare
  `server`/`cf-ray` headers alone no longer trigger a false positive, and the
  error message describes the surface as blocked rather than recommending a
  CAPTCHA bypass.
- `ALPHAX_FREE_PROFILE_ID` skips the live-model cache lookup, so the free
  route never blocks on a catalog refresh that cannot serve it.
- Browser Agent extension ships 1.7.1; the 1.6.4 artifact is removed from the
  working tree and the `.crate` exclude list.
- README architecture diagram and CI action runtimes modernized.

## [1.1.5] - 2026-11-13

### Added

- Improve mission continuation with persisted success criteria, validation
  plans, checkpoints, and changed-file scope reminders.
- Add evidence-quality guidance and a repeatable, loopback-only bounty
  evaluation harness with controlled fixtures and independent verification.
- Add searchable help, clearer TUI orientation, expanded overlays, and input
  latency diagnostics with p50/p95/p99 reporting.

### Fixed

- Revoke stale completion evidence after edits, failed writes, interrupted
  reruns, or mismatched verification arguments.
- Improve repeated tool-failure recovery and provider/tool alias handling.
- Preserve rooted POSIX paths during command-risk assessment, including
  destructive-target protection for paths such as `/etc/passwd`.
- Keep Windows input ordering and queue timing independent from rendering and
  background work while improving remote key and error-toast behavior.
- Extend TUI regression coverage for help search, overlays, input behavior,
  frame metrics, and transport diagnostics.

## [1.1.4] - 2026-11-13

### Fixed

- Keep the selected **Alphax Free (automatic)** route pinned after provider
  rate limits instead of silently switching the session to concrete free models.
- Show the active session title in the Session widget, wrap long titles, and
  suppress flow widgets when their margin would cover transcript content.
- Prompt-history Backspace removes one Unicode grapheme cluster at a time.
- Remote Escape dismisses error toasts before changing the draft and clears
  inline-preview state consistently with local input.
- Goal verification ignores changes to the optional UI intent label while
  preserving exact execution-argument matching and failed-rerun invalidation.
  MCP and extension arguments remain exact, including any `intent` field.
- Large SDK tool results retain both initial context and final diagnostics,
  matching the truncation used for native tools.
- Structured diagnostics redact password and cookie fields; retry log URLs
  omit credentials, query parameters, and fragments.
- Windows CI coverage checks GUI callback recovery, diagnostic privacy, export,
  and log rotation using the desktop host's Windows PowerShell runtime.
- Isolate model-picker test catalogs from local credential discovery and align
  tool-render assertions with error-first summaries and wrapped identifiers.

### Added

- Repeatable coding-task evaluation with fresh fixtures, independent verifiers,
  completion receipts, checkpointed reports, process-tree timeout cleanup, and
  Windows/Linux CI tests.

## [1.0.100] - 2026-11-10

### Fixed

- Windows reads console input on a dedicated worker so typing can queue while
  the UI draws. Key-to-paint timing now includes the wait in that queue.
- Modifier releases clear held shortcut hints. The composer stays below the
  transcript by default, with an optional top position in display settings.
- Typing after a paste allows immediate submission. A stray Enter immediately
  after a paste still leaves the draft intact.
- Large text pastes skip file drop parsing. Remote key releases skip input
  bookkeeping and no longer overwrite the key-to-paint timing sample.
- Local turns apply already queued printable keys before repainting. Ordinary
  drafts skip suggestion-cache copies, and history completion reuses prompt
  positions in long transcripts.
- Model selection saves the local model and effort together. Remote catalog
  updates avoid a forced disk flush for the rebuildable cache.
- History search keeps the selected result visible in short terminals and
  shares keyboard hint wording with the command palette.

## [1.0.99] - 2026-11-08

### Fixed

- **Input lag after 1.0.92.** Two independent causes, both fixed. (1) The 1.0.95
  release pinned the `eitsupi/crossterm` `feature/windows-vt-input` fork via
  `[patch.crates-io]`; that fork replaces the whole crossterm event reader, so
  once bracketed paste is enabled (alphacode enables it for the session) Windows
  delivered *every* key record through a new VT/ANSI parser instead of the
  long-stable Win32 `ReadConsoleInput` path, making all typing and every picker
  feel laggy. The override was removed in 1.0.96; the TUI links stock crossterm
  0.29 again, and Ctrl+V/Alt+V paste keeps working through alphacode's own
  asynchronous `arboard` clipboard read. (2) With keyboard enhancement enabled
  crossterm reports `Press` *and* `Release` for each key, and both run loops
  returned "needs redraw" for the release too. A release cannot change a cell, so
  this queued a second full frame (preparation + diff + flush) for every
  keystroke and made the next key wait behind a frame that painted nothing.
  Releases now skip the redraw entirely; a regression test pins the rule.
- Windows startup no longer hangs with `Timed out waiting for server to become
  ready ... after 120000ms`. The 1.0.98 named-pipe hardening built its DACL from
  `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;CO)`: `CO` (CREATOR OWNER) only applies to
  child objects, so the pipe object itself had no ACE for the user who created
  it. The server bound successfully and stayed alive, while every client open on
  a non-elevated account failed with `ERROR_ACCESS_DENIED` (5) and the client
  polled a socket it could never open until its budget ran out. Pipe instances
  now grant Generic All to SYSTEM, Built-in Administrators, the object owner
  (`OW`), and the process user's own SID, so the daemon is reachable from both
  normal and elevated terminals while still excluding other local users.

## [1.0.98] - 2026-11-08

### Fixed

- TUI typing lag: the inline model-picker fuzzy re-score ran O(entries) on every
  keystroke because the previous debounce compared only the filter string (which
  changes on every key). Added a time-based debounce (80ms) with a tick refresh
  so rapid typing coalesces rescoring without leaving stale filtered results.
- Windows IPC hardening: named pipe servers (`bind`/`accept`/`pair`) now use an
  owner-restricted security descriptor (SDDL grants Generic All to SYSTEM,
  Administrators, and Creator Owner) and reject remote clients, reducing
  exposure of the local control channel to other local users.
- Desktop GUI stability: added WPF dispatcher and global trap/exception handlers
  to log and recover from recoverable UI errors instead of terminating the
  process. Hardened the "Choose workspace folder" click path with layered error
  handling around the native COM folder picker and `new_chat` queueing.

### Known limitations (not fully addressed in this release)

- The report's broader Execution Enforcer / centralized permission-architecture
  items (complete path binding, audit-ledger persistence, TOCTOU hardening,
  process-authorization binding, and full bypass elimination) require
  architectural integration and dedicated adversarial testing; this release does
  not claim those are production-complete.
- GUI approval-channel/overlay features described in external reports are not
  present in this source tree and were not implemented here.

## [1.0.97] - 2026-11-07

### Added

- Automatic in-session update checks: every 30 minutes the release build polls
  GitHub for a newer release while a session is open, going through the same
  `ReadyToReload` path as the startup background check so users never have to
  type `/update`. Dev/checkouts return `NotAReleaseBuild`/`RunningFromCheckout`
  and never download.

## [1.0.92] - 2026-11-02

### Added

- The info widget panel can be pinned: `display.info_widget_position = "pinned"`
  stacks it in a fixed column on the right of the transcript, which wraps
  narrower so the panel never covers text or moves while output scrolls. The
  default `"flow"` keeps today's margin-docked behaviour. The column is 40
  cells and needs a chat area of at least 100; narrower windows fall back to flow.
- Each info widget has an `[x]` in its top border; clicking it hides the panel
  for the window, like the toggle key.
- `display.info_widget = false` starts the TUI with the panel hidden.

### Fixed

- A new session no longer inherits a previous session's swarm plan. Swarm state
  is keyed by the workspace (the git common dir), so a plan outlives the session
  that created it; the plan was still being handed to unrelated later sessions by
  two paths. `send_swarm_plan_to_session` (subscribe/resume/history refresh) now
  only replays a plan to a session listed in its `participants`, and
  `broadcast_swarm_plan` no longer widens delivery to every swarm member when the
  plan's participants are absent or all dead. Sessions still join a plan through
  the normal paths (`seed`, `assign_task`, `resync_plan`), which add them as
  participants, so a reconnecting owner keeps its plan graph.
- The inline `Plan graph · vN` transcript card is only created when the
  transcript can actually draw a diagram: mermaid rendering *and* a native
  terminal image protocol. Previously the card was gated on the mermaid setting
  alone, while the markdown renderer also requires a native protocol, so on
  terminals without Kitty/iTerm2/Sixel the card degraded into ~100 lines of raw
  `flowchart LR …` source. The live plan is still shown by the swarm strip and
  the dedicated swarm page.

### Changed

- Windows release binaries now carry a `VERSIONINFO` resource. The build script
  previously emitted only an icon, so the shipped executable reported
  `FileVersion 0.0.0.0` with empty `CompanyName`, `ProductName` and
  `FileDescription`. File/product versions are derived from the crate version so
  they cannot drift from the build.

## [1.0.96] - 2026-11-05

### Fixed

- **Restore input responsiveness on Windows.** 1.0.95 pinned the
  `eitsupi/crossterm` `feature/windows-vt-input` fork (upstream PR #1030) via
  `[patch.crates-io]` so Windows Terminal's native Ctrl+V paste would arrive as
  one `Event::Paste`. That fork replaces the entire crossterm event reader: once
  bracketed paste is enabled (alphacode keeps it enabled for the whole
  session), Windows delivers *every* key record through a new VT/ANSI parser
  instead of the long-stable Win32 `ReadConsoleInput` path. On Windows that
  affected all input at once - ordinary typing, `Ctrl+Backspace`, and the
  model/provider pickers all felt laggy, exactly the regression users reported
  after 1.0.92. The `[patch.crates-io]` override is removed, so the TUI links
  stock crates.io crossterm 0.29 again. Ctrl+V/Alt+V paste never depended on the
  fork's `Event::Paste`: it is served by alphacode's own asynchronous `arboard`
  clipboard read. The debug-only call sites that used the fork's
  `MouseEvent::new` constructor now build the stock public-field struct through
  `tui::mouse_event`.
- Console typing no longer runs image-path promotion on every keystroke.
  `promote_dropped_images` was invoked from `handle_text_input` for each
  printable character, allocating a trimmed copy of the full draft and calling
  `path.is_file()` on path-like input each time. Promotion already happens on
  `Enter` (`handle_enter`) and submit (`submit_input`), where it matters before
  command/skill routing, so the per-keystroke call is removed. Typing is now
  allocation-free and avoids repeated filesystem stat calls.

### Added

## [Unreleased]

## [1.0.95] - 2026-11-05

### Fixed

- Ordinary typing no longer probes the filesystem for image drops on every
  character. A syntax-only image-path check keeps text and compact paste tokens
  off that disk-I/O path while retaining native terminal image drops.
- Compact pasted text is owned by its token position, not token count or paste
  order. Inserting, replacing, deleting, copying and cutting text cannot assign
  one paste's payload to a different token or expand literal lookalikes.
- Undo and stashed drafts retain shared paste payloads without copying large
  clipboard contents per keystroke. Clear, history recall/search, dictation,
  deferred remote prompts and failed-turn restoration preserve the correct draft
  ownership. Existing native-paste coalescing and trailing-Enter guards remain.
- Browser arguments accept the `navigate_and_wait` compatibility action and
  strictly validated numeric-string IDs, preserving safe tab targeting and
  repeated-action protection.

## [1.0.93] - 2026-11-03

### Security

- The Windows release no longer contains a nested executable, which was the
  root cause of the `Gen:Variant.Barys` detection reported against 1.0.92.
  `alphacode.exe` had carried the .NET/WPF GUI host inside it
  (`include_bytes!`), then wrote it to `%USERPROFILE%\.alphacode\gui\` and
  executed it on the first `alphacode gui`. Two complete PE images in one
  unsigned file, one dropping and launching the other, is the packer/dropper
  shape generic engines score. It was confirmed structurally: the 1.0.92 binary
  held PE images at offsets 0 and 60,102,752; the same debug build now holds
  exactly one. The host ships as a sibling `AlphaCode.Desktop.exe`, resolved by
  `resolve_desktop_host()` (override, then beside the binary, then the legacy
  directory so in-place upgrades keep working). The embedded layout remains
  available behind the opt-in `embedded-gui-host` cargo feature.
- The release pipeline now packages `alphacode.exe` and `AlphaCode.Desktop.exe`
  together and fails if either binary contains more than one PE image
  (`scripts/verify-pe-single-image.py`). A future change cannot silently
  reintroduce the detection.
- Disclosure of what the 1.0.92 verdict actually was, since it matters for
  anyone triaging the report: VirusTotal showed 8/65 for
  `alphacode-windows-x86_64(2).zip` (sha256
  `aa30c118…f279c1ca`), and seven of those eight share one engine. BitDefender
  reported `Gen:Variant.Barys.381847`; MicroWorld-eScan, ALYac, VIPRE, GData and
  Emsisoft reported the same family, and Arcabit its `Trojan.Barys` naming.
  CTX added `zip.unknown.generic`. Microsoft, CrowdStrike, SentinelOne,
  Kaspersky, ESET, Sophos, Fortinet and Trend Micro all returned clean. It was a
  single heuristic family, not eight independent analyses.

### Added

- Long-running commands leave the foreground on their own. A `bash` call that
  produced no output for the idle window (2 minutes;
  `ALPHACODE_BASH_AUTO_BACKGROUND_IDLE_MS`, `0` disables) is handed to the
  background manager instead of blocking the turn until its timeout. The command
  keeps running, the session is woken on completion, and the tool result names
  the task id. Alt+B was previously the only way out, which required a human
  watching the terminal. Output *staleness* is the trigger rather than total
  runtime, so a compiler or scanner that keeps printing stays in the foreground,
  and commands that legitimately wait (an interactive prompt, `tail -f`,
  `cargo watch`, server processes) are exempt. `"auto_background": false` opts
  out per call.
- Windows GUI integration: the installer creates a Start-menu shortcut and a
  Desktop shortcut for the GUI, and records an AppUserModelID so Windows groups
  its windows, shows the right icon and offers the normal "Pin to taskbar"
  entry. Programmatic taskbar pinning is not attempted because Windows 10
  removed it; the installer says so instead of shipping a verb that fails.
  `-NoShortcuts` and `-NoDesktopShortcut` opt out, and the uninstaller removes
  both shortcuts and the registry identity.
- `scripts/dev-check.sh` runs fmt, clippy, check and tests through one rerunnable
  command, applying the MSVC environment and a dedicated `CARGO_TARGET_DIR`.
- `scripts/verify-pe-single-image.py` fails when a binary embeds another PE
  image.

### Fixed

- Proactive compaction could never fire for a model missing from the catalog.
  Providers report `DEFAULT_CONTEXT_LIMIT` (2,000,000,000 tokens) for an unknown
  model, deliberately, so prompt sizing never fails closed — but that number was
  then used as the compaction budget, and 85% of 2e9 is unreachable by any real
  transcript. Compaction therefore only ran reactively, after the provider
  returned a context-limit error. The budget now maps that sentinel to a
  conservative 128k, and context usage is computed against a non-zero budget.
- The compaction summary prompt embedded tool inputs verbatim, so a `write` or
  `apply_patch` call put an entire file body into the request that was supposed
  to compress the transcript. Inputs are now bounded at 400 characters with an
  explicit truncation marker.
- `Cargo.toml` is the single source of the release version, and the release
  workflow now fails if it disagrees with the tag. Previously a tag could
  publish a crate labelled with the previous version, and since the Windows
  VERSIONINFO resource is built from the crate version, Explorer's Properties
  dialog would advertise the stale number.
- Two library tests no longer assert internal mechanics instead of behaviour.
  The read-buffer test demanded byte-identical capacity across a
  `split_off`-based drain that legitimately reallocates down to the retain
  floor; it now asserts the contract that matters (never below the floor, never
  regrown past the original). The header test demanded the literal string
  `alphacode` at a 5-column width, where a 4-cell budget can only produce
  `Alp...`; it now asserts the line is non-blank and that its leading run is a
  real prefix of the brand.
- Two env-var tests raced each other on `ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS`
  and failed intermittently (`left: 1024, right: 3000`) because cargo runs tests
  in parallel and the variable is process-global. Both now hold the crate's
  existing `lock_test_env()` guard.
- `alphacode --version` could report an older release than the binary was built
  from. The release-tag fallback used `git describe --abbrev=0`, which returns
  the nearest tag rather than one at HEAD, and accepted it without checking —
  so a build from an untagged tree advertised whichever old tag happened to be
  closest (observed: `v1.0.91` from a 1.0.93 build whose VERSIONINFO correctly
  said 1.0.93). It now requires `--exact-match` and otherwise falls back to the
  crate version.

## [1.0.91] - 2026-11-01

### Added

- The model can now see which tools this session actually has. A live,
  policy-filtered capability map is attached to the dynamic prompt on each
  request: registered/advertised counts, capability-family samples, and how to
  discover a deferred schema. Progressive disclosure still keeps individual
  schemas out of the request, but "hidden from the prompt" is no longer
  indistinguishable from "not available".
- `tool_help` gained explicit `search`, `list`, and `enable` actions. `search`
  matches names, bounded descriptions, and capability-family hints *without*
  promoting the schemas it finds, so inspecting the catalog is cheap; `enable`
  loads selected schemas; omitting the action keeps the previous automatic
  promotion for `tools`/`query`. Broad query-only matches above eight tools
  return a bounded catalog for selection instead of promoting everything, and
  catalog output is capped at forty entries and about 8 KiB. Malformed input is
  now a validation error instead of a silent listing of unrelated tools.
- Skills can declare `triggers:` in frontmatter - outcome vocabulary ("find
  bugs", "prove the impact") kept separate from `aliases`, so a trigger word
  does not have to behave like a slash-command alias.
- Background runs now wake the agent that launched them. A `bash` call with
  `run_in_background` implies `wake`, so a finished command resumes the session
  instead of leaving it idle until someone pressed Alt+B. `bg wait` clears the
  wake flag the moment it commits to waiting, because blocking on the result
  *is* the rendezvous - leaving the flag armed injected the same completion as
  a second turn racing the wait's own return.
- `agents.plan_max_nodes` (default 40, env `ALPHACODE_PLAN_MAX_NODES`) caps how
  large one task graph may grow. Deep mode rewards splitting and adds a
  critique/verify gate per composite, so a 14-node seed was observed reaching
  75 nodes before most seed nodes had finished. Seeding is never blocked; only
  growth past the cap is, and the error names the limit so the agent
  consolidates instead of splitting again.
- The browser tool's script runner returns the value of a bare
  `await <expression>`. The obvious form -
  `await fetch(u).then(r => r.text())` - is a statement, so its value was
  discarded and the caller got `null` back from every script written that way.
- Reasoning effort is now a shared, bounded control ladder. One stepping
  function replaces the duplicated cycling logic in the hotkey, `/effort`, and
  the model picker, so the three cannot disagree. An unknown default starts from
  an ordinary reasoning level instead of implicitly selecting a swarm mode,
  though explicit progression into swarm modes still works.
- `research_ledger_context` gives the challenge/hunt workflows a stable path to
  their evidence ledger, and reports a missing ledger as a restorable
  precondition when the path is not a regular file.

### Changed

- Remote reasoning effort is tracked as two states: the request in flight and
  the effort the server has confirmed. Transmission failures no longer mutate
  confirmed state; acknowledgments advance it, retain any newer pending
  request, clear rejected requests, invalidate picker state, and redraw.
  Untracked and stale acknowledgments are ignored, and disconnects or session
  changes clear the tracking. The header hint reads the confirmed value, so it
  can no longer advertise an effort the server never accepted.
- The OpenAI catalog's effort levels are canonicalized, deduplicated, and
  ordered. An explicitly empty capability list exposes no selectable effort
  (including swarm sentinels); explicit invalid levels are rejected; and the
  default reasoning level is applied only when the current model's capability
  list supports it.
- An OpenAI-compatible profile's declared default model is no longer trusted
  blindly. `alpha` belongs to the Dragonmeta local server, so pointing the
  generic profile at LM Studio, llama.cpp, vLLM, or a relay left the header and
  status line naming a model the endpoint did not have. When a live catalog
  contradicts the declared default the advertised model is adopted; a cold
  install or an endpoint with no usable catalog keeps the declared value.
- Duplicate notifications merge into one card with a visible occurrence count.
  The timeout extends without restarting the slide-in animation or collapsing
  diagnostics the user is currently reading. Messages, hints, and recovery text
  also have terminal escape and control sequences stripped while keeping
  readable paragraphs.
- The wordmark keeps a single brand row for the whole conversation. Active
  conversations previously competed with the decorative banner, which could
  re-expand at the end of a turn; size is now chosen from terminal geometry
  rather than message or processing state.
- The composer's existing bottom border now carries width-fitted shortcuts
  (Enter adapts to send, queue, immediate continuation, command selection, or
  shell execution; Alt+Enter teaches multiline; F1 opens help; an active turn
  exposes Esc to stop). The top border names Message, Command, Local shell, or
  Server shell. Focus guidance shows scrolling and Esc-to-return when a
  side/diagram pane has focus, instead of advertising a key that would go to the
  wrong target.
- The CTF entrypoint selects a workflow from actual inputs and routes to the
  existing category references, instead of loading copied installation,
  scanning, submission, and pseudo-ML examples into every task.
  Automatic-submission and automation-disguise instructions were removed.

### Fixed

- Lifecycle hooks (`[hooks]`) now see the *attached client's* terminal env
  instead of the shared server's startup env. Hooks run inside the server, so
  with several clients they all saw the terminal that happened to start it and
  could not tell which tmux/zellij/herdr pane a session is shown in. They now
  get the same client snapshot as spawn/focus hooks (#405): `TMUX_PANE`,
  `ZELLIJ_PANE_ID`, `HERDR_PANE_ID`, ... natively and as
  `ALPHACODE_CLIENT_<NAME>`. Server terminal vars the client did not send are
  dropped rather than mixed in; sessions with no attached client keep the
  server's env.
- A client terminal-env snapshot is filtered to the known terminal variables
  before it can reach a hook process. The snapshot arrives over a socket, so
  without the allow-list a peer could inject `PATH` or `LD_PRELOAD` into the
  hook's environment. A snapshot containing no terminal variables clears the
  session's entry rather than installing an empty one.
- `Agent::clear` now drops the closed session from the hook client-env
  registry. It closes the session without going through `mark_closed`, so the
  entry leaked and a later session reusing the id could inherit the previous
  client's terminal.
- The in-app updater rejects absolute, drive-absolute, and UNC archive entry
  names before handing them to `zip`'s `enclosed_name`. From `zip` 9 that helper
  *strips* a leading separator (`/absolute.txt` becomes `absolute.txt`) instead
  of returning `None`, so an absolute entry was silently flattened into the
  extraction root and installed content from a path the archive did not choose.
  Unreadable entry names are skipped.
- The inline todo card appears on its own. `refresh_todo_card_if_needed` only
  ever refreshed an *existing* card, so a long turn could write a dozen todos
  and the user saw none of them unless they already knew the todo hotkey or
  `/todos`. The card now opens once when the session's list becomes non-empty,
  is still dismissible, and a dismissal survives later writes instead of being
  undone by the next background update.
- The header rule no longer spills onto an extra row. Header lines wrap at
  `width - 1` but the separator was built at exactly `width`, so every frame
  showed a full-width rule followed by a lone one-cell dash.
- Transcript line counts are no longer narrowed to `u16` before scrollbar and
  packed-layout decisions. Above 65,535 rows the cast wrapped and adding chrome
  could overflow; decisions and diagnostics now keep `usize` counts and convert
  only for on-screen dimensions.
- A zero-size resize clears layout, status, and copy-selection snapshots instead
  of leaving the previous frame's hit-testing geometry live. Click mapping in a
  multi-line draft now uses the same caret-based viewport as painting, so
  clicking a visible row no longer selects a different line after the draft is
  scrolled; hidden hints and collapsed borders are excluded from mapping.
- Preview wheel momentum stops when a rendered preview fits without scrolling.
  A zero maximum was mistaken for an unknown maximum, so scroll steps kept being
  accepted and discarded by the next render. Long preview offsets now saturate
  at the existing `u16` scroll limit instead of wrapping.
- Session labels, task titles, and onboarding geometry measure terminal cells
  and slice on grapheme boundaries. CJK and joined emoji no longer overwrite the
  brand badge or truncate mid-character.
- Prefix deduplication no longer drops repeated incremental reasoning tokens;
  reasoning deltas are preserved exactly as received.
- Intermediate SSE events are no longer rescanned from the beginning on every
  network refill. A separate scan cursor retains only the three-byte overlap a
  split LF/CRLF delimiter needs, and scanning works on bytes so an overlap
  position inside a UTF-8 sequence cannot panic.
- Batch results retain image attachments in original call order, including when
  read-only tools complete concurrently. Previously only the text survived
  aggregation.
- Completion-evidence reconstruction joins tool calls and results
  chronologically and consumes each matching call exactly once, so orphan
  results, future calls, and duplicate results no longer manufacture evidence.
  Test and command success requires a successful `bash` result carrying a
  command argument (a file read or an errored tool cannot supply execution
  evidence), file-modification verification requires an existing regular file
  *and* a successful write/edit/multiedit to the same canonical path, and test
  evidence requires an actual `test name ... ok` line rather than summary text
  that previously invented a test named `result`.
- Plan and status broadcasts hold their member snapshot guard through delivery.
  An older plan version can no longer overtake a newer mutation while waiting on
  routing or member locks, and status delivery no longer takes an extra
  write-lock per recipient on a hot path.
- Gemini provider instances reuse one lazily initialized HTTP client instead of
  rebuilding clients and connection pools per request.

### Performance

- Unicode draft layout is cached per exact draft text and width. Height
  measurement built styled rows and scanned the caret, then drawing and clicking
  recomputed the same grapheme segments; a bounded thread-local geometry cache
  (one entry, 256 KiB of input, 4,096 wrapped rows) now serves all three paths.
  Isolated measurement: a 2,900-byte Unicode draft drops from 39.5 us of
  segmentation to 0.07 us of warm lookup, and a 29,000-byte draft from 503.6 us
  to 0.82 us. Editing the text or changing wrap width still misses the cache;
  painting and copy snapshots do other work, so this is not a whole-application
  speedup.
- SSE event framing no longer rescans all previously seen bytes per refill.
  Isolated measurement: a 10,000-byte event delivered in 32-byte fragments drops
  from 615.7 us to 16.7 us, and a 100,000-byte event from 53.2 ms to 152.9 us.
  These deliberately tiny fragments expose the prior quadratic scan and are not
  a claim about ordinary packet sizes.
- Terminal reveal no longer throttles received text at 960 characters/second
  with a completion tail; both TUI constructors and continuation resets select
  immediate reveal, leaving redraw scheduling to control paints. The paced
  buffer remains available for callers that explicitly choose it.
- Inline worker previews retain only the final 1,400 Unicode characters of the
  growing response, bounding work and memory independently of response length
  (regression covers a 140,000-character four-byte tail). The preview stays
  separate from canonical response storage.

### Security

- The `zip` crate moves from 2 to 9, and the updater's entry-name validation was
  strengthened to match the new `enclosed_name` behavior described above.

### Notes

- Performance figures are isolated benchmarks of extracted functions on one
  machine. They exclude terminal painting, JSON decoding, network RTT, and model
  inference, and are not end-to-end or tokens-per-second multipliers.

## [1.0.90] - 2026-10-31

### Added

- OrcaRouter provider. `alphacode login --provider orcarouter` accepts a
  `sk-orca-...` API key from the console, and
  `alphacode login --provider orcarouter-oauth` runs an OAuth 2.0 + PKCE
  authorization in the browser (RFC 8252 loopback redirect, S256 challenge,
  fresh verifier and state per attempt, manual callback paste when no local
  listener can be bound). Both paths save the credential in
  `~/.config/alphacode/orcarouter.env` and drive the same OpenAI-compatible
  inference profile at `https://api.orcarouter.ai/v1`. Model selection comes
  from the live `GET /v1/models` catalog with a verified seed fallback: rows
  that advertise only `embeddings`, `image-generation`, or `rerank` are kept
  out of the chat picker, and per-model `input_modalities` decides vision
  support instead of a blanket guess. OrcaRouter keys do not refresh, so a
  401 marks the *rejected* key generation as needing re-login rather than
  silently retrying with a key the gateway already refused.
- Find-in-page over the transcript. `Ctrl+F` opens a search bar above the
  composer; typing filters the transcript live, `Enter` / `↑` / `↓` step
  through the hits (scrolling the viewport to each one), `Alt+C` toggles
  case-sensitivity, and `Esc` closes the bar and leaves the viewport where
  the user navigated to. Every hit on screen is painted with a soft amber
  background and the selected hit with a stronger one, so "what matched" is
  answerable at a glance. `/find <text>` does the same thing and is
  discoverable from the slash-command palette.
  The search runs against the *rendered* transcript rather than the raw
  message list, so it finds exactly what the user can see — markdown
  headings, role labels, and text that survived truncation — and never
  reports a hit inside trimmed tool output. The hit index is cached against
  the query and the transcript size, so an idle TUI does not rescan on every
  frame.
- The composer is now a real text field. `Shift+←` / `Shift+→` select text,
  `Ctrl`/`Alt`+`Shift+←`/`→` extend by word, `Shift+Home` / `Shift+End` select
  to the boundaries of the draft, `Ctrl+C` copies the selection, `Ctrl+X` cuts
  it, and typing or pasting over a selection replaces it. The selection is
  painted with the same highlight the transcript uses for mouse selections, so
  both ways of selecting look identical. Until now the composer could only be
  edited with the mouse, which made "fix the typo in the middle of my prompt"
  a two-handed operation.
- `@`-mentions complete file paths. Typing `@` opens a path palette over the
  project tree; `Tab` / `Enter` insert the highlighted path, and only the
  `@token` is replaced, so the rest of the prompt survives. The scan is
  bounded, cached, and runs off the keystroke path's critical section. The
  system prompt now tells the model that a `@path` in the user's message is a
  file to read.
- The slash palette can be dismissed without losing your text. `Esc` closes
  the popup and leaves the draft alone; a second `Esc` clears the draft, the
  way it always did. Previously the palette could not be dismissed at all: an
  absolute path opened it, and the only `Esc` handler cleared the whole input.
  A path-like token (`/usr/bin/env`) no longer opens the command palette in
  the first place, and `Tab` reopens a palette that was dismissed.
- The command palette now reads like a table: descriptions are aligned in a
  column, the selected row carries a background so the eye can find it
  instantly, and the footer says where you are in the list (`12/143`).
- `alphacode doctor` is now usable as a CI gate: it exits non-zero when a
  check fails, prints a summary line, and reports `ok` / `failed_checks` in
  its `--json` output. It previously always exited 0, so a pipeline could not
  tell a healthy install from a broken one.

### Performance

- The prepared-frame cache no longer hashes the whole live stream on every
  frame. The cache key now uses an exact mutation counter plus an O(1)
  content fingerprint (exact for buffers up to 512 bytes, sampled above that),
  which turns a per-frame O(bytes) hash into O(1) for long responses.
- The body builder no longer re-hashes the whole transcript several times per
  rebuild. The message cache key is memoized, so the boundary table, the
  assistant aux cache and the prefix/suffix reuse checks are O(1) per
  unchanged message instead of O(payload).
- The message cache's memo key now mixes in a content fingerprint. It was
  keyed on buffer addresses and lengths alone, and the allocator routinely
  hands a freed message's address to the next message of the same length — a
  collision there let an unrelated message inherit the previous message's
  cached render.
- The pinned todo band no longer re-renders and deep-clones the todo card on
  every frame. The finished band is cached against the payload and the
  geometry, so an idle viewport does a string comparison instead of a full
  card render.
- The headless console shows a bounded preview of a tool result *and states
  how much was left out*. It used to print only the first line, so a
  truncated result looked exactly like a complete one.

### Fixed

- The startup banner spelled nothing. `WORDMARK_BLOCK` had been generated by a
  converter that wrote per-cell SGR colour escapes into the string literals and
  left the rows ragged (164 visible cells on the first row, 134 on the rest),
  so the widest tier was unreadable wherever it did appear. It is now the plain
  six-row, 73-cell `ALPHACODE` block lettering with no embedded escapes, and
  `wordmark_art_carries_no_ansi_escapes` pins that. The escapes had a second
  cost: `wordmark_for` measures rows with `unicode-width`, which counted the
  escape bytes as printable cells and saw the art as 362 columns wide — so the
  block tier was almost never selected, and when it was, it was centred from
  that bogus width.
- The TUI header's gradient rule spilled onto an extra row. Header lines are
  wrapped at `width - 1`, but the separator was built at exactly `width`, so
  every frame showed the full-width rule followed by a lone one-cell dash.
  `gradient_separator_occupies_one_row_after_wrapping` measures the real wrap
  pipeline so the header height stays stable.
- A narrow TUI header no longer ends in a dangling `·`. The separator before
  `/model to switch` was pushed independently of the hint, so on terminals too
  narrow for the hint the model line still ended in a floating separator, which
  read as a string cut off mid-way.
- The splash screen's documented tier widths were wrong (they claimed 53/33/25
  columns for tiers that actually need 63/17/13). The comment now matches
  `WORDMARK_LADDER`, and `docs/branding.md` records that wordmark rows are plain
  text whose colour comes from the renderer.

- The agent no longer dead-ends a turn after narrating an action it never
  performed. A model that says "Let me just add dispatch via sed." and then
  emits no tool call had its turn treated as finished, so the session sat
  waiting for the user with the tool failures that caused it still
  unexplained. The harness now recognises a closing message that narrates an
  unperformed action *after a failed tool result* and asks the model to
  actually perform it — bounded to one attempt per turn, and only when the
  round being answered contained a failure, so an ordinary successful reply is
  never continued.
- A failed render no longer takes the whole session down. The release
  profile used `panic = "abort"`, which makes every `catch_unwind` in the
  codebase inert: the frame-level recovery in `ui::draw` ("rendering error
  recovered") and the mermaid renderer's own recovery only existed in debug
  builds, so the shipped binary died on the exact failures they were written
  to survive. The release profile now unwinds, which costs a little binary
  size and buys a session that survives a single bad frame.
- A panic in the mermaid renderer no longer silences unrelated panics. The
  renderer swapped the *process-global* panic hook for the duration of the
  render, so a panic on any other thread in that window was swallowed
  silently, and two concurrent renders could restore each other's hooks out
  of order. The suppression is now thread-local and the hook is installed
  exactly once.
- `/help` no longer lists a command twice. The "more commands" sweep compared
  the full registry name against the first word of the hand-curated rows, so
  a command registered with its arguments (`/find [text]`) was listed again
  even though the curated section already showed it. Both sides are compared
  on the first word now.
- A remote streaming test no longer fails on a loaded machine. It asserted
  that the paced backlog was still pending, but the reveal controller credits
  the idle time since the buffer was created to the first chunk, so a slow
  host could drain the whole backlog before the assertion ran. The buffer
  gained a `reset_reveal_clock` seam so the test starts from a deterministic
  point.
- Two test suites no longer fail depending on how the test runner schedules
  them. The metrics registry and the event bus are process-global, so a test
  that resets or publishes to them raced every other test that read them: one
  test's `reset()` wiped another's counters mid-assertion, and one test's
  `LoginCompleted` event arrived in an unrelated test's subscription and
  failed it. Both now have a shared lock that the participating tests hold
  for their whole body.
- The model-browser perf gate no longer reports a phantom panic. Its
  "cache hit must not invoke the build closure" guard was a `panic!` inside
  the closure, but that closure runs on a spawned worker wrapped in
  `catch_unwind`, so the panic was swallowed and the assertion could never
  fail — it only printed a panic banner on every run. The guard is now an
  atomic flag the test owns and asserts after the cache hit returns. A build
  panic is also logged instead of vanishing: a failed row build previously
  looked exactly like a provider that returned no models.

### Changed

- `Ctrl+F` opens the transcript search instead of moving the caret one word
  forward. Word-forward stays available on `Alt+F` and `Ctrl+Right`, and the
  help overlay now says so.
- The composer lays out its wrapped rows from a single wrap computation
  instead of two. The segment list (a grapheme split plus a display-width
  measurement of every cell) was built once for the rendered rows and again
  for the copy-selection snapshot, on every frame.
- `alphacode doctor` measures its label column in display cells rather than
  bytes, matching the rule the rest of the CLI output already follows.

## [1.0.80] - 2026-10-13

### Fixed

- A prompt you send is no longer swallowed by the screen. The status line's
  gradient rule was painted at `chunks[3].y - 1`, which is the transcript's own
  last row whenever the transcript fills its chunk. Since nothing is appended
  after a freshly sent prompt until the first token arrives, the rule drew a row
  of `─` straight across the message you had just submitted, and the prompt only
  reappeared once the assistant had something to stream. The same row carries
  the live streaming line and the legacy `↓N` overflow indicator, so both were
  being erased too. The rule is now drawn only into the slack row between the
  transcript and the status line, and only when that row exists.
- The composer sits at the bottom of the screen again. Every chunk of the chat
  column was a `Constraint::Length`, and ratatui's default `Flex::Start` parks
  whatever the constraints do not claim in the *final* spacer — so on a short
  conversation in a tall terminal the status line and composer floated in the
  middle of the screen with a large blank band beneath them (11 rows on a 40-row
  terminal), and the composer slid one row further down for every transcript
  line that arrived. The unused rows are now reserved as an explicit gap chunk,
  which keeps the transcript top-anchored and the bottom chrome flush with the
  bottom of the frame at every terminal size.
- The right-hand session facts render again, and the `↓N` scrolled-past
  indicator is visible. Both were blocked by the gradient rule above: the fact
  stack's documented anchor is `input_area.bottom() - 1` and it may only climb
  through *contiguous* blank rows, so one full-width rule immediately above the
  status line left it nowhere to go and it stood down entirely.
- The visual-debug layout overlay labels the regions it actually draws. Its
  indices predated a chunk insertion, so it was labelling the swarm strip
  "status", the status row "picker" and the notification row "input". The chat
  column's chunk indices are now named constants shared with the overlay.

- The header no longer carries a duplicate copy of the session facts. The 1.0.80
  work added a consolidated strip (context %, token totals, queue depth,
  provider billing) to the model line, which both duplicated the right-hand
  fact stack and consumed the rows that stack probes for free space: with the
  strip present the stack could not find a collision-free block and rendered
  nothing at all. The strip and its private `format_strip_tokens` helper are
  gone; the header keeps model identity, connection and the `/model` hint, and
  the facts belong to the right-hand column.
- `ui_polish::centered_gradient_separator_long_enough` no longer asserts on the
  wrong quantity. It compared `Line::spans.len()` against the requested width,
  but the separator's two-cell gutters are each a *single* span holding two
  characters, so a correct 40-cell line reports 38 spans and the test failed on
  correct output. It now asserts `Line::width()`, matching
  `gradient_separator_spans_the_full_header_width`.
- Source files saved as CP1252 no longer break the build. Five files
  (`ui_header.rs`, `verification_quality.rs`, `tui_lifecycle.rs`, `keybind.rs`
  and `input.rs`) had been written through Windows-1252, which rewrote every
  non-ASCII character into the three characters that encoding produces - a
  banner box-drawing run became three mangled glyphs apiece, and the ellipsis
  in `model.contains(...)` became `U+00E2 U+20AC U+00A6` instead of
  `U+2026`. Nothing flagged it at the time: the files remained valid UTF-8
  throughout, so the damage only surfaced later as four `unknown start of
  token` errors from rustc at `ui_header.rs:245`. The affected runs were
  decoded back through CP1252 to recover the original bytes. Genuine
  non-ASCII characters in the same files - the Mac Option-key bindings in
  `keybind.rs` and the AltGr symbols in `input.rs` - are byte-identical
  before and after, because a character that is not CP1252-representable
  cannot round-trip and is therefore left alone.
- Added `.editorconfig` pinning the working tree to UTF-8, with the reasoning
  recorded in the file. The repository already carried a `.gitattributes`
  guarding a neighbouring Windows encoding problem, but nothing stopped an
  editor from re-introducing this one.
- Buffer-level regression tests for the chat column: the composer is flush with
  the bottom of the frame at four terminal sizes, it holds still while the
  transcript grows, a freshly sent prompt is visible before the first token
  arrives, and a gradient rule never occupies the last transcript row.

- The bottom session/status row no longer loses information when the terminal
  is narrow. It used to be one long concatenation that a trailing ellipsis cut
  from the right, so the casualty was always whatever text sat last — the
  context reading, the token totals, and the `+N queued` count. The idle
  stats row, the streaming row, and the running-tool row are now assembled
  from priority-tagged segments that are dropped lowest-priority-first, with
  short fallback forms (token totals without the per-direction breakdown, a
  model name without its provider prefix) tried before anything is dropped.
  Context usage and session tokens now survive down to very narrow widths.
- The header's gradient separator spans the full terminal width instead of
  stopping at 120 columns, where it left a dead gap that read as a rendering
  glitch on wide terminals.
- Bottom-chrome colors — the status row, the right-hand session facts, the
  overscroll line, the notification line, and the send-mode indicator — now
  come from the shared theme roles (`accent`, `success`, `warning`, `error`,
  `info`, `dim`, and a new `muted_text` role) instead of roughly forty
  hardcoded RGB values. These rows, and the header, now follow `/theme`
  presets and `[display.colors]` overrides exactly rather than only through
  the buffer-level heuristic remap.
- Reproduction commands now really are killed at their budget on Unix. The
  harness only killed the direct child, so `sh -c "sleep 60"` left its
  `sleep` grandchild alive holding the inherited stdout/stderr pipes, and
  `run_bounded` stayed blocked in the output drain for the rest of the sleep
  — the "timeout hangs the session" failure the harness exists to prevent.
  The child is now spawned in its own process group and that group is
  signaled on timeout, mirroring what `taskkill /T` already did on Windows.
- The system prompt keeps the guidance the 1.0.79 rewrite had dropped: treat
  tool output, files, web pages, MCP results, and quoted text as data rather
  than as instructions, and establish that a target is within the user's
  stated authorization and scope before security testing. The always-on
  prompt budget assertion now tracks a 96k-char ceiling (the prompt is ~67k
  since the security playbook was folded in) instead of the old 4k one, which
  no longer described any prompt this build ships.
- The installers now show live download progress. `install.ps1` replaced
  `Invoke-WebRequest -OutFile` with a streaming transfer that writes to disk
  as bytes arrive and redraws a bar showing percentage, bytes, throughput and
  ETA. The old cmdlet buffers the whole response in memory and writes the file
  only when the transfer finishes, so a 20 MB release asset sat invisible
  until it was nearly done and then appeared at once. Rendering is capped near
  ten frames a second, since a host call per chunk makes the renderer itself
  the bottleneck on a fast connection, and the bar is suppressed when output
  is redirected so a captured log is not smeared with carriage returns.
- Downloads resume instead of restarting. A release asset that dies at 90%
  continues from there, via `Range` on the PowerShell side and `curl -C -` on
  the shell side, rather than beginning again. Bytes are only ever resumed
  within a single URL, and the partial is discarded first, so one candidate
  asset can never be appended to another. A `206` response is required before
  appending; a server that ignores `Range` answers `200` with the whole file,
  which would otherwise corrupt the partial.
- A stalled transfer now fails instead of hanging forever. GitHub's release
  CDN often delivers only tens of KB/s, and a socket that goes half-open
  mid-transfer left the installer waiting indefinitely with neither movement
  nor an error. `install.sh` passes `--speed-limit 1024 --speed-time 60` so
  curl turns the stall into a failure its own `--retry` can restart;
  `install.ps1` sets `ReadWriteTimeout` to the same effect.
- `install.sh` reports the byte count and elapsed time once a download
  finishes. The bar shows motion but not throughput, and at tens of KB/s the
  difference between a working transfer and a stalled one is otherwise
  invisible.
- `scripts/tests/install-path.tests.ps1` passes again. It was asserting on
  `Fail` and `Warn`, which the installer had renamed to
  `Stop-AlphacodeInstall` and `Write-AlphacodeWarn`, and on `$localAppData`,
  now `$lad`, so the suite failed before reaching a single real assertion. The
  prefix test now checks the shape of the default path rather than the name of
  a local variable, `Assert-Contains` escapes `-like` wildcards so a literal
  bracket in a needle is not read as a character class, and the suite covers
  the new download path: streaming rather than buffering, resume, the stall
  timeout, the progress frame, and silence when output is redirected.

- The command-risk gate no longer denies ordinary commands that mention a
  glob. A glob's parent directory was graded as a destruction target
  unconditionally, so every relative glob was catastrophic in a session
  started in the home directory - there the working directory *is* `$HOME`,
  and the parent of every resolved target is `path == home`. A glob in the
  working directory now grades the same as the concrete path beside it,
  which this gate already accepts, and a glob with no directory part and
  nowhere to resolve it against no longer reads its empty parent as `/`.
  Globs that really do land in a protected directory (`~/.ssh/*`,
  `~/.ssh/id_*`, `~/.aws/*`, `~/*`, `/etc/*`) are still catastrophic, now
  by checking the glob's own resolved path as well as its parent.
- The shell tokenizer handles `#` comments. An apostrophe in an ordinary
  comment (`# Check if it's ...`) opened a quoted region that swallowed the
  rest of the command and threw every later quote out of sync, so a quoted
  grep pattern was split at its `>` and the fragments were graded as
  truncating redirect targets - which are graded even for a read-only
  `grep`, and whose `was_quoted` stamp the hidden-verb scan trusts to tell
  an executed `sh` from a quoted one. A `#` at the start of a word now runs
  to end of line; a `#` inside a word is left alone.
- `2>/dev/null` inside a command substitution is recognised as a device sink
  again. The closing `)` of `$(...)` was left on the token, so `/dev/null)`
  matched no sink and a routine stderr discard was graded as a write to an
  unknown path outside the working directory.
- The malformed-call refusal costs roughly 40% fewer tokens and no longer
  repeats one instruction three times. The recorded error already carries
  the corrective sentence, the guard appended it again, and the
  agent-facing hint layer added a third copy on top. Guidance already on
  the record is now quoted once, and the generic hint is skipped when the
  error hands over a literal example to copy.

## [1.0.78] - 2026-10-11

### Fixed

- Configured colors are now applied in the session picker. That screen runs its
  own render loop instead of going through the main draw path, and it adapted
  frames for light themes but skipped the palette pass, so every
  `[display.colors]` override was silently ignored there while applying
  everywhere else.
- The same omission is fixed for the swarm replay screen, which had the same
  two-of-three adaptation sequence.
- macOS: the session-picker, hotkey-feedback, alignment and copy-badge tests no
  longer assume the literal `Alt` label. macOS renders the Option keycap, which
  is both a different string and a narrower one, so these assertions failed
  only there.
- macOS: session tests no longer compare `current_dir()` against the unresolved
  `tempdir()` path. macOS resolves `/var` to `/private/var`, so the two
  legitimately differ for the same directory.
- Tests that `chdir` into a temporary directory now restore the working
  directory on every exit path. The restore used to sit after the assertions, so
  one failed assertion left the process inside a directory that was then
  deleted, and every later test calling `current_dir()` failed with ENOENT —
  turning a single real failure into a cascade of unrelated-looking ones.
- macOS: the native-scroll bridge tests build a short socket path again. The
  previous name overflowed `sun_path` (104 bytes) inside CI's long `TMPDIR`, and
  `bind` failed with `EINVAL`.
- The paste rapid-insertion tests no longer sleep against the 20ms gap
  threshold, where ordinary CI scheduling jitter could make them fail.

### Added

- A source-level guard asserts that every render loop applying the light-theme
  pass also applies the palette pass, and that the palette pass runs second. The
  omission it prevents compiles cleanly and renders correctly under the default
  palette, so it could not be caught by an ordinary test.

## [1.0.77] - 2026-10-10

### Fixed

- The 1.0.76 release shipped a binary that still self-reported 1.0.75, because
  `Cargo.toml` was never bumped alongside the release tag. The manifest now
  tracks the release it belongs to.
- Composer text arriving from the remote transcript and from session-restore
  snapshots is now stripped of terminal control sequences, so escape-sequence
  remnants can no longer re-fill the input box with garbage (#540).
- Restoring a session now clamps `cursor_pos` against the input's byte length
  rather than its character count. The previous comparison could leave the
  cursor mid-grapheme on any non-ASCII input, making Backspace edit the wrong
  cluster.
- Backspace and Delete normalize a desynced cursor to a character boundary
  before slicing. A cursor left mid-grapheme would otherwise panic inside
  `String::drain` and, since release builds use `panic = "abort"`, take down
  the whole process.
- The `pre_tool` hook tests no longer leak their process-global
  `ALPHACODE_HOOK_PRE_TOOL` into concurrently-running tests. Every test that
  drives `Registry::execute` now takes the shared test-env lock, so a policy
  script installed by one test can no longer block unrelated tool calls in
  another and fail the suite nondeterministically on Linux CI.

## [1.0.76] - 2026-10-10

### Added

- Atria Dawn joined the OpenAI-compatible provider catalog with a 256K context
  window, exposed through Chat Completions, Messages, and Responses.

### Changed

- Antigravity login now reserves an available loopback port instead of a fixed
  one. A hard-coded port that another application already owned prevented the
  browser callback from ever reaching Alphacode.
- Windows shell commands are spawned with `CREATE_NO_WINDOW`, so a tool run
  from a GUI host no longer flashes a separate console window.

## [1.0.75] - 2026-10-10

### Improved

- The idle session summary now adapts to terminal width, keeping token totals
  and context usage visible on compact layouts while reserving billing details
  for wider terminals.
- Idle status tips now leave room for the session context reading instead of
  truncating it off the end of the row.
- Clarified the reference token value as an estimate in the wide session
  summary.

### Fixed

- Gate Windows-only shell descriptions and Python command recovery to Windows
  builds so Linux CI no longer fails on dead-code warnings.

## [1.0.73] - 2026-10-09

### Changed

- Selecting a skill from the slash palette activates it immediately and leaves
  the composer ready for the skill prompt; remote sessions refresh skill data
  before routing slash input.
- Session token usage now includes a clear notional dollar value at $1 per
  million tokens, including on free and subscription models. Provider-billed
  cost remains labeled separately.
- Replaced the working/waiting Braille spinner with a compact rotating ring.
- Direct local models with context windows up to 32K now receive a compact
  task prompt and a bounded, query-relevant tool catalog. Trivial chat turns
  omit tool schemas entirely.

### Performance

- In the captured LM Studio `hello` reproduction, estimated prompt-prefix
  overhead fell from 30,465 tokens (including 30,318 tool-schema tokens) to
  147 tokens. This is a single-request context measurement, not a latency
  benchmark.

### Fixed

- Keep the compaction budget aligned with the active provider after model or
  route switches, and use a server-learned local runtime context even when the
  model catalog advertises a larger trained context.
- Restart the outer turn after provider retry or context compaction so an
  already-completed request future is not polled again.
- Treat visible grapheme clusters as the unit for composer cursor movement,
  deletion, mouse placement, wrapping, and display-width calculations. Combining
  marks and joined emoji now stay intact while editing and resizing the input.

### Security

- Replaced the oversized default system prompt, which contained unsafe blanket
  authorization and refusal-bypass guidance, with concise engineering and
  security rules that preserve scope and treat external content as untrusted.

## [1.0.70] - 2026-10-07

### Changed

- Reduced the built-in system instructions to preserve more of a model's
  context window for the user's actual request, and improved how context is
  budgeted during long sessions.
- Refined the terminal composer, command suggestions, status, header, and
  progress rendering for clearer interaction and more reliable layout.
- Combined model, context, usage, and memory into a compact Session panel, with
  short error summaries and expandable full diagnostics.
- Added Alphacode's public homepage to default outbound User-Agent headers, the
  OpenRouter attribution fields, CLI help, package metadata, and the wide TUI
  header.

### Fixed

- Alphax Free login now activates through the active session, including remote
  sessions, without unsupported client-side model switching or unnecessary
  catalog discovery. User-facing status and errors omit internal route details.
- Improved provider route and catalog activation so login and model selection
  do not reuse stale catalog state or start competing refreshes.
- Hardened tool input and output handling with bounded file, HTTP, and process
  reads, and corrected path and streaming edge cases.
- Missing Go-based bug-bounty scanners now trigger Go setup and a targeted
  install on first use, then retry the scan; the default installer includes
  Dalfox, and failed setup clearly reports that the scan did not run.
- Fixed composer wrapping, cursor placement, copy selection, and other TUI
  regressions found during release verification.

## [1.0.68] - 2026-10-01

### Added

- **`-AddPath` for the Windows installer.** `install.sh --add-path` has configured
  PATH for Unix users since it was written; Windows users had to hand-edit
  `HKCU\Environment\Path` while the README claimed the installer did it for them.
  `-AddPath` closes that gap, and is deliberately narrow:
  - writes **only** `HKCU\Environment\Path`. The machine-wide PATH is never
    touched — that needs admin and would affect every user on the machine;
  - **never rewrites `$env:PATH`.** The session PATH is the merge of user +
    machine + anything the session added; rebuilding it from the registry would
    silently drop those session-only entries. The running shell is left alone and
    you open a new one;
  - **appends, never prepends**, so no pre-existing entry changes precedence.
    Nothing moves, so nothing that used to resolve to `git`/`python`/`node` can
    start resolving to something else;
  - reads the value *unexpanded* and writes it back with its original registry
    kind. `[Environment]::GetEnvironmentVariable('Path','User')` expands
    `REG_EXPAND_SZ`, so a PATH containing `%USERPROFILE%\bin` would have come
    back already substituted and been frozen to today's literal path — a silent,
    permanent regression for anyone who later renames their user directory or
    copies the install elsewhere;
  - refuses rather than truncating if the result would exceed the Windows
    32767-character limit;
  - is a no-op on a re-run, and compares entries case-insensitively and ignoring
    trailing separators, so it cannot accumulate duplicates;
  - broadcasts `WM_SETTINGCHANGE`, the same call `setx` and Chocolatey make.
    Without it the write silently succeeds and appears to do nothing until the
    next reboot, which is the usual "the installer lied to me" report;
  - `-PathDryRun` prints the exact value it would write and commits nothing.
  `uninstall.ps1` now removes the entry again (pass `-KeepPath` to leave it),
  so installing and uninstalling does not leave a PATH entry pointing at a
  directory that no longer exists.

### Fixed

- **The Windows installer chose the wrong directory under Windows PowerShell
  5.1.** The install prefix was selected with `$IsWindows`, a variable that only
  exists in PowerShell 6+. The documented invocation is `iwr ... | iex`, which
  on a default Windows machine runs 5.1, where `$IsWindows` is undefined and
  evaluates to `$null` — so the Windows branch was dead code on the shell most
  users actually run, and binaries silently went to `$HOME/.local` instead of
  `%LOCALAPPDATA%\alphacode`. That also made `uninstall.ps1` miss them: it looks
  for `%LOCALAPPDATA%\alphacode\bin`, so uninstalling reported success while
  leaving the binary behind. The check now uses
  `[Environment]::OSVersion.Platform`, which behaves the same in every edition.

- **A non-ASCII character could silently break the PowerShell installer.**
  `install.ps1` is distributed as `iwr ... | iex` and is routinely run by Windows
  PowerShell 5.1, which decodes a BOM-less file as the ANSI code page. In CP1252
  the byte `0x94` is a right double quotation mark, so a UTF-8 em dash
  (`E2 80 94`) decodes to two mojibake characters *and a closing quote* — which
  terminates the surrounding string and turns the rest of the line into a
  syntax error. `install.ps1` already carried one such character. CI could not
  have caught it: the check used `PSParser::Tokenize`, which only splits text
  into tokens and never checks that they form a valid script. The scripts are now
  ASCII-only, and CI does three things it previously did none of: **parses** the
  scripts with `[Parser]::ParseFile` instead of tokenizing them, **fails on any
  non-ASCII byte** in a `.ps1`, and **runs the PATH tests**.
- **`scripts/tests/install-path.tests.ps1` covers the PATH logic.** The
  installer mutates a system-level setting, so the decision-making half is a
  pure function over a string and is exercised over a matrix of shapes — empty,
  `;;`-doubled separators, trailing separators, case differences, `%VAR%`
  references, paths with spaces, sibling directories sharing a prefix, and
  re-runs — plus a dry run that asserts the registry and the session PATH are
  both untouched. The suite loads the functions by parsing `install.ps1`'s AST
  rather than dot-sourcing it, so it never downloads or installs anything, and
  it asserts the functions are *callable* rather than merely present: an earlier
  version checked names only, and reported green while every test failed with
  "not recognized". Four deliberate mutations of the planner (prepend instead of
  append, case-sensitive compare, prefix match, `%VAR%` expansion) were each
  caught by the suite.

- **`bash` rejected usable commands with "missing field `command`".** The alias
  recovery was case-sensitive and only accepted the value if `command` was
  already a JSON string, so a capitalised key (`Command`), a bare-string payload,
  or any loosely-shaped wrapper produced the same "missing field" error as a call
  that genuinely carried nothing. Recovery now goes through the shared
  `coerce_command_field`, which also adds case-insensitive lookup, and the
  fix-it line shows a command (`ls -la`) rather than a file path. It deliberately
  has **no** "the only string field must be the command" fallback, because
  `BashInput` also carries `intent` and `justification` — that fallback would
  execute a description of what to do as a shell line. `{"intent": "clean the tmp
  directory"}` is rejected, and there is a test saying so.

- **The onboarding accessibility metric counted its own banner chrome.** Tier
  10's `no_unicode_dependence` scan is documented as counting load-bearing prose,
  with "logo is decorative" excluded. It detected art with a single heuristic —
  a line whose letters are under half its non-space characters — and three lines
  slipped past it: the version banner `┌ Alphacode v1.0.52-test ┐` (0.56, mostly
  letters), the title rule `─── Welcome to alphacode ───`, and the logo
  `✨ alphacode`. Nine glyphs, which is what the assertion reported. Decorative
  framing is now stripped from the edges of a prose line and the text kept, on
  the same reasoning already applied to the `✓ `/`▸`/`•` marks in that function:
  the glyph carries nothing the words do not, so nothing load-bearing can depend
  on it. Stripping is symmetric — a line framed on both sides is a banner or
  title rule, whereas a lone leading glyph could equally be an emoji closing a
  real sentence.

- **A malformed call's failure streak was never cleaned up, and a success only
  cleared one of the two tiers.** Both surfaced while fixing the loop below:
  `clear_session` collected the keys to drop from the runtime-failure map alone,
  so a session's *malformed* streaks survived into the next session that reused
  the id — and the stored error text was never dropped at all, leaking one
  session's messages into another and growing without bound. `record_success` was
  written `failures.remove(..).is_some() || malformed.remove(..).is_some()`,
  whose short-circuit skips the second removal whenever the first one hits, so a
  malformed streak survived a success.

- **An identical malformed tool call could be re-sent without limit.** `write`
  and `bash` both reported `missing field \`file_path\`` /
  `missing field \`command\``, "The call carried no arguments at all", and the
  model re-sent the byte-identical call — eight times in one observed session,
  each one burning a turn to re-learn the same thing. The repeat guard was
  capable of stopping this and was being told not to: input-validation errors
  called `clear_failure`, which *erased* the failure streak for that exact
  (tool, input) pair, so the count could never climb. That exemption bought
  nothing, because the streak is keyed on the input — a model that genuinely
  fixes the call sends different input and starts from zero regardless. Malformed
  calls are now counted in their own tier with their own limit (3, deliberately
  more forgiving than the runtime-failure limit of 2), and the refusal quotes the
  last error and the keys that actually arrived instead of re-reporting the same
  sentence forever.

- **Ordinary commands were hard-denied for merely mentioning a shell.** The
  defense-in-depth scan for a destructive verb hidden behind an unparseable
  wrapper also matched *shell names anywhere in the segment*, and a `Catastrophic`
  verdict is explicitly "no amount of model justification can unlock it". So
  `grep -rn "sh" src/`, `git commit -m "fix the sh wrapper"`,
  `echo "run bash later"` and `grep -r "powershell" docs/` were all refused
  outright — 6 of 7 routine commands in the reported session, over the word `sh`.
  Two exclusions fix it without opening a hole: a word that was **quoted** in the
  original command is data (a grep pattern, a commit message, an echo body is
  never executed), and a program that only reads cannot execute its arguments.
  `docker run img sh -c "rm -rf /"`, `npm run sh` and
  `find . -exec sh -c 'rm -rf /'` all still fail, because in each the shell is an
  unquoted operand of something that can run it. `Token` gained a `was_quoted`
  bit for this; it is purely additive and does not affect path grading, so
  `rm -rf "$HOME"` still grades identically to `rm -rf $HOME` and quoting is not a
  bypass of the protected-path checks. `false_positive_tests.rs` is the companion
  to the existing `bypass_tests.rs`: a gate that denies ordinary work is as broken
  as one that admits destructive work.

- **A cleared default model/provider could never be cleared.** `ProviderConfig::default`
  ships a default model (the free provider's automatic routing alias) and
  `default_provider = "alphax-free"`,
  `#[serde(default)]` on the struct re-injects both for every key `config.toml`
  omits, and `save` drops a `None` field entirely because TOML has no null. So
  setting the default to nothing round-tripped straight back to the shipped value:
  `/model` "don't save a default" and `set_default_model(None, …)` were both
  no-ops that looked like they worked. Every "did the user pick this?" check was
  then answered by the fallback, which is why onboarding never preferred the
  strongest model for a first-run user — the one case that path exists for.
  `Config::has_explicit_provider_defaults` now reads the raw document, where
  "absent" is still distinguishable from "set", and treats a missing or
  unparseable file as "no explicit choice" rather than as a configured install.

- **`/config ui` swallowed every key, including `Ctrl+C`.** The overlay claimed
  all input and returned before the global control shortcuts ran, so an open
  settings overlay made the app impossible to interrupt or quit with the
  keyboard, and the mouse did nothing at all — the transcript underneath kept
  scrolling behind a modal box. `Ctrl` chords now stay global, the wheel steps
  the setting list, and a click outside the box dismisses it (judged against the
  rectangle that was actually painted).

- **`/config ui` rendered off-screen on short or narrow terminals.** The width
  was clamped to a 24-column minimum *before* being capped by the terminal, so a
  10-column terminal got a 24-wide box whose right edge was cut off, and the
  height could push the status line below the bottom of the screen. Both are now
  capped by the area last, and the footer row is only reserved when the list has
  a row to spare.

- **`webfetch` reported "followed 0 redirect(s)" for every redirected fetch.**
  The hop counter was threaded from the redirect loop into the result, but
  `Attempt::Done` hard-coded `hops: 0`, so a fetch that followed three hops
  told the model it followed none — and the model reads that as "this URL served
  the content directly", hiding the exact hop worth noticing. The count is now
  the real one, and a loop that somehow runs past the bound reports an error
  instead of hitting an `unreachable!()` panic.

- **`jwt` forged `alg:none` tokens that no server would accept, causing missed
  findings.** The `hunt-jwt` skill instructs the agent to forge an unsecured
  token with this tool and send it to the endpoint that rejected the original.
  `forge`, `manipulate`, and `forge_secret` emitted a *two-part* token
  (`header.payload`) when the signature was empty. A compact JWS is always three
  dot-separated parts — for `alg: none` the signature is the empty string, but
  the trailing dot is still part of the serialization (RFC 7519 §6). Strict
  servers reject the two-part form as malformed, and the tool's own `decode`
  rejected it as "expected 3 parts". The agent would send the token, see it
  fail, and report that the `alg:none` bypass does not work — a false negative
  caused by our encoder rather than by the target. All three paths now emit
  `header.payload.`, and `decode` names the missing-dot case explicitly instead
  of just counting parts.

- **`webfetch` reported `missing field \`url\`` for argument shapes that are
  perfectly valid.** A bare `serde_json::from_value` rejected an alias key
  (`uri`, `target`, `href`), a bare URL string where an object was expected, a
  single-element array wrapper, and any payload truncated by the provider's
  streaming decoder — each of which is a recoverable typo that cost a whole
  agent turn, and each of which the model then retried byte-for-byte until the
  repeat guard blocked it. All four now resolve through a shared coercion
  ladder (`coerce_url_arg`, `coerce_host_arg`, `coerce_text_arg`), adopted by
  `webfetch`, `websearch`, `read`, `write`, `katana`, `unfurl`, `scrapling`,
  `nmap`, `subfinder`, `amass`, `assetfinder`, `nikto`, `sqlmap`, `dalfox`,
  `gobuster` and `feroxbuster`. When nothing can be recovered the error names
  the field, the fix, *and* the keys that actually arrived, with an example
  shaped for that field — a host tool is no longer told to send a path.
- **`write` reported `missing field \`file_path\`` — and could have silently
  written a half file.** A long `write` body is the most likely argument in any
  session to exceed the provider's `max_tokens`, and the decoder discarded the
  surviving fields entirely. Complete key/value pairs are now read back out of a
  truncated payload; an *unterminated* trailing string is deliberately **not**
  recovered, because handing `write` a prefix of a file body would leave a
  corrupt file. When the payload was truncated, `write` refuses to overwrite an
  existing non-empty file at all, and writes to a new file carry an explicit
  warning in their output.
- **The `webfetch` SSRF guard was bypassable through redirects.** The guard
  validated the initial URL only, while the shared HTTP client used reqwest's
  default redirect policy (`limit(10)`). A public URL returning
  `302 Location: http://169.254.169.254/latest/meta-data/` was followed straight
  past it, turning every guarded fetch into an open proxy to the cloud metadata
  service and a browser-as-a-service for anything on loopback. The fetch client
  now disables automatic redirects and `webfetch` follows them by hand,
  re-running the full guard on every hop (with the correct 301/302/303/308
  method-downgrade rules and a 5-hop bound). Redirection is reported in the
  output rather than silently substituting the final URL.
- **Anti-bot detection false-positived on ordinary English.** `"just a moment"`
  (every latency article) and `"ray id"` (generic) were treated as proof of a
  challenge, so those pages paid a wasted request per fetch and could still be
  reported as blocked. Both now require a corroborating vendor marker. The
  check also stopped copying the whole body into a lowercased buffer — it uses
  precompiled `(?i)` literals over the first 16 KB plus a header-level check, so
  a 5 MB page is no longer scanned five times through an extra 5 MB allocation.
- **HTML→markdown rewrote the contents of code blocks.** `<pre>` bodies were run
  through the `<strong>`/`<em>`/`<code>`/`<li>` passes, so `**` in a stack trace
  became markdown emphasis, an `_var_` in a payload lost characters, and blank
  lines were reflowed away. For a security tool this is the worst kind of error:
  a wrong payload that still looks right. Fenced code is now extracted first and
  re-inserted verbatim after the entity decode, and blank-line collapsing skips
  fence interiors. Entity decoding is also single-pass — `&amp;lt;` no longer
  decodes into a real `<`.
- **Redundant full-buffer copies dominated HTML conversion.** Every stage read
  `text = re.replace_all(&text, "").to_string()`, cloning the entire body
  *again* after `replace_all` had already returned an owned `String`; 15 (text)
  to 25 (markdown) passes over a body that reaches 5 MB is ~100 MB of pointless
  allocation and memcpy per fetch. Stages now consume the buffer without a
  second copy and skip entirely when the pattern is absent. Markup-free
  payloads (JSON, source, CSV) skip the tag passes altogether, which also stops
  a `<foo>` inside a JSON string literal being stripped.
- **`smart_stream` allocated twice per line on every tool output.**
  `normalize_line` made a `Vec`, a `join`, and a `to_lowercase`; `char_bigram_count`
  copied the line into a fresh allocation and is called up to 32 times per line
  inside the fuzzy-dedup window. Both are now single-pass and allocation-free,
  with a length gate in front of the bigram comparison. Filtering results are
  also measured against the char budget, so the reported reduction describes
  what the model actually receives instead of always reading as over budget.
- **One mistyped optional field could cost an entire call.** `katana`, `read`
  and `websearch` rejected a payload outright over one mistyped optional value
  (`"threads": "20"` instead of `20`). Those fields are now coerced or dropped,
  with the rest of the request honoured.
- **Settings can be changed in-app instead of by hand-editing `config.toml`.**
  `/account <provider> settings` was read-only: it printed the current values
  and then listed the `/account … ` commands you would have to type by hand,
  which is the complaint on issue #6. `/config ui` now opens an interactive
  overlay — arrow keys to move, Space/Enter to toggle, Esc to close — covering
  compact notifications, pinned todos, centered output, alphagrep output, and
  tool-call detail. Every row persists through the same `Config` setter the typed
  commands use, so the two paths cannot drift, and the change applies on the
  next frame without a restart. Failed writes are reported in the overlay rather
  than silently reverting, and `/config ui` is listed in the help overlay, the
  `/help config` entry, the completion list, and the `/config` usage line — a
  command you have to guess the name of does not fix the problem it was added
  for.
- **Scrolling up mid-session moved the cursor to a random place and stopped
  working (issue #6).** While output was streaming, `scroll_up` converted
  bottom-follow mode into an absolute offset using the renderer's extent, but
  then clamped against `scroll_max_estimate()`, which is *deliberately
  inflated* so it never falls behind text that has been appended but not yet
  drawn. The stored offset and the drawn position therefore disagreed: the next
  frame clamped the view somewhere unrelated, and because the offset was then
  already at zero, every subsequent scroll-up was a no-op. The same mismatch
  existed in `pause_chat_auto_scroll`, reached by starting a drag-select. Both
  now resolve against the extent the renderer actually clamps against, and only
  fall back to the estimate before the first frame has reported one. The
  behaviour was session-scoped because the estimate only diverges while
  streaming, which is why scrolling worked again once the run finished.
- **`browser` rejected `action='find'`, and could not address an element that
  `interactables` had just listed.** Three separate gaps made "find this element
  and click it" impossible. `find` (and `search`/`locate`/`query`/`elements`)
  was refused outright, so the agent invented another name to locate an element
  instead of using the action built for it; those now normalize to
  `interactables`. `interactables` returned 1-based ordinals (`1.`, `2.`, `3.`)
  while the extension's `resolveElement` selects by a 0-based `index` — and the
  tool had no parameter to pass one, so the number the agent read meant nothing
  and it guessed a selector instead. That is the direct cause of the
  `click` → `Element not found` loop. The output now prints `index=N` and
  `index` is accepted on `click`/`hover`/`type`. Separately, the extension
  filters candidates through `visible()`, so an element inside a collapsed menu
  or a `display:none` widget resolved to nothing with no way to reach it;
  `include_hidden` is now forwarded. `interactables` also silently dropped the
  `contains` filter it is documented to honour, returning all 250 elements
  instead of the matching handful.
- `webfetch` no longer rejects valid URL spellings it used to: surrounding
  whitespace, a mixed-case scheme (`HTTP://`), a protocol-relative URL
  (`//host/path`), and a scheme-less host (`example.com:8443/health`). The
  scheme-vs-host distinction was tightened at the same time, so
  `data:text/html,x` and `javascript:alert(1)` are still rejected rather than
  promoted to a request against a host named `data`.

## [1.0.67] - 2026-09-30

> **Upgrade note for 1.0.66 users on Windows:** the in-app updater shipped in
> 1.0.66 cannot install any update, including this one, because of the zip
> extraction bug fixed below. Download 1.0.67 manually once; self-update works
> again from then on.

### Fixed

- **The Windows in-app updater failed on every release, for every user.** The
  zip extraction path proved containment by comparing
  `extract_dir.canonicalize()` against `extract_dir.join(file_name).parent()`.
  On Windows `canonicalize` returns a verbatim `\\?\`-prefixed path while the
  other side was built from the non-canonical `extract_dir`, so the two could
  never compare equal. The guard rejected *every* entry and aborted with
  `zip entry "alphacode.exe" resolved outside the extraction directory` before
  writing a single byte. Windows release assets are `.zip`, so this was 100% of
  Windows self-updates rather than an edge case. The root is now canonicalized
  once and entry names are joined onto that canonical path, which makes
  containment structural and also removes a `canonicalize` syscall per archive
  entry. The extraction logic moved into `extract_zip_asset_into` so it is
  directly testable, with regression coverage for top-level extraction,
  multi-file archives, and the zip-slip rejections (traversal, absolute, and
  nested entries) that the guard exists to prevent.
- **Restored 10 missing module declarations in `alphacode_tui::tui`.**
  `enhanced_status`, `improved_input`, `model_browser`,
  `model_browser_open`, `model_browser_render`, `model_performance`,
  `picker_spacing`, `smart_model_picker`, `ui_console`, `ui_empty_state` and
  `ui_professional` all exist on disk but were not declared, producing five
  unresolved-import errors and a hard build failure.
- Removed three duplicate `#[test]` definitions in `ui_header.rs` that made the
  test module fail to compile.
- **An output-length cap was misread as a context overflow, silently deleting a
  message.** `is_context_limit_error` accepted `maximum tokens`,
  `token limit`, and `exceeded && tokens` — all of which an *output* cap also
  matches. When a provider rejected a response for exceeding its output limit,
  the app compacted the conversation and retried the identical request, which
  failed the same way, while dropping a message and reporting
  `Context compacted (emergency) — older messages dropped`. Compaction shrinks
  the input and cannot raise an output cap, so the retry was never going to
  succeed. Output-cap and payload-size errors are now classified separately, and
  neither triggers token compaction.
- **A transient 404 on `SHA256SUMS` aborted every update, which is what kept
  1.0.65 users on the broken Windows updater.** In `release.yml` each `build`
  job uploads its archive straight to the release, and the separate `release`
  job merges and uploads `SHA256SUMS` afterwards — so for a window the release
  advertises `SHA256SUMS` while the CDN has not yet propagated it. The client
  fetched that 571-byte file with no retry at all, while the 21 MB archive got
  10 attempts with HTTP Range resume, so a single transient `404 Not Found`
  failed the whole install. The manifest is now fetched with 6 attempts and
  exponential backoff (~30 s), retrying `404`/`403`/`5xx` and transport errors.
  It still fails closed if every attempt fails, so an unverified binary is
  never installed; the error now says the release may still be publishing and
  to retry, rather than implying the install is broken. Note that a *missing*
  `SHA256SUMS` entry in the release payload is deliberately not retried — that
  is a real condition rather than propagation lag.
- **Emergency compaction no longer reports a fabricated token count.** The
  auto-compact path raises the observed input-token count to the full context
  limit so the compactor agrees it is out of room. Because
  `effective_token_count_with` returns `max(estimate, observed)`, that faked
  value leaked into the emitted event, so a 18k conversation was announced as
  `256,000->18,319 tokens` — reading as though the system prompt had exploded
  when it had not. The real pre-compaction size is now captured before the
  counter is raised and restored before compacting. The forced value is still
  used to select the compaction strategy, which is all it was needed for;
  `hard_compact_with` sizes its drop from a char-based estimate against
  `token_budget`, not from the observed count.

### Changed

- **The login picker now follows the selected theme.** Its seven chrome colors
  were module-level `const`s holding fixed RGB values, so the screen ignored
  the active palette and rendered as a hardcoded dark panel under all 30+
  presets — most visibly broken in light themes. They now resolve through
  `role_color`, using the `PanelBorder`, `PanelBorderMuted`, `MutedText`,
  `Border`, `SelectionBg`, `Dim` and `UserBg` roles. Three of the previous
  literals were byte-identical to those role defaults, so the default palette
  renders unchanged while every other theme becomes correct. The per-provider
  brand colors in `provider_style` intentionally stay literal: they are brand
  identities, not chrome.
- **`install.sh` can configure `PATH` automatically.** `--add-path` appends the
  bin directory to the profile for the detected shell (bash, zsh, fish,
  nushell, csh/tcsh, ksh) and is idempotent across re-runs via a marker
  comment. `--link` additionally symlinks the binary into a system bin
  directory (default `/usr/local/bin`) so no `PATH` change is needed at all.
  Both are opt-in; the previous print-only behaviour remains the default
  because `curl | bash` should not silently rewrite dotfiles. The printed
  instructions now include nushell syntax, which was previously missing.
- Deleted `ui_polish.rs` (480 lines). It was never declared as a module, so it
  had never been compiled, and nothing referenced it. Its palette, easing,
  transition and effects were all already superseded by `alphacode_tui_style`
  with theme-aware and perceptually-correct implementations.

## [1.0.66] - 2026-09-30

### Performance

- **Streaming markdown no longer deep-clones the whole rendered line tree twice
  per frame.** `IncrementalMarkdownRenderer` kept a second `Vec<Line>` mirror
  alongside its `Arc`, populated by a full deep clone on every re-render — and
  the re-render runs on *every streaming frame*, which is exactly where the line
  tree is largest. The mirror existed only to serve `last_lines()`, which had no
  callers. The renderer is now the single owner of its lines, `last_lines()`
  borrows from the `Arc`, and one full copy of the tree per frame (plus a
  permanently retained duplicate) is gone. This was the clearest
  cache-miss/clone defect in the TUI render path.
- **Composer undo history is bounded by bytes, not just by count.** Each
  snapshot is a full copy of the composer, so `INPUT_UNDO_LIMIT = 128` never
  bounded memory: one 3 MB paste followed by 128 keystrokes retained ~384 MB.
  Eviction is now oldest-first under an 8 MB budget, which keeps ordinary
  typing at the full 128 levels while capping the pathological case. The stack
  also became a `VecDeque`, so the per-keystroke eviction is O(1) instead of an
  O(128) `Vec::remove(0)` shift — the case that happens constantly during
  sustained typing. The retained byte total is now maintained incrementally,
  so eviction and `debug-profile` no longer re-sum the stack.
- **Provider request fingerprinting serializes each payload once instead of
  twice.** `log_provider_canonical_input` asked for a hash and a character count
  of the same value, and each answer ran its own `serde_json::to_string`. That
  value is a whole provider request — message array plus every tool schema — so
  the request, system prompt, and tool definitions were each serialized twice,
  on every API call, for every provider, to fill diagnostic fields. Both answers
  now come from one serialization, with identical output and identical
  error-fallback behaviour.
- **`bash` captures bounded output instead of buffering the whole pipe.** The
  command's stdout and stderr were read with `read_to_string` and only truncated
  to 30 KB afterwards, so a chatty command (`cargo build` on a cold target dir,
  `rg` across `target/`, a verbose test run) held every byte it produced, in both
  streams simultaneously, before the cap applied — unbounded memory for output
  that could never reach the model. The reader now keeps a bounded prefix and
  keeps draining to EOF. Draining past the cap is required, not an
  optimisation: stopping the read would fill the pipe buffer and block the child
  forever. The cap sits above the truncation point so `smart_stream` still sees
  the same input it used to. The detached/unix path, which read an entire
  background output file, is bounded the same way.

### Fixed

- **The transcript could not be scrolled up while the agent was working — the
  reported "I cannot scroll up" bug.** Two independent causes, both in the
  scroll path:
  - `handle_prompt_history_navigation` claimed an *unmodified* `Up`/`Down`
    whenever the composer was empty and the session had any prior prompt, and
    returned `true`, so the scroll handler was never reached. The composer is
    empty for essentially the whole time a turn runs, which is exactly when a
    reader wants to scroll back: pressing `Up` silently recalled a prompt into
    the composer instead of moving the view. An unmodified arrow on an empty
    composer now scrolls when there is somewhere to scroll, and still recalls
    history when the transcript is not scrollable. Deliberately narrow: a
    non-empty draft keeps walking history with the arrows, and every explicit
    recall chord (`Ctrl`/`Alt`/`Cmd` + `Up`/`Down`) is unchanged.
  - `scroll_up` clamped the new offset to `ui::last_max_scroll()`, the
    renderer's extent *as of the previous frame*. While the agent is streaming
    that value trails text that has already been appended, so the first
    scroll-up from the bottom was clamped straight back down to the bottom of
    the visible frame and the keystroke moved nothing. `scroll_down` already
    had streaming-aware ceiling logic; `scroll_up` did not. Both now share
    `chat_scroll_ceiling`, which only trusts the renderer's extent once the
    transcript is quiescent.
- **A leftover `PROBE` debug print was writing to stderr on every animation
  frame.** `render_idle_animation` contained an unconditional
  `eprintln!("PROBE render_idle_animation ...")`, on both the full-frame path
  and the animation-only partial repaint. At the decorative animation cadence
  that is up to 30 unbuffered, locked, `{:?}`-formatted stderr writes per second
  for the lifetime of an idle session, interleaving with the alternate screen.
- **`debug_repaint_probe_temp` was a test that always failed.** It duplicated
  the real `partial_repaint_matches_a_full_frame_at_the_same_animation_time`
  test above it and ended in `panic!("probe done")`, so it could never pass.
  Removed; the real test immediately above it covers the same behaviour with
  proper assertions.
- Duplicated doc/line comments left by an earlier merge on `draw_idle_animation`.
- **`app::tests::swarm_plan_graph_inline` was a guaranteed-red block on every
  default build.** All 29 tests in that file assert on the mermaid
  `ACTIVE_DIAGRAMS` registry, so each needs a plan-graph message to actually
  render a diagram — but diagram rendering is `#[cfg(feature =
  "mermaid-renderer")]`, and that feature is deliberately not in `default`. On a
  default build the seed helper got 0 active diagrams instead of 1 and the file
  failed. CI runs `cargo test --lib` with default features, so this was red on
  every build. The include is now gated on the feature, matching the pattern
  already used in `alphacode_tui_markdown::markdown_tests::cases::placeholders`.
  Run the full mermaid suite with
  `cargo test --lib --features mermaid-renderer swarm_plan_graph`.

### Changed

- `tests/perf_gates.rs` is now run in CI, and rewritten around
  hardware-independent **complexity gates**: each operation is measured at 400
  and 800 rows and the cost must not grow more than 2.5x when the input doubles.
  These fail identically on a laptop and on a shared CI runner, and they catch
  the regressions that actually occur — a lost cache, a `partition_point` that
  became a linear scan, a reintroduced `O(n^2)`. The previous wall-clock budgets
  (4 ms paint, 2 ms facet toggle, …) were tuned on one dev machine and are now
  measured and printed on every run but only *enforced* under
  `ALPHACODE_PERF_ABSOLUTE=1`. They were never executed in CI at all, so they
  could not have protected anything; a perf gate that fails for reasons
  unrelated to the code is worse than none, because it teaches reviewers to
  re-run instead of read.

### Added

- `alphacode bugbounty {doctor,install,list}` — first-class, cross-platform
  management of the recon toolchain. `doctor` reports present/missing binaries
  with the exact install command for each; `install` resolves a structured
  install command per binary, verifies the required package manager, runs the
  install under a hard timeout with `kill_on_drop`, and re-probes to confirm.
  Installation is never implicit — the agent reports missing tools and the
  command, a human runs the install.
- The doctor and installer now resolve `~/go/bin` (`%USERPROFILE%\go\bin`),
  which is not on `PATH` by default on Windows and often not on Unix. A
  successful `go install` was previously reported as "still missing".
- The recon tools now surface a concrete `alphacode bugbounty install <tool>`
  hint when a binary cannot be spawned, instead of a bare "not found".

### Fixed

- **Command-risk gate bypasses** (each classified `Safe` and would have run):
  - `LANG=C rm -rf ~` — a leading `VAR=value` assignment was mistaken for the
    program name.
  - `powershell -Command "Remove-Item -Recurse -Force $env:USERPROFILE"` and
    `cmd.exe /C "del /f /q ..."` — the Windows shells were not recognised as
    opaque script carriers, and the tool schema actively instructs the model
    to use them.
  - `rm -rf /c/Users/<user>`, `rm -rf C:/Windows` — the protected-path set was
    POSIX-only, so every absolute Windows path was unprotected, including the
    Git-Bash `/c/...` drive mount.
  - `rm -rf ~/.ssh/id_*` — a glob whose parent is protected was only escalated
    to the `Confirm` tier, which is allowed to execute.
  - `eval "rm -rf ~"` — `eval` was treated as a transparent wrapper, so the
    quoted script became the "program name".
  - `chroot /tmp/jail rm -rf ~`, `xargs -I {} rm -rf ~`, `su - root -c "..."` —
    the wrapper-unwrapper stops at any option value it does not recognise and
    left a non-program in the first slot, hiding the destructive verb.
  - Windows `basename` only split on `/`, so `C:\...\cmd.exe` never resolved.
- **Recon tools invoked flags that do not exist**, so they exited 2 on every
  call: `subfinder -threads` (it is `-t`), `dnsx -target` (it is `-d`),
  `waybackurls -limit` / `-no-color` (neither is registered at all).
- `httpx -status-code` is a *boolean probe*, so `-status-code 200,404` applied
  no filter and made httpx probe a host literally named `200,404`. It now uses
  `-match-code`. Also replaced the non-existent `-response-size`, `-tls` and
  `-chains` with `-content-length`, `-tls-grab` and `-include-chain`.
- `katana`: `-threads`, `-robots`, `-no-remote`, `-no-store`,
  `-include-body` and `-include-params` are not katana flags, and the `d`
  parameter collided with `-depth` (an int) while silently overwriting it.
  Replaced with `-c`, `-kf all`, `-crawl-scope`, and the correct inverses
  `-ob` / `-iqp`. The meaningless `headers: bool` became a real
  `Name: value` list.
- `waybackurls` `limit` was only echoed into metadata and never enforced; it
  is now applied locally and the true total is always reported.
- Every recon tool ran `.output()` with no timeout and no `kill_on_drop`, so a
  network stall pinned the agent turn forever and left orphans. All are now
  bounded and killed on expiry. `scrapling` additionally blocked a tokio
  worker with a synchronous, unbounded `python --version` probe.
- Recon output was buffered without a cap; a large `gau`/`ffuf`/`katana` run
  materialised hundreds of MB. Output is now line- and byte-capped, and
  truncation is reported explicitly so a capped result is never mistaken for a
  complete one.
- Failure messages were empty for most tools because goflags writes flag errors
  to **stdout**; `subfinder`, `httpx`, `katana`, `dnsx`, `ffuf` and `gau` now
  report a real diagnostic.
- Domain/target inputs are validated: a value beginning with `-` (e.g. `-t`,
  `-dates`) was silently parsed as a flag by the target tool, changing what ran
  without error.
- `webfetch` SSRF guard only understood four-part dotted-decimal IPs, so
  `127.1`, `0x7f.0.0.1`, `2130706433`, `0177.0.0.1`, `[::1]` and
  `::ffff:127.0.0.1` all reached internal services. It now parses every
  spelling the resolver accepts, and additionally resolves the hostname and
  rejects any address in a non-public range.
- `scrapling` reported "anti-bot detected" for any page whose text merely
  mentioned Cloudflare or captcha, burning a headless browser and stamping a
  false "content may be partial" note on real content.
- `jwt forge` with `alg: none` emitted `header.payload.` (trailing dot), which
  every strict verifier — including this tool's own decoder — rejects. The
  JWS unsecured form has no trailing dot.
- `jwt crack` silently fell back to 10 built-in secrets when the wordlist was
  unreadable and reported the result as authoritative; the degraded path is now
  called out so a failed crack is not mistaken for a strong secret.
- `timeout: 0` in `httpflow` / `webfetch` / `scrapling` was passed through as
  `Duration::from_secs(0)`, failing every request instantly.
- The `shell_url_safety` detector existed but was never called, so the Windows
  `&`-in-URL mangling warning it was written for was never produced.
- `bugbounty_doctor` was dead code and skipped relative `PATH` entries, reported
  non-executable files as installed, and missed `.cmd`/`.bat` shims.

### Fixed (security reasoning core)

- `mul_div_floor` divided by `d` twice, giving a 10,000× error in the LTV
  invariant. A fully-collateralised lending pool was reported as
  `DebtLteCollateralTimesLtv` violated — a permanent false positive.
- `evaluate_gates` compared only the *count* of gates, never that they were
  distinct, so twelve copies of one easy gate passed the entire 12-gate web3
  barrier.
- `should_stop` used `&&` where `||` belonged, so cost could never
  independently trigger a stop; any action with gain ≥ 0.25 continued
  regardless of cost. `ResourceLedger::pressure` also ignored the wall-clock
  budget that `exhausted()` enforces.
- `gate_security_relevance` failed *every* `VulnerabilityClass::Custom`, but
  web3 findings arrive exclusively as `Custom` — so no web3 finding could ever
  reach `Certain` confidence.
- `gate_reportability` never consulted the live scope verdict, so a finding on
  an explicitly excluded host passed all seven gates and landed at `Certain`.
- `confirmed_findings()` silently dropped every finding once it advanced to
  the `Report` stage, emptying the chain-analysis and report paths.
- `FalsePositiveDefense::is_still_viable` ignored whether any negative test had
  been run, so an untested finding read as maximally viable and an
  unexercised defense was promoted straight to `VerifiedReportable`.
- `LiveScope::set_verdict(InScope)` could not clear a sticky `out_of_scope`
  entry, permanently blocking a host re-confirmed as in scope.
- `CoverageTracker` left a stale `vuln_classes_with_signal` after a re-test
  refuted a finding, permanently recommending the endpoint for deeper testing.
- `SkillRouter::route` ignored host- and subdomain-level technologies, so the
  entire web3 routing path was skipped for exactly the targets most likely to
  need it.
- `Evidence::redacted()` did not redact `EvidenceData::Text` or `Binary`, so
  live credentials in raw HTTP transcripts shipped in exported reports. URL
  query redaction was also case-sensitive (`?Token=` slipped through).
- `VerifierVerdict::decide` made `InsufficientEvidence` unreachable and mapped
  "could not reproduce" to `Contradicted`, a hard refutation.
- `Action::ranked` and `HypothesisSet::most_urgent` iterated a `HashMap` with
  no tie-break, so they returned different results run-to-run despite
  documenting themselves as deterministic.

## [1.0.64] - 2026-09-26

### Fixed

- Prevented the health reporter from aborting AlphaCode on freshly booted systems when it constructed a 30-minute `Instant` cutoff.
- Hardened related resource-history and OpenRouter version-cache time handling against monotonic-clock underflow.
- Stopped the OpenRouter client version cache from leaking a new static string on every request and preserved the last valid version after a failed refresh.
- Fixed Firefox browser-bridge evaluation so `return <expr>`, top-level `await`, and legacy bridge calls are supported; upgraded the bundled extension to 1.6.0, moved installation to a versioned XPI path to avoid Windows file locks, and strengthened readiness diagnostics.
- Hardened reconnect bootstrap so a successful socket is not treated as a successful session attach until `SessionId`, `History`, and `Done` are correlated; added bounded timeouts, delayed/missing-session fault coverage, and terminal handling for invalid session targets.
- Fixed Windows named-pipe protocol flushing and added a detached server-reload handoff that waits for the predecessor before binding, preserving reload markers and recovering from dead predecessors.
- Added conservative tool execution classes and bounded concurrent execution for consecutive read-only batch calls while keeping mutations ordered.
- Added gateway bind/accept retry supervision and live runtime state reporting.
- Synchronized the committed lockfile with AlphaCode 1.0.63 so locked builds start reliably.

### Changed

- Pinned CI test/lint jobs to the repository's Rust 1.94.1 toolchain and enabled locked dependency resolution.
- Made release artifacts use the committed lockfile instead of mutating dependencies during publication.
- Added a product-wide 100× reliability, performance, accuracy, UX, tools, skills, security, and quality execution plan.

## [1.0.60] - 2026-09-23

- Initial release.
