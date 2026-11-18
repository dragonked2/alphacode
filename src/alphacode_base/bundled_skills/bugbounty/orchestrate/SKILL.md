---
name: orchestrate
description: "Run the bounded reconnaissance/scanner task graph, retain execution audits, and continue candidate investigation and proof collection."
---

# Bug Bounty Orchestrator

The orchestrator provides automated workflow management for bug bounty assessments.

## Usage

```
alphacode bugbounty orchestrate TARGET [OPTIONS]
```

## Options

- `--output <DIR>` — Output directory (default: bugbounty_output)
- `--silent` — Silent mode (less output)
- `--json` — JSON output
- `--dry-run` — Preview mode
- `--resume` — Resume previous run

## Workflow

This command runs a bounded, mostly static task graph; it is not an adaptive
autonomous pentest, and it does not mean every listed tool will run. First verify
the installed engagement scope, exclusions, method/rate limits, and target
allowlist. The operator's target argument is not permission to probe every
discovered hostname. Keep out-of-scope or unverified discoveries quarantined;
never pass them into active stages until the installed policy explicitly allows
them.

The default graph may schedule passive asset discovery, DNS/HTTP observations,
URL collection/crawling, and configured scanners. The exact tasks, dependencies,
tool availability, input hydration, and partial-coverage audit are authoritative
in the run's state and `coverage/` artifacts, not in this example. Scanner stages
are discovery signals only. There is no generic “exploitation” phase that should
automatically escalate a candidate: choose the smallest authorized discriminating
experiment for a specific hypothesis, use owned fixtures and controls, and stop
once the permitted impact is demonstrated. Avoid state-changing or high-volume
tests unless the engagement explicitly permits them and the experiment requires
them.

After each stage, continue without asking the user for another “go” when there is
clear, already-authorized, bounded work. A missing tool, account, credential,
policy detail, or fixture blocks only tests that require it: diagnose once, record
the exact gap, then continue independent passive, source, artifact, or permitted
read-only work. Ask one specific question only when that missing prerequisite
blocks all useful remaining work or changes authorization, impact, cost, or
irreversible state. Never claim a blocked stage is a negative result.

Resume only after reconciling the saved target, scope-policy version, identity,
tool state, artifacts, and interrupted calls. Do not treat a stale task result as
current if the deployment, policy, account, object state, or relevant configuration
changed. Check for side effects before retrying an interrupted mutation.

At every phase boundary, reconcile pending, partial, failed, blocked, and untested
coverage. Continue with the highest-value next experiment, not necessarily the
next numbered phase. The graph's “complete” state means only that its scheduled
tasks are resolved; it does not establish exhaustive coverage or a clean target.
The static graph does not autonomously verify every candidate or synthesize a
submission-ready report. Continue independent applicable coverage after a finding
is proven, and keep discovery, validation, severity, and program eligibility
separate.

## Tool Chaining

Some configured task dependencies resemble these example tool routes:

```
subfinder → httpx → katana → nuclei
amass → httpx → feroxbuster → nuclei
gau → waybackurls → unfurl → qsreplace → dalfox
```

## Coverage and continuation

There is no never-miss guarantee. Track every planned task with an outcome:
tested, not applicable with evidence, inconclusive, or blocked with a reason.
Keep pending work visible and continue authorized applicable tasks across phases.
Do not ask for another "go" after reconnaissance or stop the assessment because
one finding has sufficient proof. Report progress while continuing. End only at
resolved planned coverage, a blocker affecting all remaining tasks, the agreed
budget, or the user's stop request. Do not equate an empty generated task queue
with exhaustive security coverage.

Tool names below are examples, not proof they are installed or needed. Choose
available tools and tests from the actual surface and request budget. Static
sites require script/DOM and integration analysis; generic SQL scanning without
a reachable query surface is not useful coverage.

Workflow mechanics:
- Each phase has explicit entry and exit criteria
- Tool results are validated before proceeding
- Failed tasks remain explicit; choose an alternative only after reviewing the
  failure, scope, budget and any possible prior side effects
- Progress is tracked and reported
- Task ordering and scanner output do not prove scope, exploitability, or impact.

## Output

The orchestrator produces a Markdown run summary, state/coverage JSON and bounded
raw tool output. Candidate details are unverified until supported by proof.
Produce remediation recommendations only for established defects.

## Execution audit and resume

The executor writes `coverage.json` plus `coverage/` input manifests, per-call
returned transcripts and interruption journals. `state.json` also retains task
attempt coverage. Failed, deferred, in-flight and output-limited work is visible
separately from task completion. A completed task can still have partial coverage.
An interrupted call is indeterminate; inspect actual state before repeating a
write. Tool-internal capture limits and hydration seed caps still apply. Existing
historical tasks without an audit have unknown call coverage. These artifacts
can contain sensitive engagement data and need the same handling as raw evidence.

Load `investigation-control` when resuming or reconciling discovered inventory
against manual tests and remaining clues. The static scanner task graph cannot
replace that coverage map; reuse already-loaded guidance instead of reloading it
every phase.
