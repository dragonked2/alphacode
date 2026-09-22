---
name: recon-js
description: Client-side code analysis — download JS bundles and extract the API endpoints, GraphQL operations, and secrets that network scanning cannot see. Use on any SPA (Next.js, Nuxt, React, Angular, Vue) BEFORE generic endpoint wordlists.
---

# RECON-JS — THE BUNDLE IS THE SOURCE OF TRUTH

**Generic wordlists produce 90%+ 404s on SPAs. The JS bundle lists the
real endpoints. Analyze it first, fuzz second.**

---

## 1. ORDER OF OPERATIONS

1. Confirm the app is an SPA (view-source shows a `<div id="__next">`,
   `__NUXT__`, or a single root div plus script tags).
2. Download the bundles (section 2).
3. Extract endpoints, operations, and secrets (section 3).
4. Map them to testable targets (section 4).
5. THEN fall back to wordlists for what the bundle did not reveal.

Never skip to step 5. A 10-minute bundle pass beats an hour of 404s.

---

## 2. DOWNLOAD THE BUNDLES

```bash
# List chunk URLs from the page (works for Next.js / Nuxt / generic SPAs)
curl -s --max-time 20 "https://TARGET/" | grep -oE 'src="[^"]+\.js[^"]*"' | head -50

# Next.js: chunks live under /_next/static/chunks/ — fetch each one
curl -s --max-time 20 -o chunk.js "https://TARGET/_next/static/chunks/<file>.js"

# Embedded data: Next.js serializes page props into the HTML itself
curl -s --max-time 20 "https://TARGET/" | grep -oE '__NEXT_DATA__.*' | head -c 4000
```

Keep every response small: `head -c`, `head -50`, `--max-time 20`.
A 170KB header dump teaches nothing — filter at fetch time.

---

## 3. EXTRACTION PATTERNS

Run these against each downloaded chunk (and the HTML):

```
# API routes and fetch calls
/api/[a-zA-Z0-9/_${}.-]+
fetch\(["'`]/[^"'`]+
axios\.(get|post|put|delete|patch)\(["'`]/[^"'`]+

# GraphQL: operation names, mutations, schema hints
(mutation|query)\s+[A-Za-z0-9_]+
graphql|/graphql|__schema|__typename

# Next.js data routes (directly fetchable JSON)
/_next/data/<build-id>/<page>.json

# Secrets and keys (flag for hunt-apikey-leak, do not exfiltrate)
api[_-]?key|secret|token|password|client_secret
AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]+|xox[bap]-|sk_live_

# Source maps (full original source when left deployed)
\.js\.map
```

One pattern per pass, capped output (`sort -u | head -100`). Record
hits in the assessment notes with the chunk they came from.

---

## 4. TURN HITS INTO TARGETS

| Hit | Next step |
|-----|-----------|
| `/api/...` route | Probe directly (method, auth, IDOR — see hunt-api) |
| `/_next/data/<build>/...json` | Fetch it: exposes props/redirects without rendering |
| GraphQL operation | Send to /graphql (see hunt-graphql) |
| `__NEXT_DATA__` blob | Parse buildId, pageProps, runtimeConfig for env leaks |
| `.js.map` file | Download it: original source, comments, dead endpoints |
| Hardcoded key/token | Validate capability minimally, report via evidence-locker |

Every endpoint that came from the bundle outranks any wordlist guess.
Test bundle-derived endpoints first (see runbook target prioritization).

---

## 5. RULES

- Scope file applies to bundle URLs too (see scope skill).
- Do not publish or replay secrets beyond minimal capability proof.
- Save interesting chunks under `evidence/` with a manifest entry
  (see evidence-locker) — bundles rotate between deploys.
