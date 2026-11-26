# Evidence-based self-learning

**Objective.** AlphaCode should learn from its own executions: keep what was
verified, mark what was assumed, retain corrections that were proven right, and
stop repeating the same mistakes across sessions — without unbounded memory
growth or token waste.

This document is the design. It states what already exists, what is missing, and
the invariants that keep the feature from becoming a source of confident
nonsense.

## What already exists (do not rebuild)

Verified in `src/alphacode_memory_types/mod.rs` and `src/alphacode_base/memory.rs`:

| Primitive | Where | State |
|---|---|---|
| `TrustLevel { High, Medium, Low }` | `memory_types` | present; persisted, parsed from extraction output |
| `MemoryCategory::Correction` | `memory_types` | present, with a 365-day TTL and the highest retrieval weight (60) |
| `Reinforcement { session_id, message_index, timestamp }` | `memory_types` | present; `reinforce()` appends a breadcrumb |
| `supersede(new_id)` | `memory_types` | present; links a replaced memory to its successor |
| confidence boost/decay | `memory_types` | present; `effective_confidence()`, per-category TTL |
| hybrid retrieval (BM25 + embedding) | `memory.rs` | present |

So the substrate is right. The gap is that **nothing records where a belief came
from or why it is trusted**, so trust cannot be audited, raised on evidence, or
lowered when a belief is contradicted.

## The core defect this fixes

`TrustLevel` today is close to a label. Reading the call sites:

- extraction defaults unknown provenance to `Medium` (`turn_memory.rs`, `turn_execution.rs`);
- `Low` is used for things the agent inferred;
- `High` is reserved for the user's own statements and is otherwise assigned by
  hand (`goal.rs:648`, `skill.rs:1564`).

There is no path from *"this claim was checked and held"* to *"this memory is
now High trust"*, and no path from *"this claim was checked and failed"* to
*"this memory should decay or be superseded"*. That is the whole feature.

## Design

### 1. Provenance is a first-class field, not a tag

Add to `MemoryEntry`:

```rust
pub enum Evidence {
    /// A command ran and its output is the record. Carries what proved it.
    Verified { check: String, observed: String },
    /// The user said so. `High` trust comes from here.
    UserStated,
    /// The agent concluded it. Default `Medium`; may rise via verification.
    Inferred { basis: String },
    /// Checked and contradicted. Never retrieved as guidance.
    Refuted { contradicting: String },
}
```

`TrustLevel` becomes **derived**, not set:

| Evidence | base trust |
|---|---|
| `UserStated` | High |
| `Verified` | High |
| `Inferred` | Medium |
| `Refuted` | excluded from retrieval |

Keeping `TrustLevel` as a stored field for backward compatibility, but computing
it from `Evidence` on load, means existing memory files keep working.

### 2. A learning only lands when a check observed it

The rule that prevents confident nonsense: **a success or failure only becomes a
memory if some observable check produced it.** A tool exit code, a test result,
a diff that applied, a command's stdout. An assumption the model produced is
stored as `Inferred` with its basis, never as `Verified`.

This mirrors the `verified|assumed` distinction already required in the goal and
todo machinery (`feedback_loop`, `closed_feedback_loop`), so it is one concept
used in two places, not a new vocabulary.

### 3. Bounded growth: supersede, never append forever

Every new learning is matched against existing memories in the same
`(project, category, tag set)` bucket before insert:

- exact/near duplicate (normalized text above a similarity floor) → `reinforce()`
  the existing entry, do not insert a second one;
- contradicts an existing entry → `supersede()`: the old one is linked to the new
  one and stops being retrieved, but is retained for audit;
- otherwise → insert.

A hard cap per bucket (LRU by `effective_confidence × recency`) bounds the file
size. Correction and Verified entries are capped separately and evicted last,
because they are the highest-value rows.

### 4. Token cost is paid once, at write time, not per prompt

The failure mode of "learning" is a prompt that grows every session. Two rules:

- **Retrieval is budgeted.** `memory_prompt.rs` already assembles injected
  memories; give it a token budget derived from the model's context window
  (a fraction, not a count), and rank by `effective_confidence` so a large
  memory store does not mean a larger prompt.
- **Distillation on write.** A `Verified` learning is stored as a short
  imperative plus a pointer to the artifact that proved it (file path, test
  name, command), not as a transcript. The evidence is retrievable on demand;
  it is not injected by default.

So memory growth costs storage, not context.

### 5. Cross-session mistakes are keyed by signature, not by story

"Don't repeat this mistake" only works if the mistake is recognizable.
Store the failure as a **signature**: a normalized error kind (compiler error
code, failing test name, HTTP status plus endpoint shape) plus the fix that
worked. On a matching signature, retrieve the fix; do not re-narrate the
incident.

This is the same mechanism as `scripts/verify-*.py` guards in this repository:
a structural check that fails when a known-bad shape returns. The memory layer
should work the same way, which is why a failure memory must record the check
that detected it.

## Invariants to test

1. A memory created from a failed command is never `Verified`.
2. `Refuted` memories are never injected into a prompt.
3. Inserting a duplicate reinforces instead of growing the store.
4. A contradiction supersedes, and the superseded entry is not retrievable.
5. Prompt injection stays within the token budget regardless of store size.
6. Loading a pre-existing memory file (no `Evidence` field) assigns trust
   without error and without changing retrieval order.

Invariants 1–4 and 6 are unit-testable without a network or a model; that is a
deliberate constraint so the feature cannot regress silently.

## Non-goals

- No autonomous knowledge base the agent curates unsupervised. Every stored
  learning traces to a user statement or an observed check.
- No recall of anything the user asked to forget; `supersede` and deletion are
  the only paths, and both are explicit.
- No change to the existing extraction prompt's output contract beyond adding an
  optional `evidence` field; old outputs remain valid.

## Sequencing

1. `Evidence` field + derived trust + load-compatibility (no behaviour change).
2. Dedup / supersede on insert with the bucket cap.
3. Budgeted retrieval in `memory_prompt.rs`.
4. Failure signatures.
5. Tests for the invariants above at each step.

Step 1 is the only one that touches persisted format, so it ships first and
alone, making a rollback of later steps always safe.