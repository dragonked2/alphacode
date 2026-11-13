---
name: hunt-business-logic
description: Business logic flaws — payment and price manipulation, coupon and credit abuse, referral and reward stacking, refund and chargeback paths, inventory and quantity limits, currency rounding, entitlement bypass. Model intended rules and verify persisted consequences with controlled fixtures.
---

# BUSINESS LOGIC HUNTING

**Core:** individually valid requests can violate security rules when combined,
reordered, replayed or processed concurrently. Malformed values may also expose
logic flaws. Generic scanners often lack the product model needed to recognize
these failures; targeted automation can test a defined invariant and oracle.

---

## THE MODEL FIRST

Reconstruct the rules before breaking them. Write the invariant, then test it.

```
Rule: an order may be refunded at most once, in full
Rule: a coupon applies once per customer, and stacks with nothing
Rule: Pro tier permits 10 seats, regardless of payment status
Rule: a referral credit posts only after the referee's payment clears
Rule: inventory cannot go negative
Rule: prices come from the server, never from the client
```

**Each rule is a testable assertion. Each assertion is a hypothesis.**

---

## PAYMENT AND PRICE

```bash
# Client-supplied price — the classic
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"sku":"PRO-ANNUAL","quantity":1,"price":0,"currency":"USD"}' \
  "https://target.com/api/checkout"

# Negative / fractional quantity
-d '{"sku":"PRO-ANNUAL","quantity":-1}'      -d '{"sku":"PRO-ANNUAL","quantity":0}'
-d '{"sku":"PRO-ANNUAL","quantity":0.0000001}'

# Currency and rounding
-d '{"sku":"PRO-ANNUAL","price":10.00,"currency":"JPY"}'   # 0-decimal vs 2-decimal
-d '{"sku":"PRO-ANNUAL","price":1.00,"currency":"USD","discount":100}'

# Amount split — one item, two payments, negative remainder
-d '{"items":[{"sku":"A","price":100}],"discount":-100}'
```

```
CHECK:  is the amount in the confirmation response equal to amount charged?
CHECK:  does the server ever accept a price/symbol it did not issue?
CHECK:  coupon applied after tax, before tax, or twice?
CHECK:  is the currency taken from the request, or bound to the cart?
```

---

## COUPON, CREDIT, REFERRAL

```bash
# Apply, then re-apply — must fail
curl -s -X POST -H "Authorization: Bearer $TOKEN" -d '{"code":"WELCOME10"}' "$B/cart/apply"
curl -s -X POST -H "Authorization: Bearer $TOKEN" -d '{"code":"WELCOME10"}' "$B/cart/apply"

# Apply without checkout, then reuse at a second account
# Referrer self-referral: A refers B, B refers A
# Stack: apply two coupons in one cart — the rule said "stacks with nothing"
# Round trip: order → refund → the refunded coupon is reusable
# Partial: refund leaves the coupon balance credited twice
```

```
CHECK:  is the "already redeemed" check server-side or a client-side disable?
CHECK:  do coupon state and order state update in one transaction?
CHECK:  can a refunded order's discount be re-applied? (very common)
```

---

## ENTITLEMENTS AND LIMITS

```
Subscription state   PENDING / PAST_DUE / CANCELED / TRIAL — do premium
                     endpoints check more than "a subscription exists"?
Trial reset          new account, new trial — is the trial per-user or per-device?
Feature gate         Pro feature hidden in the SPA; is the API enforcing it?
Seat limit           10 seats, but does the 11th fail, or silently succeed?
Rate/usage quota     is the counter server-side, and does it reset correctly?
Concurrent session   is "1 device" enforced server-side?
```

An absent server-side entitlement check is a candidate only after establishing
the intended restriction and demonstrating unauthorized protected capability.
Check documented grace periods, free allowances and sharing rules. Severity and
program eligibility depend on demonstrated consequence and the actual policy;
a hidden UI control or successful HTTP response alone proves neither.

---

## RACE-LOCKED BUSINESS LOGIC

Rules above that are *supposed* to be atomic are the race-condition surface.
See `hunt-race` for the mechanics.

```
double spend        balance check and debit in two queries
double redeem       coupon validity check and mark-used in two queries
double refund       refund eligibility checked, then processed
seat limit          concurrent invites each see "9 of 10 used"
```

---

## IMPACT BOUNDARY

Business logic testing is the most likely area to cause **real financial loss**,
because the payloads are valid and the operation is real.

```
Proven:  a coupon applies twice
  -> STOP. Do not complete a second real purchase. Do not drain inventory.
  -> Do not run the 20x version to quantify it. Count the endpoints instead.

Proven:  a Pro gate is client-side only
  -> STOP at the API accepting a Pro-only call. Do not enumerate Pro data.
  -> Do not export a customer list with it.

Proven:  price comes from the client
  -> STOP at a $0 confirmation for your own cart. Do not attempt a real
     negative-balance transfer.

Proven:  a race exists in the debit path
  -> STOP at proof. No real double-spend, ever.
```

**Quantify breadth, not damage.** "The same client-supplied price is accepted
by 4 checkout endpoints" is a complete, actionable, safe finding. "I spent
$40,000 of merchant credit" is a crime, and it also gets the report thrown out.

---

## SEVERITY

```
Financial loss, systemic, trivial to trigger  -> Critical
Entitlement bypass on a paid tier              -> High
Single-user credit / coupon over-redemption   -> High
Business rule bypass with no direct loss       -> Medium
Rate/quota abuse, self-limited                 -> Low
```

Score with `severity-engine`. Business logic findings need an explicit
**precondition** in the report — "requires a registered account, no special
role" or "requires two accounts the attacker controls" — because the reviewer
needs to know what the attacker must already have.
