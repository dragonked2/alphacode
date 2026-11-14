# Tool-failure retrospective — 2026-11-13

This note records reusable lessons from the release and website-maintenance
session. It is an operational guide, not evidence that every external adapter
or target-side failure can be eliminated.

## Observed failures and durable responses

| Observation | Failing layer | Prevention/recovery |
| --- | --- | --- |
| A read call supplied both `offset` and `start_line`. | Tool schema/adapter | Select one range coordinate style before dispatch. AlphaCode intentionally rejects ambiguous ranges; matching redundant coordinates remain supported where validated. |
| A patch used an unsuitable Windows path form. | Patch adapter | Prefer repository-relative paths and inspect the resolved target before editing. If the adapter rejects the path, correct the call rather than retrying the same payload. |
| A todo update omitted required fields. | Tool schema/adapter | Send the complete structured todo payload, including IDs, status, confidence, and plan/goal fields when required. |
| `httpflow` encountered Datadome/Cloudflare protection. | Target/tool capability boundary | Record the surface as blocked or inconclusive and switch once to an authorized browser-capable path. A challenge response is not a security-test result. |
| A file-type filter reached alphagrep as `rs}`. | Tool argument serialization | Normalize only obvious trailing JSON delimiters before dispatch and keep the original malformed value in a warning log; do not silently broaden arbitrary filters. |
| A cancel raced a background task that had just completed. | Background-task lifecycle race | Make cancellation idempotent for a known finished task and report its final status; preserve an error only for an unknown task. |
| Independent and mutating operations were mixed in parallel. | Agent orchestration | Parallelize independent reads only. Serialize edits, staging, commits, builds, and shared-cache operations. |

## Scope of the lesson

The first three failures occur in the tool bridge before AlphaCode's Rust
runtime executes. Prompt guidance can reduce recurrence, but only the bridge
implementation can guarantee its own validation and path normalization.

Bot protection is an expected environmental condition. Retrying it through the
same HTTP client does not improve coverage and can increase traffic without
producing evidence. The existing `httpflow` behavior remains deliberately
conservative.

## Verification checklist

- Inspect the exact error and classify the failing layer.
- Correct one input or execution condition at a time.
- Re-run the smallest decisive check.
- Inspect resulting state, not just the process exit code.
- Preserve the failure as blocked/inconclusive evidence when the prerequisite
  is external or unavailable.
