# Repeatable coding-task evaluation

`scripts/agent_eval.py` runs a configured agent command in a fresh copy of each
fixture, then runs the original verifier outside that copy. It records exit
status, timeouts, elapsed time, task fingerprints, logs, and an atomic JSON
report after every completed trial. A zero agent or verifier exit code alone
cannot pass a task: a fresh completion receipt must be written after the verifier
returns normally. Task modules exiting early cannot bypass that check.
Interrupted reports retain `status: running`; only a finished run becomes
`complete`. Existing reports are never overwritten.

The bundled three-task manifest is a **smoke suite for the evaluation workflow**,
not a representative benchmark or evidence of superiority over another agent.
Runner tests use small Python fixture agents, not real coding models.

## Run an evaluation

From the repository root, supply a JSON argument array for the agent executable:

```powershell
python scripts/agent_eval.py --manifest tests/agent_eval/tasks.json --agent-command '["alphacode","run","--no-update","--ndjson","{prompt}"]' --label 'Alphacode VERSION / MODEL' --output "$env:TEMP/alphacode-eval-RUN.json" --timeout 300 --repetitions 3
```

Use a distinct output path for each run. `{prompt}`, `{workspace}`, and `{python}`
are replaced in command arguments without shell evaluation. The verifier uses
the same Python interpreter as the runner. Set the model and authentication
through the agent's own configuration. Record actual agent/model versions in
the label; the runner does not verify that label or infer model selection.
`--no-update` pins the AlphaCode executable for the run. `--ndjson` records
streaming progress and provider connection phases in each trial's agent log.
Add `--provider-audit` with AlphaCode to record dispatch attempts and the peak
rolling 60-second request count for each trial. A `null` traffic summary means
the runner could not obtain audit data for that trial.
With NDJSON, the same option also totals each reported token field across
inference rounds. Cache fields are separate because provider accounting varies.
Timed-out trials can contain partial usage and requests without a usage record.

The command exits nonzero if any task fails. Inspect `agent.log` and
`verifier.log` in the report's artifact directory to distinguish agent errors
from incorrect task outcomes. Timeouts stop the launched process tree. Timing
includes the agent's complete local run. The runner does not measure API cost
or human interventions. Reported token counts require AlphaCode NDJSON and
`--provider-audit`.

This copies task files, **not an OS sandbox**. Run trusted agent commands and
verifiers. The agent inherits the invoking user's environment and filesystem
permissions; the outside verifier is an independent file, not tamper-proof
against a process with those permissions. Logs stay local and can contain the
agent's output. Failed and interrupted task workspaces remain available for
inspection.

## Add representative tasks

Each task declares a unique ID, a prompt, a fixture directory, and a Python
verifier relative to the manifest directory. Fixtures cannot contain symbolic
links or escape that directory. Verifiers accept the copied workspace as their
first argument and return normally only when the requested behavior passes.
Raise an exception or exit nonzero on failure; an explicit `exit(0)` is treated
as incomplete verification because it bypasses the completion receipt.

For an actual comparison, add realistic fixes, refactors, multi-file features,
failure-recovery tasks, and regressions with independent checks. Use the same
task fingerprints, budgets, hardware, repetitions, and comparable model access
for every agent. Publish failures and raw outcomes alongside aggregate results.

## Windows 1.1.13 development baseline

Both runs used the bundled three-task manifest, a 75-second budget per task,
one repetition, and the Alphax Free route. Windows DNS resolution for that
route failed. A temporary process-local tunnel using a public DNS answer
restored connectivity without changing system DNS. The route can select
different upstream models, so these runs establish a smoke baseline and do
not isolate a release-to-release performance difference.

| Task | v1.1.12 | v1.1.13-dev `a127e1c3` | Candidate dispatches | Candidate reported input / cache read / output |
| --- | --- | --- | ---: | ---: |
| Quantity validation | Timed out at 75 s, edits verified | Passed in 36.26 s | 5 | 44,214 / 33,280 / 1,211 |
| Ordered deduplication | Timed out at 75 s, edits verified | Passed in 49.23 s | 6 | 129,496 / 105,472 / 2,185 |
| Inventory SKU normalization | Passed in 34.59 s | Passed in 61.05 s | 8 | 80,333 / 74,240 / 2,966 |

Manifest SHA-256 `17afb112c001ffbf439434308621ae42a98010f729167a3b576dfe9d26960558`.
Dispatches count client attempts. Cache reads are reported separately because
they may overlap the reported input total. Repeated trials on a fixed model
are needed to compare throughput or request efficiency.

### Release verification scope

- The clean `a127e1c3` Windows checkout passed formatting, workspace compilation
  including test targets, library Clippy with warnings denied, and the Rust
  library suite with 6,986 passed, zero failed, and 17 ignored.
- In three loopback streaming trials, `a127e1c3` sent one POST per trial and
  displayed first text in 254–289 ms from launch. The server supplied scripted
  chunks, so this measures local delivery rather than model generation speed.
  A separate scripted HTTP 429 with a ten-minute cooldown returned after one
  POST.
- The `a127e1c3` offscreen terminal benchmark measured a 2.71-second cold draw
  for 1,000 messages, including 2.62 seconds of body preparation. Local commit
  `3a86f246` passed the regression that checks reuse of the Markdown render.
  Its timing trial did not establish a cold-draw improvement. End-to-end
  Windows Terminal input latency remains unverified.
- The live coding results above use three smoke tasks and a single repetition.
  Matched comparisons with other agents, fixed-model throughput, and release
  workflow verification on the final revision remain open.

## Verify the runner

```text
python -m unittest discover -s scripts/tests -p test_agent_eval.py -v
```

The runner tests exercise fresh-copy isolation, real verifier failures,
nonzero agent exits, timeouts, missing executables, preserved existing reports,
and invalid configuration.
