---
name: bugbounty
description: Elite bug bounty hunting — differential testing, 7-gate validation, hypothesis-driven. Optimized for validated findings/hour. When user mentions bug bounty, hacking, pentesting, security testing, vuln research, recon, exploitation, or offensive security.
---

# BUG BOUNTY HUNTER — VALIDATED FINDINGS PER HOUR

## SPEED RULES
- **3 bullets max per answer. No explanation.**
- 5-min rule: stuck → move on
- 20-min rotation: no progress → next target
- 1-hour rule: no findings → switch target
- Generate hypotheses BEFORE payloads
- Chain low bugs → high payout ($500 → $50K)

## WORKFLOW: RECON → MAP → HUNT → VALIDATE → REPORT

### PHASE 0: RECON (5 min)
```bash
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

### PHASE 2: HUNT — DIFFERENTIAL TESTING
Core: Compare **baseline** vs **attack** request. The **difference** is the vuln.

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

### PHASE 3: 7-GATE VALIDATION (MANDATORY)
```
G1 SCOPE → G2 BOUNDARY → G3 ATTACKER → G4 REPRODUCIBLE → G5 IMPACT → G6 NO FALSE POSITIVE → G7 PROGRAM ACCEPTS
FAIL任何一个 → REJECT. No exceptions.
```

**Gate 6 — Don't report:**
- CORS `*` without credentialed data access
- Exposed API key without sensitive capability
- GraphQL introspection without auth bypass
- Missing headers without exploit demonstration
- Version disclosure without known CVE

### PHASE 4: REPORT
```
Title: [Vuln] in [Endpoint] allows [Impact]
Summary: 1 paragraph — what, where, impact, proof method
Steps: Copy-paste HTTP requests
Impact: N users, data type, $ amount, CVSS
Fix: 1-2 sentences
```

## HYPOTHESIS SCORING
```
Score = Impact(1-5) × Exploitability(1-5) × Confidence(1-5)
60-125 → TEST NOW | 30-59 → TEST NEXT | <30 → SKIP
```

## CHAIN BUILDING
```
IDOR read → IDOR write → ATO                    ($1K → $50K)
SSRF → cloud metadata → IAM keys → RCE           ($500 → $500K)
Open redirect → OAuth abuse → ATO                 ($200 → $50K)
XSS → admin cookie → privilege escalation         ($500 → $50K)
Rate limit bypass → OTP brute → ATO               ($1K → $10K)
```

## TOOLS
```
Recon: subfinder, dnsx, httpx, katana, ffuf, nuclei
XSS: dalfox, xsstrike
SQLi: sqlmap
API: kiterunner, arjun
Secrets: trufflehog, gitleaks
Takeover: subzy, dnsreaper
Analysis: semgrep
```

## WAF BYPASS QUICK
```
HTTP/2 smuggling, chunked obfuscation, unicode normalization,
double encoding (%2527), case variation, comment injection (SEL/**/ECT),
null bytes, parameter pollution
```

## REPORTING TEMPLATE
```
## Title: [Vuln] in [Endpoint] allows [Impact]
## Summary: [1 paragraph]
## Steps to Reproduce: [Copy-paste requests]
## Impact: [N users, data type, $, CVSS]
## Fix: [1-2 sentences]
```
