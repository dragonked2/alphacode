# AlphaCode 1.0.73 validation

## Baseline and scope

- The checked-out `main`, local `origin/main`, and `v1.0.72` tag resolve to
  `0215ad03134b5509fe9a9ab9de61cb98a5a29a9d`.
- Before this change, `Cargo.toml` and the AlphaCode package entry in
  `Cargo.lock` said `1.0.70`; they now say `1.0.73`. Build metadata reports
  `alphacode v1.0.73-dev` for this dirty working tree.
- No dependency versions changed. No commit, tag, push, or publication was
  created.
- This candidate focuses on local-model context sizing, stream recovery,
  compaction budget alignment, and unsafe default prompt content. It is not a
  full audit of every tool, swarm, TUI, or platform-specific subsystem.

## Severity-ranked findings

1. **High — local requests exceeded the configured server context.** The
   reproduced `hello` request was estimated at 30,465 prefix tokens, including
   30,318 tokens of tool schemas, against the local server's 16,384-token
   context. Trivial turns now omit tools; task turns on direct local models up
   to 32K use a compact prompt and a query-ranked, bounded tool subset.
2. **High — retry/compaction could re-poll a completed request future.** The
   MPSC streaming loop retried from an inner loop after a provider error or
   compaction. A completed future could be polled again and terminate the
   server connection. Recovery now restarts the outer turn; a fail-once mock
   provider covers the retry path.
3. **High — the bundled system prompt contained unsafe scope-bypass rules.** It
   directed blanket authorization, ignored scope and safety boundaries, and
   encouraged harmful actions. It has been replaced by concise engineering
   guidance that preserves authorization scope and treats external/tool
   content as untrusted.
4. **Moderate — active context accounting could remain stale after switching
   models, and catalog training limits could mask a smaller learned local
   runtime window.** Compaction now refreshes its budget from the active
   provider, and a learned direct-local runtime limit takes precedence over
   catalog metadata. A 4,096-versus-131,072 regression test covers the latter.

## Candidate measurements

- Same local `alphax@q4_k_m:2` / LM Studio setup and `hello` reproduction:
  prior failed-run estimate was about 30,465 prefix tokens; candidate estimate
  was 147 prefix tokens, with zero tool schemas. Candidate provider usage was
  355 input and 158 output tokens.
- A task-capable local request to read `Cargo.toml` used a compact prompt with
  16 selected tools and a 3,863-token estimated prefix (798 system + 3,065
  tool tokens). The model read the file, returned version `1.0.73`, and the
  process exited normally. The first model call reported 4,674 input / 94
  output tokens; the final call after the file read reported 9,923 input / 104
  output tokens.
- These are observed single-run token/context measurements. No repeated
  latency, throughput, success-rate, memory, or agent-quality benchmark was
  run, and no before-change latency benchmark was captured. No speedup claim
  is made.

## Verification

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --locked` — passed.
- `cargo check --workspace --tests --locked` — passed.
- `cargo test --lib --locked -- --test-threads=1 --quiet` — 5,661 passed,
  0 failed, 9 ignored.
- `cargo test --locked --test llamacpp_compat -- --test-threads=1` — 8 passed.
- `cargo clippy --lib --locked -- -D warnings` — passed.
- `cargo build --release --locked` — passed; the executable was built under
  `C:\Users\or0to\AppData\Local\Temp\alphacode-target`.
- Release executable local-model smoke runs — greeting and file-read task both
  completed without reconnecting; the file-read run exited with status 0.
- `git diff --check` — passed (Git emitted only its configured LF-to-CRLF
  informational warnings).

The test run used the repository's required external target directory and
MSVC environment. The first suite run reported failures under the inherited
`TERM=dumb`/`NO_COLOR=1` settings, parallel shared-state tests, and two
case-sensitive prompt assertions. The prompt assertions were corrected, and
the full suite passed serially with a Windows terminal profile and `NO_COLOR`
unset.

## Release readiness and remaining checks

The locked Windows release build and the required Rust checks pass. The
candidate has not been packaged into distributable release artifacts, so
installer/updater end-to-end checks and artifact checksum validation remain
unverified. Linux/macOS cross-platform checks, sustained-session/fault tests,
and broad tool, swarm, and TUI audits also remain unverified. Do not describe
this focused candidate as having passed those gates.

## TUI input follow-up and current 1.0.73 verification

The 1.0.73 candidate received a focused composer audit and regression fix on
2026-10-09.

### Severity-ranked findings

1. **Release blocker — no published 1.0.73 source tag exists yet.** The local
   base is `v1.0.72` / `0215ad03134b5509fe9a9ab9de61cb98a5a29a9d`; the remote
   tag listing has no `v1.0.73`. Cargo metadata and the working tree describe a
   local 1.0.73 candidate. Release provenance becomes reproducible only after
   the candidate changes are committed and a `v1.0.73` tag is pushed.
2. **Moderate — composer editing could split a visible Unicode character.**
   Reproduce with `A👨‍👩‍👧‍👦é`: move left and delete around the emoji or the
   combined `é`. Cursor motion and deletion previously used Unicode scalar
   boundaries, while wrapping and cursor columns summed scalar widths. That
   could split a joined emoji, delete only a combining mark, or place the
   terminal cursor several columns away from the visible glyph. Fixed in this
   candidate.

### Implemented correction

- Local, connected-remote, and disconnected-remote input now move and delete by
  extended grapheme clusters. Word movement and session-picker text deletion
  use the same boundary helpers.
- Composer wrapping, mouse cursor placement, copy-selection line maps, and
  cursor columns now count grapheme clusters and their display width together.
- Regression coverage checks joined-emoji and combining-mark movement,
  backspace/delete, index conversion, wrapping, and cursor placement.

### Verification after the TUI follow-up

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --locked` — passed.
- `cargo check --workspace --tests --locked` — passed.
- `cargo test --lib --locked -- --test-threads=1 --quiet` — 5,667 passed,
  0 failed, 9 ignored. This includes the two prompt assertions reported by the
  user and the new grapheme tests.
- `cargo clippy --lib --locked -- -D warnings` — passed.
- `cargo build --release --locked` — passed on Windows/MSVC, with target files
  under `C:\Users\or0to\AppData\Local\Temp\alphacode-target`.
- Release-binary version smoke check — `alphacode v1.0.73-dev (0215ad0, dirty)`.
  The `-dev` and dirty marker are expected until a clean tagged release build.
- No latency or throughput benchmark was run; this input correctness change
  makes no speed claim.

### GitHub release preparation and remaining work

- `Cargo.toml`, the AlphaCode package entry in `Cargo.lock`, and the changelog
  identify this candidate as 1.0.73. The composer fix is listed under the
  1.0.73 changelog section.
- `.github/workflows/release.yml` runs on `v*.*.*` tags, creates a draft GitHub
  release, and builds the platform archives. No commit, tag, push, or release
  was created in this task.
- Packaging, archive checksums, installer/updater end-to-end checks, and
  Linux/macOS builds remain unverified. The broader provider, swarm,
  sustained-session, cross-terminal, and complete TUI audits in the product
  brief remain open. This is a focused 1.0.73 candidate validation, not a claim
  that the whole-product acceptance list is complete.
