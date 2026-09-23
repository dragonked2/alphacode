---
name: lending-amm
description: Lending-market and AMM/DEX analysis — collateral valuation, health factors, liquidation, reserve/invariant manipulation, LP accounting, callbacks and hooks. Load for lending, borrowing, liquidity pools, or concentrated liquidity.
---

# LENDING + AMM

## Lending: the canonical chain to test

```
cheap state manipulation → inflated collateral → borrowing →
insufficient repayment → protocol loss
```

Check: collateral valuation source (see `oracle-security`), LTV vs
liquidation threshold gap, health-factor math at boundaries, debt-share
vs collateral-share accounting, interest accrual on all paths, isolated
vs cross markets (contamination), donation-inflated collateral, first
depositor, liquidation bonus vs discount, bad-debt handling, dust/debt
rounding that bricks liquidations.

## AMM/DEX: connected system

Reserves, invariant, liquidity, price, fees, LP ownership move together.
Check: reserve manipulation windows, invariant edge cases (empty pool,
single-sided, first LP), fee-on-transfer/rebasing tokens breaking the
`x*y=k` assumption, callbacks/hooks re-entering with stale reserves,
flash swaps repaying from manipulated proceeds, tick math at range edges,
token ordering and decimal differences, LP share mint/burn symmetry.

## Grep anchors

```bash
grep -rn "healthFactor\|liquidate\|ltv\|LiquidationThreshold\|getReserves\|slot0\|mint(\|burn(" contracts/ | head -50
```

## Rule

Show the full loop with numbers: capital in, manipulation, borrow/swap,
repayment, profit. A liquidation quirk that leaves the protocol solvent
and the attacker at a loss is not a finding.
