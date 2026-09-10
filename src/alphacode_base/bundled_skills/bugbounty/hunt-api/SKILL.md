---
name: hunt-api
description: API Security hunting with differential testing — BOLA/IDOR, mass assignment, rate limiting bypass, authentication bypass, authorization bypass. Every candidate must pass the 7-gate validation. Optimized for validated findings per hour.
---

# API SECURITY HUNTING — DIFFERENTIAL TESTING METHOD

**APIs are the #1 attack surface in modern apps. Test them systematically.**

---

## HYPOTHESIS GENERATION

```
HYPOTHESIS: [vuln class] on [API endpoint]
  Endpoint: [METHOD] [URL]
  Precondition: [attacker starting position]
  Expected: [normal authorization behavior]
  Attack: [what attacker does differently]
  Impact: [security property violated]
  Confidence: [HIGH/MEDIUM/LOW]
```

---

## API SURFACE DISCOVERY

Before testing, map the complete API surface:

```bash
# Discover API endpoints from multiple sources

# 1. Swagger/OpenAPI
curl -s https://target.com/swagger.json | python3 -m json.tool
curl -s https://target.com/openapi.json | python3 -m json.tool
curl -s https://target.com/api-docs | python3 -m json.tool

# 2. JavaScript analysis
katana -u target.com -d 3 -jc | grep -oE "/api/[a-zA-Z0-9/_-]+" | sort -u > api_endpoints.txt

# 3. Wordlist fuzzing
ffuf -u https://target.com/api/FUZZ -w /usr/share/wordlists/seclists/Discovery/Web-Content/api/api-endpoints.txt -o api_fuzz.json

# 4. GraphQL introspection
curl -s -X POST https://target.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ __schema { types { name fields { name } } } }"}' | python3 -m json.tool

# 5. kiterunner — API-specific scanning
kr scan https://target.com -w routes-large.kr -o kr_results.txt
```

---

## AUTHORIZATION TESTING (BOLA/IDOR)

### The Differential Matrix

```
TEST MATRIX:
  User A → resource A  (baseline — should work)
  User A → resource B  (test — should FAIL)
  User B → resource A  (test — should FAIL)
  unauthenticated → resource A  (test — should FAIL)

FINDING: If any cross-user or unauthenticated request succeeds → BOLA
```

### Implementation

```bash
TOKEN_A="user-a-token"
TOKEN_B="user-b-token"
VICTIM_ID="user-b-resource-id"

# Step 1: Baseline
curl -s -H "Authorization: Bearer $TOKEN_A" \
  "https://target.com/api/v2/users/$VICTIM_ID"

# Step 2: Differential
# If Step 1 returns User B's data → BOLA CONFIRMED
# If Step 1 returns 403/404 → correctly protected
```

---

## MASS ASSIGNMENT TESTING

### The Differential Matrix

```
TEST MATRIX:
  Normal request: {"name": "John", "email": "john@test.com"}
  With extra fields: {"name": "John", "email": "john@test.com", "role": "admin"}
  With privileged fields: {"name": "John", "is_verified": true, "balance": 999999}
  With internal fields: {"name": "John", "id": "other-user-id", "tenant_id": "other-tenant"}

FINDING: If extra fields are accepted and affect behavior → mass assignment
```

### Implementation

```bash
# Step 1: Normal request (baseline)
curl -s -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test User","email":"test@example.com"}' \
  "https://target.com/api/users"
# Record: what fields are in the response?

# Step 2: With privileged fields
curl -s -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test User","email":"test2@example.com","role":"admin","is_verified":true,"balance":999999}' \
  "https://target.com/api/users"

# DIFFERENTIAL: Compare responses
# If "role" appears in response as "admin" → mass assignment CONFIRMED
# If extra fields are stripped → correctly protected

# Step 3: Test UPDATE (PATCH/PUT)
curl -s -X PATCH -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Test User","role":"admin"}' \
  "https://target.com/api/users/me"

# Check if role changed
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://target.com/api/users/me" | jq .role
```

---

## RATE LIMITING BYPASS

### The Differential Matrix

```
TEST MATRIX:
  Normal request → 200 (baseline)
  Rapid requests → 429 (rate limited)
  With header bypass → 200 (bypass confirmed)
  With IP rotation → 200 (bypass confirmed)

FINDING: If any bypass technique avoids rate limiting → rate limit bypass
```

### Bypass Techniques

```bash
# Header-based IP spoofing
curl -s -H "X-Forwarded-For: 1.1.1.1" "https://target.com/api/login"
curl -s -H "X-Real-IP: 2.2.2.2" "https://target.com/api/login"
curl -s -H "X-Client-IP: 3.3.3.3" "https://target.com/api/login"
curl -s -H "X-Originating-IP: 4.4.4.4" "https://target.com/api/login"

# HTTP method change
curl -s -X GET "https://target.com/api/login"     # Rate limited
curl -s -X POST "https://target.com/api/login"    # Not rate limited?

# Content-Type change
curl -s -X POST -H "Content-Type: application/json" "https://target.com/api/login"
curl -s -X POST -H "Content-Type: application/xml" "https://target.com/api/login"

# Parameter position
curl -s "https://target.com/api/login?username=admin&password=test"   # Query
curl -s -X POST -d "username=admin&password=test" "https://target.com/api/login"  # Body
```

---

## AUTHENTICATION BYPASS

### The Differential Matrix

```
TEST MATRIX:
  Valid token → 200 (baseline)
  No token → 401/403 (expected)
  Empty token → 401/403 (expected)
  Invalid token → 401/403 (expected)
  Expired token → 401 (expected)
  Different user's token → 401/403 (expected)

FINDING: If any invalid variant succeeds → auth bypass
```

### Implementation

```bash
ENDPOINT="https://target.com/api/admin/users"

# Baseline: valid token
curl -s -H "Authorization: Bearer VALID_TOKEN" "$ENDPOINT" -w "\n%{http_code}"

# Test variants
curl -s "$ENDPOINT" -w "\nNo auth: %{http_code}"
curl -s -H "Authorization: " "$ENDPOINT" -w "\nEmpty token: %{http_code}"
curl -s -H "Authorization: Bearer invalid" "$ENDPOINT" -w "\nInvalid: %{http_code}"
curl -s -H "Authorization: Bearer eyhbGciOiJub25lIiwidHlwIjoiSldUIn0." "$ENDPOINT" -w "\nNone alg: %{http_code}"

# JWT none algorithm
# Decode JWT, change "alg" to "none", remove signature
curl -s -H "Authorization: Bearer eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ." "$ENDPOINT"
```

---

## HTTP METHOD CONFUSION

### The Differential Matrix

```
TEST MATRIX for each endpoint:
  GET → read (should have auth)
  POST → create (should have auth)
  PUT → update (should have auth + ownership)
  DELETE → remove (should have auth + ownership)
  PATCH → partial update (should have auth + ownership)
  OPTIONS → should not expose sensitive info
  HEAD → should match GET behavior
  TRACE → should be disabled

FINDING: If any method lacks auth while others have it → method-based bypass
```

```bash
METHODS=("GET" "POST" "PUT" "DELETE" "PATCH" "OPTIONS" "HEAD" "TRACE")
ENDPOINT="https://target.com/api/users/$VICTIM_ID"

for method in "${METHODS[@]}"; do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -X $method \
    -H "Authorization: Bearer $TOKEN_A" \
    -H "Content-Type: application/json" \
    "$ENDPOINT")
  echo "$method → $response"
done
```

---

## API VERSION DIFFERENTIAL

```bash
# Test all discovered versions
VERSIONS=("/api/v1" "/api/v2" "/api/v3" "/api/internal" "/api/debug" "/api/staging")

for version in "${VERSIONS[@]}"; do
  response=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN_A" \
    "https://target.com$version/users/$VICTIM_ID")
  echo "$version → $response"
done

# DIFFERENTIAL: If older/internal versions lack auth → version-based bypass
```

---

## GATE VALIDATION CHECKLIST

### Gate 1 — Scope
- [ ] Affected endpoint is in program scope

### Gate 2 — Security Boundary
- [ ] Cross-user access or privilege escalation demonstrated
- [ ] Boundary violation clearly documented

### Gate 3 — Attacker Capability
- [ ] Starting position realistic
- [ ] No unrealistic prerequisites

### Gate 4 — Reproducibility
- [ ] Exact request/response captured
- [ ] Steps reproducible by triager

### Gate 5 — Impact
- [ ] Impact class selected: Confidentiality / Integrity / Authorization
- [ ] Quantified: "can access N users' data" or "can escalate to admin"

### Gate 6 — False Positive Elimination
- [ ] Response contains real data (not empty/mock)
- [ ] Not a public endpoint
- [ ] Not the user's own data

### Gate 7 — Program Acceptance
- [ ] Bug class in scope
- [ ] Severity meets threshold
- [ ] Checked for duplicates

---

## SEVERITY ASSESSMENT

| Vuln Class | Impact | Typical Severity |
|---|---|---|
| BOLA/IDOR (read PII) | Data breach | Medium-High |
| BOLA/IDOR (read financial) | Financial impact | High-Critical |
| Mass assignment (role escalation) | Privilege escalation | High |
| Mass assignment (balance manipulation) | Financial fraud | Critical |
| Auth bypass (admin access) | System compromise | Critical |
| Rate limit bypass + credential stuffing | Account takeover | High |
| Method confusion (DELETE without auth) | Data destruction | High |

---

## CHAINS

```
BOLA + Mass Assignment → admin takeover → Critical
Auth bypass + BOLA → mass data exfil → Critical
Rate limit bypass + OTP brute → ATO → High
Method confusion + IDOR → data modification → High
Version differential + BOLA → legacy API exploitation → High
```
