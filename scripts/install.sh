#!/usr/bin/env bash
# install.sh — one-line installer for Alphacode
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
#   curl -fsSL ... | bash -s -- --version v1.0.0
#   curl -fsSL ... | bash -s -- --prefix ~/.local
#
# Supported: Linux + macOS on x86_64 and aarch64.

set -euo pipefail

REPO="${ALPHACODE_REPO:-dragonked2/alphacode}"
VERSION="${ALPHACODE_VERSION:-latest}"
PREFIX="${ALPHACODE_PREFIX:-$HOME/.local}"
BIN_DIR="${ALPHACODE_BIN_DIR:-$PREFIX/bin}"
# If set to 1, never fall back to building from source (force release-only).
SOURCE_ONLY="${ALPHACODE_SOURCE_ONLY:-}"
# If set to 1, never try the release path — always build from source.
NEVER_RELEASE="${ALPHACODE_NEVER_RELEASE:-}"
# If set, the ref (branch / tag / sha) to check out when building from source.
SOURCE_REF="${ALPHACODE_SOURCE_REF:-}"

print() { printf "\033[1;36m==>\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33m[warn]\033[0m %s\n" "$*" >&2; }
fail()  { printf "\033[1;31m[fail]\033[0m %s\n" "$*" >&2; exit 1; }

usage() {
  cat <<'USAGE'
install.sh — install Alphacode.

By default, tries to download a prebuilt release asset for your platform
from the GitHub release page. If no release is published (or there is no
asset for this OS/arch), it falls back to building from source.

Flags:
  --version <v>     Release tag to install (default: latest)
  --prefix <dir>    Install prefix (default: ~/.local)
  --bin-dir <dir>   Override the binary directory (default: <prefix>/bin)
  --no-path         Do not print PATH instructions at the end
  --from-source     Skip the release download and always build from source
  --source-only     Never fall back to building from source (release-only)
  --source-ref <r>  When building from source, check out this ref (branch/tag/sha)
  -h, --help        Show this help

Environment:
  ALPHACODE_REPO=<owner>/<repo>    Default: dragonked2/alphacode
  ALPHACODE_VERSION=<v>            Default: latest
  ALPHACODE_PREFIX=<dir>           Default: ~/.local
  ALPHACODE_BIN_DIR=<dir>          Default: <prefix>/bin
  ALPHACODE_NEVER_RELEASE=1        Alias for --from-source
  ALPHACODE_SOURCE_ONLY=1          Alias for --source-only
  ALPHACODE_SOURCE_REF=<ref>       Alias for --source-ref

Exit codes:
  0   installed successfully
  1   user error
  2   download failed
  3   checksum mismatch
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --prefix)  PREFIX="$2";  shift 2 ;;
    --bin-dir) BIN_DIR="$2"; shift 2 ;;
    --no-path) NO_PATH=1;    shift ;;
    --source-only) SOURCE_ONLY=1; shift ;;
    --from-source)  NEVER_RELEASE=1; shift ;;
    --source-ref)   SOURCE_REF="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown flag: $1 (try --help)" ;;
  esac
done

# --- build_from_source -------------------------------------------------------
#
# Fallback: no release artifact for this platform/arch. Clone the repo, build
# with cargo, and copy the resulting binary into $BIN_DIR.
#
# Requires: git, cargo, rustc >= 1.91, and a working C toolchain. This can
# take 5-30 minutes on a first build.
build_from_source() {
  command -v git   >/dev/null 2>&1 || fail "git is required to build from source"
  command -v cargo >/dev/null 2>&1 || fail "cargo is required to build from source (install Rust from https://rustup.rs)"

  # Make sure the toolchain is new enough for edition = "2024" and the
  # current dependency MSRV (mdwright-latex 0.1.3 requires rustc 1.91).
  local rust_ver
  rust_ver="$(rustc --version 2>/dev/null | awk '{print $2}')" || true
  if [ -n "$rust_ver" ]; then
    # Crude semver check: split major.minor.
    local major minor
    major="${rust_ver%%.*}"
    minor="$(echo "$rust_ver" | awk -F. '{print $2}')"
    if [ "${major:-0}" -lt 1 ] || { [ "${major:-0}" -eq 1 ] && [ "${minor:-0}" -lt 91 ]; }; then
      fail "rustc $rust_ver is too old; need >= 1.91 (update via 'rustup update')"
    fi
  fi

  local src_dir
  src_dir="$(mktemp -d)"
  # Chain the cleanup so we remove the build dir AND any earlier TMP.
  local _prev_tmp="${TMP:-}"
  trap 'rm -rf "$src_dir" ${_prev_tmp:+"$_prev_tmp"}' EXIT

  print "Cloning $REPO into a temporary build directory …"
  if [ -n "$SOURCE_REF" ]; then
    git clone --depth 1 --branch "$SOURCE_REF" "https://github.com/$REPO.git" "$src_dir/src" \
      || fail "git clone failed (ref: $SOURCE_REF)"
  else
    git clone --depth 1 "https://github.com/$REPO.git" "$src_dir/src" \
      || fail "git clone failed"
  fi

  print "Compiling alphacode (this can take 5-30 minutes on a first build) …"
  ( cd "$src_dir/src" && cargo build --release --locked ) \
    || fail "cargo build failed"

  local built
  built="$(find "$src_dir/src/target/release" -maxdepth 1 -type f -name 'alphacode' -print -quit)"
  [ -n "$built" ] || fail "build succeeded but target/release/alphacode was not produced"

  mkdir -p "$BIN_DIR"
  install -m 0755 "$built" "$BIN_DIR/alphacode"
  print "Installed → $BIN_DIR/alphacode (built from source)"
}

# --- Sanity ------------------------------------------------------------------

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar   >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s)" in
      Linux*) PLATFORM=linux ;;
      Darwin*) PLATFORM=macos ;;
      *) fail "unsupported OS: $(uname -s). On Windows run scripts/install.ps1 instead." ;;
    esac

case "$(uname -m)" in
      x86_64|amd64)  ARCH=x86_64 ;;
      aarch64|arm64) ARCH=arm64 ;;
      *) fail "unsupported architecture: $(uname -m)" ;;
    esac

# --- Pick a version ----------------------------------------------------------

# Short-circuit: build from source only.
if [ -n "$NEVER_RELEASE" ]; then
  print "--from-source requested, skipping release download."
  build_from_source
  exit 0
fi

if [ "$VERSION" = "latest" ]; then
  print "Resolving latest release from $REPO …"
  VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' \
    | sed -E 's/.*"tag_name":[[:space:]]*"([^"]+)".*/\1/')" \
    || VERSION=""
  if [ -z "$VERSION" ]; then
    if [ -n "$SOURCE_ONLY" ]; then
      fail "no release found for $REPO and --source-only is set"
    fi
    warn "no GitHub release found for $REPO — falling back to building from source."
    build_from_source
    exit 0
  fi
  print "Latest release: $VERSION"
fi

# Some releases strip the leading 'v' in their published archives.
VERSION_NO_V="${VERSION#v}"

ASSET="alphacode-${PLATFORM}-${ARCH}.tar.gz"
URL="https://github.com/$REPO/releases/download/${VERSION}/$ASSET"

# --- Download ----------------------------------------------------------------

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
print "Downloading $URL"
if ! curl -fL --retry 3 --connect-timeout 15 -o "$TMP/$ASSET" "$URL"; then
  if [ -n "$SOURCE_ONLY" ]; then
    fail "download failed (asset may not exist for $PLATFORM/$ARCH — try --version)"
  fi
  warn "no prebuilt asset for $PLATFORM/$ARCH at $VERSION — falling back to building from source."
  build_from_source
  exit 0
fi

# Optional checksum verification.
if curl -fsSL -o "$TMP/SHA256SUMS" \
     "https://github.com/$REPO/releases/download/${VERSION}/SHA256SUMS" 2>/dev/null; then
  print "Verifying checksum …"
  if command -v sha256sum >/dev/null 2>&1; then
    ( cd "$TMP" && sha256sum -c --ignore-missing < SHA256SUMS ) \
      || fail "checksum verification failed"
  else
    warn "sha256sum not available — skipping checksum verification"
  fi
fi

# --- Install -----------------------------------------------------------------

print "Extracting …"
tar -xzf "$TMP/$ASSET" -C "$TMP"

mkdir -p "$BIN_DIR"
FOUND="$(find "$TMP" -maxdepth 3 -type f -name 'alphacode' -print -quit)"
[ -n "$FOUND" ] || fail "extracted archive did not contain an 'alphacode' binary"

chmod +x "$FOUND"
mv "$FOUND" "$BIN_DIR/alphacode"
print "Installed → $BIN_DIR/alphacode"

# --- Done --------------------------------------------------------------------

INSTALLED_VERSION="$("$BIN_DIR/alphacode" --version 2>/dev/null || echo unknown)"
print "Installed version: $INSTALLED_VERSION"

if [ -z "${NO_PATH:-}" ] && ! command -v alphacode >/dev/null 2>&1; then
  echo
  printf "\033[1;33mNext step:\033[0m add '%s' to your PATH.\n" "$BIN_DIR"
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
      cat <<PATH

  # Bash / Zsh — append to your ~/.bashrc or ~/.zshrc:
  export PATH="$BIN_DIR:\$PATH"

  # Fish:
  fish_add_path "$BIN_DIR"
PATH
      ;;
  esac
fi

print "Run \`alphacode login\` to connect a model, then \`alphacode\` to start."