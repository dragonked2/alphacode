---
name: signature-security
description: Signature analysis — EIP-712 domain separation, nonces, replay, malleability, multisig quorum. Load for permit, meta-transactions, off-chain orders, or multisig-controlled assets.
---

# SIGNATURE SECURITY

## Required binding checklist

Every signed message must bind: nonce (single-use), chain id, contract
address, action-specific parameters, and expiry. Missing any one is a
candidate replay (same chain, cross chain, cross contract, or
re-submission).

## Checks

- EIP-712 domain: name, version, chainId, verifyingContract present and
  actually used in the digest — a correct-looking constant that is never
  hashed is a finding.
- Nonce model: sequential vs bitmap vs none; cancellation support.
- Malleability: `ecrecover` with unchecked `s`/`v` ranges; duplicate
  signer counting in multisig loops.
- Quorum math: threshold vs distinct-signer count; owner-set mutation
  paths.
- Meta-transactions: relayer trust, deadline enforcement, gas abstraction
  abuse (gasless draining of allowances).

## Attack shape

```
attacker-controlled message → valid signature interpretation → privileged action
```

## Grep anchors

```bash
grep -rn "ecrecover\|EIP712\|DOMAIN_SEPARATOR\|nonces(\|deadline\|block.chainid" contracts/
```
