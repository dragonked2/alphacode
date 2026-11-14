---
name: hunt-idor
description: IDOR/BOLA and tenant isolation testing with controlled identities, ownership controls, reproducible protected-content evidence and root-cause deduplication.
---

# Investigate object authorization

An identifier or HTTP 200 is a lead. Unauthorized access requires an established
policy, a controlled resource, a valid non-entitled identity and observed impact.
Use the engagement's scope, request budget and permitted actions throughout.

## Establish the policy and fixtures

1. Read the supplied scope and source when available. Identify the route or
   resolver, its authorization check and the resource's intended visibility.
2. Use two controlled accounts. Record their roles, tenants and session validity.
   Create disposable objects containing a synthetic owner-only canary. Establish
   ownership through fixture creation or an entitled control, never the ID shape.
3. Record the public/shared baseline. A public preview, deliberately shared object
   or administrative permission is not an object-authorization bypass.

## Run the differential

Capture these requests with exchange IDs and report-safe artifact references:

| Check | Required observation |
| --- | --- |
| Session validity | Each account can reach its own expected authenticated resource |
| Entitled control | Owner can read the test object and its protected canary |
| Non-entitled test | Other account requests that same object |
| Repeat | A distinct request independently reproduces the protected-content exposure |
| Negative control | The same operation is denied on a comparable non-entitled resource |
| Missing object | A known absent synthetic object distinguishes absence from denial |

Use `httpflow` sessions for isolated actors and `compare_actors` for the owner
and missing-object read controls. That action alone is not the full proof: capture
the independent repeat, session-validity and policy evidence separately. Stop and
record rate limits, stale sessions, errors and unavailable controls accurately.
Do not count those responses as successful repetitions.

Promote `authorization_review` through `tool_help` when its schema is not visible.
Use it to review recorded read observations offline. Supply
declared identities/resources, `confirmed_absent`, and observations with actual
exchange IDs, outcomes and report-safe protected-data labels. Follow `next_offset`
to inspect all matrix pairs. It flags missing proof and fixture contradictions;
it does not authenticate captures or independently validate a vulnerability.
Its evidence-supported pair count is not a distinct-root-cause count.

## Explore related surfaces with a hypothesis

Trace the same object through REST, GraphQL fields, exports, downloads and API
versions where the source or observed navigation indicates a reachable surface.
Test horizontal, vertical and tenant boundaries separately. Keep each untested
surface visible. Prioritize paths with differing authorization middleware rather
than repeating a scan of already equivalent endpoints.

For an authorized write test, use a disposable object, record before/after state
from the entitled account, and verify restoration. A success response or returned
object does not prove a write took effect. Avoid test-account deletion or changes
to the authentication configuration needed by the assessment. Treat interrupted
writes as indeterminate and reconcile actual state before retrying.

## Resolve and report

Reject public/shared access, empty bodies, expired sessions, cache artifacts and
differences limited to volatile metadata. Reconcile any claim that a resource is
both absent and returned. If a control is unavailable, record the missing proof
and keep the candidate inconclusive.

For a confirmed issue, preserve minimal synthetic proof, route/resolver source
references, preconditions, actual protected bytes or state change, reproduction
steps, remediation and remaining uncertainty. Deduplicate by the failed invariant,
boundary, operation and shared code path; retain affected endpoints as symptoms.
Assign severity from the demonstrated consequence and required privileges. Do
not promote an IDOR to Critical through hypothetical enumeration, PII or takeover.
Retest the fix on a fresh deployment and repeat the negative controls.
