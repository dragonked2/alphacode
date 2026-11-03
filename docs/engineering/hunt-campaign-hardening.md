# Evidence-led hunting campaigns

The normal system prompt and bundled bounty entrypoint now translate requests
for exhaustive hunting into accountable authorized coverage. Finding counts do
not determine severity, termination, or scope. The investigation controller adds
a campaign loop connecting asset inventory, user journeys, protected invariants,
experiments, evidence, root causes, cleanup and remaining work.

The entrypoint no longer prescribes default credential attempts, blanket token
mutation, a universal parallel-request count, or anecdotal yield claims. It no
longer ends an entire program after its first finding or several negative tests.
Further chain validation follows existing authorization and the requested
consequence. Sufficient proof ends that proof; independent coverage continues.

## Runtime fixes

- `BugBountyContext::is_in_scope` uses the shared structured matcher instead of
  substring matching. It includes the previously ignored IP ranges and URL
  exclusions. Empty or unmatched allowlists do not authorize this context.
- The shared scope gate parses URL origins and normalized path boundaries.
  An allowed URL cannot authorize a hostname suffix lookalike, a different
  scheme/port, `/api-admin` under `/api`, or a normalized traversal outside it.
- Host normalization uses the URL parser for authority handling, fixing IPv6
  port splitting and backslash/userinfo disagreement with network clients.

Plain domain rules retain the shared matcher's existing apex-and-subdomains
semantics; exact hosts are represented by `ScopePattern::Host`. Program policy
must be translated accordingly. This change does not invent a policy importer,
add action-level authorization, or prove that every network path uses this gate.
Destination redirects and downstream server-specific path decoding still need
their existing scope checks and controlled validation.

## Verification and performance claims

Regression cases cover allowed controls, lookalike hosts, domain text in paths
and queries, userinfo/backslashes, normalized traversal, origin differences,
IPv6, empty policy, and explicit IP/URL exclusions. Repeated slashes in an
explicit prefix remain restrictive, and WebSocket URL scopes preserve their
scheme boundary. A standalone harness importing the actual context and gate
source passed all 22 module tests with the final source. Harness artifacts are
under the configured temporary target directory, outside OneDrive.

The skill-creator validator accepts the revised investigation and scope
references. Its generic schema rejects the bounty entrypoint's pre-existing
Alphacode-specific `auto-invoke`, `triggers` and `aliases` fields. Those fields
remain intact; the YAML parses and the project's bundled-skill checks validate
the embedded content.

These are instruction and deterministic matching changes, not model training.
No live program was tested. No gain in vulnerability recall, accepted reports,
or performance relative to a person has been measured. Compare revisions with
held-out authorized fixtures and equal models, tools and budgets, retaining
failures and blocked coverage as well as successes.


## Additional issues found during verification

The shell inherited `NO_COLOR=1` and `TERM=dumb`. The initial library run had
6,426 passes, 45 failures and 11 ignored tests. A rerun of that same binary with
`NO_COLOR` removed and `COLORTERM=truecolor` reduced the failures to nine, with
6,462 passes and 11 ignored. Production accessibility behavior was preserved;
color/motion assertions require an explicit color-enabled test environment.

The remaining failures exposed stale expectations after the documented visual
refresh: palette snapshots, help heading case, onboarding privacy copy, orphaned
literal colors, and a supposedly unreadable fixture that inherited bright
foreground defaults. The snapshots now freeze the intended slate-and-mint values;
error/warning literals use semantic colors; the obsolete literal sample entries
are removed; and the hostile fixture explicitly covers every foreground role.
These changes retain the assertions instead of relaxing their thresholds.

Inspection also found that `parse_hex` could panic on a six-byte Unicode string
when slicing through a codepoint. It now rejects non-ASCII input before slicing,
with invalid Unicode regression cases. An isolated harness importing the color,
palette and theme sources plus byte-identical harmony module copies passed
75 tests, with one existing ignored test. This includes the new parsing cases,
frozen palette checks, literal coverage and calibration. Harness files remain
under the configured temporary target directory.
