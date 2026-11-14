# Security harness and session-failure review — 2026-11-14

This is a bounded repository review and regression pass, not a measurement of
real-program discovery rate, a full audit, or evidence for a 100x/10-of-10 claim.
The starting checkout was HEAD 5ca01463 with untracked `src/skills/` drafts.
Those drafts were preserved. The default prompt is embedded from
`src/alphacode_base/prompt/system_prompt.md`; shipped CTF and bounty entrypoints
are registered in `bundled_skills.rs`. No loading reference to the three
`better_*.txt` drafts was found. Writing those drafts alone does not upgrade
the running agent.

## Observed failures and corrections

The 30 most recently modified local Alphacode session journals contained seven
integer-deserialization failures, six truncated-write failures (including repeat
refusals), and one missing-command failure. These are attempt counts from a
bounded snapshot, not distinct bugs or a census of all sessions. No credentials
or full transcripts are included here. Other observed errors included missing
files, HTTP failures, browser bridge unavailability, script syntax errors and a
missing background task; these do not all establish harness defects.

- **Scalar contracts:** Registry dispatch now repairs unambiguous quoted integer
  and boolean fields using the actual selected tool schema, before failure
  hashing, preflight and dispatch. This handles session_search's `"50"` limit.
  It does not rewrite strings, unions, nested payloads, unknown fields, fractional
  integers or overflowing integers. Native range checks still apply.
- **Tool awareness:** `tool_help` now supports `action=inspect` with one exact
  name in `tools`, returning the live description and parameter schema immediately.
  Session tool restrictions remain applied. Inspection does not establish that
  an external binary, account or backend is ready, and does not enable a tool.
- **Truncated writes:** incomplete calls cannot create directories, create files,
  overwrite files or append prefixes. Smaller retries therefore do not duplicate
  a prefix written by a truncated append. Guidance uses complete small chunks,
  path first, preserved append mode and resulting-file inspection. Character
  counts are conservative starting advice, not a provider token-limit guarantee.
- **Exact content:** the new chunk-recovery test exposed an existing generic
  coercion bug: file content was trimmed, and values over the text parser's
  32-KiB cap could fall back to an empty body. Write now extracts content and
  aliases without modifying whitespace, empty strings or large values.
- **Investigation selection:** active investigating leads remain candidates;
  executable selection requires a planned call and available engagement budget.
  Unplanned leads are returned separately as `planning_lead`. A null executable
  lead explicitly does not mean coverage is complete. Scope, risk/cost ceilings,
  reassessment, indeterminate-work and dispatch-budget gates remain applied.
- **Prompt conflicts:** removed the standalone scope-file prerequisite from the
  bounty entrypoint and scope/recon references, while requiring established
  authorization and resolved boundaries. Replaced categorical dismissal of public
  profiles/errors/banners with intended-visibility and content checks. Removed
  literal patch-marker fragments from injected security guidance.

## Verification

Baseline before harness changes: workspace check, test compilation and Clippy
passed; full library run: 6,888 passed, zero failed, 18 ignored. The five Python
bounty-harness self-tests passed. Formatting initially caught one unchanged
line in `usage/api_keys.rs`; it was formatted.

The first focused tool run found the exact-content defect above and two
selection/description mismatches, which were corrected. Final verification:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace`: passed.
- `cargo check --workspace --tests`: passed.
- `cargo test --lib -- --test-threads=4 --quiet`: 6,894 passed, zero failed,
  18 ignored (312.39 seconds).
- `cargo clippy --lib -- -D warnings`: passed.
- `python -m unittest discover -s scripts/tests -p 'test_*.py'`: 41 passed.
- `git diff --check`: passed.

Windows Rust checks used the VS 2022 MSVC environment and
`C:/Users/or0to/AppData/Local/Temp/alphacode-target` outside OneDrive.

## Remaining evidence gaps

No live model/provider trial, held-out CTF evaluation or real-program assessment
was run. Tests prove specific parser, selection and mutation behavior, not better
model reasoning or overall vulnerability recall. The capability baseline in
`bugbounty-capability-baseline-2026-11-14.md` still identifies broader needs:
multi-class vulnerable/repaired fixtures, identity/tenant isolation, request and
availability budgets, interrupted-write reconciliation, durable fix retests and
independent impact oracles. Compare pinned models/configurations on held-out
authorized targets with repeated runs, false positives, recall, cost, scope
violations and unsupported completion claims before assigning a product score.

Source changes require rebuilding and restarting the relevant Alphacode process.
The user subsequently requested publishing v1.1.7. Release publication is gated
on successful exact-commit CI and the platform build workflow; source validation
above does not by itself establish publication or replace an installed process.
