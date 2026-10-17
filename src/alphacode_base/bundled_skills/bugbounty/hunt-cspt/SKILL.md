---
name: hunt-cspt
description: Client-Side Prototype Pollution to XSS/CSPT (client-side path traversal) — __proto__ pollution via URL params, gadget exploitation, fetch() path injection, CSPT flagging with regex checks, DOM sinks. Pairs with hunt-prototype-pollution, hunt-xss.
---

# CLIENT-SIDE PATH TRAVERSAL (CSPT) & POLLUTION GADGETS — Advanced Guide

**Core:** CSPT is a JS-level vulnerability where attacker-controlled input flows into a **local client-side `fetch()`/XHR path**. Unlike SSRF, the request goes out as the *victim user* — so it can hit authenticated, CSRF-protected endpoints. The high-severity exploit is usually "CSPT → authenticated POST request the victim never intended".

## 1. FINDING CSPT SINKS

```javascript
// In client JS (bundle*.js), hunt for path construction from attacker input:
grep -E "fetch\([a-z_$]|\.open\(['\"]?(GET|POST)['\"],\s*[a-z_$]" bundle.js
grep -E "location\.search|URLSearchParams|parseParams" bundle.js
// The dangerous shape: fresh URL assembled from a param, no allowlist:
fetch(`/api/${route}/inventory`)         // route = tracked from URL hash/query
fetch(base + path) 
```
- The tell: one parameter that changes the *path* rather than a query value. Watch DevTools Network filter for your pollutants (`#test`, `/test`, `../`) — the **request that changes shape** with your input is CSPT-flagged.

## 2. CONFIRMATION (the expensive-looking part done cheap)

```javascript
// Pollute and watch:
https://target.com/inventory#/page/../../api/users/delete
// or query style, per app routing:
https://target.com/?page=%2F%2E%2E%2Fadmin%2Fpanel-stealer
```
Checklist on confirmation:
1. The path traversal must be visible in the actual outgoing request (DevTools → Network → "Copy as fetch").
2. No allowlist/blocklist exists in the JS for that param → note the gadget (which check you'd have expected).
3. For POST-based CSPT: the victim's browser sends **their own cookies** — including CSRF tokens the app auto-attaches — so this defeats same-site defenses.

## 3. FLAGGING GADGETS (regex quickcheck)

```javascript
// Generic CSPT flagger — paste in browser console on any SPA page:
(() => {
  const orig = window.fetch;
  window.fetch = function(input, init) {
    const url = (typeof input === 'string') ? input : input.url;
    console.log(`[CSPT-FLAG] fetch: ${url}`);
    return orig.apply(this, [input, init]);
  };
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(m,u){ console.log(`[CSPT-FLAG] xhr ${m} ${u}`); return open.apply(this,[m,u]); };
})();
// Now click around / change query params; anything whose path echoes your controlled
// portion is a live sink. (Remove flagging before shipping results — it changes nothing server-side but keep reports clean.)
```

## 4. PROTOTYPE POLLUTION → EXPLOITABLE GADGETS (JS)

```http
GET /?__proto__[admin]=1
GET /?__proto__[x]=&__proto__[y]=…
or JSON-body variant: {"__proto__":{"admin":true}}
```

```javascript
//.polluted → find real world gadget sinks in the app itself
function polluted(d){const s=JSON.stringify({...default_stringify(d)});return s}
// Known device:
// (a) fetch wrapper that merges config: opts = {...defaults, ...userOpts} (prototype method hijack)
fetch(String(location))   
// (b) client-side template injection: `__proto__.innerHTML` etc.
// (c) HTTP header injection gadget: polluted key shows in WebGL shifts; check responses for content-type no-change shapes.
```

**Exploit chain checklist:**
- Sink that merges objects (merge(key, custom)) → pollution gain → persists to next application section → escalate to known CSS? → hunt-prototype-pollution for the bible.

## 5. REMEDIATION NOTES (what valid fixes look like)
- Allowlist route paths client-side (choice must be after validation, not before).
- Array-store for `Object.keys` merges; check `sanitize.js` is only half a fix (delete is not enough for `__proto__`).
- State the gadget and payload URLs — a "could-be" without an executed `fetch` is not a CSPT report.

## 6. REPORT AUTH POSITIONS
- CSPT: medium usually; becomes high when the polluted path executes an authenticated state-changing call (the app's own guards and CSRF handling are attached to the un-intended request).
- Prototype pollution: standalone reports are often LOW until paired with a gadget; collect evidence of *both* internally before filing.
- Never say "bypassed CSRF" unless the app's own token was attached to the un-intended call — that is the entire severity argument.
