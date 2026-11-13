# TUI usability and tool recovery

This change focuses on discoverability, accurate controls, focus ownership,
transport diagnostics, and tail-latency visibility. It does not establish a
10/10 product rating or comparative superiority.

## Help and input

- F1 help starts with a short orientation instead of requiring users to scan
  the entire command reference.
- `/` or Ctrl+F inside help opens a case-insensitive, multi-word filter over
  command names, descriptions, and keyboard actions. Filtering happens before
  line wrapping so narrow terminals retain the same matches.
- The search row remains visible while results scroll. It has a real caret,
  grapheme-aware editing, Home/End, Delete/Backspace, and Ctrl+U. Query length
  is bounded to 256 characters. Native bracketed paste goes to the query,
  not the composer or file-attachment path.
- Enter leaves query editing while retaining the filter. Esc clears search,
  then closes help on a subsequent press. F1 closes directly. Opening help
  resets stale search state and preserves the draft and its cursor.
- Page movement follows the current viewport with one row of overlap, including
  after a resize. Empty results explain how to recover. Small layouts retain
  an Escape hint.
- Remote F1 respects existing modal focus, matching local behavior. It cannot
  cover an active transcript search while leaving keyboard input routed beneath
  the help surface.
- Help, the keymap overview, and shortcut feedback no longer advertise the
  retired queue toggle or an opposite Enter mode. Ctrl+A is described as moving
  to the input start. Busy-composer hints shrink to keep the stop action visible.

## Latency diagnostics

The existing Windows console-reader timestamp and bounded ordered event queue
remain intact. The oldest pending input is still measured through frame flush,
including time spent in the application's queue.

The bounded sample history now retains 1,024 input-bearing frames using a deque.
`draw-stats` reports nearest-rank p50, p95, p99, maximum, capacity, sample unit,
and whether at least 100 samples exist for the p99 warmup indicator. These are
frame samples carrying input, not an independent sample for every coalesced key.
The warmup indicator is not a statistical confidence guarantee.

Live latency evaluation must capture the same Windows Terminal version, font,
viewport, theme, hardware, and build while idle, after selecting a model, and
during streaming. Report median and tail values, sample count, input order/loss,
and comparison to the starting revision. No live latency improvement is claimed
from diagnostic changes alone.

## webfetch TLS failures

An HTTPS IP-address fetch can fail certificate hostname validation while
`curl -k` succeeds. This distinguishes reachability with validation disabled
from a verified server identity; it does not by itself identify an HTTP/2 or
proxy defect.

`webfetch` now exposes its nested transport error instead of only an opaque
request failure. Changing User-Agent is not retried after transport retries
have failed, since it cannot repair a TLS, DNS, or proxy handshake.

Certificate verification remains enabled by default. Explicit JSON boolean
`insecure: true` selects a separate cached client for the requested HTTPS origin,
equivalent to opting into curl's `-k` behavior. It does not change the verified
client or carry the bypass to another redirect origin. SSRF and redirect-scope
checks still run. Both output text and metadata identify if any HTTPS hop was
unverified. Client-construction fallback also retains disabled automatic
redirects rather than falling back to an unrestricted client.

For an intended unverified inspection:

```json
{"url":"https://your-authorized-host/","insecure":true}
```

The explicit live diagnostic test uses `ALPHACODE_WEBFETCH_TEST_URL` and is
ignored by default. It checks verified failure, opt-out success with labeling,
and verified failure again to prove the default client was not weakened.

## Background cancellation compatibility

`bg` accepts `action: "kill"` as an alias for `cancel`, including in the schema.
The existing task selection, session boundaries, cancellation grace period,
and result metadata remain those of `cancel`. This does not add arbitrary PID
termination or a new force-kill policy.

## Verification boundaries

Regression coverage exercises actual render buffers, narrow/wide layouts,
Unicode query editing, modal focus, draft preservation, search recovery,
viewport paging, latency summary math, TLS opt-in/origin boundaries and output
labeling, and the background action alias.

Optional `ALPHACODE_TUI_REVIEW_DIR` exports the asserted render cells as JSON for
offscreen visual review. These fixtures are not a live terminal screenshot.
Live inspection was unavailable: the app exposed no targetable window and the
Computer Use policy blocked launching Windows Terminal. Actual terminal font
shaping, screen-reader behavior, and reader-to-flush latency remain unmeasured.
