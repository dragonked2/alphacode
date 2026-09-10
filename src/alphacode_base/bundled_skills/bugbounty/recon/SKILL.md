---
name: recon
description: Systematic attack surface discovery — build a complete inventory of web apps, APIs, GraphQL, WebSockets, mobile/backend APIs, admin interfaces, auth/OAuth flows, file upload/download, payment/financial, cloud integrations, webhooks, internal service boundaries, and cross-origin functionality. Prioritize state-changing and authorization-sensitive functionality. This is Phase 0 of the hunting workflow.
---

# RECONNAISSANCE — SYSTEMATIC ATTACK SURFACE INVENTORY

Elite-level reconnaissance focused on building an actionable inventory, not just collecting subdomains.

## CORE PRINCIPLE

> **Recon is not done when you have a list of subdomains. Recon is done when you know exactly what to test and in what order.**

---

## PHASE 0A: SCOPE DISCOVERY

Before touching the target, understand what you're allowed to test.

```bash
# 1. Read the program page thoroughly
#    - Which assets are in scope?
#    - Which bug classes are in scope?
#    - Are there exclusions?
#    - What's the severity range for payouts?

# 2. Check for policy files
curl -s https://target.com/.well-known/security.txt
curl -s https://target.com/robots.txt
curl -s https://target.com/sitemap.xml

# 3. Check for bug bounty platform
#    - HackerOne: hackerone.com/target
#    - Bugcrowd: bugcrowd.com/target
#    - Intigriti: intigriti.com/target
```

---

## PHASE 0B: PASSIVE RECONNAISSANCE

Gather intelligence without touching the target directly.

### Subdomain Enumeration (Multi-Tool)

```bash
# crt.sh (Certificate Transparency)
curl -s "https://crt.sh/?q=%.target.com&output=json" | jq -r '.[].name_value' | sort -u > crtsh.txt

# subfinder
subfinder -d target.com -all -o subfinder.txt

# amass (passive)
amass enum -passive -d target.com -o amass_passive.txt

# assetfinder
assetfinder --subs-only target.com > assetfinder.txt

# Combine and deduplicate
cat crtsh.txt subfinder.txt amass_passive.txt assetfinder.txt | sort -u > all_subs.txt
wc -l all_subs.txt
```

### DNS Intelligence

```bash
# DNS resolution with record types
dnsx -l all_subs.txt -a -aaaa -cname -mx -ns -txt -resp -o resolved.txt

# Check for interesting TXT records (SPF, DKIM, cloud verification)
dnsx -l all_subs.txt -txt -resp | grep -i "google\|microsoft\|amazon\|cloudflare\|vercel\|netlify"

# Check for CNAME pointing to third-party services (subdomain takeover candidates)
dnsx -l all_subs.txt -cname -resp | grep -i "herokuapp\|azurewebsites\|s3.amazonaws\|github.io\|shopify\|fastly\|pantheon\|surge.sh\|bitbucket\|wordpress.com"
```

### Certificate Transparency Deep Dive

```bash
# Find all subdomains from CT logs
curl -s "https://crt.sh/?q=%.target.com&output=json" | \
  jq -r '.[].name_value' | sort -u | \
  grep -v "\*" > ct_subs.txt

# Check for internal hostnames leaked in certificates
curl -s "https://crt.sh/?q=%.target.com&output=json" | \
  jq -r '.[].name_value' | sort -u | \
  grep -iE "internal|staging|dev|test|admin|api|mail|vpn|jenkins|gitlab|grafana|kibana|prometheus"
```

### Wayback Machine / URL Discovery

```bash
# Gather historical URLs
gau target.com | grep -vE "\.(css|js|png|jpg|gif|svg|woff|ttf|ico)" | sort -u > urls_gau.txt
waybackurls target.com > wayback.txt

# Find interesting endpoints from URL history
cat urls_gau.txt wayback.txt | sort -u | \
  grep -iE "admin|api|upload|download|export|import|backup|config|env|debug|test|internal|graphql|webhook"
```

### GitHub/GitLab OSINT

```bash
# Search for leaked secrets
# GitHub search queries:
#   "target.com" password
#   "target.com" api_key
#   "target.com" secret
#   "target.com" credentials
#   "target.com" AWS_ACCESS_KEY
#   "target.com" PRIVATE KEY

# Check for exposed repos
#   org:target.com
#   target.com filename:.env
#   target.com filename:docker-compose.yml
#   target.com filename:config.yml
```

### Shodan/Censys

```bash
# Shodan — find exposed services
shodan search hostname:target.com --fields ip_str,port,product,vulns

# Censys — certificate-based discovery
censys search "services.tls.certificates.leaf_data.names: target.com"
```

---

## PHASE 0C: ACTIVE RECONNAISSANCE

Touch the target to map the live attack surface.

### HTTP Probing

```bash
# Probe all resolved hosts
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects -o alive.txt

# Detailed probing with response analysis
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects \
  -content-length -content-type -web-server -cdn-name \
  -o alive_detailed.txt

# Categorize by status code
cat alive_detailed.txt | awk '{print $2}' | sort | uniq -c | sort -rn
```

### Technology Detection

```bash
# wappalyzer-cli or whatweb
whatweb https://target.com

# Check for specific frameworks
# Laravel: /_ignition/health-check, /.env
# Django: /__debug__/
# Spring Boot: /actuator/, /actuator/env
# Next.js: /_next/data/, __NEXT_DATA__
# WordPress: /wp-json/wp/v2/users, /xmlrpc.php
# Rails: /rails/info/properties, /sidekiq
```

### Directory/Endpoint Discovery

```bash
# ffuf with common wordlist
ffuf -u https://target.com/FUZZ -w /usr/share/wordlists/dirb/common.txt -o fuzz_common.json

# ffuf with API wordlist
ffuf -u https://target.com/api/FUZZ -w /usr/share/wordlists/seclists/Discovery/Web-Content/api/api-endpoints.txt -o fuzz_api.json

#ffuf with backend/admin wordlist
ffuf -u https://target.com/FUZZ -w /usr/share/wordlists/seclists/Discovery/Web-Content/backend-admin.txt -o fuzz_admin.json

# kiterunner for API discovery
kr scan https://target.com -w routes-large.kr -o kr_results.txt
```

### JavaScript Analysis

```bash
# Crawl with katana
katana -u target.com -d 3 -jc -o urls_katana.txt

# Find JS files
cat urls_katana.txt | grep "\.js$" | sort -u > js_files.txt

# Analyze each JS file for secrets and endpoints
for js in $(cat js_files.txt); do
  echo "=== $js ==="
  curl -s "$js" | grep -oiE "(api[_-]?key|secret|token|password|authorization|bearer|oauth|jwt|private[_-]?key)['\"]?\s*[:=]\s*['\"][^'\"]+['\"]" | head -5
  curl -s "$js" | grep -oE "/api/[a-zA-Z0-9/_-]+" | sort -u
done
```

---

## PHASE 0D: BUILD THE INVENTORY

After recon, produce a structured inventory. This is the output of recon — not the subdomain list.

### Inventory Template

```
INVENTORY — target.com
Date: [date]
Scope: [program scope URL]
═══════════════════════════════════════════════════════════════════

WEB APPLICATIONS
  [1] https://target.com — Main web app
      Tech: [detected framework/CMS]
      Auth: [session cookie / JWT / OAuth]
      Priority: HIGH (state-changing)

  [2] https://admin.target.com — Admin panel
      Tech: [detected]
      Auth: [admin credentials required]
      Priority: CRITICAL (privilege escalation target)

  [3] https://staging.target.com — Staging environment
      Tech: [detected]
      Auth: [may be weaker]
      Priority: HIGH (often less protected)

APIS
  [4] https://target.com/api/v1/ — REST API v1
      Auth: Bearer token
      Endpoints discovered: [N]
      Priority: HIGH (old version, may lack controls)

  [5] https://target.com/api/v2/ — REST API v2
      Auth: Bearer token
      Endpoints discovered: [N]
      Priority: HIGH

  [6] https://target.com/graphql — GraphQL
      Auth: Bearer token
      Introspection: [enabled/disabled]
      Priority: HIGH

  [7] wss://target.com/ws — WebSocket
      Auth: [token in handshake]
      Priority: MEDIUM

AUTHENTICATION/OAUTH
  [8] https://target.com/auth/login — Login
  [9] https://target.com/auth/signup — Registration
  [10] https://target.com/auth/reset — Password reset
  [11] https://target.com/auth/callback — OAuth callback
      OAuth provider: [Google/GitHub/custom]
      Redirect URI: [observed]
      Priority: CRITICAL (ATO target)

FILE HANDLING
  [12] https://target.com/api/upload — File upload
      Allowed types: [observed]
      Priority: HIGH (RCE/XSS/path traversal target)

  [13] https://target.com/api/export — Data export
      Priority: HIGH (data exfil target)

PAYMENT/FINANCIAL
  [14] https://target.com/api/checkout — Checkout
      Provider: [Stripe/Braintree/custom]
      Priority: CRITICAL (financial fraud target)

  [15] https://target.com/api/webhook — Payment webhook
      Auth: [signature/IP whitelist]
      Priority: CRITICAL (webhook abuse target)

CLOUD/INFRASTRUCTURE
  [16] S3 bucket: target-assets.s3.amazonaws.com
      Listing: [enabled/disabled]
      Priority: HIGH (data exposure)

  [17] CDN: cdn.target.com
      Priority: LOW

INTERNAL BOUNDARIES
  [18] https://internal.target.com — Internal service
      Auth: [may be IP-based only]
      Priority: HIGH (boundary crossing target)

CROSS-ORIGIN
  [19] CORS config: [reflects origin / allows credentials]
      Priority: MEDIUM (chained with data theft)
═══════════════════════════════════════════════════════════════════
Total endpoints: [N]
State-changing: [N]
Auth-sensitive: [N]
Priority 1 targets: [N]
═══════════════════════════════════════════════════════════════════
```

---

## PHASE 0E: LEAD GENERATION

From the inventory, generate testable leads for the hunt phase.

### Lead Priority Scoring

| Signal | Priority | Reason |
|--------|----------|--------|
| Auth-required endpoint (IDOR/BOLA potential) | HIGH | Authorization boundary to test |
| GraphQL endpoint | HIGH | Rich attack surface |
| Admin/debug endpoint | HIGH | Privilege escalation target |
| New feature (< 30 days old) | HIGH | Unreviewed code |
| Payment/billing endpoint | HIGH | Financial impact |
| Webhook endpoint | HIGH | Server-side request target |
| File upload endpoint | HIGH | RCE/XSS/path traversal |
| Old API version (/v1/) | HIGH | May lack security controls |
| Complex business logic | MEDIUM | Logic flaws |
| Standard CRUD endpoints | MEDIUM | IDOR candidates |
| Static assets / CDN | LOW | Low value |
| 403 on all paths | KILLED | WAF blocked |

### Lead Output Format

```
LEAD BOARD — target.com
═══════════════════════════════════════════════════════════════
ID   │ Priority │ Signal                      │ Route
═════╪══════════╪═════════════════════════════╪══════════════════
L001 │ HIGH     │ /api/v2/users/{id} (IDOR)   │ hunt-idor
L002 │ HIGH     │ /graphql (introspection)     │ hunt-graphql
L003 │ CRITICAL │ /api/checkout (payment)      │ hunt-api
L004 │ HIGH     │ /api/upload (file upload)    │ advanced-techniques
L005 │ HIGH     │ /auth/callback (OAuth)       │ hunt-oauth
L006 │ HIGH     │ /api/webhook (webhook)       │ hunt-ssrf
L007 │ MEDIUM   │ /api/v1/ (old version)       │ hunt-api
L008 │ HIGH     │ admin.target.com (admin)     │ hunt-idor
═══════════════════════════════════════════════════════════════
```

---

## RECON WORKFLOW (ORDERED)

```
1. Scope discovery        → understand what's allowed
2. Passive recon          → subdomains, DNS, CT, Wayback, GitHub
3. Active recon           → HTTP probing, tech detection, directory fuzzing
4. JavaScript analysis    → secrets, endpoints, API routes
5. Build inventory        → structured list of all assets
6. Generate leads         → prioritize what to test
7. Hand off to /hunt      → start vulnerability testing
```

## OUTPUT STRUCTURE

```
recon/
├── scope.txt             # Program scope and rules
├── subs.txt              # All subdomains
├── resolved.txt          # DNS resolution
├── alive.txt             # Live HTTP hosts
├── nmap/                 # Port scan results
├── fuzz/                 # Directory fuzzing
├── urls/                 # Discovered URLs
├── js/                   # JavaScript analysis
├── inventory.md          # Structured attack surface inventory
├── leads.md              # Prioritized lead board
└── report.md             # Recon summary
```

## AUTOMATION SCRIPT

```bash
#!/bin/bash
TARGET=$1
mkdir -p recon && cd recon

echo "[*] Phase 0A: Scope discovery..."
curl -s "https://$TARGET/.well-known/security.txt" > security.txt 2>/dev/null
curl -s "https://$TARGET/robots.txt" > robots.txt 2>/dev/null

echo "[*] Phase 0B: Passive recon..."
curl -s "https://crt.sh/?q=%.$TARGET&output=json" | jq -r '.[].name_value' | sort -u > crtsh.txt
subfinder -d $TARGET -all -o subfinder.txt 2>/dev/null
cat crtsh.txt subfinder.txt 2>/dev/null | sort -u > all_subs.txt

echo "[*] Phase 0C: Active recon..."
dnsx -l all_subs.txt -a -aaaa -cname -mx -ns -txt -resp -o resolved.txt 2>/dev/null
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects -o alive.txt 2>/dev/null

echo "[*] Directory fuzzing..."
ffuf -u https://$TARGET/FUZZ -w /usr/share/wordlists/dirb/common.txt -o fuzz.json -s 2>/dev/null

echo "[*] JavaScript analysis..."
katana -u $TARGET -d 3 -jc -o urls.txt 2>/dev/null
cat urls.txt | grep "\.js$" | sort -u > js_files.txt

echo "[*] Phase 0D: Building inventory..."
# Parse alive.txt and fuzz.json to build inventory.md
echo "[+] Recon complete! Check recon/ directory."
echo "[+] Next: Review inventory.md and generate leads.md"
```
