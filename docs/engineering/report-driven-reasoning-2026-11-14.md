# Report-driven reasoning upgrade

## Intended behavior

Alphacode should transfer a researcher's causal investigation logic to unfamiliar
products: challenge an assumption, name the invariant and prerequisites, choose
a discriminating experiment, assess counterexamples and verify actual effects.
Remembering old targets, payloads or severity labels does not accomplish this.

This change updates runtime guidance and a source-grounded curriculum; it does
not train model weights. No competitive ranking or universal 10/10 claim follows.

## Sources reviewed

- [codebygk/hackerone-bug-bounty-reports](https://github.com/codebygk/hackerone-bug-bounty-reports/tree/cb5d63cd158c4601978fee9687d65385853019b5),
  revision `cb5d63cd158c4601978fee9687d65385853019b5`. The metadata snapshot contains
  **10,000 unique report URLs**. SHA-256 of `data/reports.json`:
  `fc32c25136f84875e63d85942e2d5f59332057498243d6906d501631d8557ac8`.
  It contains titles, categories and links, not full report narratives. No repository
  license was declared at review time. The snapshot and full reports are kept in
  local study storage; they are not redistributed in Alphacode.
- Public HackerOne JSON responses for 23 reports: **14 narratives and nine
  summaries**. The initial curriculum distills **16 rules from 17 of those reports**.
  Sources, accessible-text hashes, basis and short grounding anchors accompany
  each rule. The other six collected reports have not been turned into lessons.
  **9,983 catalog reports remain without a grounded lesson.** This is an initial
  curriculum and a complete study inventory, not completion of all-report study.
- [elementalsouls/Claude-BugHunter](https://github.com/elementalsouls/Claude-BugHunter/tree/8ac4242ee0a108e65ac1343b56cce6c50ba456ef),
  revision `8ac4242ee0a108e65ac1343b56cce6c50ba456ef`. Read the methodology,
  red-team mindset, triage/validation, exceptional-condition and shadow-API
  references and the report-curation pipeline. Its code is MIT; its skill content
  is CC BY 4.0. The new adapted `researcher-reasoning` reference carries explicit
  attribution, license and a statement of changes. No upstream executable code
  or installer is incorporated.

The HackerOne source hashes cover the exact accessible UTF-8 narrative, or the
ordered joined summary content when no narrative is available. They identify
what was read; they do not certify a report's truth or independently reproduce it.

## Concrete changes

### Reasoning in every hunt

Security prompt assembly now includes the research discipline and 16 compact
structural questions from the shipped curriculum. This is rebuilt with routed
context after compaction. The model receives the general reasoning directly;
using historical-case lookup is not a prerequisite.

`hunt_reasoning` applies structural cues observed on a new product to fuller
rules containing invariants, challenged assumptions, causal steps, preconditions,
positive/negative controls, success oracles and benign counterexamples. It is
read-only and uses no network. Cue evidence is caller-declared, not authenticated.
Rules propose experiments; they cannot create proof, confidence, severity or scope.
They supplement first-principles analysis rather than limit coverage to known rules.

External reviewed lessons can be loaded from
`ALPHACODE_HOME/research/reasoning_lessons.json` (default `~/.alphacode/research/`).
Malformed imports and duplicate rule IDs fail visibly. External lesson text stays
tool-result data; it is never injected into the system prompt as instructions.

### Live controlled experiments

The live `investigation` tool now exposes the existing structured experiment and
reassessment engine. Plans can record controls, the expected invariant, success
oracle, actor/state/dimension, reopening condition and an attempt ceiling.
Expected behavior must agree with the ordinary plan field.

The ceiling counts actual planned dispatches before execution, including failed
reserved calls. A dispatched structured plan cannot be edited to reset that
ceiling. Pending reassessment and a non-continuation decision prevent further
dispatch; normal registry scope, permission and engagement budgets still apply.
These are tool-dispatch counts, not HTTP-request counts inside a tool.

`prioritize` exposes the existing information-gain/impact/cost/risk planning
estimates. They affect ordering only, never evidence confidence. `next` honors
cost/risk ceilings and skips spent or paused dispatch plans while retaining their
evidence and coverage records; no infinite retry of the top-ranked spent plan.

Attempts preserve interpretations separately from proof. Evidential outcomes
require attached recorded evidence and completed successful tool receipts;
arbitrary fingerprints or caller descriptions do not establish their truth.
Reassessment records the cause and an explicit continuation, new experiment,
pivot, pause, block or close decision. None promotes a finding.

The existing read-only streak reminder now uses an evidence checkpoint for
security work. Eight reads may be legitimate research; it no longer instructs
the hunter to mutate state or deepen exploitation merely to demonstrate progress.

### What was adapted from Claude-BugHunter

- Compare alternate consumers, representations, states and API versions.
- Revisit mapping when a new consumer or boundary appears.
- Separate a validation-layer response from reaching authenticated business logic.
- Track the cause of a blocker and preserve independent coverage.
- Curate causal mechanisms from sources with explicit citation and review.

The adaptation deliberately does not equate a stack trace with a reportable bug,
a non-404 response with a live route, an absent 429 with exploitable rate-limit
failure, a version label with a working exploit, or a vendor skill count with
performance. Owned canaries replace unnecessary credential theft or unrelated
data extraction. Current policy and budgets govern persistence and testing.

## Resumable full-collection study

From the checkout, Python 3 and the standard library are sufficient:

```powershell
python scripts/report_study.py prepare
python scripts/report_study.py collect --limit 25 --interval 1
python scripts/report_study.py status
```

The default root is `ALPHACODE_HOME/research/report-study`, otherwise
`~/.alphacode/research/report-study`. `--root PATH` before the action selects an
isolated study. `prepare --catalog PATH` accepts a previously downloaded exact
snapshot and validates its hash. The complete 10,000-ID inventory retains every
pending and unavailable record. Null titles do not discard reports.

Collection requests only the pinned catalog and public numeric report JSON URLs.
It follows no redirects, sends no cookies, downloads no attachments and executes
no report code. It verifies public status and report ID before saving. At least
one second separates requests in a pass; HTTP 429 checkpoints and ends the pass.
Cached reports resume without another network request. Unavailable and
rate-limited entries are retained for explicit operator reconciliation, not
silently retried or counted as studied. Use one collector per study root.

Drafting uses an explicitly selected trusted teacher command as a JSON argument
array. The command must consume `{input}` and create `{output}`. Inputs contain
the untrusted report text and a schema example. Process timeouts stop the process
tree using the existing evaluation runner. A successful exit without a fresh
valid output is a failure. Grounded output remains `drafted_needs_review`.
An empty candidate remains `insufficient_detail`. Drafting does not install rules.

```powershell
$teacher = ConvertTo-Json -Compress -InputObject @('YOUR-TEACHER', '--input', '{input}', '--output', '{output}')
python scripts/report_study.py draft --teacher-command-json $teacher --limit 10 --timeout 300
```

The chosen teacher can incur its own model costs; the runner records outcomes,
not API billing. Review and combine candidates into a complete curriculum using
`src/alphacode_security_core/reasoning_lessons.json` as the schema example. Review
whether the observation, mechanism, prerequisites and proposed controls are
actually justified; matching quotations are necessary but not semantic review.

```powershell
python scripts/report_study.py compile --reviewed path/to/reviewed-lessons.json --output path/to/reasoning_lessons.json
```

Compilation verifies accessible-body hashes, narrative/summary basis, exact
bounded source anchors, distinct controls, complete reasoning fields,
counterexamples, structural cues and unique rule IDs. It atomically writes the
curriculum before recording source coverage. A failed compile preserves the prior
artifact. Install only additional rule IDs in the external runtime file; the
initial 16 rules already ship in the binary. No teacher was run across all 10,000
reports in this change, and no all-report learning is claimed.

## Evaluation boundary

New tests exercise transfer to a differently named product, insufficient cues,
metadata/private-source rejection, source tampering, summary fidelity, pending
coverage, failed teacher output, rate-limit checkpointing, immutable experiment
budgets and actual dispatch suppression while reassessment is pending.

These are integration and regression tests. To measure hunting improvement,
compare before/after under the same model and budgets on fresh externally held
applications, including repaired and benign lookalikes. Independently adjudicate
unique root causes, precision, time to proof, cost and scope violations. No live
program was attacked and no comparison with XBOW or elite hunters was run.

Build/test results are recorded after verification below.
