# Authorization and pacing hardening — 2026-11-16

Status values: `IMPLEMENTED`, `TESTED`, `VERIFIED`. Nothing here is marked
`VERIFIED` without an executed check recorded in the evidence section.

## Scope of this tranche

This tranche addresses defects found by inspecting the single centralized gate
that every tool call passes through (`alphacode_app_core::tool::Registry::execute`
→ `tool::scope::enforce_call`). It does not attempt the full architecture,
TUI, or research-methodology program; see "Remaining work" for the honest
boundary.

## Defects found and corrected

### 1. `bash` was outside engagement scope enforcement — IMPLEMENTED, TESTED

`is_network_tool()` enumerated ~30 scanner names. `bash` was not among them, so
`enforce_call` returned `Ok(())` before reading any input. A scoped session
could therefore run `curl https://out-of-scope.test`, `nmap`, `ssh`, or any of
the scanners the list *did* name, simply by invoking them from a shell. The
centralization rationale in the module header ("the thirty-first tool is the one
that forgets") was defeated by the one tool that can invoke all of them.

Correction: `scope::shell_network_targets` splits a command with the existing
`alphacode_command_risk::split_segments` tokenizer, identifies segments whose
program is in `SHELL_NETWORK_PROGRAMS`, and extracts destination operands.
Value-taking flags (`-H`, `-o`, `-d`, `--wordlist`, ...) are consumed as data so
headers, bodies and output paths are not mistaken for hosts. `-l`/`--list` names
a destination list that is read with the same bounds and truncation sentinel as
the existing scanner path. Tokens naming an existing local file are treated as
file operands (`ssh -i key`, `curl --data @body`), not hosts.

A destination that only exists after shell expansion (`curl "$T"`,
`curl $(cat u.txt)`, `%TARGET%` under cmd.exe) is refused rather than assumed
in-scope, because "unknown" must not silently mean "allowed".

### 2. `browser` and `open` reached the network ungated — IMPLEMENTED, TESTED

Both tools load a caller-supplied URL and were not in `is_network_tool`. They are
now gated on their destination field, but deliberately *not* reclassified as
scanner tools: that predicate also drives request pacing and probe-burst
accounting, and `open report.pdf` must not consume a network permit. Only
absolute `http`/`https`/`ws`/`wss` destinations are checked, so local paths,
`file:` and `mailto:` targets are unaffected.

### 3. `max_rate_rps` did not hold under concurrency — IMPLEMENTED, TESTED

Admission read "instant of the last admitted call" under a read lock, released
the lock, then slept. N concurrent callers read the same timestamp, computed the
same wait, and were admitted together — producing exactly the burst the policy
exists to prevent, in exactly the situation (swarm/parallel subagents) where it
matters. Admission now reserves its slot (`next_admission`) while holding the
write lock, so concurrent callers are spaced by the declared interval.
Reservations are cleared when a policy is replaced, so a new rate is not delayed
by a schedule made under the old one.

### 4. The agent could authorize its own engagement — IMPLEMENTED, TESTED

`investigation` with `action: "start"` accepted `in_scope` from model-supplied
tool arguments and installed it as an enforcing session `ScopePolicy`. A model
could submit `in_scope: ["0.0.0.0/0"]` and convert a session with no
authorization into a fail-closed gate that permitted every address. Prompts then
reported an installed scope, presenting the model's own claim back to it as
established fact. The tool's schema asserted "Records scope, never grants
permission", but recording *was* granting.

Correction: `in_scope` is accepted only when an operator-installed scope already
exists, and only records the operator-visible asset list. It cannot create,
widen, or replace a policy. Scope is installed solely by the operator-derived
bug-bounty path and by restoring a saved engagement. The schema text now states
the enforced behaviour instead of the intended one.

## Verification performed

| Check | Command | Result |
|---|---|---|
| Baseline before edits | `cargo fmt --all -- --check`; `cargo check --workspace --tests` | passed |
| Scope regression suite | `cargo test --lib alphacode_app_core::tool::scope::` | 29 passed, 0 failed |
| Formatting | `cargo fmt --all -- --check` | clean |
| Strict lints | `cargo clippy --lib -- -D warnings` | clean (after fixing the `out_of_scope` dead-field finding) |

Strict clippy initially failed on `field out_of_scope is never read` after the
policy construction that consumed it was removed. The field is kept — models and
prompts still send it, and rejecting the call would break working flows — and is
now referenced by the guard so its role is explicit: it is recorded, never
applied as a narrowing filter.

New regression tests added (all in `tool/scope.rs`):

- `shell_network_segments_are_scope_checked` — allows in-scope `curl`, local
  shell work; denies out-of-scope `curl`/`wget`/`nmap`/`ssh`, including a chain
  where the in-scope call comes first.
- `shell_dynamic_destination_fails_closed` — `$VAR`, `${VAR}`, `$(...)` refused
  with actionable wording.
- `shell_program_must_be_the_segment_program` — `grep curl docs/` and
  `echo 'curl https://evil.test'` are not mistaken for network egress.
- `browser_and_open_destinations_are_gated` — remote URLs denied, local paths,
  `file:`/`mailto:` and URL-less browser actions allowed.
- `declared_rate_limit_holds_for_concurrent_admissions` — three concurrent
  admissions at 2 req/s span at least two intervals.
- `non_network_tool_not_blocked` — rewritten: the previous version asserted that
  `bash` running `curl https://evil.com` was *allowed*, i.e. it encoded the
  bypass as intended behaviour.

Test-harness corrections made along the way: admission tests now serialize on a
`tokio::sync::Mutex` because `GLOBAL_IN_FLIGHT` is process-wide and absolute
assertions were order-dependent; `a_slot_is_taken_exactly_once_when_the_wait_succeeds`
asserted a net delta when the correct invariant is the end state (the held
permit is released during the wait, so the delta is zero by construction).

## Known limitations of this tranche

- Shell coverage is program-name based. A custom script that performs HTTP
  (`python fetch.py`) is not detected. Extending this needs either a declared
  tool capability or command-content analysis beyond program names; guessing
  from text would deny ordinary work.
- `faraday`-style config-driven destinations (`curl` reading a config file) are
  not statically resolvable and are not denied.
- The `httpflow` scope installation in `tool/httpflow.rs` is test-only; the
  module-scope `set_session_scope` calls there remain compiled out via
  `#[cfg(test)]` context.

## Remaining work (not started)

- Persisted engagement scope has no provenance field, so an operator-granted and
  a (legacy) model-derived policy are indistinguishable on restore. Adding
  `policy_source` requires updating an untracked in-flight file
  (`alphacode_tui/tui/app/commands_security.rs`) owned by another session; it was
  deliberately deferred rather than breaking that work.
- The `from_lists(..., 1)` four-argument call in `httpflow` tests and the
  six-argument calls elsewhere disagree in arity with the current signature in
  one test-only site; confirm on the next touch of that file.
- Full `cargo test --lib` (6941 further tests) was not run in this tranche.

## Concurrent-worktree note

This checkout is shared: while this tranche ran, another session added
`src/alphacode_tui/tui/app/commands_security.rs` and edited `prompting.rs`,
`app.rs`, `commands_dispatch.rs`, `input_help.rs`,
`state_ui_input_helpers.rs` and a bundled `SKILL.md`. A build failure
("file not found for module `commands_security`") was caused by that file
appearing mid-build, not by these changes. Those files were left untouched.
