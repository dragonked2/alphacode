---
name: known-incidents
description: Web3 incident knowledge base — root causes, violated invariants, attack sequences, detection strategies. Load for pattern analogies during hypothesis generation.
---

# KNOWN INCIDENTS (reasoning examples, not checklists)

Each entry: root cause → class → primitive → violated invariant →
prerequisites → sequence → asset-flow pattern → why naive scanners miss
it → detection strategy → analogues. Do not invent incidents; verify
against reliable sources before citing.

## Oracle manipulation

- **Mango Markets ($117M):** thin-perp price pushed with attacker capital,
  inflated collateral → undercollateralized borrow. Invariant:
  `debt <= collateral x LTV` evaluated on attacker-set price. Detect:
  price influence → state influence → asset influence with viability math.
- **Curve ($70M class):** read-only reentrancy + stale price view feeding
  valuations. Scanners miss it: no classic reentrancy write pattern.

## Accounting / shares

- **ERC4626 donation/inflation:** first-depositor or donated
  `totalAssets` skews `shares = assets x supply / total`. Detect: empty
  vault + raw-transfer-receivable + missing virtual offset.
- **Desync (paired state):** one path updates A without B (early return,
  fast path). Detect: writer enumeration per pair.

## Bridge / signers

- **Wormhole ($320M class):** unverified/uninitialized upgrade path
  minting unbacked assets. Invariant: `minted <= verified locked`.
  Detect: mint-authority enumeration, not just message parsing.
- **Multisig/quorum failures:** duplicate-signer counting, threshold
  confusion, rotation gaps. Detect: distinct-signer tests.

## Access / proxy / signature

- **Parity freeze ($150M+ class):** unprotected init/ownership seizure.
  Detect: initializer coverage on every ownership-bearing contract,
  including implementations.
- **Permit replay:** missing nonce/chainid/address binding reused across
  contexts. Detect: digest-field checklist per signed action.

## How to use

Analogize structure, not names: map the incident's asset-flow pattern
and violated invariant onto the target's model. If the invariant and
the attacker-controlled assumption both transfer, you have a hypothesis
worth testing — not a finding until gated.
