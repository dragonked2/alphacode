---
name: hunting-intelligence
description: "Behavior-first vulnerability discovery for broad authorized assessments: reconstruct the product and trust graph, mine security invariants, maintain a diversified hypothesis portfolio, select high-information experiments, search exploit-graph frontiers, and turn every result into better coverage. Use at the start of a serious hunt, when scanners are quiet, when many surfaces compete, or when the next best test is unclear."
---

# Hunting Intelligence — Find Broken Decisions, Not Familiar Strings

This reference is the discovery layer for a serious authorized hunt. It does not
replace class-specific skills, `real-world-validation`, `investigation-control`,
or `finding-validation`. It decides **where to look, what property to test, and
what observation should change the plan**.

The durable advantage is not a larger payload list. It is a better model of:

1. where authority, private data, money, and code execution cross boundaries;
2. which component is trusted to make each security decision;
3. which alternate path may disagree with that decision;
4. which experiment most cheaply distinguishes a bug from the strongest benign
   explanation; and
5. what the result teaches about untested sibling behavior.

Treat public research, scanners, source matches, and unusual responses as leads.
A vulnerability exists only when target behavior breaks a security invariant
with demonstrated consequence under the validation contract.

## 1. The attacker decision loop

Think like the attacker who has limited time and wants the shortest credible path
from a low-privilege foothold to protected value. Defenders usually secure the
main happy path; hunt the alternate consumer, stale artifact, rare state, or
representation where that assumption is not rechecked.

**Shortest path is an execution strategy, not a coverage boundary.** It optimizes
the order of experiments and finds the least harmful decisive proof for one edge;
it never removes a discovered in-scope surface, actor, object/state transition,
channel, or trust edge from consideration. Minimal proof limits escalation after
an invariant is demonstrated, not discovery breadth. One successful path ends
that proof only—continue every independent applicable hypothesis.

Maintain a coverage inventory broad enough that every discovered applicable item
ends as `TESTED`, `VERIFIED`, `REFUTED` under exact conditions, `BLOCKED` with a
reason, or `NOT-APPLICABLE` with evidence. Anything deferred by budget or user
stop remains explicitly `UNTESTED`; it must not silently disappear or become a
negative conclusion. This is how the campaign leaves no known stone unaccounted
for without pretending that any finite assessment proves universal security.

```text
VALUE -> TRUST ASSUMPTION -> WEAKEST ALTERNATE PATH -> DECISIVE PROOF
      -> SIBLINGS SHARING THE DECISION -> NEXT FRONTIER
```

For each cycle, keep the decision record to five lines:

```text
Goal:       protected data, authority, money, identity, or execution sought
Assumption: the security decision defenders expect to hold
Weak path:  the cheapest evidence-backed path that may skip/disagree with it
Signal:     terminal observation that proves or kills this edge
Decision:   test, expand, pivot, block, or report
```

Do not narrate the whole candidate list or run ceremonial recon. Communicate the
leading hypothesis, execute the smallest discriminating test, and update the
model from observed behavior. Keep deep mechanics in class references and load
them only when target evidence makes them relevant.

The full engagement loop is:

```text
SCOPE -> RECONSTRUCT -> MODEL -> MINE INVARIANTS -> BUILD PORTFOLIO
      -> RUN DECISIVE EXPERIMENT -> UPDATE MODEL -> EXPAND OR PIVOT
      -> VERIFY -> REPORT -> RECONCILE COVERAGE
```

### 1.1 Scope and terminal evidence

Before active testing, record:

- eligible assets and revisions;
- allowed identities, tenants, accounts, and data;
- prohibited methods and availability limits;
- authentication and test-data requirements;
- the requested terminal outcome;
- what observation would authoritatively prove it;
- request, concurrency, and state-change budgets.

A tool exit code, scanner label, callback, HTTP status, or reflected payload is
not automatically the terminal evidence. Name the protected data, capability,
or persisted state that must be observed. For write tests, name the cleanup and
the read-after-write or read-after-delete check.

### 1.2 Reconstruct before mutating

Walk ordinary product journeys before attacking them. Learn how users create,
share, approve, bill, export, invite, integrate, revoke, recover, and delete.
Capture the application's own vocabulary. Product nouns become object classes;
verbs become operations; pricing tiers become entitlements; workflow labels
become lifecycle states.

Do not enumerate endpoints in arbitrary order. Build a compact model first, then
use endpoints as evidence about that model.

### 1.3 Maintain a portfolio, not a favorite theory

Keep several independent hypothesis families alive so one attractive anomaly
does not consume the hunt:

- identity, session, recovery, delegation;
- object/function/property authorization and tenant isolation;
- lifecycle, workflow, economic, quota, and concurrency invariants;
- backend-reaching interpreters and server-side fetches;
- parser, normalizer, signature, routing, and cache disagreements;
- file, storage, import/export, archive, and media pipelines;
- integrations, webhooks, asynchronous jobs, agents, and confused deputies;
- alternate APIs, versions, batch paths, realtime channels, and mobile/legacy
  clients;
- deployment, cloud, supply-chain, and secret-bearing surfaces when in scope.

A portfolio is not a checklist to spray. Each active item needs target evidence,
a plausible broken decision, and a discriminating oracle.

## 2. Build the four maps

The maps are small reasoning artifacts, not exhaustive documentation. Keep them
in the engagement ledger and update them when observations disagree.

### 2.1 Value map

List where impact actually lives:

| Value | Examples | Useful question |
|---|---|---|
| Authority | admin actions, invitations, approvals, API keys | Who may cause this transition? |
| Private data | messages, documents, exports, support records | Which actor/tenant may read each field? |
| Money/value | prices, credits, refunds, coupons, quotas | Which quantities must be conserved? |
| Identity | email, MFA, recovery, federation, linked accounts | What binds this credential to this principal? |
| Execution/trust | webhooks, templates, builds, agents, imports | Which untrusted input reaches a privileged interpreter? |
| Availability | queues, expensive queries, fan-out | Is bounded non-disruptive validation possible? |

Prioritize protected operations and transitions over static pages. A reachable
operation that moves value is normally a better lead than a generic banner.

### 2.2 Actor and authority map

Label every controlled identity distinctly:

```text
anonymous
account A / tenant A / role owner / free
account A2 / tenant A / role member / free
account B / tenant B / role owner / free
service account / integration / scoped token
support or admin role (only when provided or legitimately self-provisioned)
```

Record authentication mechanism, role, tenant, entitlement, delegated scopes,
session age, and object ownership. Never call a same-tenant comparison a tenant
isolation test. If a required identity is missing, mark the edge `BLOCKED`; do
not manufacture a negative conclusion.

### 2.3 Object and state map

For each high-value object, record:

```text
object: invoice
identifiers: path id, body account_id, external reference
states: draft -> issued -> paid -> refunded
operations: create, read, update, approve, export, refund, delete
owners: user, workspace, customer account
channels: web, REST v1, REST v2, GraphQL, webhook, export job
artifacts: signed URL, receipt PDF, audit event, notification
```

Security bugs cluster where ownership or allowed operations change: invites,
transfers, sharing, publication, archival, cancellation, revocation, plan change,
and asynchronous completion.

### 2.4 Trust-decision graph

Draw producer -> transformer -> decision -> consumer edges. Mark where identity,
resource ownership, policy, signatures, routing, normalization, or caching are
interpreted.

```text
browser -> edge -> gateway -> service -> queue -> worker -> object store
IdP -> callback handler -> account linker -> session issuer
user URL -> preview service -> redirect/DNS resolver -> fetcher -> parser
client price -> checkout API -> payment provider -> webhook -> entitlement job
```

For each security decision ask:

- Which component believes it made the decision?
- Which component actually enforces it?
- What representation does each component see?
- Is the decision repeated at the protected operation or assumed from upstream?
- Can another channel reach the consumer without passing the same decision?
- Does the decision survive retries, queues, caches, redirects, and version drift?

High-yield candidates are **decision seams**: two components parse, authorize,
cache, sign, or normalize the same fact differently.

## 3. Mine security invariants

An invariant says what must remain true despite attacker-controlled inputs or
sequence changes. Mine it from product behavior, policy, documentation, schema,
UI constraints, errors, source, tests, and sibling operations.

### 3.1 Invariant patterns

```text
Ownership:      only an authorized principal may read or mutate object O.
Tenant:         data and authority from tenant A never affect tenant B.
Function:       operation X requires role R at the point X is performed.
Property:       clients cannot set server-owned field P.
State:          transition S1 -> S2 is allowed only once and only by actor A.
Conservation:   credits/refunds/uses cannot be created by replay or reordering.
Freshness:      revoked or expired capability C cannot authorize future work.
Binding:        token/signature/nonce is bound to actor, audience, object, action,
                amount, redirect, and relevant representation.
Isolation:      cache/job/export result is partitioned by security context.
Interpretation: validators and consumers agree on the security-relevant meaning.
Provenance:     a privileged action is attributable to a permitted origin/consent.
```

### 3.2 Sources of implicit invariants

Look for promises the product makes without calling them security controls:

- “Only workspace owners can…”
- “One-time”, “private”, “read-only”, “expires in 10 minutes”, “cannot be undone”;
- UI controls hidden by plan, role, or state;
- a field present in admin responses but absent from member responses;
- one endpoint returning 403 while a sibling operation accepts the same object;
- separate create/preview/confirm endpoints;
- signed links, callback URLs, job handles, cursor tokens, share identifiers;
- client-side validation that reveals expected ranges or enum states;
- audit logs that imply a transition should be attributable and unique.

UI behavior is evidence of intended behavior, not proof of server enforcement.
Translate it into a server-side hypothesis.

### 3.3 Sibling inconsistency mining

Once one operation reveals a control, identify siblings that should enforce the
same invariant:

- read vs search vs export vs attachment vs notification preview;
- single vs bulk vs batch vs import;
- REST vs GraphQL vs realtime vs mobile/legacy API;
- document route vs data/prefetch route;
- v1 vs v2, regional host, locale, alternate content type;
- synchronous response vs queued worker/webhook callback;
- object endpoint vs nested child endpoint;
- user action vs integration/service-account action.

Do not test every permutation. Select siblings with a different code path,
consumer, policy layer, serializer, or lifecycle phase.

## 4. Rank the attack frontier

Prioritization is for choosing tests, not assigning severity. Never turn this
into a homemade CVSS score.

Prefer candidates with:

1. a high-value protected consequence;
2. an observed reachable operation;
3. a plausible missing or inconsistent decision;
4. controlled prerequisites and owned fixtures;
5. a crisp oracle that distinguishes success from normal behavior;
6. low operational risk and low test cost;
7. architectural leverage over a family of sibling paths;
8. novelty relative to already tested boundaries.

Deprioritize candidates supported only by a generic fingerprint, scanner label,
or payload folklore. Keep uncertainty visible: an unknown precondition may make
a cheap reconnaissance experiment valuable, but it does not make impact likely.

### 4.1 Frontier ledger

```text
ID: H-AUTHZ-07
Target operation: POST /v2/workspaces/{w}/exports
Invariant: member of tenant A cannot export tenant B's records
Target evidence: export UI + job endpoint + tenant id in body
Benign rival: server derives tenant solely from token; body id is ignored
Oracle: completed owned export contains B-canary under A identity
Controls: A/A own export; A/nonexistent; B/B own export
One variable: workspace id only
Budget/stop: one owned export per control; stop on unrelated data
Expansion if supported: download URL, job status, notification attachment
State: OPEN
```

### 4.2 Exploit-graph frontier

Model findings and leads as capabilities, not dramatic narratives:

```text
untrusted input reaches previewer
  -> previewer follows redirect
  -> fetch leaves intended network boundary
  -> response influences protected state
```

Each edge needs compatible principals, objects, state, scope, and evidence. Test
the cheapest uncertain edge first. Never claim the terminal node from an earlier
primitive. Stop a chain when the requested consequence is proven; preserve
unverified edges as hypotheses.

Prioritize **frontier nodes**: observed capabilities one decisive edge away from
a protected consequence. Avoid long speculative chains whose prerequisites have
not been observed.

## 5. Design high-information experiments

A good experiment changes one security-relevant variable and includes a working
control. It should have a clear interpretation for positive, negative, and
masked outcomes before the request is sent.

### 5.1 Experiment card

```text
Hypothesis / invariant:
Target and current revision:
Target evidence and route reachability:
Actor / tenant / role / entitlement / session:
Owned object and lifecycle state:
Strongest benign explanation:
Baseline and known-working control:
One changed variable:
Positive oracle (protected data or settled state):
Negative control:
Masked/inconclusive outcomes:
Request/concurrency/state-change budget:
Cleanup and terminal-state check:
Expand, refute, pause, and reopen rules:
Artifact paths and UTC timestamps:
```

### 5.2 Semantic differential testing

Compare meaning, not merely bytes or status:

- protected field set and canary value;
- object identity, owner, tenant, and effective role;
- authorization-relevant headers/cookies/token claims;
- settled database-visible state or subsequent read;
- job result, export contents, audit event, notification, or entitlement;
- redirect destination and code/token recipient;
- cache partition and response provenance;
- outbound callback marker and the component that caused it.

Normalize volatile fields only after preserving raw evidence. Do not normalize
away the property under test. A `200` login page, SPA shell, queued placeholder,
soft 404, or stale cache entry is not successful access.

### 5.3 Controlled mutation families

Generate mutations from the model, not random payloads:

| Dimension | Mutations |
|---|---|
| Actor | anonymous, A, A2, B, scoped integration |
| Ownership | own, same-tenant other, cross-tenant, nonexistent |
| Role/plan | lower role, expired trial, removed member, downgraded plan |
| Lifecycle | before approval, after completion, canceled, revoked, archived |
| Operation | read, update, action, export, bulk, nested child |
| Channel | web, REST, GraphQL, realtime, webhook, worker |
| Representation | path/body/query id, JSON/form/multipart, duplicate/nested field |
| Time | fresh, replayed, expired, post-logout, post-revocation |
| Sequence | omit, repeat, reorder, rollback, parallelize within safe budget |
| Routing | version, locale, data route, alternate host only if explicitly scoped |

Representation mutations are useful when independent components may disagree.
Examples include duplicate keys, type shifts, omitted/null/empty values, encoded
separators, case/Unicode normalization, path normalization, and content-type
changes. Use the smallest safe set justified by observed parsers. An allowed
mutation without a forbidden consequence is not a vulnerability.

### 5.4 Probe ladders

Escalate evidence gradually:

```text
1. Reachability: does the intended component receive the controlled input?
2. Interpretation: how is the relevant value parsed or normalized?
3. Boundary: does behavior cross actor/tenant/policy isolation?
4. Consequence: is protected data returned or protected state changed?
5. Reproduction: does it survive a clean replay and controls?
6. Scope expansion: which siblings share the same root cause?
```

Use harmless canaries and owned data. Do not jump from reflection to execution,
from callback to internal access, or from accepted input to privilege gain.

### 5.5 Result update rules

After every experiment update the model:

- **Supported:** prediction observed; run the strongest benign control and clean
  replay, then validate the minimum consequence.
- **Refuted:** a discriminating control contradicts this precise prediction;
  record the mechanism and reopen condition.
- **Inconclusive:** reachability, oracle, state, cache, WAF, identity, or tool is
  ambiguous; fix that layer before varying payload spelling.
- **Blocked:** prerequisite unavailable; continue independent hypotheses.
- **Paused:** information gain no longer justifies cost, but evidence does not
  refute it.

Three equivalent responses trigger reassessment. They do not prove a bug class
absent. Ask whether the request reached the intended path and whether the oracle
could expose the predicted consequence.

## 6. High-yield attacker questions

Use this compact matrix to generate hypotheses, then load the matching class
skill for mechanics and safety. Pick the row with target evidence and the
shortest path to protected value for the **next** experiment; do not march through
it mechanically. Selection changes sequence, never the requirement to reconcile
every applicable row and target-specific surface in the coverage inventory.

| Surface | Attacker's question | High-leverage alternate path |
|---|---|---|
| Identity/session | What stale or weakly bound artifact still grants authority? | recovery, invite, linking, role loss, tenant transfer, legacy client, revocation |
| Object/function/property authZ | Where is actor, tenant, role, or server-owned data trusted rather than rechecked? | search/export/attachment, child object, bulk, body-vs-path id, async action |
| Workflow/economics | Which transition or conserved quantity can be replayed, reordered, raced, or client-selected? | confirm without prepare, retry/rollback, coupon/refund/quota, plan downgrade |
| Integration/delegation | Can low-privilege input steer a stronger principal? | webhook, OAuth callback, preview/test mode, import URL, service account, agent/tool |
| Parser/router/signature/cache | Which two components can assign different security meaning to the same bytes? | edge/origin, validator/consumer, signer/parser, cache/auth context, Unicode/path form |
| Async/artifacts | What executes or remains readable after authority or state changes? | queue worker, job result, export, generated file, email link, signed URL, notification |
| File/transformation | Which later stage has a stronger parser or weaker ownership check? | archive, converter, previewer, variant/metadata, temporary object, overwrite/delete |
| Query/collection | Which collection path sees more than the object path? | search, GraphQL nested/batch, filters, history, audit, export, archived/deleted records |
| Alternate channel/version | Which older or indirect path missed the current control? | REST/GraphQL/realtime, v1/v2, mobile, locale/data route, regional service |
| Deployment/supply chain | Where can build or runtime trust be influenced? | CI artifact, dependency namespace, IaC state, registry, cloud identity, debug surface |

Across all rows, prefer these recurring asymmetries:

1. **checked early, used later** — the value changes across redirect, queue, retry,
   normalization, or object transfer;
2. **checked once, consumed many ways** — a sibling consumer omits the decision;
3. **data authority mismatch** — attacker controls input used by a stronger worker,
   integration, browser, or agent;
4. **representation mismatch** — policy and consumer parse different meaning;
5. **current policy, stale capability** — old session, link, job, key, cache, or
   signature survives a security-relevant transition;
6. **happy path, rare state** — cancellation, partial failure, duplicate delivery,
   archival, downgrade, or rollback skips the invariant.

Use controlled identities and owned fixtures. Inspect final protected data or
settled state. A hidden button, accepted input, callback, queued response, schema,
or response code is only a lead.

## 7. Source-assisted hunting

When source, binaries, configuration, or infrastructure definitions are supplied,
use them to make black-box experiments sharper—not to skip target reproduction.

### 7.1 Find security decisions, then trace callers and consumers

Search for:

- authorization and ownership checks;
- role/plan/feature gates;
- tenant selection and server-owned fields;
- token, signature, nonce, and redirect validation;
- normalization before routing, signing, caching, storage, or interpretation;
- serializer field filters and export builders;
- queue producers/workers and retry/idempotency logic;
- error/fallback/default paths;
- alternate versions and deprecated handlers;
- dangerous sinks only after identifying reachable untrusted input.

For each control trace all callers, sibling implementations, and the protected
operation. A strong control in one route is a map to where another route may
omit it.

### 7.2 Differential code review

Use diffs and tests to derive hypotheses:

- What invariant did a patch attempt to restore?
- Are all equivalent routes and representations covered?
- Does validation occur before the last transformation?
- Is the fixed value the one the consumer later uses?
- Are failure, retry, rollback, and async paths covered?
- Did a deny-by-default change introduce a permissive fallback elsewhere?
- Are deployment configuration and effective versions consistent with source?

A source-level omission is `SUPPORTED-SOURCE` until reproduced on the requested
target/revision, unless the user's objective is explicitly source-only review.
Dead code, impossible configuration, or compensating enforcement can invalidate
an apparent sink.

### 7.3 Taint plus authority analysis

Classic source-to-sink taint is incomplete. Track both data and authority:

```text
untrusted data: request URL -> preview parser -> fetch target
ambient authority: worker credential -> fetcher -> internal service
security decision: URL allowlist before redirect/DNS resolution
consumer: final network connection
```

The dangerous condition is often untrusted influence combined with a stronger
principal at the consumer. Record where authority is acquired, narrowed,
rechecked, cached, or lost.

## 8. Research ingestion without CVE roulette

At the start of an engagement and when stack evidence changes:

1. identify effective product, version/range, configuration, and enabled feature;
2. consult primary vendor advisories, specifications, fix commits, and regression
   tests; use current secondary research to discover leads;
3. extract the broken invariant and prerequisites, not just the payload;
4. map those prerequisites to observed target evidence;
5. derive the smallest safe target-specific experiment;
6. record source URL plus publication/update/retrieval dates;
7. reject the lead if applicability cannot be established.

Use OWASP WSTG and API Security Top 10 as coverage indexes, PortSwigger Web
Security Academy and primary research as technique references, protocol RFCs and
vendor documentation for intended behavior, and program policy as the authority
for eligibility. Recheck living documents at execution time.

Reserve part of the campaign for architecture-specific and newly published
hypotheses, but do not let novelty crowd out ordinary authorization, lifecycle,
and integration failures. A new bypass is usually a reusable disagreement
pattern—parser vs parser, policy vs alternate path, stale authority vs current
state—not a magic string.

## 9. Automation that increases judgment

Automate repetition and comparison, not conclusions.

High-value automation:

- endpoint clustering by operation, object type, auth context, and response shape;
- OpenAPI/GraphQL/client-to-live-interface diffing;
- request replay with exactly one controlled dimension changed;
- semantic JSON/HTML/artifact comparison with raw evidence retained;
- role/tenant/state matrix generation;
- token and signed-artifact metadata inspection without secret disclosure;
- bounded callback correlation with unique markers;
- source caller/consumer tracing and patch-diff extraction;
- coverage-ledger updates and deduplication by root decision.

Low-value automation:

- firing unrelated payloads at every parameter;
- counting scanner alerts as findings;
- retrying masked requests with cosmetic spelling changes;
- accepting a template's severity or CVE match without prerequisites;
- declaring a class absent because bundle grep or a smoke test was negative.

Before selecting a tool, state the question it answers and the authoritative
signal it can observe. If the tool fails, diagnose the tool; do not convert the
failure into target evidence.

## 10. Learn across the campaign

Every supported and refuted hypothesis should make the next choice better.
Record:

```text
observed mechanism:
decision point and consumer:
required actor/object/state/configuration:
decisive positive and negative artifacts:
root cause family:
affected and unaffected siblings:
strongest benign explanation tested:
reopen condition:
new hypotheses generated:
```

### 10.1 Expansion rules

Expand when evidence identifies a shared decision or consumer:

- one missing ownership check -> inspect sibling operations using the same object
  loader and alternate loaders;
- one stale capability -> inspect revocation across artifacts issued by the same
  authority service;
- one representation mismatch -> inspect components sharing those parsers;
- one worker policy gap -> inspect jobs handled by the same worker identity;
- one cache partition flaw -> inspect responses using the same key construction.

Do not inflate one root cause into many reports. Expansion measures blast radius
and finds distinct broken boundaries; reporting follows deduplication rules.

### 10.2 Pivot rules

Pivot when:

- controls show the suspected component was never reached;
- the oracle cannot distinguish the hypothesis;
- repeated results are explained by normal state, caching, or public behavior;
- prerequisites are absent at the effective revision;
- a higher-information frontier becomes available;
- the remaining proof would require prohibited impact.

Preserve the exact killed claim. Do not close an entire class from one endpoint,
role, state, or representation.

### 10.3 Campaign completion

Finish when planned applicable coverage is resolved, the agreed budget or user
stop is reached, or a blocker affects every remaining item. Finding one shortest
path is never a campaign stop condition. Before finishing, account for every
known in-scope item and reconcile:

- tested boundaries and terminal observations;
- verified findings and shared root causes;
- limited negative evidence with exact conditions;
- blocked/inconclusive hypotheses and what would unblock them;
- not-applicable surfaces with evidence;
- untested areas and why;
- cleanup and retained redacted artifacts.

No finite campaign proves a target “secure.” It can establish only the tested
properties under the recorded conditions.

## 11. Anti-patterns that suppress real findings

Avoid these campaign failures:

- **Payload-first hunting:** no product model, so probes target low-value inputs.
- **Endpoint-order testing:** path enumeration dictates priority instead of value
  and trust boundaries.
- **Single-hypothesis fixation:** one anomaly consumes the budget while independent
  high-value surfaces remain untouched.
- **Status-code authorization:** 200/403/404 substituted for protected content or
  persisted state.
- **One-account overclaim:** same-owner results generalized to cross-user/tenant.
- **Scanner closure:** tool completion mistaken for coverage.
- **CVE fingerprinting:** banner or package name treated as effective vulnerable
  code and configuration.
- **Mutation roulette:** variants do not distinguish competing explanations.
- **Speculative chains:** compatible principals, state, and prerequisites are not
  proven edge by edge.
- **Premature global negatives:** one route or version treated as class-wide.
- **Unsafe proof escalation:** impact increased after the invariant is already
  demonstrated.
- **Report-count optimization:** one cause split into duplicates, lowering signal.

## 12. Compact start-of-hunt checklist

```text
[ ] Policy cached; scope and prohibited actions resolved
[ ] Success predicate and authoritative terminal signal named
[ ] Controlled identities/tenants/roles distinctly labelled
[ ] Product journeys and high-value objects walked
[ ] Value, actor, object/state, and trust-decision maps drafted
[ ] Alternate channels, workers, artifacts, and integrations identified
[ ] Initial invariants mined from intended behavior
[ ] Diversified hypothesis portfolio ranked by frontier value
[ ] Every known in-scope item has an explicit coverage state; none vanished on pivot
[ ] Each active experiment has a control, oracle, budget, and stop rule
[ ] Raw evidence and semantic comparison paths prepared
[ ] Matching class skills and current primary research loaded as needed
[ ] Results update the model, coverage ledger, and next experiment
[ ] Findings pass validation gates; negatives and blockers stay bounded
```

The objective is not maximum activity. It is maximum **verified security-relevant
information per safe experiment**, converted into reproducible findings and
complete accounting of applicable coverage. Efficiency changes the order and
design of tests; it never excuses an unrecorded gap or a premature global claim.
