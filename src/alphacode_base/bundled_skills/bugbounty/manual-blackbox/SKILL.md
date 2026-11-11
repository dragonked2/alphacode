---
name: manual-blackbox
description: Manual-only blackbox methodology — reconstructing an application from the outside with no source access, modelling its business logic and state machines, and the false-positive self-attack protocol that keeps submitted findings at a zero-FP rate. Use when scanners have come back clean, when the target is behind a WAF, or when the differentiator between an accepted and a rejected report is rigor.
---

# MANUAL BLACKBOX — RECONSTRUCT, THEN ATTACK THE MODEL

**Core principle:** with no source, you cannot read the answer — you must
**rebuild the question**. A scanner sends payloads at a URL. A manual hunter
builds a model of what the application *is*, then attacks the places where the
model and the implementation disagree. Every high-severity manual finding is
really a statement of the form:

> "I believe this system works this way. The implementation disagrees."

**This is the skill that decides reputation.** Anyone can run nuclei. The
hunter whose last 20 reports were all accepted is the one who could not explain
a single false positive.

---

## SETUP BY HYPOTHESIS

Use authorized test accounts, controlled data, and a hypothesis ledger. Two
separate identities are needed for a cross-user comparison; separate tenants or
roles are needed when testing those specific boundaries. Record their actual
membership before interpreting a result. A missing second identity blocks that
comparison, not independent single-account, source, workflow or session tests.
Use elevated roles only when available under the program rules.

---

## PHASE 1 — BUILD THE MODEL BEFORE YOU ATTACK IT

The most common beginner failure is attacking endpoints in enumeration order.
You get shallow findings, or nothing. Spend the time up front.

### 1a. Reconstruct the product from its own words
```
security.txt          contact, policy URL, safe-harbor language
Terms / Privacy       data model, what a "user" is, what a "workspace" is
Pricing page           tier names = role names = likely privilege levels
Docs / API docs        the object model, if they published one
Marketing copy         the features that matter = the features with money
Sitemap / nav          the feature surface
```

Pricing tiers are the most useful document on the site for an authz hunter.
`Free / Pro / Team / Enterprise` tells you the privilege lattice you need to
test, and the marketing site often has a **client-side feature gate** keyed to
those tiers — which is a claim about authorization that the API may not honor.

### 1b. Map the state machine
Most business logic bugs live in a transition, not a state. Write the
transitions down:

```
DRAFT ──submit──▶ PENDING ──approve──▶ APPROVED ──invoice──▶ PAID
  ▲                  │                       │                  │
  └──edit────────────┘                       │                  │
                                             ▼                  ▼
                                         REJECTED           REFUNDED
```

Then ask, for each edge: **who can traverse it, and does the server check
that the actor is allowed to be in the source state?** Transitions are where
back-buttons, replay, direct URL navigation, and API calls that skip the UI all
converge.

### 1c. Enumerate the roles
```
signup flow      what role does a new account get, and is it chosen client-side?
invite flow      can you invite yourself a higher-privileged account?
role change      is there a UI or API to change your own role?
demo/trial       does the program offer elevated roles? use them if so
```

### 1d. Write the ledger before the first payload
```
| # | Hypothesis (the model)          | Test (one request) | Kill condition |
|---|--------------------------------|--------------------|----------------|
| 1 | Orders are owner-scoped        | A reads B's order  | 403/404         |
| 2 | Refund is owner-scoped         | A refunds B's      | 403/404         |
| 3 | Order is immutable after PAID  | A PATCHes B's PAID | 405/422         |
| 4 | Promotion is single-use        | B reuses A's code | non-2xx         |
```

A hypothesis written *before* the test is the entire difference between
findings and noise. After the test, you are looking for what you hoped to see.

---

## PHASE 2 — WHAT MANUAL BEATS THE SCANNER ON

Automation is unbeatable at volume and blind to all of the following.

```
SEQUENCING        multi-step flows needing intermediate state from the app's
                  own response: extract a nonce, replay a signed URL, chain a
                  302 through a login callback

BUSINESS LOGIC    "an order may be refunded once" — no payload triggers this,
                  because the payload is a legitimate second order

ROLE LATTICE      Free→Pro→Admin client gates that the API ignores

STATE             direct navigation to a state the UI would not offer:
                  /orders/123/refund when the order is PAID, not PENDING

COMBINED          one weak control is not a bug; the *pair* is.
                  read-any-order + no CSRF on export = account-level disclosure

TIMING            rate limits keyed to the wrong dimension, discount abuse,
                  referral stacking — only visible with a deliberate model

SUBSCRIPTION      entitlements enforced in the UI, absent in the API
```

### Concrete manual techniques
```
Two-account diff      do A's and B's responses differ in field *set*, not just values?
                      a field present for B and not A is a leak of shape
Delete-as-other       DELETE on B's object with A's token (proof, no confirmation)
Reorder a workflow    call step 3 while the object is in a state step 3 rejects
Duplicate with perm   same action twice, second should fail — coupon, referral, email
Negative numbers      quantity -1, price -100, balance -1, offset -1
                    type confusion where a string meets a number field
Null / omitted        remove a field the UI always sends
Client gate           feature hidden in the SPA, endpoint still live?
Replay a webhook     re-deliver a signed payload with the same signature
Offline arithmetic   is the price/credit/limit computed client-side?
Header trust          X-Forwarded-Host, X-Original-URL, X-Rewrite-URL,
                    X-Forwarded-Prefix, Forwarded
Confused deputy      a low-priv action performed through a high-priv feature
```

---

## PHASE 3 — THE FALSE-POSITIVE SELF-ATTACK

**This is the phase that separates accepted reports from rejected ones.** Before
any finding leaves your hands, try to destroy it. Assume you are the triager
who wants to close it.

### The seven questions
```
1. IS THE RESPONSE MINE?      Did I get 200 + a JSON body, or 200 + the login
                              page, an SPA shell, a soft-404, or a WAF page?
                              This one causes more false positives than any
                              other. Verify the body, not the status code.

2. DID I CROSS A BOUNDARY?    A response difference is not a security boundary.
                              Different JSON key ORDER or an extra analytics
                              field is not a leak. What specifically was I not
                              supposed to see, and is it in the scope file?

3. IS IT THE FEATURE?         "I read another user's profile" — was that the
                              documented purpose of the endpoint? Public
                              profiles are public. Re-read the docs.

4. IS IT A CACHE OR A RACE?   Re-request three times from a clean session.
                              A 200 that only appears twice is a race, not authz.

5. DID I CREATE THE CONDITION?
                              Mass assignment, IDOR, and race findings are
                              invalid if the object was created by the test
                              itself. "I uploaded A to A's bucket, then read
                              it" proves nothing about isolation.

6. DOES IT SURVIVE A CLEAN REPLAY?
                              Fresh session, fresh account, curl only, no browser
                              state, no cookies you did not intend. If it only
                              reproduces with your devtools session, it is your
                              session, not their bug.

7. WOULD THE OWNER'S OWN TEAM FIX IT?
                              "No, that is intended" is the rejection reason.
                              If the honest fix is "nothing", drop it.
```

### The triager simulation
Write the rejection reason *before* you submit. If you can write a convincing
one, the finding is not ready. This is uncomfortable and it works.

```
> "This is intended — the endpoint returns public profile data by design."
> "This only reproduces because your test uploaded the file."
> "The 200 is your own session's cached response."
> "This requires admin, so it is not a privilege escalation."
> "No security boundary is crossed; the data is already public."
```

If any of these survives, either fix the test or drop the finding. Reporting a
finding you know is invalid costs more than finding nothing.

### Negative-result discipline
Tested-and-clean is a deliverable, and stating it protects you when the owner
later finds the thing you missed. Write it as:

```
## Confirmed Not Vulnerable
- Order write authZ — A PATCH/DELETE on B's orders, 6 methods, all 403/404
- Coupon reuse — same code twice, 2nd returns 409
- Tenant isolation — A's token against B's tenant id, 403

## Blockers
- No second-org account; org boundary untestable

## Not Tested
- Refund flow (moves money, outside the scope file's rules)
```

---

## PHASE 4 — DISCIPLINE

```
PACE
  Write the hypothesis before the payload, always.
  One test per hypothesis. Three identical negative results → kill it, move on.
  Prefer a clean negative to an unfinished rabbit hole.

RECORDING
  Every request that produced something unexpected goes in the notes verbatim,
  at the time. Reconstructing a request from memory is how duplicates and
  wrong-impact reports happen.

VALUE
  A Medium you can prove beats a Critical you can only describe. Programs pay
  per accepted unique bug. Breadth of *findings* is the reliable strategy;
  depth of *claims* is what gets reports closed.

SAFETY
  The first proven boundary is the end of the test. Not because depth is
  forbidden to think about, but because the owner never consented to it, and
  the report is stronger for the restraint.
```

---

## PHASE 5 — DESTRUCTIVE TESTING WITH RESTORATION

Write access is where the severity lives, and write access testing is
inherently destructive. This section governs every write test.

**The rule: every destructive action must be reversible, or it must not
happen.** A finding that costs the owner unrecoverable data is not a
finding — it is an incident.

### The restoration protocol

Before any write test, for each endpoint class:

```
1. CAN I CREATE IT?      Is there a create endpoint for this object type?
2. WILL THE ID BE GUESSABLE?  Sequential, UUID, or predictable?
3. CAN I DELETE IT?      Does a delete endpoint exist and does my role own it?
4. IF I CANNOT RESTORE, DO NOT TOUCH IT.
```

Record the answers in the assessment notes. If any answer is "no",
the object type is off-limits for destructive testing.

### The test-artifact pattern

Never test writes against production data. Always:

1. **Create a test artifact** with a name that identifies it as yours
   (e.g. `alphacode-test-<timestamp>`).
2. **Perform the write test on the artifact.**
3. **Delete the artifact immediately.**
4. **Verify deletion by re-querying the API** — do not trust the delete
   response. A 204 from the delete endpoint does not mean the object is
   gone. Query the list endpoint and the object endpoint to confirm.

### What is never acceptable

```
X Deleting a production object to prove DELETE works
X Modifying a production record's content
X Creating objects that persist beyond the assessment
X Bulk operations (delete all, mass-update) — one object only
X Testing delete on an object type with no create endpoint
```

### Unrecoverable damage — acknowledge it

Sometimes the create endpoint exists but the delete endpoint does not,
or the object is deleted but the ID is unrecoverable (no
create-subcategory endpoint, no create-banner endpoint). **In that
case:**

1. Stop immediately — do not create or delete anything else.
2. Record exactly what was affected and whether it is recoverable.
3. Report it honestly: "During testing, sub-category 1 was deleted.
   The create endpoint for sub-categories was not available, so it could
   not be restored. Category 3 was recreated as id 15. Banner 1 is
   unrecoverable."

An honest damage report is worth more than a clean report that hid
the damage. The owner needs to know what happened.

### The compound risk of unauthenticated writes + CORS

When an API accepts writes without authentication AND returns
`Access-Control-Allow-Origin: *`, the finding is no longer "the API is
broken" — it is "any website on the internet can make authenticated
admin calls against this API." Note this in the report as a compound
risk. It is the difference between a Critical that one team fixes and
a Critical that gets a security advisory.

---

## THE SHARPEST VERSION OF THIS SKILL

```
Read the product until you can predict its behavior better than its developers
can, then file the specific prediction it violates. Write every hypothesis down
before you test it. Try to disprove each finding before you show anyone. Stop
at the first proven boundary and write the rest as an analyst note.
```

That produces few reports and a high acceptance rate. It is also, quietly,
the only version of "maximum impact" that keeps working over a career.
