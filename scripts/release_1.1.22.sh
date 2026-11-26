#!/usr/bin/env bash
# Cut the 1.1.22 release: commit, tag, push, and let the tagged workflow build,
# verify and publish the Windows/Linux/macOS artifacts.
#
# Run from a green tree on the release branch. The tag push is what triggers
# .github/workflows/release.yml; nothing is attached to a GitHub release until
# that run's verification steps pass:
#   * verify-pe-single-image.py  -- no nested PE in alphacode.exe
#   * verify-pe-metadata.py      -- zero overlay + complete VERSIONINFO + no
#                                   leaked build paths, on BOTH binaries
#   * verify-no-av-triggers.py   -- no AV-trigger strings in EITHER binary
#   * version identity check     -- tag == Cargo.toml == Cargo.lock
#
# Set SKIP_PREFLIGHT=1 to bypass the local byte-level gate (CI still runs it).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[ "$VERSION" = "1.1.22" ] || { echo "error: Cargo.toml version is $VERSION, expected 1.1.22" >&2; exit 1; }

# Local gate before anything is pushed. A tag that produces a failing release run
# is worse than no tag: the workflow builds on all platforms and only fails
# later. These checks are also re-run from scratch by the workflow, so this is a
# fast fail, not a substitute for CI.
if [ "${SKIP_PREFLIGHT:-0}" != "1" ]; then
    echo "==> local guard preflight"
    for script in verify-no-av-triggers.py verify-pe-metadata.py verify-pe-single-image.py; do
        [ -f "scripts/$script" ] || { echo "error: scripts/$script is missing" >&2; exit 1; }
    done
    # A stale dist/ must not be what gets published. When a local build exists,
    # gate on the actual bytes rather than just trusting the scripts are present.
    if [ -f dist/alphacode-windows-x86_64.zip ] && [ -d target/release ]; then
        for binary in alphacode.exe AlphaCode.Desktop.exe; do
            path="target/release/$binary"
            [ -f "$path" ] || { echo "error: $path is missing" >&2; exit 1; }
            python3 scripts/verify-pe-metadata.py "$path" >/dev/null \
                || { echo "error: $binary fails the metadata guard" >&2; exit 1; }
            python3 scripts/verify-no-av-triggers.py "$path" >/dev/null \
                || { echo "error: $binary carries AV trigger strings" >&2; exit 1; }
        done
        echo "    dist artifacts pass the structural guards"
    else
        echo "    no dist artifact to check; skipping byte-level preflight"
    fi
fi

echo "==> committing 1.1.22"
git add -A
if ! git diff --cached --quiet; then
    git commit -m "Release 1.1.22: remove PE overlay and restore GUI host metadata

The 1.1.21 AlphaCode.Desktop.exe carried a 684-byte PE overlay: build.rs
appended a compiled VERSIONINFO .res after the last section. The appended
block was never registered in the resource directory, so Windows read
ProductName/CompanyName/LegalCopyright as empty while VirusTotal tagged the
file 'overlay'. Declaring the fields as csc assembly attributes links
VERSIONINFO into .rsrc, removing the overlay and populating every field.

Guards: verify-pe-metadata.py (new) plus AV-trigger scanning now cover the
GUI host, not just the CLI."
else
    echo "    nothing staged; assuming the release commit already exists"
fi

echo "==> tagging v1.1.22"
git tag -f v1.1.22

# Push the *branch the release was cut from*, not a hard-coded `main`. The work
# for this release lives on a release branch; pushing `main` would tag v1.1.22
# against whatever main currently points at, which is not this fix.
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" = "HEAD" ]; then
    echo "error: detached HEAD; check out the release branch" >&2
    exit 1
fi
echo "==> pushing $BRANCH and the tag"
git push -u origin "$BRANCH"
git push -f origin v1.1.22

echo "==> tagged workflow will build and verify every target"
echo "    watch: gh run list --workflow=release.yml --limit 3"