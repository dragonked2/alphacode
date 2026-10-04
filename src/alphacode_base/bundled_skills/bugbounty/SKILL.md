---
name: bugbounty
description: "Security assessment and review — differential testing, 7-gate validation, hypothesis-driven, evidence-first. Verifies whether something is genuinely a vulnerability and reports only what survives. Use when the user mentions bug bounty, security review, authorized pentesting, vulnerability research, recon, or defensive security testing."
auto-invoke: true
aliases:
  - bug bounty
  - bug bounties
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
- **No destructive or state-changing proof.** Never delete, modify, or lock
  out real user data; never exfiltrate more than a few records needed to prove
  exposure; no DoS, no persistence, no access beyond the single boundary.
- **Negative results are results.** "Tested and clean" is a real deliverable.
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

## PACING RULES
- 5-min rule: stuck on one hypothesis → note it and move on, don't grind.
- 20-min rotation: no signal → next target or next technique.
- 1-hour rule: nothing promising → report current status, don't keep hammering.
- Generate hypotheses BEFORE payloads.
- Time-box reads so a slow endpoint never eats the session.

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
**difference** is the finding. If there is no difference, the control works.
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
G6 NO FALSE POSITIVE → G7 PROGRAM ACCEPTS
FAIL any gate → REJECT. No exceptions.
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
Negative results: what was tested and found clean
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
conflates "how sure am I" with "how bad is this". Programs score with CVSS v4.0.

**Impact is measured by what you demonstrated, not by what a chain *could*
reach.** Report the demonstrated severity even when a theoretical chain would
score higher — and put the chain in a separate **analyst note** section, never
in the impact paragraph. A chain blended into the impact statement makes the
whole report speculative, and the proven half gets discounted with it
(see chain-reasoning, severity-engine).

Track hypotheses with kill rules and time budgets (see runbook 10.4):
3 identical results (404/401/sanitized) → KILL. Killed stays dead.

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
## Confirmed Not Vulnerable
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

### Tool Chaining (Automated Pipeline)

The orchestrator chains tools intelligently for maximum coverage:

```
Phase 1: Reconnaissance
  subfinder -d TARGET -all | anew subs.txt
  amass enum -d TARGET -passive | anew subs.txt
  assetfinder --subs-only TARGET | anew subs.txt
  dnsx -d TARGET -resp | anew subs.txt
  cat subs.txt | httprobe -c 50 | tee live.txt

Phase 2: Attack Surface Mapping
  cat live.txt | httpx -sc -title -tech-detect -web-server -cdn | tee httpx.txt
  katana -u TARGET -d 3 -jc -kf all | anew urls.txt
  gau TARGET | anew urls.txt
  waybackurls TARGET | anew urls.txt
  cat urls.txt | unfurl -u domains | anew domains.txt
  cat urls.txt | unfurl -u paths | anew paths.txt
  cat urls.txt | unfurl -u keys | anew keys.txt
  cat urls.txt | unfurl -u values | anew values.txt
  feroxbuster -u TARGET -r -d 3 | anew dirs.txt
  gobuster dir -u TARGET -w common.txt | anew dirs.txt

Phase 3: Vulnerability Discovery
  cat live.txt | nuclei -severity critical,high,medium -c 50 | tee nuclei.txt
  nikto -u TARGET | tee nikto.txt
  cat urls.txt | gf xss | dalfox pipe | tee xss.txt
  cat urls.txt | gf sqli | sqlmap -u --batch --level 2 --risk 2 | tee sqli.txt
  cat live.txt | corsy | tee cors.txt
  cat live.txt | crlfuzz | tee crlf.txt

Phase 4: Exploitation
  cat urls.txt | gf sqli | sqlmap -u --batch --level 3 --risk 3 --dbs | tee sqli_exploit.txt
  gobuster dir -u TARGET -w big.txt -x php,html,js | tee gobuster_exploit.txt
```

### Orchestrator Mode

The bug bounty orchestrator provides automated workflow management:

- **Intelligent task chaining** — tools are executed in optimal order
- **Never-miss guarantee** — every task is tracked and completed
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

## CONTROLS AND FILTERS — REPORT, DON'T ROUTE AROUND

WAFs, input filters, and rate limiters are **findings-in-waiting**: a filter
that sanitizes one payload class but not its siblings is a real (lower-
severity) issue worth reporting. The professional move is to characterize the
control and report the gap — not to spend the session evading it.

If a payload is blocked: record it as a **control observation** (which input,
which filter, what it returned) and move on. That is a legitimate result.

Variants worth *one* quick check when characterizing a filter — report any
inconsistency you find, then stop:
```
double encoding (%2527), case variation, unicode normalization,
comment injection (SEL/**/ECT), parameter pollution
```

Do not build an evasion ladder. Sustained filter bypass is out of scope for a
review engagement and puts the owner at risk.

---

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
7. Maintain the control ledger; kill hypotheses with 3 identical results

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
