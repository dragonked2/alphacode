# Changelog

All notable changes to Alphacode are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
  ships `default_model = "kilo-auto/free"` and `default_provider = "alphax-free"`,
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
  compact notifications, pinned todos, centered output, agentgrep output, and
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
