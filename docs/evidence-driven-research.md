# Evidence-Driven Security Research Workflow

This document describes the research-workflow capabilities in the
`alphacode_security_core` crate and how to use them together during a
multi-repository security review. It reflects **implemented** behavior; each
section names the module that provides it.

## Why this exists

A real multi-repository review surfaced a specific failure pattern: a suspected
missing `RUnlock()` in a Go signing service was described as *Critical* before
the production-impact chain was established, the reproduction was built late,
and the session lost state across context compactions. The modules below make
the correct order — hypothesis, reproduction, impact validation, severity —
the path of least resistance.

## 1. Evidence gate and validation states

`finding.rs` + `proof.rs` (existing) and `claim_checker.rs` (app core):

- A finding moves through `FindingStage`: `observation → hypothesis →
  candidate → reproduction → validation → impact_confirmation → ... →
  confirmed_finding → report`.
- `Finding::advance` refuses to walk a finding from `program_check` into
  `confirmed_finding` without an attached `ProofObject` that verifies.
- Structured `PROOF:` blocks in reports are parsed and matched to their claims;
  an unverified claim keeps its evidence string (e.g. "baseline failed")
  instead of being reported as confirmed.

## 2. Reproduction harness (`repro_harness.rs`)

Run a candidate reproduction with a hard wall-clock budget:

```rust
use alphacode_security_core::{ReproOutcome, ReproWorkspace, run_bounded,
    go_rwlock_leak_template, go_rwlock_fixed_template, interpret_blocking};
use std::time::Duration;

let ws = ReproWorkspace::create("rwmutex")?;
let result = ws.run("leak", ReproLanguage::Go, &go_rwlock_leak_template(),
                    Duration::from_secs(30));

// A timeout is not proof. Interpret it against the test's own contract:
let (observation, narrative) = interpret_blocking(&result, true);
// observation: BlockedAsExpected | CompletedInstead | Inconclusive
```

Guarantees:

- `run_bounded` kills the child **and its process tree** (`taskkill /T` on
  Windows) at the deadline, then reaps it, so a deliberately blocking test
  cannot hang the session.
- stdout/stderr are captured on dedicated threads (no pipe deadlocks) and
  truncated at a bounded size with an explicit truncation marker.
- The result records outcome (`Passed` / `Failed { exit_code }` / `TimedOut` /
  `SpawnError`), elapsed time, and the exact command line — enough to save as
  evidence and re-run.
- Reusable templates: `go_rwlock_leak_template`, `go_rwlock_fixed_template`
  (the regression case for the fix), `rust_blocking_test_template`.

## 3. Workspace and path reliability (`workspace.rs`)

`Workspace::open(root)` canonicalizes and validates the root; every operation
resolves paths lexically against it and rejects absolute paths and `..`
escapes (including Windows root-relative and drive-relative forms).

```rust
let ws = Workspace::open(repo_root)?;
let text = ws.read_verified("signer/service.go")?;      // categorized errors
let report = ws.write_verified("FINDINGS.md", &ledger)?; // read-back verified
```

- `write_verified` writes, reads back, and compares; `Ok` means the bytes on
  disk match. A failed or unverifiable write is an error, never a success.
- One bounded shell fallback exists for direct-write failures; the direct
  error is still reported when both fail.
- Errors are `FsError { kind, operation, context }` with kinds: `not_found`,
  `permission_denied`, `outside_workspace`, `workspace_unavailable`,
  `inconsistent_read` (the observed "tool says missing, shell says present"
  shape), `io`. Only `io` is retryable, so the workflow stops retrying the
  same broken operation.

## 4. Durable research checkpoints (`research_session.rs`)

```rust
let mut session = ResearchSession::new("eng-42", "review circlefin repos");
session.track_repository("arc-remote-signer", "abc123", None);
session.add_finding(finding);
session.record_rejection(rejected);          // negative finding
session.record_phase_time(WorkflowPhase::Reproduction, elapsed);
session.checkpoint(root)?;                   // atomic temp-file + rename
let loaded = ResearchSession::load(root)?;   // schema-checked
let summary = loaded.resume_summary();       // compact re-entry point
let ledger = loaded.render_findings_ledger(); // FINDINGS.md equivalent
```

- `checkpoint` writes to a temp file, verifies it, then renames: a partial
  write can never corrupt the record. Newer schema versions are refused
  rather than guessed at.
- `CoverageLedger` records which components were reviewed at which revision
  and at what depth (`Inspected` vs `Validated` — reading a function is not
  testing it). A changed revision invalidates the old entries instead of
  being silently accepted.
- `RejectedHypothesisRecord` stores dead ends with the code examined,
  experiments run, rationale, and coverage gaps.
- `ToolFailureRecord` dedupes repeated failures by (tool, operation) with a
  truncated error message, so a retry loop is visible as a pattern.
- Nothing here stores credentials; callers must scrub secrets before
  recording, and failure messages are truncated defensively.

## 5. Prioritization (`prioritization.rs`)

`prioritize(targets, config)` scores components on evidence-based risk
factors (key custody, consensus, authN/authZ, network exposure, untrusted
input, recent changes, existing findings) and returns a plan that explains
every position. `PrioritizerConfig::example_session_config()` shows the
reviewed session's order (signer > consensus > EVM/tx) as a documented
example, not a default. Out-of-scope components are listed as
`do_not_test` rather than silently dropped.

## 6. Concurrency audit (`concurrency_audit.rs`)

- `concurrency_checklist(AuditLanguage::Go)` returns the systematic checklist
  (lock/unlock symmetry, defer usage, waitgroup pairing, goroutine lifecycle,
  channel behavior, context cancellation, atomics vs non-atomics, cleanup on
  error paths).
- `scan_go_lock_symmetry(source)` flags functions where `Lock`/`RLock` counts
  exceed `Unlock`/`RUnlock` with no deferred unlock, possible double-unlocks,
  or balanced counts with early returns and no defer. Every candidate is a
  **hypothesis** (`SCANNER_MAX_SEVERITY` is `info`) — it must be reproduced
  before it becomes a finding.

## 7. Build readiness (`build_readiness.rs`)

`assess_build_readiness(path)` is a cheap, side-effect-free probe: language
and toolchain, lockfile presence, missing generated code (protobuf imports
with no matching `.pb.go`), CGO/native requirements, and required env vars.
`max_feasible_tier()` never claims more than the environment supports, and
`fallback_validation_paths` names the smaller test that can still validate a
hypothesis when a full build is blocked.

`EvidenceTier` in this module keeps the evidence levels distinct: static
review < compilation < unit tests < integration tests < standalone
reproduction < end-to-end impact. A readiness report never implies a test
passed.

## 8. Severity and reporting (`severity.rs`)

```rust
let assessment = SeverityAssessment::assess(&SeverityAssessmentInput {
    evidence_tier: EvidenceTier::Reproduced,      // cap
    category_suggested_severity: Some(Severity::Critical), // anchor, capped
    demonstrated_impact: String::new(),           // => proposed <= Medium
    ..Default::default()
});
let report = FindingReport::from_finding(&finding, revision, assessment,
    summary, evidence);
println!("{}", report.render());
```

- Severity is capped by the evidence tier: a Critical-sounding category with
  only a pattern observed cannot come out above Low; a reproduced defect with
  no demonstrated impact cannot come out above Medium.
- CVSS base metrics are recorded individually and only scored when complete;
  incomplete inputs are rendered with `X` placeholders and explicitly not
  scored. A program policy is never invented — the report says "not
  established" unless the caller supplied one.
- `FindingReport::render` always produces the required sections (summary,
  affected component and revision, technical details, exact code path,
  preconditions, reproduction steps, actual vs expected results, evidence and
  logs, demonstrated impact, limitations, remediation, regression-test
  results, severity rationale, scope considerations). Empty evidence renders
  as "_Not established._" rather than being omitted.
