---
name: web-app-testing
description: "Web, API, GraphQL, and browser testing manual: network request discipline, authenticated session mapping, state-machine completion, differential response diffing, API surface inspection, mass assignment, HTTP method override, GraphQL authorization testing, browser automation, and web research discipline. Load when testing an authenticated web application, API, internal service, or GraphQL surface."
auto-invoke: false
---

# Web Application, API, and GraphQL Testing

This manual carries the detailed web/API/GraphQL/browser testing rules that used to live in the always-on system prompt. Load it with `skill_manage` (action=load, name=web-app-testing) when the target is a web application or API. Coordinate with `bugbounty` class skills (`hunt-api`, `hunt-graphql`, `hunt-idor`, ...), `recon-ops` for discovery, and `finding-validation` before claiming a finding.

# Network Operations

Network requests are tools for gathering evidence.

## Reach for the network tools yourself

Do not wait for the user to ask for HTTP probing, scraping, or browser interaction. When a task needs to observe or change the state of a live endpoint, reach for the relevant tool on your own initiative.

Pick the lightest tool that answers the question:

| Need | Tool |
| --- | --- |
| Fetch a URL, read docs, list research | `webfetch` |
| Search the web for current information | `websearch` |
| One-off HTTP request with custom headers, method, or body | `curl` |
| Status/header/liveness check across many hosts | `httpx` |
| Parse and query structured data from a page | `scrapling` |
| JavaScript-rendered pages, real interaction, session state | `browser` |
| Capture, modify, replay a request | `httpflow` |
| Differential response diffing | `httpflow` + `diff`, or a scripted harness |

Use `curl` by default for direct HTTP work — it handles arbitrary methods, headers, and bodies in one call. Reach for `browser` only when the answer depends on JavaScript execution, cookies, or real interaction. Reach for `scrapling` when the job is extraction rather than one-off inspection.

If a tool is unavailable, report that plainly and continue with the closest alternative instead of stopping to ask.

## Request discipline

For a scoped assessment:

- Send requests.
- Replay requests.
- Modify parameters.
- Compare responses.
- Follow redirects correctly.
- Test authentication.
- Test authorization.
- Enumerate accessible APIs.
- Use browser automation where useful.
- Capture request/response evidence.
- Record timing and relevant state changes.

Use the lowest-impact experiment that can establish the required security property.

Prefer bounded requests, controlled concurrency, and minimal payloads when they provide equivalent evidence.

Do not generate unnecessary traffic. Do not use a destructive production test where an isolated reproduction or safer differential test can establish the same point.

For potentially disruptive tests, identify the expected observable outcome and recovery path before executing the test.

A live system's reachability does not itself authorize every operation against it.

---



---

# Web Research

For current information:

- Use available web tools.
- Prefer authoritative sources.
- Follow primary documentation.
- Verify important current claims.
- Check dates and version applicability.
- Distinguish official policy from community commentary.
- Cite relevant sources in user-facing research summaries.
- Avoid outdated assumptions when current information is available.

For bug bounty research, inspect the current program scope, exclusions, and testing rules before extensive active testing and before recommending submission.

When reviewing policies, distinguish:

1. General platform guidance.
2. Program-specific rules.
3. Asset-specific scope.
4. Testing-method restrictions.
5. Impact-specific exclusions.
6. Actual triager feedback.

Never invent a policy clause or assume that a general guideline fully describes a particular program's rules.

---


---

# Web Application Testing

For an authenticated web application, establish:

- Current test identity.
- Current organization or tenant.
- Roles.
- Account identifiers.
- Relevant resources.
- API endpoints.
- Session state.
- CSRF mechanisms.
- Authorization model.

Map:

`browser → frontend → API → backend → storage`

Look for security assumptions at every boundary.

Do not limit research to visible UI functionality. Backend APIs are part of the application.

## State-machine completion rule

A redirect after a state-changing POST does not necessarily establish that the workflow completed.

1. Capture the response and `Location` header.
2. Determine the expected next state.
3. Follow the documented flow using the appropriate session and method.
4. Avoid unintentionally replaying state-changing requests.
5. Inspect the actual terminal state.
6. Verify that the state change occurred once and as intended.

Do not blindly use redirect-following behavior when it may replay a request body or cause duplicate side effects.

A workflow is complete only when its terminal state is observed.

## Differential testing

Compare equivalent operations across:

- Roles.
- Accounts.
- Tenants.
- Resource owners.
- HTTP methods.
- API versions.
- Frontend and backend paths.
- Direct and indirect access paths.

Change one variable at a time when possible so the cause of a difference remains clear.

## Response diffing discipline

For every differential test:

- Capture status, length, headers, body hash, and timing for both responses.
- Use `diff` or a scripted harness. Do not eyeball.
- Note the exact differences. A different `Content-Length` may mean a leaked object or a different error page — inspect.
- If responses are identical, the test is negative for that variable; move to the next.
- If responses are different, the *nature* of the difference tells you what changed on the server side.

---

# API Testing

For each meaningful API surface, inspect:

- Authentication.
- Authorization.
- Object identifiers.
- User identifiers.
- Organization identifiers.
- HTTP methods.
- Content types.
- Query parameters.
- Body parameters.
- Headers.
- Pagination.
- Filtering.
- Sorting.
- Export functions.
- Bulk operations.
- GraphQL queries.
- GraphQL mutations.
- Rate-limit behavior.
- Error handling.
- Version compatibility.
- Alternate content types (JSON, XML, form-encoded, multipart).
- HTTP method override headers.

Compare requests that should be equivalent.

Investigate requests that should not be authorized.

Prefer test-owned objects and controlled account comparisons when they establish the same property without accessing unrelated private data.

## Mass assignment

Look for endpoints that accept a JSON body and write fields the UI did not send:

- `is_admin: true`, `role: "admin"`, `verified: true`, `balance: 999999`, `credits: 999`.
- `created_at`, `updated_at` — writable timestamps.
- `tenant_id`, `org_id` — cross-tenant move.
- `email_verified_at` — bypass email verification.
- Nested objects — `profile: { is_admin: true }`.

Test by adding one field at a time to a baseline request and diffing the response and the resulting object state.

## HTTP method override

- `X-HTTP-Method-Override: PUT`, `_method=PUT`, `X-Method-Override: DELETE`.
- Hidden form fields with `_method`.
- Query string `?_method=PUT`.

If the route guard is method-specific, the override may bypass it.

---


---

# GraphQL Testing

For GraphQL:

- Enumerate the schema where permitted (`__schema`, `__type`).
- Identify queries.
- Identify mutations.
- Identify object identifiers.
- Identify resolver-level authorization.
- Test alternate IDs using suitable test resources.
- Test nested object access.
- Test field-level authorization.
- Test mutations across ownership boundaries.
- Test aliases and fragments.
- Test batching behavior (array of queries in one request).
- Test tenant boundaries.
- Inspect pagination and resource-consumption controls.
- Compare frontend restrictions with resolver enforcement.
- Decode Node IDs (often base64 of `Type:ID`) and test cross-type access.
- Test introspection in production (often disabled, sometimes not).
- Test `@include`/`@skip` directives for auth bypass.
- Test query depth and complexity limits.
- Test subscription authorization (WebSocket).

Do not assume frontend authorization equals GraphQL authorization.

A successful query is not automatically a vulnerability. Establish the expected permission boundary and demonstrate a violation.

---


---

# Browser Automation

Browser automation is an execution tool, not merely a visualization tool.

When useful:

- Log in using an authorized test session.
- Navigate the application.
- Capture network requests.
- Inspect DOM behavior.
- Exercise functionality.
- Trigger state transitions.
- Compare client and server behavior.
- Reproduce security issues.
- Capture evidence.
- Verify the terminal state.
- Run JS in page context to test DOM XSS, postMessage, and client-side sinks.
- Intercept and modify `fetch`/`XHR` calls.
- Test client-side storage (localStorage, sessionStorage, IndexedDB).

Use the runtime's browser capabilities when available.

Do not stop merely because a task requires interactive state.

Keep session state isolated between test accounts. Avoid recording credentials or unrelated private data in artifacts.

---
