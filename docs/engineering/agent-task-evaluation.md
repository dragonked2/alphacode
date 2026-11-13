# Repeatable coding-task evaluation

`scripts/agent_eval.py` runs a configured agent command in a fresh copy of each
fixture, then runs the original verifier outside that copy. It records exit
status, timeouts, elapsed time, task fingerprints, logs, and an atomic JSON
report after every completed trial. A zero agent or verifier exit code alone
cannot pass a task: a fresh completion receipt must be written after the verifier
returns normally. Task modules exiting early cannot bypass that check.
Interrupted reports retain `status: running`; only a finished run becomes
`complete`. Existing reports are never overwritten.

The bundled two-task manifest is a **smoke suite for the evaluation workflow**,
not a representative benchmark or evidence of superiority over another agent.
Runner tests use small Python fixture agents, not real coding models.

## Run an evaluation

From the repository root, supply a JSON argument array for the agent executable:

```powershell
python scripts/agent_eval.py --manifest tests/agent_eval/tasks.json --agent-command '["alphacode","run","{prompt}"]' --label 'Alphacode VERSION / MODEL' --output "$env:TEMP/alphacode-eval-RUN.json" --timeout 300 --repetitions 3
```

Use a distinct output path for each run. `{prompt}`, `{workspace}`, and `{python}`
are replaced in command arguments without shell evaluation. The verifier uses
the same Python interpreter as the runner. Set the model and authentication
through the agent's own configuration. Record actual agent/model versions in
the label; the runner does not verify that label or infer model selection.

The command exits nonzero if any task fails. Inspect `agent.log` and
`verifier.log` in the report's artifact directory to distinguish agent errors
from incorrect task outcomes. Timeouts stop the launched process tree. Timing
includes the agent's complete local run; token usage, API cost, and human
interventions are not measured.

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

## Verify the runner

```text
python -m unittest discover -s scripts/tests -p test_agent_eval.py -v
```

The runner tests exercise fresh-copy isolation, real verifier failures,
nonzero agent exits, timeouts, missing executables, preserved existing reports,
and invalid configuration.
