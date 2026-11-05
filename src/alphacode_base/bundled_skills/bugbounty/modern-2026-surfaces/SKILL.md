---
name: modern-2026-surfaces
description: "Evidence-driven hunting guidance for modern 2026 attack surfaces: AI/LLM agents, MCP servers, OAuth/OIDC flows, Next.js and middleware authorization, HTTP parser/cache disagreements, GraphQL, deserialization, cloud metadata SSRF, and false-positive controls for authorized assessments. Do not treat publication date, framework fingerprints, or vulnerability names as proof of a finding."
---

# Modern 2026 hunting surfaces

This skill adds surface-specific hunting guidance for current platforms and integration patterns. Every hypothesis still follows the mandatory contract in `real-world-validation`: a stated security invariant, a falsifiable prediction, owned fixtures, and demonstrated unauthorized data or state before reporting.

## 1. AI/LLM agent and MCP surfaces

Treat model instruction-following as untrusted user input, and separate it from the tool's actual authorization. A response saying "action succeeded" is not proof the tool executed it.

| Boundary to test | What must be verified | Weak evidence (not enough alone) |
|---|---|---|
| Principal → client → MCP server → downstream resource | Intended token audience, per-client consent, per-user authorization, and whether session/state handles are accepted as identity | A tool call returned "ok" |
| Token passthrough | Tokens accepted only if explicitly issued to this MCP server/audience; no passthrough of upstream-issued opaque tokens | A request was forwarded |
| OAuth metadata discovery | HTTPS enforcement, private-IP/link-local blocking, redirect-target validation, DNS pinning between check and use | The URL resolved once |
| Retrieved documents / poisoned context | Tool arguments, authorization context, and resulting state, not model output | The document mentions an action |
| Confused-deputy MCP proxies | Per-client consent registry checked before third-party authorization; exact redirect-URI match | Consent cookie existed |

Use a local controlled callback or an owned endpoint; never probe unrelated internal infrastructure. Revalidate scope after resolving an external URL.

**Primary source:** MCP Security Best Practices, version 2025-11-25, `modelcontextprotocol.io/specification/2025-11-25/basic/security_best_practices` (checked live). It covers confused deputy, token passthrough, SSRF via OAuth metadata discovery, session hijacking, local MCP server compromise, and URL scheme validation.

## 2. OAuth / OIDC and session state

Verify current vendor advisories and program rules before selecting tests. OAuth 2.0 Security BCP (RFC 9700, January 2025) is the baseline for redirect-based flows; OpenID Connect nonce and issuer handling, PKCE, sender-constrained tokens, and exact redirect-URI matching.

Falsifiable hypotheses rather than payload roulette:

- **Redirect-URI validation:** client allows pattern/wildcard redirect URIs, or open redirector leaks codes. Prove with an exact authorization code delivered to a controlled receiver under scope.
- **Mix-up / issuer confusion:** client uses multiple authorization servers without `iss` binding or distinct redirect URIs. Prove the code/token ends up at the wrong server.
- **PKCE downgrade / missing verifier enforcement:** token request with `code_verifier` accepted without a matching authorization-request `code_challenge`.
- **Implicit grant still enabled:** access token issued in the authorization response (URL leakage, replay).
- **Sender constraint missing:** stolen bearer token usable at another client or resource server.
- **Scope/privilege over-grant:** token accepted for resources or actions outside the documented authorization.
- **Stale sessions / session fixation:** session accepted after password change, logout, or identifier reuse without re-binding.

Negative control: same flow from an owned account with no manipulation. Status alone never proves authorization — inspect the protected data or persisted effect.

## 3. Next.js / middleware and proxy authorization

Verify router type, enabled features, and deployment evidence before testing. A framework name or banner does not establish the effective dependency, backported patches, or middleware placement.

- **Authorization only in middleware/proxy:** compare the normal document route with framework data/prefetch routes, locale handling, and dynamic-route parsing. Prove unauthorized application data is reachable through a path that bypasses the middleware check.
- **Cache poisoning / cache deception:** distinct cache keys per identity, normalized vs. raw path handling, and shared public cache returning private content under another actor's cache key. Require an isolated owned cache key and reproducible cross-context effect; avoid poisoning other users' entries.
- **Soft-404 / SPA shell:** a 200 returning the app shell is not private data. Compare response bodies semantically with owned canaries.
- **Header / cookie confusion:** middleware trusts a header the application also consumes; test single-vs-batch, frontend-vs-backend, signed-vs-unsigned fields.

Primary research lead: Vercel May 2026 security release (13 advisories spanning middleware/proxy bypass, SSRF, cache poisoning, XSS, DoS). Check `https://vercel.com/changelog/next-js-may-2026-security-release` and the Next.js advisory index at execution time; never treat an unreviewed release list as proof the target is affected.

## 4. HTTP parser / cache disagreements

Map client→edge and edge→origin separately. HTTP/2 at the edge does not establish HTTP/2 upstream. Model framing, normalization, and connection reuse at each hop.

Desynchronization needs an isolated lab or explicitly permitted test channel — never shared queues or other users' traffic. Distinguish timeout, ordinary rejection, and connection closure from a confirmed desync. Reproduce on an owned connection with a negative control; retain exact request bytes and negotiated protocols. High-level clients may normalize away the condition.

Cache entanglement (PortSwigger research, updated January 2026): treat cache-related issues as genuine security issues when there is an isolated cross-user effect. A shared-cache poisoning test requires explicit permission for the shared effect; otherwise use a unique controlled marker and an isolated owned cache key.

## 5. GraphQL

Authorization must be checked per resolver/field/object/batch/version — schema introspection or availability alone is not a finding. Compare authorized vs. unauthorized access to the same field, object, batch operation, and API version. GraphQL does not change the actor/tenant/object invariant: actual unauthorized data or state is required.

## 6. Deserialization and unsafe interpreters

Test only where a deserialization sink or interpreter is observed and reachable. A generic deserialization PoC (e.g., a standalone gadget chain) validates a mechanism, not the target. Reproduce in the actual affected revision with exact triggering sequence, or state precisely what remains inferred.

## 7. Cloud metadata SSRF

Server-side fetch or open redirect can reach cloud metadata (`169.254.169.254`), private IPs, localhost, or internal services. Separate DNS resolution, HTTP fetch, and access to a protected resource. Use a unique controlled marker; exclude scanner, browser, and mail-preview callbacks. Block private and reserved IP ranges at the client; do not manually validate IP addresses (encoding tricks defeat hand-rolled parsers).

## 8. Client-side and supply chain

Frontend bundles reveal interfaces but do not contain every endpoint. Missing upload/SSRF strings in three bundles means only that those searches found no candidate — never convert that into "no surface exists." `.git`, `.env`, CI/CD, IaC state, and dependency confusion are supply-chain leads; validate reachability and authorization before testing.

## 9. False-positive controls (apply before reporting)

Reject a candidate when any of these hold and the benign explanation fits:

- Reflection without verified script execution / data consequence.
- CORS `*` without credentialed private-data access.
- GraphQL introspection without auth bypass.
- Missing headers without demonstrated exploit.
- Version disclosure without a known applicable CVE and observed prerequisites.
- Public data shown to a logged-in user.
- Self-XSS, clickjacking on non-sensitive pages, best-practice headers with no impact.
- Accepted upload with no forbidden consequence (renamed file is insufficient).
- A WAF block followed by an allowed response, without proof the bypass reached a protected boundary.
- A scanner match without controlled reproduction.
- Timing differences without interleaved controls and repeated samples.
- Public-by-design behavior, same-owner access, stale session, cache hit, login-redirect body, synthetic response, unreachable code, compensating control, or impossible prerequisite.

## 10. Experiment card for any modern surface

```
Claim / strongest benign explanation:
Primary source and retrieval date:
Asset, scope, and policy:
Actor, tenant, role, disposable fixture:
Expected security invariant:
Baseline / one changed variable / negative control:
Observation that supports / refutes:
Request/concurrency budget and stop condition:
Tool artifact paths, UTC timestamps, account labels, marker:
Observed data or persistent effect / repeatability:
State: untested | testing | supported | refuted | inconclusive | blocked
Next discriminating experiment / cleanup:
```

Stop an individual proof when the requested consequence is demonstrated with minimal evidence. Continue remaining independent coverage. Never claim a technique is "2026-level" merely because it was published in 2026; applicability must be observed on the target.
