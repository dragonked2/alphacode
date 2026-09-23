---
name: defi-accounting
description: DeFi accounting analysis — ERC4626 share inflation, donation attacks, rounding, decimal mismatch, desync, phantom value, fee/reserve errors. Load when vaults, shares, or paired state variables exist.
---

# DEFI ACCOUNTING

## Core test: paired-state walk

For every pair that must stay in sync (totalAssets/totalSupply,
debt/collateral, reserves/LP, balances/accounting):

1. List all writers of A and all writers of B.
2. For each path writing A: does it write B? In which order?
3. Flag early returns, fast paths, callbacks, and donation-receivable
   functions (anything accepting raw `transfer` without `deposit`).

## Bug shapes

- **Share inflation (empty/near-empty vault):** deposit 1 wei, donate to
  inflate `totalAssets`, victim deposit rounds to 0 shares. Check
  `_decimalsOffset()` / virtual shares; absence on an ERC4626 is a red flag.
- **Donation via direct transfer:** `totalAssets()` reading
  `token.balanceOf(vault)` diverges from internally tracked balances.
- **Rounding direction:** mint rounds down, withdraw rounds up — verify
  each `preview*/convertTo*` pair; test first/last wei amounts.
- **Decimal mismatch:** underlying (6) vs shares (18) vs oracle (8);
  trace every scale factor at mint, conversion, and liquidation.
- **Stale/desynced views:** interest accrual, rewards, or strategy profit
  updated on some paths only; read-only re-entry returning stale data.
- **Missing debit/credit:** deposit path credits but withdraw path skips
  the debit on one leg (compare sibling functions line by line).
- **Fee/reserve errors:** fees taken from the wrong base, reserves
  double-counted as both backing and profit.

## Grep anchors

```bash
grep -rn "totalAssets\|totalSupply\|totalDebt\|convertTo\|previewDeposit\|previewMint\|_decimalsOffset" contracts/
grep -rn "balanceOf(address(this))" contracts/
```

## Rule

Never report an accounting anomaly without showing the
attacker-controlled economic effect: which invariant breaks, the
before→after state transition, and the asset movement. A rounding error
worth 1 wei with no amplification path is informational at best.
