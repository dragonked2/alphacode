---
name: reasoning-cases
description: "Worked causal decisions for authorized hunting: convert clues into falsifiable hypotheses, choose authoritative impact oracles, resolve benign explanations, adapt and preserve reproducible evidence."
---

# Worked hunting decisions

These are reasoning examples over synthetic, controlled applications, not claims
about a real target. Transfer the question and control design, not a memorized
endpoint or exploit. Apply the current engagement policy and use owned fixtures.
Each result changes a specific belief; none establishes universal coverage.

## 1. A second account receives HTTP 200

Observation: account B can request an object created by account A. Explanations
include a public preview, an intended share, an administrative role, a login page,
a cache artifact and an ownership-check failure.

First identify the promised visibility and compare a stable protected field, not
response length. Use a synthetic private canary placed by A. Confirm A can read
it, B's own session is valid, and the object is not shared with B. Compare with
an intentionally public object and a missing synthetic object. Repeat B's request
independently and keep each capture distinct.

If B receives only the public preview, reject that disclosure claim. If a private
canary is reproducibly exposed, the read boundary has evidence. Do not call it a
write or account takeover. Trace export/download/list siblings to learn whether
the same failing check or a separate defect is involved. The offline
`authorization_review` tool can identify missing read evidence but cannot certify
the authenticity or meaning of the captures supplied to it.

## 2. The first request leaks, but the repeat fails

Observation: one response contains the protected canary; a later request is denied
or errors. The denied response cannot count as a second successful exposure.
Preserve the first capture and investigate the differing conditions: cache state,
session expiry, object lifecycle, backend routing or a genuinely intermittent flaw.

Choose one condition to control next. Record every attempt and success. A failed
repeat reduces reproducibility evidence; it does not erase the original observation
or prove the application safe. If the object is simultaneously recorded absent,
reconcile the fixture before promotion. Do not manufacture corroboration by
renaming or reimporting the same exchange.

## 3. Revocation appears ineffective

Observation: after removing B's access, B still sees a document in the browser.
The visible content may be cached locally, legitimately public, subject to a
documented grace period, or freshly served through stale server authorization.

Establish the effective revocation point and the exact capability being revoked.
Request a fresh controlled field after that point using B's existing capability,
then compare a fresh B session and an entitled A request. Separate refresh-token,
access-token, signed-link and browser-cache behavior. Ask which component promises
immediate invalidation. A page already held in memory is not proof of new access.

If a queue is involved, compare authorization at enqueue, execution and result
retrieval. Tag the controlled job and wait for its documented terminal state.
Decide whether policy permits snapshot access or requires authorization at delivery.
An accepted job ID proves neither completion nor unauthorized data retrieval.

## 4. Input causes an interpreter-like error

Observation: a parameter changes a response or produces a database/template error.
Competing explanations include normal validation, a generic exception page, a
different route, caching, backend variance and actual interpreter control.

Trace the reachable input and consumer when source is available. Construct paired
tests whose predictions differ under the suspected semantics but are otherwise
equivalent. Prefer a bounded synthetic consequence with an independently checked
baseline; do not rely on an error substring or one timing outlier. If timing is
the only allowed oracle, use matched controls and repeat measurements within the
budget, preserving noise and failures rather than selecting the slowest response.

For client execution, identify the parsing context and observe actual execution
in the intended browser origin. Reflection in text, a download or another origin
does not satisfy that oracle. Request the class-specific reference for details.

## 5. An external callback arrives

Observation: a unique callback token is observed after submitting a URL. Determine
whether the target server, user's browser, scanner preview or another intermediary
made the interaction. Correlate the token, timestamp and protocol with one bounded
test; use a fresh token for a control that should not trigger the consumer.

A DNS interaction establishes less than a completed HTTP fetch. A completed fetch
establishes less than a sensitive read or useful internal capability. Preserve
those distinctions. Explore a further boundary only if it is permitted and needed
for the requested consequence; avoid treating a callback as automatic cloud
credential access. A source guard may apply to an initial URL while a later
redirect/consumer uses another representation: inspect that actual handoff.

## 6. Two operations both return success

Observation: two requests redeem a synthetic entitlement and both return 2xx.
The responses may acknowledge duplicate attempts while applying only one effect.
Record initial state, operation IDs and the authoritative settled result. Compare
the serial baseline with a minimal, permitted concurrency experiment and verify
that the same invariant, actor, object and preconditions apply.

Only a violated final conservation/idempotency rule supports the consequence.
Do not infer financial loss from a UI counter, a queue acknowledgement or an
unsettled provider event. Use disposable value and restore it as the program
allows. After an interruption inspect actual state before replaying the operation.
If the policy prohibits concurrency or an oracle is unavailable, retain the
unanswered hypothesis and test an independent eligible workflow.

## 7. Two moderate findings seem to form a critical chain

Observation: one issue yields a link and another accepts a link-like value.
Write the first issue's exact demonstrated capability and the second issue's
required input. Compare principal, tenant, origin, encoding, expiry, audience,
permissions and deployment configuration. Similar strings do not prove compatible
capabilities. A link usable only by an administrator cannot be silently substituted
into an unauthenticated chain.

Model each missing edge as its own hypothesis with controls and terminal evidence.
A negative edge leaves the independent original findings intact. Describe the
chain as conditional until the permitted end consequence is demonstrated. Stop
at sufficient evidence; keep investigating unrelated roots within the budget.

## 8. Source shows a missing check

Observation: an export handler lacks the guard present in a detail handler.
Trace callers, middleware, feature gates, account context and downstream storage
checks. Establish that the handler is reachable in the tested deployment and
that another layer does not enforce the same invariant. Use controlled requests
to test the specific omission. Dead code or a different deployed revision changes
the claim, not just its confidence score.

For a repair, add a regression that fails on the vulnerable behavior and passes
after the correction while preserving entitled functionality. A local fix test
does not establish a deployed retest. Re-read the actual deployed state before
marking remediation verified. Published patches are useful for learning mechanisms;
rediscovering a known fix is not a blind novel-discovery benchmark.

## 9. Choose the next test without tunnel vision

Keep one compact row per candidate: evidence locator, invariant, competing
explanations, missing prerequisite, predicted observation, cost/risk, outcome and
reopen condition. Rank the next experiment by the decision it can change. Count
repeated observations and related scanner alerts once when assessing evidentiary
weight; evaluate shared root causes separately from distinct affected surfaces.

If all active candidates depend on an expired session, repair that prerequisite
rather than cycling payloads. If one requires an unavailable role, mark that edge
blocked and continue independent public/owned coverage. If an experiment changes
no belief and resolves no coverage item, redesign it before spending more requests.
Checkpoint decisive artifacts so a resumed agent can continue from evidence.

Measure improvement on versioned tasks with independent adjudication, repaired
variants and false-positive traps. Preserve failed runs, costs and time-to-proof.
Prompt instructions and unit tests demonstrate integration and specified behavior;
they do not prove superiority to elite hunters or a future leaderboard position.
