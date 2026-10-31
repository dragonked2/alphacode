# CTF session reliability

Source: the user's supplied session post-mortem. The original transcript and
challenge instance were not provided, so its aggregate counts and target-specific
claims have not been independently reproduced. The reported UTC span and minute
total also differ; neither is used as a benchmark.

## Implemented changes

- Windows tool shells set Python UTF-8 output and UTF-8 runtime mode for both
  Git Bash and cmd.exe, including background execution.
- `challenge.md` is read from the session workspace whenever the system prompt
  is rebuilt. Evidence is JSON-delimited as task data, capped at 12 KiB, and a
  truncation/read failure is visible rather than interpreted as no prior work.
- The CTF workflow is now actually registered in bundled skills and automatically
  included for explicit CTF requests or an existing ledger. It requires hints,
  stack evidence, closed hypotheses, budgets, and verified completion criteria.
- A 12-call checkpoint budget counts different recognized network probes in
  agent-driven CTF sessions with a ledger. Writing a changed ledger resets it;
  direct user tool calls and local analysis are unaffected. This enforces a
  checkpoint, not the semantic validity of the model's reasoning, and does not
  count individual requests inside a shell script.
- Probe reminders use the newest calls, and an old todo call no longer exempts
  a later run of probes.
- `ctf_web_recon` performs a bounded GET-only pass with headers, a randomized
  missing-page control, same-origin selected paths, separate HTTP classes,
  sampled body hashes, response caps, request and total deadlines, rate-limit
  stopping, preserved partial results, no redirects, and per-target scope checks.
  It does not automatically fire exploit payloads or guess credentials.
- Literal `for source in ...` headers are no longer mistaken for invoking the
  `source` builtin. Substitutions, redirects, and loop bodies remain assessed.
- Dead session-owned browser tabs recover to a new isolated background tab;
  failed actions are not replayed, and the user's active tab is not read as a
  substitute. Eval has an explicit default deadline. Existing bounded retries
  for read-only bridge failures remain in place.

## Validation and limits

Regression tests cover ledger refresh/truncation, explicit CTF detection,
changed-payload checkpoint budgeting, status classification, origin confinement,
a local HTTP redirect/header fixture, Windows Python output, literal loop headers
and protected loop bodies. Live browser-extension recovery and the original CTF
need separate end-to-end validation. No external challenge was probed here.

These changes reduce known failure modes; they cannot guarantee a solve, infer
missing hints, prove that a forbidden route exists, or verify that a cosmetic
ledger edit represents a real pivot.

Focused verification passed: 167 tests across challenge context, checkpoint
budgeting, recon and its local HTTP fixture, probe reminders, command-risk
assessment, Windows Python Unicode output, browser contracts, tool disclosure,
and help-command recovery. Workspace checks, strict library Clippy, formatting,
and the WPF interaction smoke test also passed during implementation. Final
whole-suite verification includes the accompanying UI test updates.

Final full run (true-color terminal test environment): 6,315 passed, 12 ignored,
one failure in the existing process-global catalog-scheduler test
`ensure_started_is_a_noop_without_a_runtime`. That test passes in isolation;
its `OnceLock` assertion races another test that starts the shared scheduler.
All CTF and UI tests passed in the full run. This unrelated scheduler test is
not changed by this work.

The execution environment supplies `NO_COLOR=1` and `TERM=dumb`; tests asserting
RGB colors or animation require clearing `NO_COLOR` and setting
`TERM=xterm-256color`, `COLORTERM=truecolor`, and `FORCE_COLOR=3`. This configuration
was used for the final full run. Console capability tests also passed separately.
