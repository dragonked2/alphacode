---
name: dependency-audit
description: Audit project dependencies for known vulnerabilities, outdated packages, license compliance, and supply chain risks.
---

# Dependency Audit Skill

Audit dependencies for security, freshness, and compliance.

## Process

1. **Inventory** — list all direct and transitive dependencies
2. **Vulnerabilities** — check against known CVE databases (NVD, GitHub Advisory)
3. **Updates** — identify outdated packages with available security patches
4. **Licenses** — flag copyleft or restrictive licenses in commercial contexts
5. **Supply chain** — verify package integrity (lockfiles, signatures, maintainer trust)
6. **Report** — prioritized list with remediation steps

## Red Flags

- Dependencies with open CVEs at Critical/High severity
- Packages not updated in 2+ years
- Single maintainer on critical-path packages
- Typosquatting candidates (similar names to popular packages)
- Bundled dependencies with different licenses
- Post-install scripts in package.json
- Packages with excessive permissions (broad filesystem/network access)
