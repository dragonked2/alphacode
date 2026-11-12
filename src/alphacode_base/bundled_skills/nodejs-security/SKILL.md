---
name: nodejs-security
description: Authorized Next.js and Node.js security review covering routing, Server Actions, RSC, caching, proxy trust, backend input, runtime exposure and dependency reachability.
auto-invoke: false
---

# Next.js and Node.js security review

Read scope before probing. In a source review, check the lockfile, runtime or
image manifest, routes, middleware and deployment configuration. Record the
resolved Next.js, React, Node.js and middleware versions separately. In a
black-box review, corroborate fingerprints and mark unavailable versions
unknown. Identify the router, deployment adapter, runtime, enabled features
and edge-to-origin path. Check advisory prerequisites from primary sources
and reproduce the relevant effect on an authorized fixture.

## Next.js

| Surface | Discriminating test |
| --- | --- |
| Middleware or Proxy bypass, rewrites and path normalization | Map matcher exclusions and normalization at the edge and origin. Compare canonical and alternate paths to the same owned object across actors with a missing-object control. A redirect change or application shell does not establish access. |
| Route Handler method confusion | Compare the normal method, unexpected methods, and any observed method-override path. Check authorization per handler. Verify a private fixture read or independent mutation read-back. A generated HEAD or OPTIONS response and a different status are intermediate evidence. |
| Server Actions | Trace authentication, ownership and validation at the action or data-access call. Compare an ordinary action from the same build under denied and entitled actors and verify independent read-back. An action ID or a hidden UI does not establish authorization. |
| RSC, prefetch, Pages Router data and serialized props | Compare document, data and navigation representations from the same build and route. Use only transport headers or markers observed in that deployment. For disclosure verify the exact protected fixture field. For payload tampering or unsafe decoding verify server-side acceptance and its controlled effect. Layout data, redaction flags and parser errors are leads. |
| Static generation, revalidation, data and CDN caches | Identify the cache layer and vary by actor, tenant, representation and locale. Use an isolated key, separate sessions and a clean retrieval. Distinguish client navigation cache from a shared cross-actor leak. |
| Client-side exposure from environment variables and build artifacts | Trace the value into a client bundle, source map, RSC payload or page data. Check build-time NEXT_PUBLIC_ variables, next.config env and data passed to client components. Verify that the exact exposed value was meant to be private. Public settings and identifier names do not establish disclosure. |
| SSRF through Route Handlers, Middleware or Proxy | Trace URL input in previews, webhooks, rewrites and server-side fetches. Verify destination and redirects with a unique nonce at an authorized receiver. Distinguish DNS, backend retrieval and response access. Test private network reachability only with an explicitly authorized controlled destination. |
| Image optimization SSRF and resource exhaustion | Inspect the deployed image loader, remotePatterns and redirect behavior. Compare a permitted image with a disallowed controlled destination and verify the server fetch independently. Evaluate decode or resize resource cost with bounded fixtures in an isolated environment. An accepted URL or large input alone is insufficient. |
| Rendering sinks and upload serving | Verify a browser sink under the target origin and policy, or verify that a stored fixture is served in the affected execution context. Reflection or upload acceptance is intermediate evidence. |

## Node.js

| Surface | Discriminating test |
| --- | --- |
| Express, Fastify, Koa and Nest routes | Map registration order, prefixes, guards and middleware scope. Compare independent actor controls and verify the protected read or settled state. |
| Reverse proxy trust | Establish the proxy hops and where client-supplied forwarded values are overwritten. Verify a trust-dependent authorization, redirect, origin or accounting effect. Header reflection is intermediate evidence. |
| Prototype pollution | Trace controlled keys through recursive merges or property setters, including relevant dependency versions. Compare a fresh isolated process with a benign input. Verify an inherited property reaches a meaningful consumer before claiming escalation or execution. |
| Command and argument injection | Identify child_process.exec, execFile or spawn, the shell setting, the platform and the downstream argument parser. Compare inert input with a harmless controlled effect. Separate shell interpretation from argument injection and ordinary output. |
| NoSQL injection | Trace request JSON into MongoDB filters, query builders and any enabled server-side expressions. Compare literal values with controlled operator-shaped inputs such as $ne, $gt, $regex or $where where supported. Verify a known fixture is returned or an actual authentication decision changes. |
| Template injection | Trace input into EJS, Pug, Handlebars or another engine. Distinguish rendering a variable from compiling attacker-controlled template source. Compare an inert literal with a harmless evaluated expression and verify the affected server or browser context. |
| Unsafe deserialization and evaluation | Identify the decoder and reachable callbacks, object reconstruction or executable sink. Plain JSON.parse decodes data. Show what consumes that data afterward, or prove unsafe evaluation or a serializer gadget with a harmless control. Parser failures cannot establish execution. |
| Path traversal and unsafe file serving | Trace fs.readFile, sendFile, static serving, archives and module loading through decoding and canonicalization. Verify a known permitted canary outside the intended root, including symlink behavior when applicable. A path error alone is insufficient. |
| Webhooks, URL clients and realtime channels | Compare authentic and tampered inputs or permitted and denied identities. Verify settled state, a correlated callback, or an unauthorized subscription payload using separate sessions. |
| Insecure dependencies | Use the lockfile and deployed artifact to verify the resolved npm dependency and runtime reachability. Treat npm audit and scanner output as leads. Verify the advisory prerequisites and reproduce the affected behavior in an authorized fixture. |
| Inspectors and runtime configuration | For management interfaces, verify actual access with non-mutating evidence. Banners and open ports are leads. |
| Event loop, HTTP parsers and resource limits | Use a local or explicitly authorized isolated fixture and bounded inputs. Record resource use and healthy control requests. Verify which layer experiences the effect. |

Apply the relevant controls in the bug bounty control-verification reference.
Record the strongest demonstrated impact and the prerequisites still unverified.
For a CTF, validate the recovered flag or required state against the challenge.

## Primary references

- [Next.js data security](https://nextjs.org/docs/app/guides/data-security)
- [Next.js authentication](https://nextjs.org/docs/app/guides/authentication)
- [Next.js Proxy](https://nextjs.org/docs/app/getting-started/proxy)
- [Next.js environment variables](https://nextjs.org/docs/15/app/guides/environment-variables)
- [Next.js image component](https://nextjs.org/docs/pages/api-reference/components/image)
- [Next.js security advisories](https://github.com/vercel/next.js/security/advisories)
- [Node.js security practices](https://nodejs.org/learn/getting-started/security-best-practices)
- [Node.js security releases](https://nodejs.org/en/blog/vulnerability)
- [Node.js child process API](https://nodejs.org/api/child_process.html)
- [Express behind proxies](https://expressjs.com/en/guide/behind-proxies/)
- [MongoDB query methods](https://www.mongodb.com/docs/manual/reference/method/db.collection.find/)
- [npm audit reports](https://docs.npmjs.com/about-audit-reports/)
