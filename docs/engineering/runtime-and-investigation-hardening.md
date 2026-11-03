# Runtime and investigation hardening

## Model transport and prompt handling

OpenRouter and direct OpenAI-compatible streaming requests now stop when their
response consumer closes. Cancellation drops the in-flight request, including
header waits, idle response reads and retry backoff. This prevents abandoned
local inference or retries from competing with the next turn. The provider may
still charge for work it already performed; cancellation is not a billing guarantee.

The retry loop borrows its immutable request tree instead of deep-cloning the
conversation and tool schemas for every attempt. Direct compatible routes skip
OpenRouter cache capability lookups and cache annotations. Request bodies remain
serialized once for transmission. Token estimation counts JSON bytes without
allocating another serialized buffer and charges image parts the shared 1,600
token heuristic, rather than treating their base64 transport bytes as text.
This is an estimate, not a tokenizer or a resolution-aware vision accounting API.

A chat SSE stream that closes before either a finish reason or `[DONE]` returns
an unexpected-EOF error. Unfinished tool accumulators are discarded, and existing
transient-error retry/rollback handling can recover the turn. Compatible servers
that emit a finish reason but omit `[DONE]` remain supported. Previously the EOF
path fabricated successful message completion, potentially presenting truncated
text or incomplete tool arguments as a completed response.

Local tool-schema compaction removes annotations only at JSON Schema locations.
It preserves parameters named `description`, `title` and `examples`, along with
literal `const` and `default` objects. Compact schemas must retain their original
contract; deleting a parameter to save tokens makes smaller models less reliable.

## Evidence and investigation decisions

Hypothesis evidence imports update existing IDs instead of duplicating them.
Identical data does not count as independent corroboration, including copies
classified under a different evidence type. Proposed payloads and analysis do
not count as observations. Evidence counts can produce heuristic High confidence,
but no longer manufacture Certain confidence. Promotion requires a confirmed,
reproducible result, two distinct supporting observation bodies, no unresolved
contradiction, and no outstanding required verification in the hypothesis set.
These structural checks cannot independently establish that an artifact is true
or that two distinct bodies came from independent experiments.

Hypotheses marked insufficient-data remain recorded but are excluded from
automatic next-test selection until their status is deliberately changed.

Chains require distinct nonblank findings and unambiguous ordering. Dependency
flags alone cannot fully validate a chain: impact, evidence and intermediate
required-state descriptions must be present. The computed severity baseline is
the highest dependency-satisfied link; it no longer increases merely because
several findings are combined or a speculative critical link was added. A higher
combined rating requires assessment of the demonstrated end consequence.

Unused optional impact fields are omitted from serialized attack-chain records.
This removes hundreds of null-valued keys from empty records that may be stored
or included in model context, without dropping populated impacts. Deserialization
accepts both omitted fields and the explicit nulls emitted by older versions.
The regression test requires a minimal chain to serialize below 512 bytes and
round-trips populated impacts and legacy null values.

The bundled bugbounty skill exposes `experiment-design` as an on-demand reference.
It teaches invariant-based tests, competing explanations, controlled state and
identity comparisons, constraint-based pivots, backward/forward chain reasoning,
and compact artifact-based working context. It complements the existing scope,
investigation-control and validation references without loading the entire
technique library into every turn.

## Verification scope

Regression tests cover idle-request cancellation against a loopback HTTP server,
truncated SSE and finish-reason-only completion, image/Unicode/JSON token-size
estimates, schema semantics, repeated evidence, blocked scheduling, promotion
gates and chain validation/severity. Repository checks and their actual results
are reported with the change delivery. No external target was tested and no live
provider comparison, token-saving percentage, end-to-end speedup or performance
advantage over human hunters is claimed.

The complete color-enabled library run before the final serialization-only
change recorded 6,457 passing, 9 failing and 11 ignored tests. All nine failures
are in existing UI work: onboarding title wording, two help-section expectations,
palette calibration/coverage and four historical-palette expectations. The
default environment sets `NO_COLOR=1`, producing additional expected-color and
animation failures; the second run cleared it only in its child process and used
`COLORTERM=truecolor`. Both runs used `CARGO_PROFILE_TEST_OPT_LEVEL=0` to avoid
expensive optimization while exercising the test harness.

On the final code, the focused library run covering OpenRouter/direct-compatible
providers, all security-core tests, schema preservation and the onboarding
scorecard passed: 367 passed, 0 failed, 2 ignored. The additional serialization
round-trip regression is included in this run. The two ignored tests are not
treated as verified live-provider coverage.

Formatting and both workspace checks passed on the final code. Strict Clippy
passed before the final serialization-only change. Its final rerun was queued
behind a separate test build holding the shared Cargo build-directory lock;
that pending rerun is not reported as a completed check.
