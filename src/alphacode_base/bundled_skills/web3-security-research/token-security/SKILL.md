---
name: token-security
description: Token analysis — mint/burn authority, permit, fee-on-transfer, rebasing, hooks, upgradeability. Load for any token, especially ones used as collateral, vault assets, or bridge assets.
---

# TOKEN SECURITY

## Authority map (answer first)

Who can mint? Who can burn whose tokens? Who can pause/blacklist/seize?
Is there an arbitrary-mint path (unrestricted `mint`, broken access
control, initializer)? Can supply be manipulated (rebasing, reflection,
unbounded mint)? A token with attacker-influenced supply poisons every
protocol that prices it as collateral.

## Behavioral quirks vs the host protocol

- Fee-on-transfer: host accounting assuming exact amounts breaks.
- Rebasing/reflection: balances change without transfers; debt and share
  math desync.
- Hooks/callbacks (ERC777, ERC1363, safe-transfer to contracts): re-entry
  with stale accounting.
- Permit (EIP-2612): signature replay across chains/forks if domain
  separator omits chainid/address; nonce handling; front-runnable approvals.
- Blacklist/pause/upgrade: censoring withdrawals, freezing collateral,
  or changing semantics under a live market.

## Grep anchors

```bash
grep -rn "function mint\|function burn\|_mint(\|_burn(\|permit(\|DOMAIN_SEPARATOR\|feeOnTransfer\|_beforeTokenTransfer\|_afterTokenTransfer" contracts/
```

## Rule

Token quirks are only findings when they produce **host-protocol loss**:
show the downstream invariant break (collateral overvalued, shares
mispriced, bridge over-minted). Quirks in isolation are informational.
