# Adaptive hunting and reproduction correctness — 2026-11-14

This pass preserves the pre-existing investigation, reasoning-tool and prompt
changes in the shared checkout. Its additions are limited to the bundled bounty
workflow and the reproduction harness.

The bounty workflow now requires an engagement map, product-specific security
invariants, benign alternative explanations, distinguishing controls, request
costs and explicit stop conditions. Each observation should advance, close,
redesign or block an experiment. Stable discovery can be reused only while its
prerequisites hold; independent reads may run concurrently within program limits,
while shared state mutations remain sequential. Duplicate instances are grouped
by root cause. These are agent instructions, not a new enforced global scheduler.

The Rust workspace harness previously returned compiler success without executing
the reproduction. It now executes the generated platform-specific binary after a
successful build, sharing one time budget between compilation and execution and
returning the runtime outcome and output. Compiler failures remain failures.

Output capture previously stopped reading and closed the pipe at twice its
retention limit. That could break a noisy child and change the experiment. It now
drains to EOF while retaining only the configured limit plus a truncation sentinel.
The truncation message does not claim a count of discarded bytes it did not retain.
Regression tests check runtime execution and full draining with bounded retention.

No deployed program was assessed. No discovery speed, precision, recall or bounty
acceptance improvement has been measured. Process-tree cleanup remains best effort;
this change does not make arbitrary reproduction programs a sandbox.

## Provider request compatibility

An additional change handles structured HTTP 400 errors that explicitly identify
an unsupported optional Chat Completions setting. The transport removes only the
named, allowlisted setting and sends one repaired request. Generic 400s, invalid
values, authentication failures, required fields, model IDs, messages, tools and
output limits are not repaired this way. A second rejection returns to normal
error handling. No error response body is copied into the repair log.

The regression fixture returns two different unsupported-setting errors. It checks
that the first request is intact, the second removes exactly the first setting,
and the second rejection ends the repair path. Pure tests cover required fields,
invalid values, missing diagnostics and non-400 statuses.

The supplied Alphax Free message contains a redacted status, not the original
upstream diagnostic. This change fixes the explicit unsupported-optional-setting
failure class; it does not establish the cause of that particular incident or
claim the deployed gateway was tested. Existing fallback behavior is preserved.

## Request-rate inspection

The November 14 local log contained 635 OpenAI-compatible stream-attempt entries
across 225 active calendar minutes when inspected. Median: 2 attempts per active
minute; mean: 2.82; peak calendar minute: 12; peak rolling 60 seconds: 14.
Recent active calendar minutes had 1–2 entries. Timestamps use Egypt Standard Time.
These are aggregate logged attempts, not a per-task or provider-side HTTP count:
the entries had no session attribution, attempts log before send, and protocol or
compatibility fallbacks may send additional requests. Other runtimes and model
catalog traffic are outside this count. The machine-wide log can grow after this
snapshot. Aggregate observations are saved separately as JSON.

Source inspection found the shared rolling budget configurable through
`ALPHACODE_PROVIDER_REQUESTS_PER_MINUTE` and
`ALPHACODE_PROVIDER_REQUESTS_PER_HOUR`. Alphax has no default rolling RPM cap;
TokenHarbor defaults to 50/minute and 1500/hour. The concurrency gate defaults to
two requests awaiting headers and 150 ms spacing under contention. Neither is
a fixed requests-per-minute rate. No rate settings were changed by this pass.

## Unnecessary-request audit

The RPM figures above do not measure catalog overhead. A separate scan of the
same log found 26 failed LM Studio profile-catalog refresh outcomes and 10 failed
OrcaRouter profile-catalog refresh outcomes. These are historical log outcomes,
not counts of provider-billed generations or evidence of a runaway task loop.
The log spans multiple sessions/builds; current guards cannot be inferred from
those entries alone.

Two source-level request amplification paths were corrected:

- An explicit model-catalog refresh previously refreshed endpoint metadata for
  the active model, the first 16 catalog entries, and additional cached models
  up to 24 targets. It now refreshes only the active model's endpoint metadata.
  Other model metadata remains available through on-demand loading.
- The before-refresh snapshot called display helpers that could launch another
  background catalog request on a missing/stale cache. The explicit refresh now
  records its attempt before taking those snapshots, suppressing that redundant
  background scheduling through the existing retry guard.

A local HTTP regression fixture supplies 30 unused catalog models with a cold
in-memory cache. It asserts that explicit refresh issues exactly two GETs: the
catalog and active model endpoint metadata. It sends no model generation calls.

Remaining behavior: the configured-profile sweeper wakes every ten minutes,
after a one-minute initial delay, and refreshes missing/stale catalogs with
per-process single-flight and failure backoff. It includes configured inactive
profiles; failed local-server checks therefore remain possible. Alphax is excluded
from that sweep in current source, but its constructor separately schedules a
free-pool discovery request at most hourly per process. Catalog/free-pool GETs
use direct HTTP sends rather than the shared generation-transport RPM limiter.
Multiple daemon processes have separate catalog refresh guards. This audit does
not establish a complete physical HTTP count across those paths.

`ALPHACODE_DISABLE_CATALOG_REFRESH=1` disables scheduled sweeps only. It does not
disable explicit refresh, active-provider cache refresh, startup prefetch or the
separate free-pool discovery path. No user configuration was changed.

## Verification

- Formatting, workspace check and workspace test compilation passed.
- Full library suite passed before the later provider changes: 6879 passed,
  18 ignored, no failures.
- Reproduction harness: 11 passed; local bounty evaluator self-tests: 5 passed.
- After compatibility repair: all 13 SSE transport tests passed, including the
  loopback two-rejection test; workspace checks and library Clippy passed.
- After catalog changes: workspace checks passed and the cold-catalog request
  regression passed (exactly two GETs with 30 unused models). Library Clippy
  also passed with warnings denied.
