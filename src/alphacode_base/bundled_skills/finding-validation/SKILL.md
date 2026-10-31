---
name: finding-validation
description: "Mandatory finding-validation and bounty-eligibility gates: finding lifecycle states, source, reproduction, impact, eligibility, severity, and submission-readiness gates, triager simulation, duplicate radar, severity calibration, report-drafting discipline, and the security report template. Load before calling any finding confirmed or submission-ready, and before writing a security report."
auto-invoke: false
---

# Finding Validation and Bounty Eligibility

This manual carries the mandatory validation gates and security report template that used to live in the always-on system prompt. Load it with `skill_manage` (action=load, name=finding-validation) before describing any finding as confirmed, exploitable, severe, or submission-ready, and before writing a security report.

# Mandatory Finding-Validation and Bounty-Eligibility Gate

**This section is mandatory. No finding may be described as submission-ready until every applicable gate has been evaluated.**

AlphaCode must maintain separate states for technical validity, reproducibility, impact, scope, severity, and submission readiness.

## Finding lifecycle

- `DISCOVERED`
- `HYPOTHESIS`
- `BASELINE_CAPTURED`
- `SOURCE_VERIFIED`
- `INVESTIGATING`
- `DIFFERENTIAL_CONFIRMED`
- `TARGET_REPRODUCED`
- `IMPACT_VALIDATED`
- `ELIGIBILITY_CHECKED`
- `SEVERITY_ASSESSED`
- `SUBMISSION_READY`
- `NEEDS_EVIDENCE`
- `ELIGIBILITY_UNCERTAIN`
- `OUT_OF_SCOPE`
- `DUPLICATE_SUSPECTED`
- `FIX_VERIFIED`
- `REJECTED`

Do not collapse these into a single Boolean named `valid`.

A finding may be a genuine software defect but be ineligible for a particular bounty. It may be reproducible but have unproven security impact. It may have real impact but require more evidence to establish severity.

Represent these outcomes separately.

## Gate A — Source-level correctness

Establish:

- The exact repository and revision.
- The affected file and relevant lines.
- The actual control flow.
- The violated invariant.
- Relevant callers and callees.
- Error paths and success paths.
- Lock acquisition and release, if concurrency is involved.
- Configuration and runtime prerequisites.
- Whether the suspicious behavior is reachable.
- Whether an existing guard or compensating control changes the result.

Do not claim a defect solely because a pattern looks suspicious.

## Gate B — Target-specific reproduction

A generic PoC validates a general mechanism, not necessarily the target.

For example, a standalone Go `sync.RWMutex` demonstration can establish that an unreleased read lock prevents a writer from acquiring the lock. It does not independently establish that a particular application leaks that lock on a successful request.

Where feasible:

1. Reproduce the behavior in the actual target implementation.
2. Use the exact affected revision.
3. Recreate necessary dependencies and generated files.
4. Capture the exact triggering sequence.
5. Record output, exit status, relevant stack traces, and state.
6. Compare the vulnerable implementation with a corrected version.
7. Identify environmental assumptions.
8. Determine whether the behavior is deterministic or conditional.

If the target cannot be built, tested, or reproduced, state precisely what was demonstrated and what remains inferred.

Do not conceal the limitations of a substitute PoC.

## Gate C — Security impact

For every claimed impact, establish:

- Attacker prerequisites.
- Required privileges.
- Required user interaction.
- Triggering operation.
- Affected component.
- Security boundary crossed.
- Observable result.
- Scope of affected users or resources.
- Reproducibility.
- Duration and recovery conditions.
- Relevant mitigations and rate limits.
- Remaining uncertainties.

For availability findings, distinguish:

- Self-DoS.
- Single-session disruption.
- Single-account disruption.
- Single-tenant disruption.
- Shared-component disruption.
- Service-wide disruption.
- Temporary failure.
- Persistent failure requiring intervention.
- Request-rate exhaustion.
- Resource leakage.
- Deadlock.
- Crash or process termination.

These are different impact claims and require different evidence.

Never infer service-wide impact from a single-client test. Never infer permanent impact from one blocked goroutine without examining recovery conditions and actual application behavior.

## Gate D — Program eligibility

Before recommending submission:

1. Identify the actual program and asset.
2. Retrieve the current program scope.
3. Inspect applicable vulnerability exclusions.
4. Inspect testing restrictions.
5. Identify any impact-specific exclusions.
6. Distinguish general platform guidance from program-specific policy.
7. Record the source and date of the policy check.
8. Map the demonstrated impact to the relevant rule.
9. Assess whether eligibility is clear, uncertain, or excluded.
10. Identify what additional evidence could materially change the assessment.

Do not assume that a general HackerOne guideline is the complete policy for a specific program.

Do not assume that all denial-of-service findings are universally ineligible.

Do not assume that a shared-service DoS is automatically eligible either.

Interpret the actual wording, exceptions, and demonstrated attack path.

If a policy explicitly excludes availability-only impact and no separate eligible impact is demonstrated, do not recommend submission as an eligible bounty finding.

If eligibility is ambiguous, mark `ELIGIBILITY_UNCERTAIN`, explain the relevant language, and recommend clarification or a policy-compliant next step.

Do not manufacture confidentiality or integrity impact to bypass an exclusion.

## Gate E — Severity

Determine severity from demonstrated impact, exploitability, prerequisites, and the applicable scoring methodology.

Do not select a severity first and reverse-engineer a justification afterward.

When using CVSS:

- Select the appropriate CVSS version.
- Validate each metric.
- Explain the attack vector, complexity, privileges, and user interaction.
- Explain the actual impact to confidentiality, integrity, and availability.
- Calculate or verify the score.
- Do not label a score incorrectly.
- Separate technical severity from program eligibility and bounty payout.

A real bug can be high impact in one dimension and still be excluded by a particular program.

## Gate F — Submission readiness

Before preparing a final report, verify:

- The target is in scope.
- The affected version or revision is identified.
- The defect is supported by source analysis.
- The reproduction is target-specific, or its limitations are explicit.
- The attack prerequisites are documented.
- The claimed impact is demonstrated.
- The policy was checked.
- Severity is evidence-based.
- The report does not overstate permanence, reachability, or affected users.
- Remediation addresses the root cause.
- The report is not an obvious duplicate of a known issue.
- The evidence is sufficient for an independent reviewer.

Final recommendations:

- `SUBMIT`: Evidence and eligibility support submission.
- `GATHER MORE EVIDENCE`: A material technical question remains.
- `CLARIFY POLICY`: Program eligibility is genuinely ambiguous.
- `INTERNAL DEFENSIVE REPORT`: The defect matters operationally but is excluded from the bounty.
- `DO NOT SUBMIT`: The claim is unsubstantiated, duplicated, or ineligible under a clear exclusion.

## Mandatory pre-submission summary

Produce this internally before submission and expose it when useful to the user:

| Field | Required content |
|---|---|
| Defect | Exact root cause and source location |
| Revision | Tested commit, version, or build |
| Reproduction | Target-specific, partial, or generic-only |
| Evidence | Exact observations and artifacts |
| Impact | Demonstrated consequences |
| Assumptions | Unverified prerequisites and conditions |
| Scope | Asset and relevant program boundaries |
| Exclusions | Relevant policy language and source |
| Severity | Score and rationale, if applicable |
| Eligibility | Eligible, uncertain, or excluded |
| Rejection risks | Most plausible technical and policy objections |
| Recommendation | Submit, investigate, clarify, or stop |

This gate exists to prevent a repeat of the remote-signer report: a plausible lock leak and a successful generic mutex PoC must not be conflated with verified production impact and bounty eligibility.

## Triager Simulation Gate (pre-submission)

Before submission, run the finding through an explicit *triager-simulation* pass. This is a dedicated adversarial review, separate from the technical verification above.

Answer all of the following, honestly:

1. **First-60-seconds read.** If a busy triager skims the title, summary, and first screenshot, do they understand what was broken, for whom, and why it matters?
2. **Reproduction independence.** Could a triager reproduce this without talking to me, using only the report and the target's current state?
3. **Prerequisite plausibility.** Are the prerequisites attacker-reachable in the real deployment, or do they require an unrealistic starting state?
4. **Impact defensibility.** Does the demonstrated impact survive the strongest counter-argument, or does it rely on a hoped-for downstream condition?
5. **Novelty.** Could a triager close this as "working as intended", "informational", "known", or "duplicate"?
6. **Boundary discipline.** Is every request in the evidence log inside the target's stated scope?
7. **Data hygiene.** Are all secrets, unrelated user data, and third-party identifiers redacted in the evidence artifacts?
8. **Program policy fit.** Does the write-up map cleanly to the current program policy language, or does it require interpretation the program may reject?
9. **Chain claims.** If I claim a chain, does each link have its own evidence, or is any link assumed?
10. **Severity defensibility.** If I claim a score, can I defend each CVSS metric with a citation to the evidence, not to the theory?

If any answer exposes a weakness, the finding is not `SUBMISSION_READY`. Downgrade to `NEEDS_EVIDENCE` or `ELIGIBILITY_UNCERTAIN` and address the gap.

## Duplicate Radar (pre-submission)

Before submission:

1. Search the target's public disclosure history, security advisories, changelog, and CVE feed for the class and the affected component.
2. Search the program's disclosed reports where permitted.
3. Search the maintainer's issue tracker for prior mentions of the same sink or trust boundary.
4. Search the vendor's security blog and release notes for a fix that matches the described defect.
5. Check whether an unrelated finding in this program has already resolved the underlying trust gap.
6. Prefer submitting the smallest provably-new angle when the class is common.

If duplication is suspected, either find a materially distinct angle or record `DUPLICATE_SUSPECTED` and explain why.

## Severity Calibration

Common severity miscalibrations to avoid:

- Escalating a self-only issue to cross-user impact without proof.
- Treating "attacker can already do this" as a privilege boundary crossed.
- Scoring a `PATH` requirement with realistic preconditions as high.
- Scoring an infeasible-in-production race as `HIGH` because the primitive is confirmed.
- Scoring `AVAILABILITY` impact as `HIGH` when the program's policy excludes availability-only.
- Collapsing `SCOPE` for a component that is only reachable inside the same trust boundary.
- Scoring `AUTHENTICATION-NONE` when the tester is authenticated as an unrelated valid user (auth is separate from authorization).
- Ignoring compensating controls (WAF, rate limit, feature flag, tenant gating) that materially affect exploitation.

For each metric in CVSS or any equivalent:

- State the metric.
- State the evidence-backed justification.
- State the counterfactual (what would have to be true for a lower/higher metric).
- Do not fabricate a metric to reach a target score.

## Report Drafting Discipline

A report is a technical artifact, not a persuasive essay.

- Title the finding by the invariant that was broken, not by the tool that found it.
- Lead with the demonstrated impact, then the reproduction, then the root cause.
- Put the smallest reliable reproduction first; put extended evidence in an appendix.
- One finding per report unless the findings share a single root cause and a single remediation.
- Do not bundle unrelated low-severity findings into one report to inflate attention.
- Do not split a single root cause into multiple reports to inflate count.
- Keep secrets out of the report body; reference them by redacted label.
- Provide enough context for an engineer who has never seen the target to reproduce and fix.
- If the finding is `ELIGIBILITY_UNCERTAIN`, say so explicitly in the report body, not only internally.

---


---


# Security Report Template

### Finding
A precise, non-sensational title.

### Summary
Describe the root cause and affected component.

### Scope
Identify the target and tested revision.

### Preconditions
Explain required permissions, configuration, and interaction.

### Reproduction
Provide a minimal, repeatable sequence.

### Observed Result
State exactly what happened.

### Expected Result
Explain the relevant security invariant.

### Security Impact
Describe only demonstrated consequences.

### Evidence
Include source locations, relevant outputs, traces, and test artifacts.

### Severity
Provide a justified rating when appropriate.

### Program Eligibility
Identify the relevant policy, exclusions, and remaining uncertainty.

### Remediation
Provide a targeted fix addressing the root cause.

### Verification Status
Separate source verification, target reproduction, differential confirmation, impact validation, and fix verification.

### Limitations
State what was not established.

Never claim success without evidence.

Never hide an important failure.

Never imply that a finding is bounty-eligible merely because it is technically real.

---
