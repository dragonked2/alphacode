# Connection and streaming runtime hardening

This change targets specific local overhead and failure modes. It does not claim
an end-to-end 100x speedup. Model generation, provider queues and external tool
runtime remain separate costs. Builds, tests and benchmarks were skipped at the
user's request; source review and formatting are not runtime verification.

## Request delivery

Normal TUI request writes now have a five-second deadline covering writer-lock
wait, write and flush. Detached control writes retain their two-second deadline
and now flush consistently. Both paths use a cancellation guard after taking the
writer lock: a failed or dropped write invalidates the connection before another
request can append bytes to a potentially torn JSON frame.

The read path notices the invalid connection and enters normal reconnection.
A failed message send holds automatic queued dispatch. An uncertain transport
write also holds sends across reconnection, preserving the existing recovery
mechanisms while requiring inspection of synchronized history before resubmission.
A successful write is not proof of server execution. No exactly-once delivery or
automatic idempotency is claimed.

Client and server share the eight-MiB request-frame limit, including newline.
Oversized prompts/attachments are rejected before transport writes rather than
forcing the server to drain a doomed frame. A rejected queued send is held
instead of being serialized and retried on each UI tick.

Writes taking at least 250 ms emit a `REMOTE_WRITE_SLOW` record with request ID,
encoded byte count, writer wait, write and total time. Payload text is not logged
by this instrumentation. These records can support later bottleneck measurement.

## Read path and memory

The client consumes only through the next newline, so a following frame cannot
cause a valid near-limit frame to be rejected. Cancellation-safe persistent reads
and the existing 256-MiB response-frame cap remain in place.

For history frames larger than 256 KiB, the receive allocation moves directly to
the parser. Only a following tail is copied, rather than duplicating the entire
frame during extraction. A small buffer remains available for subsequent events.
Normal small frames keep their existing extraction path. Malformed/EOF diagnostic
previews inspect at most 1 KiB before conversion, avoiding whole-frame lossy UTF-8
allocations for a short log preview.

## Reconnection

Windows named-pipe busy polling starts at five milliseconds and backs off to
50 milliseconds within the same three-second maximum wait. Short pipe handoff
windows can recover without always paying a fixed 50-ms sleep. Other OS errors
still reach the outer reconnect logic immediately.

Outer reconnect sleeps retain the existing base schedule, attempt limits and
30-second cap, with bounded 80–100 percent per-client variation to reduce
synchronized retries after daemon restarts. Session identity and bootstrap/history
validation remain required before a connection is considered established.

## Streaming and contention

Queued stream chunks cache their remaining character count and retain a byte
cursor. Each reveal decodes only its next prefix. Consumed prefixes compact only
when at least 64 KiB and at least half the retained string have been consumed.
This removes per-frame recounts and repeated whole-tail shifts, replacing their
quadratic work on large coalesced chunks with amortized linear processing.
Original text, character-based pacing, reasoning markers and flush ordering are
preserved by design; equivalence has not been tested in this pass.

The server event forwarder acquires the shared connection-presence write lock
only for events that mutate presence/tool state. Ordinary token and usage events
no longer take that global write lock. Failed-write diagnostics no longer format
an entire large event before clipping the resulting string.

## Follow-up measurement

Compare equivalent sessions and model/provider settings. Measure connection and
history-attach latency, first-token and completed-response latency, UI frame time,
CPU time, peak resident memory, reconnect outcomes and duplicate/missing turns.
Include slow providers, large Unicode responses, compacted history, images,
interrupted writes and concurrent clients. Record actual results before claiming
a numerical improvement or changing provider limits/timeouts further.
