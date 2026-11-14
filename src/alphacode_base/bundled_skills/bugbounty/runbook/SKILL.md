---
name: runbook
description: Structured security runbooks — Predefined workflows for common security tasks. Use when following a structured security workflow, triaging a web application, mapping attack surface, or performing scoped pentest work. Keeps the agent moving through recognizable task shapes instead of random tool calls.
---

# SECURITY RUNBOOKS

**Structured workflows that keep you moving through real tasks, not random tool calls.**

> Bug bounty assessments ALSO follow section 10 (operating rules):
> scope file, checkpointing, priority order, kill rules, differential
> testing, compound-risk notes, output discipline, no-finding checklist.

---

## Assessment completion rule

Time estimates below are planning hints, not permission to stop early. Adapt
coverage to the observed surface and agreed budget. Keep pending items moving
through discovery, testing, validation and reporting without repeated consent
for work already authorized. Stop an individual proof when sufficient, then
continue independent coverage. A header-only pass or missing public CVEs does
not complete a website assessment. Record tested, inapplicable, inconclusive and
blocked items separately; resolve pending work before a final report unless a
real blocker, budget limit or user stop ends the assessment.

## 1. AVAILABLE RUNBOOKS

```
RUNBOOK                          PURPOSE                        TARGET TYPE
═══════════════════════════════════════════════════════════════════════════
appsec-web-triage                AppSec web app triage           URL
web-surface                      Web attack surface mapping      URL
network-surface                  Network attack surface          IP/CIDR
osint-target                     OSINT target research           Domain
pentest-starter                  Full pentest workflow           URL/IP/Domain
api-security-audit               API security audit              API endpoint
cloud-posture-triage             Cloud security posture          Cloud config
container-triage                 Container security              Container image
iac-triage                       Infrastructure-as-Code          IaC files
mobile-app-triage                Mobile app security             APK/IPA
```

---

## 2. RUNBOOK: appsec-web-triage

**Purpose:** Quick AppSec triage of a web application. Find the low-hanging fruit fast.

```
PHASE 1: DISCOVER (5 min)
├── Fingerprint technology stack
│   ├── Check HTTP headers (Server, X-Powered-By)
│   ├── Check HTML meta tags, comments, JS sources
│   └── Use whatweb or manual inspection
├── Find all entry points
│   ├── Login/registration forms
│   ├── Search forms
│   ├── File upload endpoints
│   ├── API endpoints (check for /api/, /graphql, /swagger)
│   └── Admin panels (/admin, /dashboard, /manage)
└── Map authentication
    ├── What auth method? (cookie, JWT, API key)
    ├── Is there MFA?
    └── Password reset flow?

PHASE 2: TEST (15 min)
├── Authentication tests
│   ├── Default credentials (admin/admin, admin/password)
│   ├── Brute force protection (rate limiting?)
│   ├── Session management (fixation, timeout)
│   └── Password policy strength
├── Authorization tests
│   ├── IDOR on user endpoints (change ID in URL/body)
│   ├── Privilege escalation (user → admin)
│   ├── Horizontal privilege escalation (access other users)
│   └── Vertical privilege escalation (access admin functions)
├── Input validation tests
│   ├── XSS on search/comment fields
│   ├── SQLi on parameterized endpoints
│   ├── Command injection on file/process endpoints
│   └── Path traversal on file endpoints
└── Configuration tests
    ├── CORS policy (reflects origin? credentials?)
    ├── Security headers (CSP, HSTS, X-Frame-Options)
    ├── Error handling (verbose errors? stack traces?)
    └── Debug mode (exposed in production?)

PHASE 3: REPORT (5 min)
├── List findings with severity
├── Prioritize by impact
├── Create reproduction steps
└── Draft report
```

---

## 3. RUNBOOK: web-surface

**Purpose:** Map the full attack surface of a web target.

```
PHASE 1: SUBDOMAIN ENUMERATION (10 min)
├── Passive
│   ├── crt.sh (certificate transparency)
│   ├── subfinder
│   ├── amass (passive mode)
│   └── SecurityTrails/VirusTotal (if available)
├── Active
│   ├── amass (active mode)
│   ├── dnsx (DNS resolution)
│   └── Subdomain brute-force
└── Output: combined subdomain list

PHASE 2: HOST DISCOVERY (5 min)
├── DNS resolution (dnsx)
├── HTTP probing (httpx)
│   ├── Status codes
│   ├── Titles
│   ├── Technology detection
│   └── CDN detection
└── Output: live hosts with metadata

PHASE 3: PORT SCANNING (10 min)
├── Fast scan (naabu top 1000)
├── Full scan (nmap -sV -sC)
├── UDP scan (if in scope)
└── Output: open ports + services

PHASE 4: DIRECTORY FUZZING (10 min)
├── Common paths (ffuf + common.txt)
├── API endpoints (ffuf + api-endpoints.txt)
├── Sensitive files (.env, .git, backup)
└── Output: discovered paths

PHASE 5: JAVASCRIPT ANALYSIS (10 min)
├── Crawl with katana/gau
├── Extract endpoints from JS
├── Find secrets/tokens in JS
├── Map API surface
└── Output: endpoints + secrets

PHASE 6: TECHNOLOGY FINGERPRINTING (5 min)
├── CMS detection (WordPress, Drupal, Joomla)
├── Framework detection (React, Angular, Vue)
├── Server detection (nginx, Apache, IIS)
├── WAF detection (Cloudflare, Akamai, AWS WAF)
└── Output: technology profile
```

---

## 4. RUNBOOK: pentest-starter

**Purpose:** Full pentest workflow from reconnaissance to exploitation.

```
PHASE 1: RECON (30 min)
├── Run web-surface runbook
├── OSINT on target employees
├── GitHub/GitLab code leaks
├── Shodan/Censys host info
├── Email harvesting
└── Output: full attack surface

PHASE 2: VULNERABILITY SCANNING (20 min)
├── Nuclei template scan (critical + high)
├── Manual testing of high-value endpoints
├── Authentication bypass attempts
├── IDOR testing on all user endpoints
├── SQL injection testing
├── XSS testing
└── Output: candidate findings

PHASE 3: VERIFICATION (30 min)
├── Confirm each candidate against its baseline (expected vs actual)
├── Rule out intended behavior before calling anything a finding
├── Build a minimal, non-destructive PoC for each
├── Check the same boundary for a WRITE path only if read was proven AND
│   the user asked for depth (note it otherwise — do not test it)
├── Measure affected scope (how many records/accounts), not just reachability
└── Output: verified findings with PoCs, or documented rejections

PHASE 4: IMPACT ASSESSMENT (15 min)
├── For each proven finding: who is affected, what data is exposed
├── CVSS from demonstrated impact — not from a theoretical chain
├── Record plausible compound risk as an analyst note, untested
├── Note anything that limited coverage (missing role, blocked path)
└── Output: severity per finding + honest coverage statement

NEVER: establish persistence, move laterally between accounts or hosts, or
exfiltrate beyond the minimum needed to prove one boundary. Those are
destructive and out of scope for a review. If the user explicitly needs
containment/incident-response work, that is a different engagement.

PHASE 5: REPORTING (15 min)
├── Document all findings
├── Create reproduction steps
├── Assign severity
├── Write remediation guidance
└── Output: final report
```

---

## 5. RUNBOOK: network-surface

**Purpose:** Network attack surface mapping for IP/CIDR targets.

```
PHASE 1: HOST DISCOVERY (5 min)
├── Ping sweep
├── ARP scan (if local)
├── TCP SYN scan (top ports)
└── Output: live hosts

PHASE 2: PORT SCANNING (15 min)
├── Full port scan (all 65535)
├── Service version detection
├── OS detection
├── Script scan (-sC)
└── Output: open ports + services

PHASE 3: SERVICE ENUMERATION (15 min)
├── Web servers (HTTP/HTTPS)
│   ├── Directory fuzzing
│   ├── Technology detection
│   └── SSL/TLS analysis
├── SSH
│   ├── Version check
│   └── Key-based auth test
├── Database ports
│   ├── MySQL (3306) — default creds
│   ├── PostgreSQL (5432) — default creds
│   ├── MongoDB (27017) — auth bypass
│   ├── Redis (6379) — unauthenticated
│   └── MSSQL (1433) — default creds
├── Mail ports
│   ├── SMTP (25/587) — open relay test
│   └── IMAP/POP3 — default creds
└── Other services
    ├── SMB (445) — null session
    ├── RDP (3389) — default creds
    ├── VNC (5900) — default creds
    └── Docker (2375/2376) — unauthenticated
```

---

## 6. RUNBOOK: osint-target

**Purpose:** OSINT research on a domain target.

```
PHASE 1: DOMAIN RECON (10 min)
├── WHOIS lookup
├── DNS records (A, AAAA, MX, NS, TXT, CNAME, SOA)
├── Certificate transparency (crt.sh)
├── Reverse DNS
└── Output: domain profile

PHASE 2: SUBDOMAIN ENUMERATION (10 min)
├── Passive (crt.sh, SecurityTrails)
├── Active (subfinder, amass)
├── DNS resolution
└── Output: subdomain list

PHASE 3: EMAIL HARVESTING (10 min)
├── theHarvester
├── Hunter.io
├── GitHub email search
├── LinkedIn employee enumeration
└── Output: email list

PHASE 4: PUBLIC DATA (10 min)
├── GitHub/GitLab code search
├── Pastebin/pastebin clones
├── Shodan/Censys
├── Google dorking
├── Wayback Machine
└── Output: public data findings

PHASE 5: INFRASTRUCTURE (10 min)
├── IP ranges (BGP info)
├── ASN lookup
├── Cloud provider identification
├── CDN detection
├── WAF detection
└── Output: infrastructure profile
```

---

## 7. RUNBOOK: api-security-audit

**Purpose:** Security audit of REST/GraphQL APIs.

```
PHASE 1: API DISCOVERY (10 min)
├── Swagger/OpenAPI endpoints (/swagger, /api-docs, /openapi.json)
├── GraphQL introspection (/graphql)
├── JavaScript analysis for API endpoints
├── Network tab analysis
├── Mobile app API calls
└── Output: API endpoint list

PHASE 2: AUTHENTICATION (10 min)
├── Auth method identification
├── Token validation
├── API key management
├── OAuth flow analysis
├── Rate limiting test
└── Output: auth assessment

PHASE 3: AUTHORIZATION (15 min)
├── IDOR on all endpoints
├── Mass assignment testing
├── Horizontal privilege escalation
├── Vertical privilege escalation
├── Function-level authorization
└── Output: authz findings

PHASE 4: INPUT VALIDATION (15 min)
├── SQL injection
├── NoSQL injection
├── Command injection
├── XXE
├── SSTI
├── Parameter pollution
└── Output: injection findings

PHASE 5: BUSINESS LOGIC (10 min)
├── Rate limiting bypass
├── Race conditions
├── Price/quantity manipulation
├── Workflow bypass
├── State machine manipulation
└── Output: logic findings
```

---

## 8. RUNNING A RUNBOOK

```bash
# List available runbooks
/runbook list

# Run a specific runbook
/runbook run appsec-web-triage https://example.com

# Run with custom target
/runbook run web-surface https://api.example.com

# Check current runbook progress
/runbook status

# Switch to next phase
/runbook next

# Pause and resume later
/runbook pause
/runbook resume
```

---

## 9. RUNBOOK PROGRESS TRACKING

```markdown
## Runbook: appsec-web-triage — example.com

**Started:** 2026-01-16 09:00
**Status:** Phase 2 (Testing)

### Phase 1: Discover ✅ Complete
- [x] Technology stack: React + Node.js + PostgreSQL
- [x] Entry points: /login, /search, /api/users, /upload
- [x] Auth: JWT tokens, no MFA

### Phase 2: Test 🔄 In Progress
- [x] Default credentials: None found
- [x] IDOR: Found on /api/users/{id} — HIGH
- [ ] XSS: Testing search field
- [ ] SQLi: Testing /search
- [ ] CORS: Checked — reflects origin with credentials — HIGH
- [ ] Security headers: Missing CSP — INFO

### Phase 3: Report ⏳ Pending
- [ ] Document findings
- [ ] Create PoCs
- [ ] Write report
```

---

## 10. BUG BOUNTY ASSESSMENT OPERATING RULES

Mandatory for any bug bounty / pentest engagement. These fix the
recurring failure modes: lost context, wasted tests, scope drift,
unreadable reports.

### 10.0 Posture: reviewer, not attacker

This governs every skill in this bundle, and it overrides any "chain to
maximum impact" phrasing you may find in a technique reference.

You are assessing an application for its owner. Concretely:

- **Report the demonstrated severity.** A proven read-only IDOR is not a
  Critical because write access might exist. Do not price a finding by what a
  chain *could* reach.
- **Stop each proof at sufficient evidence.** Continue independent coverage.
  Test a further chain edge when existing authorization and the requested
  consequence cover it; resolve only a material scope gap, not each phase.
- **Expected behavior is not a vulnerability.** Public content served to any
  authenticated user can be legitimate. Check the actual sensitivity and policy:
  a banner or error alone proves neither a vulnerability nor correct enforcement.
- **Never act beyond the boundary.** No persistence, no lateral movement, no
  bulk exfiltration, lockouts or disruptive actions. For permitted state-change
  tests use disposable owned objects, readback and cleanup. Choose minimal
  synthetic proof rather than collecting unrelated records.
- **A clean result is a good result.** "Tested these areas, all controls held"
  is a deliverable. Do not pad a report to look productive.
- **Explain your reasoning.** State what you expected, what you observed, and
  why that is (or is not) a vulnerability.

Historical bug-bounty writeups in these skills are **precedent data** — they
show which bug classes programs accept and how they were described. Read them
for technique and reporting structure, not as a payout target. A large
historical payout is not evidence that a similar bug is present or severe here.

### 10.1 Scope file (see scope skill)

Write it before any testing. Check every new host against it.
Out-of-scope or ambiguous → skip, log one line, move on.

### 10.2 Checkpointing (never lose work to compaction)

Every ~5 minutes of testing, append to the running assessment notes:

```markdown
## Checkpoint <time>
- Tested: <what, with what>
- Found: <findings or "nothing — killed H-3">
- Next: <next hypothesis>
```

Keep findings, scope file, and hypothesis ledger (10.4) in these
notes so a fresh session resumes without re-testing.

### 10.3 Target prioritization (test in this order)

1. Authentication and authorization endpoints (biggest impact).
2. User-controlled input reaching backend (SQLi, command injection, SSRF).
3. Business logic flows (payment, transactions, permissions).
4. Information disclosure enabling further attacks.
5. UI-only issues LAST (XSS without auth context, bare open redirect).

### 10.4 Hypothesis ledger with kill rules

Track each hypothesis with a kill criterion and a time budget:

```markdown
| ID | Hypothesis | Kill rule | Budget | State |
|----|-----------|-----------|--------|-------|
| H-1 | An alternate API route skips an ownership check | Observed routes and source contradict this mechanism | Engagement-defined | REFUTED under tested revision |
| H-2 | Circle API CORS → CSRF | no authed impact | 15 min | TESTING |
```

Kill rules (strict):

- 3 equivalent results → check controls and reachability; pause inconclusive tests instead of declaring the control secure.
- No information gain → pause with a recorded blind spot and reopening condition.
- Framework guess contradicts reliable stack evidence → reject that assumption;
  keep an independently supported security question open.

A refuted hypothesis is not retried under a new name with the same evidence.
Reopen it when a specific new observation, revision or policy invalidates the
prior decisive control. Paused, blocked and untested items are not refuted.

### 10.5 Differential testing

Choose controlled actor/owner/role/tenant comparisons from the actual policy,
including public/shared and absent-object controls where applicable. Verify
session validity and protected content or state, then independently reproduce it.
A difference is a lead; the invariant and authoritative consequence decide the
finding. A denial for another operation cannot validate this operation's control.

### 10.6 Compound-risk analysis (note it, don't chase it)

After a validated finding, record its exact demonstrated capability and any
conditional follow-on risk. Compare the next edge's actor, tenant, origin,
lifetime and permissions before selecting another authorized experiment:

```
IDOR read            → note: write access on the same endpoint may exist
Info disclosure      → note: leaked config could weaken auth
Open redirect        → note: on a trusted origin this aids phishing
```

Continue another edge only when the scope and requested consequence cover it;
otherwise keep it conditional. Preserve the independent original finding even
when an extension fails. Severity follows demonstrated consequence, not graph size.

### 10.7 Output discipline

- Filter at fetch time: `grep`, `head -c`, `head -100`, `--max-time 20`.
- Status-code-only questions get `-o /dev/null -w "%{http_code}"`.
- Headers-only questions get `-I`.
- Never dump a full bundle, header set, or crawl into the transcript;
  save to `evidence/` and quote the 5 relevant lines.

### 10.8 Script execution

- Prefer inline `bash -c '...'` for one-off tests.
- If you write a script file, run `pwd && ls` first and execute by
  the verified path — never assume `/tmp` or CWD.
- Never `find /` — target specific directories only.
- Reconcile recorded background task IDs on resume. Stop only processes owned by
  this engagement after checking their state; never kill unrelated user work.

### 10.9 No-finding validation checklist

Before concluding, reconcile applicable coverage and state its limits:

1. [ ] Every discovered applicable mechanism has tested, inconclusive, blocked,
       deferred or evidenced not-applicable status; a class name is not coverage.
2. [ ] Client-side code and observed endpoints have dispositions when relevant.
3. [ ] Auth mechanisms tested for BYPASS, not just rejection.
4. [ ] API responses checked for leakage enabling further attacks.
5. [ ] Third-party integrations tested for their specific patterns.
6. [ ] Severity filter applied to the report (see report skill).
