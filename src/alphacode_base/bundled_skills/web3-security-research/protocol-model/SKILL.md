---
name: protocol-model
description: Web3 protocol reconnaissance — contract maps, proxy/admin discovery, privilege inventories, asset-flow graphs, trust boundaries. Load at the start of any Web3 engagement.
---

# PROTOCOL MODEL

## Contract inventory

For each contract record: address/source, proxy type (UUPS/transparent/
beacon/none), implementation slot, admin, upgrade authority, initializer
status, roles, and which of these it can do: hold assets, mint, burn,
transfer, upgrade, pause, seize, set parameters, change oracles, arbitrary
call. Grep anchors:

```bash
grep -rn "onlyOwner\|onlyRole\|hasRole\|_checkRole\|initializer\|UUPSUpgradeable\|TransparentUpgradeableProxy\|delegatecall\|selfdestruct" contracts/
grep -rn "function initialize" contracts/ -A5
cast storage <proxy> 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc 2>/dev/null
```

## Asset-flow graph

```
USER → DEPOSIT → VAULT → SHARES → STRATEGY → DEX → UNDERLYING
```

Per asset track: origin, custody, accounting representation, conversion,
transfer, redemption, exit. Covered asset kinds: ETH, ERC20, ERC721,
ERC1155, wrapped, synthetic, shares, LP, debt, receipt, bridge
representations.

## Trust boundaries

Draw every boundary value crosses: user↔contract, contract↔contract,
chain↔chain (bridge), off-chain↔on-chain (oracle, keeper, signer).
For each crossing ask: who attests, what is verified, what is assumed,
what happens on stale/malicious input.

## Kill signals (check before deep work)

- TVL < $500K or payout cap < $10K realistic → deprioritize
- 2+ top-tier audits on the exact deployed version + simple flow → low yield
- No source and no ABI and no bytecode access → recon only

## Output: protocol-model.md (see `finding-templates`)

Contracts table, privilege matrix, asset-flow graph, trust boundaries,
open questions. The model is a living document — update it as evidence
accumulates and steer detector priority from it.
