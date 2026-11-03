# Bounty evidence and workflow hardening

## Changes

The proof gate now matches all retained exchange fields (method case-insensitively,
URL, request body, status, response headers/body and source tool). Different
responses to the same method/URL/status are retained separately by the
orchestrator. A baseline must cite the attached treatment and, when session
history is available, a control present in that history. Materiality and its
report diagnostics are recalculated from exchanges, ignoring caller-supplied
serialized difference summaries.

Existing successful finding and claim-checker fixtures now include both observed
exchanges. Regression tests cover substituted response bodies, missing controls,
unrelated treatments and forged difference summaries.

The bundled bounty entrypoint routes to `real-world-validation`, covering program
contracts, experiment selection, modern trust boundaries, evidence controls,
minimal proof, cleanup and reporting. Control verification no longer promotes
status-only IDOR, soft-404, upload acceptance or duplicate-response observations
into confirmed vulnerabilities. Framework recipes explicitly require applicable
prerequisites and validation.

## Limits

These changes harden evidence consistency, not semantic vulnerability detection.
Status/body-size heuristics still cannot establish impact or distinguish all
benign differences. Request headers, actor identities, timestamps and bodies
beyond the retention cap are not represented in `HttpExchange`; keep original
artifacts and account context separately. Exact retained-field matching can
reject manually normalized evidence; cite the captured exchange and explain
normalization outside it. Ingested history must come from trusted tool results.
`verify()` without history remains a weaker structural check.

No live program was tested and no improvement in bounty acceptance, recall or
false-positive rate was measured. Do not claim guaranteed coverage or zero false
positives. Evaluate behavioral improvements against held-out authorized fixtures
under equal models, budgets and tool versions, including blocked and failed runs.

## Behavioral review cases

| Observation | Required decision |
|---|---|
| A gets 200 for B's publicly shared item | Intended behavior unless another boundary is violated |
| A gets B's private canary; valid owner and absent-object controls work | Supported authorization candidate; minimal reproduction and policy review |
| Unknown routes all return an identical 200 SPA | Inconclusive route existence, no IDOR or enumeration claim |
| A different response body reuses an observed URL/status | Reject evidence substitution |
| Control was never captured, or treatment refers to another request | Reject baseline provenance |
| Serialized delta claims a difference between identical exchanges | Reject signal; recompute from exchanges |
| Duplicate coupon requests both return 200; settled balance unchanged | No demonstrated invariant violation |
| Unique callback resolves in DNS, no HTTP request observed | DNS interaction only; no claim of protected HTTP access |
| Removed tenant member still receives a private owned realtime event | Investigate documented revocation semantics and reproduce bounded exposure |
| Scanner fails or tokens expire | Blocked coverage; diagnose before interpreting target behavior |

The PowerShell installer completion section now places `alphacode` first, then
login and doctor as secondary commands, with a compact colored heading and a
short terminal restart hint. The existing installer behavior tests exercise
PATH and download safeguards without performing an install.

## Transcript-driven continuation correction

The supplied exchange switched a website question into product/CVE research,
refused site-level assessment based on shared hosting, promoted missing headers
without demonstrated impact, generalized a few 404s into no secret exposure,
and asked for another "go" after an already-authorized full assessment.

The system prompt and research/runbook/orchestration guidance now preserve the
requested target, accept stated ownership for bounded site assessment, track
coverage outcomes and continue applicable work without phase-by-phase consent.
Stopping sufficient proof applies to that hypothesis, not the remaining hunt.
The shared hosting and third-party boundaries remain explicit. These are prompt
and workflow corrections; their presence is not evidence of measured model
compliance. The supplied exchange should be replayed in a fresh bounded fixture
session to evaluate response behavior without contacting the live site.
