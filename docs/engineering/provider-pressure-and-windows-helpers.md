# Provider request pressure and Windows helper windows

## Observed causes and changes

The user reported repeated `Provider is temporarily busy` errors and brief terminal windows during background work. The TUI maps non-policy HTTP 429 responses to that message. Local log inspection counted repeated 429 mentions; these counts include repeated logging of the same error and are not a measurement of unique requests.

The existing provider admission module was absent from the module tree and had no callers. The transport's previous shared cooldown acted only when a response supplied `Retry-After`. The module is now compiled and called by the shared initial-response transport:

- Admission state is keyed by origin and shared by sessions in the same process.
- The default ceiling is two simultaneous requests awaiting headers; environment overrides remain available.
- Contending requests are paced. After capacity errors, pacing also applies to rapid sequential starts.
- HTTP 429 and 503 trigger shared cooldowns. Explicit server hints are honored; absent hints use 5, 10, 20, 40 and then at most 60 seconds. Concurrent shared-budget work retains long server cooldowns and returns a deferred error when an automatic wait would exceed 180 seconds, rather than retrying before the deadline.
- Consecutive capacity failures reduce concurrency. Recovery requires a run of successes; a success never clears a promised cooldown.
- Permits release on headers, error or cancellation, rather than remaining occupied for the streaming body.
- Idle registry entries are reclaimed before using a bounded overflow gate.

Gemini and Copilot request starts and OpenAI compaction also use the shared transport. Gemini now returns its first HTTP 429, including the structured server retry deadline, to the existing bounded agent retry loop. It no longer multiplies that loop by five internal rate-limit attempts. Other transport failure retries remain available.

This does not increase provider quotas or guarantee that a busy upstream will accept a request. The concurrency ceiling itself is per process. Concurrent checkout work added a persistent request-budget module and optional background-memory admission, with shared request-start windows and cooldowns across local processes. Provider SDKs/CLI subprocesses, catalog/usage paths that bypass these helpers, token-per-minute quotas, and long-lived stream concurrency after headers still require separate accounting.

## Background windows

Captured source-state and binary-version probes, MCP stdio servers, observer hooks, authentication/GitHub/Tailscale discovery helpers, Git/Cargo update and diagnostic helpers, self-development probes/builds, background rebuild Cargo calls, Windows terminal-location probes, plan searches, the Go installer launcher, and the server's captured input shell now use the existing `CREATE_NO_WINDOW` helper. Explicit terminal session launches retain their existing behavior. An installer or GUI program may still open its own UI; the console flag cannot suppress arbitrary child application windows.

A Windows regression test launches a child test process with the helper and checks `GetConsoleWindow()` inside that process. This tests the actual child creation flag rather than matching command text.

## Forwarded-header laboratory experiment

`httpflow` already supports explicit custom headers on scoped requests. A loopback-only regression fixture compares two `X-Forwarded-For` values against:

1. A route that keys its limit on the socket peer address.
2. An intentionally vulnerable route that trusts the supplied header as its client identity.

The fixture checks that the actual peer address remains loopback for every request. Expected results are 200 then 429 for the first route and 200 then 200 for the vulnerable route. This establishes a fixture's proxy-trust weakness, not source-IP rotation or a provider bypass. No forwarding-header rotation was added to provider traffic, and no external provider or target was probed.

## Verification

Results are recorded after the final code checks below. Initial investigation foundation validation completed with 6,696 passed and 15 ignored before these follow-up changes.


## Verification record (2026-11-13)

- The expanded full library suite passed with **6,709 passed, zero failed, 15 ignored**. This includes the Windows helper child-process console assertion, the scoped forwarded-header fixture, all ten enabled gate tests and the Gemini first-429 regression.
- A prior expanded run passed 6,708 tests before the additional Gemini test was added.
- One attempted targeted-test relink failed with Windows LNK1104 while the existing test executable was running. The serial full-suite run above subsequently linked and passed. No linker configuration change was needed.
- Formatting and strict Clippy passed for these changes. A final check of the concurrent persistent-budget integration is recorded separately below.
- No provider-account requests, externally hosted lab probes or production-rate benchmark was performed. Applying source changes to an installed running executable requires a rebuild and restart.

## Files changed by this follow-up

Provider admission and request paths:

- `src/alphacode_provider_core/gate.rs`
- `src/alphacode_provider_core/mod.rs`
- `src/alphacode_provider_core/transport.rs`
- `src/alphacode_provider_gemini_runtime/mod.rs`
- `src/alphacode_provider_copilot_runtime/mod.rs`
- `src/alphacode_provider_openai_runtime/openai_provider_impl.rs`

Windows captured-helper paths and the console regression:

- `src/alphacode_base/platform.rs`
- `src/alphacode_base/github.rs`
- `src/alphacode_base/gateway.rs`
- `src/alphacode_base/auth/copilot.rs`
- `src/alphacode_base/auth/cursor.rs`
- `src/alphacode_base/mcp/client.rs`
- `src/alphacode_base/hooks.rs`
- `src/alphacode_build_support/mod.rs`
- `src/alphacode_build_support/source_state.rs`
- `src/alphacode_build_support/paths.rs`
- `src/alphacode_app_core/update.rs`
- `src/alphacode_app_core/system_diagnostics.rs`
- `src/alphacode_app_core/session_launch.rs`
- `src/alphacode_app_core/session_rebuild.rs`
- `src/alphacode_app_core/bugbounty_install.rs`
- `src/alphacode_app_core/server/client_actions.rs`
- `src/alphacode_app_core/tool/selfdev/setup.rs`
- `src/alphacode_app_core/tool/selfdev/status.rs`
- `src/alphacode_app_core/tool/selfdev/build_queue.rs`
- `src/alphacode_app_core/tool/plan.rs`

The loopback-only header experiment is in `src/alphacode_app_core/tool/httpflow.rs`. This documentation is `docs/engineering/provider-pressure-and-windows-helpers.md`. Earlier changes in several of these files were already present in the shared checkout and were retained.

The concurrently added persistent request budget, long-hint handling and sidecar integration are separate in-flight work; they were preserved and included in final focused verification rather than attributed to this follow-up.
