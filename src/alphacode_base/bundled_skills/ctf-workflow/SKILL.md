---
name: ctf-workflow
description: "CTF and lab playbook: challenge triage, the vulnerability battery, signal-driven exploration, exploit-and-solve phases, hypothesis kill rules, and the built-in CTF arsenal. Load when solving CTF, lab, sandbox, or intentionally vulnerable challenges."
auto-invoke: false
---

# CTF and Lab Workflow

This workflow is included automatically for explicit CTF requests and workspaces
with a `challenge.md` ledger. It can also be loaded with `skill_manage`
(action=load, name=ctf-workflow). The bundled `ctf` skill carries category-specific
techniques.

# CTF / Lab Mode

## Mandatory evidence checkpoint

Before probing, read the challenge descriptions, attached files, provided hints,
and explicit scope. Create `challenge.md` in the session workspace using `write`;
do not overwrite an existing ledger. AlphaCode re-reads this file into each
system prompt, including after compaction. Keep its current-state summary short (under 12 KiB); archive raw transcripts separately.

Record: challenge IDs and objectives; exact hints with source attribution;
authorized origins; observed headers and stack evidence; known credentials with
their source (never publish them); endpoint/status map; open hypotheses with a
discriminating next experiment and attempt budget; closed hypotheses and the
evidence needed to reopen them; verified flags and platform confirmation.
Checkpoint after every material finding and before changing hypotheses.

Read server headers first. `X-Powered-By: Express` is evidence to investigate,
not a conclusive fingerprint; do not assume Flask or any other stack without
corroboration. Revisit hints before trying increasingly obscure explanations.
For a corroborated Next.js or Node.js challenge, load `nodejs-security` with
`skill_manage` and apply the relevant framework controls within challenge scope.

Use `ctf_web_recon` for a bounded initial pass and evidence-selected same-origin
paths. If its schema is not advertised yet, discover it with `tool_help` first.
Its missing-page control helps detect wildcard 200/403 responses. Keep
200, redirects, 401, 403, 404, 429 and 5xx separate. A 403 is a forbidden response,
not proof that the requested route exists; compare a missing-route baseline.
Stop on rate limits. Do not use a server error alone as exploit evidence.

Limit each hypothesis to three initial discriminating experiments. Three
equivalent results trigger a control and reachability check, not automatic
refutation. Masked or non-discriminating results remain inconclusive; changing a payload's spelling does not create a
new hypothesis. After ten probing tool calls, checkpoint and pivot. Reopen a
closed hypothesis only with recorded new evidence. Never turn failed guesses
into unbounded password loops: use only credentials supported by challenge
artifacts or hints, unless a finite search space is explicitly the challenge's
intended task. Record its bound, scope, stop condition, and time budget first.

The checkpoint reminder prompts a ledger update and a change of hypothesis
when the evidence warrants it. Continue with the next authorized experiment
until the objective is verified or a concrete blocker prevents progress.
A per-pass timeout leaves partial observations available for the next pass.

Browser work uses a background tab owned by this session. After tab
death or a timeout, inspect state before repeating an action. Never retarget
the user's active tab, and never blindly replay eval, clicks, or submissions.

For CTFs, labs, sandboxes, intentionally vulnerable applications, and challenge environments, use an intensive, experiment-driven workflow within the established environment.

Activities may include:

- Reverse engineering binaries.
- Exploiting intended vulnerabilities.
- Writing exploit scripts.
- Decoding cryptography challenges.
- Enumerating services.
- Brute-forcing challenge credentials.
- Analyzing packets.
- Building challenge payloads.
- Developing shellcode where appropriate.
- Performing privilege escalation.
- Extracting flags.
- Automating repetitive challenge steps.

Avoid unnecessary damage to the host beyond what is needed to solve the challenge.

Alphacode exposes wrappers for a CTF arsenal (verify advertised schemas and installed dependencies first): `pwntools`, `radare2`, `ghidra`, `angr`, `binwalk`, `steghide`, `exiftool`, `volatility`, `tshark`, `z3`, `hashcat`, `john`, plus the `flag_scanner`, `challenge_classifier`, and `ctf_auto_solver` helpers. Reach for the matching tool instead of hand-assembling bash equivalents; `challenge_classifier` is useful for a fast first pass and then run targeted tools yourself.

For artifact-driven category analysis and reproducible scripts, load this
skill's `solver-engineering` reference with `skill_manage` (action=read).
It covers local oracles, constrained search, remote adaptation and evaluation.

## CTF workflow for web challenges

### Phase 1 — Triage and vulnerability battery

Initial budget: approximately 0–15 minutes.

1. Inspect challenge metadata, points, solves, tags, files, and stack.
2. Map entry points and user-controlled inputs.
3. Identify likely trust boundaries and backend technologies.
4. Select a small initial set of payloads based on the actual input context.
5. Test relevant input-handling classes such as quoting, boolean behavior, template evaluation, traversal, type confusion, and command interpretation.
6. If wildcard or substring matching is observed, investigate whether the behavior comes from application logic, a database query, or a search engine before attributing it to SQL injection.

Do not blindly fire every payload at every input. Prefer tests that distinguish competing hypotheses.

### Phase 2 — Signal-driven exploration

Approximate budget: 15–40 minutes.

1. Map parameters that materially change response behavior.
2. Track hypotheses in a ledger.
3. Set a time budget and a kill criterion for each hypothesis.
4. Investigate common, high-probability vulnerabilities before obscure theories.
5. Use a short initial budget for ordinary classes and a somewhat larger budget for genuinely complex protocol, cryptography, or reverse-engineering work.
6. Treat flavor text as weak evidence unless corroborated by implementation or behavior.
7. Reallocate time as new evidence changes the expected value of each hypothesis.

### Phase 3 — Exploit and solve

Once a credible path is established:

1. Build the smallest reliable exploit.
2. Reproduce the relevant behavior.
3. Complete the intended challenge objective.
4. Capture the result.
5. Record the reasoning and reusable technique.
6. Verify the challenge's actual terminal state.

Do not claim a solve based on a matching word or intermediate response.
For protected-content challenges, compare the exact returned field or artifact
with the public preview and a known challenge control. A lock flag, omitted
byte count, blurred placeholder, reflected input, or success status does not
demonstrate access to the hidden payload. Treat unequal or truncated captures
without a verified challenge marker as inconclusive. Record a solve only when
the recovered flag or required state satisfies the challenge's success check.

## Hypothesis kill rules

- Give every hypothesis a time budget.
- Define what observation would confirm or disprove it.
- When the budget expires, reassess the evidence.
- Distinguish refuted predictions from blocked, paused, and inconclusive tests; a timeout or repeated wildcard response is not refutation.
- At regular checkpoints, reprioritize by information gained and record why stale paths are paused.
- Keep a limited combined budget for exotic explanations until ordinary explanations have been tested.
- Reopen a rejected hypothesis only when new evidence materially changes the analysis.

The objective is fast convergence, not attachment to the first theory.

---
