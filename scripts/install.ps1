#!/usr/bin/env pwsh
# install.ps1 -- PowerShell installer for Alphacode (Windows + cross-platform pwsh)
#
# Usage:
#   iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
#   iwr -useb ... | iex -Version v1.0.0
#   iwr -useb ... | iex -Prefix "$env:LOCALAPPDATA\Programs\alphacode"
#   iwr -useb ... | iex -FromSource                  # skip release, build locally
#   iwr -useb ... | iex -SourceRef main               # build from a specific ref
#   iwr -useb ... | iex -AddPath                      # add the bin dir to the user PATH
#   iwr -useb ... | iex -AddPath -PathDryRun          # show what -AddPath would do, write nothing
#
# By default, tries to download a prebuilt release asset. If no release is
# published (or there is no asset for this OS/arch), it falls back to
# building from source. Requires: git, cargo, rustc >= 1.91.
#
# PATH handling (-AddPath) mirrors what install.sh --add-path does on Unix, so
# the two installers offer the same capability. It is deliberately narrow:
#   * it writes ONLY HKCU\Environment\Path. The machine-wide PATH is never
#     touched (that needs admin and would affect every user on the box);
#   * it never rewrites $env:PATH. The session PATH is the merge of user +
#     machine + anything the session added; rebuilding it from the registry
#     would silently drop those session-only entries for this shell and for
#     anything it spawns. The already-running shell is left alone on purpose --
#     it keeps working, and a new shell picks the value up;
#   * it appends rather than prepends, so no existing entry changes precedence;
#   * it is idempotent, and refuses to run twice;
#   * it preserves the registry value *kind*, so a PATH using %USERPROFILE%
#     style references stays expandable (see Add-AlphacodeToUserPath).

[CmdletBinding()]
param(
  [string]$Version   = $env:ALPHACODE_VERSION,
  [string]$Repo      = ($env:ALPHACODE_REPO -as [string]),
  [string]$Prefix    = $env:ALPHACODE_PREFIX,
  [string]$BinDir    = $env:ALPHACODE_BIN_DIR,
  [switch]$NoPath,
  # Retained as a no-op compatibility alias: PATH is now applied by default, so
  # older instructions and wrappers that still pass -AddPath keep working.
  [switch]$AddPath,
  [switch]$PathDryRun,
  [switch]$FromSource,
  [switch]$SourceOnly,
  [string]$SourceRef = $env:ALPHACODE_SOURCE_REF
)

$ErrorActionPreference = 'Stop'

# Is this Windows?
#
# Deliberately NOT `$IsWindows`. That variable only exists in PowerShell 6+,
# and the documented invocation for this script is `iwr ... | iex`, which on a
# default Windows machine runs Windows PowerShell 5.1 -- where `$IsWindows` is
# undefined and evaluates to $null. Testing it there is always false, so the
# Windows branch below was dead code on the very shell most users run, and the
# install directory silently became "$HOME/.local" instead of LOCALAPPDATA.
# [Environment]::OSVersion.Platform exists in every edition and needs no
# PowerShell version.
function Test-AlphacodeWindows {
    [CmdletBinding()]
    param()
    return ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
}

# True when this script was dot-sourced or `iex`-evaluated into the caller's
# session, rather than run as a file via `-File` / `& script.ps1`.
#
# The distinction decides what a failure is allowed to do. When the script owns
# the process (`pwsh -File install.ps1`) a non-zero `exit` is correct and is
# what CI wants. When it has been evaluated into someone else's session
# (`iwr ... | iex`, the documented one-liner) `exit` tears down that user's
# terminal -- closing windows, discarding unsaved scrollback and half-typed
# input. So in that case the installer reports the failure and returns.
#
# `$PSCommandPath` is the reliable signal here. Verified on Windows PowerShell
# 5.1: it holds the script path when the file is run (`-File`, `& script.ps1`,
# or `. script.ps1`) and is EMPTY when the text arrives via `iex`/piped stdin.
# `$MyInvocation.ScriptName` is NOT usable for this: at the top level of a
# `-File` script it is already empty, so a check built on it reports "hosted"
# for the case where exiting is perfectly safe -- and would let a failed
# `-File` install return 0.
function Test-AlphacodeHost {
    [CmdletBinding()]
    param()
    return [string]::IsNullOrEmpty($PSCommandPath)
}

if (-not $Repo)    { $Repo    = 'dragonked2/alphacode' }
if (-not $Version) { $Version = 'latest' }
if (-not $Prefix)  {
  # `%LOCALAPPDATA%\Programs\<app>` is the per-user equivalent of
  # `C:\Program Files`, and is what Git / VS Code / ripgrep use. The old
  # default was `%LOCALAPPDATA%\alphacode`, which had three problems:
  #   * it disagreed with the install location the README documents;
  #   * it put `bin` directly under the app's own *data* directory, so
  #     uninstall.ps1's "remove %LOCALAPPDATA%\alphacode" also deleted the
  #     installed binary alongside any saved sessions/auth;
  #   * a data dir simply named `alphacode` is easy to confuse with the
  #     %APPDATA% config directory.
  # Programs\ keeps code and user data separate.
  if (Test-AlphacodeWindows) {
    $localAppData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { $env:USERPROFILE }
    $Prefix = Join-Path $localAppData 'Programs\alphacode'
  }
  else { $Prefix = "$HOME/.local" }
}
if (-not $BinDir)  { $BinDir = Join-Path $Prefix 'bin' }

# Fail aborts the install by THROWING, never by calling `exit`.
#
# This script's documented invocation is `iwr ... | iex`, which evaluates the
# text in the caller's own session. `exit` in that context does not end the
# installer -- it ends the user's *terminal*: every unsaved buffer, half-typed
# command and running pipeline in that window is destroyed, and the shell
# closes with no prompt. A single failed download (offline, proxy, rate limit)
# or a checksum mismatch was enough to trigger it.
#
# `throw` gives the intended behaviour in both invocation styles:
#   * `pwsh -File scripts/install.ps1` -> unhandled terminating error, exit 1
#   * `iwr ... | iex`                   -> red error, session survives, and the
#                                         failure is still visible to the user
#
# A distinct exception type is what lets the catch blocks that legitimately
# tolerate *network* errors rethrow an *integrity* failure instead of silently
# downgrading it (see the checksum block).
class AlphacodeInstallFailure : System.Exception {
    # Deliberately contains no methods and no constructors of its own.
    #
    # Windows PowerShell 5.1 -- the shell `iwr ... | iex` runs on a stock
    # Windows machine -- cannot parse ANY method or constructor declared
    # inside a PowerShell class. Each of these was measured as a parse error
    # there:
    #     T([string]$d) : base($d) {}      # the "obvious" form
    #     T([string]$d) { }                # still a parse error
    #     static [T] Create([string]$d) {} # static methods too
    # A parse error anywhere in the file aborts the whole script, so all of
    # it has to live outside the class body.
    #
    # With only the implicit parameterless constructor available,
    # `[T]::new($msg)` fails at runtime ("Cannot find an overload for new and
    # the argument count: 1") and System.Exception.Message is read-only, so
    # the text is carried in this own field instead.
    [string]$Detail = ''
}

# Builds the typed exception. Outside the class because of the parse
# constraints documented above.
function New-AlphacodeInstallFailure {
    param([Parameter(Mandatory)][string]$Message)
    $e = [AlphacodeInstallFailure]::new()
    $e.Detail = $Message
    return $e
}

function Print([string]$msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Warn ([string]$msg) { Write-Host "[warn] $msg" -ForegroundColor Yellow }
function Fail ([string]$msg) { throw (New-AlphacodeInstallFailure -Message $msg) }


# --- user PATH ---------------------------------------------------------------
#
# Split into a pure planner and an effectful applier on purpose. The planner
# takes the current user PATH as a *string argument* and touches nothing, so it
# can be exercised over a matrix of shapes (empty, trailing separators, case
# differences, %VAR% references, sibling directories) without a registry, and
# without any risk of the test itself mutating the machine it runs on. That is
# what scripts/tests/install-path.tests.ps1 drives.

# Normalise a PATH entry for comparison: trimmed, and without trailing
# separators. Windows treats `C:\a\bin` and `C:\a\bin\` as the same directory,
# so without this every re-run would append a duplicate.
function ConvertTo-AlphacodeComparablePathEntry([string]$Entry) {
    if ($null -eq $Entry) { return '' }
    return $Entry.Trim().TrimEnd('\', '/')
}

# Decide what the user PATH should become. Pure: no registry, no environment
# mutation, no I/O.
#
# Appends rather than prepends. Appending is the only option that provably
# cannot change which executable wins for any pre-existing entry: nothing moves,
# so nothing that used to resolve to X can start resolving to Y. Prepending
# would be defensible for the same directory twice, but it reorders PATH, and
# PATH order decides which `git`/`python`/`node` a script gets.
function Get-AlphacodePathPlan {
    [CmdletBinding()]
    param(
        [AllowNull()][AllowEmptyString()][string]$UserPath,
        [Parameter(Mandatory)][string]$BinDir
    )

    $target = ConvertTo-AlphacodeComparablePathEntry $BinDir
    if ([string]::IsNullOrWhiteSpace($target)) {
        throw 'BinDir must not be empty.'
    }

    # Split, drop empty segments (a doubled ';;' from a hand-edited PATH), but
    # keep each entry's original text so nothing is rewritten on the way out.
    $entries = @()
    if ($null -ne $UserPath) {
        $entries = @($UserPath -split ';' | Where-Object { $_ -ne '' })
    }

    foreach ($entry in $entries) {
        if ((ConvertTo-AlphacodeComparablePathEntry $entry) -ieq $target) {
            # Already there. Return the value untouched rather than a rebuilt
            # one, so a re-run cannot normalise or reorder the user's PATH.
            return [pscustomobject]@{
                AlreadyPresent = $true
                Original      = $UserPath
                Value         = $UserPath
            }
        }
    }

    $value = if ($entries.Count -eq 0) { $target } else { ($entries -join ';') + ';' + $target }
    return [pscustomobject]@{
        AlreadyPresent = $false
        Original      = $UserPath
        Value         = $value
    }
}

# Tell already-running GUI processes (Explorer, the taskbar, anything that
# caches the environment) that `Environment` changed, so a *new* terminal picks
# the value up without a sign-out. This is the same broadcast `setx` and
# Chocolatey send; without it the write silently succeeds and appears to do
# nothing until the next reboot, which is the usual "the installer lied to me"
# report.
#
# Failure here is not fatal: the registry value is already committed, and the
# user can still open a new shell. Best-effort by design.
function Publish-AlphacodeEnvironmentChange {
    [CmdletBinding()]
    param()

    try {
        if (-not ('AlphacodeInstaller.Native' -as [type])) {
            Add-Type -Namespace 'AlphacodeInstaller' -Name 'Native' -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(
    IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam,
    uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
        }
        # SMTO_ABORTIFHUNG = 0x0002. A hung Explorer would otherwise make the
        # installer block for the whole timeout.
        $result = [UIntPtr]::Zero
        [void][AlphacodeInstaller.Native]::SendMessageTimeout(
            [IntPtr]0xffff,   # HWND_BROADCAST
            0x001A,           # WM_SETTINGCHANGE
            [UIntPtr]::Zero,
            'Environment',
            0x0002,           # SMTO_ABORTIFHUNG
            5000,
            [ref]$result)
    } catch {
        Write-Host "[warn] Could not broadcast the environment change: $($_.Exception.Message)" -ForegroundColor Yellow
        Write-Host "[warn] Open a new terminal for the PATH change to take effect." -ForegroundColor Yellow
    }
}

# Read HKCU\Environment\Path *raw*.
#
# Two things matter here and both are reasons not to use
# [Environment]::GetEnvironmentVariable('Path','User'):
#   1. That call expands REG_EXPAND_SZ, so a PATH containing `%USERPROFILE%\bin`
#      comes back already substituted. Writing it straight back would freeze
#      today's profile path into the registry forever -- a silent, permanent
#      regression for anyone whose user directory is renamed or whose install
#      is copied to another machine.
#   2. It merges nothing, but it also hides the value *kind*, which is needed to
#      write it back correctly (below).
function Get-AlphacodeRawUserPath {
    [CmdletBinding()]
    param()

    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    if ($null -eq $key) { return [pscustomobject]@{ Exists = $false; Value = $null; Kind = [Microsoft.Win32.RegistryValueKind]::String } }
    try {
        $exists = $key.GetValueNames() -contains 'Path'
        $kind = if ($exists) { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::String }
        $raw = if ($exists) {
            $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        } else { $null }
        return [pscustomobject]@{ Exists = $exists; Value = $raw; Kind = $kind }
    } finally {
        $key.Close()
    }
}

# Append the bin dir to the *user* PATH, idempotently. Returns a result object
# describing what happened so the caller can report accurately.
function Add-AlphacodeToUserPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$BinDir,
        [switch]$DryRun
    )

    if (-not (Test-AlphacodeWindows)) {
        Write-Host "[warn] -AddPath only applies on Windows; on other platforms use your shell profile." -ForegroundColor Yellow
        return [pscustomobject]@{ Status = 'skipped'; Reason = 'not-windows' }
    }

    $current = Get-AlphacodeRawUserPath
    $plan = Get-AlphacodePathPlan -UserPath $current.Value -BinDir $BinDir

    if ($plan.AlreadyPresent) {
        Print "Already on your user PATH: $BinDir"
        return [pscustomobject]@{ Status = 'already-present'; Value = $plan.Value }
    }

    # REG_EXPAND_SZ values are limited to 32767 characters. Silently truncating
    # here would produce a PATH that is missing whichever entries fell off the
    # end, so refuse instead and let the user decide.
    if ($plan.Value.Length -gt 32767) {
        Fail "Adding '$BinDir' would make your user PATH $($plan.Value.Length) characters, over the Windows limit of 32767. Not writing it -- trim your user PATH and re-run."
    }

    if ($DryRun) {
        Write-Host "[dry-run] HKCU\Environment\Path would be set to:"
        Write-Host "[dry-run]   $($plan.Value)"
        Write-Host "[dry-run] (nothing was written; re-run without -PathDryRun to apply)"
        return [pscustomobject]@{ Status = 'dry-run'; Value = $plan.Value }
    }

    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    if ($null -eq $key) {
        Warn 'Could not open HKCU\Environment for writing; PATH not modified.'
        return [pscustomobject]@{ Status = 'failed'; Reason = 'key-unavailable' }
    }
    try {
        # Write back with the value kind we read. Defaulting to String here would
        # convert an ExpandString PATH to a literal one and break every %VAR%
        # reference in it.
        $kind = if ($current.Kind -eq [Microsoft.Win32.RegistryValueKind]::Unknown) {
            [Microsoft.Win32.RegistryValueKind]::ExpandString
        } else { $current.Kind }
        $key.SetValue('Path', $plan.Value, $kind)
    } finally {
        $key.Close()
    }

    Publish-AlphacodeEnvironmentChange
    Print "Added to your user PATH: $BinDir"
    Write-Host "    Open a new terminal to pick it up (this one is unchanged on purpose)."
    return [pscustomobject]@{ Status = 'added'; Value = $plan.Value }
}

# Make the freshly installed binary usable in THIS shell, right now.
#
# The registry write alone does not help the shell the user is standing in,
# which is the actual complaint behind "the installer did not add alphacode to
# PATH": they run the one-liner, then type `alphacode`, and get
# "not recognized". A new terminal would fix it, but they do not have another
# terminal open -- that is the whole point of a one-line installer.
#
# This PREPENDS to $env:PATH for the current process only. It is strictly a
# session-local convenience:
#   * it is never persisted, so nothing about the user's stored environment is
#     rewritten (the deliberate choice documented on Add-AlphacodeToUserPath);
#   * it only affects this process and the children it spawns, so it cannot
#     change any other terminal;
#   * it is prepended, so `alphacode` resolves even if another `alphacode`
#     earlier in the session PATH would otherwise win.
#
# Prepending to the *session* is safe where prepending to the *stored user*
# PATH would not be: the stored value is what every future shell inherits, so
# reordering it changes which `git`/`python` wins for all of them.
function Add-AlphacodeToSessionPath {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$BinDir)

    $target = ConvertTo-AlphacodeComparablePathEntry $BinDir
    $sep = [System.IO.Path]::PathSeparator
    $entries = @($env:PATH -split [regex]::Escape($sep) | Where-Object { $_ -ne '' })
    $already = $false
    foreach ($e in $entries) {
        if ((ConvertTo-AlphacodeComparablePathEntry $e) -ieq $target) { $already = $true; break }
    }
    if ($already) { return [pscustomobject]@{ Status = 'already-present' } }

    $env:PATH = "$BinDir$sep" + $env:PATH
    return [pscustomobject]@{ Status = 'added' }
}

# --- build_from_source -------------------------------------------------------
#
# Fallback: no release artifact for this platform/arch. Clone the repo, build
# with cargo, and copy the resulting binary into $BinDir.
#
# Requires: git, cargo, rustc >= 1.91, and a working C toolchain. This can
# take 5-30 minutes on a first build.
function Build-FromSource {
  if (-not (Get-Command git   -ErrorAction SilentlyContinue)) { Fail "git is required to build from source" }
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail "cargo is required to build from source (install Rust from https://rustup.rs)" }

  # rustc >= 1.91 (edition 2024 + current dependency MSRV) check.
  $rv = (& rustc --version) 2>$null
  if ($rv -match 'rustc\s+(\d+)\.(\d+)') {
    $major = [int]$Matches[1]; $minor = [int]$Matches[2]
    if ($major -lt 1 -or ($major -eq 1 -and $minor -lt 91)) {
      Fail "rustc $($Matches[0]) is too old; need >= 1.91 (update via 'rustup update')"
    }
  }

  $srcDir = Join-Path ([System.IO.Path]::GetTempPath()) ("alphacode-src-" + [System.Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $srcDir | Out-Null

  try {
    Print "Cloning $Repo into a temporary build directory ..."
    $cloneUrl = "https://github.com/$Repo.git"
    if ($SourceRef) {
      & git clone --depth 1 --branch $SourceRef $cloneUrl "$srcDir\src" | Out-Null
      if ($LASTEXITCODE -ne 0) { Fail "git clone failed (ref: $SourceRef)" }
    } else {
      & git clone --depth 1 $cloneUrl "$srcDir\src" | Out-Null
      if ($LASTEXITCODE -ne 0) { Fail "git clone failed" }
    }

    Print "Compiling alphacode (this can take 5-30 minutes on a first build) ..."
    # NOTE: no --locked on purpose. The committed Cargo.lock does not list
    # platform-conditional deps for every target triple, and CI itself runs
    # without `--locked` (see .github/workflows/release.yml: `locked: false`).
    # If we passed --locked here, a fresh source build on a platform the
    # lockfile wasn't regenerated for would fail with
    # "Cargo.lock needs to be updated".
    & cargo build --release --manifest-path "$srcDir\src\Cargo.toml"
    if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }

    $builtExe = Join-Path "$srcDir\src\target\release" 'alphacode.exe'
    if (-not (Test-Path $builtExe)) {
      Fail "build succeeded but target\release\alphacode.exe was not produced"
    }

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    $destExe = Join-Path $BinDir 'alphacode.exe'
    Copy-Item -Path $builtExe -Destination $destExe -Force
    Print "Installed -> $destExe (built from source)"
  } finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $srcDir
  }
}

# --- Installer entry point -------------------------------------------------------
#
# The body below is wrapped in a function and invoked through a handler so that a
# failure ends the *installer* without ending the user's shell. See the note on
# `Fail` above for why `exit` was removed: under `iwr ... | iex` it closes the
# user's terminal. The early-out paths use `return` for the same reason -- the
# old `return`s at the top level of an `iex`-evaluated script did not stop the
# script at all, they merely ended the enclosing scope.
function Invoke-AlphacodeInstall {
  # --- Architecture ------------------------------------------------------------

  # Map a Windows architecture token onto the Rust target suffix used by the
  # release asset names.
  #
  # Three inputs are consulted, ordered by how reliably each describes the
  # *machine* rather than the shell that happens to be running:
  #
  #   1. PROCESSOR_ARCHITEW6432 -- set by Windows only when a 32-bit process
  #      runs under 64-bit OS emulation (WOW64). Its presence means "the real OS
  #      is 64-bit" and its value is the true machine architecture. The old code
  #      read only PROCESSOR_ARCHITECTURE, so a 32-bit PowerShell on x64 Windows
  #      saw "x86" and died with "unsupported architecture: x86" even though a
  #      perfectly good x86_64 release was sitting in the same release.
  #   2. PROCESSOR_ARCHITECTURE_WINDOWS -- set on ARM64 Windows when an x64
  #      process is emulated; it reports the *OS* architecture. Without it an
  #      x64-emulated shell on ARM64 silently downloads the x86_64 build.
  #   3. PROCESSOR_ARCHITECTURE -- the fallback, and the right answer in the
  #      common 64-bit case.
  #
  # Pure function, no I/O and no reads of the real environment, so the test
  # suite can drive the whole matrix without touching the machine it runs on
  # (see scripts/tests/install-path.tests.ps1).
  function Get-AlphacodeWindowsArch {
      [CmdletBinding()]
      param(
          [AllowNull()][AllowEmptyString()][string]$NativeArch,
          [AllowNull()][AllowEmptyString()][string]$Wow64Arch,
          [AllowNull()][AllowEmptyString()][string]$OsArch
      )

      # An unset variable arrives as empty; fall back to the native token.
      if (-not $Wow64Arch) { $Wow64Arch = $NativeArch }
      if (-not $OsArch)   { $OsArch = $NativeArch }

      foreach ($token in @($OsArch, $Wow64Arch, $NativeArch)) {
          if ([string]::IsNullOrWhiteSpace($token)) { continue }
          switch ($token.Trim().ToUpperInvariant()) {
              'ARM64' { return 'arm64' }
              'AMD64' { return 'x86_64' }
          }
      }
      return $null
  }

  $Arch = Get-AlphacodeWindowsArch `
      -NativeArch $env:PROCESSOR_ARCHITECTURE `
      -Wow64Arch  $env:PROCESSOR_ARCHITEW6432 `
      -OsArch     $env:PROCESSOR_ARCHITECTURE_WINDOWS

  # Platform gate.
  #
  # Uses the Test-AlphacodeWindows helper instead of `$IsWindows -or $env:OS`.
  # `$IsWindows` exists only in PowerShell 6+; the documented invocation is
  # `iwr ... | iex`, which on a stock Windows machine runs Windows PowerShell 5.1,
  # where that variable is undefined and evaluates to $null -- exactly the
  # PS6-only assumption this script already documents avoiding at the top for
  # the prefix default. It survived here only because $env:OS happened to be
  # "Windows_NT", so the real detection was silently delegated to a string
  # compare on an env var that is absent from cleared/hardened environments.
  if (-not (Test-AlphacodeWindows)) {
    Fail "this script is for Windows. On Linux/macOS use scripts/install.sh."
  }

  if (-not $Arch) {
    Fail ("unsupported architecture " +
          "(PROCESSOR_ARCHITECTURE='$env:PROCESSOR_ARCHITECTURE' " +
          "PROCESSOR_ARCHITEW6432='$env:PROCESSOR_ARCHITEW6432' " +
          "PROCESSOR_ARCHITECTURE_WINDOWS='$env:PROCESSOR_ARCHITECTURE_WINDOWS'). " +
          "Re-run with `$env:ALPHACODE_ARCH='x86_64' to force the 64-bit build, " +
          "or download the asset manually with -Prefix <dir>.")
  }

  $Platform = 'windows'
  $asset = "alphacode-windows-$Arch.zip"

  # Escape hatch for the one case detection cannot settle: an x64 process
  # emulated on ARM64 Windows. Both builds are published and only the user knows
  # which one they want.
  if ($env:ALPHACODE_ARCH) {
    $forced = $env:ALPHACODE_ARCH.Trim().ToLowerInvariant()
    if ($forced -eq 'x86_64' -or $forced -eq 'arm64') {
      $Arch = $forced
      $asset = "alphacode-windows-$Arch.zip"
      Print "Architecture forced to $Arch via ALPHACODE_ARCH."
    }
  }

  # --- Version -----------------------------------------------------------------

  # Short-circuit: build from source only.
  if ($FromSource) {
    Print '[FromSource] requested, skipping release download.'
    Build-FromSource
    return
  }

  if ($Version -eq 'latest') {
    Print "Resolving latest release from $Repo ..."
    $rel = $null
    try {
      $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    } catch {
      $rel = $null
    }
    if (-not $rel -or -not $rel.tag_name) {
      if ($SourceOnly) { Fail "no release found for $Repo and -SourceOnly is set" }
      Warn "no GitHub release found for $Repo -- falling back to building from source."
      Build-FromSource
      Print "Done."
      return
    }
    $Version = $rel.tag_name
    Print "Latest release: $Version"
  }
  $VersionNoV = $Version.TrimStart('v')

  # --- Download ----------------------------------------------------------------

  $Tmp      = [System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid().ToString('N')
  $ZipPath  = Join-Path $Tmp $asset
  $Extract  = Join-Path $Tmp 'extract'
  $Url      = "https://github.com/$Repo/releases/download/$Version/$asset"
  New-Item -ItemType Directory -Force -Path $Tmp,$Extract | Out-Null

  Print "Downloading $Url"
  try {
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
  } catch {
    if ($SourceOnly) { Fail "download failed: $($_.Exception.Message)" }
    Warn "no prebuilt asset for $Platform/$Arch at $Version -- falling back to building from source."
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Tmp
    Build-FromSource
    Print "Done."
    return
  }

  # --- Checksum verification ----------------------------------------------------
  #
  # Fetching SHA256SUMS may legitimately fail (offline, proxy, a release with
  # no manifest) and that must degrade to a warning. A digest that *disagrees*
  # is a different thing entirely: the bytes on disk are not the bytes the
  # project published, which is the entire reason the manifest exists.
  #
  # The previous shape conflated the two:
  #     try { ...; Fail "checksum mismatch" } catch { Warn "...continuing" }
  # `Fail` raised inside the try and the catch could not tell an integrity
  # failure from a network error, so it downgraded a tampered download into a
  # warning and then installed it anyway. The check was decorative.
  #
  # Here only the *fetch* is wrapped in try/catch; the comparison sits outside
  # it, so a mismatch is unconditionally fatal.
  $expectedHash = $null
  try {
    $sums = Invoke-WebRequest `
      -Uri "https://github.com/$Repo/releases/download/$Version/SHA256SUMS" `
      -UseBasicParsing -ErrorAction Stop
    # Match the filename *field* (sha256sum format: "<digest>  <name>", with an
    # optional leading "*" in binary mode) rather than doing a substring test,
    # so a similarly-named asset cannot supply the wrong digest.
    $line = ($sums.Content -split "`n" | Where-Object {
        $parts = @($_ -split '\s+' | Where-Object { $_ -ne '' })
        if ($parts.Count -lt 2) { $false } else { (($parts[1]) -replace '^\*', '') -eq $asset }
      } | Select-Object -First 1)
    if ($line) {
      $expectedHash = (@($line -split '\s+' | Where-Object { $_ -ne '' })[0]).ToLowerInvariant()
    }
  } catch {
    Warn "could not fetch SHA256SUMS (offline or proxy?); continuing unverified"
  }

  if ($expectedHash) {
    $actualHash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expectedHash -ne $actualHash) {
      Fail ("checksum mismatch for $asset -- the download does not match the " +
            "published release and has NOT been installed." +
            "`n  expected $expectedHash" +
            "`n  actual   $actualHash" +
            "`nRe-run the installer. If it persists, the download was corrupted " +
            "or tampered with -- do not run this file.")
    }
    Print "Checksum verified."
  } else {
    Warn "no published checksum found for $asset -- this download is unverified"
  }

  # --- Extract -----------------------------------------------------------------

  Print "Extracting ..."
  try {
    Expand-Archive -Path $ZipPath -DestinationPath $Extract -Force
  } catch {
    Fail "extract failed: $($_.Exception.Message)"
  }

  $binary = Get-ChildItem -Path $Extract -Recurse -Filter 'alphacode.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $binary) {
    Fail "extracted archive did not contain 'alphacode.exe'"
  }

  # --- Install -----------------------------------------------------------------

  New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
  $installedExe = Join-Path $BinDir 'alphacode.exe'
  Copy-Item -Path $binary.FullName -Destination $installedExe -Force

  # Also copy .bin payload files if present (release wrapper scripts need them).
  $payloadFiles = Get-ChildItem -Path $Extract -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue
  foreach ($pf in $payloadFiles) {
      $destBin = Join-Path $BinDir $pf.Name
      Copy-Item -Path $pf.FullName -Destination $destBin -Force
  }

  Print "Installed -> $installedExe"

  # Verify the installed binary works. Use --version (handled by clap before
  # any application logic) so the check succeeds even if a re-exec path would
  # otherwise interfere with subcommand parsing.
  # Brief pause: Windows SmartScreen / Defender may need a moment to allow a
  # freshly-copied executable to run.
  Start-Sleep -Milliseconds 500
  try {
    $proc = Start-Process -FilePath "$BinDir\alphacode.exe" -ArgumentList '--version' `
      -NoNewWindow -Wait -PassThru -RedirectStandardOutput "$Tmp\version_stdout.txt" `
      -RedirectStandardError "$Tmp\version_stderr.txt"
    $exitCode = $proc.ExitCode
    $stdout = if (Test-Path "$Tmp\version_stdout.txt") { Get-Content "$Tmp\version_stdout.txt" -Raw } else { '' }
    $stderr = if (Test-Path "$Tmp\version_stderr.txt") { Get-Content "$Tmp\version_stderr.txt" -Raw } else { '' }
    if ($exitCode -eq 0 -and $stdout) {
      $versionLine = ($stdout -split "`n" | Where-Object { $_ -match 'alphacode\s+v[\d.]+' } | Select-Object -First 1)
      if ($versionLine) {
        $version = ($versionLine -replace '.*alphacode\s+(v[\d.]+).*','$1')
        Print "Installed version: $version"
      } else {
        Print "Installed (could not parse version from: $stdout)"
      }
    } else {
      $detail = if ($stderr) { $stderr.Trim() } else { "exit code $exitCode" }
      Warn "Binary installed but could not verify version: $detail"
    }
  } catch {
    $excMsg = "$($_.Exception.Message)"
    if ([string]::IsNullOrWhiteSpace($excMsg)) {
      Write-Host '[warn] Binary installed but could not verify version: Start-Process failed (no exception detail; check file permissions and antivirus)' -ForegroundColor Yellow
    } else {
      Warn "Binary installed but could not verify version: $excMsg"
    }
  }

  # --- PATH ------------------------------------------------------------------
  #
  # The old flow required the user to know the `-AddPath` flag existed: the
  # default install printed "Next step: ... re-run with -AddPath", so a fresh
  # user's first experience of `iwr | iex` ended with alphacode NOT being on
  # PATH. That is the single largest piece of fresh-user friction, and the
  # thing this script exists to remove.
  #
  # Now the PATH entry is part of a working install by default. It stays
  # deliberately narrow (user scope only, appended, idempotent, REG_EXPAND_SZ
  # preserved, WM_SETTINGCHANGE broadcast) -- see Add-AlphacodeToUserPath --
  # and both prior behaviours remain available:
  #   -NoPath    opt out entirely
  #   -PathDryRun preview without writing
  $pathResult = $null
  if (-not $NoPath) {
    $pathResult = Add-AlphacodeToUserPath -BinDir $BinDir -DryRun:$PathDryRun
  }

  # Activate the binary in the CURRENT shell too, so `alphacode` works on the
  # very next line the user types. Skipped on a dry run, which writes nothing.
  $sessionOk = $false
  if (-not $PathDryRun) {
    try {
      [void](Add-AlphacodeToSessionPath -BinDir $BinDir)
      $sessionOk = $true
    } catch {
      Warn "could not update this shell's PATH: $($_.Exception.Message)"
    }
  }

  if ($pathResult -and $pathResult.Status -eq 'failed') {
    Write-Host ""
    Write-Host "Could not update your user PATH automatically." -ForegroundColor Yellow
    Write-Host "  Add it by hand in Settings > System > About > Advanced system"
    Write-Host "  settings > Environment Variables > user Path:"
    Write-Host "    $BinDir"
    if ($sessionOk) {
      Write-Host "  `alphacode` still works in THIS window; only new terminals need it."
    }
  } elseif ($sessionOk) {
    # Confirm the thing the user actually cares about: that `alphacode` runs.
    Write-Host ""
    Print "Ready to use -- \`alphacode\` is available in this window now."
  }

  if (-not $PathDryRun) {
    Write-Host ""
    Write-Host "Next:"
    Write-Host "  alphacode login    # connect a model provider"
    Write-Host "  alphacode          # start the TUI"
    Write-Host ""
    Write-Host "Not on PATH in a NEW terminal? Re-run with -AddPath, or add"
    Write-Host "  $BinDir"
    Write-Host "to your user Path (Settings > Environment Variables)."
  }

  Remove-Item -Recurse -Force $Tmp
  Print "Done."

}

# Invoke the installer, and report failure in the way that is correct for BOTH
# invocation styles:
#   * `pwsh -File scripts/install.ps1` -> non-zero exit code for CI/callers.
#   * `iwr ... | iex`                 -> red failure + $LASTEXITCODE, and the
#                                       session SURVIVES.
try {
  Invoke-AlphacodeInstall
}
catch {
  Write-Host "[fail] $($_.Exception.Message)" -ForegroundColor Red
  $global:LASTEXITCODE = 1
  if (-not (Test-AlphacodeHost)) {
    # Running as a script file: propagate the failing exit code.
    exit 1
  }
  # Running via `iex`: do NOT exit -- the caller's terminal must survive a
  # failed install. $LASTEXITCODE above is set for anyone checking it.
  return
}
