# Session capability awareness

## Findings and changes

Progressive disclosure intentionally hides specialized schemas to keep requests
small, but a hidden schema was easy to confuse with an unavailable capability.
Compact prompts did not explain discovery. The discovery implementation searched
only names and used an accumulating process-wide manifest that did not track
late registration/removal or distinguish separate registry instances.

Production `tool_help` instances now hold a weak reference to their own live
registry. Discovery reflects current registrations, applies session allow/deny
policy, forgets obsolete promotions, canonicalizes aliases, and deduplicates
names. The weak reference does not create an ownership cycle. The old constructor
retains the legacy manifest fallback for standalone compatibility.

`tool_help` supports explicit `search`, `list`, and `enable` actions. Search
matches names, bounded descriptions, and capability-family hints without loading
schemas. Existing exact-name and small-query promotion calls still work. Broad
query-only matches above eight tools return a bounded catalog for selection
instead of promoting every schema. Exact tool selections remain explicit.
Catalog output is limited to forty entries and approximately 8 KiB. Malformed
inputs return validation errors instead of silently listing unrelated tools.

Both CLI and TUI request loops now attach a fresh, policy-filtered capability
map to the dynamic prompt. It uses live names rather than building all tool
schemas, remains outside the cached static prompt, and is included in prefix
accounting used for context reservation. The map is capped at 1,400 bytes for
compact contexts and 2,400 otherwise. It includes actual registered/permitted
and advertised counts, family samples, discovery instructions, and a short guide
to choosing a useful first action. Cached schemas for removed/restricted tools
are filtered before sending the request.

The operating guidance now distinguishes registered wrappers from installed
binaries, credentials, healthy backends, and authorization. An SDK provider that
executes tools internally gets separate guidance rather than a claim that the
application registry is its executable tool inventory. Discovery never grants
permissions or overrides restrictions. No model intelligence or performance
multiplier is claimed.

## Regression coverage

- Actual CLI and TUI provider requests receive the refreshed inventory when a
  deferred tool is added and removed between turns, without loading its schema.
- Live discovery tracks registration/removal and does not leak another
  registry's tool names.
- Search does not promote schemas and respects allow/deny policy.
- Alias promotion is canonical and duplicate selections are deduplicated.
- Broad queries do not automatically load a whole tool family.
- Invalid requests fail explicitly.
- Catalogs and capability maps remain bounded with large descriptions/names.
- SDK-managed providers do not inherit native registry access claims.

## Validation

Pending final repository checks and regression results. No live model/provider
benchmark was run; the request-path tests use an offline recording provider.
