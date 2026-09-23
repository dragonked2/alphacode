---
name: credential-attack
description: Password spray methodology for bug bounty — when to do it vs web-vuln hunting, the wordlist-gen + breach-check + osint-employees + spray pipeline, mode selection (http-form / oauth / o365 / okta), rate-limit + lockout tactics, legal guardrails, success detection, and the spray → authenticated /hunt chain pattern.
---

# CREDENTIAL ATTACK PIPELINE

Real-world initial-access vector. Verizon DBIR consistently ranks Stolen Credentials in the top 3 incident types. Most BB hunters skip this because they only try `rockyou.txt` and get rate-limited.

**Core principle:** humans pick lazy passwords. `{CompanyName}{Year}!`, `{ProductName}{Season}`, `{City}123`. Harvesting company-specific vocabulary before spraying is what makes the hit-rate go from 0.01% to 1%+.

---

## WHEN TO RUN CREDENTIAL ATTACK

Credential attack is a **parallel branch** to `/hunt`, not a replacement:

```
/recon ──┬──▶ /hunt (web vuln scan) ──┐
         │                             ├──▶ /validate ──▶ /report
         └──▶ /wordlist-gen → ... → /spray ──┘
```

**Run it when:**
- Target has a discoverable login endpoint (web form / O365 / Okta / OAuth)
- Program scope **explicitly permits** authentication testing or credential testing
- You can stomach a 30-min-to-multi-hour run (with conservative defaults)

**Skip it when:**
- Program policy lists "credential stuffing", "brute force", or "password attacks" as out-of-scope
- Target only has SSO via a provider you don't control
- Login endpoint is rate-limited so aggressively that even 1 attempt/30min triggers alerts

**KILL signals (don't even start):**
- No login surface in recon output
- WAF (Cloudflare with Bot Management, Akamai) on every auth endpoint
- Program runs an active red-team — they'll see your spray immediately
- You don't have a clean wordlist yet (running rockyou.txt is a waste of lockouts)

---

## THE 4-STAGE PIPELINE

```
/wordlist-gen ──▶ /breach-check ──▶ /osint-employees ──▶ /spray
 (company words)    (rank by HIBP)    (real usernames)    (live attempts)
```

### Stage 1 — Wordlist Generation

Crawls the target website with `cewler`, deduplicates, applies hashcat rules to mutate.

**Mode selection:**

| Mode | Rules | When |
|------|-------|------|
| `minimal` | top10_2025 (10 rules) | Cautious spray, paranoid program |
| `balanced` *(default)* | best66 (66 rules) | Standard — best signal/noise |
| `aggressive` | OneRuleToRuleThemAll (52k) | **Offline cracking only**, NOT spray |

**Filter selection:**

| Filter | When |
|--------|------|
| `strict` *(default)* | API-doc-heavy sites. Drops CSS hex colors, URL slugs, random API tokens |
| `loose` | Marketing sites without API examples |

### Stage 2 — Breach Check

Sends only first 5 chars of SHA-1 to HIBP (k-anonymity). **Free, no API key, full passwords never leave your machine.**

**Breach-count interpretation:**

| Range | Meaning | Spray strategy |
|-------|---------|----------------|
| **0** | Never leaked | Could be company-specific OR truly random |
| **1-1000** | "Sweet spot" — proven human use, not yet in every spray list | **Prioritize** |
| **1k-1M** | Mainstream | Usually already tried by previous attackers |
| **>1M** | Generic (`password`, `123456`) | Skip — every WAF expects these |

### Stage 3 — OSINT Employee Enumeration

`theHarvester` (search engines + CT logs) → derive names from email local-parts → `username-anarchy` permutations.

**Realistic expectations:**

| Target type | Expected emails | Expected names |
|-------------|----------------|----------------|
| US/EU SaaS | 5-50 | depends |
| State utility | **0** | 0 |
| Local SME | 0-10 | 0-5 |

### Stage 4 — The Spray

**Mode selection:**

| Mode | Use case | Engine |
|------|----------|--------|
| `http-form` | Custom login page (most BB targets) | Pure Python urllib |
| `oauth` | OAuth password grant (`grant_type=password`) | Pure Python urllib |
| `o365` | Microsoft 365 / Azure AD | `trevorspray` |
| `okta` | Okta SSO | `trevorspray` |

**Hard guards (no override possible without `--i-understand`):**
1. **Typed-hostname confirmation** — you must type the target hostname back
2. **Lockout warning** — calculates per-user failed-attempt count
3. **Audit log JSONL** — every attempt logged (passwords as SHA-256 prefix only)
4. **Spray order** — `pass[i] × all_users` per round (not brute per-user)

---

## SPRAY ORDER — WHY IT MATTERS

```
WRONG (brute-force order, will lockout):
  alice: pass1, pass2, pass3, ...  ← alice gets 8 failed attempts in seconds
  bob: pass1, pass2, pass3, ...

RIGHT (spray order, distributes failures):
  Round 1: pass1 → alice, bob, charlie (1 failed each)
  [delay 30 min]
  Round 2: pass2 → alice, bob, charlie (2 failed total each)
```

Default rate-limit: `--delay 1800 --jitter 60` (30 min/round + ±60s).

---

## SUCCESS DETECTION

### http-form mode (checked in order):
1. `--success-regex <body-regex>` matches → success
2. `--fail-regex` set AND body does NOT match → success
3. HTTP redirect to non-login path → success (heuristic)
4. **Always supply `--fail-regex "Invalid|incorrect|wrong"` for production sprays**

### oauth mode:
- HTTP 200 with `"access_token"` in JSON → success
- HTTP 4xx → fail (unambiguous)

---

## CHAIN PATTERN: SPRAY → AUTHENTICATED /HUNT

```
/spray finds valid creds (low-payout finding by itself)
↓
Re-run /hunt with the session cookie or bearer token
↓
Authenticated /hunt sees admin pages, internal APIs, IDOR on user data
↓
Find a P1/P2 IDOR or business-logic bug behind the login wall
↓
Chain report: "ATO via spray + IDOR exposes all user PII" (high payout)
```

The spray-only finding alone is **usually rejected** by mature BBPs. The chain is what pays.

---

## LEGAL GUARDRAILS

Before running `/spray` against ANY target, verify:
1. **Program policy explicitly allows credential testing**
2. **The wordlist does not contain plaintext breach data** (HIBP hash-prefix is fine; plaintext breach corpus is not)
3. **Stop on first hit by default**
4. **Report the lockout impact** with timestamps from the audit log

---

## OPERATIONAL CHECKLIST

Before pressing enter on `/spray`:
- [ ] `/scope <login-host>` returns IN SCOPE
- [ ] Program policy reviewed for credential-testing rules
- [ ] Wordlist filtered (`--filter strict`) and HIBP-ranked (`--max-count 1000000`)
- [ ] Usernames file has REAL usernames (from OSINT)
- [ ] Default delay (`--delay 1800 --jitter 60`) unless program permits faster
- [ ] `--dry-run` passed once to verify post-data template

During spray:
- [ ] Monitor audit log for HTTP 429 / 503 / response-time spikes
- [ ] If status codes get weird → assume detection and abort

After spray:
- [ ] If hit: STOP, document the find, do NOT log in further
- [ ] If lockouts likely happened: notify program with audit log timestamps
