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

## CORE PRINCIPLES

- **Prove the boundary, then stop.** Demonstrate the *minimum* needed to show
  the boundary is broken. Do not escalate further unless the user asks.
- **Expected behavior is not a finding.** Before testing anything, work out
  what the feature is *supposed* to do. Reading a public profile as a logged-in
  user is the feature working, not IDOR.
- **Minimize state-changing proof.** Use owned disposable fixtures where authorized. Never delete, modify, or lock
  out real user data; never exfiltrate more than a few records needed to prove
  exposure; no DoS, no persistence, no access beyond the single boundary.
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

Run the **3x3 actor/resource matrix** (anonymous / User A / User B × own /
other / nonexistent) on every resource endpoint, and maintain a control ledger
where each control is a row with a test and a result. See
`control-verification` for the full per-class falsification catalog — authn,
authz, tenant isolation, validation, encoding, rate limits, session, cache,
file access, error handling — and the false-negative traps (`/v1` vs `/v2`,
`GET` vs `DELETE`, body id vs path id, GraphQL, bulk endpoints, exports).

**Verify broadly, escalate narrowly.** Test every control in the ledger; go
exactly one layer past any single boundary, and only when asked.

## ROUTING — WHICH SKILL FOR WHICH SURFACE

| Surface | Skill |
|---------|-------|
| Any resource endpoint, authZ, tenant boundary | `control-verification` |
| Manual, no-source, high-stakes, scanner came back clean | `manual-blackbox` |
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
- Around five minutes without new evidence: checkpoint and reconsider the next experiment.
- Around twenty minutes without discriminating signal: pause or pivot with assumptions recorded.
- 1-hour rule: nothing promising → report current status, don't keep hammering.
- Generate hypotheses BEFORE payloads.
- Time-box reads so a slow endpoint never eats the session.

## HUNT DISCIPLINE (session-level rules — read before the first request)

These rules exist because the most expensive hunt failure is not a missed
vuln — it is hours of unattributed probes and half-finished programs.

**Before the first active request, in this order:**
1. Load the relevant class skill(s) (`skill_manage load`) — including this one.
2. Open the `todo` list: one item per planned surface + the kill criteria
   ("retire after 3 dead hypotheses", "no discriminating signal in a block →
   pivot and log it"). No todo list → no active testing.
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

**Multi-program protocol.** Hunt one program to a deliberate stop: a
confirmed finding, or a written kill decision. Do not jump between programs
mid-investigation. A killed program stays dead — returning to it requires NEW
evidence (new scope, new recon, new intel, or new code), not just unspent
frustration. Record each stop: `KILLED After testing <hypotheses>: <what
ruled it out>. Revisit only with: <specific new evidence>`.

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

### PHASE 0: RECON (5 min — see recon, recon-js, recon-deep, tool-doctor)

```bash
# 0. Tools one at a time (tool-doctor); missing → install or fallback, never stall
#    bash scripts/install_bugbounty_tools.sh go     # ProjectDiscovery pipeline
#    bash scripts/install_bugbounty_tools.sh --verify
# 1. Fingerprint first (headers, framework, WAF) — framework routes, not generic wordlists
# 2. JS bundle before fuzzing (recon-js): chunks → endpoints → secrets
# 3. Subdomains via native tools only — NEVER websearch for enumeration
subfinder -d TARGET -all | dnsx -resp | httpx -sc -title -tech-detect
ffuf -u TARGET/FUZZ -w common.txt -mc 200
katana -u TARGET -d 3 -jc | grep -oE "/api/[a-zA-Z0-9/_-]+" | sort -u
curl -s TARGET/.well-known/security.txt; curl -s TARGET/robots.txt
```

### PHASE 1: MAP (5 min)
```
PRIORITY 1: Payment, admin, auth, file upload, webhooks, tenant boundaries
PRIORITY 2: API endpoints with IDs, data export, GraphQL
PRIORITY 3: Static assets, docs, health checks
```

### PHASE 2: ASSESS — DIFFERENTIAL TESTING
Core: Compare **baseline** (expected) vs **test** (attack) request. The
**difference** is a signal to explain, not a finding. Attribute it to a
broken security invariant using controls; exclude caching, routing, session
state, and normal variability. No difference establishes only what this
experiment could detect, not that the control works universally.
Run the full **3x3 actor/resource matrix** on every resource endpoint
(control-verification) — the bug is in the cell nobody tested.

```
IDOR:    User A → User B's resource → should FAIL
SSRF:    External URL → 200, Internal URL → should FAIL
XSS:     Normal input → safe, Payload → should FAIL (encoded)
SQLi:    Normal query → expected, Injection → different result
Auth:    Valid token → 200, Invalid → should FAIL (401/403)
Race:    1 request → 1 success, 20 parallel → should FAIL (still 1)
Method:  GET auth'd, DELETE unauth'd → should FAIL
Version: /v2 auth'd, /v1 unauth'd → should FAIL
```

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

## SEVERITY TRIAGE (not payout triage)

Two different models, for two different jobs. Do not mix them.

```
RECON TRIAGE — deciding what to spend time on
  Score = Impact(1-5) × Exploitability(1-5) × Confidence(1-5)
  60-125 → TEST NOW | 30-59 → TEST NEXT | <30 → SKIP

REPORT SEVERITY — what goes in the finding
  CVSS v4.0 base, computed with a calculator, demonstrated metrics only.
  See severity-engine.
```

The recon heuristic is a prioritization aid and is fine as one. It is **not**
valid in a report: it is not CVSS, and folding confidence into a severity score
conflates "how sure am I" with "how bad is this". Use the scoring standard required by the program; keep confidence separate from severity.

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

Priority order: auth/authZ → backend-reaching input → business logic →
info disclosure → UI-only last. Checkpoint notes every ~5 min.
Differential testing (authed vs unauthed) for everything auth-adjacent.
Filter at fetch time (`head`, `--max-time`, status-only flags). No-finding
checklist before declaring clean.

## IMPACT SUFFICIENCY (the anti-escalation rule)

Once a boundary crossing is proven, the review is **done**. Write it up.

```
IDOR read proven      → STOP. Do not attempt write access, then admin.
SSRF blind proven     → STOP. Do not scan internal ranges or pull cloud metadata.
Open redirect proven  → STOP. Do not build an OAuth abuse chain uninvited.
XSS reflected proven  → STOP. Do not attempt to steal a real admin session.
```

Escalate further only when:
- the base finding is already verified **and**
- the user explicitly asks for deeper impact, **and**
- the next step is still within the scope file's rules.

Where a chain is genuinely relevant, describe it as an analyst note ("this may
compound with an XSS on the same origin") and let the owner decide. Do not
execute the chain to make the report look stronger.

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

State coverage honestly. A short, accurate "clean" report is worth more than a
long list of unverified suspicions.

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

- **Intelligent task chaining** — tools are executed in optimal order
- **Coverage tracking** — completed, blocked, and untested tasks stay explicit; no completeness guarantee
- **Progress tracking** — real-time status of all phases
- **Smart retry** — failed tasks are retried with alternative approaches
- **Parallel execution** — independent tasks run concurrently

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

## REAL-WORLD LESSONS — THE PATTERNS THAT ACTUALLY PAY OUT

These are cross-cutting patterns observed across multiple engagements. Each
one is detailed in its specific skill; this section is the index.

| # | Lesson | Skill |
|---|--------|-------|
| 1 | **JS bundle is the highest-yield first step.** Every API endpoint, auth config, and sometimes hardcoded secrets live in frontend JS. Analyse it before any wordlist. Chain the chunks — endpoints live in lazy-loaded bundles too. | `recon-js` |
| 2 | **`alg:none` JWT bypass is trivial to test.** `jwt forge --algorithm none` produces a valid token in one call. Run it on every token-bearing endpoint — it is the cheapest highest-severity test in the arsenal. | `hunt-jwt` |
| 3 | **Small wordlists win.** A 47-word list built from company vocabulary cracked an 8-digit numeric admin password in seconds. Try the 5 default credentials first, then a <100-word target-derived list. | `credential-attack` |
| 4 | **`curl -F` sends `application/octet-stream`.** The server-side MIME filter sees this even for valid images. Manual multipart with an explicit `Content-Type` on the part is required to test upload filters honestly. | `control-verification` |
| 5 | **Every destructive action must be reversible.** Create a test artifact, test on it, delete it, then **verify deletion by re-querying the API** — do not trust the 204. If you cannot restore it, do not touch it, and report the damage honestly. | `manual-blackbox` |
| 6 | **CORS `*` + unauthenticated API = full data breach from any malicious website.** This compound risk turns a broken API into a security advisory. | `manual-blackbox` |
| 7 | **`grep -oE` with `-` mid-class fails silently.** A null grep result looks like "no endpoints found". Verify on a known-good line before trusting it. | `recon-js` |
| 8 | **Windows `cmd.exe` interprets `&` in URLs.** Write URLs to files or use env vars to avoid shell parsing issues. | (general) |
| 9 | **A clean target is a good result.** One of three targets had zero findings after thorough testing. That is a deliverable. | `manual-blackbox` |
| 10 | **Rate-limit behaviour is the finding, not an obstacle.** Record the threshold; do not try to defeat it. | `credential-attack` |

### The highest-yield sequence for an unknown SPA

1. Download the JS bundle(s) — all chunks, not just the main one (`recon-js`)
2. Extract endpoints, auth config, secrets (`recon-js` section 3)
3. Test bundle-derived endpoints **unauthenticated** first (`hunt-api`)
4. On any token-bearing endpoint: run the `alg:none` forge test (`hunt-jwt`)
5. On any login endpoint: try the 5 default credentials (`credential-attack`)
6. On any upload endpoint: construct multipart manually (`control-verification`)
7. Maintain the control ledger; reassess equivalent results using controls

This sequence found a full compromise on one of three targets. On the
other two it produced clean, honest results.

## REPORTING TEMPLATE
```
## Title: [Vuln] in [Endpoint] allows [Impact]
## Summary: [1 paragraph]
## Steps to Reproduce: [Copy-paste requests]
## Impact: [N users, data type, $, CVSS]
## Fix: [1-2 sentences]
```
