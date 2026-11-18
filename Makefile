# Thin wrappers around the Cargo commands documented in AGENTS.md.
#
# CI runs `cargo` directly (see .github/workflows/ci.yml); these targets exist for
# developers and sandboxes where the toolchain lives outside `PATH`. `CARGO` is
# resolved from `PATH` first and then from the usual toolchain install roots, so
# `make test-orcarouter` behaves the same in a login shell and in a minimal
# environment that only has /usr/bin:/bin.
#
# Build output goes outside the working tree by default so a checkout never grows
# a multi-gigabyte `target/` and repeated runs reuse one compiled dependency graph.

# Resolve the toolchain's own binaries rather than the rustup proxies in
# `~/.cargo/bin`: under a sandbox HOME the proxies try to *install* a toolchain,
# which hangs without network, while the real `cargo`/`cargo-fmt` siblings work
# offline. PATH is checked first for ordinary developer setups.
CARGO ?= $(shell command -v cargo 2>/dev/null || \
	ls -1d /home/*/.rustup/toolchains/*/bin/cargo /root/.rustup/toolchains/*/bin/cargo \
		/usr/local/rustup/toolchains/*/bin/cargo 2>/dev/null | head -n 1)

# Cargo shells out to `rustc`, `rustfmt` and `cc`, so the toolchain directory has
# to be on PATH even when the caller's PATH only holds /usr/bin:/bin.
TOOLCHAIN_BIN := $(dir $(CARGO))
export PATH := $(TOOLCHAIN_BIN):$(PATH)

# Reuse an existing registry cache when the caller's HOME has none (containers and
# sandboxes usually keep it under the toolchain owner's home).
CARGO_HOME ?= $(shell test -d $(HOME)/.cargo/registry && echo $(HOME)/.cargo || \
	ls -1d /home/*/.cargo /usr/local/cargo /root/.cargo 2>/dev/null | head -n 1)

# Same for RUSTUP_HOME: the `cargo-fmt`/`cargo-clippy` shims are rustup proxies,
# so they need the toolchain root to resolve against, otherwise they try to
# download a channel when the caller's HOME has no usable `~/.rustup`. When the
# cargo we resolved lives in a rustup toolchain directory, that directory *is*
# the answer; a half-installed `$HOME/.rustup` must not win over it.
TOOLCHAIN_RUSTUP_HOME := $(if $(findstring /toolchains/,$(CARGO)),$(abspath $(TOOLCHAIN_BIN)/../../..))
RUSTUP_HOME ?= $(or $(TOOLCHAIN_RUSTUP_HOME),\
	$(firstword $(wildcard /usr/local/rustup /home/*/.rustup /root/.rustup)),\
	$(HOME)/.rustup)
export RUSTUP_HOME

CARGO_TARGET_DIR ?= /tmp/alphacode-target
export CARGO_HOME
export CARGO_TARGET_DIR

# `xa11y` (pulled in by the clipboard/TUI stack) links `-lxkbcommon`, which needs
# the *development* symlink `libxkbcommon.so`. Distro images often ship only the
# runtime `libxkbcommon.so.0`, and then the final link of the test binary fails
# with `unable to find library -lxkbcommon`. Provide the symlink in a scratch
# directory outside the checkout and point the linker at it.
LINKLIBS_DIR := $(CARGO_TARGET_DIR)/linklibs
XKB_RUNTIME := /usr/lib/x86_64-linux-gnu/libxkbcommon.so.0
XKB_DEV := /usr/lib/x86_64-linux-gnu/libxkbcommon.so

export RUSTFLAGS := -L $(LINKLIBS_DIR) $(RUSTFLAGS)

.PHONY: fmt-check check check-strict clippy-strict test test-fast test-one test-orcarouter test-orcarouter-live linklibs

linklibs:
	@mkdir -p $(LINKLIBS_DIR)
	@if [ ! -e "$(XKB_DEV)" ] && [ -e "$(XKB_RUNTIME)" ]; then \
		ln -sf "$(XKB_RUNTIME)" "$(LINKLIBS_DIR)/libxkbcommon.so"; \
	fi

# Formatting gate from AGENTS.md; needs rustfmt only, no build.
fmt-check:
	$(CARGO) fmt --all -- --check

check: linklibs
	$(CARGO) check --workspace --tests

# CI-equivalent strict gate, built into its own target directory.
#
# `RUSTFLAGS` participates in Cargo's fingerprint. CI builds with
# `RUSTFLAGS="-D warnings"`, so running that here against the normal target
# directory invalidates and recompiles the whole dependency graph plus the
# first-party crate, and then invalidates it *again* on the way back to the
# default flags. On this tree that is minutes in each direction, which is how
# the strict gate ends up deferred until CI rejects the branch.
#
# A separate CARGO_TARGET_DIR keeps strict and fast-dev artifacts warm
# independently. Costs disk, saves a full rebuild per strict check.
STRICT_TARGET_DIR ?= $(CARGO_TARGET_DIR)-strict

check-strict: linklibs
	RUSTFLAGS="-D warnings" CARGO_TARGET_DIR=$(STRICT_TARGET_DIR) \
		$(CARGO) check --workspace --tests

clippy-strict: linklibs
	RUSTFLAGS="-D warnings" CARGO_TARGET_DIR=$(STRICT_TARGET_DIR) \
		$(CARGO) clippy --locked --lib --all-targets -- -D warnings

# The OrcaRouter provider/auth/catalog regression suite.
test-orcarouter: linklibs
	$(CARGO) test --lib orcarouter

# Live checks through the real provider path; they need ORCAROUTER_API_KEY and
# are ignored by default, so they must be asked for explicitly.
test-orcarouter-live: linklibs
	$(CARGO) test --lib orcarouter_live -- --ignored --nocapture

test: linklibs
	$(CARGO) test --lib

# Focused iteration: `make test-one FILTER=reload_helper_access_tests`.
#
# The suite is one large test binary, so it must be built and linked before any
# test can run. A name filter reuses that binary, making every call after the
# first cheap; running the entire suite to check one function pays the link cost
# for nothing.
FILTER ?=
test-one: linklibs
	@test -n "$(FILTER)" || { echo "usage: make test-one FILTER=<name substring>"; exit 2; }
	$(CARGO) test --lib $(FILTER)

# Prefer nextest when installed: it gives each test its own process, which
# removes the cross-test interference (env vars, global mutexes, shared temp
# dirs) that makes individual failures unreproducible, and it reports the
# slowest tests so real hot spots surface instead of being guessed at.
test-fast: linklibs
	@if $(CARGO) nextest --version >/dev/null 2>&1; then \
		$(CARGO) nextest run --lib --no-fail-fast; \
	else \
		echo "nextest not installed; using the default harness"; \
		echo "install for isolated, faster runs: cargo install cargo-nextest --locked"; \
		$(CARGO) test --lib; \
	fi
