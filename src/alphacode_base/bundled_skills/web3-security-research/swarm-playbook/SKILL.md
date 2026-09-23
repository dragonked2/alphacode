---
name: swarm-playbook
description: Multi-agent Web3 research orchestration — specialist roles, coordinator merge rules, independent verification. Load when the swarm tool is available for a Web3 engagement.
---

# SWARM PLAYBOOK

## Roles (spawn per active hypothesis, not all at once)

Web3Recon, ProtocolModel, AssetFlow, Accounting, Oracle, Lending, Vault,
AMM, Token, Bridge, Governance, AccessControl, Proxy, Signature,
Economic, Bytecode, ExploitChain, Verification, Triage. Keep the set
minimal: recon + model first, then only roles matching live hypotheses.

Suggested spawn (via `swarm` tool `spawn` with completion-report
directive): one coordinator + workers per hypothesis cluster, capped by
the server's live-worker budget. Deep nodes close with `complete_node`
artifacts: findings, evidence (file:line), validation, open questions,
confidence, what was not checked.

## Coordinator rules

- Merge observations into the shared protocol model; deduplicate.
- Workers emit **observations, never findings**.
- Conflicting worker claims → structured arbitration (evidence quality,
  provenance, reproducibility, consistency), never a blind vote.
- Fan out independent hypotheses in parallel; serialize dependent steps.

## Independent VerificationAgent (adversarial)

Gets the evidence, not the original reasoning chain. Must attempt
disproof: reproduce independently, hunt alternative explanations
(public-by-design, unreachable, non-exploitable, already protected,
expected API behavior, scanner/environment artifact), run the control
comparison. Only surviving candidates advance.

## TriageAgent

Applies the 12 gates, assigns UNCONFIRMED→CONFIRMED, suppresses
false positives, and emits the finding template. Anything unproven
stays a hypothesis with explicit missing evidence.
