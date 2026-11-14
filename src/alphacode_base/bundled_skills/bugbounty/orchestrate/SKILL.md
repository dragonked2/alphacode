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

The orchestrator runs 5 phases automatically:

### Phase 1: Reconnaissance
- Subdomain enumeration (subfinder, amass, assetfinder)
- DNS enumeration (dnsx)
- HTTP probing (httpx)

### Phase 2: Attack Surface Mapping
- Web crawling (katana)
- URL discovery (gau, waybackurls)
- Content discovery (feroxbuster)
- URL parsing (unfurl)

### Phase 3: Vulnerability Discovery
- Template scanning (nuclei)
- Web server scanning (nikto)
- XSS scanning (dalfox)
- CORS scanning (corsy)
- CRLF scanning (crlfuzz)

### Phase 4: Exploitation
- SQL injection (sqlmap)
- Directory brute force (gobuster)

### Phase 5: Reporting
- Observed inventory and unverified scanner candidates
- Task outcomes, deferred work and execution coverage
- Raw evidence locations and investigation state

The static graph does not autonomously verify every candidate or synthesize a
complete vulnerability report. Continue with controlled investigations, relevant
class workflows and independent reproduction before assigning findings/severity.

## Tool Chaining

The orchestrator chains tools intelligently:

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

Read `investigation-control` to map the discovered inventory to manual tests and
remaining clues. The static scanner task graph cannot replace that coverage map.
