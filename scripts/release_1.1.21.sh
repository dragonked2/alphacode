#!/usr/bin/env bash
# Cut the 1.1.21 release: commit, tag, push, and let the tagged workflow build,
# verify and publish the Windows/Linux/macOS artifacts.
#
# Run from a green tree on `main`. The tag push is what triggers
# .github/workflows/release.yml; nothing is attached to a GitHub release until
# that run's verification steps (single-PE guard, AV-trigger guard, version
# identity check) pass.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[ "$VERSION" = "1.1.21" ] || { echo "error: Cargo.toml version is $VERSION, expected 1.1.21" >&2; exit 1; }

echo "==> committing 1.1.21"
git add -A
if ! git diff --cached --quiet; then
    git commit -m "Release 1.1.21: remove AV-trigger plaintext, fix paste drop and GUI transcript"
else
    echo "    nothing staged; assuming the release commit already exists"
fi

echo "==> tagging v1.1.21"
git tag -f v1.1.21

echo "==> pushing main and the tag"
git push origin main
git push -f origin v1.1.21

echo "==> tagged workflow will build and verify every target"
echo "    watch: gh run list --workflow=release.yml --limit 3"
