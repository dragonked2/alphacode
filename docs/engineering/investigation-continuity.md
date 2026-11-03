# Investigation continuity and execution coverage

The bundled investigation controller connects four records: asset inventory,
coverage matrix, clue register and hypothesis graph. The reasoning changes are
skill instructions, not model training or an exhaustive vulnerability database.
They prioritize reachable protected operations, controlled experiments and
compatible evidence-backed chains. They preserve unresolved leads instead of
claiming that a target was fully tested because scanner tasks completed.

## Runtime changes

- Scanner candidates use a tool name plus SHA-256 of the complete observed line.
  Identical output deduplicates across retries and concurrent ingestion. Dynamic
  text still produces different candidates; this is not root-cause deduplication.
- Bare hostnames from HTTP probes enter the host inventory, not the URL set.
- Seed collection uses hash sets for membership checks and stable task/asset
  ordering before bounded selection. The existing request/seed caps remain.
- Proof verification borrows the recorded exchange collection instead of cloning
  all retained response bodies for every candidate. The evidence gate is unchanged.
- State replacement uses a unique temporary file in the destination directory,
  syncs it, and replaces the destination without first removing the old snapshot.

## Execution artifacts

`state.json` retains task attempt audits with backward-compatible defaults.
Older snapshots without audit entries load, but have unknown per-call coverage.

For each attempt, `coverage/` holds:

- A unique input manifest with the hydrated tool parameters.
- A journal naming the target, task/tool, planned/attempted/successful calls,
  failed and deferred indices, in-flight input, omitted observation lines,
  aggregate truncation, stop reason and artifact paths.
- The transcript returned by each attempted call, before executor aggregation.

The journal is written before dispatch and after observing a result. If the
process dies while a call is in flight, its actual outcome is indeterminate.
On resume, journals newer than the last phase snapshot are reconciled by manifest
identity. A task with an unresolved in-flight attempt is blocked from automatic
replay. Review the manifest, captured response if present, and actual target state
before resolving the journal. Do not mark an unknown write successful merely to
remove the block. Preserve the evidence and the reason for any manual resolution.

`coverage.json` aggregates attempt records when the run report is written.
`report.md` distinguishes partial execution from useful task output. If the
queue is empty but recorded execution gaps remain, the stop reason is
`coverage_incomplete`, rather than `surface_exhausted`.

These records are execution accounting, not proof that an asset was tested or
that a vulnerability exists. One tool invocation may contain multiple targets.
Hydration caps apply before the manifest is produced; discovered but unselected
assets remain in state inventories. Tool-internal truncation still applies.
Per-call transcripts and parameter manifests can contain sensitive engagement
data and require the same handling as the other raw evidence.

## Knowledge corrections

The chain guidance now requires compatible actors, sessions, permissions,
versions/configurations and timing for every edge. Further already-authorized
validation can continue when it is needed to establish the requested consequence.
Ordinary writable fields, DNS resolution, scanner labels and model claims are
not evidence of the more powerful capabilities sometimes inferred from them.

The research broker no longer ships misleading component-to-CVE examples or
invented WSTG identifiers. It routes applicability to primary advisories and
versioned methodology references. KEV, EPSS and CVSS remain separate from observed
impact and from permission to perform a test.

The CTF entrypoint now exposes the solver-engineering reference directly and
requires clue disposition, known-valid/invalid oracles, constraint reduction,
raw-artifact review and a reproducible terminal success condition.

## Verification status

Builds, automated checks and tests were intentionally not run for these changes,
as requested. Source files were formatted. Runtime behavior, performance and
finding/solve-rate improvements have not been measured. Existing unrelated
checkout edits were preserved.
