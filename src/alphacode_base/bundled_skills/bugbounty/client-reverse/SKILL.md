---
name: client-reverse
description: Client-side request signing and anti-bot token reversal for bug bounty — when a request carries a sign/sig/hmac/token/nonce/timestamp/X-Sensor header that Burp Repeater cannot replay, recover the signer just enough to reproduce the request outside the client. Covers packet-first staging, locate→recover→runtime→validation→replay spine, webpack/wasm/JSVMP de-obfuscation, hooking fetch/XHR, anti-bot tokens (Akamai/DataDome/PerimeterX), and the bounty payoff: reach the protected API to then hunt IDOR/auth/business-logic.
---

# CLIENT-SIDE REQUEST-SIGNING / ANTI-BOT TOKEN REVERSAL

You hit a request you cannot replay. Burp Repeater returns `401 invalid signature` or `403 bot detected` even though the browser/app does it fine. There is a `sign`, `sig`, `X-Signature`, `_token`, `nonce`, `X-Acf-Sensor-Data`, or encrypted body the client computes.

> **Why a bug bounty hunter cares:** the signature is not the bug. The signature is the **lock on the door**. Behind it is an API the program assumed only their own client would ever reach — so that API is often under-tested for IDOR, BOLA, mass assignment, and business logic. Reversing the signer is the cost of admission; the **payout** comes from what you fuzz once you're inside. Never report "I reversed your sign algorithm" as a finding — that is N/A.

---

## THE CORE PRINCIPLE: PACKET-FIRST

> Reverse engineering is a **blocker-resolution step, not the default entrypoint.**

```
1. Capture the real request (Burp/mitmproxy proxy, or DevTools → Network → Copy as cURL)
2. Replay it UNCHANGED (paste cURL into terminal, or Burp Repeater)
   → 200 / works? → IT'S NOT SIGNED. Skip all reversing. Go fuzz it.
3. Replay it again 5 min later → still 200? → no freshness check (replay window is wide/infinite)
4. Mutate ONE non-signed field (e.g. change an `id` in the body, keep sign as-is)
   → 200? → the sign does NOT cover that field → tamper freely, no reversing needed
   → 401? → the sign covers it → NOW you reverse (continue to STAGES below)
```

Steps 2–4 alone kill ~half of "I need to reverse this" assumptions.

---

## STAGE SPINE: locate → recover → runtime → validation → replay

```
intake → evidence → locate → recover → runtime → validation → replay
```

| Stage | Enter when... | Goal | Exit when... |
|---|---|---|---|
| **locate** | the signing function is unproven | find where the sign field is written and what feeds it | you can point at writer ← builder ← entry ← source |
| **recover** | boundary is real but code is obfuscated | de-shell only the layer blocking you | you have a readable or callable signer contract |
| **runtime** | code is clear but browser-exec ≠ your-exec | find the first divergence | local run reproduces browser sign output |
| **validation** | remaining work is equivalence proof | match checkpoints, not just final output | sign(input) == observed for fresh inputs |
| **replay** | sign reproduces outside the client | Burp/Python baseline request you can fuzz | a stable request you can mutate for IDOR/auth |

---

## STAGE 1 — LOCATE: trace backward from the signature field

```text
writer ← builder ← entry ← source
```

- **writer** — the line that finally puts `sign` into the body/header/query/cookie/WS frame
- **builder** — the transform: `HMAC`, `MD5`, `AES`, sort-then-concat, `JSON.stringify` ordering
- **entry** — the UI action / callback / response that kicks off the chain
- **source** — what feeds the inputs: upstream response, localStorage, cookie, `Date.now()`, user input

### Browser: find the writer in Chrome DevTools

```
# 1. XHR/fetch breakpoint — break the moment the signed request fires
DevTools → Sources → XHR/fetch Breakpoints → + → paste the endpoint path

# 2. Search the bundle for the field name
DevTools → Sources → Ctrl+Shift+F (search all loaded scripts)
search: "sign" "X-Signature" ".sign =" "headers[" "signature"

# 3. DOM/event breakpoint when a click triggers it
DevTools → Elements → right-click the button → Break on → subtree/attribute modifications
```

### Strong first observation points

| Sink (where sign lands) | First place to prove |
|---|---|
| request body field | final `JSON.stringify` / submit / `fetch(body=...)` |
| request header | the `headers[...] =` or `setRequestHeader` call |
| JS-set cookie | the `document.cookie =` setter |
| WebSocket frame | the final envelope object right before `ws.send(...)` |
| anti-bot blob (`X-Sensor-Data`, `_px`) | the SDK `init()` and the getter that returns the blob |

---

## STAGE 2 — RECOVER: de-shell only what blocks you

| Shell you hit | What it means | Minimal move |
|---|---|---|
| webpack bootstrap | modules wrapped in `__webpack_require__` | break inside the target module, read the local closure |
| string-array obfuscation | `_0x4a2b[12]` lookups | in console, print the decoder array; or breakpoint and read live |
| worker / `postMessage` bridge | sign runs in a Web Worker | breakpoint the worker script |
| wasm loader | sign math compiled to wasm | hook the JS↔wasm boundary; capture inputs/outputs |
| JSVMP (custom bytecode VM) | a dispatcher loop interpreting bytes | **do not reverse the VM** — hook inputs+output, treat as black box |

> **The black-box shortcut beats decompilation 90% of the time.** You almost never need to understand the HMAC math. You need the **input tuple** and a way to **call the function**.

### Reimplement vs reuse the page's signer

```javascript
// REUSE — if the signer is a reachable function, just call it from the console
window.__sign = signFn; // assign at a breakpoint inside the builder
window.__sign({id: 999, ts: Date.now()}) // → get a valid sig for ANY payload

// HOOK to log every real sign(input)->output (no reimplementation):
(function(){
  const orig = CryptoSigner.prototype.sign;
  CryptoSigner.prototype.sign = function(...a){
    const out = orig.apply(this, a);
    console.log('SIGN', JSON.stringify(a), '=>', out);
    return out;
  };
})();
```

---

## STAGE 3 — ISOLATE THE INPUTS

For every input to the signer, classify it:

| Input | Type | Attacker-mutable? | Implication |
|---|---|---|---|
| `timestamp` / `ts` | per-request | yes | regenerate per replay; check validity window |
| `nonce` / `requestId` | per-request random | yes | generate fresh; check uniqueness enforcement |
| `deviceId` / `uuid` | per-session | yes (one value, reusable) | grab once, pin it |
| request **body** / **path** | per-request | yes | **the prize** — mutate body + re-sign to fuzz IDOR |
| **secret key** / `appSecret` | constant, baked in | **no (you extract it)** | if in JS bundle → full forge ability |

**DECISION:**
```
secret is in the client (hardcoded in JS or APK)
  → you can re-sign ANY request offline → full replay, fuzz everything

secret is server-side only, but you can call the page's signer function
  → you can sign any payload while the page is open → replay via headless browser bridge

secret is server-side AND signer is uncallable (heavy anti-debug)
  → you may only replay UNCHANGED requests
  → then test: does the sign omit the path/body? → forge anyway
  → does it never expire? → replay-window bug, report that
```

---

## STAGE 4 — RUNTIME & VALIDATION

```python
import hmac, hashlib, json, time

def sign(body: dict, ts: str, nonce: str, secret: bytes) -> str:
    # Reproduce the EXACT canonicalization the JS does
    payload = json.dumps(body, separators=(',', ':'), sort_keys=False)
    msg = f"{ts}{nonce}{payload}".encode()
    return hmac.new(secret, msg, hashlib.sha256).hexdigest()

# VALIDATE: feed a captured input, compare to observed sig
assert sign(captured_body, captured_ts, captured_nonce, SECRET) == observed_sig
```

The two killers are almost always **(a)** JSON key ordering / separator whitespace, and **(b)** concatenation order of the input fields.

---

## STAGE 5 — REPLAY: build the request you'll fuzz

```python
import requests, time, uuid

SECRET = b"extracted_from_bundle"
DEVICE = "pinned-session-device-id"

def signed_request(body):
    ts = str(int(time.time()*1000))
    nonce = uuid.uuid4().hex
    sig = sign(body, ts, nonce, SECRET)
    return requests.post("https://target.com/api/order",
        json=body,
        headers={"X-Timestamp": ts, "X-Nonce": nonce, "X-Signature": sig,
                 "X-Device-Id": DEVICE})

# NOW the bug hunt begins — mutate the body to reach the protected logic
for victim_id in range(1000, 1100):
    r = signed_request({"orderId": victim_id})
    if r.status_code == 200 and "not authorized" not in r.text:
        print(f"IDOR: read order {victim_id}", r.json())
```

---

## ANTI-BOT TOKENS (Akamai / DataDome / PerimeterX / hCaptcha)

Same spine, harder shell. Two realistic paths:

1. **Reuse, don't reverse.** Grab one fresh valid token from a real browser session and replay within its validity window. Enough to prove the protected endpoint is reachable.
2. **Headless bridge.** Drive a real browser (Selenium/Playwright) to mint the token, hand it to your Python fuzzer.

> Full reversal of Akamai v3 sensor-data is a week-long research effort — **out of scope for a single bounty.** If the anti-bot itself is misconfigured (token never expires, cross-account validation), report **that**.

---

## WHAT'S SUBMITTABLE vs N/A

| Finding | Verdict |
|---|---|
| "I reversed your client signing algorithm" | **N/A** — not a vuln by itself |
| Signed request replays unchanged forever (no freshness) | **Low/Medium** — replay-attack |
| Sign omits body/path → forge any request | **Medium** — sign bypass |
| Behind the sign → IDOR/auth bypass/mass assignment | **High/Critical** — the real bug |
| Anti-bot token from accountA validates accountB requests | **High** — cross-account bypass |
| Anti-bot token never expires | **Medium** — replay window |
