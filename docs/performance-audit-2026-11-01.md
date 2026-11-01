# Streaming and transport audit — 2026-11-01

Scope: provider streaming, terminal reveal, inline worker previews, HTTP connection reuse, and a review of the previous performance audit against current code. This is a focused source audit with an isolated parser benchmark, not a whole-program profile or a live-provider TPS measurement.

## Fixed

| Finding | Change | Evidence |
| --- | --- | --- |
| Terminal reveal throttled received text at 960 characters/second, with a completion tail | Both TUI constructors and continuation resets select immediate reveal. Redraw scheduling still controls terminal paints. The paced buffer remains available for callers explicitly selecting it. | Regression test checks immediate delivery of 100,000 characters and reasoning/answer ordering. |
| SSE extraction searched the whole suffix for absent CRLF delimiters and moved that suffix for each event | Search both delimiter forms together, advance a cursor, and compact on network refill. | Existing LF/CRLF, split UTF-8, EOF and recovery tests plus a new cursor regression. Optimized isolated benchmark below. |
| Prefix deduplication silently removed repeated incremental reasoning tokens | Preserve reasoning deltas exactly as received. | Test feeds `a`, `a`, `abc` and requires all three unchanged. |
| Inline worker previews copied the entire growing response for every delta | Retain only the final 1,400 Unicode characters, bounding work and memory independently of response length. | Unicode-tail regression uses 140,000 four-byte characters. Preview remains separate from canonical response storage. |
| Gemini instances rebuilt clients and pools | Reuse a single lazily initialized Gemini HTTP client, preserving existing transport settings. | Workspace build and library checks. No live handshake benchmark. |

## Parser benchmark

Windows, optimized standalone Rust executable. The new function was extracted directly from the working source; the baseline used the previous function. Every iteration allocated the same returned event string. Each input was a single synthetic LF-delimited chunk containing the stated number of small text events. Both implementations had to return exactly that count.

| Events per chunk | Previous | Updated |
| --- | --- | --- |
| 1,000 | 5.38 ms | 0.076 ms |
| 10,000 | 528 ms | 0.748 ms |
| 20,000 | 2.076 s | 1.507 ms |

These deliberately large batches expose the previous quadratic behavior. They do not represent ordinary network chunk sizes, end-to-end latency, or model generation speed. Timing varies by machine and load. The remaining incomplete SSE event is still rescanned on refill; large delimiter-free fragmented events were not benchmarked.

## Existing optimizations verified

Generic provider requests already share an HTTP client with pooled connections, TCP keepalive, HTTP/2 keepalive and a separate fresh-transport retry path. Both headless and server streaming agent loops already use bounded read-only tool execution plans. Canonical request fingerprinting is gated behind diagnostic settings. Provider-message body eviction preserves prefix hashes; removing it would trade memory for CPU and requires long-session measurements.

## Limits

No live provider was called and no accuracy score, connection recovery target, or overall TPS increase was measured. Immediate reveal can display a large provider burst in one frame; this intentionally removes the artificial typing delay. Model-side inference speed, upstream buffering, quotas and network RTT remain external factors. Existing unrelated working-tree changes were preserved.

## Verification

- `cargo fmt --all -- --check`: passed after formatting the existing working-tree edits as well as the new code.
- `cargo check --workspace`: passed.
- `cargo check --workspace --tests`: passed.
- `cargo test --lib -- --quiet`: 6,333 passed, 0 failed, 12 ignored (271.92 seconds of test execution, excluding compilation).
- `cargo clippy --lib -- -D warnings`: passed.
- Existing redundant closures in skill matching were simplified to satisfy strict Clippy warnings.

The first test run inherited `NO_COLOR=1` and `TERM=dumb`, causing color and animation assertions to fail. Representative failures passed in isolation after removing `NO_COLOR` and setting `TERM=xterm-256color` in the test process. The final full suite used that environment. A new immediate-reveal regression also caught a reasoning-boundary reset; the implementation now drains the frame without applying message-end reset semantics, and the final suite includes the correction. The initial timing failure passed on rerun.

Builds used the VS 2022 MSVC environment and `CARGO_TARGET_DIR=C:\Users\or0to\AppData\Local\Temp\alphacode-target` outside OneDrive.
