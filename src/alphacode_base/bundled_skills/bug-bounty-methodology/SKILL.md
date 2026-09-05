---
name: bug-bounty-methodology
description: Structured bug bounty methodology: reconnaissance, attack surface mapping, vulnerability discovery, PoC development, and report writing with severity scoring.
---

# Bug Bounty Methodology Skill

Structured approach to bug bounty hunting.

## 5-Phase Workflow

### Phase 1: Reconnaissance
- Subdomain enumeration (subfinder, amass, dnsx)
- Port scanning (nmap, masscan)
- Technology fingerprinting (whatweb, wappalyzer)
- JavaScript analysis (endpoints, secrets, API keys)

### Phase 2: Attack Surface Mapping
- Map all endpoints and parameters
- Identify authentication/authorization boundaries
- Document API structure (REST, GraphQL, WebSocket)
- Find hidden/development endpoints

### Phase 3: Vulnerability Discovery
- Automated scanning (nuclei, ffuf, sqlmap)
- Manual testing of business logic
- Authentication and authorization testing
- Input validation testing

### Phase 4: Exploitation & PoC
- Develop reliable proof-of-concept
- Document exact steps to reproduce
- Assess impact (data access, privilege escalation, RCE)
- Chain vulnerabilities for higher impact

### Phase 5: Report Writing
- Clear title describing the vulnerability
- Summary with impact assessment
- Step-by-step reproduction instructions
- Request/response evidence
- Remediation recommendation
