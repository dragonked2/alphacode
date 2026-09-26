# Changelog

All notable changes to Alphacode are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/).

## [1.0.64] - 2026-09-26

### Fixed

- Prevented the health reporter from aborting AlphaCode on freshly booted systems when it constructed a 30-minute `Instant` cutoff.
- Hardened related resource-history and OpenRouter version-cache time handling against monotonic-clock underflow.
- Stopped the OpenRouter client version cache from leaking a new static string on every request and preserved the last valid version after a failed refresh.
- Fixed Firefox browser-bridge evaluation so `return <expr>`, top-level `await`, and legacy bridge calls are supported; upgraded the bundled extension to 1.6.0, moved installation to a versioned XPI path to avoid Windows file locks, and strengthened readiness diagnostics.
- Hardened reconnect bootstrap so a successful socket is not treated as a successful session attach until `SessionId`, `History`, and `Done` are correlated; added bounded timeouts, delayed/missing-session fault coverage, and terminal handling for invalid session targets.
- Fixed Windows named-pipe protocol flushing and added a detached server-reload handoff that waits for the predecessor before binding, preserving reload markers and recovering from dead predecessors.
- Added conservative tool execution classes and bounded concurrent execution for consecutive read-only batch calls while keeping mutations ordered.
- Added gateway bind/accept retry supervision and live runtime state reporting.
- Synchronized the committed lockfile with AlphaCode 1.0.63 so locked builds start reliably.

### Changed

- Pinned CI test/lint jobs to the repository's Rust 1.94.1 toolchain and enabled locked dependency resolution.
- Made release artifacts use the committed lockfile instead of mutating dependencies during publication.
- Added a product-wide 100× reliability, performance, accuracy, UX, tools, skills, security, and quality execution plan.

## [1.0.60] - 2026-09-23

- Initial release.
