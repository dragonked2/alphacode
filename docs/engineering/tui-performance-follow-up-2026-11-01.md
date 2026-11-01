# TUI, streaming and stability follow-up — 2026-11-01

This audit targets repeat rendering work, mouse/editor consistency, long-history arithmetic, and fragmented network events. The measured gains below apply to isolated paths; they are not whole-application or model tokens-per-second multipliers.

## Changes and evidence

| Issue | Trigger and correction |
| --- | --- |
| Repeated Unicode layout work | Height measurement constructed styled rows and scanned the caret, then drawing and clicking recomputed grapheme segments. Height measurement now reads the segment count. A thread-local cache shares immutable geometry across these paths, keyed by exact draft text and width. It retains one entry, limited to 256 KiB of input and 4,096 wrapped rows; oversized inputs evict it. Cursor/style changes do not require new segmentation. |
| Clicking the wrong draft line | Moving the caret upward in a draft longer than its viewport changed the painted scroll offset, while mouse mapping always assumed the final rows were visible. Both now use the same caret-based viewport calculation. |
| Hidden hint and border hit-testing | A one-row editor could reserve an invisible hint during click mapping, and its bottom border was treated as text. Mapping now follows the actual rendered hint height and rejects border/collapsed rows. |
| Focus guidance described the wrong keys | The composer advertised submission while keyboard focus belonged to a side pane. Guidance now shows scrolling, Esc to return, and help while side/diagram panes have focus. |
| Large branding competed with work | Active and existing conversations now keep a single brand row. A fresh empty welcome screen retains the existing wordmark tier. The compact height remains after a conversation finishes, avoiding a decorative expansion at the end of each turn. |
| Long histories could overflow layout arithmetic | Transcript line counts were cast to `u16` before scrollbar/packed-layout decisions, wrapping above 65,535 rows and potentially overflowing when adding chrome. Decisions and diagnostics now retain `usize` counts; conversion happens only for on-screen dimensions. Boundary tests include 65,536 rows and saturated sizes. |
| Zero-size resize retained old geometry | The early return now clears layout/status and copy-selection snapshots instead of leaving the previous frame's hit-testing geometry active. |
| Fragmented SSE events were repeatedly rescanned | A partial event rescanned all previous bytes on every network refill. A separate scan cursor retains only the three-byte overlap needed for split LF/CRLF delimiters. Compaction and proxy recovery update that cursor. Scanning uses bytes so overlap positions inside UTF-8 sequences cannot panic. |

## Isolated measurements

Optimized standalone Rust executables extracted the current input-layout and event-framing functions from the working source. Unicode layout uses the repository's already-built Unicode dependencies. Both paths verify equivalent returned geometry/events. Measurements are local, hardware/load dependent, and intentionally exclude terminal painting, JSON decoding, network RTT and model inference.

| Input/path | Previous operation | Updated operation | Ratio |
| --- | --- | --- | --- |
| 2,900-byte Unicode draft | 39.508 µs segmentation | 0.069 µs warm lookup | 574.6× |
| 29,000-byte Unicode draft | 503.605 µs segmentation | 0.818 µs warm lookup | 616.0× |
| 10,000-byte event, 32-byte fragments | 615.7 µs framing | 16.7 µs framing | 36.9× |
| 100,000-byte event, 32-byte fragments | 53.201 ms framing | 152.9 µs framing | 347.9× |

Draft timings average 100 uncached computations and 10,000 warm lookups. Editing the text or changing its wrap width still incurs a cache miss. Drawing and copy snapshots continue to do other work; the complete TUI is not 616× faster. Tiny network fragments deliberately expose the prior quadratic scan pattern and are not a claim about typical packet sizes. The event benchmark produces exactly one complete event in both implementations.

## Regression and visual coverage

New cases exercise cache hits, content/width invalidation, eviction, Unicode boundaries, clicking visible draft rows at three caret positions, hidden hints, footer exclusion, pane focus, compact conversation headers and long-history size boundaries. Fragment tests feed 10,000 CJK characters individually, then each LF/CRLF delimiter character separately, and require the original payload unchanged.

The existing 50-frame matrix now includes a populated conversation in its streaming scenario, so the compact header is reviewed with actual message content. Review artifacts are headless TestBackend captures. Live providers, terminal-emulator font/shaping behavior, and end-to-end latency/TPS are not measured by these tests.

## Verification results

- `cargo fmt --all -- --check`: passed; included review tests were also checked directly with Rustfmt.
- `cargo check --workspace`: passed.
- `cargo check --workspace --tests`: passed.
- `cargo clippy --lib -- -D warnings`: passed.
- Focused regressions: **55 passed, 0 failed** (1.78 seconds).
- Full library suite: **6,354 passed, 0 failed, 12 ignored** (217.77 seconds of test execution, excluding compilation).
- The 50-frame matrix was exported and the populated 120-column conversation was visually reviewed, including a separate 256-color render pass. The existing preview helper was corrected to map indexed terminal colors when rendering the captures to PNG.

Builds used the required VS 2022 MSVC environment and external temporary Cargo target directory. The test process used `TERM=xterm-256color` with `NO_COLOR` removed. No live provider request or end-to-end inference benchmark was performed. Existing working-tree changes were preserved.
