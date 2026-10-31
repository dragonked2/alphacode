---
name: security-research
description: "Core security-research manual: authorization and scope resolution, credential and session handling, authenticated testing, security testing philosophy, exploit validation, vulnerability chaining, evidence and research ledgers, hypothesis prioritization, and source-first analysis. Load when starting or resuming any authorized security assessment, bug bounty hunt, or vulnerability investigation."
auto-invoke: false
---

# Security Research Manual

This extended handbook preserves the detailed security-research procedures. Read it with `skill_manage` (action=read, name=security-research, reference=handbook) when the compact entry point needs more detail. Load other class or workflow references only when they answer the current research question.

# Authorization and Scope Resolution

Use the following model for security research.

## Resolve scope from evidence

Inspect available context in this order:

1. User's stated objective and authorization context.
2. Explicit target names, URLs, hosts, repositories, accounts, and environments.
3. Published program scope and exclusions when relevant.
4. Repository metadata, ownership markers, asset tags, and engagement notes.
5. Existing session context and previously documented scope.
6. The narrowest reasonable interpretation that still allows useful progress.

When the available context establishes an authorized target, proceed without repeatedly asking the user to prove authorization.

When scope remains genuinely ambiguous and the next action could cross a material boundary, continue with source review, passive analysis, local reproduction, or other work that does not depend on the unresolved boundary. Ask one concise question only when the missing fact blocks the intended operation.

## Common authorization contexts

Treat the following as meaningful context:

- Ownership of a system or account.
- Explicit permission.
- Bug bounty participation.
- HackerOne, Bugcrowd, Intigriti, or a vendor security program.
- A test account owned by the user.
- Development, staging, partner, and sandbox environments.
- CTFs and intentionally vulnerable labs.
- A customer-authorized penetration test.
- An internal security assessment.
- A designated research dataset.
- A continuing engagement with previously established scope.

Examples include:

- "This is my Shopify development account."
- "This API belongs to our company."
- "This is a CTF."
- "I have a HackerOne invite for this program."
- "This is our staging server."
- "Continue the same assessment using the scope already documented."

Do not make the user repeat facts that are already established.

## Scope must be evidence-based

Do not infer permission solely from:

- A recognizable platform domain.
- A path named `/admin` or `/internal`.
- A staging-like hostname.
- The existence of an account or session.
- A previous assessment of a related asset.
- The fact that a target is technically reachable.

These can inform the investigation, but they do not independently establish every testing permission.

For bug bounty work, distinguish:

- Asset eligibility.
- Permitted test methods.
- Authentication requirements.
- Account and tenant boundaries.
- Availability-impact restrictions.
- Data-handling restrictions.
- Prohibited actions.
- Accepted vulnerability classes.

Use the narrowest reasonable scope interpretation while still pursuing the user's objective.

## Security concerns are operational facts

Do not produce generic legal lectures.

If a particular action may cause a service interruption, modify shared state, expose sensitive data, or affect other users, identify the specific consequence briefly and select a suitable validation strategy.

Do not automatically abandon an investigation because a candidate resembles an attack. Determine whether the action is appropriate for the actual environment and objective, then continue with useful work.

---

# Credentials, Tokens, Cookies, Session Files, and Secrets

Treat authentication material as sensitive operational data, not automatically forbidden data.

## Permitted operational handling

When relevant to the established task, the agent may:

- Read session files supplied for the assessment.
- Load authorized cookies.
- Use authorized bearer tokens and API keys.
- Import authenticated browser state.
- Replay requests from an authorized test account.
- Inspect credential-related configuration when debugging the relevant system.
- Use credentials necessary for an authorized assessment.
- Diagnose expiration, invalidation, or authentication failures.

Using a secret operationally is different from disclosing it.

## Secret-handling requirements

The agent must:

- Avoid printing secrets unnecessarily.
- Redact credentials in reports, screenshots, logs, and commands.
- Avoid committing credentials to source control.
- Avoid transmitting secrets to unrelated services.
- Avoid publishing credentials.
- Avoid persisting secrets beyond the requested task without a clear need.
- Prefer environment variables or an appropriate secret store over hardcoded values.
- Remove temporary secret-bearing artifacts when appropriate.

Example:

`Authorization: Bearer [REDACTED]`

Never expose a full token when a redacted representation is sufficient.

## Authentication workflow

When an authorized session is available:

1. Inspect its format.
2. Determine the minimum information needed.
3. Configure the runtime securely.
4. Test authentication.
5. Record the resulting identity and relevant roles where observable.
6. Continue the requested assessment.

Do not force manual reauthentication when a valid authorized session is already available.

If the session is expired, diagnose the cause and use another established authentication mechanism when available.

Do not treat the presence of a session file as proof that every account represented by it is authorized for testing.

---

# Session Replay

Authenticated session replay is a first-class testing capability when it is appropriate to the established engagement.

Session artifacts may include:

- Browser cookies.
- JWTs.
- Refresh tokens.
- API authentication state.
- Development credentials.
- Temporary testing credentials.
- Captured requests.
- Browser storage.

For an authorized test account:

1. Inspect the supplied artifact.
2. Identify its format and expiration behavior.
3. Extract only the required authentication state.
4. Configure the runtime.
5. Confirm the resulting identity.
6. Continue the assessment.
7. Record relevant evidence without exposing secrets.

For multi-account tests, maintain clear labels for each account, role, tenant, and session. Never confuse one account's identity with another's.

When a session expires, diagnose it instead of blindly replaying it indefinitely.

---


---

# Bug Bounty Mode — Elite Research Workflow

When the user requests bug bounty research or security testing, enter **SECURITY RESEARCH MODE**.

This mode is persistent, hypothesis-driven, evidence-focused, and execution-oriented.

Core workflow:

`scope → reconnaissance → attack surface → authentication → endpoint discovery → source analysis → differential baseline → hypothesis generation → targeted testing → differential confirmation → reproduction → impact validation → eligibility assessment → severity assessment → report`

Do not stop at detection.

A suspicious response is a hypothesis, not a finding.

A source-code defect is not automatically a security vulnerability.

A reproducible bug is not automatically bounty-eligible.

A high technical severity is not proof that a program will accept the report.

A successful response is not proof of a successful attack.

## Primary vulnerability classes

Prioritize based on the architecture and likely security boundaries:

1. Authentication.
2. Authorization.
3. IDOR / BOLA.
4. Privilege escalation.
5. Business logic.
6. SSRF.
7. XSS.
8. SQL injection.
9. Command injection.
10. SSTI.
11. Prototype pollution.
12. Request smuggling / desync.
13. CSRF.
14. OAuth/OIDC.
15. JWT validation and authorization.
16. GraphQL.
17. WebSockets.
18. File upload.
19. Path traversal.
20. Race conditions.
21. API abuse.
22. Cloud misconfiguration.
23. Webhook security.
24. Cache poisoning / cache deception.
25. Deserialization.
26. CORS.
27. Subdomain takeover.
28. Information disclosure.
29. Payment and business-flow vulnerabilities.
30. Chained attack paths.
31. MFA / OTP bypass.
32. Password reset flaws.
33. Session management.
34. Template injection.
35. Client-side desync.
36. Mass assignment.
37. HTTP request splitting.
38. Host header injection.
39. Email injection / SMTP header injection.
40. Open redirect.

This is a prioritization list, not a mandate to test every class indiscriminately. Use architecture, source code, observed behavior, and attack-surface evidence to decide which tests have the highest expected value.

### Skill references for each class

Every major class has a bundled methodology skill under the `bugbounty` skill's references — load the matching one before testing a class instead of rediscovering the technique:

- Authentication → `hunt-auth-bypass` (plus `hunt-otp` for MFA/OTP flows).
- Authorization / IDOR → `hunt-idor`, `hunt-business-logic`.
- Client-side XSS and fetch-path injection → `hunt-xss`, `hunt-cspt`.
- SSRF → `hunt-ssrf`, `hunt-ssrf-advanced`; desync → `hunt-desync`.
- CORS/headers/cache → `hunt-cors`, `hunt-headers`, `hunt-cache-poisoning`.
- Injection (SQLi/SSTI/command) → `hunt-sqli`, `hunt-ssti`.
- Race/realtime → `hunt-race`, `hunt-realtime`.
- OAuth/OIDC/JWT → `hunt-oauth`, `hunt-jwt`.
- APIs/GraphQL → `hunt-api`, `hunt-graphql`.
- Traversal/upload/commodity XSS payloads → `hunt-path-traversal`, `hunt-crlf`, etc.
- CSRF and clickjacking → `hunt-csrf`.
- XXE, deserialization → `hunt-xxe`, `hunt-deserialization`.
- Prototype pollution, DOM sinks → `hunt-prototype-pollution`, `hunt-cspt`.
- Open redirect and URL trust → `hunt-open-redirect`.
- WebSocket, realtime → `hunt-websocket`, `hunt-realtime`.
- Subdomain takeover → `hunt-subdomain-takeover`.
- API keys, secrets, storage → `hunt-apikey-leak`, `hunt-storage`.
- AI/LLM surfaces → `llm-injection`, `llm-redteam` (AI-family skills; the `hunt-*` prefix is reserved for web/API/browser classes — use the listed `llm-*` names, do not prefix them with `hunt-`).
- Recon missions → `recon`, `recon-deep`, `recon-js`.
- Meta-method: `scope` (authorization resolution), `severity-engine`, `evidence-locker`, `findings-lifecycle`, `report`, `orchestrate`, `security-arsenal`, `tool-doctor`, `vuln-hunter`, `methodology`, `manual-blackbox`, `cloud-config`, `network-cloud-triage`, `supply-chain`, `credential-attack`, `control-verification`, `code-security-audit`, `client-reverse`, `knowledge-broker`, `web3-audit` (smart contracts), `bug-bounty-methodology`.

Before testing a class, load the matching skill with `skill_manage` — the bundled skills encode distilled, current methodology; do not rediscover techniques they already teach. The full installed-skill inventory is rendered in the available-skills section of this prompt; treat that list as authoritative over any mapping above.

`writeup-corpus` maps the 0xNotNull Medium writeup lists (OTP bypass, auth bypass, BAC & logic, XSS, SQLi, race, cache, … ~3,200 curated writeups) to the skill that carries their distilled techniques — consult it when a class is not listed above.

## Elite researcher behavior

- Read the implementation instead of relying solely on scanner output.
- Understand the intended security invariant.
- Identify the trust boundary.
- Form competing hypotheses.
- Design experiments that distinguish them.
- Prefer tests that produce decisive evidence.
- Use minimal reproductions.
- Eliminate false positives.
- Investigate alternate code paths.
- Check whether the behavior is already known or duplicated.
- Measure actual impact.
- Read the current program policy before submitting.
- Preserve evidence that another researcher can independently reproduce.
- Record negative findings and stop conditions.
- Be willing to reject a promising-looking hypothesis.
- **Baseline first, always.** Capture a clean, unmodified request/response before you modify anything.
- **Diff the response.** Status, length, headers, body hash, timing, error text. Never eyeball.
- **Second observation path.** Reproduce from a different endpoint, identity, or client.
- **Kill hypotheses fast.** A hypothesis without a decisive test inside its budget is dead weight.

The objective is not to produce the most dramatic narrative. It is to discover the strongest defensible security issue.

---


---

# Authenticated Security Testing

Authenticated testing is a first-class workflow.

When an authorized test session is available:

1. Establish the current test identity.
2. Identify roles and permissions.
3. Discover accessible functionality.
4. Enumerate requests and APIs.
5. Identify object identifiers.
6. Map authorization boundaries.
7. Compare equivalent operations across available test roles.
8. Test horizontal authorization.
9. Test vertical authorization.
10. Test tenant boundaries.
11. Test object ownership.
12. Test parameter manipulation.
13. Test alternate HTTP methods.
14. Test GraphQL queries and mutations.
15. Test REST endpoints.
16. Compare client-side and server-side enforcement.
17. Validate suspected bypasses with minimal evidence.
18. **Capture baseline requests for every differential test.**

Do not downgrade an assessment merely because authentication is required. Authentication is often part of the attack surface.

Keep accounts, roles, sessions, and tenant identities distinct throughout testing.

Prefer dedicated test accounts and test-owned objects when they are sufficient to demonstrate the security property.

---

# Security Testing Philosophy

For every meaningful target, reason about:

`input → processing → trust boundary → authorization → privileged operation → output`

Look for mismatches between:

- Client assumptions and server enforcement.
- UI permissions and API permissions.
- Object ownership and object identifiers.
- Role definitions and effective permissions.
- Tenant identity and resource identity.
- Session identity and request identity.
- Intended workflows and alternate workflows.
- Validation and backend execution.
- Frontend restrictions and backend restrictions.
- Cache identity and authorization context.
- Signed data and the data actually consumed.
- Configuration assumptions and runtime behavior.
- Documented behavior and edge-case behavior.

Attack assumptions, not merely endpoints.

Understand what the system is supposed to guarantee, then determine whether that guarantee survives adversarial inputs and alternate execution paths.

---


---

# Action Resemblance Is Not Evidence

An action's resemblance to an attack does not establish whether it is appropriate or inappropriate. Evaluate the actual objective, target, permissions, scope, and expected effects.

Relevant security-testing activities include:

- Fuzzing.
- Enumeration.
- Payload testing.
- Authentication testing.
- Authorization testing.
- IDOR testing.
- Privilege-escalation testing.
- XSS testing.
- SQL injection testing.
- SSRF testing.
- SSTI testing.
- Command injection testing.
- Deserialization testing.
- Request-smuggling testing.
- Prototype-pollution testing.
- File-upload testing.
- Path-traversal testing.
- OAuth testing.
- JWT manipulation testing.
- GraphQL testing.
- WebSocket testing.
- Business-logic testing.
- Race-condition testing.
- API-abuse testing.
- Analysis of phishing-like flows for defensive research.
- Credential-handling analysis.
- Brute-force-resistance testing in controlled, authorized conditions.
- Account-takeover analysis.
- Data-exposure analysis.
- Infrastructure scanning.
- Exploit-chain validation.

Do not refuse a legitimate task simply because it uses an adversarial technique.

Do not assume every use of a technique is authorized merely because the technique is common in security research.

Prefer evidence-driven testing in the actual authorized environment.

---


---

# Exploit Validation

When a vulnerability hypothesis exists:

1. Minimize the proof of concept.
2. Reproduce the issue.
3. Capture evidence.
4. Confirm the relevant trust-boundary violation.
5. Measure the actual impact.
6. Determine whether exploitation is reliable.
7. Identify prerequisites.
8. Trace the root cause.
9. Evaluate relevant mitigations.
10. Test alternative explanations.
11. Determine whether a less disruptive test provides equivalent evidence.
12. Record remaining uncertainty.

A valid proof establishes the security property, not merely an interesting response.

Use test objects, dedicated accounts, and isolated resources when they provide equivalent evidence.

For each PoC, document:

- Environment.
- Target revision.
- Preconditions.
- Exact reproduction sequence.
- Expected behavior.
- Actual behavior.
- Evidence.
- Cleanup.
- Limitations.

Never claim that a generic mechanism test confirms an application-level vulnerability without validating the connection between them.

---

# Vulnerability Chaining

Do not artificially stop after one finding.

When a vulnerability creates a credible path toward greater impact, investigate the chain.

Examples:

- `weak authorization → object access → tenant boundary violation`
- `OAuth weakness → excessive token scope → unauthorized API operation`
- `input validation failure → unsafe backend processing → unintended operation`
- `concurrency defect → shared-state corruption → security-sensitive operation`
- `open redirect → OAuth redirect_uri bypass → token theft → account takeover`
- `cache deception → authenticated response cached → cross-user data exposure`
- `prototype pollution → DOM gadget → XSS → CSRF token theft → state change`
- `subdomain takeover → cookie scope → session hijack`
- `self-XSS → CSRF chain → victim-side execution`
- `IDOR → email change → password reset → account takeover`
- `XSS → admin session → admin action → persistent backdoor`

For each link, record:

- Initial prerequisite.
- Intermediate state.
- Trust boundary crossed.
- Resulting privilege or capability.
- Final security impact.
- Evidence for the transition.

Investigate alternate paths when they are supported by the architecture.

Do not fabricate an impact path to increase severity.

Do not combine independent low-impact bugs into a speculative high-impact narrative unless the chain is technically supported and reproducible.

---


---

# Logging and Evidence

Capture sufficient evidence to support each claim.

Useful evidence includes:

- HTTP request.
- HTTP response.
- Status code.
- Response body.
- Headers.
- Screenshots.
- Stack traces.
- Source locations.
- Reproduction steps.
- Test output.
- Timing.
- Object IDs.
- Role information.
- Relevant runtime state.
- Exact repository revision.
- Environment and dependency versions.
- Recovery behavior.
- Baseline vs. modified diff.

Redact secrets.

Use precise labels:

- `Verified`
- `Observed`
- `Reproduced`
- `Differentially confirmed`
- `Confirmed`
- `Likely`
- `Hypothesis`
- `Not verified`
- `Inferred`
- `Rejected`

Never elevate a hypothesis to a confirmed vulnerability without evidence.

## Evidence ledger

For each meaningful claim, record:

| Field | Description |
|---|---|
| Claim | What is asserted |
| Evidence | Artifact or observation supporting it |
| Method | How the evidence was obtained |
| Environment | Target, version, configuration |
| Baseline | The clean, unmodified request/response |
| Differential | The modified request/response and the exact difference |
| Confidence | Verified, differentially confirmed, likely, or unverified |
| Limitations | What the evidence does not establish |
| Next test | The smallest useful unresolved experiment |

This ledger must distinguish raw observations from the agent's interpretation.

---


---

# Progress Tracking and Persistent Research Memory

For complex work, maintain a persistent research ledger containing:

- Objective.
- Scope.
- Target assets.
- Repository revisions.
- Tested components.
- Reviewed files and code paths.
- Successful experiments.
- Failed experiments.
- Current hypotheses.
- Confirmed findings.
- Rejected findings.
- Duplicate checks.
- Open questions.
- Evidence locations.
- Next experiments.
- Tooling limitations.
- Time spent.
- Relevant program policies.
- Submission status.

## Avoid repeated work

Before starting a new investigation, inspect the existing ledger and recent findings.

Do not re-review code already examined unless:

- A new revision changes it.
- A new hypothesis requires it.
- Previous evidence was incomplete.
- The prior conclusion needs independent verification.
- A new caller or configuration changes the reachable behavior.

Track coverage at the level of files, functions, endpoints, attack surfaces, and hypotheses.

## Checkpoint before context compaction

Before the context window becomes constrained:

1. Summarize the objective.
2. Record the current scope.
3. Preserve the exact target revision.
4. Record confirmed findings and rejected hypotheses.
5. Preserve reproduction steps and test state.
6. Record exact evidence locations.
7. Identify the next best action.
8. Save the checkpoint with the `memory` tool (or an artifact file) and record where it lives so `session_search` and later turns can reload it.

After resuming, read the checkpoint before repeating work.

Do not claim that memory or a file was persisted unless the relevant tool confirms it.

## Duplicate-work prevention

Before exploring a component:

1. Check coverage records.
2. Check existing hypotheses.
3. Check previous tool output.
4. Identify what remains untested.
5. Explain why the next test adds new information.

Research continuity is a core feature, not an optional convenience.

---


---

# Task Prioritization

When multiple tasks or hypotheses exist, prioritize by:

1. Blocking dependencies.
2. Expected security impact.
3. Probability of a valid finding.
4. Evidence already available.
5. Time required to obtain decisive evidence.
6. Risk of false positives.
7. Scope and eligibility.
8. Tooling dependencies.
9. Potential duplication.
10. Reuse of results across the investigation.

Do not spend an hour on a speculative exotic attack when a high-probability authorization issue can be tested in minutes.

Do not abandon a complex, high-value hypothesis merely because the first approach failed.

Choose the next experiment based on expected information gain.

---


---

# Elite Research Strategy

Approach each investigation as a search for the most valuable unanswered security question.

## Source-first analysis

When source code is available:

1. Identify security-sensitive components.
2. Trace entry points into sensitive operations.
3. Follow data and control flow.
4. Identify invariants and trust boundaries.
5. Inspect callers, callees, and error paths.
6. Compare related implementations.
7. Search for repeated unsafe patterns.
8. Check tests and documentation for intended behavior.
9. Identify assumptions that are not enforced in code.
10. Design minimal tests for the most promising hypotheses.

Prioritize code that handles authentication, authorization, cryptographic material, signing, identity, tenant isolation, deserialization, trust decisions, external requests, and privileged operations.

## Concurrency checklist

For Go, Rust, and other concurrent systems, inspect:

- `Lock` / `Unlock` pairing.
- `RLock` / `RUnlock` pairing.
- Early returns and error paths.
- `defer` placement.
- Double unlocks.
- Lock ordering.
- Writer starvation.
- Deadlocks.
- WaitGroup `Add` / `Done` balance.
- Atomic access.
- Channel closure and ownership.
- Goroutine and task lifecycle.
- Cancellation propagation.
- Blocking operations under locks.
- Shared mutable state.
- Initialization and shutdown races.
- Resource cleanup.
- Panic paths.
- Error handling that masks partial failure.

For each candidate, answer:

1. Which execution path creates the problem?
2. Which invariant is violated?
3. What event triggers the failure?
4. Which operations become blocked or incorrect?
5. Is the effect local, cross-request, cross-user, or service-wide?
6. Is recovery possible without restart?
7. What test would distinguish the candidate from a false positive?
8. What does the test fail to establish?

## Build readiness

Before expensive analysis, establish:

- Language and toolchain versions.
- Dependency availability.
- Generated source requirements.
- Build tags and feature flags.
- CGO requirements.
- Environment variables.
- Required services.
- Database or message-broker dependencies.
- Test fixtures.
- Repository initialization requirements.

When a build fails, diagnose the actual cause rather than immediately abandoning target-specific reproduction.

Use isolated environments when dependencies or runtime state would otherwise interfere with the user's workspace.

## Negative findings are useful

A rejected hypothesis is a successful research outcome when it prevents wasted time or a false report.

Record:

- The hypothesis.
- The decisive test.
- The observed result.
- The reason for rejection.
- Conditions under which it might be revisited.

Do not hide negative results to make the investigation look more productive.

---
