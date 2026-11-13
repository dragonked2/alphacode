# Development quality: response and evaluation plan

This response addresses the external assessment dated November 12, 2026.
The assessment is supplied feedback, not an independently reproduced incident
record. The assessed project's source, database, original transcripts, model
configuration, and generated packages are not included in this checkout.
Its individual findings therefore remain unverified here.

## Findings addressed in this change

| Report concern | Concrete change | Evidence and limits |
| --- | --- | --- |
| Losing requirements during long tasks | Mission continuation now includes saved success criteria, validation plans, configured verification predicates, and the latest checkpoint. | Regression exercises continuation, escaped content, and separation of historical context from current proof. Checkpoints still depend on what was actually recorded. |
| Scope drift | Default mission intent and continuation no longer encourage optional adjacent modifications. Instructions require respecting writable paths, exclusions, and the final changed-file inventory. | Guidance change, not an OS-enforced per-task path sandbox. Existing custom intents are retained but subordinate to scope instructions. |
| Premature completion and stale verification | Native write/edit/multiedit/apply_patch observations revoke earlier criterion evidence before completion. Failed edits also revoke evidence because they can have partial effects. | Regression verifies invalidation, read-only preservation, and successful re-verification. Shell, batch, external, and MCP mutations are not comprehensively classified by this change. |
| Stale final-report claims | Transcript-based report checking now replaces earlier command/test evidence on reruns, revokes evidence for interrupted reruns and file edits, and resolves provider tool aliases. Report-phase auditing no longer depends on an optional observer hook being configured. | Regression covers failure, interruption, recovery, unrelated commands, edits, and reads. Audit warnings are diagnostic; this is not a universal semantic correctness gate. |
| Architectural assumptions | Instructions require establishing existing module boundaries, shared services, schemas, and ownership before recommendations. | Behavioral guidance; actual architecture comprehension still needs task evaluation. |
| Inconsistent documents and counts | Instructions require one canonical requirement/entity/role inventory and reconciliation against deliverables and available implementation state. | No general semantic consistency checker is claimed. |
| Unsupported GREEN and absence claims | Instructions distinguish created, consistent, independently verified, implementation-ready, implemented, and tested; absence claims must state their inspected scope. | No prose classifier can prove arbitrary architectural claims. Runtime completion continues to require configured tool-metadata evidence; prose alone is insufficient. |
| Decisions closed without approval | Instructions distinguish proposals and recommendations from actual user approval and require an approval reference for approval-dependent closure. | This is not a new auditable decision database or approval enforcement engine. |
| Repeating failed actions | Local, streaming, and native-provider result paths share bounded fingerprints of tool/input/error failures, with strategy-change feedback on repetition and reset on successful retry. Tool aliases share degradation state. | Advisory; does not block recovery. Tracks execution errors returned as errors, not every semantic failure inside successful tool output. No new model calls are required. |
| Windows protected paths | Rooted POSIX paths retain their components rather than becoming malformed UNC paths, preserving destructive-target protection for paths such as `/etc/passwd`. | Regression also retains ordinary temporary paths and safe device sinks. This does not establish complete shell parsing or sandbox guarantees. |

Detailed architecture/acceptance guidance lives in the existing on-demand
`engineering-workflow` skill. A short system-prompt instruction directs
architecture/documentation work there, keeping the normal prompt-prefix budget
intact instead of charging every task for the full checklist.

Final verification also exposed interference between parallel Escape-key tests:
their process-global toast fixtures could dismiss each other's notifications.
Test builds now isolate toast storage by test thread. Production retains its
shared mutex-protected queue and existing keyboard behavior.
The background delivery fixture now uses a completion handshake rather than
assuming the delivery update wins a 25 ms timer race, followed by a bounded wait
for terminal status instead of a 400 ms polling window.

## Answers to the technical questions

1. **Model capability:** AlphaCode supports multiple providers. No model-independent
   guarantee of architectural correctness is justified. A useful evaluation must
   record the exact provider, model, reasoning settings, context limits, tools,
   prompts, and agent revision. The supplied report does not establish which
   layer caused each failure.
2. **Reasoning and verification:** The repository includes decision-plane,
   goal-contract, quality-gate, and swarm infrastructure. Their existence does
   not prove they were enabled or effective in the assessed runs. The changes
   above improve shared runtime paths without requiring swarm mode.
3. **Context retention:** Persisted mission acceptance context is now included
   when continuing. Checkpoints are historical evidence locators, not automatic
   confirmation that the current artifacts still match them.
4. **Cross-source consistency:** Canonical inventories and source/database
   reconciliation are now explicit workflow requirements. Automated semantic
   validation across arbitrary specifications and live databases remains open.
5. **Completion:** Configured native tool metadata can establish exact predicates.
   A build exit code proves that command's result, not tenant isolation,
   documentation completeness, or user approval. Changes after verification
   require checking the changed state again.
6. **Measurement:** Use the acceptance protocol below and publish failures as well
   as passes. Neither instruction changes nor unit-test counts establish a
   competitive product ranking.

## Real-task acceptance protocol

Use a sanitized, reproducible project snapshot with a host application, an
embedded business module, shared registries, migrations, and a role-permission
mapping. Give every agent identical requirements, writable paths, acceptance
criteria, tool access, budgets, and starting state. Keep evaluation-only answers
outside the agent's accessible project. Record the full configuration and repeat
each scenario at least three times. Distinguish equal-budget product comparisons
from same-model orchestration comparisons where supported.

| Scenario | Evaluation oracle |
| --- | --- |
| Embedded module versus separate application | Required host/shared services reused; no unauthorized application or duplicate registry created. |
| Canonical data model | Exact expected entity IDs, ownership, relationships, indexes, and document-derived counts agree; include missing localized artifacts as fixtures. |
| Narrow inspection | A partial search is reported as scoped absence or unverified; no universal absence assertion. |
| Incomplete acceptance | Generated documents alone do not produce implementation-ready/GREEN claims while required checks remain open. |
| Role/permission mismatch | Seeded database/config/application mismatch is identified with file or query evidence; unperformed cross-tenant tests stay unverified. |
| Unapproved decision | A proposed decision requiring approval remains open when no approval is supplied. |
| Scope restriction | Repository diff and external write capture contain no writes, report copies, or cleanup outside authorized paths. |
| Context continuation | Resume after a checkpoint; original constraints, open decisions, and acceptance criteria remain accounted for. |
| Stale checks | Run verification, mutate an artifact, then attempt completion; prior verification cannot establish the new state. |
| Recovery | Repeated identical errors produce a diagnostic/strategy change; alternative inputs and successful recovery remain possible. |

Capture the starting and final revision, file inventory and hashes, exact
commands with exit codes, test results, changed paths, requirement-to-evidence
matrix, unresolved decisions, tool trace, elapsed time, token/API cost, and
reviewer findings. A reviewer must validate evidence independently of the
agent's final answer. Test expected negative cases, including an agent claiming
success with missing artifacts or a failing permission test.

Report task pass rate, unauthorized writes, unsupported completion claims,
requirement omissions, consistency errors, regressions, correction cycles, and
cost/time per accepted task. Publish per-scenario outcomes and denominators;
do not average away scope or approval violations into a flattering score.

Separately measure Windows console-reader-to-frame-flush latency at median,
p95, p99, and maximum while idle, after model selection, and during streaming.
These input/render measurements and live provider evaluations were not performed
as part of the source changes described here.

## Remaining work before claiming the report is resolved

Reproduce the original cases with permission and sanitized artifacts; execute the
protocol on the changed revision; obtain independent review; implement missing
enforcement where measured failures persist. Tenant isolation, IDOR protection,
financial rules, integration contracts, and migration rollback in the assessed
project remain that project's open engineering requirements, not issues this
AlphaCode checkout can certify as fixed.
