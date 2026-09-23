---
name: governance-proxy
description: Governance and proxy/upgrade analysis — voting power, timelocks, upgrade authorization, storage layout, initialization. Load for DAOs, timelocked admin, or any upgradeable contract.
---

# GOVERNANCE + PROXY

## Governance chain to test

```
temporary capital → voting power → proposal → execution → asset control
```

Check: vote-token acquisition (flash loan in same tx? snapshot timing?),
delegation mechanics, proposal thresholds vs quorum vs timelock delay,
emergency powers, and whether upgrades are reachable through governance.

## Proxy checklist

- Type: UUPS / transparent / beacon / diamond — who holds upgrade rights?
- Slots: implementation (0x3608…bbc) and admin (0xb531…),
  `_disableInitializers()` in constructor, `initializer` modifiers,
  reinitialization versions.
- Storage collisions: proxy vs implementation layout, upgrade layout
  changes, gap arrays.
- Upgrade authorization: `_authorizeUpgrade` body — empty or
  attacker-reachable authorization is takeover.
- Function collisions (transparent proxy admin) and
  `delegatecall` privilege propagation.

## Grep anchors

```bash
grep -rn "_authorizeUpgrade\|_disableInitializers\|initializer\|UUPSUpgradeable\|propose(\|queue(\|execute(" contracts/
```

## Rule

Upgrades are attack surface: any finding must name the caller, the path,
and the resulting asset or control gain. "Admin is EOA" is a trust
assumption to document, not a vulnerability by itself.
