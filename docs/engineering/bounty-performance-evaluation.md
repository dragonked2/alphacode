# Measuring bounty hunting performance

## Local discovery smoke test

`scripts/bounty_eval.py` starts disposable loopback-only HTTP applications with
random fixture IDs, tokens and canaries. It exercises a vulnerable export path,
a repaired variant and intentionally public data. Agents receive ordinary
application routes and two controlled accounts, never the variant name in the
prompt or trial workspace name. Trials are shuffled. The grader requires actual
cross-account access, owner and missing-object controls, and repeated access;
supplied canary text or successful process exit alone earns no discovery credit.
The root-cause count deduplicates opposite-direction manifestations.

From PowerShell, a bounded run using an explicitly selected executable is:

```powershell
$bountyCommand = ConvertTo-Json -Compress -InputObject @('C:\path\to\alphacode.exe', 'run', '{prompt}')
python scripts/bounty_eval.py --command-json $bountyCommand --output "$env:TEMP\bounty-smoke-unique.json" --label 'revision/model/configuration' --timeout 180 --request-budget 100
```

Reports retain failures, timeouts, missing/malformed submissions, duplicate
findings, request counts and precision/recall with undefined values represented
as null. Completion counts accompany finding metrics: a successful process with
no submission is incomplete, and a supported finding saved before a timeout does
not make the assessment complete. A rejected claim in this smoke suite means it
failed the proof contract, not necessarily that its underlying hypothesis was
disproven. Artifacts include agent output and bounded request logs without bearer
credentials. TASK.md and agent logs may contain disposable test tokens.

This is a public smoke suite with one root-cause family, not a blind benchmark.
It isolates HTTP fixtures and workspaces, not host access or outbound traffic.
Only run trusted agents and do not use its score as a broad hunting rating.
The request budget applies to the fixture; the runner cannot establish that an
agent made no requests to other destinations. Python harness self-tests run with:

```text
python -m unittest discover -s scripts/tests -p test_bounty_eval.py -v
```

Ratings and competitive claims require results. Skill breadth, unit-test counts,
model self-ratings and deterministic fixture accuracy do not measure autonomous
vulnerability discovery. A 10x comparison is undefined until its metric, budget
and target population are fixed.

## Evaluation contract

Use authorized isolated applications with resettable data and controlled actors.
Freeze each target revision, policy, account permissions and expected invariants.
Include vulnerable and repaired variants, benign anomalies, expired credentials,
blocked tooling and applications with no planted issue. Keep evaluation targets
and answers outside the agent's accessible checkout and development corpus.

Record the following before running either system:

- Agent revision, model/version, enabled tools and configuration.
- Target image/revision, seed, policy and supplied accounts/context.
- Wall-clock, request, concurrency, token and monetary budgets.
- Available source access and network reachability.
- Run count, scoring rules and treatment of timeouts or unavailable systems.

Give competitors equivalent information and permissions. When a service cannot
accept the same environment or disclose its resource use, report that limitation
instead of normalizing with invented numbers. Do not infer a head-to-head result
from a vendor leaderboard, a public CTF score or a local proof-gate benchmark.

## What counts

A discovery counts only after an independent reviewer reproduces the broken
invariant from the supplied artifacts. Reviewers must check ownership, intended
sharing, session validity, positive/negative controls, causal attribution and
observed consequence. Root-cause duplicates count once, with affected surfaces
retained separately. A model summary or two agreeing agents is not an independent
reproduction. Keep original captures separately from bounded prompt excerpts.

Report all runs, including unsuccessful ones:

| Metric | Definition |
| --- | --- |
| Discovery recall | Independently reproduced root causes / eligible known root causes |
| Report precision | Independently reproduced submitted root causes / all submitted root causes |
| Time to proof | Elapsed time from start to reproducible evidence; failures remain censored, not dropped |
| Cost per discovery | Total run costs, including failed runs / reproduced root causes |
| Unsupported claims | Completion, severity or exploit claims unsupported by retained evidence |
| Coverage | Tested, inconclusive, blocked and untested items under stated conditions |
| Scope violations | Actual prohibited requests/actions, including ones yielding no finding |

Use multiple runs and report spread as well as central tendency. Separate
authorization, business logic, injection, client execution, parser/cache and
deployment categories. Publish exclusions and adjudication disagreements.
Precision is undefined when there are no submissions; cost per discovery is
undefined when there are no reproduced discoveries. Neither is a perfect score.

## Comparative claims

Specify the numerator: for example, ten times as many reproduced root causes
within the same cost and time limits. Alternatively compare cost or time at
matched recall and precision. Do not combine these into an unexplained rating.
Show uncertainty and ensure a result is not driven by one target family.

The shipped deterministic security benchmark is a regression suite for supplied
observations. Its public evaluation fixtures are inspectable; once inspected
for development they cannot serve as a blind discovery test. Fresh externally
held targets are required for a competitive performance claim.

## Current hardening

Verification quality now rejects invalid numeric inputs and requires every
essential dimension before reportable promotion. Survived false-positive checks
must correspond to recorded tests. HTTP proof excerpts mark truncation so an
omitted canary cannot establish absence, and preserve request-body whitespace.
These changes remove specific unsound decisions. They do not establish improved
discovery recall, program acceptance or superiority over another system.

The regression benchmark also separates deduplication from vulnerability
classification: merge counts are computed without consulting the answer key,
scored once from the recorded outcome, and reported independently of discovery
precision/recall. Per-family and aggregate scores use the same implementation.
Merge regressions fail the gate. An answer-mutation test verifies that changing
deduplication answers changes scoring, never engine predictions.
