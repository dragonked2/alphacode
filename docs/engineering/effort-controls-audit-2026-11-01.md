# Effort controls and lifecycle audit — 2026-11-01

This pass reviews effort as a connected control path: shortcut and slash input, model-picker selection, defaults, provider capability lists, transport, acknowledgment, persistence, and session restoration.

- A shared bounded stepping function replaces duplicated cycling logic. Unknown defaults start from an ordinary reasoning level, never implicitly selecting a swarm mode. Explicit progression into swarm modes remains possible.
- OpenAI catalog effort levels are canonicalized, deduplicated and ordered. An explicitly empty capability list exposes no selectable effort, including swarm sentinels. Explicit invalid levels are rejected. Default reasoning is applied only when supported by the current model's capability list, keeping display and request construction consistent.
- OpenAI and compatible runtimes accept default/auto resets. Resetting a saved remote OpenAI default now also requests a reset of the active server session.
- Local changes and model switches persist the provider's effective effort into session state. Model-picker effort failures are surfaced instead of discarded. Session restoration synchronizes normalized values and publishes actual provider state to the swarm side table rather than a failed request.
- Remote requests are tracked separately from confirmed effort. Transmission failures do not alter confirmed state. Acknowledgments update the confirmed state, retain any newer pending request, clear rejected requests, invalidate picker state and request a redraw. Untracked and stale acknowledgments are ignored; disconnects and session changes clear tracking.
- Autocomplete uses the same capability ladder as the controls and includes supported orchestration choices. Help explains reasoning effort, default reset, waiting for confirmation, and the additional-agent behavior of swarm levels.
- Offline remote controls do not modify the local stand-in provider.

Remote capability lists still use existing inference when the server has not supplied a model-specific capability list; the server remains authoritative about acceptance. These changes improve control correctness, not the intrinsic reasoning quality of the serving model. No live-provider quality or latency claim is made.

Regression tests cover unknown/default stepping, boundaries, incomplete and unordered metadata, default reset, unsupported defaults, remote reply ordering/rejection, capability-aware suggestions, and session persistence. The broader suite also identified and fixed remote F1's open/close toggle.

## Final verification

- cargo fmt --all -- --check: passed. Included test sources touched in this pass were also formatted directly.
- cargo check --workspace: passed.
- cargo check --workspace --tests: passed.
- cargo clippy --lib -- -D warnings: passed.
- cargo test --lib: 6,401 passed, 0 failed, 11 ignored; 338.63 seconds of test execution on the final source.
- The initial-attach/session-switch effort regression also passed independently before the full final run.
- git diff --check: passed.

Builds used VS 2022 MSVC and CARGO_TARGET_DIR=C:\Users\or0to\AppData\Local\Temp\alphacode-target, outside OneDrive. The test process used CARGO_PROFILE_TEST_OPT_LEVEL=0 to shorten first-party test compilation; Cargo.toml release/development settings were not changed by this pass. TERM=xterm-256color was set and NO_COLOR was absent in the verification process. No live-provider quality/latency benchmark or terminal-emulator shaping test was performed.
