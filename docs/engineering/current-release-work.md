# 1.0.93 integration

This release integrates the user-requested gold GUI/TUI theme, responsive block
banner family, compact composer with animated focus, connected-provider model
metadata/picker, and Settings connections that reuse CLI authentication and
credential storage. Azure and Bedrock configuration precedes secret input;
credentials never enter GUI preferences or process arguments. Browser/device
flows are cancellable and refresh provider/model availability after completion.

Session todo storage, plan participant filtering, and session-change render
cache resets prevent earlier work from appearing in a fresh session without
removing saved work.

Security workflow improvements emphasize official scope, reproducible evidence,
coverage ledgers, impact-based severity, and honest limits. Shared scope matching
handles parsed authorities, URL origins/path boundaries, exclusions and IPv6.
Malformed Unicode hex colors cannot panic the palette parser. Eight previously
unregistered manuals now ship in the embedded catalog; catalog hints are bounded
while complete manuals remain available on demand.

Tool-policy denials produce paired error results in both turn paths and do not
offer unrelated model fallback. Browser open can recover an absent tab target
by creating an owned tab. Curl operands and shell body markers are parsed
without treating endpoint names as local destructive commands; actual output
paths and destructive operations retain protection.

CI uses an explicit Intel macOS runner for Intel builds, a deterministic color
baseline, and unoptimized functional tests while keeping performance tests in
release mode. Windows archives use Python ZIP/checksum creation and contain
both the CLI and desktop host. Publication waits for all eight platform builds
and a complete checksum manifest.

## Evidence

- Prior complete color-enabled library run: 6,479 passed, one prefix-budget
  failure, 11 ignored. Shorter skill hints address the budget without raising it.
- Current workspace/test compilation passed; final full suite and strict Clippy
  are rerun after integration, with actual outcomes in the delivery message.
- Actual-source command-risk harness: 72 passed, including curl false-positive
  and protected-path regressions.
- Native GUI smoke passes across dark/light themes and full/compact windows;
  verifies model filtering, masked provider controls, no key in preferences,
  drafts, command input, conversation rendering, work panel and bridge actions.
- No live external bounty target or account was tested.

See [developer-report-audit-1.0.93.md](developer-report-audit-1.0.93.md) for the
report-by-report assessment, evidence boundaries and external items (upstream
429 availability, signing certificate, antivirus vendor verdict).
