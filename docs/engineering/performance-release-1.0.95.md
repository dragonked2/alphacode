# Version 1.0.95 performance and reliability audit

## Scope and changes

This is a focused improvement pass, not a claim that every function is optimal.
Existing in-flight typing/paste changes and bundled hunting-intelligence skill
work are included in the release snapshot.

- Swarm rate-limit recovery now continues gradually to the configured worker
  limit. Previously it recovered once, then remained permanently throttled.
- Swarm RPC routing borrows only the event type and request id, avoiding a JSON
  value tree for every discarded broadcast. Typed responses still deserialize
  through the normal protocol, including escaped event names.
- Swarm ancestry permission checks walk borrowed parent ids with a bounded edge
  count, avoiding string clones and temporary ancestor/visited collections.
  Missing parents and cycles terminate without granting unrelated ownership.
- Swarm detail summaries normalize only the needed prefix rather than splitting,
  allocating and joining the entire worker output.
- Named DragonMeta GLM/Alpha endpoints retain effort selection without
  inventing variants for unrelated GLM routes. Remote OpenAI Pro model
  ladders match the restricted ladders in the direct provider runtime.
- The model widget shows low/none effort explicitly. Swarm card clipping uses
  grapheme clusters and terminal-cell widths, preserving joined emoji.
- Default test harness optimization matches CI (level 0); optimized
  dependencies remain shared, and performance gates still run in release mode.
- ZIP extraction reuses the existing flate2 Rust backend rather than enabling
  Zopfli and an additional zlib-rs backend.
- HTTP stream-cancellation tests accept both EOF and connection reset, the
  two valid TCP close outcomes, while retaining their timeout/error checks.
- Catalog refresh-expiry tests use an explicit future clock instead of
  subtracting beyond a fresh Windows runner's clock origin.
- The Windows Go reproduction test compiles separately with a 120-second build
  budget and retains a 30-second execution budget. A cold toolchain compilation
  no longer gets mistaken for runtime blocking.

## Model selection and paste responsiveness follow-up

The pinned Crossterm Windows VT-input patch (upstream PR #1030, commit
`e2bc49e52a0f5813f5c15ceaa37b673b6b8414a7`) decodes Windows Terminal's native
paste framing into `Event::Paste`. Windows Terminal's own Ctrl+V paste binding
is preserved; no terminal-wide keyboard configuration changes are made. The
patch coordinates VT input, bracketed paste and mouse capture, and restores
console-mode bits on cleanup. Dependency tests cover the Windows parser and
mode transformations. Source builds must honor the root Cargo patch/lockfile;
this release is published as GitHub binaries, not a crates.io upload.

Reference: https://github.com/crossterm-rs/crossterm/pull/1030


- Ordinary terminal keys now pass through immediately. The old stream read ahead
  across key bursts and used 35/40 ms settling windows to infer multiline paste.
  That inference is removed: explicit Ctrl+V/Alt+V use the shared asynchronous
  clipboard action, and terminal bracketed pastes remain atomic Event::Paste
  payloads. Typing speed no longer changes the meaning of input.
- A deterministic stream regression fails if delivering the first character or
  Enter polls for a second event. Large bracketed-paste and shortcut tests cover
  atomic payloads without depending on wall-clock thresholds. Integration
  coverage preserves one compact paste draft in local, remote and streaming modes.
- Simplified and stale remote catalogs now classify the bundled automatic alias
  as the direct Alphax Free profile before effort expansion. It no longer becomes
  an OpenRouter/auto row with invented effort variants. Selection feedback uses
  the product name instead of a decorated internal route id.
- Closing a picker discards pending catalog results. Normal typing after dismissal
  does not reopen the picker or adopt a late load.
- Free-pool display lookups inspect borrowed membership rather than cloning and
  deduplicating the full pool. The model widget caches pool usage totals until
  usage or pool membership changes, avoiding a sorted lifetime ledger rebuild on
  every redraw. Regression coverage checks both invalidation paths.

## Build investigation

GitHub release run 41940302966 (v1.0.94): Windows x64 build step 10m15s,
Windows ARM64 16m04s, macOS Intel 25m13s, macOS ARM64 cross-build 38m38s.
The macOS ARM64 log explicitly reported a cache miss. Alpine builds were cold
Docker builds without persisted Cargo registry/artifacts.

GitHub caches are immutable and scoped to the current/default branch; one tag
cannot restore another tag's caches. Previously CI and release used different
keys, target-directory layouts and RUSTFLAGS, so even main-branch caches did not
provide compatible artifacts. CI now uses the same explicit native targets,
flags, runner generation and revision-aware cache keys as releases. Main-branch
builds seed caches accessible to tags, and source revisions refresh them.
The application itself is also a large compilation unit: 1,176 Rust source
files and about 719,000 lines under src compile as one library package. Its
release optimization and test linking remain substantial even with warm
dependency caches; deleting foreign-target lockfile entries cannot fix that.

Alpine persists its registry/artifacts through the container mount and primes
main-branch caches on dependency/build-configuration changes.

References:
- https://github.com/dragonked2/alphacode/actions/runs/41940302966
- https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching

Default normal/build dependency graph counts from cargo tree --locked,
deduplicated by package/version (not build units or lockfile entries):

| Target | Before | After |
| --- | ---: | ---: |
| Windows x64 MSVC | 408 | 398 |
| Linux x64 GNU | 450 | 440 |
| macOS ARM64 | 414 | 404 |

Platform counts legitimately differ: Linux uses Wayland/X11/accessibility stacks;
macOS uses Objective-C/CoreGraphics; Windows uses Windows bindings. Optional AWS,
ONNX, PDF and email stacks remain opt-in. No platform functionality was removed
to force equal counts. Cargo.lock includes optional and foreign-target packages
that are not necessarily compiled into any particular binary.

## Isolated summary benchmark

Optimized standalone Rust, MSVC, 200 calls summarizing 1 MB to 80 characters:

| Input | Before total | After total |
| --- | ---: | ---: |
| Repeated words | 666,794 us | 84 us |
| One long token | 145,484 us | 86 us |

Source functions were extracted from HEAD and the working tree and compiled
with rustc -O; results were consumed with black_box. 100,000 short Unicode and
whitespace equivalence cases passed. These numbers describe this helper on this
host, not model token throughput or a whole-application speed multiplier.
Whitespace-only prefixes/suffixes can still require scanning to determine the
normalized result. Actual CI build-time improvements need subsequent runs with
populated main-branch caches; the first build of changed dependencies stays cold.

## Verification

Local verification runs formatting, workspace checks, test compilation,
library tests and Clippy with warnings denied. The workflow files pass
Actionlint and the Alpine build script passes bash syntax validation.
The release workflow additionally requires green main-branch CI for the exact
release source revision before publishing. Final platform results are available
in GitHub Actions for the release commit; the microbenchmark above is local.
Live provider throughput, visual terminal behavior on every terminal emulator,
and optional integrations are not exhaustively benchmarked by this pass.
