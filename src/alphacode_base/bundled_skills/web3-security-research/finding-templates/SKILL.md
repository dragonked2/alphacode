---
name: finding-templates
description: Web3 output templates — protocol model, attack path, finding, evidence, verification report. Load when writing up models or findings.
---

# FINDING TEMPLATES

## protocol-model.md

```markdown
# Protocol Model: <name>
## Contracts (address/source, proxy, admin, roles, capabilities)
## Privilege matrix (who can mint/burn/upgrade/pause/seize/set-oracle/arbitrary-call)
## Asset-flow graph (origin → custody → representation → exit per asset)
## Trust boundaries (attested vs assumed per crossing)
## Invariants (statement + why it applies)
## Open questions
```

## attack-path.md

```markdown
# Attack Path: <hypothesis>
## Prerequisites (capital, roles, state, liquidity)
## Step table (tx, action, before → after per tracked variable)
## Asset flow (entries/exits per step)
## Economics (capital, costs, gain, repayment, net profit)
## Weakest link + what would disprove it
```

## finding.md

```markdown
# Title: <vuln> in <component> allows <impact>
## Severity: Critical|High|Medium|Low|Info
## Affected component (file:line, contract, function)
## Root cause
## Preconditions
## Attacker capability
## Vulnerable path (call chain)
## Violated invariant
## Attack sequence (numbered, with state transitions)
## Asset flow
## Economic impact (loss + gain + math)
## Reproduction evidence (commands/tests, or what is missing)
## Why existing controls do not prevent it
## Recommended remediation
## Confidence: UNCONFIRMED|THEORETICAL|PROBABLE|VERIFIED|CONFIRMED
## Claim ledger (observed|reproduced|inferred|hypothetical per claim)
```

## evidence.md

Per artifact: id, kind (request/response/trace/diff/tool-output),
verbatim content or path, collector, timestamp, reproducibility,
which hypotheses it supports/contradicts, dependencies.

## verification.md

Gate-by-gate verdicts (pass/fail + reason), verifier identity,
disproof attempts, alternative explanations tested, final verdict.
```

Never fabricate transaction hashes, addresses, balances, or results.
Distinguish observed / reproduced / inferred / hypothetical on every claim.
