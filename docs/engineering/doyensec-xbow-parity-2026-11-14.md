# Doyensec comparison and AlphaCode capability work

## Objective and current result

The requested objective is XBOW capability parity, better independently validated
discoveries, fivefold speed and root-cause improvements, and ultimately a leading
HackerOne ranking. This change does not establish any of those competitive outcomes.
It improves authorization evidence review and makes that engine callable by the
agent. Live discovery, program acceptance and leaderboard results remain unmeasured.

## Report reviewed

Source: [Doyensec, Comparing AI Application Security Testing Platforms: Aikido vs.
XBOW](https://doyensec.com/resources/ComparingAIApplicationSecurityTestingPlatforms_Doyensec.pdf).
The user-supplied HubSpot copy was downloaded and read (52 PDF pages; SHA-256
`a2c44cdb4049dbe66389feb51907be0247a4c3912916ebb9ad3a7dff7fde894f`).
Page references below are physical PDF pages, including the cover.

The study was sponsored by Aikido; Doyensec states that collection and analysis
were separated and that the sponsor did not influence the results. It compared
two applications with source access, authenticated testing and $4,000 tiers.
Findings received manual validation and peer review. The study explicitly did not
establish complete ground truth or recall (pp. 6-11, 25-29).

| Application | Tested image | Aikido TP / FP | XBOW TP / FP |
| --- | --- | --- | --- |
| Fider | `getfider/fider:v0.33.0` | 17 / 2 | 24 / 1 |
| Photoview | `photoview/photoview:2.4.0` | 32 / 0 | 7 / 0 |

These are reported findings, not consistently deduplicated root causes (pp. 31-46).
Aikido took approximately 8h40m for Fider and less than eight hours for Photoview.
XBOW's Fider assessment involved interruptions, restarts and a later report;
Photoview has start/completion dates rather than precise elapsed telemetry
(pp. 15-24). Calendar/report-delivery delays must not be relabeled active execution.

Fivefold speed against the approximate Fider Aikido time suggests a development
budget around 104 minutes. The Photoview duration is an upper bound: completing
within 96 minutes does **not** prove fivefold speed against an unknown duration
below eight hours. Exact matched measurements are required for both platforms.
Multiplying 49 reported findings by five would produce 245, but that is **not** a
root-cause target supported by the study. First adjudicate distinct roots for all
systems, then compare on the same reset targets with matched permissions/budgets.

The most actionable observations are context-dependent severity errors, incomplete
or unusable reproduction steps, duplicate symptoms, account/authentication damage,
and target availability problems. The report's two targets and one configuration
per platform do not establish broad platform rankings (pp. 28-48).

## Changes in this working tree

- The authorization analyzer counts only distinct returned exchanges containing a
  nonblank protected-data marker as corroboration. Denied, failed, empty and
  unobserved attempts cannot complete a leak's repeat requirement.
- A negative control must concern the same operation. A denied delete cannot
  corroborate an unauthorized read.
- Contradictory recorded absence blocks promotion until the fixture is reconciled.
- Full read-matrix analysis builds a run-local identity/resource/observation index
  rather than rescanning the entire observation inventory for every pair. There
  is no cross-run cache of potentially stale evidence. Scalar and indexed results
  are checked for exact equality, including ordering and missing-proof details.
- The registered `authorization_review` tool exposes this review to the agent.
  It is offline, bounded to 32 identities, 256 resources, 10,000 observations and
  2 MiB input, and paginates 16 assessments per response. Pair counts are explicitly
  distinct from discoveries; no root-cause count is invented. It does not validate
  capture authenticity, send requests, prove writes or provide independent review.
- The IDOR workflow now specifies session, owner, public/shared, negative and
  repeated-access controls, source-guided exploration and demonstrated severity.
  The orchestration workflow accurately describes scanner candidates and coverage.

The new tool consumes the serialized `AccessCase` schema advertised in its tool
definition. Resource references use `kind:id`; markers should be report-safe labels
for controlled synthetic content. Keep original captures outside tool arguments.

## Capability acceptance work still required

Current reference: XBOW's public [results guidance](https://docs.xbow.com/console/guidance/interpreting-results/),
[artifact support](https://docs.xbow.com/console/reference/artifacts/),
[reports](https://docs.xbow.com/console/reference/reports/) and
[safety guidance](https://docs.xbow.com/console/guidance/testing-safely/).
Those are vendor descriptions, not independently reproduced product results.
Do not treat limitations in the older study as current XBOW limitations.

| Capability | AlphaCode evidence and remaining acceptance condition |
| --- | --- |
| Autonomous discovery and adaptation | Scanner execution and durable lead records exist; prove a complete source/HTTP/browser investigation loop on reset applications, including failed strategies and blockers. |
| Authenticated web/API testing | HTTP/browser tools exist; test identity isolation, session refresh, horizontal/vertical/tenant controls and replay artifacts end to end. This change exposes offline read evidence review only. |
| Source-assisted prioritization | Search and prioritization components exist; measure how source context changes investigation selection and validated discovery yield. |
| Injection, SSRF, XSS and business logic | Tool/skill presence is insufficient. Add vulnerable/repaired real-app cases with independent consequence oracles, not keyword matches. |
| Objective proof and impact | Proof and severity modules exist; authenticate retained exchanges, reproduce findings independently, deduplicate roots and record adjudication disagreement. |
| Bounded parallel testing | Phase-level concurrency exists. Verify shared request/resource limits and cancellation across all tools; increasing concurrency alone can damage availability. |
| Reports and remediation | Run reports and finding types exist; exercise full evidence-to-export consistency and usable reproduction scripts. |
| Fix retests | Require a new observation tied to the deployed fixed revision, including repaired and unaffected controls. A local patch passing tests is insufficient. |
| Operational recovery | Verify that account/auth configuration survives testing, interrupted writes are reconciled, and processes are cleaned up. |
| HackerOne acceptance | Requires an account, explicitly authorized program scope/rules, eligible findings, submission review and accepted outcomes over time. No reports were submitted by this change. |

## Verification and measurements

Use the MSVC environment and external `CARGO_TARGET_DIR` specified in `AGENTS.md`.
Run the repository checks and the affected authorization/tool tests. The ignored
timing experiment is opt-in:

```text
cargo test --lib matrix_timing_experiment -- --ignored --nocapture
```

It compares seven alternating scalar/indexed trials over 4,000 matrix pairs and
8,000 observations, checks exact assessment equality and prints median and maximum
durations. It measures in-process evidence analysis, not target scanning, model
latency, independent vulnerability discovery, competitor performance or ranking.
The test intentionally has no noisy wall-clock assertion. Record the build profile
and machine when interpreting its result.

For a competitive run, retain target image digests/source revisions, reset state,
accounts/roles, model and agent revisions, budgets, exact start/end/proof times,
costs, request logs, failures and independent adjudications. Use multiple trials
and report spread. Keep known published issues separate from held-out discovery.
No Docker executable or matched competitor run was available in this session.
