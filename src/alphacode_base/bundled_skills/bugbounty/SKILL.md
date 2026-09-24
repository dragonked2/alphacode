---
name: bugbounty
description: "Elite bug bounty hunting — differential testing, 7-gate validation, hypothesis-driven. Optimized for validated findings/hour. When user mentions bug bounty, hacking, pentesting, security testing, vuln research, recon, exploitation, or offensive security."
---

# BUG BOUNTY HUNTER — VALIDATED FINDINGS PER HOUR

## SPEED RULES
- **3 bullets max per answer. No explanation.**
- 5-min rule: stuck → move on
- 20-min rotation: no progress → next target
- 1-hour rule: no findings → switch target
- Generate hypotheses BEFORE payloads
- Chain low bugs → high payout ($500 → $50K)

## WORKFLOW: SCOPE → RECON → MAP → HUNT → VALIDATE → REPORT

### PHASE -1: SCOPE (first, always — see scope skill)

Write the scope file (IN-SCOPE / OUT-OF-SCOPE / severity focus).
Check every new host against it. No scope file → no testing.

### PHASE 0: RECON (5 min — see recon, recon-js, tool-doctor)

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
FAIL any gate → REJECT. No exceptions.
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

Track hypotheses with kill rules and time budgets (see runbook 10.4):
3 identical results (404/401/sanitized) → KILL. Killed stays dead.

## OPERATING RULES (see runbook section 10)

Priority order: auth/authZ → backend-reaching input → business logic →
info disclosure → UI-only last. Checkpoint notes every ~5 min.
Differential testing (authed vs unauthed) for everything auth-adjacent.
Chain each finding ("what does this enable?", ≤2 steps). Filter at
fetch time (`head`, `--max-time`, status-only flags). No-finding
checklist before declaring clean.

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
