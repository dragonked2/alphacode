#!/usr/bin/env bash
# install.sh -- one-line installer for AlphaCode (Linux and macOS)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.sh | bash
#   curl -fsSL ... | bash -s -- --version v1.0.0
#   curl -fsSL ... | bash -s -- --prefix ~/.local
#   curl -fsSL ... | sudo bash                  # system-wide, into /usr/local/bin
#
# Supported: Linux and macOS on x86_64 and arm64/aarch64 (glibc and musl).
#
# What it does:
#   1. Downloads the prebuilt release for your OS/CPU and verifies its SHA-256
#      checksum (sha256sum, shasum or openssl -- macOS has no sha256sum).
#      With no release or asset it falls back to building from source.
#   2. Installs the binary atomically, so updating a running copy is safe.
#   3. Puts it on PATH so nobody edits dotfiles by hand:
#        * writes one marked, idempotent block into the startup files your
#          shells actually read (bash, zsh, fish, nushell, csh/tcsh, ksh/sh),
#          including macOS's login-shell files and ~/.profile for GUI sessions;
#        * starts a clean shell and checks that a NEW terminal finds this exact
#          binary, and warns if a different `alphacode` would win;
#        * falls back to a symlink in /usr/local/bin when no startup file can
#          be edited (needs write access, or re-run with sudo).
#
# NOTE ON "THIS TERMINAL": `curl | bash` runs in a child process, and no child
# can change its parent shell's PATH. Open a new terminal, or run the one-line
# command the installer prints at the end, to use it immediately.
#
# The whole script lives in main(), invoked on the LAST line. If the download
# is cut short, bash sees an incomplete function and runs nothing, instead of
# executing half an installer.

set -euo pipefail

REPO="${ALPHACODE_REPO:-dragonked2/alphacode}"
VERSION="${ALPHACODE_VERSION:-latest}"
PREFIX="${ALPHACODE_PREFIX:-}"
BIN_DIR="${ALPHACODE_BIN_DIR:-}"
# If set to 1, never fall back to building from source (force release-only).
SOURCE_ONLY="${ALPHACODE_SOURCE_ONLY:-}"
# If set to 1, never try the release path -- always build from source.
NEVER_RELEASE="${ALPHACODE_NEVER_RELEASE:-}"
# Branch / tag / sha to check out when building from source.
SOURCE_REF="${ALPHACODE_SOURCE_REF:-}"
NO_PATH="${ALPHACODE_NO_PATH:-}"
LINK_BIN=""
PATH_DRY_RUN=""

BLOCK_BEGIN='# >>> alphacode >>>'
BLOCK_END='# <<< alphacode <<<'
LEGACY_MARKER='# added by alphacode install.sh'   # written by older versions

WORK=""
PLATFORM=""
ARCH=""
SHELL_PATH=""
TARGETS=""
PATH_CHANGED=""
PATH_STATE=""
LINK_PATH=""
INSTALLED_VERSION=""
START_FAILED=""
BUILT_FROM_SOURCE=""
BASES=()
CANDIDATES=()
ASSET=""
BASE_USED=""
STAGED_BIN=""
STAGED_PAYLOAD=""

# -- output ---------------------------------------------------------------------

C_CYAN=""; C_YELLOW=""; C_RED=""; C_GREEN=""; C_DIM=""; C_OFF=""
setup_colors() {
  if [ -t 1 ] && [ -t 2 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != "dumb" ]; then
    C_CYAN=$'\033[1;36m'; C_YELLOW=$'\033[1;33m'; C_RED=$'\033[1;31m'
    C_GREEN=$'\033[1;32m'; C_DIM=$'\033[0;90m'; C_OFF=$'\033[0m'
  fi
  return 0
}

info() { printf '%s==>%s %s\n' "$C_CYAN" "$C_OFF" "$*"; }
ok()   { printf '%s[ok]%s %s\n' "$C_GREEN" "$C_OFF" "$*"; }
warn() { printf '%s[warn]%s %s\n' "$C_YELLOW" "$C_OFF" "$*" >&2; }
die()  { local code="$1"; shift; printf '%s[fail]%s %s\n' "$C_RED" "$C_OFF" "$*" >&2; exit "$code"; }
fail() { die 1 "$@"; }

usage() {
  cat <<'USAGE'
install.sh -- install AlphaCode (Linux and macOS).

By default, downloads the prebuilt release for your platform, verifies its
checksum, installs it, and puts it on PATH. If no release or asset exists, it
falls back to building from source.

Flags:
  --version <v>     Release tag to install (default: latest)
  --prefix <dir>    Install prefix (default: ~/.local, or /usr/local as root)
  --bin-dir <dir>   Override the binary directory (default: <prefix>/bin)
  --add-path        (the default; kept for compatibility) Add the bin dir to
                    your shell startup files so `alphacode` is on PATH in new
                    terminals. Detects bash/zsh/fish/nushell/csh/ksh and is
                    idempotent across re-runs.
  --no-path         Do not touch PATH at all: no startup-file edits
  --path-dry-run    Show which files would change; write nothing
  --link            Also symlink the binary into a system bin dir (default
                    /usr/local/bin, or /opt/homebrew/bin) so no PATH change
                    is needed at all. Needs write access; else re-run with sudo.
  --from-source     Skip the release download and always build from source
  --source-only     Never fall back to building from source (release-only)
  --source-ref <r>  When building from source, check out this ref
  -h, --help        Show this help

Flags that take a value also accept --flag=value.

Environment:
  ALPHACODE_REPO=<owner>/<repo>    Default: dragonked2/alphacode
  ALPHACODE_VERSION=<v>            Default: latest
  ALPHACODE_PREFIX=<dir>           Default: ~/.local (/usr/local as root)
  ALPHACODE_BIN_DIR=<dir>          Default: <prefix>/bin
  ALPHACODE_LINK_DIR=<dir>         Default: /usr/local/bin (used by --link)
  ALPHACODE_ARCH=x86_64|arm64      Force the architecture
  ALPHACODE_NO_PATH=1              Alias for --no-path
  ALPHACODE_NEVER_RELEASE=1        Alias for --from-source
  ALPHACODE_SOURCE_ONLY=1          Alias for --source-only
  ALPHACODE_SOURCE_REF=<ref>       Alias for --source-ref
  ALPHACODE_SKIP_VERIFY=1          Skip the fresh-shell PATH check
  NO_COLOR=1                       Disable colored output

Exit codes:
  0   installed successfully
  1   user or environment error
  2   download failed
  3   checksum mismatch
  4   installed, but the binary cannot start on this machine
USAGE
}

# -- small helpers --------------------------------------------------------------

need_arg() {
  if [ $# -lt 2 ] || [ -z "${2:-}" ]; then
    fail "$1 needs a value (try --help)"
  fi
}

# Expand a leading ~ (quoted tildes are not expanded by the shell) and make the
# path absolute, without a trailing slash.
expand_path() {
  local p="$1"
  case "$p" in
    "~")   p="$HOME" ;;
    "~/"*) p="$HOME/${p#"~/"}" ;;
  esac
  case "$p" in
    /*) ;;
    *)  p="$PWD/$p" ;;
  esac
  while [ "$p" != "/" ] && [ "${p%/}" != "$p" ]; do p="${p%/}"; done
  printf '%s\n' "$p"
}

# Show $HOME as ~ in messages.
tilde() {
  case "$1" in
    "$HOME"/*) printf '~/%s' "${1#"$HOME"/}" ;;
    *)         printf '%s' "$1" ;;
  esac
}

cleanup() {
  local rc=$?
  if [ -n "${WORK:-}" ] && [ -d "$WORK" ]; then
    case "${WORK##*/}" in
      alphacode-install.*) rm -rf -- "$WORK" ;;
    esac
  fi
  return $rc
}

# Portable timeout: GNU `timeout`, Homebrew `gtimeout`, else a watchdog. macOS
# ships none of them. The watchdog must not inherit stdout, or a surrounding
# $(...) would wait for its `sleep`.
run_with_timeout() {
  local secs="$1"; shift
  if command -v timeout >/dev/null 2>&1; then timeout "$secs" "$@"; return $?; fi
  if command -v gtimeout >/dev/null 2>&1; then gtimeout "$secs" "$@"; return $?; fi
  local pid watcher rc=0
  "$@" &
  pid=$!
  ( sleep "$secs"; kill -9 "$pid" 2>/dev/null ) >/dev/null 2>&1 &
  watcher=$!
  wait "$pid" 2>/dev/null || rc=$?
  kill "$watcher" 2>/dev/null || true
  wait "$watcher" 2>/dev/null || true
  return $rc
}

# SHA-256 of a file using whichever tool exists. macOS ships shasum, not
# sha256sum, and the old script silently skipped verification there.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{ print $1 }'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | awk '{ print $NF }'
  else
    return 1
  fi
}

lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

curl_download() {  # url out
  local progress="-sS"
  [ -t 2 ] && progress="--progress-bar"
  curl -fL "$progress" --retry 3 --retry-delay 2 --connect-timeout 15 \
       --proto '=https' --tlsv1.2 -o "$2" "$1"
}

# -- platform -------------------------------------------------------------------

detect_platform() {
  case "$(uname -s)" in
    Linux*)  PLATFORM=linux ;;
    Darwin*) PLATFORM=macos ;;
    MINGW*|MSYS*|CYGWIN*) fail "this is a Windows shell. Run scripts/install.ps1 in PowerShell instead." ;;
    *) fail "unsupported OS: $(uname -s). On Windows run scripts/install.ps1 instead." ;;
  esac

  case "$(uname -m)" in
    x86_64|amd64)  ARCH=x86_64 ;;
    aarch64|arm64) ARCH=arm64 ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
  esac

  # A shell running under Rosetta 2 reports x86_64 on an Apple Silicon Mac.
  # Prefer the native arm64 build there.
  if [ "$PLATFORM" = macos ] && [ "$ARCH" = x86_64 ]; then
    if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
      ARCH=arm64
      info "Rosetta detected; installing the native arm64 build."
    fi
  fi

  if [ -n "${ALPHACODE_ARCH:-}" ]; then
    case "$(lower "$ALPHACODE_ARCH")" in
      x86_64|amd64)  ARCH=x86_64 ;;
      arm64|aarch64) ARCH=arm64 ;;
      *) warn "ignoring ALPHACODE_ARCH='$ALPHACODE_ARCH' (expected x86_64 or arm64)" ;;
    esac
  fi
}

# A glibc binary cannot start on a musl host (Alpine, postmarketOS): it dies with
# "Error loading shared library ld-linux-*.so". `ldd --version` mentions musl on
# musl hosts. Hosts without ldd fall back to the musl loader file. (A glibc host
# can have musl installed alongside, so the file test is only a fallback.)
is_musl() {
  [ "$PLATFORM" = linux ] || return 1
  if command -v ldd >/dev/null 2>&1; then
    ldd --version 2>&1 | grep -qi musl
    return $?
  fi
  local f
  for f in /lib/ld-musl-*.so.1; do
    [ -e "$f" ] && return 0
  done
  return 1
}

resolve_locations() {
  [ -n "${HOME:-}" ] || fail "HOME is not set"

  if [ -z "$PREFIX" ] && [ -z "$BIN_DIR" ]; then
    # Root installs system-wide: /usr/local/bin is already on everyone's PATH,
    # so `curl ... | sudo bash` needs no PATH configuration at all.
    if [ "$(id -u)" = "0" ]; then PREFIX="/usr/local"; else PREFIX="$HOME/.local"; fi
  fi
  [ -n "$PREFIX" ] || PREFIX="$HOME/.local"
  PREFIX="$(expand_path "$PREFIX")"
  # Computed AFTER flag parsing; the old script derived BIN_DIR from the default
  # prefix first, so `--prefix /opt/x` silently installed to ~/.local/bin.
  if [ -n "$BIN_DIR" ]; then BIN_DIR="$(expand_path "$BIN_DIR")"; else BIN_DIR="$PREFIX/bin"; fi
}

# -- getting the binary ---------------------------------------------------------

# Candidate assets in preference order. On musl, the static musl build is tried
# first, then the glibc one, so releases that predate the musl workflow still
# install instead of forcing a long source build.
build_candidates() {
  local musl_arch=""
  case "$ARCH" in
    arm64)  musl_arch=aarch64 ;;
    x86_64) musl_arch=x86_64 ;;
  esac
  if is_musl && [ -n "$musl_arch" ]; then
    CANDIDATES=("alphacode-linux-musl-${musl_arch}.tar.gz" "alphacode-linux-${ARCH}.tar.gz")
    info "musl libc detected; preferring the static musl build."
  else
    CANDIDATES=("alphacode-${PLATFORM}-${ARCH}.tar.gz")
  fi
}

# Work out where to download from. The API is only used to learn the tag; it is
# rate limited to 60 requests/hour per IP, and the old script treated a limit
# error as "no release" and started a 5-30 minute source build. GitHub's
# /releases/latest/download redirect needs no API call at all.
resolve_release_bases() {
  local root="https://github.com/$REPO/releases"
  BASES=()
  if [ "$VERSION" = "latest" ]; then
    info "Resolving latest release from $REPO ..."
    local api="$WORK/api.json" tag=""
    # Download to a file first: `curl | grep -m1` made curl die of SIGPIPE.
    if curl -fsSL --retry 2 --connect-timeout 15 -H 'User-Agent: alphacode-installer' \
         -o "$api" "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null; then
      tag="$(grep -m1 '"tag_name"' "$api" \
        | sed -E 's/.*"tag_name":[[:space:]]*"([^"]+)".*/\1/' || true)"
    fi
    case "$tag" in *[!A-Za-z0-9._+-]*) tag="" ;; esac
    if [ -n "$tag" ]; then
      VERSION="$tag"
      BASES=("$root/download/$tag")
      info "Latest release: $tag"
    else
      BASES=("$root/latest/download")
    fi
  else
    BASES=("$root/download/$VERSION")
    # Accept `1.2.3` as well as `v1.2.3`.
    case "$VERSION" in [0-9]*) BASES+=("$root/download/v$VERSION") ;; esac
  fi
}

download_release() {
  local base cand
  for base in "${BASES[@]}"; do
    for cand in "${CANDIDATES[@]}"; do
      info "Downloading $base/$cand"
      if curl_download "$base/$cand" "$WORK/$cand"; then
        ASSET="$cand"; BASE_USED="$base"
        return 0
      fi
      rm -f "$WORK/$cand"
    done
  done
  return 1
}

# Missing digest and wrong digest are deliberately different outcomes: the first
# warns, the second fails (exit 3). Collapsing them would let a corrupted or
# tampered download pass as "no checksum published". The musl archives come from
# a separate workflow and may be absent from SHA256SUMS, so the digest is looked
# up for THIS asset by exact filename, never via `--ignore-missing`.
verify_checksum() {
  info "Verifying checksum ..."
  local actual expected=""
  if ! actual="$(sha256_of "$WORK/$ASSET")"; then
    warn "no SHA-256 tool found (sha256sum, shasum, openssl); skipping verification"
    return 0
  fi

  if curl -fsSL --retry 2 --connect-timeout 15 -o "$WORK/SHA256SUMS" \
       "$BASE_USED/SHA256SUMS" 2>/dev/null; then
    # Exact match on the filename field, tolerating the binary-mode "*" prefix.
    expected="$(awk -v want="$ASSET" \
      '{ n = $2; sub(/^\*/, "", n); if (n == want) { print $1; exit } }' "$WORK/SHA256SUMS" || true)"
  fi
  if [ -z "$expected" ] && curl -fsSL --retry 2 --connect-timeout 15 -o "$WORK/$ASSET.sha256" \
       "$BASE_USED/$ASSET.sha256" 2>/dev/null; then
    expected="$(awk '{ print $1; exit }' "$WORK/$ASSET.sha256" || true)"
  fi

  if [ -z "$expected" ]; then
    warn "no published checksum for $ASSET; this download is unverified"
    return 0
  fi
  if [ "$(lower "$actual")" != "$(lower "$expected")" ]; then
    die 3 "checksum mismatch for $ASSET: expected $expected, got $actual. The download was NOT installed. Re-run; if it persists, do not run this file."
  fi
  ok "Checksum verified."
}

stage_release() {
  info "Extracting ..."
  local ex="$WORK/extract"
  mkdir -p "$ex"
  tar -xzf "$WORK/$ASSET" -C "$ex" || fail "failed to extract $ASSET"
  # `find ... | sed -n 1p` instead of `-quit`: BSD find on older macOS lacks it.
  STAGED_BIN="$(find "$ex" -maxdepth 3 -type f -name alphacode | sed -n '1p')"
  [ -n "$STAGED_BIN" ] || fail "the extracted archive did not contain an 'alphacode' binary"
  # Release wrapper scripts may ship .bin payload files that must sit beside it.
  STAGED_PAYLOAD="$(find "$ex" -maxdepth 3 -type f -name '*.bin' || true)"
}

# Copy into the destination directory under a temporary name, then rename over
# the target. rename(2) is atomic and works even while the old binary is running,
# which a direct overwrite does not ("Text file busy").
install_atomic() {  # src dest [mode]
  local src="$1" dest="$2" mode="${3:-}" dir tmp
  dir="${dest%/*}"
  mkdir -p "$dir" 2>/dev/null \
    || fail "cannot create $dir (permission denied?). Try --prefix ~/.local, or re-run with sudo."
  tmp="$dir/.${dest##*/}.new.$$"
  if ! cp -p "$src" "$tmp" 2>/dev/null; then
    rm -f "$tmp"
    fail "cannot write to $dir (permission denied?). Try --prefix ~/.local, or re-run with sudo."
  fi
  if [ -n "$mode" ]; then chmod "$mode" "$tmp"; fi
  mv -f "$tmp" "$dest" || { rm -f "$tmp"; fail "cannot replace $dest"; }
}

install_staged() {
  install_atomic "$STAGED_BIN" "$BIN_DIR/alphacode" 0755
  local f
  if [ -n "$STAGED_PAYLOAD" ]; then
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      install_atomic "$f" "$BIN_DIR/${f##*/}"
    done <<EOF
$STAGED_PAYLOAD
EOF
  fi
}

# Fallback: no artifact for this platform/arch. Clone, build with cargo, install.
# Needs git, cargo, rustc >= 1.91 and a C toolchain; 5-30 minutes the first time.
build_from_source() {
  # rustup installs to ~/.cargo/bin, which a fresh `curl | bash` may not have.
  if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  fi
  command -v git   >/dev/null 2>&1 || fail "git is required to build from source"
  command -v cargo >/dev/null 2>&1 || fail "cargo is required to build from source (install Rust from https://rustup.rs)"
  command -v rustc >/dev/null 2>&1 || fail "rustc was not found (install Rust from https://rustup.rs)"
  if ! command -v cc >/dev/null 2>&1 && ! command -v gcc >/dev/null 2>&1 && ! command -v clang >/dev/null 2>&1; then
    if [ "$PLATFORM" = macos ]; then
      fail "a C toolchain is required to build from source: run 'xcode-select --install'"
    fi
    fail "a C toolchain is required to build from source (Debian/Ubuntu: sudo apt-get install -y build-essential pkg-config libxkbcommon-dev)"
  fi

  # Edition 2024 plus the current dependency MSRV need rustc >= 1.91.
  local rust_ver major minor
  rust_ver="$(rustc --version 2>/dev/null | awk '{ print $2 }')" || true
  if [ -n "$rust_ver" ]; then
    major="${rust_ver%%.*}"
    minor="$(printf '%s' "$rust_ver" | awk -F. '{ print $2 }')"
    if [ "${major:-0}" -lt 1 ] || { [ "${major:-0}" -eq 1 ] && [ "${minor:-0}" -lt 91 ]; }; then
      fail "rustc $rust_ver is too old; need >= 1.91 (update with 'rustup update')"
    fi
  fi

  export GIT_TERMINAL_PROMPT=0
  local src="$WORK/src"
  info "Cloning $REPO into a temporary build directory ..."
  if [ -n "$SOURCE_REF" ]; then
    git clone --quiet --depth 1 --branch "$SOURCE_REF" "https://github.com/$REPO.git" "$src" \
      || fail "git clone failed (ref: $SOURCE_REF)"
  else
    git clone --quiet --depth 1 "https://github.com/$REPO.git" "$src" \
      || fail "git clone failed"
  fi

  info "Compiling alphacode (this can take 5-30 minutes on a first build) ..."
  # No --locked on purpose: the committed Cargo.lock does not list
  # platform-conditional deps for every target triple, and CI builds without it.
  ( cd "$src" && cargo build --release --target-dir "$src/target" ) \
    || fail "cargo build failed"

  local built="$src/target/release/alphacode"
  [ -f "$built" ] || fail "the build succeeded but target/release/alphacode was not produced"
  STAGED_BIN="$built"
  STAGED_PAYLOAD=""
  BUILT_FROM_SOURCE=1
}

# -- after install --------------------------------------------------------------

# Files fetched with curl carry no quarantine flag, but remove it anyway if
# present, and re-sign ad hoc when the signature is invalid: Apple Silicon kills
# unsigned or invalidly signed binaries with "Killed: 9".
macos_fixups() {
  [ "$PLATFORM" = macos ] || return 0
  xattr -d com.apple.quarantine "$BIN_DIR/alphacode" 2>/dev/null || true
  if command -v codesign >/dev/null 2>&1 && ! codesign -v "$BIN_DIR/alphacode" >/dev/null 2>&1; then
    codesign --force --sign - "$BIN_DIR/alphacode" >/dev/null 2>&1 || true
  fi
  return 0
}

# The prebuilt Linux binary links the system libxkbcommon. Desktop distros have
# it; minimal server and container images often do not, and the binary then dies
# at startup. Say what is actually wrong so the install does not look broken.
diagnose_start_failure() {
  warn "alphacode was installed but cannot start: $(printf '%s' "$1" | sed -n '1p')"
  START_FAILED=1
  if [ "$PLATFORM" = linux ] && command -v ldd >/dev/null 2>&1; then
    local missing
    missing="$(ldd "$BIN_DIR/alphacode" 2>/dev/null | awk '/not found/ { print $1 }' | sort -u | tr '\n' ' ')" || true
    if [ -n "$missing" ]; then
      warn "missing shared libraries: ${missing% }"
      if   command -v apt-get >/dev/null 2>&1; then warn "  sudo apt-get install -y libxkbcommon0"
      elif command -v dnf     >/dev/null 2>&1; then warn "  sudo dnf install -y libxkbcommon"
      elif command -v pacman  >/dev/null 2>&1; then warn "  sudo pacman -S libxkbcommon"
      elif command -v apk     >/dev/null 2>&1; then warn "  sudo apk add libxkbcommon"
      elif command -v zypper  >/dev/null 2>&1; then warn "  sudo zypper install libxkbcommon0"
      fi
    fi
  fi
  case "$1" in
    *"No such file"*|*"not found"*)
      warn "this often means a glibc/musl mismatch; try --from-source" ;;
  esac
  warn "or build from source: re-run with --from-source"
}

check_installed() {
  local out rc=0
  out="$("$BIN_DIR/alphacode" --version 2>&1)" || rc=$?
  if [ "$rc" -ne 0 ]; then
    diagnose_start_failure "$out"
    return 0
  fi
  INSTALLED_VERSION="$(printf '%s\n' "$out" \
    | sed -nE 's/.*alphacode[[:space:]]+v?([0-9][0-9A-Za-z.+-]*).*/\1/p' | sed -n '1p')"
  if [ -n "$INSTALLED_VERSION" ]; then
    ok "Installed version: $INSTALLED_VERSION"
  else
    ok "Installed (version output: $(printf '%s' "$out" | sed -n '1p'))"
  fi
}

# -- PATH configuration ---------------------------------------------------------
#
# Editing startup files is the one intrusive thing this installer does, so it is
# tightly constrained: one clearly marked block per file, never reordering
# anything already there, rewritten in place on re-run (no duplicates), and
# skipped entirely with --no-path or when the dir is already on PATH.

detect_shell() {
  local sp="${SHELL:-}"
  if [ -z "$sp" ]; then
    if [ "$PLATFORM" = macos ] && command -v dscl >/dev/null 2>&1; then
      sp="$(dscl . -read "/Users/$(id -un)" UserShell 2>/dev/null | awk '{ print $2 }')" || true
    elif command -v getent >/dev/null 2>&1; then
      sp="$(getent passwd "$(id -un)" 2>/dev/null | awk -F: '{ print $7 }')" || true
    fi
  fi
  [ -n "$sp" ] || return 1
  SHELL_PATH="$sp"
  local name="${sp##*/}"
  name="${name#-}"   # login shells can appear as "-bash"
  lower "$name"
  printf '\n'
}

add_target() {  # kind path  (de-duplicated by path)
  case "$TARGETS" in
    *"|$2"$'\n'*) return 0 ;;
  esac
  TARGETS="${TARGETS}${1}|${2}"$'\n'
}

# bash reads the FIRST of these at login. Never create ~/.bash_profile when
# ~/.profile exists: doing so makes bash stop reading ~/.profile.
bash_login_file() {
  local f
  for f in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
    if [ -f "$f" ]; then printf '%s\n' "$f"; return 0; fi
  done
  # macOS Terminal opens LOGIN shells, which never read ~/.bashrc by themselves.
  if [ "$PLATFORM" = macos ]; then printf '%s\n' "$HOME/.bash_profile"; else printf '%s\n' "$HOME/.profile"; fi
}

csh_rc_file() {
  if [ "$1" = tcsh ] && [ -f "$HOME/.tcshrc" ]; then printf '%s\n' "$HOME/.tcshrc"
  elif [ -f "$HOME/.cshrc" ]; then printf '%s\n' "$HOME/.cshrc"
  elif [ "$1" = tcsh ]; then printf '%s\n' "$HOME/.tcshrc"
  else printf '%s\n' "$HOME/.cshrc"
  fi
}

nu_env_file() {
  local f=""
  if command -v nu >/dev/null 2>&1; then
    f="$(nu -c 'echo $nu.env-path' 2>/dev/null | sed -n '1p')" || true
  fi
  case "$f" in
    /*) printf '%s\n' "$f"; return 0 ;;
  esac
  if [ "$PLATFORM" = macos ]; then
    printf '%s\n' "$HOME/Library/Application Support/nushell/env.nu"
  else
    printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/nushell/env.nu"
  fi
}

# Decide which startup files to edit. The detected login shell's files are
# created if missing. For every OTHER shell, only files that already exist are
# touched, so unused shells do not get dotfiles they never asked for. ~/.profile
# is included when present: it is what Ubuntu/Debian GUI sessions and login
# shells read.
collect_targets() {  # primary-shell-name
  local primary="$1"
  local zdot="${ZDOTDIR:-$HOME}"
  local xdg="${XDG_CONFIG_HOME:-$HOME/.config}"
  TARGETS=""

  case "$primary" in
    bash)       add_target posix "$HOME/.bashrc"; add_target posix "$(bash_login_file)" ;;
    zsh)        add_target posix "$zdot/.zshrc";  add_target posix "$zdot/.zprofile" ;;
    fish)       add_target fish  "$xdg/fish/conf.d/alphacode.fish" ;;
    nu|nushell) add_target nu    "$(nu_env_file)" ;;
    csh|tcsh)   add_target csh   "$(csh_rc_file "$primary")" ;;
    *)          add_target posix "$HOME/.profile" ;;   # sh, dash, ash, ksh, unknown
  esac

  [ -f "$HOME/.profile" ]      && add_target posix "$HOME/.profile"
  [ -f "$HOME/.bash_profile" ] && add_target posix "$HOME/.bash_profile"
  [ -f "$HOME/.bashrc" ]       && add_target posix "$HOME/.bashrc"
  [ -f "$zdot/.zshrc" ]        && add_target posix "$zdot/.zshrc"
  [ -f "$zdot/.zprofile" ]     && add_target posix "$zdot/.zprofile"
  [ -d "$xdg/fish" ]           && add_target fish  "$xdg/fish/conf.d/alphacode.fish"
  return 0
}

escape_dq() { printf '%s' "$1" | sed -e 's/[\\"$`]/\\&/g'; }

# Directory as it should appear inside double quotes. Under $HOME it is written
# as $HOME/... so the startup file survives a renamed home or a copied dotfile.
dir_expr() {
  case "$BIN_DIR" in
    "$HOME"/*) printf '$HOME/%s' "$(escape_dq "${BIN_DIR#"$HOME"/}")" ;;
    *)         escape_dq "$BIN_DIR" ;;
  esac
}

# The block checks PATH at run time, so loading it from several files (for
# example both ~/.zprofile and ~/.zshrc) never adds the directory twice.
block_posix() {
  local d; d="$(dir_expr)"
  printf '%s\n' "$BLOCK_BEGIN" \
    '# Added by the AlphaCode installer. Delete this block to undo.' \
    'case ":$PATH:" in' \
    "  *\":$d:\"*) ;;" \
    "  *) export PATH=\"$d:\$PATH\" ;;" \
    'esac' \
    "$BLOCK_END"
}

block_fish() {
  local d; d="$(dir_expr)"
  printf '%s\n' "$BLOCK_BEGIN" \
    '# Added by the AlphaCode installer. Delete this block to undo.' \
    "if not contains -- \"$d\" \$PATH" \
    "    set -gx PATH \"$d\" \$PATH" \
    'end' \
    "$BLOCK_END"
}

block_nu() {
  local d; d="$(printf '%s' "$BIN_DIR" | sed -e 's/[\\"]/\\&/g')"
  printf '%s\n' "$BLOCK_BEGIN" \
    '# Added by the AlphaCode installer. Delete this block to undo.' \
    "if not (\"$d\" in \$env.PATH) { \$env.PATH = (\$env.PATH | prepend \"$d\") }" \
    "$BLOCK_END"
}

block_csh() {
  printf '%s\n' "$BLOCK_BEGIN" \
    '# Added by the AlphaCode installer. Delete this block to undo.' \
    "if ( \"\$PATH\" !~ *$BIN_DIR* ) setenv PATH \"$BIN_DIR:\$PATH\"" \
    "$BLOCK_END"
}

# Returns 0 = written, 2 = already up to date, 1 = failed.
write_block() {  # kind file
  local kind="$1" file="$2" content current tmp
  case "$kind" in
    posix) content="$(block_posix)" ;;
    fish)  content="$(block_fish)" ;;
    nu)    content="$(block_nu)" ;;
    csh)   content="$(block_csh)" ;;
    *)     return 1 ;;
  esac

  if [ -f "$file" ]; then
    if grep -qF "$BLOCK_BEGIN" "$file"; then
      current="$(awk -v b="$BLOCK_BEGIN" -v e="$BLOCK_END" \
        '$0 == b { p = 1 } p { print } p && $0 == e { exit }' "$file")"
      [ "$current" != "$content" ] || return 2
      [ -z "$PATH_DRY_RUN" ] || return 0
      [ -w "$file" ] || return 1
      # Replace our own block in place; everything else is left byte-for-byte.
      tmp="$WORK/rc.$$"
      awk -v b="$BLOCK_BEGIN" -v e="$BLOCK_END" \
        '$0 == b { skip = 1; next } skip && $0 == e { skip = 0; next } !skip { print }' \
        "$file" > "$tmp" || return 1
      { cat "$tmp"; printf '\n%s\n' "$content"; } > "$file" || return 1
      return 0
    fi
    # A block from an older installer that already names this directory.
    if grep -qF "$LEGACY_MARKER" "$file" && grep -qF "$BIN_DIR" "$file"; then
      return 2
    fi
    [ -z "$PATH_DRY_RUN" ] || return 0
    [ -w "$file" ] || return 1
    # Make sure the last existing line ends in a newline before appending.
    if [ -s "$file" ] && [ -n "$(tail -c1 "$file")" ]; then printf '\n' >> "$file" || return 1; fi
    printf '\n%s\n' "$content" >> "$file" || return 1
    return 0
  fi

  [ -z "$PATH_DRY_RUN" ] || return 0
  mkdir -p "$(dirname "$file")" 2>/dev/null || return 1
  printf '%s\n' "$content" > "$file" 2>/dev/null || return 1
  return 0
}

pick_link_dir() {
  if [ -n "${ALPHACODE_LINK_DIR:-}" ]; then printf '%s\n' "$ALPHACODE_LINK_DIR"; return 0; fi
  local d
  for d in /usr/local/bin /opt/homebrew/bin; do
    if [ -d "$d" ] && [ -w "$d" ]; then printf '%s\n' "$d"; return 0; fi
  done
  printf '%s\n' /usr/local/bin
}

# Symlink into a system bin dir so `alphacode` resolves with no PATH edit at all,
# including for GUI apps, cron and non-interactive ssh.
link_into_system_bin() {  # quiet?
  local quiet="${1:-}" target link
  target="$(pick_link_dir)"
  link="$target/alphacode"
  [ "$target" != "$BIN_DIR" ] || return 0
  if [ ! -d "$target" ]; then
    [ -n "$quiet" ] || warn "$target does not exist; create it, or use the PATH edit instead"
    return 1
  fi
  if [ -e "$link" ] && [ ! -L "$link" ]; then
    [ -n "$quiet" ] || warn "$link already exists and is not a symlink; leaving it alone"
    return 1
  fi
  if [ ! -w "$target" ]; then
    [ -n "$quiet" ] || { warn "$target is not writable. To link it, run:"; warn "  sudo ln -sf \"$BIN_DIR/alphacode\" \"$link\""; }
    return 1
  fi
  ln -sf "$BIN_DIR/alphacode" "$link"
  LINK_PATH="$link"
  info "linked $link -> $BIN_DIR/alphacode"
}

configure_path() {
  PATH_STATE=""
  PATH_CHANGED=""

  if [ -n "$NO_PATH" ]; then PATH_STATE="skipped"; return 0; fi

  case ":$PATH:" in
    *":$BIN_DIR:"*) PATH_STATE="already"; return 0 ;;
  esac

  local primary="" kind file rc any_ok=0
  primary="$(detect_shell)" || primary=""
  collect_targets "$primary"

  while IFS='|' read -r kind file; do
    [ -n "$kind" ] || continue
    rc=0
    write_block "$kind" "$file" || rc=$?
    case "$rc" in
      0) any_ok=1; PATH_CHANGED="${PATH_CHANGED}${file}"$'\n'
         if [ -n "$PATH_DRY_RUN" ]; then info "[dry-run] would update $(tilde "$file")"; fi ;;
      2) any_ok=1 ;;
      *) warn "could not update $(tilde "$file") (read-only or not writable)" ;;
    esac
  done <<EOF
$TARGETS
EOF

  if [ -n "$PATH_DRY_RUN" ]; then PATH_STATE="dry-run"; return 0; fi

  # No startup file could be edited: try a symlink instead, quietly.
  if [ "$any_ok" -eq 0 ]; then
    if link_into_system_bin quiet; then PATH_STATE="linked"; return 0; fi
    PATH_STATE="failed"; return 0
  fi

  if [ -n "$PATH_CHANGED" ]; then PATH_STATE="configured"; else PATH_STATE="up-to-date"; fi
  PRIMARY_SHELL="$primary"
}

PRIMARY_SHELL=""
VERIFY_STATE=""
VERIFY_MSG=""

# Start a clean shell the way a NEW terminal would (empty environment, minimal
# PATH, interactive, with and without --login) and ask it where `alphacode` is.
# This proves the startup-file edit works instead of assuming it does. A shell
# that fails to start is reported as "unknown", never as an install failure.
verify_new_shells() {
  [ -z "${ALPHACODE_SKIP_VERIFY:-}" ] || return 0
  local name="$PRIMARY_SHELL" sp flag out found bad="" ran=0 ours="$BIN_DIR/alphacode"
  case "$name" in bash|zsh|fish) ;; *) return 0 ;; esac
  sp="$(command -v "$name" 2>/dev/null || true)"
  [ -n "$sp" ] || return 0

  for flag in -ic -lic; do
    out="$(run_with_timeout 12 env -i \
      HOME="$HOME" USER="$(id -un)" LOGNAME="$(id -un)" SHELL="$sp" TERM=dumb \
      PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
      ${ZDOTDIR:+"ZDOTDIR=$ZDOTDIR"} ${XDG_CONFIG_HOME:+"XDG_CONFIG_HOME=$XDG_CONFIG_HOME"} \
      "$sp" "$flag" 'printf "__AC_START__\n"; command -v alphacode; printf "__AC_END__\n"' \
      </dev/null 2>/dev/null || true)"
    case "$out" in *__AC_START__*) ran=1 ;; *) continue ;; esac
    found="$(printf '%s\n' "$out" | sed -n '/__AC_START__/,/__AC_END__/p' | grep '^/' | sed -n '1p' || true)"
    if [ -z "$found" ]; then
      bad="${bad} ${name} ${flag}: not found;"
    elif [ "$found" != "$ours" ] && [ "$found" != "$LINK_PATH" ]; then
      bad="${bad} ${name} ${flag}: a different alphacode wins ($found);"
    fi
  done

  if [ "$ran" -eq 0 ]; then VERIFY_STATE="unknown"; return 0; fi
  if [ -z "$bad" ]; then
    VERIFY_STATE="ok"
    VERIFY_MSG="Verified: new $name terminals find $(tilde "$ours")."
  else
    VERIFY_STATE="bad"
    VERIFY_MSG="A fresh $name shell did not resolve to the new binary:${bad%;}"
  fi
}

# -- finishing ------------------------------------------------------------------

print_summary() {
  local existing
  echo
  existing="$(command -v alphacode 2>/dev/null || true)"
  if [ -n "$existing" ] && [ "$existing" != "$BIN_DIR/alphacode" ] && [ "$existing" != "$LINK_PATH" ] \
     && [ "${PATH_STATE:-}" = "already" ]; then
    warn "Another alphacode is earlier on your PATH: $existing"
    warn "It will run instead of the one just installed. Remove it, or put $BIN_DIR first."
  fi

  case "$PATH_STATE" in
    already)
      ok "$(tilde "$BIN_DIR") is already on your PATH; nothing to configure." ;;
    configured|up-to-date)
      if [ -n "$PATH_CHANGED" ]; then
        local f list=""
        while IFS= read -r f; do [ -n "$f" ] && list="${list:+$list, }$(tilde "$f")"; done <<EOF
$PATH_CHANGED
EOF
        ok "Added $(tilde "$BIN_DIR") to your PATH via: $list"
      else
        ok "Your shell startup files already put $(tilde "$BIN_DIR") on PATH."
      fi ;;
    linked)
      ok "Linked into $(dirname "$LINK_PATH"), which is already on PATH." ;;
    dry-run)
      info "[dry-run] nothing was written." ;;
    skipped)
      warn "PATH was left alone (--no-path). Run it by full path: $BIN_DIR/alphacode" ;;
    failed)
      warn "Could not update your shell startup files automatically."
      printf '%sAdd this line to your shell profile, then open a new terminal:%s\n' "$C_DIM" "$C_OFF"
      printf '  export PATH="%s:$PATH"\n' "$BIN_DIR"
      printf '%sOr link it into a directory that is already on PATH:%s\n' "$C_DIM" "$C_OFF"
      printf '  sudo ln -sf "%s/alphacode" /usr/local/bin/alphacode\n' "$BIN_DIR" ;;
  esac

  case "$VERIFY_STATE" in
    ok)  ok "$VERIFY_MSG" ;;
    bad) warn "$VERIFY_MSG"; warn "Open a new terminal and run: command -v alphacode" ;;
  esac

  # An installer is a child process and cannot change its parent shell, so say
  # exactly how to get alphacode in the terminal the user is standing in.
  case "$PATH_STATE" in
    configured|up-to-date|linked)
      echo
      printf 'To use it in THIS terminal right now (an installer cannot change its parent shell):\n'
      case "$PRIMARY_SHELL" in
        fish)      printf '  exec fish\n' ;;
        nu|nushell) printf '  exec nu\n' ;;
        "")        printf '  export PATH="%s:$PATH"\n' "$BIN_DIR" ;;
        *)         printf '  exec "$SHELL" -l\n' ;;
      esac
      printf 'Or just open a new terminal window.\n' ;;
  esac

  echo
  info "Next:"
  printf '  alphacode login    # connect a model provider\n'
  printf '  alphacode          # start the TUI\n'
  printf '  alphacode doctor   # check your setup\n'
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --version)      need_arg "$@"; VERSION="$2"; shift 2 ;;
      --version=*)    VERSION="${1#*=}"; shift ;;
      --prefix)       need_arg "$@"; PREFIX="$2"; shift 2 ;;
      --prefix=*)     PREFIX="${1#*=}"; shift ;;
      --bin-dir)      need_arg "$@"; BIN_DIR="$2"; shift 2 ;;
      --bin-dir=*)    BIN_DIR="${1#*=}"; shift ;;
      --source-ref)   need_arg "$@"; SOURCE_REF="$2"; shift 2 ;;
      --source-ref=*) SOURCE_REF="${1#*=}"; shift ;;
      --add-path)     shift ;;   # compatibility no-op: PATH is configured by default
      --no-path)      NO_PATH=1; shift ;;
      --path-dry-run) PATH_DRY_RUN=1; shift ;;
      --link)         LINK_BIN=1; shift ;;
      --source-only)  SOURCE_ONLY=1; shift ;;
      --from-source)  NEVER_RELEASE=1; shift ;;
      -h|--help)      usage; exit 0 ;;
      *) fail "unknown flag: $1 (try --help)" ;;
    esac
  done
}

main() {
  setup_colors
  parse_args "$@"

  command -v curl >/dev/null 2>&1 || fail "curl is required"
  command -v tar  >/dev/null 2>&1 || fail "tar is required"

  detect_platform
  resolve_locations

  local tmpbase="${TMPDIR:-/tmp}"
  tmpbase="${tmpbase%/}"
  WORK="$(mktemp -d "$tmpbase/alphacode-install.XXXXXX")" || fail "cannot create a temporary directory in $tmpbase"
  trap cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  if [ -n "$NEVER_RELEASE" ]; then
    info "--from-source requested; skipping the release download."
    build_from_source
  else
    resolve_release_bases
    build_candidates
    if download_release; then
      info "Downloaded $ASSET"
      verify_checksum
      stage_release
    else
      if [ -n "$SOURCE_ONLY" ]; then
        die 2 "download failed (no asset for $PLATFORM/$ARCH at $VERSION; try --version) and --source-only is set"
      fi
      warn "no prebuilt asset for $PLATFORM/$ARCH at $VERSION; falling back to building from source."
      build_from_source
    fi
  fi

  install_staged
  ok "Installed -> $(tilde "$BIN_DIR")/alphacode$([ -n "$BUILT_FROM_SOURCE" ] && printf ' (built from source)')"
  macos_fixups
  check_installed

  if [ -n "$LINK_BIN" ]; then
    link_into_system_bin || warn "symlink not created; $BIN_DIR still works"
  fi

  configure_path
  case "$PATH_STATE" in
    configured|up-to-date|linked) verify_new_shells ;;
  esac

  print_summary

  if [ -n "$START_FAILED" ]; then exit 4; fi
  return 0
}

# ALPHACODE_NO_AUTORUN=1 lets tests source this file and call its functions.
if [ "${ALPHACODE_NO_AUTORUN:-}" != "1" ]; then
  main "$@"
fi
