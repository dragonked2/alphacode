---
name: recon
description: Attack surface discovery — fingerprint first, then subdomains, DNS, tech detection, directory fuzzing, JS analysis, inventory building. Native tools first, websearch never for enumeration.
---

# RECON — FINGERPRINT FIRST, THEN ENUMERATE

**Recon is done when you know exactly what to test and in what order.**

Order is mandatory: SCOPE → TOOLS → FINGERPRINT → JS BUNDLE →
SUBDOMAINS → HOSTS → DIRECTORIES → INVENTORY.

---

## 0. SCOPE AND TOOLS (ALWAYS FIRST)

1. Write the scope file (see scope skill). No scope file → no testing.
2. Check tools ONE AT A TIME (see tool-doctor). A missing tool never
   stops recon — switch to its listed fallback immediately.
3. NEVER use websearch to enumerate subdomains. Search engines hit
   anti-bot challenges and return junk. Enumeration is a DNS job:
   subfinder → assetfinder/amass → crt.sh via the httpflow tool.

---

## 1. FINGERPRINT (5 min — BEFORE any wordlist)

Identify headers, server, framework, and WAF with small filtered calls:

```bash
curl -sI --max-time 20 "https://TARGET/" | head -30
curl -s --max-time 20 "https://TARGET/" | head -c 2000
curl -s --max-time 20 "https://TARGET/robots.txt" | head -40
curl -s --max-time 20 "https://TARGET/.well-known/security.txt" | head -20
```

Then test FRAMEWORK-SPECIFIC routes, not generic wordlists:

```
Next.js  → /_next/static/, /_next/data/<build>/, __NEXT_DATA__, JS chunks (see recon-js)
Nuxt     → /_nuxt/, __NUXT__
Django   → /admin/, csrfmiddlewaretoken patterns
Express  → standard Express routes, X-Powered-By
GraphQL  → /graphql, /api/graphql (introspection only if in scope)
Swagger  → /swagger, /api-docs, /openapi.json
```

Generic `/api/v1/health`-style guessing is the LAST resort: if three
framework-specific guesses 404, stop guessing blindly and go back to
the JS bundle (recon-js).

---

## 2. SUBDOMAINS — PASSIVE, NATIVE TOOLS FIRST

```bash
subfinder -d TARGET -all -o subfinder.txt
```

Fallback chain (first available wins):

1. `assetfinder --subs-only TARGET`
2. `amass enum -passive -d TARGET`
3. crt.sh via the httpflow tool (`action: request`,
   `url: https://crt.sh/?q=%.TARGET&output=json`), parse
   `name_value` fields — no jq required.
4. DNS resolve everything before probing (next section).

Immediately filter against the scope file (see scope skill) BEFORE
any resolution or probing:

```bash
grep -v -E '^(help|explorer|community)\.' subs.txt > subs-in-scope.txt
```

(Replace the exclusion pattern with the assessment's OUT-OF-SCOPE list.)

Windows notes: prefer `nslookup` or PowerShell `Resolve-DnsName`
when dnsx is missing; check tools with `Get-Command <tool>`
(PowerShell) instead of `command -v`.

---

## 3. RESOLVE, THEN PROBE (ACTIVE)

```bash
dnsx -l subs-in-scope.txt -a -aaaa -cname -resp -o resolved.txt
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects -o alive.txt
```

Fallbacks: `nslookup`/`Resolve-DnsName` for DNS; `curl -s -o
/dev/null -w "%{http_code} %{url_effective}\n"` for probing.
Cap output (`head -100`), filter at fetch time (`--max-time 20`).
A 170KB response teaches nothing — re-fetch narrowly.

---

## 4. DIRECTORIES AND APIS

- Framework-derived paths first (from sections 1 and recon-js).
- Then `ffuf -u https://TARGET/FUZZ -w common.txt -mc 200,301,302,403`.
- API surface: `katana -u TARGET -d 3 -jc`, keep `/api/` hits only.
- Sensitive files: `.env`, `.git/HEAD`, backups — one request each,
  stop at the first definitive answer per hypothesis (see runbook
  kill rules).

---

## 5. INVENTORY OUTPUT

End recon with a ranked inventory, not raw tool output:

```
Endpoint | Auth | State-Changing | Priority | Source
/api/v1/users | Yes | Yes | HIGH (IDOR) | bundle
/graphql | Yes | Yes | HIGH (rich surface) | bundle
/admin | Yes(admin) | Yes | CRITICAL (privilege) | ffuf
/auth/reset | No | Yes | HIGH (password reset) | fingerprint
```

`Source` must be `bundle` or `fingerprint` for at least the top
entries — if every row says `wordlist`, redo sections 1 and recon-js.
