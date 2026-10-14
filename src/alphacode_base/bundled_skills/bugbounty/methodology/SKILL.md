---
name: bug-bounty-methodology
description: "Real-world security assessment methodology with program-specific workflows, time management, and reporting discipline. Structured for accurate validated findings and honest coverage."
---

# Bug Bounty Methodology — Real World

## Time Budget Per Program
```
Day 1: Recon + Map (2h) → first verified findings (or a clean result)
Day 2: Deep assessment (4h) → verification
Day 3: Report + cleanup (2h) → submit
Total: 8h per program rotation
```

## Phase 1: Program Selection (<5 min)

```bash
# Find programs with recent activity
# HackerOne: sort by last report date
# Bugcrowd: filter by "must have" and recent reports
# Intigriti: filter by "priority" programs
# Immunefi: DeFi programs

# Key selection criteria:
# 1. Scope: *.target.com (wide) vs specific endpoints (narrow)
# 2. Recent reports (<30 days) = program active
# 3. Response time: <24h = good, >7d = slow to triage
# 4. Duplicate rate: <30% = less crowded
# 5. Clear rules on safe-harbor and prohibited actions
```

## Phase 2: Recon (30 min)

```bash
# Subdomain enumeration
subfinder -d target.com -all -o subs.txt
dnsx -l subs.txt -resp -o resolved.txt
httpx -l resolved.txt -sc -title -tech-detect -o live.txt

# JS endpoint extraction
katana -u https://target.com -d 3 -jc -o endpoints.txt
grep -oE '"/api/[^"]*"' endpoints.txt | sort -u

# Secret scanning
trufflehog git https://github.com/target/repo --json > secrets.json

# Port scanning (quick)
nmap -sV --top-ports 100 -T4 target.com -oN nmap.txt
```

## Phase 3: Map (15 min)

```
PRIORITY 1 (test first):
- Payment flows (checkout, refund, subscription)
- Admin panels (admin.target.com, /internal)
- Authentication (login, reset, MFA, OAuth)
- File upload/processing
- Webhooks, callbacks, integrations
- Tenant boundaries (multi-tenant apps)

PRIORITY 2:
- API endpoints with IDs (/api/v1/users/123)
- Data export (CSV, PDF, reports)
- GraphQL endpoints
- WebSocket connections

PRIORITY 3:
- Health checks, status pages
- Documentation, API docs
- Static assets, CDN
```

## Phase 4: Hunt — Differential Testing

### Method: Compare baseline vs attack. Difference = vuln.

```bash
# IDOR — Test with two accounts
# Baseline: GET /api/users/100 (own account) → 200
# Attack:   GET /api/users/101 (other account) → should be 403

# SSRF — Test internal URLs
# Baseline: GET /api/fetch?url=https://example.com → 200
# Attack:   GET /api/fetch?url=http://169.254.169.254 → should fail

# XSS — Test input reflection
# Baseline: GET /search?q=normal → 200, no script execution
# Attack:   GET /search?q=<script>alert(1)</script> → should be encoded

# SQLi — Test injection
# Baseline: GET /api/users?id=1 → 200
# Attack:   GET /api/users?id=1' OR '1'='1 → different response

# Race — Test concurrent requests
# Baseline: 1x POST /api/coupon/redeem → 1 success
# Attack:    20x parallel → should still be 1 success
```

## Phase 5: Compound-Risk Notes (record, don't chase)

Once a boundary crossing is proven, the review of that finding is **done**.
Write down what it *could* enable, and stop:

```
IDOR read         → note: a write path on the same endpoint would be worse
SSRF              → note: cloud metadata reachable from this egress?
Open redirect     → note: on a trusted origin this aids phishing
XSS               → note: same-origin content could raise the impact
CRLF              → note: cookie injection would change severity
```

Two hard rules:
- **Do not execute the chain** to raise a finding's severity. That is a scope
  decision for the user, not a tactic.
- **Report the demonstrated impact**, not the compounded one. A read-only
  IDOR is not a Critical because account takeover might follow.

## Phase 6: 7-Gate Validation

```
G1: IN SCOPE? → Check program scope definition
G2: BOUNDARY CROSSED? → Authz/authn boundary
G3: ACTOR PERSPECTIVE? → Unauth'd or low-priv
G4: REPRODUCIBLE? → Works every time
G5: IMPACT? → Data access, ATO, RCE, financial — demonstrated, not asserted
G6: NO FALSE POSITIVE? → Not a known acceptable behavior
G7: PROGRAM ACCEPTS? → Not explicitly excluded
FAIL ANY → DO NOT REPORT
```

### Gate 2 — boundary or feature?
Ask whether the resource was *meant* to be reachable by that actor. Public
content served to any authenticated user, documented defaults, and
already-public data are the feature working. This gate rejects more findings
than all the others combined — check it first.

### Gate 6 — Common False Positives (Don't Report)
- CORS `*` without credentialed data access
- Exposed API key without sensitive capability
- GraphQL introspection without auth bypass
- Missing security headers without exploit
- Version disclosure without known CVE
- Clickjacking on non-sensitive pages
- SSL/TLS configuration issues
- Publicly available data shown to a logged-in user

## Phase 7: Report

```
Title: [Vuln Type] in [Endpoint] allows [Impact]

Summary: [1 paragraph — what, where, impact, proof method]

Steps to Reproduce:
1. [Exact steps]
2. [Copy-paste HTTP requests]
3. [Screenshots/videos if needed]

Impact: [who is affected, what data is exposed, CVSS score]

Fix: [1-2 sentences — concrete remediation]
```

A "no vulnerabilities found" report is a valid, valuable deliverable. State
what you covered and what you did not.

## Real Program Workflows

### HackerOne Program
```bash
# API token for submission
H1_TOKEN="your-api-key"
# List programs
curl -s -H "Authorization: Bearer $H1_TOKEN" https://api.hackerone.com/v1/hackers/programs | jq '.data[].attributes.name'
# Submit report
curl -s -X POST https://api.hackerone.com/v1/reports \
  -H "Authorization: Bearer $H1_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"data":{"type":"report","attributes":{"team_handle":"target","title":"...","vulnerability_information":"...","impact":"..."}}}'
```

### Bugcrowd Program
```bash
# Submission via API
curl -s -X POST https://api.bugcrowd.com/submissions \
  -H "Authorization: Token your-token" \
  -H "Content-Type: application/json" \
  -d '{"submission":{"title":"...","description":"...","severity":"high","program":"target"}}'
```

## Time Management Rules
```
5-min rule:  Stuck on technique → try different approach
20-min rule: No progress on target → move to next
1-hr rule:   No findings → switch program
8-hr rule:   Full rotation complete → submit what you have
```

## Tools Quick Reference
```
Recon:      subfinder, dnsx, httpx, katana, nuclei
XSS:        dalfox, xsstrike, kxss
SQLi:       sqlmap, ghauri
SSRF:       interactsh, curl
API:        kiterunner, arjun, ffuf
Secrets:    trufflehog, gitleaks
Takeover:   subzy, dnsreaper
Analysis:   semgrep, mobsf
```
