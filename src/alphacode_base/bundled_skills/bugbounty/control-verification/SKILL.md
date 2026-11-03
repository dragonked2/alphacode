---
name: control-verification
description: Treats every security control as a falsifiable claim rather than a fact — the 3x3 actor/resource matrix, per-class control ledger, and differential tests for authn, authz, tenant isolation, validation, encoding, rate limits, session and cache controls. Use when testing an app that appears to have protections, when a control looks like it works, or before concluding an area is clean.
---

# CONTROL VERIFICATION — TRUST NOTHING, INCLUDING THE CONTROLS

**Core principle:** a control is a falsifiable claim. Untested actor/resource
pairs are coverage gaps, not bugs. Establish intended visibility and privileges
from the feature contract before selecting a test. Status codes alone neither
prove a boundary violation nor establish that every path is protected.

---

## THE 3x3 ACTOR / RESOURCE MATRIX

The highest-yield artifact in this skill. For any endpoint that returns or
mutates a resource, run all nine cells. **The bug is in the cell nobody tested.**

|              | **Own resource** | **Other user's resource** | **Nonexistent resource** |
|--------------|------------------|---------------------------|--------------------------|
| **Anonymous** | must 401/403     | must 401/403              | must 401/403             |
| **User A**    | must 200         | must 403 (or 404)         | must 404                 |
| **User B**    | must 403         | must 200                  | must 404                 |

```bash
BASE="https://target.com/api/orders"
A_ORD=$(curl -s -H "Authorization: Bearer $TOKEN_A" "$BASE" | jq -r '.[0].id')
B_ORD=$(curl -s -H "Authorization: Bearer $TOKEN_B" "$BASE" | jq -r '.[0].id')

# Row: User A. Col: B's resource.  <- the finding when this returns 200
curl -s -o /dev/null -w "anon/own=%{http_code}  " "$BASE/$A_ORD"
curl -s -o /dev/null -w "anon/other=%{http_code}  " "$BASE/$B_ORD"
curl -s -o /dev/null -w "A/own=%{http_code}  " -H "Authorization: Bearer $TOKEN_A" "$BASE/$A_ORD"
curl -s -o /dev/null -w "A/other=%{http_code}  " -H "Authorization: Bearer $TOKEN_A" "$BASE/$B_ORD"   # <-- KEY
curl -s -o /dev/null -w "A/nonexistent=%{http_code}\n" -H "Authorization: Bearer $TOKEN_A" "$BASE/999999999"
```

**Interpreting the results correctly:**

- The matrix above is an example for private owner-only resources, not a universal
  HTTP contract. Define expected results for public, shared, delegated and tenant
  resources before testing. Use two owned accounts with separate cookie jars.
- `A/other=200` is a candidate only. Confirm the body contains B's private fixture,
  that A lacks a sharing/delegation grant, and that B's authorized read succeeds.
- A `403` body leaking B's private fixture can demonstrate disclosure; returning
  A's own data ordinarily does not demonstrate a cross-account violation.
- A nonexistent ID returning `200` may be a soft-404 or SPA shell. Compare a known
  resource, a random absent resource, and the actual data before alleging an oracle.
- Anonymous access is valid for public data and potentially a bug for private
  data. Determine intent; never infer privacy solely from the status code.
- Different methods may legitimately have different policies. Confirm an
  unauthorized effect, including persisted state or job output, for each claim.
- Use disposable owned objects for mutations. Do not run the method loop below
  against valuable objects: DELETE/PUT/PATCH can change data even without a body.

---

## THE CONTROL LEDGER

Maintain a running table in assessment notes. A control with no test result is
recorded as `UNTESTED`, never as `OK` — untested is the honest default.

```
| Control              | Claim source          | Test           | Result   | Status |
|----------------------|-----------------------|----------------|----------|--------|
| Order read authZ     | 401 on GET /orders    | 3x3 matrix     | B private fixture read by A | BROKEN |
| Order write authZ    | (none observed)       | DELETE as A/B  | B fixture deleted by A | BROKEN |
| Tenant isolation     | docs claim per-tenant | header swap    | owned tenant B canary read | BROKEN |
| Promo single-use     | "one per customer"    | 2x replay      | 2x 200; no settled effect checked | INCONCLUSIVE |
| Upload type filter   | "images only"         | .php named .png| accepted; no consequence shown | INCONCLUSIVE |
```

Claim sources, in order of strength: **explicit documentation** > **API docs /
OpenAPI schema** > **front-end code that hides the control** > **observed 401** >
**assumed**. A hidden UI element is a hypothesis source, not an authorization contract.
Confirm the role policy and demonstrate unauthorized data or state at the API.

---

## PER-CLASS FALSIFICATION CATALOG

Each row suggests a hypothesis and initial probe. A single anomalous response
is insufficient: pair it with a valid baseline, a discriminating negative control,
and an observed security consequence. Apply the program request budget.

### Authentication
```
Claim "valid session required"   → drop the Authorization header entirely
Claim "token is verified"        → alg=none, empty signature, HS/RS confusion
Claim "sessions expire"          → replay a token from hours ago
Claim "logout works"             → replay a token issued before logout
```

### Authorization
```
Claim "only admins"              → User A (non-admin) hits the admin route
Claim "owner-only edit"          → A PATCHes B's object
Claim "role is server-side"      → tamper role/admin/is_staff in the body
Claim "resource is scoped"       → the full 3x3 matrix
```

### Tenant isolation (multi-tenant SaaS — highest severity per finding)
```
Claim "tenants are isolated"     → A's token + B's tenant_id in the path/body
Claim "tenant from the session"  → remove the tenant header entirely
Claim "cross-tenant blocked"     → A's token + B's *object* id (not tenant id)
```

### Input validation
```
Claim "server validates"         → client-side-only field, mutated in the request
Claim "type enforced"            → array/object where a string is expected
Claim "length limited"           → 10k-char value, boundary values 0/-1/MAXINT
Claim "enum restricted"          → value outside the documented enum
```

### Output encoding
```
Claim "output is encoded"        → reflect a unique marker, check raw vs encoded
Claim "CSP blocks script"        → inject a marker, read the CSP header
```

### Rate limiting / anti-automation
```
Claim "rate limited"             → bounded requests within program limits; inspect headers and effects
Claim "per-account"              → compare owned account contexts only if permitted
Claim "per-IP"                   → characterize documented behavior without bypassing anti-abuse
```
Rate limits are in scope to *characterize*, not to *defeat*. Establish whether
the documented limit behaves as expected. Absence of 429 within a small sample
is inconclusive; report only demonstrated security impact within scope. Do not build an evasion ladder.

### Session management
```
Claim "cookie is httponly"       → read Set-Cookie flags
Claim "cookie is secure"         → read Set-Cookie flags over the real scheme
Claim "session rotates on login" → compare pre- and post-login cookie values
```

### Cache
```
Claim "responses are private"    → read Cache-Control, then test a shared cache
Claim "no caching of user data"  → does a cached response leak across users?
```

### Crypto / secrets handling
```
Claim "data is encrypted at rest"→ is the value actually ciphertext in transit?
Claim "signature is checked"     → flip one byte of the signed value
Claim "random is unpredictable"  → are tokens/IDs guessable or time-seeded?
```

### File access
```
Claim "uploads are private"      → fetch another user's upload URL unauthed
Claim "type restricted"         → upload a non-image, check how it is served
Claim "path is generated"       → does the stored path reflect my input?
```

### Upload filter bypass — the MIME-type trap

This is the single most common false negative in file-upload testing,
and it is not a scanner limitation — it is a **tooling** limitation.

**The trap:** `curl -F "file=@shell.png"` sends
`Content-Type: application/octet-stream` on the part, even though the
file is a valid PNG. The server-side MIME filter reads the part's
`Content-Type` header and rejects it. The upload *looks* like it should
work — the file is a real PNG — but the filter sees octet-stream.

**The fix is manual multipart construction with an explicit
Content-Type on the part:**
```bash
# WRONG: curl -F sends application/octet-stream, filter rejects
curl -s -X POST https://target.com/api/upload -F "file=@shell.png"

# RIGHT: explicit Content-Type on the part boundary
curl -s -X POST https://target.com/api/upload \
  -H "Content-Type: multipart/form-data; boundary=----BOUNDARY" \
  -d $'------BOUNDARY\r\nContent-Disposition: form-data; name="file"; filename="shell.png"\r\nContent-Type: image/png\r\n\r\n<binary>\r\n------BOUNDARY--\r\n'
```

For binary payloads, write the body to a file and use `--data-binary`:

```bash
{
  printf '------BOUNDARY\r\n'
  printf 'Content-Disposition: form-data; name="file"; filename="test.png"\r\n'
  printf 'Content-Type: image/png\r\n\r\n'
  cat test.png
  printf '\r\n------BOUNDARY--\r\n'
} > body.bin
curl -s -X POST https://target.com/api/upload \
  -H "Content-Type: multipart/form-data; boundary=----BOUNDARY" \
  --data-binary @body.bin
```

**Use a known valid image as a positive control.** Vary the part MIME,
filename and actual content independently. Acceptance or rejection alone cannot
identify which validation layer ran. Verify stored and served behavior using
owned fixtures; acceptance of non-image content is not automatically a security
issue. Preserve raw multipart bytes when diagnosing tool serialization.

### Error handling
```
Claim "errors are safe"          → force a 500, look for stack traces / paths
Claim "validation is enforced"   → bypass with type juggling / null / []
```

---

## FALSE-NEGATIVE TRAPS

The failure mode of this skill is declaring "clean" too early. These are the
places real authZ bugs hide:

```
/v1 vs /v2 vs /internal vs /beta      older + internal versions are far weaker
GET vs POST vs PATCH vs DELETE         per-handler enforcement is the norm
body id vs path id vs query id         two sources, one validated
alternate response shapes             ?id=1, {"id":1}, {"ids":[1]}, {"filter":...}
GraphQL                               the same object via a different resolver
bulk/batch endpoints                  one BOLA, N objects
exports, reports, webhooks             async paths that skip controller auth
admin SPA routes                      the API behind /admin may be /api/admin
nested children                       authZ on parent only, not on child ids
soft-deleted / archived objects        different query path, different auth
object count and arity                ?ids=1,2,3 vs three single requests
HEAD/OPTIONS responses                often leak metadata past the real handler
```

Only on disposable owned fixtures, with mutation testing authorized, adapt this
method checklist to the documented interface:

```bash
curl -s -o /dev/null -w "%{http_code} " -X GET    -H "Authorization: Bearer $TOKEN_A" "$P/$B_ID"
curl -s -o /dev/null -w "%{http_code} " -X POST   -H "Authorization: Bearer $TOKEN_A" "$P/$B_ID"
curl -s -o /dev/null -w "%{http_code} " -X PUT    -H "Authorization: Bearer $TOKEN_A" "$P/$B_ID"
curl -s -o /dev/null -w "%{http_code} " -X PATCH  -H "Authorization: Bearer $TOKEN_A" "$P/$B_ID"
curl -s -o /dev/null -w "%{http_code} " -X DELETE -H "Authorization: Bearer $TOKEN_A" "$P/$B_ID"
curl -s -o /dev/null -w "%{http_code}\n" -X OPTIONS "$P/$B_ID"
```

---

## IMPACT BOUNDARY

Verifying a control is complete when the boundary is proven. Verification is
not a licence to keep going deeper.

```
IDOR read proven      → STOP. Do not attempt write, then admin, then deletion.
Tenant cross-read proven → STOP. Do not enumerate how many tenants are open.
Rate limit absent proven  → STOP. Characterize it, do not use it to brute force.
Upload filter broken     → STOP at "non-image stored and served". No shells.
```

Escalate further only when all three hold: the finding is verified, the user
explicitly asks for deeper impact, and the next step is inside the scope file.
Where a longer chain matters, put it in the report as an **analyst note** —
"this same endpoint class also exposes `POST /admin/*` unauthenticated" — and
let the owner decide. The owner pays for the finding they can act on, not for
the one you demonstrated for an audience.

**Verify broadly, escalate narrowly.** Test every control in the ledger. Go
exactly one layer past any single boundary, and only when asked.

---

## BOUNDED COVERAGE IS A RESULT

Report passing tests as bounded observations, never proof that the application
is clean. Separate untested, blocked and inconclusive coverage:

```
## Control Verification — <target>
| Control class | Controls tested | Passed | Failed | Notes |
|---------------|------------------|--------|--------|-------|
| Authentication| 4                | 4      | 0      |       |
| Authorization | 11               | 9      | 2      | order DELETE, tenant header |
| ...
## Untested (and why)
- Webhook signature verification — no way to trigger a real webhook safely
```

An honest "verified 40 controls, 2 broken, 6 untestable" is worth more to an
owner than a page of unverified suspicions, and it tells them exactly where
their remaining risk lives.
