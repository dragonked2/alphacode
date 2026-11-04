# Hunting decisions and local extension imports

Coverage now distinguishes completed target observations from errors, skips and
planned tests. Those incomplete attempts stay eligible for retry and do not
inflate endpoint or vulnerability-class coverage. A failed retest retains earlier
positive evidence until a completed retest resolves it. Signals are calculated
per endpoint across methods; another endpoint cannot keep a refuted signal alive.
Broad testing no longer removes an unresolved candidate from the follow-up list.

Risk prioritization counts each evidenced factor once, ignores blank evidence and
invalid numeric weights, and orders scope before scores or user overrides.
Unknown component IDs are not treated as authorized. Lower-priority components
remain in the plan. The experiment and report references explain failed
prerequisites, independent evidence, intermittent reproduction, root causes and
eligibility uncertainty.

`/add` browses the client session's current directory. `/add <directory>` browses
another directory, while a selected file of any name is parsed as a skill. Quoted
paths, relative/absolute paths and home expansion are supported. Validated skills
are installed globally under `~/.alphacode/skills/` in the standard layout, with
standard resource directories. Existing directories are preserved. Loose-file
imports do not copy unrelated adjacent files. Resources are copied without
execution; linked and overly nested resources are rejected.

Local plugin directories and `plugin.json` paths use Alphacode's plugin installer.
`/add` preserves installed plugins and refreshes the client skill list. Plugin
tools remain unsupported. Imports are local to the terminal client; separate
remote servers do not receive uploads. The browser reuses the existing action
picker, with file-specific copy, filtering, keyboard selection and cancellation.
Explicitly activated skill instructions accompany connected user turns through
the existing system-reminder field, so the server receives the selected workflow
even when its skill snapshot predates a local import. Retries retain that reminder.

Limits: the coverage key still consists of endpoint, method and vulnerability
class; actor, fixture and revision context belong in evidence and experiment
notes. Retained-field checks and risk scores cannot establish vulnerability
semantics. These changes do not train a model or demonstrate improved bounty
acceptance or recall. No live target was tested.
