# Swarm, tool output, and completion-evidence audit — 2026-11-01

## Changes

- Plan broadcasts read their snapshot after resolving routing and retain the plan guard through synchronous delivery. An older version cannot overtake a newer mutation while waiting for routing or member locks. Reconnect snapshots retain the same guard through delivery.
- Status broadcasts retain the member snapshot guard through delivery. Removed one write-lock acquisition per recipient, avoiding the snapshot/send race and repeated attachment-map pruning on this hot path.
- Shared snapshot delivery reaches all live session attachments, falling back to the primary sender when restored attachment channels are closed.
- Plans whose participants all have dead channels fall back to current swarm membership. Participants are sorted and deduplicated.
- Batch results retain image attachments in original call order, including when read-only tools complete concurrently. Previously only text survived aggregation.
- Completion evidence joins tool calls and results chronologically and consumes each matching call once. Orphan results, future calls, and duplicate results no longer manufacture evidence.
- Test and command success evidence requires a successful bash tool result with a command argument. File reads and errored tools cannot supply execution evidence.
- File-modification verification requires both an existing regular file and a successful write/edit/multiedit to the same canonical path in this session. File existence alone no longer verifies a modification. Shell-based mutations are conservatively unverified without structured mutation evidence.
- Commands must match the observed command; test names match exactly or at a Rust module boundary. Arbitrary substrings cannot verify a different command or test.
- Status connection counts exclude closed attachment channels without mutating the attachment map.
- Test evidence requires an individual `test name ... ok` line; summary text no longer invents a test named `result`. Exit-code evidence uses a complete numeric marker line and the last observed exit code.

## Regression coverage

Updated the existing deterministic tests that previously asserted plan inversion and dead-participant starvation. Added live/closed attachment coverage and chronological/duplicate/error/quoted-output evidence regressions. Re-enabled the status ordering fixture with an ordering invariant independent of the exact scheduler yield point. Extended the bounded-parallel batch test to verify image retention and order.

## Performance and limits

Evidence reconstruction now traverses the transcript once and releases matched tool arguments as results are processed, rather than retaining cloned arguments for the entire transcript.

Status delivery now uses one member read acquisition rather than one read plus a write acquisition for every recipient. No model tokens-per-second or end-to-end latency multiplier is claimed. Sends are synchronous unbounded-channel enqueues; guards remain held during cloning and enqueueing, not during network I/O. The existing bounded read-only scheduler and serial mutation barriers remain intact.

This pass targets server swarm snapshots, batch output, and completion-evidence reconstruction. It does not establish exhaustive correctness of every subsystem. Text-based execution evidence is not a cryptographic execution attestation; fabricated shell output can still imitate a real runner. Successful mutation-tool evidence establishes a write operation, not a content-diff or rollback provenance chain. Live provider throughput and network outage behavior need separate integration measurements.

## Validation

Final working-tree validation completed successfully:

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo check --workspace --tests`
- `cargo clippy --lib -- -D warnings`
- Focused swarm, batch, and claim-checker regressions: **66 passed**, zero failures.
- `cargo test --lib -- --quiet`: **6,360 passed, zero failures, 11 ignored**, 236.64 seconds.
- Whitespace check on changed source files passed.

Windows builds used the VS 2022 MSVC environment and `CARGO_TARGET_DIR=C:\Users\or0to\AppData\Local\Temp\alphacode-target`, outside OneDrive. Existing in-flight changes were preserved.
