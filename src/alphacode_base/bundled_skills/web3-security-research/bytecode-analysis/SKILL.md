---
name: bytecode-analysis
description: Bytecode/unverified-contract analysis, invariant derivation, and the 12-gate verification protocol. Load when source is missing or before classifying any finding as confirmed.
---

# BYTECODE + INVARIANTS + VERIFICATION

## Unverified contracts (source unavailable)

1. Obtain runtime + creation bytecode from legitimate sources (explorer
   APIs, local node).
2. Recover selectors (`cast selectors <bytecode>`), control flow, storage
   accesses, external calls, delegatecalls, token transfers, privileged
   paths.
3. Compare deployed behavior against any ABI/source fragments; reason
   from observed runtime behavior. Unverified is not vulnerable by itself.

## Invariant derivation

For each value movement write the invariant and why it applies
(backing relationship, conservation of funds, authorization model).
Track totals: assets, liabilities, shares, rates, debt, collateral,
supply, reserves, fees, liquidity, oracle values. Probe novel
violations: what does the protocol assume, which assumptions are
attacker-controlled, and can valid single operations compose into an
invalid economic state?

## 12-gate verification (all must pass)

1. Reachability — attacker can reach the state.
2. Attacker control — attacker controls required input/state.
3. Invariant violation — name the exact invariant.
4. State transition — before → action → after.
5. Asset effect — exactly what moves.
6. Economic effect — protocol loss and attacker gain with numbers.
7. Reproducibility — local unit test / fork / simulation (safe env only).
8. Scope — component is in target.
9. Threat model — no trusted/privileged actor required (or document it).
10. Alternative explanation — not intentional behavior.
11. Existing mitigation — nothing else blocks it.
12. Evidence — sufficient to report.

Fail any gate → reject or downgrade to hypothesis.

## Confidence (evidence-based, not felt)

UNCONFIRMED (hypothesis only) → THEORETICAL (path reasoned) →
PROBABLE (partial evidence) → VERIFIED (reproduced locally) →
CONFIRMED (all gates + independent check). Cite code path, function,
transition, violation, movement, reproduction, economics per level.
