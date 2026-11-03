---
name: chain-reasoning
description: Build and validate compatible vulnerability chains, prioritize newly reachable boundaries, and distinguish demonstrated impact from hypothetical escalation.
---

# Evidence-backed chain reasoning

A primitive is a capability under specific conditions. It is not a severity
label. Connect demonstrated capabilities to protected operations through an
explicit graph, using `investigation-control` for clue continuity and
`real-world-validation` for controls and proof. Continue already-authorized
investigation without repeated permission requests; a new host or capability
does not expand scope. Confirm the next action is permitted before executing it.

## Represent each edge

```text
Edge ID / source clue or finding:
Actor, tenant, session, origin, version/configuration:
Capability observed, constraints and exact evidence:
Output obtained -> prerequisite required by next edge:
Next protected operation and expected security boundary:
Controlled fixture / baseline / negative control:
Next discriminating experiment / budget / stop condition:
State: proposed | reachable | tested-supported | refuted | blocked
```

Require compatible prerequisites across all edges. Check principal continuity,
cookie/token scope, read versus write authority, user interaction, lifetime,
origin, network reachability and configuration. A leak from an admin-only debug
page is not an unauthenticated entry point. Public DNS does not prove access to
an internal service. A credential-looking string does not prove a valid secret.

## Choose follow-up by what the primitive actually permits

| Primitive | Useful follow-up within existing authorization | Insufficient evidence |
|---|---|---|
| Error/config disclosure | Confirm a reachable feature/version or owned private configuration boundary | Automatically assigning Low/Medium severity or assuming an internal service is accessible |
| Object read | Compare another owned tenant's private canary and relevant export/download path | Guessable IDs, public fields or same-owner objects |
| Object/field write | Verify a documented forbidden change on a disposable owned fixture, then read persisted state | Changing an ordinary writable `display_name` and calling it mass assignment |
| File upload/storage | Trace who retrieves the object, how it is interpreted and whether access control holds | Upload accepted, renamed extension, or inert payload stored |
| SSRF-like callback | Distinguish DNS, HTTP fetch and an actual authorized protected-resource consequence | Treating a callback alone as internal read or credential access |
| Credential exposure | Check source, environment, audience and narrowly permitted capability with owned data | Inferring broad account takeover from the string or enumerating unrelated records |
| Workflow/race | Identify invariant, run a sequential control and verify the settled controlled result | Several success responses, transient duplicate UI, or a production financial loss |
| Agent instruction following | Correlate actual tool call, principal and resulting controlled state | A model saying it performed an action |

## Demonstrate the requested consequence

Once an edge is proven, stop redundant proof. Reprioritize independent coverage
or test another necessary edge if it is in scope and needed to establish the
requested consequence. Use reversible fixtures, small canaries and bounded
concurrency. A new permission requirement or excluded action blocks that edge;
preserve the hypothesis and continue permitted work.

Do not truncate a legitimate authorized chain merely because its first edge was
interesting. Do not deepen a chain solely to inflate severity. Decide from the
engagement objective, satisfied prerequisites, safety limits and sufficient proof.

## Report the graph honestly

Separate:

- Demonstrated path: tested entry point, compatible actor context, verified edges
  and observed end effect, each linked to captured evidence.
- Conditional path: untested edges, missing prerequisites and what would establish
  or refute them. Exclude hypothetical effects from the demonstrated severity.
- Remediation: the earliest enforceable trust boundary and any independent
  controls that must also be corrected. Deduplicate shared causes without hiding
  separately exploitable boundaries.

Report exact affected fixtures and tested breadth. One instance does not prove
all tenants, routes or records are affected. A failed higher-impact edge does
not invalidate an independently proven lower-impact finding.
