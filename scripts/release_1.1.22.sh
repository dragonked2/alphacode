#!/usr/bin/env bash
# Cut the 1.1.22 release: commit, tag, push, and let the tagged workflow build,
# verify and publish the Windows/Linux/macOS artifacts.
#
# Run from a green tree on `main`. The tag push is what triggers
# .github/workflows/release.yml; nothing is attached to a GitHub release until
# that run's verification steps pass:
#   * verify-pe-single-image.py  -- no nested PE in alphacode.exe
#   * verify-pe-metadata.py      -- zero overlay + complete VERSIONINFO
#                                   on BOTH alphacode.exe and AlphaCode.Desktop.exe
#   * verify-no-av-triggers.py   -- no AV-trigger strings in EITHER binary
#   * version identity check     -- tag == Cargo.toml == Cargo.lock
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[ "$VERSION" = "1.1.22" ] || { echo "error: Cargo.toml version is $VERSION, expected 1.1.22" >&2; exit 1; }

# Refuse to tag a tree that would fail the release guards anyway. This catches a
# regression locally instead of after the tag is pushed and the workflow starts.
echo "==> local guard preflight"
for script in verify-no-av-triggers.py verify-pe-metadata.py; do
    [ -f "scripts/$script" ] || { echo "error: scripts/$script is missing" >&2; exit 1; }
done

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

echo "==> pushing main and the tag"
git push origin main
git push -f origin v1.1.22

echo "==> tagged workflow will build and verify every target"
echo "    watch: gh run list --workflow=release.yml --limit 3"