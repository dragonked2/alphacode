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
- **Example:** "A second owned account receives an email field; its privacy and ownership need controls before calling this IDOR."

### Verified
- **What:** Promoted with proof semantics. The vulnerability is real and exploitable.
- **Evidence:** Full PoC, reproduction steps, impact demonstrated.
- **Action:** Write the report at the severity you demonstrated. Assign severity.
- **Example:** "IDOR confirmed — /api/users/123 with a second user's token returns that user's email. Read-only; no write path tested."

**Verified is where the review of a finding stops.** Do not keep escalating a
proven finding to raise its severity — see the impact-sufficiency rule in the
`bugbounty` skill. If a chain seems relevant, record it as an analyst note.

### Reportable
- **What:** Ready for submission. Has evidence + replay/exemption.
- **Evidence:** Complete PoC, reproduction steps, impact, severity, remediation.
- **Action:** Prepare the report for the user; submit only when explicitly authorized.
- **Example:** Report written with severity, CVSS, and fix recommendation.

### Rejected
- **What:** Ruled out. Investigation showed this is not a vulnerability.
- **Evidence:** May have evidence showing it's a false positive.
- **Action:** Document why it was rejected. Don't delete — it prevents re-investigation.
- **Example:** "The supposedly private fixture is documented and verified as public; the tested read does not violate that boundary."

### Stale
- **What:** Was once trusted but no longer. Target changed, fix applied, or context changed.
- **Evidence:** Historical evidence may exist but is no longer valid.
- **Action:** Re-investigate if the context changes.
- **Example:** "Found XSS in search page, but target deployed WAF that now blocks the payload."

### Damage — the state you hope never to use

- **What:** The assessment itself caused harm. A test artifact was not
  restored, a production object was modified or deleted, or an action
  had an unintended side effect.
- **Evidence:** The before/after state, the restoration attempt and its
  result, and whether the damage is recoverable.
- **Action:** Report it. Immediately. Do not hide it, do not let it
  age into a stale finding, and do not let another finding's severity
  distract from it. The owner needs to know what happened and whether
  they need to restore from backup.
- **Example:** "During write-access testing, sub-category 1 was deleted.
  The create endpoint for sub-categories was not available, so it could
  not be recreated. Category 3 was recreated as id 15. Banner 1 is
  unrecoverable — no create-banner endpoint exists."

**A damage finding is reported alongside, not instead of, the vulnerability
that enabled it.** Both go in the report. The damage does not invalidate
the finding — it is evidence of its severity — but it must be stated
plainly and without softening.

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
- [ ] **Is it intended behavior?** (was this resource meant to be reachable
      by this actor? public content shown to a logged-in user is the feature
      working, not a vulnerability)
- [ ] Is there a known fix?

### Compound Risk (analyst note, not a test)
- Might this compound with another finding? [Note only — do not execute]
- Reported as context for the owner to prioritize, not as demonstrated impact

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
  - False positive confirmed with a discriminating control
  - Intended behavior
  - No impact
  - A discriminating control rules out the specific claim under tested conditions

OBSERVED → REJECTED:
  - Evidence doesn't support the claim
  - A discriminating control shows the observed behavior does not support the claim
```

Failure to reproduce, missing access, tool errors, ambiguous policy, or an
unreliable oracle are inconclusive; record the blocker and retry condition.
A verified target change makes older evidence stale rather than retroactively
proving the historical observation false. Rejection applies only to the
specific tested claim, not every possible impact or related route.

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
## Finding Tracker — example.com assessment

| ID | Type | Endpoint | State | Severity | Note |
|----|------|----------|-------|----------|------|
| FND-001 | IDOR | /api/users/{id} | Verified | High | read-only, not escalated |
| FND-002 | SQLi | /search | Observed | Critical | |
| FND-003 | XSS | /comments | Candidate | Medium | |
| FND-004 | Open Redirect | /redirect | Rejected | - | warning interstitial = designed behavior |
| FND-005 | CORS | /api | Stale | - | |

### Coverage Notes
- Not tested: admin panel (out of scope, no test account)
- Rejected with evidence: FND-004 (interstitial is the intended control)
```

Severity here is the **demonstrated** severity. Do not raise a row's severity
to reflect a chain you did not execute.

---

## 6. FALSE POSITIVE RULES

### Signals that need context, not automatic rejection:

```
- Empty data, public behavior, missing headers, version disclosure and
  introspection alone do not establish impact; check the actual invariant.
- A missing credential or a failed test may prevent verification; it does not
  prove no impact exists. Record the limitation and avoid unsupported claims.
- Origin reflection without credentials does not prove credentialed browser
  data exposure; assess the actual data, browser behavior and trust boundary.
- Documented behavior and program exclusions affect eligibility, but distinguish
  those from the technical question of whether an unintended boundary failed.
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
