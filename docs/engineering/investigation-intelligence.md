# Investigation intelligence architecture assessment

## Audit and preservation

The root Cargo package exposes modules through src/lib.rs; alphagrep is a workspace member. Provider implementations, transport, agent turn execution, tool registration, native recon bridges, bundled skills, TUI input/rendering, and storage remain in place. This audit traces the security execution and persistence paths; it is not a claim that every source line has been reviewed.

| Existing component | Decision | Reason |
| --- | --- | --- |
| security_core hypothesis, observation, authz, proof, validation | Preserve | Existing evidence gates and differential controls should remain authoritative |
| security_core context, adaptive cycle, research session | Preserve | Useful reasoning and source-review records, but not the live hunt snapshot |
| app_core bugbounty_orchestrator | Upgrade | Live task queue and discovery sets have no shared asset/hypothesis relationships |
| app_core bugbounty_executor snapshot | Upgrade | Durable phase snapshots omit investigation state; historical proofs deliberately require fresh verification |
| scope_gate and tool scope checks | Preserve | Discovery must not expand authorization |
| provider ecosystem, native ProjectDiscovery integrations, TUI | Preserve | No provider coupling or input changes needed for the foundation |
| investigation intelligence | Introduce | Typed asset relationships, evidence references, explicit transitions, durable blockers, ranked leads |

## Original weaknesses

Discovery is primarily flat sets. Scanner candidates have stable identifiers but lack lifecycle and evidence-linked investigation records. Multiple reasoning models exist without being connected to the runtime snapshot. An empty generated task queue does not establish that unresolved theories were investigated. Existing proof checks distinguish scanner labels from confirmed findings and must not be bypassed.

## Implementation design

Add a deterministic, provider-independent intelligence record to the existing orchestrator and its backward-compatible snapshot. Record canonical HTTP assets and hosts with per-tool provenance; keep scope classification computed from an explicit policy instead of inferred from a link. Store hypotheses referencing assets and immutable, redacted artifact metadata. Enforce discovery -> hypothesis -> investigation -> validated/rejected/blocked transitions. Validation reuses ProofObject against actual exchanges; rejection requires contradictory evidence; reopening requires new evidence. Chain transitions need separately verified proofs and validated endpoint hypotheses. Rank only actionable scoped hypotheses deterministically, with bounded cost and risk. Persist through the existing atomic state replacement path.

## Boundaries

This phase does not add exploit payloads, tools, new providers, or claim improved real-world discovery rates. Structured verification still depends on honest tool observation capture and meaningful controlled experiments. Rate limits and tool dispatch scope remain the existing runtime's responsibility. Cross-tool execution scheduling and specialized engines require later integration and lab measurement.

## Implemented capability and integration

- `security_core/investigation.rs`: path-level asset inventory with provenance; evidence-linked graph relationships; immutable evidence metadata; deduplicated hypotheses; guarded lifecycle transitions; explicit rejection, blocker, reopening and contradiction-resolution records; scope-aware bounded ranking; paired-capture validation; separately observed chain experiments; snapshot reference checks.
- `app_core/bugbounty_orchestrator.rs`: discoveries feed the inventory and scanner signals with URLs create discovered leads. Existing candidate and finding gates remain in use.
- `app_core/bugbounty_executor.rs`: the intelligence record travels with the existing atomic snapshot; old snapshots default to an empty record; report output exposes investigation state and incomplete asset investigation.
- `security_core/mod.rs`: exports the new module. No dependencies, tools, providers or exploit payloads added. Existing bundled investigation and vulnerability skills remain available.

## Evidence and security boundaries

`Validated` means that the recorded controlled experiment passed the existing observed-exchange evidence gate. It is not independent confirmation that the declared behavior violates a security boundary. Paired HTTP evidence alone cannot establish ownership, permission, causality, business impact or severity. Chain experiments require validated constituent experiments and a separate capture; an independent reviewer must still establish the semantic prerequisite/capability connection. These records do not automatically create reportable findings in the existing runtime.

The new inventory removes URL userinfo, query values and fragments. It does not promise to find secrets embedded in paths, operator-supplied descriptions, or artifact paths. Evidence redaction is a caller attestation; artifact content/hash verification is not implemented here. Existing raw transcripts, inputs and legacy URL inventories retain their existing handling requirements. No active target testing was performed.

## Remaining engineering work

### Recovered ten-phase acceptance map

The original assignment has been recovered from the engagement journal. The
following map distinguishes existing foundations from runtime acceptance; it is
not a phase-completion declaration. Each phase needs executable test evidence.

| Phase | Existing implementation locations | Acceptance still required |
| --- | --- | --- |
| 1 — Audit | `src/lib.rs`, `agent/turn_loops.rs`, provider runtime modules, `tool/mod.rs`, browser, skill, memory, compaction, session and TUI modules | Trace shared dispatch and lifecycle ownership; preserve providers, ProjectDiscovery and unrelated edits |
| 2 — Core intelligence | `security_core/investigation.rs`, `hypothesis.rs`, `hypothesis_set.rs`, `observation.rs`, `action.rs` | Structured experiments, clue progression, explicit inconclusive/pause state, equivalent-negative reassessment and scoped prioritization in a live caller |
| 3 — Tool reliability | `tool/schema_preflight.rs`, `repeat_guard.rs`, `scope.rs`, `recon_common.rs`, browser and HTTP actor comparison | Structured failure categories, diagnostic-before-retry, bounded recovery, isolated account contexts, evidence captured from actual outcomes |
| 4 — Security intelligence | investigation assets/relationships, `scope.rs`, `authz.rs`, `skill_router.rs`, bundled class workflows, native recon tools | Evidence-backed application/API inventory; observed/inferred distinctions; controlled trust-boundary workflows and relevant skill retrieval |
| 5 — Persistent memory | `research_session.rs`, bug-bounty executor snapshots, session journals, `agent/challenge_context.rs` | Engagement-specific serialized mutations, verified artifact references, checkpoint restoration after compaction/model changes without silently replacing corrupt state |
| 6 — Verification | `proof.rs`, `authz.rs`, `validation.rs`, `verification_quality.rs` | Independent controls and all seven gates connected to runtime candidates; discovery confidence must not grant reportability |
| 7 — Reporting | finding/evidence/report modules and executor output | Sanitized, observed-evidence reports and explicit readiness; no automatic submission |
| 8 — TUI | existing TUI app, info widget, commands and render tests | On-demand research status without blocking input/rendering; buffer-level assertions |
| 9 — Benchmarking | existing synthetic proof/authz/browser/tool tests and inventory fixture | Cover vulnerable and secure controls, continuity and recovery; pin comparisons and label unmeasured model/live performance |
| 10 — Release | root Cargo checks and native MSVC wrappers | Formatting, workspace/test compilation, tests, strict Clippy and release build with actual exit results |

Runtime integration will use the existing investigation record and proof gates,
not a second model-authored vulnerability authority. The research checkpoint
will contain that same record. A tool-facing persistence owner must lock the
read/modify/commit transaction (including across processes), while blocking I/O
runs off the async runtime. A unique temporary checkpoint is not a writer lock.
Tool observations must never be inferred from a command having been scheduled.
Program authorization remains the deterministic session scope policy; restored
inventory and model-supplied scope text cannot expand it.

1. Connect ranked hypotheses to agent-driven controlled experiments and the existing authorization/validation engines. The current CLI task queue remains task-driven; it does not autonomously execute these new leads.
2. Automatically derive evidence-backed asset/identity/workflow edges from structured tool results. Generic graph relationships are available, but discovery currently populates nodes only.
3. Checkpoint each investigation mutation through a serialized persistence owner. The existing runtime still saves at phase boundaries; interrupted in-flight calls retain the existing crash journals.
4. Implement verified artifact ingestion/redaction and independent boundary/causality reviews, with environment/revision invalidation.
5. Add complete per-feature/identity/operation coverage and program-policy resource accounting. The current coverage-gap query identifies assets without any validated experiment; it is not exhaustive coverage.
6. Integrate specialized engines, dynamic skill dependency validation and operator TUI presentation after the state foundation has lab coverage.
7. Benchmark real discovery precision, false positives, chain correctness and model/tool cost against a pinned baseline in controlled applications. The synthetic fixture below does not measure those outcomes.

## Reproducible structural fixture

`deterministic_fixture_measures_inventory_and_lead_retention` compares the existing flat-URL inventory shape with the canonical inventory on the same 200 query variants of one controlled path. Expected counts: 200 raw URL variants, one canonical path, two retained discovery sources, one unverified lead after serialization/restoration. Query variants are intentionally collapsed at the graph layer; this is not proof of semantic endpoint equivalence. Timing is printed for observation only, with no speed threshold or improvement claim.

## Regression-driven correction

The first full library run exposed an existing arbitration inconsistency: a reproduced finding became contradicted after more than two disproof attempts even if none succeeded. The implementation now keeps attempt count separate from explanatory results, and the conflicting legacy test expectation was corrected. Existing uncommitted arbitration regression tests were preserved. This is a reasoning correctness fix, not measured discovery improvement.

The hunt stop decision now returns `coverage_incomplete` for unresolved investigation leads after the generated tool queue is empty, or `blocked` when the remaining leads are explicitly blocked. Dry-turn counts cannot silently close these leads.

## Verification record (2026-11-13)

Builds use the VS 2022 MSVC environment and `CARGO_TARGET_DIR=C:\Users\or0to\AppData\Local\Temp\alphacode-target` outside OneDrive.

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace`: passed.
- `cargo clippy --lib -- -D warnings`: passed on the final production changes.
- `cargo test --lib investigation -- --nocapture`: 16 passed, zero failed, including both runtime integration regressions. The deterministic inventory fixture printed 200 raw URL variants, one canonical path, one retained unverified lead and 2,112 microseconds for this single debug-profile sample (not a throughput benchmark).
- Initial full suite: 6,650 passed, 39 failed, 15 ignored. A subsequent pre-fix run with two additional tests had 6,652 passed and the same 39 failures. The failures exposed the count-based arbitration defect and color/animation assertions under inherited `NO_COLOR=1`, `TERM=dumb`. A final run removes `NO_COLOR` and sets `TERM=xterm-256color`, `COLORTERM=truecolor` in the test process only.

No tagged revision, release publication, external-target testing, real-world discovery-rate measurement or provider benchmark was performed. Existing unrelated checkout edits were preserved. One existing untracked schema-preflight test received rustfmt-only wrapping so the workspace formatting gate passes.

Final foundation regression result before the follow-up provider/helper work: `cargo test --lib -- --quiet` passed with **6,696 passed, zero failed, 15 ignored**, using the normal color-capable test environment described above. The arbitration regression passed after the count-based refutation fix. Follow-up provider/helper and header-lab validation is tracked in `provider-pressure-and-windows-helpers.md`.
