# Provider traffic audit for 1.1.7

## What was found and changed

1. Explicit OpenRouter refresh previously requested `/models` and endpoint data
   for up to 24 model IDs: up to **25 HTTP requests per refresh**. It now requests
   the catalog and only the selected model's endpoints: **2 requests** with a
   nonempty selected model (1 when none is selected). A direct compatible profile
   requests **1 catalog**, and profiles without catalog support request **0**.
2. The before/after catalog snapshots could schedule a duplicate background
   catalog request. The explicit attempt is recorded before reading snapshots.
3. Cold prefetch and picker refresh could both fetch the catalog. A shared async
   lock now serializes foreground and background fetches across provider forks;
   background waiters recheck cache freshness after acquiring it. Concurrent cold
   prefetch/picker calls on that shared runtime require **1 catalog request**.
4. `/usage` validated OpenAI, Anthropic, Gemini and ordinary compatible API keys
   with model-list requests, which do not return usage. Those probes now cost
   **0 requests**, and key status says `configured (not checked)`. Balance,
   subscription quota, and admin spend endpoints remain available.

## Counts and their boundaries

There is no fixed number of requests per user prompt or task. One successful
inference round on a warm direct HTTP provider uses **1 HTTP request**. A task
with three model rounds uses **3**, before retries, auxiliary work or metadata.
Several parallel tool calls returned by one model response do not each need
their own inference request; the next round can send their results together.

Loopback regression fixtures assert actual accepted request paths for the
2-request explicit refresh, 1-request concurrent cold catalog, and 1 catalog +
3 successful warm inference rounds. The latter repeatedly reads the picker and
prefetches between rounds without additional catalog traffic. A child-process
integration test compares 4 loopback HTTP sends with 4 audit records and verifies
prompt grouping, worker propagation, and secret/query/path exclusion.

Counts during real tasks also include applicable retries, protocol fallback,
tool-result rounds, continuation/recovery, compaction, memory inference, auth
refresh, and independently triggered catalog/usage work. A WebSocket connection
is a separate handshake; each model message on it is counted separately.
Ping/pong, token chunks and Cursor heartbeat frames are not model requests.

Existing logs contain 635 logged compatible stream attempts over 225 active
calendar minutes (mean 2.82, median 2, peak calendar minute 12, peak rolling minute
14). Those are **historical log attempts, not exact HTTP counts**. They have no
session attribution, so exact historical per-prompt/task totals cannot be
reconstructed. See `provider-request-observations-2026-11-14.json`.

## Background activity that remains

These are conditional reads, not an unconditional requests-per-minute loop:

| Path | Current policy |
| --- | --- |
| Hosted compatible catalog | Refresh on access after 60 minutes; background retries at least 5 minutes apart per runtime |
| Local model catalog | Refresh on access after 15 seconds; background retry gap 5 seconds |
| OpenRouter endpoint metadata | 2-hour freshness; background attempt gap 1 hour per model, at most 8 concurrent |
| Inactive standard OpenRouter catalog | 24-hour freshness, guarded refresh |
| Free gateway pool discovery | Startup-triggered, at most once per process per hour |
| OAuth usage widget | On stale reads, 5-minute cache/error backoff and an in-flight guard |
| Explicit `/usage` report | 2-minute provider-report cache; calls depend on configured accounts and supported APIs |
| OpenCode-compatible client version | GitHub release metadata, first relevant header build then once per hour per process |

A warm direct provider does not poll its catalog once per prompt. **Zero idle
catalog requests** does not imply zero total idle traffic when the OAuth usage
widget, a scheduled agent task, or another process is active. Process-local
refresh guards do not deduplicate independently launched processes.

## Measure a real run

Enable before starting a fresh Alphacode process (including its daemon). Use a
fresh directory for each measured run:

```powershell
$env:ALPHACODE_PROVIDER_AUDIT_DIR = "$env:TEMP\alphacode-traffic-117"
alphacode
```

The original [traffic summarizer](https://github.com/dragonked2/alphacode/blob/v1.1.12/scripts/provider_traffic.py)
was removed in 1.1.13. It reports counts per prompt invocation, session, UTC calendar minute,
origin, and category, and the peak half-open rolling 60-second window. A session
is not necessarily one task: it may contain several prompts. Separate sessions
from spawned agents are reported separately. Null prompt/session IDs mean
unattributed work, including independent background tasks; these requests remain
in the total. Zero-request prompts/minutes do not create dispatch records.

Audit files are local, opt-in, per process and per run. They contain timestamps,
process/session/turn IDs, HTTP method or `WS`, origin and a coarse request kind.
They contain no body, model ID, URL path/query, key, token or auth header.
`inference_or_other_post` is intentionally coarse: an HTTP POST alone cannot
reliably distinguish memory, compaction, inference and other provider operations.

Coverage includes shared reqwest inference transport, async provider catalog,
auth and usage sends, OpenAI WebSocket connection/model dispatches, Cursor native
HTTP/2 inference, and the blocking OpenCode client-version metadata lookup.
Counts measure **client dispatch attempts**, including failed connections, not
provider receipts, billing, redirect hops or reqwest's internal protocol retries.
Builds using the optional AWS SDK, external Claude CLI processes, browser-based
provider flows and external tool processes may generate additional traffic
outside this ledger. Use provider-side access logs or a controlled network
capture for an exact wire count across those boundaries. No live provider quota
was spent to produce the regression evidence.

## Release verification

Version is prepared in `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`. Verification
results are recorded below after running the final local gates. Existing
uncommitted work is preserved; preparation does not publish a tag or release.
