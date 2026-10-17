---
name: writeup-corpus
description: Map of the 0xNotNull Medium writeup lists (bug-bounty-critical, BAC & logic, XSS, SQLi, OTP, API, info disclosure...) to the matching alphacode hunt-* skill for each topic, with the gaps they were distilled to fill.
---

# WRITEUP CORPUS → SKILL MAP

**Core:** The 20 publication lists by 0xNotNull on Medium (~3200 curated writeups across topics) were mined for the recurring techniques. Each topic maps to the alphacode skill that carries its distilled methodology; topic gaps became new skills. When hunting a specific bug class, load the matching skill here instead of rediscovering the technique from scratch.

| List (stories) | alphacode skill(s) |
|---|---|
| OTP bypass / MFA / 2FA (149) | `hunt-otp` (+ `hunt-race`, `hunt-auth-bypass`) |
| Authentication bypass (418) | `hunt-auth-bypass`, `hunt-idor`, `hunt-jwt`, `hunt-oauth` |
| Business logic / BAC (620) | `hunt-business-logic`, `hunt-idor`, `hunt-auth-bypass`, `hunt-realtime` |
| XSS — client-side (145) | `hunt-xss`, `hunt-cspt`, `hunt-prototype-pollution`, `hunt-storage` |
| Info disclosure / JS recon (305) | `recon-js`, `hunt-apikey-leak`, `hunt-open-redirect` |
| SQL injection (82) | `hunt-sqli`, `hunt-graphql` |
| Race conditions (80) | `hunt-race`, `hunt-realtime` |
| API / GraphQL (71) | `hunt-api`, `hunt-graphql` |
| Cache poisoning (44) | `hunt-cache-poisoning`, `hunt-headers`, `hunt-desync` |
| Path traversal / file upload (39) | `hunt-path-traversal`, `hunt-storage` |
| Open redirect (38) | `hunt-open-redirect`, `hunt-csrf` |
| CSRF (116) | `hunt-csrf`, `hunt-otp` |
| SSRF (116) | `hunt-ssrf`, `hunt-ssrf-advanced`, `hunt-desync` |
| CORS (14) | `hunt-cors` |
| Host header / Cache (6) | `hunt-headers`, `hunt-cache-poisoning` |
| JWT (8) | `hunt-jwt`, `hunt-deserialization` |
| XEE / XXE (4) | `hunt-xxe` |
| CRLF / CRLFuzz (6) | `hunt-crlf` |
| Desync / smuggling (44) | `hunt-desync`, `hunt-ssrf-advanced` |

## THE THREE GAPS → the skills distilled from them
- **OTP (149 stories, no alphacode skill existed)** → `hunt-otp`: code value, comparison, flow-state, rate-limit classes.
- **Auth bypass (418 stories)** → `hunt-auth-bypass`: credential, logic, token, and authz-differential classes.
- **CSPT (3 stories, almost undocumented)** → `hunt-cspt`: client-side path traversal recon, confirmation, and pollution-gadget pairing.

## KNOWING WHAT ALREADY-EXISTS COVERS
The remaining topics were comprehensive in alphacode already (see the table) — do not build new material for them; chain the existing skills per the Pairs columns above.

## METHOD NOTES (how the corpus was mined)
- Each list is essentially "curated writeups per bug class". The recurring *causal patterns* (not individual idiosyncrasies) were extracted: e.g. OTP stories repeatedly fall into "code value manipulation", "response tampering", "flow confusion", "limits only guarded one axis" — hence the four attack surfaces in `hunt-otp`.
- Keep this map updated when new hunt skills land; the value is the *pairing*, not the story counts.
