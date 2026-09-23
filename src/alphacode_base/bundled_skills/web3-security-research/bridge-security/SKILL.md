---
name: bridge-security
description: Bridge and cross-chain analysis — message validation, replay, validator quorum, mint/burn authorization, locked-vs-minted accounting. Load for bridges, messaging layers, and cross-chain asset representations.
---

# BRIDGE + CROSS-CHAIN

## Critical invariant

```
mintedValue(destination) <= legitimately verified lockedValue(source)
```

Every finding must show how this breaks. Two canonical chains:

```
forged message → mint → redeem → real asset extraction
compromised signer → valid-looking message → execution → asset drain
```

## Checklist

- Message validation: source chain id, source contract, nonce, replay
  protection, domain separation, expiry, duplicate-execution guards,
  ordering assumptions.
- Authorization: validator set, quorum math (duplicate signers? threshold
  vs count?), signer rotation, finality assumptions of the source chain.
- Mint/burn paths: who can call `mint` on destination? Is there any path
  besides verified messages (admin, initializer, sibling function)?
- Accounting: locked vs minted reconciliation, per-chain caps, emergency
  pause/withdraw paths and who triggers them.
- Arbitrary execution: generic message execution with attacker-controlled
  calldata/target is remote code execution by design — check constraints.
- Client diversity and liveness: single relayer/validator dependency.

## Rule

Do not assume the bug is in Solidity logic — signer compromise, validator
centralization, and finality assumptions are in scope as explicitly
modeled trust assumptions with evidence, not as speculative findings.
