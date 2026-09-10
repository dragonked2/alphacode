---
name: bugbounty
description: Elite bug bounty hunting skill with mandatory 7-gate finding validation, differential testing, hypothesis-driven hunting, and attack surface inventory. When user mentions bug bounty, hacking, pentesting, security testing, vulnerability research, recon, exploitation, or offensive security work. This skill optimizes for VALIDATED FINDINGS PER HOUR, not finding count. Every candidate must pass 7 gates before report.
sources: community, public_research, claude-bughunter, agentic-bug-hunter
---

# ALPHACODE BUG BOUNTY HUNTER — HYPOTHESIS-DRIVEN WITH MANDATORY VALIDATION

**This skill optimizes for: validated findings per hour.**

**Acceptable output:**
```
Targets analyzed: 1,842
Candidates generated: 613
Rejected at gates: 604
Validated: 9
High/Critical: 2
```

**Unacceptable output:**
```
613 vulnerabilities found
```
where 90% are API keys, CORS headers, missing security headers, exposed versions, and theoretical issues.

---

## PHASE 0: ATTACK SURFACE INVENTORY

Before any testing, build a complete inventory. Prioritize state-changing and authorization-sensitive functionality over cosmetic findings.

### Inventory Checklist

```
WEB APPLICATIONS
  [ ] Primary web app(s) — main domain, subdomains
  [ ] Admin interfaces — /admin, /dashboard, /manage, /internal
  [ ] Staging/dev environments — staging.*, dev.*, test.*
  [ ] Documentation — /docs, /api-docs, /swagger, /openapi

APIS
  [ ] REST APIs — /api/v1, /api/v2, /v1/, /v2/
  [ ] GraphQL — /graphql, /api/graphql, /gql
  [ ] WebSocket endpoints — wss://, ws://
  [ ] Mobile/backend APIs — separate subdomain, different auth
  [ ] Internal APIs — not linked from frontend

AUTHENTICATION/OAUTH
  [ ] Login endpoints — /login, /auth, /sso
  [ ] OAuth flows — authorization endpoint, token endpoint, callback
  [ ] SAML/SSO — SSO callback, metadata endpoints
  [ ] Password reset — /forgot, /reset
  [ ] MFA/2FA — /mfa, /verify, /otp
  [ ] Registration — /signup, /register
  [ ] API key management — /keys, /tokens, /credentials

FILE HANDLING
  [ ] File upload — /upload, /import, /attach
  [ ] File download — /download, /export, /files
  [ ] Avatar/image upload — /avatar, /profile/image
  [ ] Document generation — /pdf, /report, /export

PAYMENT/FINANCIAL
  [ ] Checkout — /checkout, /pay, /purchase
  [ ] Billing — /billing, /subscription, /plan
  [ ] Webhooks — /webhook, /callback, /notify
  [ ] Refunds — /refund, /credit
  [ ] Invoices — /invoice, /receipt

CLOUD/INFRASTRUCTURE
  [ ] S3/GCS/Azure Blob — file storage
  [ ] CDN — static assets
  [ ] Email service — /send, /email
  [ ] Analytics — tracking endpoints

INTERNAL BOUNDARIES
  [ ] Tenant isolation — multi-tenant endpoints
  [ ] Service-to-service — internal RPC/gRPC
  [ ] Database access — direct DB connections
  [ ] Cache — Redis, Memcached

CROSS-ORIGIN
  [ ] CORS configuration — which origins are trusted
  [ ] Webhooks outbound — what URLs are called
  [ ] Third-party integrations — OAuth clients, API consumers
```

### Inventory Output Format

```
INVENTORY — target.com
═══════════════════════════════════════════════════════════
Category          │ Endpoint                    │ Auth Required │ State-Changing
══════════════════╪═════════════════════════════╪═══════════════╪═══════════════
Web App           │ https://target.com          │ Yes           │ No
Admin Panel       │ https://target.com/admin    │ Yes (admin)   │ Yes
REST API v1       │ /api/v1/users               │ Yes           │ Yes
REST API v2       │ /api/v2/users               │ Yes           │ Yes
GraphQL           │ /graphql                    │ Yes           │ Yes
WebSocket         │ wss://target.com/ws         │ Yes           │ Yes
File Upload       │ /api/upload                 │ Yes           │ Yes
Payment           │ /api/checkout               │ Yes           │ Yes
Webhook           │ /api/webhook                │ No (IP)       │ Yes
Password Reset    │ /auth/reset                 │ No            │ Yes
OAuth Callback    │ /auth/callback              │ No            │ No
═══════════════════════════════════════════════════════════
```

### Priority Matrix

```
PRIORITY 1 — Test FIRST (state-changing + auth-sensitive):
  Payment/billing endpoints
  Admin panels
  Authentication system (login, reset, OAuth)
  File upload endpoints
  User management (CRUD on user data)
  Webhook endpoints
  Multi-tenant isolation boundaries

PRIORITY 2 — Test SECOND (read + auth):
  API endpoints with ID parameters
  Data export/download endpoints
  GraphQL queries with user data
  Search/filter endpoints

PRIORITY 3 — Test LAST (low-value, cosmetic):
  Static assets
  Documentation pages
  Contact forms
  Newsletter signup
  Health check endpoints
```

---

## PHASE 1: VULNERABILITY HYPOTHESIS GENERATION

Instead of blindly firing payloads, generate structured hypotheses. Rank by: **impact × exploitability × confidence**.

### Hypothesis Template

```
HYPOTHESIS: [vuln class] on [endpoint/feature]
  Endpoint: [METHOD] [URL]
  Precondition: [attacker starting position]
  Expected behavior: [what should happen]
  Attack behavior: [what attacker expects]
  Impact if true: [concrete security property violated]
  Confidence: [HIGH/MEDIUM/LOW]
  Priority score: [impact × exploitability × confidence]
```

### Hypothesis Ranking Matrix

| Impact (1-5) | Exploitability (1-5) | Confidence (1-5) | Priority Score | Action |
|---|---|---|---|---|
| 5 (RCE/ATO) | 5 (trivial) | 5 (confirmed) | 125 | REPORT IMMEDIATELY |
| 5 (RCE/ATO) | 3 (requires skill) | 3 (probable) | 45 | INVESTIGATE NOW |
| 3 (data read) | 5 (trivial) | 4 (likely) | 60 | INVESTIGATE NOW |
| 2 (info disclosure) | 5 (trivial) | 5 (confirmed) | 50 | LOW PRIORITY |
| 1 (missing header) | 5 (trivial) | 5 (confirmed) | 25 | SKIP OR CHAIN |

### Hypothesis Classes (Ranked by Typical Impact)

```
TIER 1 — CRITICAL/HIGH (test these first):
  Broken access control / IDOR
  Authentication bypass
  Privilege escalation
  Business-logic flaws (payment, coupon, race)
  Account takeover
  SSRF → cloud metadata / internal services
  SQL/NoSQL injection
  Command injection
  File upload → RCE
  OAuth flaws → ATO

TIER 2 — MEDIUM/HIGH:
  Stored XSS
  SSTI
  Path traversal
  GraphQL auth bypass
  Webhook abuse
  Race conditions
  Multi-tenant isolation failures
  API authorization flaws

TIER 3 — MEDIUM/LOW:
  Reflected XSS
  CSRF (chained with sensitive action)
  Open redirect (chained with OAuth)
  Information disclosure
  Missing security headers (chained only)
```

---

## PHASE 2: DIFFERENTIAL TESTING

This is the core methodology. The agent must seek **behavioral differences**, not merely interesting responses.

### Authorization Differential

```
TEST MATRIX:
  User A → resource A (baseline — should work)
  User A → resource B (should FAIL — different user's resource)
  User B → resource A (should FAIL — different user accessing A's resource)
  unauthenticated → resource A (should FAIL — no credentials)

EXPECTED: All cross-user and unauthenticated requests should fail identically.
FINDING: If any cross-user request succeeds → authorization bypass.
```

### Privilege Escalation Differential

```
TEST MATRIX:
  low_priv_user → privileged_operation (should FAIL)
  high_priv_user → privileged_operation (should SUCCEED)
  low_priv_user + admin_token → privileged_operation (should FAIL if token bound)

FINDING: If low_priv succeeds → privilege escalation.
```

### State Change Differential

```
TEST SEQUENCE:
  1. Record state BEFORE: GET /api/resource/X → response_A
  2. Execute attack: PUT/POST/PATCH /api/resource/X with malicious payload
  3. Record state AFTER: GET /api/resource/X → response_B
  4. Compare: response_A vs response_B

FINDING: If state changed unauthorized → integrity violation.
NOT A FINDING: If state unchanged → no impact, reject.
```

### Authentication Differential

```
TEST MATRIX:
  valid_token → protected_resource (should SUCCEED)
  modified_token → protected_resource (should FAIL)
  expired_token → protected_resource (should FAIL)
  replayed_token → protected_resource (should FAIL based on expiry)
  no_token → protected_resource (should FAIL)
  different_user_token → protected_resource (should FAIL)

FINDING: If modified/expired/replayed token succeeds → auth bypass.
```

### API Version Differential

```
TEST MATRIX:
  /api/v2/endpoint (current — has auth)
  /api/v1/endpoint (old — may lack auth)
  /api/internal/endpoint (internal — may lack auth)
  /api/debug/endpoint (debug — may lack auth)

FINDING: If older/internal version lacks auth → version-based bypass.
```

### HTTP Method Differential

```
TEST MATRIX for each endpoint:
  GET → read (should have auth)
  POST → create (should have auth + CSRF)
  PUT → update (should have auth + ownership check)
  DELETE → remove (should have auth + ownership check)
  PATCH → partial update (should have auth + ownership check)
  OPTIONS → should not expose sensitive info
  HEAD → should match GET behavior

FINDING: If any method lacks auth while others have it → method-based bypass.
```

### Content-Type Differential

```
TEST MATRIX:
  Content-Type: application/json (normal)
  Content-Type: application/xml (XXE?)
  Content-Type: text/plain (parser confusion?)
  Content-Type: application/x-www-form-urlencoded (parameter pollution?)
  Content-Type: multipart/form-data (file upload bypass?)

FINDING: If different content type bypasses validation → parser confusion.
```

---

## PHASE 3: MANDATORY 7-GATE FINDING VALIDATION

**Every candidate finding MUST pass all 7 gates. No exceptions.**

### Gate 1 — Scope

```
QUESTION: Is the affected asset explicitly in scope for the target's current bug bounty program?
  YES → proceed to Gate 2
  NO → REJECT (no exceptions based on how interesting the issue appears)
```

### Gate 2 — Security Boundary

```
QUESTION: What security boundary, trust boundary, authorization boundary, or security control is actually being violated?

EXPECTED BOUNDARY VIOLATIONS:
  User → another user
  Unauthenticated → authenticated
  Low privilege → high privilege
  Tenant A → Tenant B
  Internet → internal service
  Untrusted input → privileged interpreter

  VIOLATION IDENTIFIED → proceed to Gate 3
  NO VIOLATION → REJECT/INFO
```

### Gate 3 — Attacker Capability

```
QUESTION: What can a realistic attacker do with the finding, using only capabilities an attacker would reasonably possess?

ATTACKER STARTING POSITION:
  [ ] Unauthenticated Internet user
  [ ] Authenticated normal user
  [ ] Low-privileged account
  [ ] Compromised account
  [ ] Internal network position

  If exploitation requires an unrealistic or unavailable prerequisite → DOWNGRADE or REJECT
```

### Gate 4 — Reproducibility

```
QUESTION: Can the vulnerability be reproduced deterministically with the minimum necessary requests?

REQUIRED EVIDENCE:
  [ ] Preconditions stated
  [ ] Minimal request sequence provided
  [ ] Relevant response captured
  [ ] Observable security consequence demonstrated

  "This might be exploitable" → REJECT
  "This causes X under condition Y" → proceed to Gate 5

  If it cannot be reproduced → HYPOTHESIS ONLY (do not report as confirmed)
```

### Gate 5 — Impact

```
QUESTION: What concrete security impact occurs after exploitation?

REQUIRED — select one or more:
  [ ] Confidentiality breach (data access)
  [ ] Integrity violation (data modification)
  [ ] Availability impact (DoS)
  [ ] Authentication bypass
  [ ] Authorization bypass
  [ ] Account takeover
  [ ] Privilege escalation
  [ ] Financial loss
  [ ] Remote code execution
  [ ] Cross-tenant compromise
  [ ] Sensitive data exposure
  [ ] Infrastructure compromise

  "Could potentially" → NOT EVIDENCE → REJECT
  Concrete impact with evidence → proceed to Gate 6
```

### Gate 6 — False-Positive Elimination

```
QUESTION: What legitimate explanation could produce the observed behavior, and what evidence rules that explanation out?

FOR EVERY CANDIDATE — actively attempt to disprove yourself:

CORS:
  Don't report: Access-Control-Allow-Origin: *
  Establish whether:
    [ ] Credentials are involved
    [ ] Sensitive authenticated data is accessible cross-origin
    [ ] An attacker-controlled origin can read the response
    [ ] The endpoint contains meaningful sensitive information
    [ ] Preflight/security behavior actually permits the attack
  Otherwise → REJECT

EXPOSED API KEY:
  Don't report: "Alchemy key works."
  Establish whether:
    [ ] It provides a sensitive capability
    [ ] That capability creates meaningful impact
  Otherwise → REJECT

GRAPHQL INTROSPECTION:
  Don't report: "Introspection works."
  Establish whether:
    [ ] There are mutations
    [ ] Mutations lack auth
    [ ] You can query other users' data
  Otherwise → REJECT

MISSING HEADERS:
  Don't report: "Missing CSP/HSTS."
  Establish whether:
    [ ] The missing header creates exploitable conditions
    [ ] You can demonstrate an attack that the header would prevent
  Otherwise → REJECT

VERSION DISCLOSURE:
  Don't report: "Server version visible."
  Establish whether:
    [ ] There's a known CVE for that version
    [ ] You can exploit the version-specific vulnerability
  Otherwise → REJECT

  False positive eliminated → proceed to Gate 7
  Cannot eliminate false positive → REJECT
```

### Gate 7 — Program Acceptance

```
QUESTION: Does the demonstrated impact satisfy the target program's vulnerability taxonomy, severity criteria, and exclusions?

CHECK:
  [ ] Scope — is this asset type covered?
  [ ] Exclusions — is this bug class excluded?
  [ ] Duplicate rules — check Hacktivity for similar reports
  [ ] Severity requirements — does it meet minimum severity?
  [ ] PoC requirements — can you meet their PoC format?
  [ ] Third-party rules — is this a third-party service?
  [ ] Rate-limit/testing restrictions — did you comply?
  [ ] Mainnet/production restrictions — are you testing prod?

  Technically valid but explicitly excluded → OUT OF SCOPE — DO NOT SUBMIT
  Meets all criteria → VALIDATED FINDING
```

---

## FINDING STATE MACHINE

Every finding must follow this state machine. No skipping states.

```
DISCOVERED
    ↓
IN SCOPE? ──────────────────── NO → REJECT
    ↓ YES
SECURITY BOUNDARY VIOLATION? ─ NO → REJECT
    ↓ YES
REALISTIC ATTACKER? ────────── NO → REJECT/DOWNGRADE
    ↓ YES
REPRODUCIBLE? ──────────────── NO → HYPOTHESIS ONLY (park it)
    ↓ YES
CONCRETE IMPACT? ───────────── NO → REJECT
    ↓ YES
FALSE-POSITIVE CHECK PASSED? ─ NO → REJECT
    ↓ YES
PROGRAM ACCEPTS IT? ────────── NO → OUT OF SCOPE
    ↓ YES
VALIDATED FINDING
    ↓
SEVERITY ASSESSMENT
    ↓
MINIMAL SAFE PoC
    ↓
REPORT
```

### State Definitions

| State | Meaning | Next Action |
|-------|---------|-------------|
| DISCOVERED | Candidate identified during testing | Run through Gate 1 |
| IN SCOPE | Asset is in program scope | Run through Gate 2 |
| BOUNDARY VIOLATED | Security boundary identified | Run through Gate 3 |
| REALISTIC ATTACKER | Attacker capability confirmed | Run through Gate 4 |
| REPRODUCIBLE | Deterministic reproduction confirmed | Run through Gate 5 |
| IMPACTED | Concrete impact demonstrated | Run through Gate 6 |
| FALSE-POSITIVE CHECKED | False positive eliminated | Run through Gate 7 |
| PROGRAM ACCEPTED | Program accepts this finding | SEVERITY ASSESS → REPORT |
| REJECTED | Failed a gate | Document why, move on |
| HYPOTHESIS ONLY | Cannot reproduce yet | Park, return later |
| OUT OF SCOPE | Valid but excluded | Do not submit |

---

## IMPACT VALIDATION — THE "SO WHAT" TEST

Every candidate finding must answer: **"What security property was actually violated?"**

### Mandatory Impact Questions

```
FOR EVERY CANDIDATE, ANSWER:

1. Can attacker access another user's data?          → Confidentiality
2. Can attacker modify another user's data?           → Integrity
3. Can attacker execute code?                         → RCE
4. Can attacker authenticate as another user?         → Authentication
5. Can attacker obtain credentials/tokens?            → Credential theft
6. Can attacker cross a tenant boundary?              → Multi-tenant isolation
7. Can attacker perform privileged actions?           → Authorization
8. Can attacker cause meaningful financial loss?      → Financial
9. Can attacker compromise infrastructure?            → Infrastructure

IF THE ANSWER IS ONLY:
  "The endpoint returned 200."
  "The header was missing."
  "The version was disclosed."
  → THERE IS NO VULNERABILITY YET.
```

### Impact Evidence Requirements

| Impact Class | Required Evidence |
|---|---|
| Confidentiality | Show data belonging to another user |
| Integrity | Show data was modified without authorization |
| Authentication | Show login as another user |
| Authorization | Show privileged action performed by low-priv user |
| RCE | Show command execution on server |
| Financial | Show monetary loss or unauthorized transaction |
| Account Takeover | Show full account access via exploit |
| Infrastructure | Show internal network/cloud access |

---

## DIFFERENTIAL TESTING PATTERNS BY VULN CLASS

### IDOR Differential

```bash
# Two-session diff — the gold standard
TOKEN_A="attacker-token"
TOKEN_B="victim-token"

# Baseline: attacker reads own data
curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/me"
# → {"id": "A", "name": "Attacker", "email": "attacker@test.com"}

# Test: attacker reads victim's data
curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/VICTIM_ID"
# → If returns victim's data → IDOR CONFIRMED
# → If returns 403/404 → correctly protected

# DIFFERENTIAL CHECK:
# Response for A's own data ≠ Response for B's data when using A's token
# The DIFFERENCE is the vulnerability
```

### SSRF Differential

```bash
# Baseline: fetch external URL
curl -s "https://target.com/api/fetch?url=http://httpbin.org/ip"
# → Returns httpbin response

# Test: fetch internal URL
curl -s "https://target.com/api/fetch?url=http://169.254.169.254/latest/meta-data/"
# → If returns AWS metadata → SSRF CONFIRMED
# → If returns error/timeout → correctly filtered

# DIFFERENTIAL CHECK:
# External fetch succeeds AND internal fetch succeeds
# The DIFFERENCE between allowed external and forbidden internal is the vulnerability
```

### XSS Differential

```bash
# Baseline: normal input
curl -s "https://target.com/search?q=hello"
# → Returns page with "hello" in response

# Test: XSS payload
curl -s "https://target.com/search?q=<script>alert(1)</script>"
# → If payload appears unescaped in response → XSS CONFIRMED
# → If payload is HTML-encoded → correctly sanitized

# DIFFERENTIAL CHECK:
# Normal input is reflected safely, malicious input is reflected unsafely
# The DIFFERENCE in handling is the vulnerability
```

### SQL Injection Differential

```bash
# Baseline: normal input
curl -s "https://target.com/api/users?id=1"
# → Returns user 1

# Test: SQLi payload
curl -s "https://target.com/api/users?id=1' OR '1'='1"
# → If returns all users → SQLi CONFIRMED
# → If returns error → might still be SQLi (error-based)
# → If returns same user → correctly parameterized

# DIFFERENTIAL CHECK:
# Normal query returns expected result, injection returns different/unexpected result
# The DIFFERENCE in behavior is the vulnerability
```

### Authentication Bypass Differential

```bash
# Baseline: valid token
curl -s -H "Authorization: Bearer VALID_TOKEN" "https://target.com/api/admin"
# → Returns admin data

# Test: no token
curl -s "https://target.com/api/admin"
# → Should return 401/403

# Test: invalid token
curl -s -H "Authorization: Bearer invalid" "https://target.com/api/admin"
# → Should return 401/403

# Test: expired token
curl -s -H "Authorization: Bearer EXPIRED_TOKEN" "https://target.com/api/admin"
# → Should return 401

# DIFFERENTIAL CHECK:
# Valid token succeeds, all invalid variants fail
# If ANY invalid variant succeeds → auth bypass
```

### Race Condition Differential

```bash
# State BEFORE
Balance=$(curl -s -H "Authorization: Bearer $TOKEN" "https://target.com/api/balance" | jq .balance)
echo "Before: $Balance"

# Send N concurrent requests
for i in $(seq 1 20); do
  curl -s -X POST -H "Authorization: Bearer $TOKEN" "https://target.com/api/redeem" \
    -d '{"coupon":"DISCOUNT50"}' &
done
wait

# State AFTER
BalanceAfter=$(curl -s -H "Authorization: Bearer $TOKEN" "https://target.com/api/balance" | jq .balance)
echo "After: $BalanceAfter"

# DIFFERENTIAL CHECK:
# If BalanceAfter shows multiple discounts applied → race condition CONFIRMED
# Expected: discount applied once. Actual: applied N times.
```

---

## ADVANCED CHAIN BUILDING

### Chain Architecture Principles

```
PRINCIPLE 1: Every Finding is a Chain Link
  Don't report individual findings
  Connect them into exploit chains
  Low + Medium + Low = Critical

PRINCIPLE 2: Chains Must Be End-to-End
  Each step must be proven
  Each step must lead to the next
  The final impact must be demonstrated

PRINCIPLE 3: Chains Pay More Than Singles
  Single IDOR: $1K-$5K
  IDOR chain to ATO: $10K-$50K
  SSRF to RCE: $50K-$500K
```

### Known A→B→C Chains

```
BUG A (Signal)          →  HUNT FOR BUG B              →  ESCALATE TO C
══════════════════════════════════════════════════════════════════════════════
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
File upload             →  SVG XSS / path traversal      →  Stored XSS → ATO
```

---

## TIME MANAGEMENT

### The 5-Minute Rule

```
If you can't determine if a finding is real within 5 minutes:
→ Mark as "needs more investigation"
→ Move to next target
→ Come back later if time permits
```

### The 20-Minute Rotation

```
Every 20 minutes, ask yourself:
1. Am I making progress?
2. Have I found anything?
3. Is this target worth more time?

If NO to all 3 → MOVE TO NEXT TARGET
```

### The 1-Hour Rule

```
If you've been on one target for 1 hour with no findings:
→ Switch to a different target
→ Come back tomorrow with fresh eyes
```

---

## TOOL MASTERY

### Tool Selection Matrix

```
TASK                        → PRIMARY TOOL      → SECONDARY TOOL
══════════════════════════════════════════════════════════════════
Subdomain enumeration       → subfinder          → amass, assetfinder
DNS resolution              → dnsx               → dig, nslookup
HTTP probing                → httpx              → curl, wget
Port scanning               → naabu              → nmap
JavaScript crawling         → katana             → gau, waybackurls
Directory fuzzing           → ffuf               → gobuster, dirsearch
Template scanning           → nuclei             → nikto
XSS testing                 → dalfox             → xsstrike
SQL injection               → sqlmap            → commix
Hidden parameter discovery  → arjun              → paramspider
Secret scanning             → trufflehog         → gitleaks
API endpoint discovery      → kiterunner         → ffuf
Subdomain takeover          → subzy              → dnsreaper
Static analysis             → semgrep            → bandit, brakeman
```

---

## REPORTING

### Report Structure

```markdown
## Title: [Vuln] in [Endpoint] allows [Impact]

## Summary (1 paragraph)
[What] in [where] allows [attacker] to [impact] affecting [scope].
I confirmed this by [method] and demonstrated [proof].

## Steps to Reproduce
1. [Exact HTTP request — copy-paste ready]
2. [Exact response showing impact]
3. [Screenshot/video of impact]

## Impact
- [N] users affected
- [Data type] exposed
- [$ amount] at risk
- CVSS: [Score] — [Vector]

## Fix
[1-2 sentences with code example]
```

### Payout Optimization

```
SEVERITY → PAYOUT RANGE → HOW TO MAXIMIZE
══════════════════════════════════════════════════════════════
Critical  → $10K-$500K  → Prove ATO, RCE, or mass data exfil
High      → $5K-$50K    → Prove privilege escalation or financial impact
Medium    → $1K-$15K    → Prove data access or auth bypass
Low       → $200-$5K    → Chain with other findings for higher severity
Info      → $0-$500     → Only if chained with other findings
```

---

**Remember**: The goal is to find bugs that cause REAL HARM to REAL USERS. Every test must have a concrete HTTP request as evidence. Every finding must pass all 7 gates. Every report must be copy-paste ready for submission. Optimize for validated findings per hour, not finding count.
