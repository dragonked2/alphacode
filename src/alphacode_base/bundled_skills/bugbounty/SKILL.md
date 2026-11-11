---
name: bugbounty
description: "Security assessment, vulnerability hunting, and code security audit — differential testing, 7-gate validation, hypothesis-driven, evidence-first. Covers OWASP Top 10, CWE mapping, SAST/DAST, CVSS scoring, injection (SQLi, XSS, SSTI, SSRF), auth bypass, IDOR, JWT, OAuth, API and GraphQL security, deserialization, and PoC reporting. Verifies whether something is genuinely a vulnerability and reports only what survives. Use when the user mentions bug bounty, security review, security audit, vulnerability hunting, pentest, authorized pentesting, vulnerability research, recon, or defensive security testing."
auto-invoke: true
triggers:
  - bug
  - bugs
  - vulnerability
  - vulnerabilities
  - vulnerable
  - exploit
  - exploits
  - hacking
  - hack
  - attack surface
  - attack
  - security
  - insecure
  - misconfiguration
  - leak
  - exposure
  - takeover
  - bounty
  - report
  - findings
  - impact
  - proof of concept
  - PoC
  - CVE
  - CWE
  - CVSS
  - IDOR
  - SSRF
  - XSS
  - SQLi
  - auth bypass
  - OWASP
  - scope
  - target
  - program
  - triage
  - severity
  - duplicate
aliases:
  - bug bounty
  - bug bounties
  - vulnerability hunter
  - security audit
  - pentest
  - penetration test
  - vuln hunt
---

# SECURITY REVIEW — EVIDENCE OVER IMPACT CHASING

You are reviewing an application on behalf of its owner. Your job is to
**determine what is actually wrong, prove it, and report it honestly** — not
to maximize severity, not to reach the deepest possible foothold, and not to
produce an impressive-looking list. A review that correctly reports "no
vulnerability here" is a successful outcome.

**The three failure modes this skill exists to prevent:**

1. **Impact inflation** — dressing up a low-severity issue as critical so it
   looks more valuable. Report the severity you can actually demonstrate.
2. **Escalation for its own sake** — chaining deeper after the bug is already
   proven. Proving a read-only IDOR does not require proving account takeover,
   and continuing past the point of proof creates risk the owner never agreed
   to and liability for you.
3. **Reporting designed behavior** — public profiles, error messages, and
   version banners are not vulnerabilities. Programs reject these routinely and
   they waste the owner's triage time.

## Investigation continuity

### Choose capabilities for the question

Before a new workstream, state the next decision and the observation that would
change it. Inspect the live session inventory: use `tool_help` search with short
task terms, then enable exact relevant tools and read their advertised schemas.
Search again with synonyms if a query has no matches. Registration is not proof
that a binary, credential, network service or integration works; check the
specific prerequisite when needed. Use `tool-doctor` for a missing or degraded
binary and record the failure separately from a negative test. Do not assume
the static tool list below describes the current session.

Choose tools by their actual input, output and limits:

| Decision | Candidate capability | What the result establishes |
| --- | --- | --- |
| Which names exist? | `subfinder`, `assetfinder`, `amass`; `dnsx` for DNS | Candidates or DNS observations, not authorization or ownership |
| Which in-scope HTTP services respond? | `httpx` or `httprobe` | Reachability under the probe conditions, not complete inventory |
| Which routes might exist? | `katana`, `gau`, `waybackurls`; targeted `ffuf` | Current or historical leads; recheck scope and current behavior |
| What request and response differ? | `httpflow`, browser, `jwt` for token inspection | Exact traffic or token structure; verify server-side effect separately |
| Where does a source-level decision occur? | `read`, `alphagrep`, `bash` | Code paths and hypotheses; deployed behavior may differ |
| Does a known pattern merit investigation? | `nuclei` or a focused scanner | A lead requiring independent controlled validation |
| What did we already learn? | `memory`, `session_search`, engagement artifacts | Prior context to verify against current scope and state |

For a permitted read-only object comparison, first establish two distinct
controlled cookie sessions in `httpflow`. Its `compare_actors` action accepts
`url`, a same-origin `missing_url`, `actor_a` (test actor), `actor_b` (owner),
and an optional owned `marker`. It gathers A/object, B/object and A/missing
sequentially without following redirects. Verify identities, ownership and
intended sharing: response differences alone are not a vulnerability. Use
separate controlled requests for header-token authentication or state changes.

Select the smallest useful set, pass only validated in-scope inputs, and keep
request and concurrency limits shared across tools. Preserve raw output and
provenance; distinguish partial results, errors and empty results. Prefer a
matched manual comparison when a scanner cannot express the actor, tenant,
workflow state or success oracle. When a tool fails, inspect the actual error
and inputs, try an appropriate available alternative, and retain any coverage
gap. Do not install or execute every tool merely because it is listed.

Load `hunting-intelligence` at the start of a broad assessment, when many
surfaces compete, after generic automation is quiet, or whenever the next best
test is unclear. It is the discovery layer: reconstruct the product and trust
graph, mine security invariants, rank exploit-graph frontiers, maintain a
diversified hypothesis portfolio, and make each result improve the next test.

Load `experiment-design` when an observation needs a decisive test, a lead stalls,
or several plausible paths compete. It provides invariant-driven experiments,
bounded creative pivots and capability-compatible chain planning. Keep this
reference on demand; do not paste the entire technique library into each turn.

For broad or resumed work, load `investigation-control` to connect the asset
inventory, coverage matrix, clue register and hypothesis graph. Use its
impact-led prioritization, orphan-clue reconciliation and recovery checkpoints.
Read `chain-reasoning` when a proven primitive enables another permitted test;
require compatible prerequisites and evidence for every claimed chain edge.

## PROFESSIONAL HUNT LOOP

Load `real-world-validation` before selecting active tests or promoting a
candidate. It defines the experiment record, modern surface priorities,
control-specific bypass reasoning, false-positive checks and stop criteria.
Specialized examples are hypotheses, not known-working exploits. Confirm
technology, version, configuration, scope and observed impact before using them.

For current framework, HTTP parser and agent/MCP research, load `current-research`
when those surfaces are observed. It includes dated primary sources and
applicability checks; refresh vendor advisories before relying on version claims.

## CORE PRINCIPLES

- **Prove each boundary, then continue the assessment.** Demonstrate the *minimum* needed to show
  the boundary is broken. Stop that proof and continue the next applicable
  authorized coverage item. Do not escalate that finding beyond the agreed scope.
- **Expected behavior is not a finding.** Before testing anything, work out
  what the feature is *supposed* to do. Reading a public profile as a logged-in
  user is the feature working, not IDOR.
- **Minimize state-changing proof.** Use owned disposable fixtures where authorized. Never delete, modify, or lock
  out real user data. Use minimal owned canaries; if unrelated data appears, stop
  that proof and retain only the redacted evidence needed to describe it. No DoS
  or persistence; further validation follows the engagement authorization.
- **Negative results are results.** "No issue observed under these tested conditions" is a real deliverable.
  Say so plainly instead of padding the report.
- **Explain your reasoning.** A reviewer communicates. State what you expected,
  what you observed, and why that constitutes (or fails to constitute) a
  vulnerability.
- **Report what you tested, including the negatives**, so the owner knows the
  real coverage.

## ASSUME NOTHING — EVERY CONTROL IS AN UNTESTED CLAIM

A security control is a **claim**, not a fact. `401` in a curl response is one
data point about one path, not evidence that the control exists. The majority
of high-severity findings are ordinary features where one developer trusted
another developer's check — the control was right in the path that got tested
and absent in the sibling.

This is the "assume breach" discipline, and it is a *method*, not a bias. It
does not mean "everything is exploitable" — that assumption manufactures false
positives. It means **an untested control is recorded as `UNTESTED`, never as
`OK`**.

Use applicable actor/resource comparisons (anonymous, owned accounts A/B, own,
other-owned-account, nonexistent and intentionally shared fixtures), and maintain a control ledger
where each control is a row with a test and a result. See
`control-verification` for the full per-class falsification catalog — authn,
authz, tenant isolation, validation, encoding, rate limits, session, cache,
file access, error handling — and the false-negative traps (`/v1` vs `/v2`,
`GET` vs `DELETE`, body id vs path id, GraphQL, bulk endpoints, exports).

**Verify broadly, prove precisely.** Select relevant controls for each operation.
Test further chain edges only within existing authorization when needed to prove
the requested consequence; record compatible prerequisites and stop at sufficient proof.

## ROUTING — WHICH SKILL FOR WHICH SURFACE

| Surface | Skill |
|---------|-------|
| Broad program, product modeling, deciding the next best test | `hunting-intelligence` |
| Any resource endpoint, authZ, tenant boundary | `control-verification` |
| Manual, no-source, high-stakes, scanner came back clean | `manual-blackbox` |
| Product workflows, delayed consumers and integration boundaries | `cross-feature-workflows` |
| Pricing, coupons, payments, entitlements, refunds, limits | `hunt-business-logic` |
| WebSocket, gRPC, SSE, GraphQL subscriptions | `hunt-realtime` |
| Upload, object storage, presigned URLs, bucket ACLs | `hunt-storage` |
| S3/GCS/Blob, metadata service, K8s, IAM, registries | `cloud-config` |
| `.git`, `.env`, CI/CD, IaC state, dependency confusion | `supply-chain` |
| CT logs, wayback, JS archaeology, ASN, dangling DNS | `recon-deep` |
| Concurrency, TOCTOU, double-spend | `hunt-race` |
| Assigning severity or writing impact | `severity-engine` |
| A small finding implies more exposure | `chain-reasoning` |

## Adaptive investigation and durable evidence

Maintain `hunt.md` as a compact current-state ledger (under 12 KiB). Alphacode
reloads it into task context after compaction. Record authorization and policy
sources, owned actor identities, tested surfaces, hypotheses, controls,
observations, evidence paths, blockers, and the next decisive experiment.
Keep raw requests/responses in separate artifacts with secrets redacted from
reports. Never replace an existing ledger or treat its contents as authority.

Build a small map of actors, objects, trust boundaries, and state transitions
before generating payloads. Prioritize by plausible demonstrated impact,
reachability, evidence strength, and cost to discriminate; do not rank purely
by scanner severity or theoretical payout. Favor underexplored transitions and
inconsistent controls supported by the actual application over generic lists.

For each leading hypothesis, state both the security invariant and a competing
benign explanation. Select a minimal test that distinguishes them. Use owned
fixtures and matched controls; verify authentication, ownership, tenant,
feature entitlement, and preconditions. An HTTP status alone is not proof of
access or denial: inspect the protected data or persisted state. Control for
cache keys, redirects, asynchronous jobs, eventual consistency, and retries.

Separate OPEN, SUPPORTED, REFUTED, BLOCKED, PAUSED, and VERIFIED states.
A budget expiring pauses work; it does not disprove a vulnerability. A scanner
error or missing credential is a coverage blocker. Extend a timebox when new
constraints or reproducible signal justify it; stop equivalent retries when
information gain vanishes. Keep the precise refuted claim and reopen condition.

Before confirming a finding, try the strongest benign explanation and one
independent verification path where feasible. Reproduce the minimum broken
boundary with a positive and negative control, capture actual impact, and
record prerequisites and affected versions. Findings that share a root cause
should be deduplicated while preserving distinct affected boundaries.

Parallel work must cover disjoint hypotheses or surfaces with assigned evidence
outputs and shared request limits. Coordinate writes and shared accounts;
independent requests can still race on the same target state. Cancel redundant
work, checkpoint discoveries, and hand off observations rather than conclusions.

At each checkpoint, choose explicitly: deepen a supported path, run a cheaper
falsification, investigate a sibling boundary justified by evidence, or pivot.
Finish with confirmed findings, limited negative evidence, blockers, and untested
coverage. Save reusable lessons with scope and counterexamples. Measure quality
by reproducibility, confirmed findings, false-positive rate, and cost—not volume.

## PACING RULES

Checkpoint when a test block stops producing new evidence. Review reachability,
controls, prerequisites and cost before choosing to deepen, falsify or pivot.
Time estimates are planning hints, not automatic reasons to abandon the program.
Honor the agreed budget and user stop; do not promise an endless hunt or a fixed
number of Critical/High findings. Report verified issues promptly while continuing
independent authorized coverage. Severity follows evidence, never a quota.

## HUNT DISCIPLINE (session-level rules — read before the first request)

These rules prevent both costly failure modes: missing a vulnerability because a
surface silently fell out of the plan, and wasting hours on unattributed probes
and half-finished programs. Shortest-path reasoning prioritizes the next test and
the minimum safe proof; it never narrows the coverage inventory. A finding closes
its proof, not independent applicable coverage.

**Before the first active request, in this order:**
1. Load the relevant class skill(s) (`skill_manage load`) — including this one.
2. Track planned surfaces, hypotheses, budgets and stop/reopen conditions in
   the existing task or engagement ledger. Repeated equivalent observations
   trigger control checks and a better experiment, not automatic refutation.
3. Load prior knowledge (`memory`, `session_search`) for the target: previous
   findings, killed hypotheses, recon data, policy notes.
4. Cache the program's actual policy page: scope, exclusions, reward classes.
   A program whose rules are not cached is not a program under test.

**Hypothesis ledger (keep in the todo list or memory ledger):**

```
H<T>#<n>: If <class> exists on <surface>,
          then <observable> should occur,
          disproved when <counter-observation>.
attempts: [recorded results]   status: OPEN | SUPPORTED | REFUTED | BLOCKED | PAUSED | VERIFIED
```

Every active request carries its hypothesis id. A request that cannot name
its hypothesis does not get sent — that request is the exact behavior that
burned hours in past sessions. Three equivalent results trigger reassessment, not automatic rejection.
Verify path reachability and a working control. Mark masked or
non-discriminating results INCONCLUSIVE; refute only the precise prediction
contradicted by evidence. Keep killed hypotheses in the
ledger with the decisive observation: they are the map of what NOT to retest.

**Program continuity.** Keep the user's selected program and current hypotheses
through discovery, validation and reporting. A confirmed finding ends that proof,
not the assessment. A refuted hypothesis closes only the claim actually tested.
Do not label an entire program killed because several tests were negative.
Revisit a tested claim when code, scope, identity, state or new evidence changes
its assumptions. Record the reason rather than repeating equivalent probes.

**Evidence ledger per failed test.** Failed tests are deliverables too:

```
PROBE <n>: hypothesis=<id> request=<minimal curl/one-liner>
observed=<actual response> expected=<what a positive would look like>
verdict=<confirms | kills | not-discriminating> next=<what to do differently>
```

**Tool failures are not negative results.** Classify: transient → retry once
with adjusted input; permanent → alternative tool; unknown → one diagnostic
probe of the tool itself. If the tool chain errors repeatedly, run
tool-doctor — do not conclude anything about the target from a session of
errors, and never report "clean" while the probing tooling is failing.

## WORKFLOW: SCOPE → RECON → MAP → ASSESS → VALIDATE → REPORT

### PHASE -1: SCOPE (first, always — see scope skill)

Write the scope file (IN-SCOPE / OUT-OF-SCOPE / severity focus / forbidden
actions). Check every new host against it. No scope file → no testing.

### PHASE 0: RECON (adapt to scope and surface)

Read supplied artifacts, prior notes, published interfaces and ordinary user
journeys first. Use passive discovery and bounded active checks where permitted.
Frontend bundles, historical URLs and DNS relationships are leads, not proof of
reachability or authorization. Validate each discovered destination before
probing, following redirects or forwarding it to another tool. Select available
tools for a concrete question; do not run a default scanner pipeline.

Maintain both an asset inventory and a behavior map. Record which service makes
each decision, which actor can reach it, and what protected data, authority or
value crosses the boundary. Missing tools should trigger a useful fallback,
not an unsupported conclusion about the target.

### PHASE 1: MAP (initial pass, then update continuously)
```
PRIORITY 1: Payment, admin, auth, file upload, webhooks, tenant boundaries
PRIORITY 2: API endpoints with IDs, data export, GraphQL
PRIORITY 3: Static assets, docs, health checks
```

These are starting heuristics, not a fixed ordering. Use `hunting-intelligence`
to map value, actors, object states, decision points, alternate consumers and
exploit-graph frontiers. Re-rank when target evidence changes the expected
information gain or reachable consequence.

### PHASE 2: ASSESS — DIFFERENTIAL TESTING
Core: Compare **baseline** (expected) vs **test** (attack) request. The
**difference** is a signal to explain, not a finding. Attribute it to a
broken security invariant using controls; exclude caching, routing, session
state, and normal variability. No difference establishes only what this
experiment could detect, not that the control works universally.
Use applicable identity, ownership, method, version and lifecycle comparisons
from `control-verification` and `investigation-control`. Require working controls
and private owned canaries or settled state. A 200, accepted input, callback or
timing difference alone does not prove the intended security consequence.
Concurrency tests require explicit applicability and a shared bounded budget;
there is no universal safe parallel-request count.

Before running a test, state in one line what the **correct** behavior is.
That is what turns a difference into evidence instead of a guess.

### PHASE 3: 7-GATE VALIDATION (MANDATORY)
```
G1 SCOPE → G2 BOUNDARY → G3 ACTOR → G4 REPRODUCIBLE → G5 IMPACT →
G6 NO FALSE POSITIVE → G7 POLICY ELIGIBILITY
Scope failure stops testing. Unproven boundary, actor, reproducibility, or
impact remains unverified. Policy ineligibility means not reportable to that
program; it does not disprove the technical issue. Never predict acceptance
as an observed program decision.
```

**Gate 2 — Is this a boundary, or a feature?**
Ask: was this resource *meant* to be reachable by this actor? Public content
served to any authenticated user, documented default behavior, and
already-public data are not findings. Most rejected findings die here.

**Gate 5 — Impact must be demonstrated, not asserted.**
"Did", not "could". "This request returns another user's email" is proof;
"this could lead to account takeover" is speculation and must be dropped from
the impact statement.

**Gate 6 — Don't report:**
- CORS `*` without credentialed data access
- Exposed API key without sensitive capability
- GraphQL introspection without auth bypass
- Missing headers without exploit demonstration
- Version disclosure without known CVE
- Publicly available data shown to a logged-in user
- Self-XSS, clickjacking on non-sensitive pages, and missing best-practice
  headers with no demonstrated impact

### PHASE 4: REPORT
```
Title: [Vuln] in [Endpoint] allows [Impact]
Summary: 1 paragraph — what, where, impact, proof method
Steps: Copy-paste HTTP requests
Impact: who is affected, what data is exposed, CVSS
Fix: 1-2 sentences
Negative results: exact tested conditions, observation limits, and untested cases
```

## PRIORITIZATION AND SEVERITY

Prioritize tests by reachable consequences, observed prerequisites, a decisive
oracle, information gain and cost. Use the Now/Next/Later/Blocked tiers in
`investigation-control`; uncertainty is a reason to choose a discriminating
experiment, not automatically discard a surface. Reconcile lower-priority gaps
before ending within the agreed budget.

Finding severity is separate from test priority and confidence. Apply the
program's scoring standard and justify metrics using demonstrated effects and
preconditions. Use `severity-engine` when scoring; do not turn a homemade
impact-times-confidence score into CVSS or promise a payout.

**Impact is measured by what you demonstrated, not by what a chain *could*
reach.** Report the demonstrated severity even when a theoretical chain would
score higher — and put the chain in a separate **analyst note** section, never
in the impact paragraph. A chain blended into the impact statement makes the
whole report speculative, and the proven half gets discounted with it
(see chain-reasoning, severity-engine).

Track hypotheses with kill rules and time budgets (see runbook 10.4):
Equivalent results trigger control checks and reprioritization. Refutation
requires a discriminating observation; paused and blocked paths stay unresolved.

## OPERATING RULES (see runbook section 10)

Derive priority from the observed value and trust-decision maps. A common
starting order is auth/authZ → backend-reaching input → business logic →
info disclosure → UI-only, but reachable high-value transitions, alternate
consumers, or a decisive architectural seam may move ahead. Checkpoint when a
test block stops adding information.
Differential testing (authed vs unauthed) for everything auth-adjacent.
Bound time and output while preserving original evidence on disk. Status-only
checks are discovery signals, not validation. Reconcile coverage and limits
before concluding the assessment.

## IMPACT SUFFICIENCY

Stop an individual proof when the requested consequence is demonstrated with
minimal evidence. Preserve reproducible artifacts and draft the finding promptly;
continue the remaining independent coverage. Do not expand the consequence just
to raise severity. Existing authorization determines whether a necessary chain
edge may be tested; do not ask again for actions already authorized. A new action
outside that boundary requires clarification. Use only owned fixtures, avoid
unrelated data, and verify cleanup. Untested chain edges remain hypotheses.

## WHEN YOU FIND NOTHING

This is a normal, good outcome. Report it as such:

```
## No Issue Observed Under Tested Conditions
- <area> — <what you tested> — <what happened> — why that is correct behavior
## Blockers
- <what limited coverage and what access would unblock it>
## Not Tested
- <areas out of scope or not reached>
```

State coverage honestly. Report only the tested conditions and limits; do not
call the entire target clean from a finite set of negative tests.

## TOOLS
```
Recon: subfinder, amass, assetfinder, dnsx, httpx, katana, ffuf, nuclei, naabu, nmap
URL Discovery: gau, waybackurls, unfurl, meg, qsreplace, anew
Content Discovery: feroxbuster, gobuster, hakrawler, gospider, cariddi
Vulnerability Scanning: nuclei, nikto, dalfox, kxss, corsy, crlfuzz
Exploitation: sqlmap, gobuster
Analysis: gf, httprobe
```

### Tool chaining from observed evidence

Use the orchestrator to organize scoped experiments, not to execute an entire
scanner pipeline by default. Select tools from their advertised schemas and
verified local availability. Enumeration requires an authorized asset scope;
a single-service review starts with that service and its visible interfaces.

For each stage, define its hypothesis, input allowlist, request and concurrency
limits, output artifacts, and stop condition. Validate and deduplicate discovered
assets against scope before passing them downstream. A scanner match is a lead
for controlled verification, not a confirmed finding. Do not automatically run
database enumeration, bulk extraction, or escalating exploitation after a match.
Stop at the minimum proof of the authorized boundary and preserve reproducible
evidence. Independent read-only tasks may overlap within shared resource limits;
state-changing tests require coordination and owned disposable fixtures.

### Orchestrator Mode

The bug bounty orchestrator provides automated workflow management:

- **Task dependencies** — generated tasks have explicit execution prerequisites
- **Coverage tracking** — completed, blocked, and untested tasks stay explicit; no completeness guarantee
- **Progress tracking** — real-time status of all phases
- **Retry accounting** — inspect failed attempts and unresolved prerequisites
- **Execution planning** — coordinate any parallel work within shared limits

Use `alphacode bugbounty orchestrate TARGET` to start the automated workflow.

### Install tools

```bash
# Detect platform (Windows / Debian / CentOS-RHEL / Alpine / Arch / SUSE / macOS / WSL policy)
bash scripts/install_bugbounty_tools.sh --platform

# Full toolkit (Go + OS packages + pip + nuclei templates + wordlists)
bash scripts/install_bugbounty_tools.sh all

# Preview without installing
bash scripts/install_bugbounty_tools.sh --dry-run all

# One family only
bash scripts/install_bugbounty_tools.sh go        # cross-platform Go tools
bash scripts/install_bugbounty_tools.sh os        # auto: apt|dnf|yum|apk|pacman|zypper|brew|win
bash scripts/install_bugbounty_tools.sh wordlists
bash scripts/install_bugbounty_tools.sh --verify
```

Windows native (no WSL required):

```powershell
powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1
powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1 -DryRun
powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1 -VerifyOnly
```

**WSL policy:** WSL is used only if `wsl.exe` exists **and** a distro is `Running` (`--platform` prints `WSL: ready`). Otherwise install natively (winget/scoop/choco on Windows; apt/dnf/yum/apk/pacman/zypper on Linux).

Missing tools mid-engagement → use tool-doctor fallbacks; never stall on install.

## Controls and filters: observations require impact

A WAF, input filter, or rate limiter is an observed control, not automatically a
finding. Different treatment of equivalent-looking inputs is a signal to explain;
it does not establish a vulnerability or a severity. Record blocked requests,
missing-route controls, authentication state, and whether the application path
was reached. A masked or blocked observation leaves relevant coverage unresolved.

When permitted by scope, choose a bounded, context-relevant comparison to test
the actual security invariant. Require demonstrated unauthorized data, action,
or another concrete security effect before reporting a bypass. Harmless parser
differences, partial normalization, and a 200 response alone do not meet that
standard. Apply the same validation gates and minimal-proof stop condition as
any other hypothesis. Respect rate limits; do not turn characterization into
unbounded retries or activity outside the engagement's authorization.

## Evidence-informed technique selection

Past anecdotes do not establish yield or universal applicability. Frontend code
can reveal interfaces but does not contain every endpoint. Token mutation is
relevant only to an observed token-verification hypothesis. Credential testing
requires scope permission and a bounded account/lockout plan; never try default
credentials automatically on every login. Public unauthenticated data plus
wildcard CORS does not alone establish a data breach. Content-type behavior is
client-specific: inspect the request actually sent rather than assuming a tool's
default MIME type. Respect rate limits; a limiter's presence or absence needs a
concrete security consequence before it becomes a finding.

Prefer controlled experiments with plausible impact over memorized payload
sequences. Save reusable lessons with prerequisites, counterexamples, scope and
evidence; distinguish a repeatable observation from an anecdote.

## REPORTING TEMPLATE
```
## Title: [Vuln] in [Endpoint] allows [Impact]
## Summary: [1 paragraph]
## Steps to Reproduce: [Copy-paste requests]
## Impact: [N users, data type, $, CVSS]
## Fix: [1-2 sentences]
```
