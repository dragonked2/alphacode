---
name: web3-security-research
description: "Deep Web3/DeFi security research — protocol modeling, asset-flow graphs, invariant reasoning, accounting/oracle/bridge/governance analysis, multi-transaction exploit chains, economic viability checks, and 12-gate verification. Use for any Solidity/EVM audit, DeFi review, bridge/cross-chain assessment, or when hunting fund-loss, insolvency, or privilege-escalation bugs."
---

# WEB3 SECURITY RESEARCH — VALUE-FLOW AUDITING

Find bugs that let an attacker **create, release, control, withdraw, borrow,
redeem, mint, or transfer value they are not entitled to** — while rejecting
weak findings aggressively.

## PRIME DIRECTIVE

Reason about **protocols and economic invariants**, not pattern names.
A reentrancy-shaped function with no value transfer is not a finding.
An accounting quirk with no attacker-controlled economic effect is not a finding.
Prefer **fewer high-confidence findings** over many weak ones.

## PIPELINE: UNDERSTAND → MODEL → INVARIANTS → HYPOTHESES → CHAINS → VERIFY → REPORT

### Phase 1 — Cheap reconnaissance (see `protocol-model`)

1. Enumerate contracts, proxies, admins, roles, tokens, vaults, pools,
   oracles, bridges, routers, governance, keepers, callbacks, hooks.
2. Record who can **mint, burn, transfer, upgrade, pause, seize, set
   parameters, change oracles, execute arbitrary calls**.
3. Build the protocol map. No map → no hunting.

Suggested tools: `read`, `agentgrep` (Solidity selectors, role modifiers),
`bash` (forge/slither/cast when installed), `webfetch` (docs, deployed ABI).

### Phase 2 — Asset-flow graph (see `protocol-model`)

For every value-bearing asset trace origin → custody → accounting
representation → conversion → redemption → exit. Distinguish **real
backing** from **accounting representation** (shares, LP tokens, debt
tokens, receipts, bridge representations).

### Phase 3 — Invariants (see `bytecode-analysis`, `defi-accounting`)

Derive protocol-specific invariants and explain why each applies:

```
redeemable supply <= backed assets
liabilities <= assets
debt <= collateral value x LTV
shares x exchangeRate ~= represented assets
withdrawn <= legitimately credited
mintedBridgeValue <= verifiedLockedValue
unauthorized users cannot grow privileged state
```

### Phase 4 — Targeted hypotheses (see references per domain)

Score each hypothesis before spending verification budget:

```
Score = Impact(1-5) x Exploitability(1-5) x Confidence(1-5)
60-125 → TEST NOW | 30-59 → QUEUE | <30 → DROP
```

Increase priority adaptively: lending + manipulatable oracle + high LTV +
shallow market → push oracle/collateral/flash-loan/liquidation work first.
Do not spread effort evenly across unrelated detectors.

### Phase 5 — Exploit chains + economics (see `exploit-chaining`, `economic-attacks`)

Build multi-transaction chains with per-step state
(balances, shares, debt, prices, roles, nonces, reserves). For each chain
compute attacker capital, temporary (flash) capital, liquidity needed,
gas, protocol loss, attacker gain, repayment, **net profit**. Reject
economically impossible attacks. Flash loans are temporary capital, never
vulnerabilities by themselves.

### Phase 6 — 12-gate verification (see `bytecode-analysis`)

Every candidate must pass reachability, attacker control, invariant
violation, state transition (before → action → after), asset effect,
economic effect, reproducibility (local unit test / fork / simulation —
never destructive mainnet transactions), scope, threat model, alternative
explanation, existing mitigation, and evidence. Fail any gate → reject.

### Phase 7 — Report (see `finding-templates`)

Use the finding template. Mark every claim as **observed** (seen in
code/trace), **reproduced** (local/fork PoC), **inferred** (reasoned, not
executed), or **hypothetical** (requires conditions not yet shown). Never
fabricate hashes, addresses, balances, or exploit results.

## SWARM PLAYBOOK (see `swarm-playbook`)

Fan out per hypothesis when the swarm tool is available: recon, protocol
modeling, accounting, oracle, lending/AMM, bridge, token, governance,
access control, economic, bytecode, exploit-chain workers feed
observations to a coordinator. Workers produce **observations, never
findings**. An independent verifier agent attempts disproof and
reproduction before anything is classified confirmed (see TriageAgent).

## FALSE-POSITIVE SUPPRESSION

Reject: unreachable paths; harmless reentrancy; intentional privileged
functions where owner trust is in-model; oracle manipulation whose cost
exceeds gain; price gaps with no exploitable state transition; token
quirks with no protocol loss; informational issues; duplicates;
out-of-scope dependencies; test-only code; dead code.

Scope the economic gate: the profit equation applies to
*value-extraction* findings only. Privilege findings (access-control
gaps, governance takeover paths, unprotected initializers, fund-lock
griefing) carry no profit number — gate them on reachability +
threat-model + evidence, never on `netProfit > 0`. A missing-expiry
signature binding alone is at most informational, never confirmed.

## SAFE VERIFICATION

Static reasoning → local unit tests → isolated simulation → local fork
with snapshots. No destructive mainnet transactions, no live-fund
exploitation. Verification proves the vulnerability, nothing more.
