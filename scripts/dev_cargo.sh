#!/usr/bin/env bash
# Cargo entry point used by self-dev tests. On Windows it imports the MSVC
# environment without forcing callers through WSL or losing quoted Cargo args.
set -euo pipefail

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    vcvars='/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvars64.bat'
    if [[ ! -f "$vcvars" ]]; then
      for candidate in /c/Program\ Files/Microsoft\ Visual\ Studio/2022/*/VC/Auxiliary/Build/vcvars64.bat; do
        if [[ -f "$candidate" ]]; then
          vcvars="$candidate"
          break
        fi
      done
    fi
    if [[ ! -f "$vcvars" ]]; then
      printf 'MSVC setup not found. Install Visual Studio 2022 Build Tools with the C++ workload.\n' >&2
      exit 1
    fi

    env_file="$(mktemp "${TMPDIR:-/tmp}/alphacode-vcvars-env-XXXXXX")"
    cmd_file="$(mktemp "${TMPDIR:-/tmp}/alphacode-vcvars-XXXXXX.cmd")"
    trap 'rm -f "$env_file" "$cmd_file"' EXIT
    {
      printf '@echo off\r\n'
      printf 'call "%s" >nul 2>&1\r\n' "$(cygpath -w "$vcvars")"
      printf 'if errorlevel 1 exit /b %%errorlevel%%\r\n'
      printf 'set\r\n'
    } >"$cmd_file"
    MSYS_NO_PATHCONV=1 cmd.exe /D /S /C "$(cygpath -w "$cmd_file")" >"$env_file"

    while IFS='=' read -r key value; do
      value="${value%$'\r'}"
      [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
      if [[ "${key^^}" == PATH ]]; then
        value="$(cygpath --path -u "$value")"
      fi
      export "$key=$value"
    done <"$env_file"

    if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
      local_app_data="${LOCALAPPDATA:-${TMP:-C:\\Windows\\Temp}}"
      CARGO_TARGET_DIR="${local_app_data}\\Temp\\alphacode-target"
      export CARGO_TARGET_DIR
    fi
    ;;
esac

exec cargo "$@"
