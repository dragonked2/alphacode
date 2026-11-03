---
name: investigation-control
description: Maintain clue continuity, explicit coverage, impact-led prioritization and reliable pivots throughout an authorized assessment.
---

# Investigation control

Use for broad assessments, long investigations, resume/compaction, or when a
promising lead is stuck. Pair with `real-world-validation`; load category
techniques only when an observed mechanism motivates them. The objective is
verified impact and accountable coverage within the engagement, not a promised
critical finding, an infinite scan, or a guarantee of missing nothing.

## Keep four connected records

Store these in existing engagement notes, preserving prior work. Use small
current-state tables linked to raw artifacts. Do not invent unsupported tool
schemas or claim these notes are automatically executed by the orchestrator.

1. Asset inventory: exact origin/component, discovery source, scope decision,
   environment/version evidence, roles required, last observed state. Discovery
   does not confer authorization. Preserve excluded assets with a reason.
2. Coverage matrix: asset + feature + actor/tenant + operation + lifecycle state;
   applicability, tested revision, result, evidence, next action. Record pending,
   tested, not-applicable, inconclusive and blocked separately. Tool success does
   not close an entire feature or vulnerability class.
3. Clue register: stable ID, observation, source/artifact + locator, interpretation,
   competing explanation, related hypotheses, disposition and reopen condition.
   Label observed facts separately from inferred relationships.
4. Hypothesis graph: intended invariant, entry point, controllable primitive,
   prerequisites, proposed effect, confirming/refuting experiment and current
   evidence. Connect edges to clue IDs and owned test fixtures.

A clue's disposition is one of untriaged, investigating, linked-to-test,
resolved-with-evidence, deferred-with-reason, or outside-scope. A timeout or a
missing account is blocked/inconclusive, not resolved. Keep failed attempts;
new constraints may make a previously blocked path testable.

## Read for constraints and boundaries

Inspect supplied scope, source, routes, manifests, documentation and raw tool
output before buying more scan volume. Extract actionable clues from:

- Browser/JS: route maps, API clients, source maps if exposed, feature flags,
  origin/message handling, DOM sinks, storage, hidden workflow transitions.
- API: object references, parent-child relationships, batch methods, alternate
  versions, GraphQL resolvers, async job IDs, download/export endpoints.
- Identity: issuer/client/tenant bindings, invitations, account linking,
  recovery, role changes, membership removal and session invalidation.
- Data flow: parsing, decoding, canonicalization, authorization checks,
  persistence, caching, signing and downstream consumers.
- Operations: error provenance, correlation IDs, retries, cancellation,
  idempotency and settlement. An error message is a lead, not execution proof.

For each observation ask: what does it constrain, which boundary might differ,
what ordinary explanation competes, and what inexpensive observation would
change the plan? Similar URLs are not interchangeable when identity, tenant,
method, body, headers, state or backend changes. Conversely, cosmetic payload
variants against the same unchanged boundary add little independent coverage.

## Prioritize reachable consequences

First identify what the program protects: private tenant data, administrative
control, financial/accounting invariants, signed artifacts, workflow integrity
or tool authority. Walk backward from a protected operation to the reachable
entry points and forward from observed input to its consumer.

Prefer tests with observed reachability, satisfied prerequisites, a meaningful
security boundary and a decisive low-cost oracle. Use ordinal tiers and explain
the reason; do not fabricate calibrated probabilities or expected payouts.

- Now: reachable protected operation with a controlled baseline and a concrete
  discrepancy, or a cheap test that separates two leading explanations.
- Next: plausible boundary with one obtainable prerequisite or an unresolved
  control. Acquire the prerequisite rather than cycling equivalent payloads.
- Later: weak fingerprint, unsupported version match, expensive speculative
  chain, or mechanism whose required feature has not been observed.
- Blocked: missing scope, role, fixture, environment or reliable oracle. Record
  exactly what would unblock it and continue independent permitted work.

Do not discard a valid lower-severity issue to chase an imagined critical.
After sufficient proof, record it and reprioritize independent coverage. Avoid
fixed scan-first ladders when source or behavior points directly to a boundary.

## Hunt policy mismatches systematically

When a feature exists and testing is permitted, compare only relevant pairs:

| Boundary | Discriminating comparison |
|---|---|
| Object ownership | Own, other owned-account, nonexistent and explicitly shared fixtures |
| Tenant/role | Same endpoint under two tenants/roles with distinct canaries |
| Delegation | Before grant, during grant and after membership/token revocation |
| Nested resources | Parent authorization versus child authorization and mixed-owner batches |
| Async workflows | Enqueue, execution, status, result download and expiry/revocation |
| Write controls | Ordinary writable field versus a documented protected field on a disposable fixture |
| Business invariants | Valid sequential flow versus reordered/replayed/bounded concurrent flow; inspect settled state |
| Distributed parsing | Producer and consumer interpretation, exact bytes and normalization |
| Storage/capabilities | Object policy versus signing permission, URL expiry and documented revocation behavior |
| Agent/tool authority | Retrieved text versus tool permissions, principal propagation and tenant retrieval |

Do not assume every row applies. Rule out public design, shared ownership,
frontend shells, cached copies, stale tokens, failed writes and eventual
consistency before claiming a boundary break. A status change alone is weak.

## Construct compatible chains

Read `chain-reasoning` when one primitive unlocks a new authorized experiment.
For every edge, record actual authority gained, identity/session, origin, data
format, timing, prerequisites and the next required permission. Check that the
output of one edge satisfies the input of the next in the same actor context.
An admin-only primitive cannot be silently combined with an unauthenticated
attacker. Two vulnerabilities in incompatible configurations are not a chain.

Test a further edge when existing authorization permits it and it is necessary
to establish the requested consequence. Reuse owned canaries and reversible
fixtures. Stop at sufficient proof of that consequence. Leave untested edges
clearly hypothetical; never assign their combined severity to the demonstrated
portion. Do not use third-party credentials or records to increase the score.

## Pivot and recover without losing information

At a checkpoint, reconcile discovered assets, coverage rows, open clues and
hypotheses. Identify orphan clues with no disposition, tests without artifacts,
assumptions treated as facts, partial tool runs, omitted output, and tasks whose
prerequisites failed. Choose the next discriminating test for the top live path.

When stuck: inspect the raw result and request actually sent; verify tool/schema,
identity, input reachability, routing and local assumptions. Seek an independent
observation (source, debugger, fresh fixture, server state) rather than merely
changing tools. Change one constraint or approach and state why it can help.
Reopen old paths only when new evidence invalidates their decisive observation.

Before compaction/resume, save objective, scope/version, inventory path, open
clue IDs, best hypotheses, rejected explanations, budgets, in-flight writes and
exact next action. Resume by reading those artifacts, checking changed state,
and revalidating expired sessions. An interrupted write has an indeterminate
outcome until observed; never blindly replay it.

## Finish with accountable limits

An empty scanner queue means its generated work ended. Reconcile the planned
coverage and every high-value open clue before ending an assessment. Close or
explicitly defer each item with evidence, a blocker or the agreed budget/stop
reason. Unknown coverage remains unknown; do not invent completion percentages.

Report demonstrated impact, repeatable minimal steps, evidence locators,
preconditions, remediation boundary and cleanup. List untested areas, pending
clues, tool limitations and remaining assumptions separately. Technical validity,
program eligibility and eventual acceptance are different outcomes.


## Broad-program campaign loop

Treat "full scope" as an inventory to reconcile, not an endpoint list to spray.
Record the official policy source, retrieval time, eligibility, exclusions and
shared request/concurrency budget. When the policy cannot be read, do not use a
mirror as authorization; continue supplied-artifact work and resolve the missing
contract before active tests that depend on it. Keep discovery, authorization,
reachability and vulnerability status separate.

Build a behavior map alongside the asset map: user journey -> service making the
decision -> protected object/value -> invariant -> permitted experiment. Cover
observed account lifecycle, delegation, exports/jobs, support, payments/rewards
and uploads as applicable; do not invent features or permissions. Map shared
backends without assuming every branded integration is in scope.

At the end of each work block:
1. Reconcile new assets and features against scope and the coverage matrix.
2. Select the highest-value unanswered question, including blind spots in actors,
   lifecycle, alternate interfaces and failed prerequisites.
3. Run a bounded hypothesis/control experiment and retain the original artifacts.
4. Reproduce supported findings from fresh state when feasible, consider the
   strongest benign explanation, and link variants to their root cause.
5. Draft confirmed findings promptly, check cleanup, then continue independent
   work. External submission follows the user's authorization and program rules.
6. Checkpoint remaining questions, budget consumption, changes and next actions.

A hypothesis ID, outcome, artifact and next decision should explain each test
block. Do not count cosmetic payload variations as independent coverage. A tested
matrix row records the precise conditions and revision, not universal safety.
Fixed finding quotas cannot justify severity inflation or endless execution.

## Improvement needs evidence

Measure independently reproduced findings, unsupported claims, unresolved
coverage, time/request cost and duplicate work. For before/after comparisons use
held-out authorized local fixtures under equal models, tools and budgets; retain
blocked and unsuccessful runs. Keep expected answers out of agent context and
avoid tuning on the held-out set. Report observed results and limitations, never
infer improved recall or superiority from instruction changes alone.
