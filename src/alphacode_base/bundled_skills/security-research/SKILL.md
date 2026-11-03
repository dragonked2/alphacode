---
name: security-research
description: "Evidence-driven workflow for authorized vulnerability research and bug bounty assessments. Use to plan, resume, prioritize, disprove, and document a hunt."
---

# Security research

Optimize for reproducible, in-scope security impact. A clean negative result is useful. Zero false positives is an objective, never a guarantee or a claimed measurement without a labeled evaluation set.

## Start or resume

1. Read the supplied scope, program rules, accounts, and previous evidence. Preserve existing authorization; resolve only material gaps before dependent actions. Discovery does not grant scope.
2. Record a compact engagement card: target/revision, allowed assets and methods, exclusions, actor/tenant roles, rate budget, objective, and evidence directory. Keep credentials out of notes and reports.
3. Model the system: entry points, sensitive objects, roles, trust boundaries, state transitions, and expected invariants. Prioritize reachable boundaries with useful impact over large payload lists.
4. Select one falsifiable hypothesis and the cheapest discriminating test. State what would refute it before testing. Load only the class reference needed for that test.
5. Capture a normal baseline, change one variable, observe the result, run a negative control, and reproduce the distinction. Keep original request/response artifacts, actor context, timestamps, and the actual target revision.
6. Track validity, impact, scope, eligibility, and severity separately. Load `finding-validation` before calling a result confirmed or drafting a security report. Numeric confidence cannot replace evidence or establish program eligibility.
7. Stop testing that hypothesis once sufficient proof establishes the claim or it is refuted; continue the next applicable coverage item. End the assessment only when planned coverage is resolved, all remaining work is genuinely blocked, the agreed budget is reached, or the user stops it. Skip and record out-of-scope actions without abandoning independent in-scope work. Pivot when new evidence changes the hypothesis; do not repeat an unchanged failed test.

## Carry the requested assessment through

For a full assessment, keep a coverage ledger alongside the hypothesis ledger:
`surface | applicable tests | status | evidence/result | next action or blocker`.
Use pending, tested, not-applicable (with reason), inconclusive, and blocked.
Reconnaissance resolves discovery items; it does not resolve DOM/source review,
authorization, validation or other applicable tests. Continue authorized work
without asking for another "go" at each phase. A proven finding ends its proof,
not the whole assessment. Report interim observations as progress, then continue.

For an owned static website, inspect same-origin HTML and scripts, trace URL/hash,
storage and message inputs to relevant sinks, identify API and third-party
boundaries, and use bounded route checks selected from actual assets. Rule out
soft-404s and client-side fallback. Test only reachable mechanisms. Third-party
hosting is a boundary, not a reason to refuse site-level testing or scan shared
infrastructure. Missing headers and public CORS belong in hardening observations
unless an actual security consequence is demonstrated. Public CVE searches never
substitute for reviewing the requested deployed surface.

For long or broad investigations read bugbounty's `investigation-control`
reference. Reconcile clues with no disposition, failed prerequisites, partial
runs and interrupted operations before closing coverage. Use `chain-reasoning`
for compatible, explicitly evidenced multi-step consequences.

## Evidence contract

Maintain one concise ledger row per hypothesis:

`id | invariant | actor/resource | baseline | changed input | observation/artifact | negative control | status | uncertainty | next discriminating test`

Use `UNTESTED`, `HYPOTHESIS`, `NEEDS_EVIDENCE`, `REPRODUCED`, `DISPROVEN`, or `BLOCKED`; reserve report-ready for findings that pass the validation and current policy gates. Absence of evidence is not evidence of absence. A scanner alert, response code, reflection, version match, generic PoC, or model assertion is a lead.

Before promotion, try the strongest plausible alternative explanation: public by design, same-owner access, stale session, redirect/login body, cache contamination, synthetic response, scanner artifact, unreachable code, compensating control, or impossible prerequisites. Record the outcome rather than claiming all possibilities were eliminated.

## Choose guidance by the question

Load other skills with `skill_manage` (`action=load`, `name=...`). Read a bundled reference with `action=read`, `name=bugbounty`, and `reference=<reference key>`.

| Question | Guidance | Required distinction |
|---|---|---|
| What is reachable and permitted? | `recon-ops`; bugbounty `scope`, `recon`, `recon-js` | Discovered asset versus authorized asset |
| Who can read or change which object? | `hunt-idor`, `hunt-api`, `hunt-graphql` | Two controlled users/tenants, own/other/nonexistent object, actual data or side effect |
| Can identity or session state be confused? | `hunt-auth-bypass`, `hunt-oauth`, `hunt-jwt`, `hunt-otp` | Token claims versus server-enforced identity and privileges |
| Does a multi-step workflow preserve its invariant? | `hunt-business-logic`, `hunt-race` | State before/after, idempotency, duplicates, ordering, actual durable result |
| Is input interpreted in another context? | `hunt-xss`, `hunt-sqli`, `hunt-ssti`, `hunt-xxe` | Reflection or error versus verified execution/data consequence |
| Can a server fetch or open an unintended resource? | `hunt-ssrf`, `hunt-path-traversal`, `hunt-storage` | Controlled callback/read versus timing, DNS, or status inference |
| Do parsers, proxies, and caches disagree? | `hunt-desync`, `hunt-cache-poisoning`, `hunt-crlf`, `hunt-cspt` | Per-user artifact versus repeatable boundary effect; explicit permission for shared effects |
| Do browser protections enforce the intended boundary? | `hunt-cors`, `hunt-csrf`, `hunt-websocket` | Browser-realistic credentials, origin, state change, and readable response |
| Is a leaked value usable and relevant? | `hunt-apikey-leak`, `cloud-config` | Identifier/test value versus scoped permission; use minimal permitted validation |
| Is a chain stronger than its components? | `chain-reasoning` | Every edge proven with compatible prerequisites and actor context |
| Can another reviewer reproduce the claim? | `finding-validation`; bugbounty `report`, `severity-engine` | Demonstrated impact versus hypothetical reach and payout |

For white-box work trace entry point, transformations, guards, sink, and callers at the tested revision. Use a local harness or regression test to distinguish vulnerable and corrected behavior. A standalone mechanism demonstration does not verify target reachability.

## Efficient research

- Search the code or endpoint inventory first; request bounded output and read targeted ranges. Use deterministic scripts for repeatable comparisons. Save full raw evidence once and reference paths or hashes.
- Reuse a known baseline only while actor, session, target version, and state remain comparable. Rebaseline when these change. Avoid comparing dynamic timestamps/nonces as if they were security differences.
- Test one trust-boundary dimension at a time: actor, object owner, tenant, role, method, route/version, content type, parser, or workflow state. Choose variations from architecture and observations instead of blind permutations.
- Keep a compact coverage matrix including negative results and untested areas. Deduplicate by root cause and boundary rather than URL count. Stop redundant proof after the acceptance criterion is met.
- Share scope and artifact references instead of copying raw transcripts into every model turn or worker. Load one relevant reference at a time; summarize decisions and unresolved facts before compaction, preserving evidence locators and prerequisites.
- Explain the invariant and why the observation supports or refutes it. Teach the applicable reasoning as the investigation progresses; avoid exhaustive tutorials unrelated to the current boundary.

## Deeper reference

Read this skill's `handbook` reference for detailed authorization, credential/session handling, authenticated testing, source-first analysis, chaining, evidence ledgers, and research-memory procedures when those mechanics are needed. It preserves the extended manual without loading it on every hunt.

Methodology reference: [OWASP Web Security Testing Guide](https://owasp.org/www-project-web-security-testing-guide/). Verify current target policy and technology-specific behavior during the engagement; a generic checklist cannot establish either.

## Do not mistake a smoke test for an assessment

A few `/me` requests, UUID-shaped paths and a bundle keyword search are a reconnaissance checkpoint, not a completed IDOR/BAC/RCE hunt. Invented paths that return a SPA document do not test ownership. A missing second controlled identity blocks cross-account comparisons; it does not refute IDOR, and it does not block independent authorized lifecycle, session, source/DOM, API-version or workflow questions. Inspect observed functionality and requests before declaring a class inapplicable. Missing upload/SSRF strings in three bundles means only that those searches found no candidate; never convert that into “no RCE surface exists.” Keep other actors and server-only/dynamic behavior explicitly untested.

Do not silently replace the program's rules with a self-invented read-only assessment. Resolve official scope and permitted actions first; within them, choose minimal reversible owned-data experiments appropriate to the user's request. Do not infer permission for unrelated hosts or real-user resources. Keep cookies/tokens out of conversational output; select only target-scoped credentials needed for an experiment and redact artifacts.

Before closing, reconcile every observed feature and promising clue with the ledger. Continue the next independent, permitted experiment while one prerequisite is blocked. A final negative report must identify exact checks, artifacts, untested cells and actual budget/blocking conditions; say “no issue observed in these checks,” never “all boundaries verified” or “class refuted” without corresponding coverage. A failed skill load is not proof its contents exist in a different skill.

A confidence follow-up must produce a new discriminating experiment, inspect previously unreviewed evidence, or correct a task's status/scope. Repeating the same report, increasing percentages, and re-grepping identical artifacts is not validation. Keep blocked work blocked, and checkpoint an actionable next step rather than certifying an incomplete campaign as complete. Do not promise findings or manufacture endless equivalent tests.
