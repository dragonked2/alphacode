---
name: findings-lifecycle
description: Finding lifecycle management — Track security findings through states: candidate → observed → verified → reportable (or rejected/stale). Use when managing findings, deciding if something is a real vulnerability, or organizing results from security testing.
---

# FINDING LIFECYCLE MANAGEMENT

**Not every idea is a finding. Track the evidence, not the excitement.**

---

## 1. FINDING STATES

```
                    ┌──────────────┐
                    │  CANDIDATE   │  Suspicion, not yet proven
                    │  (weak sig)  │
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            │            ▼
     ┌──────────────┐      │    ┌──────────────┐
     │  OBSERVED    │      │    │   REJECTED   │  Ruled out
     │  (evidence)  │      │    │   (closed)   │
     └──────┬───────┘      │    └──────────────┘
            │              │
            ▼              │
     ┌──────────────┐      │
     │   VERIFIED   │      │
     │  (proof)     │      │
     └──────┬───────┘      │
            │              │
       ┌────┴────┐         │
       │         │         │
       ▼         ▼         │
┌───────────┐ ┌──────────┐ │
│REPORTABLE │ │  STALE   │ │
│ (proof +  │ │ (no long │ │
│  replay)  │ │  trust)  │ │
└───────────┘ └──────────┘ │
```

---

## 2. STATE DEFINITIONS

### Candidate
- **What:** A suspicion or weak signal. Something "feels" off but isn't proven.
- **Evidence:** None or minimal. Maybe a suspicious response, an unusual behavior.
- **Action:** Investigate further. Gather evidence. Don't report yet.
- **Example:** "This endpoint returns 200 without auth, but I'm not sure if it's public by design."

### Observed
- **What:** An evidence-backed signal. You have proof something exists.
- **Evidence:** HTTP requests, responses, screenshots, tool output.
- **Action:** Verify exploitation. Build proof. Check for false positive.
- **Example:** "IDOR confirmed — accessing /api/users/123 with attacker token returns victim's email."

### Verified
- **What:** Promoted with proof semantics. The vulnerability is real and exploitable.
- **Evidence:** Full PoC, reproduction steps, impact demonstrated.
- **Action:** Write report. Build chain if possible. Assign severity.
- **Example:** "IDOR + mass assignment = admin takeover. Full chain documented."

### Reportable
- **What:** Ready for submission. Has evidence + replay/exemption.
- **Evidence:** Complete PoC, reproduction steps, impact, severity, remediation.
- **Action:** Submit to bug bounty program or client.
- **Example:** Report written with severity, CVSS, and fix recommendation.

### Rejected
- **What:** Ruled out. Investigation showed this is not a vulnerability.
- **Evidence:** May have evidence showing it's a false positive.
- **Action:** Document why it was rejected. Don't delete — it prevents re-investigation.
- **Example:** "CORS reflects origin but Access-Control-Allow-Credentials is false → no impact."

### Stale
- **What:** Was once trusted but no longer. Target changed, fix applied, or context changed.
- **Evidence:** Historical evidence may exist but is no longer valid.
- **Action:** Re-investigate if the context changes.
- **Example:** "Found XSS in search page, but target deployed WAF that now blocks the payload."

---

## 3. FINDING TEMPLATE

For every finding, track:

```markdown
## FINDING: [Vuln Type] in [Endpoint]

**ID:** FND-001
**State:** candidate | observed | verified | reportable | rejected | stale
**Severity:** Critical | High | Medium | Low | Info
**CVSS:** 8.5 (CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:N)

### Description
[What is the vulnerability?]

### Evidence
- EVD-001: [HTTP request/response showing the issue]
- EVD-002: [Screenshot or tool output]

### Reproduction Steps
1. Send [specific request]
2. Observe [specific response]
3. [Verify impact]

### Impact
- What can an attacker do? [Specific impact]
- How many users affected? [Scope]
- Business impact? [Data loss, financial, reputation]

### False Positive Check
- [ ] Does it return real data? (not mock/empty)
- [ ] Is it reproducible?
- [ ] Is it intended behavior?
- [ ] Is there a known fix?

### Chain Opportunities
- Can this be chained with FND-XXX? [Description]
- What would the combined impact be? [Impact]

### Remediation
[How to fix this vulnerability]

### References
- CVE: [if applicable]
- OWASP: [WSTG section]
- CVSS Calculator: [link]
```

---

## 4. STATE TRANSITIONS

### Promoting a Finding

```
CANDIDATE → OBSERVED:
  ✓ You have HTTP request/response as evidence
  ✓ The finding is reproducible
  ✓ You can show the specific issue

OBSERVED → VERIFIED:
  ✓ You have a full PoC
  ✓ Impact is demonstrated
  ✓ False positive checks passed
  ✓ You can show exploit steps

VERIFIED → REPORTABLE:
  ✓ Report is written
  ✓ Severity is assigned
  ✓ Remediation is provided
  ✓ All evidence is attached
```

### Rejecting a Finding

```
CANDIDATE → REJECTED:
  - False positive confirmed
  - Intended behavior
  - No impact
  - Cannot reproduce

OBSERVED → REJECTED:
  - Evidence doesn't support the claim
  - Target deployed fix
  - Context changed
```

### Staling a Finding

```
OBSERVED/VERIFIED → STALE:
  - Target applied fix (verify with retest)
  - Technology changed
  - Scope changed
  - Evidence is no longer valid
```

---

## 5. FINDING TRACKER

Maintain a tracker across your operation:

```markdown
## Finding Tracker — example.com Pentest

| ID | Type | Endpoint | State | Severity | Chain |
|----|------|----------|-------|----------|-------|
| FND-001 | IDOR | /api/users/{id} | Verified | High | → ATO |
| FND-002 | SQLi | /search | Observed | Critical | → RCE |
| FND-003 | XSS | /comments | Candidate | Medium | → CSRF |
| FND-004 | Open Redirect | /redirect | Rejected | - | - |
| FND-005 | CORS | /api | Stale | - | - |

### Chain Map
- FND-001 (IDOR) + FND-006 (Mass Assignment) → ATO (Critical)
- FND-002 (SQLi) → Database Dump (Critical)
- FND-003 (XSS) + FND-007 (CSRF) → Session Hijack (High)
```

---

## 6. FALSE POSITIVE RULES

### Always Reject If:

```
- Endpoint returns empty/null data (not real user data)
- Behavior is documented as intended
- No sensitive data is exposed
- Impact cannot be demonstrated
- Finding is "missing header" without exploitation chain
- Finding is "version disclosure" without known CVE
- Finding is "self-XSS" (only you can trigger it)
- Finding is "clickjacking on non-sensitive page"
- Finding is "CORS reflects origin" without credentials
- Finding is "GraphQL introspection works" without data access
```

### Always Verify If:

```
- You can show different user's data (IDOR)
- You can execute code (RCE)
- You can access internal services (SSRF)
- You can bypass authentication (auth bypass)
- You can modify other users' data (mass assignment)
- You can escalate privileges (privilege escalation)
```

---

## 7. QUICK COMMANDS

```bash
# List current findings
/findings list

# Inspect a specific finding
/findings inspect FND-001

# Promote a candidate to verified
/promote FND-001 --evidence EVD-001 --replay REPLAY-001

# Reject a finding
/reject FND-004 --reason "intended behavior, documented in API docs"

# Generate report from findings
/report build
```
