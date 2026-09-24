#!/usr/bin/env bash
# shellcheck disable=SC2024
# Bootstrap tooling for the /bugbounty skill.
#
# Supported platforms (detected automatically):
#   - Windows native (Git Bash / MSYS2 / Cygwin) — winget/scoop/choco + Go/pip
#   - WSL — ONLY if wsl.exe exists AND a distro is registered AND status is Running
#   - Debian/Ubuntu/Kali/Parrot — apt
#   - RHEL/CentOS/Rocky/Alma/Fedora/Amazon Linux — dnf or yum
#   - Alpine — apk
#   - Arch/Manjaro — pacman
#   - openSUSE/SLES — zypper
#   - macOS — brew
#
# WSL policy: never invoke `wsl.exe` unless `wsl_status` returns "ready".
# Native Windows tools are always preferred over WSL.
#
# Usage:
#   bash scripts/install_bugbounty_tools.sh [OPTIONS] MODE
#
# Modes:
#   go, os, apt, dnf, yum, apk, pacman, zypper, brew, win, pip,
#   nuclei, wordlists, manual, all, --verify, --platform
#
# Options:
#   --dry-run   Show what would be installed without installing
#   --force     Reinstall packages even if already present
#
# Examples:
#   bash scripts/install_bugbounty_tools.sh all
#   bash scripts/install_bugbounty_tools.sh --dry-run go
#   bash scripts/install_bugbounty_tools.sh --platform
#   bash scripts/install_bugbounty_tools.sh --verify

set -euo pipefail

DRY_RUN=false
FORCE=false
MODE=""
FAILED=()
SUCCEEDED=()
SKIPPED=()
LOG_DIR="${HOME}/.bugbounty-tools"
LOG_FILE=""
BB_VENV="${HOME}/.bugbounty-tools/venv"
WORDLIST_DIR="${HOME}/.bugbounty-tools/wordlists"
PLATFORM="unknown"
PKG_FAMILY="none"

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    --force) FORCE=true; shift ;;
    -*) if [ -z "$MODE" ]; then MODE="$1"; shift; else echo "Unknown option: $1" >&2; exit 2; fi ;;
    *) MODE="$1"; shift ;;
  esac
done
MODE="${MODE:-all}"

setup_logging() {
  mkdir -p "$LOG_DIR"
  LOG_FILE="${LOG_DIR}/install-$(date +%Y-%m-%d_%H%M%S).log"
  log_info "Logging to $LOG_FILE"
}

log_info() { echo "==> $*" | tee -a "${LOG_FILE:-/dev/null}"; }
log_warn() { echo "WARNING: $*" | tee -a "${LOG_FILE:-/dev/null}" >&2; }
log_error() { echo "ERROR: $*" | tee -a "${LOG_FILE:-/dev/null}" >&2; }

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    log_error "'$cmd' is required but not found in PATH"
    return 1
  fi
}

have() { command -v "$1" >/dev/null 2>&1; }

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------
detect_platform() {
  local uname_s uname_m
  uname_s="$(uname -s 2>/dev/null || echo unknown)"
  uname_m="$(uname -m 2>/dev/null || echo unknown)"
  PLATFORM="unknown (${uname_s}/${uname_m})"
  PKG_FAMILY="none"

  # Already inside WSL?
  if [ -f /proc/version ] && grep -qi microsoft /proc/version 2>/dev/null; then
    PLATFORM="wsl"
  fi

  case "$uname_s" in
    Darwin)
      PLATFORM="macos"
      if have brew; then PKG_FAMILY="brew"; fi
      ;;
    MINGW*|MSYS*|CYGWIN*)
      PLATFORM="windows-native"
      if have winget; then PKG_FAMILY="winget"
      elif have scoop; then PKG_FAMILY="scoop"
      elif have choco; then PKG_FAMILY="choco"
      fi
      ;;
    Linux)
      if [ "$PLATFORM" = "wsl" ]; then
        # Still prefer native package manager inside the running WSL distro
        :
      else
        PLATFORM="linux"
      fi
      if [ -f /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        local id="${ID:-}" id_like="${ID_LIKE:-}"
        case "$id $id_like" in
          *debian*|*ubuntu*|*kali*|*parrot*)
            PLATFORM="${PLATFORM:+${PLATFORM}/}debian-family"; PKG_FAMILY="apt" ;;
          *rhel*|*centos*|*rocky*|*almalinux*|*fedora*|*amzn*)
            PLATFORM="${PLATFORM:+${PLATFORM}/}rhel-family"
            if have dnf; then PKG_FAMILY="dnf"; elif have yum; then PKG_FAMILY="yum"; fi
            ;;
          *alpine*)
            PLATFORM="${PLATFORM:+${PLATFORM}/}alpine"; PKG_FAMILY="apk" ;;
          *arch*|*manjaro*)
            PLATFORM="${PLATFORM:+${PLATFORM}/}arch"; PKG_FAMILY="pacman" ;;
          *suse*|*opensuse*)
            PLATFORM="${PLATFORM:+${PLATFORM}/}suse"; PKG_FAMILY="zypper" ;;
        esac
      fi
      # Fallback if os-release missing or unmatched
      if [ "$PKG_FAMILY" = "none" ]; then
        if have apt-get; then PKG_FAMILY="apt"
        elif have dnf; then PKG_FAMILY="dnf"
        elif have yum; then PKG_FAMILY="yum"
        elif have apk; then PKG_FAMILY="apk"
        elif have pacman; then PKG_FAMILY="pacman"
        elif have zypper; then PKG_FAMILY="zypper"
        fi
      fi
      ;;
  esac

  # Windows native already detected MINGW/MSYS — if uname says Linux but
  # we're under Git Bash somehow, still keep linux path.
  if [ "$PLATFORM" = "unknown" ]; then
    PLATFORM="$uname_s"
  fi
}

# WSL readiness: ready only if wsl.exe exists, lists a distro, and status is Running.
# Returns: ready | no-wsl | no-distro | not-running | unknown
wsl_status() {
  if ! have wsl.exe && ! have wsl; then
    echo "no-wsl"; return 0
  fi
  local wsl_cmd=""
  if have wsl.exe; then wsl_cmd="wsl.exe"; elif have wsl; then wsl_cmd="wsl"; else
    echo "no-wsl"; return 0
  fi
  # List distros (quiet)
  local distros=""
  distros="$("$wsl_cmd" -l -q 2>/dev/null | tr -d '\0' || true)"
  # Strip UTF-1LE artifacts if present (common when wsl prints UTF-16)
  if printf '%s' "$distros" | grep -q $'\0'; then
    distros="$(printf '%s' "$distros" | tr -d '\0')"
  fi
  distros="$(printf '%s' "$distros" | sed 's/\r//g' | sed '/^$/d')"
  if [ -z "$distros" ]; then
    # Could be empty list or UTF-16 garbage; try status line
    local status_out
    status_out="$("$wsl_cmd" -l -v 2>/dev/null | tr -d '\0' || true)"
    if printf '%s' "$status_out" | grep -qi 'Stopped\|Running\|Installing'; then
      if printf '%s' "$status_out" | grep -qi 'Running'; then
        echo "ready"; return 0
      fi
      echo "not-running"; return 0
    fi
    echo "no-distro"; return 0
  fi
  # Any registered distro that is Running?
  local verbose
  verbose="$("$wsl_cmd" -l -v 2>/dev/null | tr -d '\0' || true)"
  if printf '%s' "$verbose" | grep -qi 'Running'; then
    echo "ready"
  else
    # -l -q returned names but none Running
    if printf '%s' "$verbose" | grep -qiE 'Stopped|Installing|Uninstalling'; then
      echo "not-running"
    else
      # Names present; assume at least one can start — still not "ready" until Running
      echo "no-distro"
    fi
  fi
}

print_platform() {
  detect_platform
  local wsl
  wsl="$(wsl_status)"
  echo "Platform:   $PLATFORM"
  echo "Pkg family: $PKG_FAMILY"
  echo "WSL:        $wsl"
  echo ""
  echo "Notes:"
  echo "  - Native Windows tools (Go, winget, pip) are preferred."
  echo "  - WSL is used only when status=ready (installed + Running distro)."
  if [ "$PKG_FAMILY" = "none" ] && [ "$PLATFORM" != "windows-native" ]; then
    echo "  - No supported package manager detected; use Go/pip/manual paths."
  fi
}

# ---------------------------------------------------------------------------
# Go tools (cross-platform — primary path on every OS)
# ---------------------------------------------------------------------------
GO_TOOLS=(
  "subfinder:github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest"
  "httpx:github.com/projectdiscovery/httpx/cmd/httpx@latest"
  "nuclei:github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest"
  "katana:github.com/projectdiscovery/katana/cmd/katana@latest"
  "dnsx:github.com/projectdiscovery/dnsx/cmd/dnsx@latest"
  "naabu:github.com/projectdiscovery/naabu/v2/cmd/naabu@latest"
  "assetfinder:github.com/tomnomnom/assetfinder@latest"
  "gau:github.com/lc/gau/v2/cmd/gau@latest"
  "waybackurls:github.com/tomnomnom/waybackurls@latest"
  "anew:github.com/tomnomnom/anew@latest"
  "ffuf:github.com/ffuf/ffuf/v2@latest"
  "gobuster:github.com/OJ/gobuster/v3@latest"
  "dalfox:github.com/hahwul/dalfox/v2@latest"
  "subzy:github.com/Pentestify/subzy@latest"
  "arjun:github.com/s0md3v/Arjun@latest"
  "httprobe:github.com/tomnomnom/httprobe@latest"
  "qsreplace:github.com/tomnomnom/qsreplace@latest"
  "gf:github.com/tomnomnom/gf@latest"
  "trufflehog:github.com/trufflesecurity/trufflehog/v3@latest"
  "gitleaks:github.com/gitleaks/gitleaks/v8@latest"
)

ensure_go_path_hint() {
  local go_bin="${GOBIN:-${GOPATH:-$HOME/go}/bin}"
  case ":$PATH:" in
    *":${go_bin}:"*) ;;
    *)
      log_warn "Add Go bin to PATH: export PATH=\"\$PATH:${go_bin}\""
      # Windows Git Bash: also mention Windows PATH
      if [ "$PLATFORM" = "windows-native" ]; then
        log_warn "Windows: add %USERPROFILE%\\go\\bin (or GOBIN) to System PATH"
      fi
      ;;
  esac
}

install_go() {
  if ! have go; then
    log_warn "go not found — skipping Go tool installs"
    if [ "$PLATFORM" = "windows-native" ]; then
      log_warn "Install Go: winget install GoLang.Go  |  https://go.dev/dl/"
    elif [ "$PKG_FAMILY" = "apt" ]; then
      log_warn "Install Go: sudo apt install -y golang-go"
    elif [ "$PKG_FAMILY" = "dnf" ] || [ "$PKG_FAMILY" = "yum" ]; then
      log_warn "Install Go: sudo ${PKG_FAMILY} install -y golang"
    elif [ "$PKG_FAMILY" = "apk" ]; then
      log_warn "Install Go: sudo apk add go"
    elif [ "$PKG_FAMILY" = "pacman" ]; then
      log_warn "Install Go: sudo pacman -S go"
    elif [ "$PKG_FAMILY" = "zypper" ]; then
      log_warn "Install Go: sudo zypper install go"
    elif [ "$PKG_FAMILY" = "brew" ]; then
      log_warn "Install Go: brew install go"
    else
      log_warn "Install Go: https://go.dev/dl/"
    fi
    SKIPPED+=("go:all")
    return 0
  fi

  ensure_go_path_hint

  local to_install=()
  for entry in "${GO_TOOLS[@]}"; do
    local bin="${entry%%:*}"
    if [ "$FORCE" = false ] && have "$bin"; then
      SKIPPED+=("go:$bin")
      continue
    fi
    to_install+=("$bin")
  done

  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "go: all tools already installed"
    return 0
  fi

  log_info "go: installing ${#to_install[@]} tools"
  if [ "$DRY_RUN" = true ]; then
    for entry in "${GO_TOOLS[@]}"; do
      local bin="${entry%%:*}" path="${entry#*:}"
      if [ "$FORCE" = false ] && have "$bin"; then continue; fi
      log_info "Would: go install -v $path"
    done
    return 0
  fi

  for entry in "${GO_TOOLS[@]}"; do
    local bin="${entry%%:*}"
    local path="${entry#*:}"
    if [ "$FORCE" = false ] && have "$bin"; then
      continue
    fi
    if go install -v "$path" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("go:$bin")
      log_info "installed go:$bin"
    else
      log_warn "go install failed: $bin"
      FAILED+=("go:$bin")
    fi
  done
}

# ---------------------------------------------------------------------------
# Shared package lists (per family — names differ by distro)
# ---------------------------------------------------------------------------
# Debian/Ubuntu/Kali
APT_PKGS=(
  nmap nikto sqlmap ffuf gobuster jq curl wget
  whatweb wafw00f dnsutils whois
)
# Optional seclists may be large / unavailable on minimal images
APT_PKGS_OPTIONAL=(seclists)

# RHEL/CentOS/Rocky/Alma/Fedora/Amazon
# Note: ffuf/gobuster often missing — installed via Go instead
DNF_PKGS=(
  nmap nikto sqlmap jq curl wget bind-utils whois
  whatweb 2>/dev/null || true
)
# Rebuild cleanly without invalid entries
DNF_PKGS=(
  nmap nikto sqlmap jq curl wget bind-utils whois
)
DNF_PKGS_OPTIONAL=(whatweb)

# Alpine
APK_PKGS=(
  nmap nikto sqlmap jq curl wget bind-utils whois
)

# Arch
PACMAN_PKGS=(
  nmap nikto sqlmap jq curl wget bind whois
)

# openSUSE
ZYPPER_PKGS=(
  nmap nikto sqlmap jq curl wget bind-utils whois
)

# macOS Homebrew
BREW_PKGS=(
  nmap nikto sqlmap jq curl wget bind whois
)

# Windows winget (IDs; empty = skip)
WINGET_PKGS=(
  "Insecure.Nmap"
  "Microsoft jq"
  "cURL.cURL"
  "Git.Git"
  "GoLang.Go"
  "Python.Python.3.12"
)
# scoop bucket extras (installed only if scoop present)
SCOOP_PKGS=(nmap jq curl go python)

# ---------------------------------------------------------------------------
# OS package installers
# ---------------------------------------------------------------------------
run_as_root() {
  if [ "$(id -u 2>/dev/null || echo 1)" = "0" ]; then
    "$@"
  elif have sudo; then
    sudo "$@"
  else
    log_warn "Need root for: $* (no sudo)"
    return 1
  fi
}

install_apt() {
  require_cmd apt-get || return 1
  local to_install=()
  for pkg in "${APT_PKGS[@]}"; do
    if [ "$FORCE" = false ] && dpkg -s "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("apt:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  # optional packages
  for pkg in "${APT_PKGS_OPTIONAL[@]}"; do
    if [ "$FORCE" = false ] && dpkg -s "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("apt:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "apt: all packages already installed"; return 0
  fi
  log_info "apt: ${#to_install[@]} packages to install"
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  run_as_root env DEBIAN_FRONTEND=noninteractive apt-get update -q >>"$LOG_FILE" 2>&1 \
    || log_warn "apt-get update failed"
  for pkg in "${to_install[@]}"; do
    if run_as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y -q "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("apt:$pkg")
    else
      log_warn "apt install failed: $pkg"; FAILED+=("apt:$pkg")
    fi
  done
}

install_dnf() {
  require_cmd dnf || return 1
  local to_install=()
  for pkg in "${DNF_PKGS[@]}"; do
    if [ "$FORCE" = false ] && rpm -q "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("dnf:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  for pkg in "${DNF_PKGS_OPTIONAL[@]}"; do
    if [ "$FORCE" = false ] && rpm -q "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("dnf:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "dnf: all packages already installed"; return 0
  fi
  log_info "dnf: ${#to_install[@]} packages to install"
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  for pkg in "${to_install[@]}"; do
    if run_as_root dnf install -y "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("dnf:$pkg")
    else
      log_warn "dnf install failed: $pkg"; FAILED+=("dnf:$pkg")
    fi
  done
}

install_yum() {
  require_cmd yum || return 1
  local to_install=()
  for pkg in "${DNF_PKGS[@]}"; do
    if [ "$FORCE" = false ] && rpm -q "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("yum:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "yum: all packages already installed"; return 0
  fi
  log_info "yum: ${#to_install[@]} packages to install"
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  for pkg in "${to_install[@]}"; do
    if run_as_root yum install -y "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("yum:$pkg")
    else
      log_warn "yum install failed: $pkg"; FAILED+=("yum:$pkg")
    fi
  done
}

install_apk() {
  require_cmd apk || return 1
  local to_install=()
  for pkg in "${APK_PKGS[@]}"; do
    if [ "$FORCE" = false ] && apk info -e "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("apk:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "apk: all packages already installed"; return 0
  fi
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  for pkg in "${to_install[@]}"; do
    if run_as_root apk add --no-cache "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("apk:$pkg")
    else
      log_warn "apk install failed: $pkg"; FAILED+=("apk:$pkg")
    fi
  done
}

install_pacman() {
  require_cmd pacman || return 1
  local to_install=()
  for pkg in "${PACMAN_PKGS[@]}"; do
    if [ "$FORCE" = false ] && pacman -Qi "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("pacman:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "pacman: all packages already installed"; return 0
  fi
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  if run_as_root pacman -Sy --noconfirm "${to_install[@]}" >>"$LOG_FILE" 2>&1; then
    for pkg in "${to_install[@]}"; do SUCCEEDED+=("pacman:$pkg"); done
  else
    log_warn "pacman install failed"; FAILED+=("pacman:batch")
  fi
}

install_zypper() {
  require_cmd zypper || return 1
  local to_install=()
  for pkg in "${ZYPPER_PKGS[@]}"; do
    if [ "$FORCE" = false ] && rpm -q "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("zypper:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "zypper: all packages already installed"; return 0
  fi
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  for pkg in "${to_install[@]}"; do
    if run_as_root zypper --non-interactive install "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("zypper:$pkg")
    else
      log_warn "zypper install failed: $pkg"; FAILED+=("zypper:$pkg")
    fi
  done
}

install_brew() {
  require_cmd brew || return 1
  local to_install=()
  for pkg in "${BREW_PKGS[@]}"; do
    if [ "$FORCE" = false ] && brew list --formula "$pkg" >/dev/null 2>&1; then
      SKIPPED+=("brew:$pkg"); continue
    fi
    to_install+=("$pkg")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "brew: all packages already installed"; return 0
  fi
  log_info "brew: ${#to_install[@]} packages to install"
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  for pkg in "${to_install[@]}"; do
    if brew install "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("brew:$pkg")
    else
      log_warn "brew install failed: $pkg"; FAILED+=("brew:$pkg")
    fi
  done
}

install_win() {
  # Native Windows only — never uses WSL.
  detect_platform
  if [ "$PLATFORM" != "windows-native" ]; then
    log_warn "win mode is for Windows native (Git Bash/MSYS/Cygwin); current=$PLATFORM"
    # Still try if winget/scoop/choco exist (cross shells)
    if ! have winget && ! have scoop && ! have choco; then
      SKIPPED+=("win:no-manager")
      return 0
    fi
  fi

  if have winget; then
    log_info "winget: installing packages"
    if [ "$DRY_RUN" = true ]; then
      for id in "${WINGET_PKGS[@]}"; do log_info "Would: winget install --id $id -e --accept-package-agreements --accept-source-agreements"; done
    else
      for id in "${WINGET_PKGS[@]}"; do
        if winget install --id "$id" -e --accept-package-agreements --accept-source-agreements >>"$LOG_FILE" 2>&1; then
          SUCCEEDED+=("winget:$id")
        else
          # Already installed often returns non-zero — treat as skip if present
          log_warn "winget install failed or already present: $id"
          SKIPPED+=("winget:$id")
        fi
      done
    fi
  else
    log_warn "winget not found"
    SKIPPED+=("winget:missing")
  fi

  if have scoop; then
    log_info "scoop: installing"
    for pkg in "${SCOOP_PKGS[@]}"; do
      if [ "$FORCE" = false ] && have "$pkg"; then SKIPPED+=("scoop:$pkg"); continue; fi
      if [ "$DRY_RUN" = true ]; then
        log_info "Would: scoop install $pkg"
      elif scoop install "$pkg" >>"$LOG_FILE" 2>&1; then
        SUCCEEDED+=("scoop:$pkg")
      else
        FAILED+=("scoop:$pkg")
      fi
    done
  fi

  if have choco; then
    log_info "choco: installing nmap jq curl"
    if [ "$DRY_RUN" = true ]; then
      log_info "Would: choco install -y nmap jq curl"
    elif choco install -y nmap jq curl >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("choco:batch")
    else
      FAILED+=("choco:batch")
    fi
  fi

  # PowerShell helper for native Windows (also installs via WinGet)
  local ps1
  ps1="$(dirname "$0")/install_bugbounty_tools.ps1"
  if [ -f "$ps1" ] && have powershell.exe; then
    log_info "Also running PowerShell installer for native Windows coverage"
    if [ "$DRY_RUN" = true ]; then
      log_info "Would: powershell -ExecutionPolicy Bypass -File $ps1 -DryRun"
    elif powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$ps1" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("powershell:installer")
    else
      log_warn "PowerShell installer returned non-zero"
      FAILED+=("powershell:installer")
    fi
  fi
}

# Dispatch "os" to the right family
install_os() {
  detect_platform
  case "$PKG_FAMILY" in
    apt) install_apt ;;
    dnf) install_dnf ;;
    yum) install_yum ;;
    apk) install_apk ;;
    pacman) install_pacman ;;
    zypper) install_zypper ;;
    brew) install_brew ;;
    winget|scoop|choco) install_win ;;
    none)
      if [ "$PLATFORM" = "windows-native" ]; then
        install_win
      else
        log_warn "No OS package manager detected on $PLATFORM — using Go/pip only"
        SKIPPED+=("os:none")
      fi
      ;;
    *)
      log_warn "Unknown package family: $PKG_FAMILY"
      SKIPPED+=("os:$PKG_FAMILY")
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Python (python3 or python — Windows often only has `python`)
# ---------------------------------------------------------------------------
PIP_PACKAGES=(
  "sqlmap:sqlmap"
  "arjun:arjun"
  "httpie:http"
  "wafw00f:wafw00f"
  "requests:requests"
  "semgrep:semgrep"
)

PY=""
pick_python() {
  if have python3; then PY="python3"
  elif have python; then PY="python"
  else PY=""
  fi
}

py_module_installed() {
  [ -n "$PY" ] || return 1
  "$PY" -c "import $1" 2>/dev/null
}

install_pip() {
  pick_python
  if [ -z "$PY" ]; then
    log_warn "python/python3 not found — skipping pip installs"
    if [ "$PLATFORM" = "windows-native" ]; then
      log_warn "Install Python: winget install Python.Python.3.12"
    fi
    SKIPPED+=("pip:all"); return 0
  fi

  local pip_flags=()
  # PEP 668 / EXTERNALLY-MANAGED
  if "$PY" -c "import sysconfig; marker = sysconfig.get_path('stdlib') + '/EXTERNALLY-MANAGED'; open(marker)" 2>/dev/null; then
    if [ -z "${VIRTUAL_ENV:-}" ]; then
      if [ "$DRY_RUN" = true ]; then
        log_info "PEP 668 — would create venv at $BB_VENV"
      else
        log_info "PEP 668 — creating venv at $BB_VENV"
        "$PY" -m venv "$BB_VENV" 2>>"${LOG_FILE:-/dev/null}" || pip_flags+=(--user)
        if [ -d "$BB_VENV" ] && [ -z "${pip_flags[*]:-}" ]; then
          if [ -f "$BB_VENV/bin/activate" ]; then
            # shellcheck disable=SC1091
            . "$BB_VENV/bin/activate"
          elif [ -f "$BB_VENV/Scripts/activate" ]; then
            # shellcheck disable=SC1091
            . "$BB_VENV/Scripts/activate"
          fi
          log_info "Activated $BB_VENV"
        fi
      fi
    fi
  fi

  # On Windows, prefer --user if not elevated and no venv
  if [ "$PLATFORM" = "windows-native" ] && [ -z "${VIRTUAL_ENV:-}" ]; then
    pip_flags+=(--user)
  fi

  local to_install=()
  for entry in "${PIP_PACKAGES[@]}"; do
    local name="${entry%%:*}"
    local mod="${entry#*:}"
    if [ "$FORCE" = false ] && py_module_installed "$mod"; then
      SKIPPED+=("pip:$name"); continue
    fi
    to_install+=("$name")
  done
  if [ ${#to_install[@]} -eq 0 ]; then
    log_info "pip: all packages already installed"; return 0
  fi
  if [ "$DRY_RUN" = true ]; then
    log_info "Would install: ${to_install[*]}"; return 0
  fi
  for pkg in "${to_install[@]}"; do
    if "$PY" -m pip install ${pip_flags[@]+"${pip_flags[@]}"} "$pkg" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("pip:$pkg")
    else
      log_warn "pip install failed: $pkg"; FAILED+=("pip:$pkg")
    fi
  done
}

# ---------------------------------------------------------------------------
# Nuclei templates
# ---------------------------------------------------------------------------
install_nuclei() {
  if ! have nuclei; then
    log_warn "nuclei not found — install go tools first"
    SKIPPED+=("nuclei:templates"); return 0
  fi
  if [ "$DRY_RUN" = true ]; then
    log_info "Would: nuclei -update-templates"; return 0
  fi
  log_info "Updating nuclei templates"
  if nuclei -update-templates >>"$LOG_FILE" 2>&1; then
    SUCCEEDED+=("nuclei:templates")
  else
    log_warn "nuclei -update-templates failed"; FAILED+=("nuclei:templates")
  fi
}

# ---------------------------------------------------------------------------
# Wordlists (curl or wget — cross-platform)
# ---------------------------------------------------------------------------
fetch_url() {
  local url="$1" dest="$2"
  if have curl; then
    curl -fsSL --max-time 60 "$url" -o "$dest"
  elif have wget; then
    wget -q -O "$dest" "$url"
  else
    return 127
  fi
}

install_wordlists() {
  mkdir -p "$WORDLIST_DIR"
  local urls=(
    "https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/Web-Content/common.txt|common.txt"
    "https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/Web-Content/api/api-endpoints.txt|api-endpoints.txt"
    "https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/DNS/subdomains-top1million-5000.txt|subdomains-top1m.txt"
    "https://raw.githubusercontent.com/danielmiessler/SecLists/master/Passwords/Common-Credentials/10k-most-common.txt|10k-most-common.txt"
  )
  for entry in "${urls[@]}"; do
    local url="${entry%%|*}"
    local name="${entry##*|}"
    local dest="${WORDLIST_DIR}/${name}"
    if [ "$FORCE" = false ] && [ -s "$dest" ]; then
      SKIPPED+=("wl:$name"); continue
    fi
    if [ "$DRY_RUN" = true ]; then
      log_info "Would fetch $name"; continue
    fi
    if fetch_url "$url" "$dest" >>"$LOG_FILE" 2>&1; then
      SUCCEEDED+=("wl:$name"); log_info "wordlist: $name"
    else
      log_warn "wordlist fetch failed: $name"; FAILED+=("wl:$name")
    fi
  done
}

print_manual() {
  cat <<'EOF'
Manual / platform-specific installs:

  Debian/Ubuntu/Kali:
    sudo apt update && sudo apt install -y nmap sqlmap ffuf gobuster nikto jq
  RHEL/CentOS/Rocky/Alma/Fedora:
    sudo dnf install -y nmap sqlmap jq          # or: sudo yum install -y ...
    # ffuf/gobuster via Go (often not in EPEL)
  Alpine:
    sudo apk add nmap sqlmap jq
  Arch:
    sudo pacman -S nmap sqlmap jq
  openSUSE:
    sudo zypper install nmap sqlmap jq
  macOS:
    brew install nmap sqlmap jq
  Windows (native — no WSL required):
    winget install Insecure.Nmap
    winget install GoLang.Go
    winget install Python.Python.3.12
    # or: powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1

  WSL (ONLY if installed AND a distro is Running):
    wsl -l -v     # must show a distro with State: Running
    # then inside that distro use apt/dnf as appropriate

  Cross-platform Go tools:
    go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
    go install github.com/projectdiscovery/httpx/cmd/httpx@latest
    go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest
    go install github.com/ffuf/ffuf/v2@latest
EOF
}

verify() {
  detect_platform
  local missing=() found=()
  local -a checks=(
    subfinder httpx nuclei katana naabu dnsx assetfinder gau waybackurls
    anew ffuf gobuster dalfox arjun qsreplace gf jq nmap sqlmap nikto
    curl wget whatweb wafw00f semgrep trufflehog gitleaks go
  )
  # python3 vs python
  pick_python
  if [ -n "$PY" ]; then found+=("$PY"); else missing+=("python"); fi

  log_info "Platform: $PLATFORM (pkg=$PKG_FAMILY)"
  log_info "Verifying bug-bounty tools"
  for cmd in "${checks[@]}"; do
    if have "$cmd"; then found+=("$cmd"); else missing+=("$cmd"); fi
  done

  local wsl
  wsl="$(wsl_status)"
  echo ""
  echo "Platform: $PLATFORM | Pkg: $PKG_FAMILY | WSL: $wsl"
  echo "Found: ${#found[@]}"
  echo "Missing: ${#missing[@]}"
  if [ ${#missing[@]} -gt 0 ]; then
    echo "Missing:"
    for m in "${missing[@]}"; do echo "  - $m"; done
    echo ""
    echo "Install: bash scripts/install_bugbounty_tools.sh all"
    if [ "$PLATFORM" = "windows-native" ]; then
      echo "Windows: powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1"
    fi
    if [ "$wsl" != "ready" ]; then
      echo "WSL not ready ($wsl) — not using WSL. Prefer native installers."
    fi
  fi
}

print_summary() {
  echo ""
  echo "========================================"
  echo " Bug-bounty Install Summary"
  echo "========================================"
  echo " Platform:  $PLATFORM"
  echo " Pkg family:$PKG_FAMILY"
  echo " Installed: ${#SUCCEEDED[@]}"
  echo " Skipped:   ${#SKIPPED[@]} (already present / N/A)"
  echo " Failed:    ${#FAILED[@]}"
  if [ ${#FAILED[@]} -gt 0 ]; then
    echo " Failed:"
    for f in "${FAILED[@]}"; do echo "   - $f"; done
  fi
  echo "========================================"
  [ -n "${LOG_FILE:-}" ] && echo " Log: $LOG_FILE"
}

if [ "$DRY_RUN" = false ] && [ "$MODE" != "--verify" ] && [ "$MODE" != "manual" ] && [ "$MODE" != "--platform" ]; then
  setup_logging
fi

detect_platform

case "$MODE" in
  --platform) print_platform ;;
  go) install_go; print_summary ;;
  os) install_os; print_summary ;;
  apt) install_apt; print_summary ;;
  dnf) install_dnf; print_summary ;;
  yum) install_yum; print_summary ;;
  apk) install_apk; print_summary ;;
  pacman) install_pacman; print_summary ;;
  zypper) install_zypper; print_summary ;;
  brew) install_brew; print_summary ;;
  win) install_win; print_summary ;;
  pip) install_pip; print_summary ;;
  nuclei) install_nuclei; print_summary ;;
  wordlists) install_wordlists; print_summary ;;
  manual) print_manual ;;
  --verify) verify ;;
  all)
    install_go
    install_os
    install_pip
    install_nuclei
    install_wordlists
    print_manual
    print_summary
    ;;
  *)
    log_error "Unknown mode: $MODE"
    echo "Usage: $0 [--dry-run] [--force] {go|os|apt|dnf|yum|apk|pacman|zypper|brew|win|pip|nuclei|wordlists|manual|all|--verify|--platform}" >&2
    exit 2
    ;;
esac

if [ ${#FAILED[@]} -gt 0 ]; then
  exit 1
fi
