---
name: report
description: Bug bounty report writing with mandatory 7-gate validation. Professional report templates, severity mapping, VRT alignment. Every report must reference which gates the finding passed. Use when writing reports or preparing submissions.
---

# REPORT WRITING — GATE-VALIDATED REPORTS

Every report must reference the 7-gate validation that the finding passed.

---

## PRE-SUBMISSION CHECKLIST

Before writing ANY report, verify the finding state:

```
FINDING STATE: VALIDATED (passed all 7 gates)
  Gate 1 (Scope): ✓ Asset in program scope
  Gate 2 (Boundary): ✓ Security boundary violation identified
  Gate 3 (Attacker): ✓ Realistic attacker capability
  Gate 4 (Reproducible): ✓ Repeatable evidence; measured conditions for timing-dependent issues
  Gate 5 (Impact): ✓ Concrete security impact
  Gate 6 (FP): ✓ Relevant benign explanations tested; remaining uncertainty disclosed
  Gate 7 (Program): ✓ Checked against available program eligibility rules

IF ANY GATE FAILED → DO NOT WRITE REPORT
```

These gates govern submission-ready vulnerability claims. A failed or unknown
gate still belongs in internal candidate notes or the assessment's limitations;
do not hide unresolved work or claim that the program has accepted a report.
For intermittent behavior, include controlled trials, successes/attempts, timing,
settled state and negative controls rather than inventing deterministic results.

Attach a compact evidence index: finding ID -> original artifact -> actor/tenant
and fixture -> baseline/treatment -> observed invariant violation. Redact secrets
from the report while preserving reproducible request structure. Separate observed
impact from conditional chain extensions. Group variants by demonstrated root
cause, and keep independent authorization failures separate when the fix differs.

Close the assessment with completed coverage under exact tested conditions,
failed/skipped/inconclusive work and its retry prerequisites, cleanup status and
remaining high-value questions. Tool completion and request volume are not proof
of exhaustive coverage. Never infer severity solely from the vulnerability label
in a template table below; explain attacker prerequisites and demonstrated effect.

---

## REPORT TEMPLATES

### HackerOne Report

```markdown
# Summary

Vulnerability type: [Vulnerability Type]
Vulnerability severity: [Critical/High/Medium/Low/Info]
Weakness: [CWE-XXX]

# Vulnerability Detail

[Detailed technical explanation]

# Steps To Reproduce

1. [Exact HTTP request — copy-paste ready]
2. [Exact response showing impact]
3. [Screenshot/video of impact]

# Proof of Concept

[Request/Response pairs, screenshots, code]

# Impact

[Business impact, affected users, data exposure]

# Remediation

[Specific fix recommendations]

# Supported Scenario

[When this vulnerability can be exploited]

# Out Of Scope

[Any limitations or edge cases]
```

### Bugcrowd Report (VRT-aligned)

```markdown
# Vulnerability Name

**VRT Category**: [Category from VRT]
**VRT Subcategory**: [Subcategory]
**Severity**: [P1-P5]

## Description
[Technical description]

## Impact
[Business impact]

## Steps to Reproduce
1. [Step 1]
2. [Step 2]
3. [Step 3]

## Proof of Concept
[Evidence]

## Remediation
[Fix recommendation]
```

### Immunefi Report (Web3)

```markdown
# Summary

Vulnerability Type: [Type]
Affected Protocol: [Name]
Chain(s): [Ethereum, BSC, etc.]
Severity: [Critical/High/Medium/Low/Info]

## Vulnerability Description
[Technical details]

## Impact
[Financial impact, affected users]

## Proof of Concept
[Steps and evidence]

## Remediation
[Fix recommendations]
```

---

## SEVERITY MAPPING

### CVSS Scoring

| Severity | Score | Typical Findings |
|----------|-------|------------------|
| Critical | 9.0-10.0 | RCE, SQLi with data exfil, Auth bypass, ATO |
| High | 7.0-8.9 | SSRF, Stored XSS, IDOR with PII, Privilege escalation |
| Medium | 4.0-6.9 | CSRF, Open Redirect, Limited IDOR, Race condition |
| Low | 0.1-3.9 | Info disclosure, Limited impact findings |
| Info | 0.0 | Best practice violations (only if chained) |

### Severity Justification

Pick severity from the **impact you demonstrated**, and show the reasoning so
the owner (or triager) can disagree with evidence rather than opinion:

| Demonstrated impact | Typical severity |
|---------------------|------------------|
| RCE, or auth bypass taking over arbitrary accounts | Critical |
| Read/write access to other users' sensitive data, privilege escalation | High |
| CSRF on a meaningful action, open redirect on a trusted origin, limited cross-tenant read | Medium |
| Minor information exposure with no sensitive data | Low |
| Hardening gaps with no demonstrated impact | Info (usually not worth a report on its own) |

Two rules that keep reports honest:
- **Do not inflate.** If you proved a read-only IDOR, it is not Critical just
  because write access might exist. State the demonstrated severity.
- **Do not under-report by timidity.** A proven high-impact bug reported as
  "medium, probably" is as wrong as inflation — say what you showed.
- **Chains are notes, not severity.** "This may compound with an XSS on the
  same origin" belongs in an analyst note, not in the impact score.

---

## SEVERITY FILTERING

When the user scopes the report ("Critical only", "High and above"):

1. Parse the filter at assessment start and record it in the scope
   file (see scope skill) — e.g. `Severity focus: Critical only`.
2. Tag EVERY finding with a severity DURING testing, not at report time.
3. At report time, include only matching findings. State explicitly
   what was tested but filtered out:
   `Tested: XSS, open redirect, headers (below Critical threshold — excluded).`
4. Critical means: RCE, SQLi with data exfiltration, SSRF with
   internal-network access, auth bypass / ATO. Never inflate a
   Medium to fill a Critical-only report — report "no Critical
   findings" with the no-finding checklist (see runbook 10.9) instead.

---

## EVIDENCE STANDARD

Every finding ships with request + response + status code + impact
analysis (full templates live in the evidence-locker skill):

- Auth findings: demonstrated with real authenticated requests —
  an unauthenticated CORS/CSRF observation without authed impact
  is not a finding.
- RCE: full command-execution output.
- Data exposure: the exposed data and why it is sensitive.
- Link each finding to its evidence IDs (EVD-001…); no evidence,
  no report.

---

## WRITING BEST PRACTICES

### Title
- Be specific: "IDOR in /api/users/{id} allows reading any user's PII"
- Not generic: "Security Vulnerability Found"

### Summary
- One paragraph maximum
- What, where, impact
- No technical jargon

### Steps to Reproduce
- Numbered list
- Exact URLs and parameters
- Include request/response pairs
- Reproducible by anyone

### Impact
- Business impact, not just technical
- Quantify if possible (users affected, data exposed)
- Real-world scenario

### Remediation
- Specific, actionable recommendations
- Not just "fix the vulnerability"
- Include code examples if helpful

---

## COMMON MISTAKES TO AVOID

1. **Vague titles** — "Security Issue" vs "IDOR in /api/users/{id}"
2. **Missing impact** — Technical details without business context
3. **Unreproducible steps** — Steps that don't work for triagers
4. **Sensitive data** — Including real PII in reports
5. **Poor formatting** — Walls of text without structure
6. **Duplicate submissions** — Not checking Hacktivity first
7. **Out of scope** — Testing excluded assets
8. **Theoretical impact** — "Could potentially" instead of "did"

---

## EVIDENCE CLEANING

```bash
# Remove PII from screenshots
# Blur names, emails, phone numbers
# Use placeholder data

# Clean request/response pairs
# Remove session tokens
# Remove real credentials
# Use [REDACTED] for sensitive values

# Sanitize logs
# Remove IP addresses
# Remove usernames
# Use generic placeholders
```

---

## SUBMISSION PLATFORMS

### HackerOne
- https://hackerone.com
- Follow program rules strictly
- Use their report template
- Check Hacktivity for duplicates

### Bugcrowd
- https://bugcrowd.com
- Align with VRT (Vulnerability Rating Taxonomy)
- Use their severity definitions
- Follow disclosure policy

### Intigriti
- https://intigriti.com
- European bug bounty platform
- Follow their submission guidelines
- Check for duplicate reports

### Immunefi
- https://immunefi.com
- Web3/DeFi focused
- Follow their disclosure policy
- Report demonstrated impact; do not inflate a theoretical drain into a Critical

---

## REPORT REVIEW CHECKLIST

Before submitting:
- [ ] Title is specific and descriptive
- [ ] Summary is clear and concise
- [ ] Steps are reproducible
- [ ] Evidence is clean (no PII)
- [ ] Impact is quantified
- [ ] Remediation is actionable
- [ ] Formatting is consistent
- [ ] No sensitive data exposed
- [ ] Duplicate check completed
- [ ] Program rules followed
- [ ] All 7 gates passed (documented in finding state)
- [ ] Severity matches demonstrated impact
