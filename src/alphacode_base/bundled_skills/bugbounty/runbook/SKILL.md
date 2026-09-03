---
name: runbook
description: Structured security runbooks — Predefined workflows for common security tasks. Use when following a structured security workflow, triaging a web application, mapping attack surface, or performing scoped pentest work. Keeps the agent moving through recognizable task shapes instead of random tool calls.
---

# SECURITY RUNBOOKS

**Structured workflows that keep you moving through real tasks, not random tool calls.**

---

## 1. AVAILABLE RUNBOOKS

```
RUNBOOK                          PURPOSE                        TARGET TYPE
═══════════════════════════════════════════════════════════════════════════
appsec-web-triage                AppSec web app triage           URL
web-surface                      Web attack surface mapping      URL
network-surface                  Network attack surface          IP/CIDR
osint-target                     OSINT target research           Domain
pentest-starter                  Full pentest workflow           URL/IP/Domain
api-security-audit               API security audit              API endpoint
cloud-posture-triage             Cloud security posture          Cloud config
container-triage                 Container security              Container image
iac-triage                       Infrastructure-as-Code          IaC files
mobile-app-triage                Mobile app security             APK/IPA
```

---

## 2. RUNBOOK: appsec-web-triage

**Purpose:** Quick AppSec triage of a web application. Find the low-hanging fruit fast.

```
PHASE 1: DISCOVER (5 min)
├── Fingerprint technology stack
│   ├── Check HTTP headers (Server, X-Powered-By)
│   ├── Check HTML meta tags, comments, JS sources
│   └── Use whatweb or manual inspection
├── Find all entry points
│   ├── Login/registration forms
│   ├── Search forms
│   ├── File upload endpoints
│   ├── API endpoints (check for /api/, /graphql, /swagger)
│   └── Admin panels (/admin, /dashboard, /manage)
└── Map authentication
    ├── What auth method? (cookie, JWT, API key)
    ├── Is there MFA?
    └── Password reset flow?

PHASE 2: TEST (15 min)
├── Authentication tests
│   ├── Default credentials (admin/admin, admin/password)
│   ├── Brute force protection (rate limiting?)
│   ├── Session management (fixation, timeout)
│   └── Password policy strength
├── Authorization tests
│   ├── IDOR on user endpoints (change ID in URL/body)
│   ├── Privilege escalation (user → admin)
│   ├── Horizontal privilege escalation (access other users)
│   └── Vertical privilege escalation (access admin functions)
├── Input validation tests
│   ├── XSS on search/comment fields
│   ├── SQLi on parameterized endpoints
│   ├── Command injection on file/process endpoints
│   └── Path traversal on file endpoints
└── Configuration tests
    ├── CORS policy (reflects origin? credentials?)
    ├── Security headers (CSP, HSTS, X-Frame-Options)
    ├── Error handling (verbose errors? stack traces?)
    └── Debug mode (exposed in production?)

PHASE 3: REPORT (5 min)
├── List findings with severity
├── Prioritize by impact
├── Create reproduction steps
└── Draft report
```

---

## 3. RUNBOOK: web-surface

**Purpose:** Map the full attack surface of a web target.

```
PHASE 1: SUBDOMAIN ENUMERATION (10 min)
├── Passive
│   ├── crt.sh (certificate transparency)
│   ├── subfinder
│   ├── amass (passive mode)
│   └── SecurityTrails/VirusTotal (if available)
├── Active
│   ├── amass (active mode)
│   ├── dnsx (DNS resolution)
│   └── Subdomain brute-force
└── Output: combined subdomain list

PHASE 2: HOST DISCOVERY (5 min)
├── DNS resolution (dnsx)
├── HTTP probing (httpx)
│   ├── Status codes
│   ├── Titles
│   ├── Technology detection
│   └── CDN detection
└── Output: live hosts with metadata

PHASE 3: PORT SCANNING (10 min)
├── Fast scan (naabu top 1000)
├── Full scan (nmap -sV -sC)
├── UDP scan (if in scope)
└── Output: open ports + services

PHASE 4: DIRECTORY FUZZING (10 min)
├── Common paths (ffuf + common.txt)
├── API endpoints (ffuf + api-endpoints.txt)
├── Sensitive files (.env, .git, backup)
└── Output: discovered paths

PHASE 5: JAVASCRIPT ANALYSIS (10 min)
├── Crawl with katana/gau
├── Extract endpoints from JS
├── Find secrets/tokens in JS
├── Map API surface
└── Output: endpoints + secrets

PHASE 6: TECHNOLOGY FINGERPRINTING (5 min)
├── CMS detection (WordPress, Drupal, Joomla)
├── Framework detection (React, Angular, Vue)
├── Server detection (nginx, Apache, IIS)
├── WAF detection (Cloudflare, Akamai, AWS WAF)
└── Output: technology profile
```

---

## 4. RUNBOOK: pentest-starter

**Purpose:** Full pentest workflow from reconnaissance to exploitation.

```
PHASE 1: RECON (30 min)
├── Run web-surface runbook
├── OSINT on target employees
├── GitHub/GitLab code leaks
├── Shodan/Censys host info
├── Email harvesting
└── Output: full attack surface

PHASE 2: VULNERABILITY SCANNING (20 min)
├── Nuclei template scan (critical + high)
├── Manual testing of high-value endpoints
├── Authentication bypass attempts
├── IDOR testing on all user endpoints
├── SQL injection testing
├── XSS testing
└── Output: candidate findings

PHASE 3: EXPLOITATION (30 min)
├── Verify all candidate findings
├── Build proof of concept for each
├── Chain low findings into high
├── Test for privilege escalation
├── Test for data exfiltration
└── Output: verified findings with PoCs

PHASE 4: POST-EXPLOITATION (15 min)
├── Lateral movement (if applicable)
├── Persistence testing (if applicable)
├── Data access scope
├── Impact demonstration
└── Output: impact assessment

PHASE 5: REPORTING (15 min)
├── Document all findings
├── Create reproduction steps
├── Assign severity
├── Write remediation guidance
└── Output: final report
```

---

## 5. RUNBOOK: network-surface

**Purpose:** Network attack surface mapping for IP/CIDR targets.

```
PHASE 1: HOST DISCOVERY (5 min)
├── Ping sweep
├── ARP scan (if local)
├── TCP SYN scan (top ports)
└── Output: live hosts

PHASE 2: PORT SCANNING (15 min)
├── Full port scan (all 65535)
├── Service version detection
├── OS detection
├── Script scan (-sC)
└── Output: open ports + services

PHASE 3: SERVICE ENUMERATION (15 min)
├── Web servers (HTTP/HTTPS)
│   ├── Directory fuzzing
│   ├── Technology detection
│   └── SSL/TLS analysis
├── SSH
│   ├── Version check
│   └── Key-based auth test
├── Database ports
│   ├── MySQL (3306) — default creds
│   ├── PostgreSQL (5432) — default creds
│   ├── MongoDB (27017) — auth bypass
│   ├── Redis (6379) — unauthenticated
│   └── MSSQL (1433) — default creds
├── Mail ports
│   ├── SMTP (25/587) — open relay test
│   └── IMAP/POP3 — default creds
└── Other services
    ├── SMB (445) — null session
    ├── RDP (3389) — default creds
    ├── VNC (5900) — default creds
    └── Docker (2375/2376) — unauthenticated
```

---

## 6. RUNBOOK: osint-target

**Purpose:** OSINT research on a domain target.

```
PHASE 1: DOMAIN RECON (10 min)
├── WHOIS lookup
├── DNS records (A, AAAA, MX, NS, TXT, CNAME, SOA)
├── Certificate transparency (crt.sh)
├── Reverse DNS
└── Output: domain profile

PHASE 2: SUBDOMAIN ENUMERATION (10 min)
├── Passive (crt.sh, SecurityTrails)
├── Active (subfinder, amass)
├── DNS resolution
└── Output: subdomain list

PHASE 3: EMAIL HARVESTING (10 min)
├── theHarvester
├── Hunter.io
├── GitHub email search
├── LinkedIn employee enumeration
└── Output: email list

PHASE 4: PUBLIC DATA (10 min)
├── GitHub/GitLab code search
├── Pastebin/pastebin clones
├── Shodan/Censys
├── Google dorking
├── Wayback Machine
└── Output: public data findings

PHASE 5: INFRASTRUCTURE (10 min)
├── IP ranges (BGP info)
├── ASN lookup
├── Cloud provider identification
├── CDN detection
├── WAF detection
└── Output: infrastructure profile
```

---

## 7. RUNBOOK: api-security-audit

**Purpose:** Security audit of REST/GraphQL APIs.

```
PHASE 1: API DISCOVERY (10 min)
├── Swagger/OpenAPI endpoints (/swagger, /api-docs, /openapi.json)
├── GraphQL introspection (/graphql)
├── JavaScript analysis for API endpoints
├── Network tab analysis
├── Mobile app API calls
└── Output: API endpoint list

PHASE 2: AUTHENTICATION (10 min)
├── Auth method identification
├── Token validation
├── API key management
├── OAuth flow analysis
├── Rate limiting test
└── Output: auth assessment

PHASE 3: AUTHORIZATION (15 min)
├── IDOR on all endpoints
├── Mass assignment testing
├── Horizontal privilege escalation
├── Vertical privilege escalation
├── Function-level authorization
└── Output: authz findings

PHASE 4: INPUT VALIDATION (15 min)
├── SQL injection
├── NoSQL injection
├── Command injection
├── XXE
├── SSTI
├── Parameter pollution
└── Output: injection findings

PHASE 5: BUSINESS LOGIC (10 min)
├── Rate limiting bypass
├── Race conditions
├── Price/quantity manipulation
├── Workflow bypass
├── State machine manipulation
└── Output: logic findings
```

---

## 8. RUNNING A RUNBOOK

```bash
# List available runbooks
/runbook list

# Run a specific runbook
/runbook run appsec-web-triage https://example.com

# Run with custom target
/runbook run web-surface https://api.example.com

# Check current runbook progress
/runbook status

# Switch to next phase
/runbook next

# Pause and resume later
/runbook pause
/runbook resume
```

---

## 9. RUNBOOK PROGRESS TRACKING

```markdown
## Runbook: appsec-web-triage — example.com

**Started:** 2026-01-16 09:00
**Status:** Phase 2 (Testing)

### Phase 1: Discover ✅ Complete
- [x] Technology stack: React + Node.js + PostgreSQL
- [x] Entry points: /login, /search, /api/users, /upload
- [x] Auth: JWT tokens, no MFA

### Phase 2: Test 🔄 In Progress
- [x] Default credentials: None found
- [x] IDOR: Found on /api/users/{id} — HIGH
- [ ] XSS: Testing search field
- [ ] SQLi: Testing /search
- [ ] CORS: Checked — reflects origin with credentials — HIGH
- [ ] Security headers: Missing CSP — INFO

### Phase 3: Report ⏳ Pending
- [ ] Document findings
- [ ] Create PoCs
- [ ] Write report
```
