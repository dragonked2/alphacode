---
name: report
description: Bug bounty report writing — Professional report templates, severity mapping, VRT alignment. Use when writing reports, when user mentions reporting, or when preparing submissions. Includes H1, Bugcrowd, Intigriti, and Immunefi formats.
---

# 🎯 Report Writing Skill

Elite-level bug bounty report writing and submission preparation.

## Report Checklist

### Before Writing
- [ ] Vulnerability validated and reproducible
- [ ] Impact clearly demonstrated
- [ ] Proof of concept prepared
- [ ] Evidence cleaned (no PII)
- [ ] Severity assessed

### Report Structure
- [ ] Clear, descriptive title
- [ ] Executive summary
- [ ] Technical details
- [ ] Steps to reproduce
- [ ] Proof of concept
- [ ] Impact analysis
- [ ] Remediation recommendations

### After Writing
- [ ] Readability check
- [ ] Formatting consistency
- [ ] Evidence attached
- [ ] No sensitive data exposed
- [ ] Submission ready

## Report Templates

### HackerOne Report
```markdown
# Summary

Vulnerability type: [Vulnern Type]
Vulnerability severity: [Critical/High/Medium/Low/Info]
Weakness: [CWE-XXX]

# Vulnerability Detail

[Detailed technical explanation]

# Steps To Reproduce

1. Navigate to [URL]
2. [Step 2]
3. [Step 3]
4. Observe [result]

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

### Intigriti Report
```markdown
# Summary

Vulnerability Type: [Type]
Affected Component: [URL/Endpoint]
Severity: [Critical/High/Medium/Low/Info]

## Description
[Technical details]

## Proof of Concept
[Steps and evidence]

## Impact
[Business impact]

## Remediation
[Fix recommendations]
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

## Tool Used
[Tools used for discovery]
```

## Severity Mapping

### CVSS Scoring
| Severity | Score | Typical Findings |
|----------|-------|------------------|
| Critical | 9.0-10.0 | RCE, SQLi with data exfil, Auth bypass |
| High | 7.0-8.9 | SSRF, Stored XSS, IDOR with PII |
| Medium | 4.0-6.9 | CSRF, Open Redirect, Info Disclosure |
| Low | 0.1-3.9 | Missing headers, Version disclosure |
| Info | 0.0 | Best practice violations |

### HackerOne Severity Guidelines
| Severity | Typical Payout | Examples |
|----------|---------------|----------|
| Critical | $5,000-$50,000+ | RCE, Full account takeover, SQLi |
| High | $2,000-$10,000 | Stored XSS, SSRF, IDOR with sensitive data |
| Medium | $500-$2,000 | CSRF, Open Redirect, Limited IDOR |
| Low | $100-$500 | Info disclosure, Missing headers |
| None | $0-100 | Best practice, Documentation |

## Writing Best Practices

### Title
- Be specific: "SQL Injection in /api/users allows data exfiltration"
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

## Common Mistakes to Avoid

1. **Vague titles** — "Security Issue" vs "SQL Injection in /api/login"
2. **Missing impact** — Technical details without business context
3. **Unreproducible steps** — Steps that don't work for triagers
4. **Sensitive data** — Including real PII in reports
5. **Poor formatting** — Walls of text without structure
6. **Duplicate submissions** — Not checking Hacktivity first
7. **Out of scope** — Testing excluded assets

## Evidence Cleaning
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

## Submission Platforms

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
```

## Report Review Checklist

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
