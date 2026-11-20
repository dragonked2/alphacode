# Windows 1.1.13 coding evaluation baseline

The Python evaluation runner was removed for 1.1.13. The original runner,
fixtures, and evaluation instructions are available in the
[development revision](https://github.com/dragonked2/alphacode/blob/8775cbaa796057d216ad757c5b2e4becd392a067/docs/engineering/agent-task-evaluation.md).

Both runs used the bundled three-task manifest, a 75-second budget per task,
one repetition, and the Alphax Free route. Windows DNS resolution for that
route failed. A temporary process-local tunnel using a public DNS answer
restored connectivity without changing system DNS. The route can select
different upstream models, so these runs establish a smoke baseline and do
not isolate a release-to-release performance difference.

| Task | v1.1.12 | v1.1.13-dev `a127e1c3` | Candidate dispatches | Candidate reported input / cache read / output |
| --- | --- | --- | ---: | ---: |
| Quantity validation | Timed out at 75 s, edits verified | Passed in 36.26 s | 5 | 44,214 / 33,280 / 1,211 |
| Ordered deduplication | Timed out at 75 s, edits verified | Passed in 49.23 s | 6 | 129,496 / 105,472 / 2,185 |
| Inventory SKU normalization | Passed in 34.59 s | Passed in 61.05 s | 8 | 80,333 / 74,240 / 2,966 |

Manifest SHA-256 `17afb112c001ffbf439434308621ae42a98010f729167a3b576dfe9d26960558`.
Dispatches count client attempts. Cache reads are reported separately because
they may overlap the reported input total. Repeated trials on a fixed model
are needed to compare throughput or request efficiency.

### Release verification scope

- The clean `a127e1c3` Windows checkout passed formatting, workspace compilation
  including test targets, library Clippy with warnings denied, and the Rust
  library suite with 6,986 passed, zero failed, and 17 ignored.
- In three loopback streaming trials, `a127e1c3` sent one POST per trial and
  displayed first text in 254–289 ms from launch. The server supplied scripted
  chunks, so this measures local delivery rather than model generation speed.
  A separate scripted HTTP 429 with a ten-minute cooldown returned after one
  POST.
- The `a127e1c3` offscreen terminal benchmark measured a 2.71-second cold draw
  for 1,000 messages, including 2.62 seconds of body preparation. Local commit
  `3a86f246` passed the regression that checks reuse of the Markdown render.
  Its timing trial did not establish a cold-draw improvement. End-to-end
  Windows Terminal input latency remains unverified.
- The live coding results above use three smoke tasks and a single repetition.
  Matched comparisons with other agents, fixed-model throughput, and release
  workflow verification on the final revision remain open.
