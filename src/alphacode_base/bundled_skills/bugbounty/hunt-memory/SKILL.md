---
name: hunt-memory
description: Cross-session hunt memory and chain builder with state machine — session resume, lead board, pattern database, finding deduplication, bug chain discovery, and finding lifecycle management. Use when resuming a previous hunt session, tracking findings across sessions, or managing hunt state.
---

# HUNT MEMORY & CHAIN BUILDER — STATE MACHINE INTEGRATION

Cross-session persistence with mandatory state tracking. Every finding follows the state machine. No shortcuts.

---

## FINDING STATE MACHINE

Every finding MUST follow these states. No skipping.

```
DISCOVERED
    ↓
IN SCOPE? ──────────────────── NO → REJECT
    ↓ YES
SECURITY BOUNDARY VIOLATION? ─ NO → REJECT
    ↓ YES
REALISTIC ATTACKER? ────────── NO → REJECT/DOWNGRADE
    ↓ YES
REPRODUCIBLE? ──────────────── NO → HYPOTHESIS ONLY (park it)
    ↓ YES
CONCRETE IMPACT? ───────────── NO → REJECT
    ↓ YES
FALSE-POSITIVE CHECK PASSED? ─ NO → REJECT
    ↓ YES
PROGRAM ACCEPTS IT? ────────── NO → OUT OF SCOPE
    ↓ YES
VALIDATED FINDING
    ↓
SEVERITY ASSESSMENT
    ↓
MINIMAL SAFE PoC
    ↓
REPORT
```

### State Definitions

| State | Meaning | Action |
|-------|---------|--------|
| DISCOVERED | Candidate identified | Run Gate 1 |
| IN SCOPE | Asset in program scope | Run Gate 2 |
| BOUNDARY VIOLATED | Security boundary identified | Run Gate 3 |
| REALISTIC ATTACKER | Attacker capability confirmed | Run Gate 4 |
| REPRODUCIBLE | Deterministic reproduction | Run Gate 5 |
| IMPACTED | Concrete impact shown | Run Gate 6 |
| FP_CHECKED | False positive eliminated | Run Gate 7 |
| ACCEPTED | Program accepts | Severity → PoC → Report |
| REJECTED | Failed a gate | Document why, move on |
| HYPOTHESIS_ONLY | Cannot reproduce yet | Park, return later |
| OUT_OF_SCOPE | Valid but excluded | Do not submit |

---

## SESSION RESUME

### Quick Resume

```
Target: target.com
Last session: 2026-09-01
Status: Phase 3 (Hunt) — IDOR testing in progress

Completed:
✓ Phase 1: Recon — 47 subdomains found, 12 live
✓ Phase 2: Mapping — 156 endpoints cataloged, auth = JWT
○ Phase 3: Hunt — Testing IDOR on /api/v2/users/{id}

Finding States:
  F001: DISCOVERED → IN SCOPE → BOUNDARY VIOLATED → REPRODUCIBLE → IMPACTED
        Status: AWAITING FP_CHECK
        Endpoint: GET /api/v2/users/{id}
        Evidence: User A can read User B's data

  F002: DISCOVERED → IN SCOPE → REJECTED (not reproducible)
        Status: HYPOTHESIS ONLY
        Endpoint: POST /api/v2/users/{id}/transfer
        Notes: Race condition possible but couldn't reproduce

Next steps:
→ Complete FP_CHECK for F001
→ Finish IDOR sweep on /api/v2/users/{id}
→ Test /api/v2/orders/{id} (sibling endpoint)
→ Test /api/v1/ (old version pattern)
```

### Session State Tracking

| Field | Description |
|-------|-------------|
| Target | Primary target domain |
| Phase | Current workflow phase (1-5) |
| Mode | Wide or Deep route |
| Focus | Primary vuln class being tested |
| Auth status | Anonymous or authenticated (which session) |
| Findings | List with state machine status |
| Dead ends | What didn't work (don't retry) |
| Next actions | Prioritized list of what to try next |
| Time invested | Total hours on this target |

---

## LEAD BOARD

### Lead Lifecycle

```
NEW → QUEUED → ACTIVE → REPORTING → REPORTED
                    ↓
                  KILLED (dead end, blocked, N/A)
```

### Lead Priority Scoring

| Signal | Priority | Reason |
|--------|----------|--------|
| Auth-required endpoint (IDOR/BOLA potential) | HIGH | Authorization boundary |
| GraphQL endpoint | HIGH | Rich attack surface |
| Admin/debug endpoint | HIGH | Privilege escalation |
| New feature (< 30 days old) | HIGH | Unreviewed code |
| Payment/billing endpoint | HIGH | Financial impact |
| Webhook endpoint | HIGH | Server-side request |
| File upload endpoint | HIGH | RCE/XSS/path traversal |
| Old API version (/v1/) | HIGH | May lack controls |
| Complex business logic | MEDIUM | Logic flaws |
| Standard CRUD endpoints | MEDIUM | IDOR candidates |
| Static assets / CDN | LOW | Low value |
| 403 on all paths | KILLED | WAF blocked |

---

## PATTERN DATABASE

### Cross-Target Patterns

```
PATTERN: Old API version lacks auth
- Target A: /v1/users had no auth, /v2/users did → IDOR
- Target B: /v1/orders had no auth, /v2/orders did → IDOR
→ NEXT: Always check old API versions for auth gaps

PATTERN: GraphQL batched queries bypass rate limits
- Target A: 1000 login attempts in one batch → OTP bypass
→ NEXT: Check if GraphQL has rate limiting separate from REST

PATTERN: JS bundle contains hardcoded API keys
- Target A: OAuth client_secret in webpack bundle
→ NEXT: Always download and analyze JS bundles for secrets
```

---

## CHAIN BUILDER

### Known A→B→C Chains

```
BUG A (Signal)          →  HUNT FOR BUG B              →  ESCALATE TO C
══════════════════════════════════════════════════════════════════════════════
IDOR (read)             →  PUT/DELETE on same endpoint  →  Full account manipulation
SSRF (any)              →  Cloud metadata access         →  IAM credential exfil → RCE
XSS (stored)            →  HttpOnly check on session     →  Session hijack → ATO
Open redirect           →  OAuth redirect_uri accepts    →  Auth code theft → ATO
S3 bucket listing       →  JS bundle enumeration         →  OAuth client_secret → chain
Rate limit bypass       →  OTP brute force               →  Account takeover
GraphQL introspection   →  Missing field-level auth      →  Mass PII exfil
Debug endpoint          →  Leaked env variables           →  Cloud credential → infra access
CORS reflects origin    →  Test with credentials         →  Credentialed data theft
Host header injection   →  Password reset poisoning      →  ATO via reset link
```

### Chain Validation Checklist

For each chain, verify:
- [ ] Bug A is confirmed (real HTTP request, real response)
- [ ] Bug B is reachable from Bug A (same session, same access level)
- [ ] Bug C is achievable from Bug B (realistic attack scenario)
- [ ] The chain works end-to-end (not just individual pieces)
- [ ] Impact is quantified ("affects N users", "exposes $X value")
- [ ] Each bug can be a SEPARATE report (more bounties)

---

## DEDUPLICATION

Before writing any report, check for duplicates:

1. **Search Hacktivity**: Ctrl+F on program name + endpoint + bug class
2. **Search GitHub issues**: `is:issue label:security ENDPOINT_NAME`
3. **Check changelog**: Does it mention this behavior?
4. **Check recent disclosures**: Last 5 reports for this program
5. **Google it**: "TARGET_NAME ENDPOINT_NAME bug bounty"

**If duplicate found**: Don't report. Add to dead ends list. Move on.

---

## FINDING TEMPLATES

### IDOR Template

```
FINDING: IDOR on [METHOD] /api/[resource]/{id}
State: VALIDATED
Gate 1 (Scope): [asset] is in scope ✓
Gate 2 (Boundary): User A → User B's resource ✓
Gate 3 (Attacker): Authenticated normal user ✓
Gate 4 (Reproducible): [exact request/response] ✓
Gate 5 (Impact): Confidentiality — [data type] exposed ✓
Gate 6 (FP): Response contains different user's data ✓
Gate 7 (Program): Bug class in scope, meets severity ✓
Evidence: [request/response pair]
Severity: [score]
```

### SSRF Template

```
FINDING: SSRF on [METHOD] /api/[feature]
State: VALIDATED
Gate 1 (Scope): [asset] is in scope ✓
Gate 2 (Boundary): Internet → internal service ✓
Gate 3 (Attacker): [position] ✓
Gate 4 (Reproducible): [exact request/response] ✓
Gate 5 (Impact): [cloud metadata / internal access / RCE] ✓
Gate 6 (FP): Internal resource content in response ✓
Gate 7 (Program): SSRF in scope ✓
Evidence: [request/response pair]
Severity: [score]
```
