---
name: oracle-security
description: Oracle analysis — staleness, TWAP windows, single-source spot prices, decimals, manipulation cost vs gain. Load for any price-dependent mint, borrow, liquidation, or settlement logic.
---

# ORACLE SECURITY

## Checklist per price feed

- Freshness: is `updatedAt` checked against a max age? Missing staleness
  check = stale price accepted indefinitely on feed outage.
- Source count: single AMM spot read is flash-loan manipulatable; require
  Chainlink-style primary + TWAP fallback or equivalent.
- TWAP window: < 15 min is suspect; < 5 min is near-spot. Manipulation
  cost falls as the window shortens.
- Decimals and token ordering: verify scale at every hop; inverted
  quote/base flips the price.
- Update authorization: who can push prices? A keeper/EOA price setter
  is a trusted party — model compromise explicitly.
- Fallback behavior: what happens when feeds disagree or revert — fail
  closed (revert) or fail open (use last/zero)?

## Viability math (mandatory)

```
price influence → state influence → asset influence
manipulationCost = liquidityNeeded + fees + gas + slippage
netProfit = attackerGain - manipulationCost - repayment
```

Reject manipulation whose cost exceeds gain, or that requires pool
liquidity that does not exist. Quote the numbers; "oracle can be
manipulated" without economics is not a finding.

## Grep anchors

```bash
grep -rn "latestRoundData\|getReserves\|slot0\|observe(\|getAmountsOut\|updatedAt" contracts/
```
