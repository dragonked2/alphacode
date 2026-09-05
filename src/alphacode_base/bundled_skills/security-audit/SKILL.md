---
name: security-audit
description: Comprehensive code security audit: OWASP Top 10, CWE mapping, SAST/DAST methodology, and structured findings with CVSS scoring.
---

# Security Audit Skill

Systematic security code review methodology.

## Process

1. **Scope** — identify entry points, trust boundaries, and data flows
2. **Map** — trace user input from source to sink
3. **Classify** — map each finding to CWE and OWASP Top 10 categories
4. **Score** — assign CVSS v3.1 severity (Critical/High/Medium/Low/Info)
5. **Exploit** — write a concrete proof-of-concept for each finding
6. **Report** — structured findings with severity, location, evidence, and remediation

## OWASP Top 10 Coverage

- A01: Broken Access Control
- A02: Cryptographic Failures
- A03: Injection (SQL, NoSQL, OS, LDAP)
- A04: Insecure Design
- A05: Security Misconfiguration
- A06: Vulnerable Components
- A07: Authentication Failures
- A08: Data Integrity Failures
- A09: Logging & Monitoring Failures
- A10: Server-Side Request Forgery

## Output Format

For each finding provide:
- **Title**: short descriptive name
- **Severity**: Critical / High / Medium / Low / Info
- **CWE**: CWE-ID and name
- **Location**: file:line
- **Evidence**: code snippet showing the vulnerability
- **Impact**: what an attacker can achieve
- **Remediation**: specific code fix
