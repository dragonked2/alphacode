---
name: solver-engineering
description: Reproducible, bounded CTF solvers with local oracles, category-specific constraints, and reliable remote adaptation.
---

# Solver engineering

Use for a challenge with artifacts or a credible hypothesis. Stay within the
competition's services and rules. Inspect supplied code before executing it;
preserve originals and work in a disposable directory or isolated environment.

## Shrink the problem before spending compute

Record input grammar, output oracle, controllable bytes, hidden state,
invariants and success condition. Build the smallest local harness that reaches
the relevant branch. Capture a known-valid and known-invalid input first. Every
new test should distinguish explanations or reduce a search space. Keep seeds,
tool versions, hashes and commands with the result.

- Web: reconstruct route, session and state transitions from provided source.
  Separate proxy behavior from the application; test a private fixture, then
  adapt only the observed deployment difference. Pin framework/runtime versions
  in a local replica rather than using an arbitrary latest image.
- Pwn/reversing: determine architecture, ABI, endianness, protections, loader and
  libc/build identity. Derive offsets in the debugger. Reproduce a crash and
  confirm the controlled primitive before constructing a chain. Bound symbolic
  execution to the relevant function, constrain input grammar, and validate any
  satisfying assignment against the actual binary. A solver model can be wrong.
- Crypto: write down the algebra and exact oracle first. Establish modulus,
  units, signedness, padding and sample quality. Check attack prerequisites
  (nonce reuse, shared factors, leakage bounds) before running expensive tools.
  Confirm recovered candidates by reencryption, signature checks or an independent
  original relation; plausible plaintext alone is weak evidence.
- Forensics: preserve hashes, offsets, extraction provenance and time zones.
  Validate magic bytes and structure after decoding. Bound decompression and
  recursion. Correlate timelines and independent artifacts instead of treating
  an incidental flag-shaped string as the challenge objective.
- Jails/VMs: model the exact parser, allowed operations and reachable objects.
  Reproduce in the supplied interpreter/version. A rejected syntax variant does
  not refute a semantic path; identify which restriction rejected it.

## Make the successful path repeatable

Separate transport from the transformation or exploit logic. Use explicit
lengths, byte encodings, receive delimiters, deadlines and bounded retries.
Persist intermediate constraints and expensive results keyed by artifact hash
and relevant parameters. Invalidate caches when those inputs change. Put finite
bounds on fuzzing, search and solver runs; report timeout as unknown.

Test the candidate against the local oracle, a negative control and a fresh
process. For nondeterministic challenges record repeated outcomes and the seed
or race assumptions. Adapt remote framing, ASLR, dependencies and timing from
observations. Do not repeatedly resend a state-changing step after a timeout
without checking whether it already took effect.

A reusable solve artifact should accept the challenge input/endpoint explicitly,
report failed assumptions clearly, and reproduce the objective without manual
hidden steps. Keep credentials and returned flags out of reusable public notes.
Use the platform's actual success signal where available and permitted; an
intermediate foothold or local fixture flag does not establish a remote solve.

## Evaluate improvements

Use a fixed authorized challenge corpus with hashes and versioned environments.
Measure time to first useful observation, time to verified objective, solve rate,
requests, tool failures and compute cost. Include unsolved runs in the denominator.
Compare changes under equivalent model/settings and budgets. Record leakage
from prior writeups; memorized solutions are not unseen-challenge accuracy.
Keep improvements that raise verified outcomes or reduce cost at equal coverage.
Do not claim a speedup from shorter prompts or more parallelism without measuring.
