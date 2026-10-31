# Prompt Budget Architecture — Progressive Disclosure

**Date:** 2026-10-28
**Status:** implemented baseline

## 1. Problem (measured, not estimated)

Measured through the real code path (`Registry::new` → `definitions(None)` and
`build_system_prompt_split_with_tier(Standard)`):

| Component | Chars | ~Tokens | Share |
|---|---:|---:|---:|
| Tool input schemas (81 tools) | 106,290 | 26,572 | 43.2% |
| System prompt markdown | 91,007 | 22,751 | 37.0% |
| Tool descriptions | 37,865 | 9,466 | 15.4% |
| Skills catalog | 7,507 | 1,877 | 3.1% |
| **Total static prefix** | **246,010** | **~61,502** | 100% |

The payload is sent on **every** request. Two facts drive the design:

1. **Schemas dominate, not prose.** 73% of all tool bytes are JSON Schema.
   Trimming description text alone cannot reach a 5–7k budget.
2. **The tool surface is mostly irrelevant per turn.** 44 of 85 tools belong to
   overlapping families (4 file-edit tools, 5 crawlers, 5 CTF-forensics tools,
   3 subdomain tools, …). A Rust build turn never needs `sqlmap` or `tshark`.

## 2. Design: progressive disclosure

`ToolRegistry` already separates **execution** (`self.tools`: every tool, always
callable) from **advertisement** (`definitions()`: the prompt-visible subset).
That separation is the mechanism: advertise few, keep all callable, and let the
model promote tools on demand.

### 2.1 Tool tiers

| Tier | Contents | Always sent |
|---|---|---|
| `core` | read, write, edit, apply_patch, bash, alphagrep, ls, todo, webfetch, websearch, batch, apply_patch | yes |
| `standard` | core + httpflow, scrapling, browser, httpfetch, memory, skill_manage, discover_tools, side_panel, … | yes (default) |
| `extended` | recon, CTF, media, cloud, swarm, ambient tools | on promotion |
| `always` | tools the session explicitly allow-listed | yes |

Promotion is per-session and sticky: once advertised, a tool stays advertised so
the model's learned call shape does not rot mid-turn.

### 2.2 Promotion path

`tool_search(query)` / the existing `discover_tools` surface lets the model pull
in a capability family. A promoted family is added to the session's advertised
set and the tool snapshot is unlocked exactly once (one intentional cache miss),
mirroring the existing MCP late-registration path (#206).

### 2.3 System prompt tiers

`PromptTier` already exists (`Minimal` / `Compact` / `Standard`). The change is
budgeting `Standard`:

- Keep the operating contract: identity, scope/authorization, execution
  discipline, evidence gates, reporting, communication.
- Move the 319-module field playbook and the long security class catalog behind
  the already-bundled `playbook` skill (its Section 43 pointer already says the
  library is on-demand).
- Cap the always-on markdown well under the current 91 KB.

### 2.4 Known waste removed

- `swarm` carried a 32,603-char definition (8,150 tokens) because the entire
  `swarm_prompt.md` (22 KB) was concatenated into the tool description. Moved
  behind the tool's own documentation + skill.
- Duplicate families collapse into one advertised entry plus promoted members.

## 3. Acceptance criteria

- Standard-tier static prefix ≤ 7,000 tokens on a fresh session.
- Every tool remains callable when promoted (no capability loss).
- Prompt-cache stability preserved: stable ordering, one intentional miss per
  promotion.
- `cargo test --lib` green.

## 4. Verification

`cargo test --lib prompt_budget` reports the measured prefix and fails if it
regresses past the budget.
