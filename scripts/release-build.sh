#!/usr/bin/env bash
# Build and verify a Windows release archive locally, matching what
# .github/workflows/release.yml produces.
#
# Exists because "does the release work?" required hand-assembling the same
# things every time: the release profile, the GUI-host sibling, the archive
# layout and the single-PE guard. Doing it by hand is how the nested-PE layout
# reached users in 1.0.92.
#
# Usage:
#   scripts/release-build.sh              # build + package + verify
#   scripts/release-build.sh --skip-build # re-package an existing build
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

SKIP_BUILD=0
[ "${1:-}" = "--skip-build" ] && SKIP_BUILD=1

: "${CARGO_TARGET_DIR:=${TMPDIR:-/tmp}/alphacode-target}"
export CARGO_TARGET_DIR

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[ -n "$VERSION" ] || { echo "error: could not read version from Cargo.toml" >&2; exit 1; }

echo "==> AlphaCode $VERSION"

if [ "$SKIP_BUILD" -eq 0 ]; then
    echo "==> cargo build --release --locked"
    script="$(mktemp "${TMPDIR:-/tmp}/ac-release-XXXXXX.cmd")"
    {
        echo "@echo off"
        echo "call \"C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Auxiliary\\Build\\vcvars64.bat\" >nul 2>&1"
        echo "if errorlevel 1 for %%f in (\"C:\\Program Files\\Microsoft Visual Studio\\2022\\*\\VC\\Auxiliary\\Build\\vcvars64.bat\") do call \"%%f\" >nul 2>&1"
        echo "set CARGO_TARGET_DIR=$(cygpath -w "$CARGO_TARGET_DIR")"
        echo "set ALPHACODE_RELEASE_BUILD=1"
        echo "set ALPHACODE_BUILD_GIT_DIRTY=0"
        echo "cd /d \"$(cygpath -w "$REPO")\""
        echo "cargo build --release --locked --bin alphacode"
    } > "$script"
    MSYS_NO_PATHCONV=1 cmd.exe /C "$(cygpath -w "$script")"
    status=$?
    rm -f "$script"
    [ $status -eq 0 ] || { echo "error: release build failed ($status)" >&2; exit $status; }
fi

PROFILE_DIR="$CARGO_TARGET_DIR/release"
BIN="$PROFILE_DIR/alphacode.exe"
HOST="$PROFILE_DIR/AlphaCode.Desktop.exe"

echo "==> verifying build outputs"
for path in "$BIN" "$HOST"; do
    [ -f "$path" ] || { echo "error: missing $path" >&2; exit 1; }
    echo "    $path ($(stat -c%s "$path") bytes)"
done

echo "==> single-PE guard"
python3 scripts/verify-pe-single-image.py "$BIN" || exit 1
python3 scripts/verify-pe-single-image.py "$HOST" || exit 1

echo "==> package"
ARCHIVE="alphacode-windows-x86_64"
mkdir -p dist
rm -f "dist/${ARCHIVE}.zip" "dist/${ARCHIVE}.sha256"
# -j: both files at the archive root, so the GUI host is a sibling of the binary
# and `resolve_desktop_host()` finds it without any installation step.
if command -v zip >/dev/null 2>&1; then
    ( cd "$PROFILE_DIR" && zip -j -9 "$REPO/dist/${ARCHIVE}.zip" alphacode.exe AlphaCode.Desktop.exe )
else
    python3 - "$PROFILE_DIR" "$REPO/dist/${ARCHIVE}.zip" <<'PY'
import sys, zipfile, os
profile, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
    for name in ('alphacode.exe', 'AlphaCode.Desktop.exe'):
        zf.write(os.path.join(profile, name), arcname=name)
PY
fi

echo "==> archive contents"
python3 - "dist/${ARCHIVE}.zip" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as zf:
    for info in zf.infolist():
        print(f"    {info.filename}  ({info.file_size} bytes)")
    names = {i.filename for i in zf.infolist()}
    missing = {'alphacode.exe', 'AlphaCode.Desktop.exe'} - names
    if missing:
        raise SystemExit(f"error: archive is missing {sorted(missing)}")
PY

# Emit the checksum with a portable helper. `sha256sum ... > file` under Git
# Bash on Windows intermittently fails with "failed to set file descriptor
# text/binary mode: Bad file descriptor" and leaves a zero-byte .sha256 next to
# a valid archive — a silent release defect. Writing the digest from Python
# avoids the descriptor issue entirely, and the result is verified by reading it
# back and re-hashing.
python3 - "dist/${ARCHIVE}.zip" "dist/${ARCHIVE}.sha256" <<'PY'
import hashlib, sys
archive, out = sys.argv[1], sys.argv[2]
digest = hashlib.sha256(open(archive, 'rb').read()).hexdigest()
with open(out, 'w', encoding='ascii', newline='\n') as fh:
    fh.write(f"{digest}  {archive.split('/')[-1]}\n")
PY
recorded="$(cut -d' ' -f1 "dist/${ARCHIVE}.sha256")"
actual="$(python3 -c "import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "dist/${ARCHIVE}.zip")"
if [ "$recorded" != "$actual" ]; then
    echo "error: checksum file does not match the archive" >&2
    exit 1
fi
echo "==> sha256 $actual"
echo "==> done: dist/${ARCHIVE}.zip"
