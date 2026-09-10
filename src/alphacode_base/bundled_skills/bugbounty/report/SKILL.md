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
  Gate 4 (Reproducible): ✓ Deterministic reproduction
  Gate 5 (Impact): ✓ Concrete security impact
  Gate 6 (FP): ✓ False positive eliminated
  Gate 7 (Program): ✓ Program accepts this finding

IF ANY GATE FAILED → DO NOT WRITE REPORT
```

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

### HackerOne Severity Guidelines

| Severity | Typical Payout | Examples |
|----------|---------------|----------|
| Critical | $5,000-$50,000+ | RCE, Full ATO, SQLi, SSRF→RCE |
| High | $2,000-$10,000 | Stored XSS, SSRF, IDOR with sensitive data |
| Medium | $500-$2,000 | CSRF, Open Redirect, Limited IDOR |
| Low | $100-$500 | Info disclosure, Limited impact |
| None | $0-100 | Best practice (chained only) |

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
- Higher bounties for critical findings
- Follow their disclosure policy

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
