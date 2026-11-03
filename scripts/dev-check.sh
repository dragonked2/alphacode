#!/usr/bin/env bash
# One rerunnable verification entry point for developers and agents.
#
# Why this exists
# ---------------
# Verifying a change used to require rediscovering several environment details
# on every run, and getting any of them wrong produced a confusing failure
# rather than a clear one:
#
#   * Git Bash rewrites a leading `/C` into a Windows path, so
#     `cmd.exe /C "..."` silently ran `cmd.exe` with no command and exited 0.
#     The build then "passed" without compiling anything. Fix:
#     `MSYS_NO_PATHCONV=1` on every `cmd.exe` invocation here.
#   * The MSVC toolchain needs `vcvars64.bat` run first. A bare `cargo` from Git
#     Bash can pick up the wrong linker and fail inside `cc`.
#   * A shared `target/` locks against a concurrent cargo. A dedicated
#     `CARGO_TARGET_DIR` keeps runs independent and reuses the cache.
#
# Usage:
#   scripts/dev-check.sh            # fmt + clippy + tests
#   scripts/dev-check.sh check      # cargo check --workspace --all-targets
#   scripts/dev-check.sh test bash  # cargo test --lib -- <filter>
#   scripts/dev-check.sh fmt
#   scripts/dev-check.sh clippy
#   scripts/dev-check.sh pe <file>  # single-PE structural check (release guard)
#
# Every run appends to `.alphacode/scratch/dev-check.log` and prints the tail.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"
LOG="$REPO/.alphacode/scratch/dev-check.log"
mkdir -p "$(dirname "$LOG")"
exec > >(tee "$LOG") 2>&1

# Dedicated target dir so we never block on another cargo's package-cache lock.
: "${CARGO_TARGET_DIR:=${TMPDIR:-/tmp}/alphacode-target}"
export CARGO_TARGET_DIR

# Does this shell need a Windows command shim? Git Bash on Windows does.
needs_windows_shim() {
    case "$(uname -s)" in
        MINGW* | MSYS* | CYGWIN*) return 0 ;;
        *) return 1 ;;
    esac
}

# Run cargo with the MSVC environment applied on Windows.
run_cargo() {
    if needs_windows_shim; then
        local vcvars="/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvars64.bat"
        local vcvars_alt="/c/Program Files/Microsoft Visual Studio/2022/*/VC/Auxiliary/Build/vcvars64.bat"
        local script
        script="$(mktemp "${TMPDIR:-/tmp}/ac-run-XXXXXX.cmd")"
        {
            echo "@echo off"
            if [ -f "$vcvars" ]; then
                echo "call \"$(cygpath -w "$vcvars")\" >nul 2>&1"
            else
                # Try the Community/Enterprise layout before giving up.
                echo "for %%f in (\"C:\\Program Files\\Microsoft Visual Studio\\2022\\*\\VC\\Auxiliary\\Build\\vcvars64.bat\") do call \"%%f\" >nul 2>&1"
            fi
            echo "set CARGO_TARGET_DIR=$(cygpath -w "$CARGO_TARGET_DIR")"
            echo "cd /d \"$(cygpath -w "$REPO")\""
            echo "cargo $*"
        } > "$script"
        # MSYS_NO_PATHCONV=1 is load-bearing: without it Git Bash rewrites `/C`
        # and the shim runs with no arguments, exiting 0 without compiling.
        MSYS_NO_PATHCONV=1 cmd.exe /C "$(cygpath -w "$script")"
        local status=$?
        rm -f "$script"
        return $status
    fi
    # shellcheck disable=SC2086
    cargo $*
}

cmd="${1:-all}"
shift || true

case "$cmd" in
    fmt)
        run_cargo fmt --all -- --check
        ;;
    fmt-fix)
        run_cargo fmt --all
        ;;
    clippy)
        run_cargo clippy --locked --lib --all-targets -- -D warnings
        ;;
    check)
        run_cargo check --workspace --all-targets --locked
        ;;
    test)
        run_cargo test --lib --locked -- "$@"
        ;;
    pe)
        python3 scripts/verify-pe-single-image.py "$@"
        ;;
    all)
        status=0
        for gate in fmt clippy check test; do
            echo "===== $gate ====="
            if [ "$gate" = test ]; then run_cargo test --lib --locked; else
                case "$gate" in
                    fmt) run_cargo fmt --all -- --check ;;
                    clippy) run_cargo clippy --locked --lib --all-targets -- -D warnings ;;
                    check) run_cargo check --workspace --all-targets --locked ;;
                esac
            fi
            code=$?
            echo "----- $gate exit=$code -----"
            [ $code -ne 0 ] && status=$code
        done
        exit $status
        ;;
    *)
        echo "unknown command: $cmd" >&2
        sed -n '18,30p' "$0" >&2
        exit 2
        ;;
esac
status=$?
echo "----- exit=$status -----"
exit $status