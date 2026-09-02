---
name: hunt-memory
description: Cross-session hunt memory and chain builder — session resume, lead board, pattern database, finding deduplication, and bug chain discovery. Use when resuming a previous hunt session, tracking findings across sessions, building bug chains from A→B→C signals, or managing hunt state across multiple targets.
---

# HUNT MEMORY & CHAIN BUILDER

Cross-session persistence for bug bounty hunters. Remember everything: patterns found on one target inform the next, and sessions pick up where they left off.

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
  - GET works (read PII) — confirmed for IDs 123-130
  - PUT not tested yet
  - DELETE not tested yet
  - GraphQL node() not tested yet

Next steps:
→ Finish IDOR sweep on /api/v2/users/{id}
→ Test /api/v2/orders/{id} (sibling endpoint)
→ Test /api/v2/invoices/{id} (sibling endpoint)
→ Check if v1 API lacks auth (old version pattern)
```

### Session State Tracking

| Field | Description |
|-------|-------------|
| Target | Primary target domain |
| Phase | Current workflow phase (1-5) |
| Mode | Wide or Deep route |
| Focus | Primary vuln class being tested |
| Auth status | Anonymous or authenticated (which session) |
| Findings | List of confirmed/possible findings |
| Dead ends | What didn't work (don't retry) |
| Next actions | Prioritized list of what to try next |
| Time invested | Total hours on this target |

---

## LEAD BOARD

### How Leads Work

After recon, every signal becomes a lead on the board. Each lead has a priority, status, and routing.

```
LEAD BOARD — target.com
═══════════════════════════════════════════════════
ID  │ Priority │ Status    │ Signal              │ Route
══════╪════════╪═══════════╪═════════════════════╪═════════════════
L001 │ HIGH     │ ACTIVE    │ GraphQL endpoint    │ hunt-graphql
L002 │ HIGH     │ QUEUED    │ /admin/debug        │ advanced-techniques
L003 │ MEDIUM   │ QUEUED    │ S3 bucket listing   │ web3-audit (chain)
L004 │ LOW      │ DEFERRED  │ Staging subdomain   │ recon (deep)
L005 │ MEDIUM   │ KILLED    │ 403 on all paths    │ WAF blocked
═══════════════════════════════════════════════════
Active: 1 │ Queued: 2 │ Deferred: 1 │ Killed: 1
```

### Lead Lifecycle

```
NEW → QUEUED → ACTIVE → REPORTING → REPORTED
                    ↓
                  KILLED (dead end, blocked, N/A)
```

### Lead Priority Scoring

| Signal | Priority |
|--------|----------|
| Auth-required endpoint (IDOR/BOLA potential) | HIGH |
| GraphQL endpoint | HIGH |
| Admin/debug endpoint | HIGH |
| New feature (< 30 days old) | HIGH |
| Complex business logic (payment, coupon) | MEDIUM |
| Standard CRUD endpoints | MEDIUM |
| Static assets / CDN | LOW |
| 403 on all paths | KILLED |

---

## PATTERN DATABASE

### Cross-Target Patterns

Track patterns that work across targets:

```
PATTERN: Old API version lacks auth
- Target A: /v1/users had no auth, /v2/users did → IDOR
- Target B: /v1/orders had no auth, /v2/orders did → IDOR
- Target C: /api/v1/ had no auth → confirmed
→ NEXT: Always check old API versions for auth gaps

PATTERN: GraphQL batched queries bypass rate limits
- Target A: 1000 login attempts in one batch → OTP bypass
- Target B: Rate limit on /login but not on GraphQL mutation
→ NEXT: Check if GraphQL has rate limiting separate from REST

PATTERN: JS bundle contains hardcoded API keys
- Target A: OAuth client_secret in webpack bundle
- Target B: Stripe publishable key + hidden test key in bundle
→ NEXT: Always download and analyze JS bundles for secrets
```

### Finding Templates

Reusable templates for common findings:

**IDOR Template:**
```
Endpoint: [METHOD] /api/[resource]/{id}
Auth required: Yes (any user)
Impact: Read/modify other user's [resource type]
Affected users: All users with [resource type]
Severity: [根据 impact]
Evidence: [request/response pair]
```

**SSRF Template:**
```
Endpoint: [METHOD] /api/[feature]
Parameter: [param_name]
Impact: Internal network access, cloud metadata
Severity: [根据 cloud/internal]
Evidence: [request showing internal response]
```

---

## CHAIN BUILDER

### Known A→B→C Chains

When you find bug A, systematically check for B and C:

```
BUG A (Signal)          →  HUNT FOR BUG B              →  ESCALATE TO C
═══════════════════════════════════════════════════════════════════════════════
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

### Chain Report Strategy

```
Single bugs: 1 report per bug
Chains: Separate reports for standalone + chain

Example:
Report 1: "IDOR in /api/users/{id} allows reading any user's PII" (Medium)
Report 2: "IDOR chain: S3 bucket → JS bundle → OAuth secret → full account takeover" (Critical)

This gets you TWO bounties instead of one.
```

---

## DEDUPLICATION

Before writing any report, check for duplicates:

1. **Search Hacktivity**: Ctrl+F on program name + endpoint + bug class
2. **Search GitHub issues**: `is:issue label:security ENDPOINT_NAME`
3. **Check changelog**: Does it mention this behavior?
4. **Check recent disclosures**: Last 5 reports for this program
5. **Google it**: "TARGET_NAME ENDPOINT_NAME bug bounty"

**If duplicate found**: Don't report. Add to dead ends list. Move on.
