# Runtime goal evidence

Goal completion uses native tool metadata and the identity of the executed call.
Printed output is not a completion signal: quoted examples, source files, and
failed commands can all contain strings such as `exit code: 0`.

## Mission configuration

Mission JSON accepts an optional `verification_criteria` list. Definitions replace
prose success criteria or validation-plan items with the same statement; other
definitions append a criterion. A mission with explicit definitions requires all
criteria to be verified. Prose items without a definition remain unverified and
need review; the runtime does not invent verification commands from text.

For example, a criterion can require the native exit metadata from a specific
foreground command:

```json
{
  "verification_criteria": [
    {
      "statement": "library tests pass",
      "tier": "FirstParty",
      "check": {
        "ToolMetadata": {
          "tool_name": "bash",
          "input": { "command": "cargo test --lib" },
          "metadata_pointer": "/process_exit_code",
          "expected": 0
        }
      }
    }
  ]
}
```

`input` must equal the complete normalized input object of the executed call.
Extra arguments, a different command, or a different tool do not qualify. Use the
canonical tool name. JSON pointers select native `ToolOutput.metadata`, never a
JSON-looking string in stdout. Empty tool names, missing call IDs, null expected
values, missing metadata, invocation failures, and legacy text predicates cannot
establish a pass.

## Shared execution rules

Local and streamed result recorders use the same goal observer. SDK-native calls
use the same admission and observation functions; their terminal reminder travels
inside the native tool result to preserve tool/result ordering. Streaming runs
check admission at dispatch time, including between serial calls. Already-running
parallel reads are not retroactively cancelled when another result meets a goal.

Each verified criterion records the criterion definition, call ID, tool name, and
observation time. Separate calls may satisfy separate criteria. A failed rerun of
the same tool/input pair removes its earlier pass. Replacing or reordering a
criterion invalidates evidence for that definition. Goal definitions are refreshed
from the active mission at turn start so a changed mission cannot keep an old
terminal contract.

The ledger is serializable with the contract and old snapshots default to an empty
ledger. This does not by itself add contract persistence or crash recovery to every
agent entrypoint.

## Limits and validation status

Metadata proves the configured tool-level predicate, not the adequacy of a test
suite or the correctness of its assertions. Evidence is a point-in-time observation;
unrelated later file changes are not automatically detected by this ledger. Exact
input matching deliberately favors missed matches over accepting a different call.
Legacy `RuntimeDetectable` and `ExternalArtifact` definitions deserialize but no
longer trigger automatic completion from output text.

Regression cases cover source/input binding, missing metadata, failed reruns,
separate-call accumulation, threshold policies, changed criteria, and serialization
compatibility. They have not been executed: the user requested no checks. No build,
test, performance, recovery, or perfect-quality claim follows from these edits.
