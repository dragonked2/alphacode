---
name: cross-feature-workflows
description: Map authorized product workflows and investigate security controls across feature and processing boundaries.
---

# Cross-feature workflow investigation

Inspired by The XSS Rat's [main application methodology](https://github.com/The-XSS-Rat/SecurityTesting/blob/a68ec21186298e63682849731be4861432de5282/Checklists/Main%20app%20methodology.md) and [bug bounty guide](https://github.com/The-XSS-Rat/SecurityTesting/blob/a68ec21186298e63682849731be4861432de5282/Checklists/2026-bug-bounty-guide.md). This is an independently written workflow, not a copy of the repository or a claim that its suggestions establish impact.

## Build an observable product map

Within the program's current rules, use the product normally with controlled accounts and data. Record only observed features, requests, roles, object ownership and state changes. Consult available product and API documentation to discover candidate workflows, then check whether the deployed behavior agrees. Separate documented or inferred functionality from what was actually observed; a missing role, paid feature or unavailable account is a coverage gap, not a reason to invent one.

For each significant object, record:

`object | creator/owner/tenant | input or source | stored state | consumers | actor at each consumer | trust boundary | evidence`

Trace handoffs such as create → edit → preview → export, upload → processing → download, record → notification → administration, or configuration → scheduled job → callback. Include alternate clients, versions, asynchronous jobs and integrations only when they are observed or documented and in scope. A parameter name or old URL suggests a question; it does not prove the endpoint exists or is vulnerable.

## Select discriminating experiments

For each handoff, write an invariant and compare the same controlled object across its entry and consuming paths. Prioritize sensitive operations, privilege boundaries, durable state and reachable integrations over generic header observations. Examples:

- Does an export, preview, search result or notification enforce the same owner, tenant and role boundary as the original record? Compare authorized and unauthorized actors where distinct controlled identities are available.
- Does data saved by one feature acquire a different interpretation when later rendered, queried, imported or processed? Trace a harmless unique marker through actual consumers before choosing a context-appropriate proof. A marker's appearance or an outbound callback alone does not establish a specific exploit or impact.
- Does an asynchronous operation recheck authorization and state at execution time, including after a controlled permission or lifecycle change? Verify the durable effect, not merely an accepted request.
- Does a documented transition remain enforced when the same action is reached through an observed alternate interface? Compare equivalent requests and object states, including a negative control.

Use the smallest permitted experiment that distinguishes the hypothesis. Track actor, tenant, object, feature, state, expected result, observed result and evidence. Avoid broad persistent payload placement, unapproved automated scans, real-user data, irreversible changes and third-party callbacks outside the rules. Avoid assuming that similar-looking routes share authorization or that a UI restriction is a server-side control.

## Disposition and handoff

For each candidate, test plausible alternatives such as own-data access, legitimate shared membership, stale state, an SPA fallback or a background process unrelated to the request. Mark each handoff tested, pending, blocked or not applicable with supporting evidence. If a second actor or consumer is unavailable, keep that comparison blocked and continue independent work. Before reporting, use `finding-validation` to separate reproduction, demonstrated impact, severity and eligibility. Do not infer Critical/High severity from an integration point or chain idea alone.
