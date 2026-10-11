#!/usr/bin/env pwsh
# install.ps1 -- PowerShell installer for AlphaCode (Windows)
#
# QUICK INSTALL (default settings):
#   irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
#
# INSTALL WITH OPTIONS
#   `iex` cannot take parameters, so either load the script as a script block:
#
#     $i = [scriptblock]::Create((irm https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1))
#     & $i -Version v1.0.0
#     & $i -Prefix "$env:LOCALAPPDATA\Programs\alphacode"
#     & $i -FromSource                 # skip the release, build locally
#     & $i -FromSource -SourceRef main # build a specific branch/tag
#     & $i -NoPath                     # do not touch PATH
#     & $i -PathDryRun                 # show the PATH change, write nothing
#     & $i -Scope Machine              # all users; needs an elevated PowerShell
#
#   ...or use environment variables, which work with the plain one-liner:
#
#     $env:ALPHACODE_VERSION = 'v1.0.0'; irm <url> | iex
#
#   Variables: ALPHACODE_VERSION, ALPHACODE_REPO, ALPHACODE_PREFIX,
#   ALPHACODE_BIN_DIR, ALPHACODE_SCOPE (User|Machine), ALPHACODE_SOURCE_REF,
#   ALPHACODE_ARCH (x86_64|arm64, forces the architecture).
#
# WHAT IT DOES
#   1. Downloads the prebuilt release for your CPU (x64 or ARM64) and verifies
#      its SHA-256 checksum. If there is no release or asset, it falls back to
#      building from source (needs git, cargo, rustc >= 1.91, a C toolchain).
#   2. Installs alphacode.exe (replacing a running copy safely when updating).
#   3. Puts the install directory on PATH so you never have to edit
#      Environment Variables by hand:
#        * persistent  : HKCU\Environment\Path (or HKLM with -Scope Machine);
#        * this window : the current session PATH, so `alphacode` works at once;
#        * running apps: broadcasts WM_SETTINGCHANGE so Explorer and new
#                        terminals pick the change up without signing out;
#        * verification: reads the registry back and launches a fresh process
#                        with the real registry PATH to prove that a NEW
#                        terminal will find this exact alphacode.exe, and warns
#                        if a different alphacode would win.
#
# PATH SAFETY RULES
#   * User scope writes ONLY HKCU\Environment\Path. Machine scope is opt-in and
#     requires administrator rights.
#   * The existing value is appended to, never reordered or rewritten, and the
#     registry value kind (REG_EXPAND_SZ vs REG_SZ) is preserved so %VAR%
#     references keep working.
#   * Idempotent: re-running never adds a duplicate, including when the existing
#     entry uses %LOCALAPPDATA% instead of the expanded path.
#   * A value longer than the 32767-character limit is refused, not truncated.

[CmdletBinding()]
param(
  [string]$Version   = $env:ALPHACODE_VERSION,
  [string]$Repo      = $env:ALPHACODE_REPO,
  [string]$Prefix    = $env:ALPHACODE_PREFIX,
  [string]$BinDir    = $env:ALPHACODE_BIN_DIR,
  [ValidateSet('User', 'Machine')]
  [string]$Scope     = $(if ($env:ALPHACODE_SCOPE) { $env:ALPHACODE_SCOPE } else { 'User' }),
  [switch]$NoPath,
  # No-op compatibility alias: PATH is applied by default, so older
  # instructions and wrappers that still pass -AddPath keep working.
  [switch]$AddPath,
  [switch]$PathDryRun,
  [switch]$FromSource,
  [switch]$SourceOnly,
  [string]$SourceRef = $env:ALPHACODE_SOURCE_REF
)

# NOTE: $ErrorActionPreference and $ProgressPreference are set INSIDE
# Invoke-AlphacodeInstall, never here. This file is normally evaluated with
# `iex`, which runs in the caller's own scope; setting them at the top level
# would leave the user's terminal stuck in 'Stop' mode after the install.

# Windows PowerShell 5.1 negotiates TLS 1.0/1.1 by default, which GitHub
# rejects ("The underlying connection was closed"). Enable TLS 1.2 explicitly.
try {
  [Net.ServicePointManager]::SecurityProtocol =
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch { }

# Corporate proxies: send the signed-in user's credentials when one is required.
try {
  [System.Net.WebRequest]::DefaultWebProxy.Credentials =
    [System.Net.CredentialCache]::DefaultNetworkCredentials
} catch { }

$script:AlphacodeMinRustMinor = 91
$script:AlphacodeIsHosted     = [string]::IsNullOrEmpty($PSCommandPath)

# =============================================================================
# Output helpers
# =============================================================================

function Write-AlphacodeInfo([string]$Message) { Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-AlphacodeOk  ([string]$Message) { Write-Host "[ok] $Message" -ForegroundColor Green }
function Write-AlphacodeWarn([string]$Message) { Write-Host "[warn] $Message" -ForegroundColor Yellow }

# Abort the install by THROWING, never by calling `exit`. Under `iex` the text
# runs in the caller's session, and `exit` there closes the user's terminal.
# A plain exception type is used on purpose: Windows PowerShell 5.1 cannot parse
# methods or constructors inside PowerShell classes, and a custom class that
# only carries a field loses its text in `$_.Exception.Message`.
function Stop-AlphacodeInstall([string]$Message) {
  throw [System.InvalidOperationException]::new($Message)
}

# =============================================================================
# Environment detection (pure helpers: no I/O, so they can be unit-tested)
# =============================================================================

# Deliberately NOT `$IsWindows`: it only exists in PowerShell 6+, and the
# documented one-liner runs Windows PowerShell 5.1 where it is $null.
function Test-AlphacodeWindows {
  [CmdletBinding()]
  param()
  return ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
}

# True when the script text was evaluated into someone else's session
# (`iex`, script block) instead of being run as a file. `$PSCommandPath` is
# empty in that case. Decides whether a failure may `exit` (file) or must just
# report and return (hosted).
function Test-AlphacodeHost {
  [CmdletBinding()]
  param()
  return $script:AlphacodeIsHosted
}

function Test-AlphacodeAdmin {
  [CmdletBinding()]
  param()
  try {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($id)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  } catch { return $false }
}

# Map Windows architecture tokens onto the Rust target suffix used by release
# asset names. Inputs are ordered from "describes the machine" to "describes
# the shell":
#   RuntimeArch -- .NET's OSArchitecture ('X64', 'Arm64', ...), when available;
#   OsArch      -- PROCESSOR_ARCHITECTURE_WINDOWS (set under x64 emulation on ARM64);
#   Wow64Arch   -- PROCESSOR_ARCHITEW6432 (set for 32-bit shells on 64-bit Windows);
#   NativeArch  -- PROCESSOR_ARCHITECTURE (correct in the plain 64-bit case).
function Get-AlphacodeWindowsArch {
  [CmdletBinding()]
  param(
    [AllowNull()][AllowEmptyString()][string]$NativeArch,
    [AllowNull()][AllowEmptyString()][string]$Wow64Arch,
    [AllowNull()][AllowEmptyString()][string]$OsArch,
    [AllowNull()][AllowEmptyString()][string]$RuntimeArch
  )

  foreach ($token in @($RuntimeArch, $OsArch, $Wow64Arch, $NativeArch)) {
    if ([string]::IsNullOrWhiteSpace($token)) { continue }
    switch ($token.Trim().ToUpperInvariant()) {
      'ARM64' { return 'arm64' }
      'AMD64' { return 'x86_64' }
      'X64'   { return 'x86_64' }
    }
  }
  return $null
}

function Get-AlphacodeDefaultPrefix {
  [CmdletBinding()]
  param([string]$InstallScope = 'User')

  if (Test-AlphacodeWindows) {
    # Per-user "Program Files" equivalent (what Git, VS Code and ripgrep use).
    # Keeps code apart from the %LOCALAPPDATA%\alphacode data directory so an
    # uninstall that wipes user data does not also wipe the binary.
    if ($InstallScope -eq 'Machine') {
      $pf = if ($env:ProgramFiles) { $env:ProgramFiles } else { 'C:\Program Files' }
      return (Join-Path $pf 'alphacode')
    }
    $lad = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA }
           else { [Environment]::GetFolderPath('LocalApplicationData') }
    return (Join-Path $lad 'Programs\alphacode')
  }
  return "$HOME/.local"
}

# Expand %VARS% and make a path absolute against the PowerShell location (not
# the .NET process directory), so `-Prefix .\tools` ends up as a full path.
function Resolve-AlphacodePath([string]$Path) {
  $expanded = [Environment]::ExpandEnvironmentVariables($Path.Trim().Trim('"'))
  return $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($expanded).TrimEnd('\', '/')
}

# =============================================================================
# PATH planning (pure) and registry access
# =============================================================================
#
# The planner takes the current PATH as a string and touches nothing, so it can
# be tested over a matrix of shapes (empty, trailing separators, case
# differences, %VAR% references, sibling directories) without a registry.

# Normalise one PATH entry for comparison: trimmed, unquoted, %VAR% expanded,
# and without trailing separators. Windows treats `C:\a\bin`, `C:\a\bin\` and
# `%LOCALAPPDATA%\a\bin` as the same directory; without this, every re-run
# would append another duplicate.
function ConvertTo-AlphacodeComparablePathEntry([string]$Entry) {
  if ($null -eq $Entry) { return '' }
  $e = $Entry.Trim().Trim('"')
  $e = [Environment]::ExpandEnvironmentVariables($e)
  return $e.Trim().TrimEnd('\', '/')
}

# Decide what the PATH should become. Appends rather than prepends: appending
# provably cannot change which executable wins for any pre-existing entry.
function Get-AlphacodePathPlan {
  [CmdletBinding()]
  param(
    [AllowNull()][AllowEmptyString()][string]$UserPath,
    [Parameter(Mandatory)][string]$BinDir
  )

  $target = ConvertTo-AlphacodeComparablePathEntry $BinDir
  if ([string]::IsNullOrWhiteSpace($target)) { throw 'BinDir must not be empty.' }

  # Drop empty segments (a doubled ';;' from a hand-edited PATH) but keep each
  # entry's original text so nothing is rewritten on the way out.
  $entries = @()
  if ($null -ne $UserPath) { $entries = @($UserPath -split ';' | Where-Object { $_ -ne '' }) }

  foreach ($entry in $entries) {
    if ((ConvertTo-AlphacodeComparablePathEntry $entry) -ieq $target) {
      # Already there: return the value untouched so a re-run cannot normalise
      # or reorder the stored PATH.
      return [pscustomobject]@{ AlreadyPresent = $true; Original = $UserPath; Value = $UserPath }
    }
  }

  $value = if ($entries.Count -eq 0) { $BinDir.TrimEnd('\', '/') }
           else { ($entries -join ';') + ';' + $BinDir.TrimEnd('\', '/') }
  return [pscustomobject]@{ AlreadyPresent = $false; Original = $UserPath; Value = $value }
}

function Open-AlphacodeEnvironmentKey {
  param([string]$PathScope, [bool]$Writable)
  if ($PathScope -eq 'Machine') {
    return [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
      'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $Writable)
  }
  return [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $Writable)
}

# Read the registry PATH *raw* (unexpanded) together with its value kind.
# [Environment]::GetEnvironmentVariable would expand REG_EXPAND_SZ, so writing
# its result back would freeze today's %USERPROFILE% into the registry and hide
# the kind needed to write the value back correctly.
function Get-AlphacodeRawPath {
  [CmdletBinding()]
  param([ValidateSet('User', 'Machine')][string]$PathScope = 'User')

  $none = [pscustomobject]@{ Exists = $false; Value = $null; Kind = [Microsoft.Win32.RegistryValueKind]::ExpandString }
  if (-not (Test-AlphacodeWindows)) { return $none }
  $key = Open-AlphacodeEnvironmentKey -PathScope $PathScope -Writable $false
  if ($null -eq $key) { return $none }
  try {
    $exists = $key.GetValueNames() -contains 'Path'
    if (-not $exists) { return $none }
    $kind = $key.GetValueKind('Path')
    $raw  = $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    return [pscustomobject]@{ Exists = $true; Value = $raw; Kind = $kind }
  } finally { $key.Close() }
}

# Kept for compatibility with existing tests and callers.
function Get-AlphacodeRawUserPath {
  [CmdletBinding()]
  param()
  return (Get-AlphacodeRawPath -PathScope 'User')
}

# Tell running GUI processes (Explorer, taskbar, anything that caches the
# environment) that `Environment` changed, so NEW terminals started from them
# see the value without a sign-out. Same broadcast `setx` sends. Best-effort.
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
    $result = [UIntPtr]::Zero
    # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG (a hung Explorer must
    # not block the installer for the whole timeout).
    [void][AlphacodeInstaller.Native]::SendMessageTimeout(
      [IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 5000, [ref]$result)
  } catch {
    Write-AlphacodeWarn "Could not broadcast the environment change: $($_.Exception.Message)"
    Write-AlphacodeWarn 'Close and reopen your terminal for the PATH change to take effect.'
  }
}

# Append BinDir to the persistent PATH of the chosen scope. Returns a result
# object so the caller can report accurately. Never throws for expected
# failures: the binary is already installed, so a PATH problem must degrade to
# clear manual instructions rather than abort the install.
function Add-AlphacodeToPath {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$BinDir,
    [ValidateSet('User', 'Machine')][string]$PathScope = 'User',
    [switch]$DryRun
  )

  if (-not (Test-AlphacodeWindows)) {
    Write-AlphacodeWarn 'PATH registration only applies on Windows; use your shell profile elsewhere.'
    return [pscustomobject]@{ Status = 'skipped'; Reason = 'not-windows' }
  }

  $label = if ($PathScope -eq 'Machine') { 'HKLM' } else { 'HKCU' }
  $current = Get-AlphacodeRawPath -PathScope $PathScope
  $plan = Get-AlphacodePathPlan -UserPath $current.Value -BinDir $BinDir

  if ($plan.AlreadyPresent) {
    Write-AlphacodeInfo "Already on your $($PathScope.ToLower()) PATH: $BinDir"
    return [pscustomobject]@{ Status = 'already-present'; Value = $plan.Value }
  }

  # REG_EXPAND_SZ is limited to 32767 characters. Truncating would silently
  # drop whichever entries fell off the end, so refuse instead.
  if ($plan.Value.Length -gt 32767) {
    Write-AlphacodeWarn ("Adding '$BinDir' would make the $($PathScope.ToLower()) PATH $($plan.Value.Length) " +
                         'characters, over the Windows limit of 32767. Not writing it; trim your PATH and re-run.')
    return [pscustomobject]@{ Status = 'failed'; Reason = 'too-long' }
  }

  if ($DryRun) {
    Write-Host "[dry-run] $label\Environment\Path would be set to:"
    Write-Host "[dry-run]   $($plan.Value)"
    Write-Host '[dry-run] (nothing was written; re-run without -PathDryRun to apply)'
    return [pscustomobject]@{ Status = 'dry-run'; Value = $plan.Value }
  }

  $key = $null
  try { $key = Open-AlphacodeEnvironmentKey -PathScope $PathScope -Writable $true } catch { $key = $null }
  if ($null -eq $key) {
    Write-AlphacodeWarn "Could not open $label\Environment for writing; PATH not modified."
    return [pscustomobject]@{ Status = 'failed'; Reason = 'key-unavailable' }
  }
  try {
    # Write back with the kind we read; a missing value defaults to
    # REG_EXPAND_SZ, which is what Windows itself uses for PATH.
    $kind = if (-not $current.Exists -or $current.Kind -eq [Microsoft.Win32.RegistryValueKind]::Unknown) {
      [Microsoft.Win32.RegistryValueKind]::ExpandString
    } else { $current.Kind }
    $key.SetValue('Path', $plan.Value, $kind)
  } catch {
    Write-AlphacodeWarn "Writing $label\Environment\Path failed: $($_.Exception.Message)"
    return [pscustomobject]@{ Status = 'failed'; Reason = 'write-failed' }
  } finally { $key.Close() }

  # Read it back: do not claim success on the strength of SetValue not throwing.
  $after = Get-AlphacodePathPlan -UserPath (Get-AlphacodeRawPath -PathScope $PathScope).Value -BinDir $BinDir
  if (-not $after.AlreadyPresent) {
    Write-AlphacodeWarn 'The PATH write did not persist (policy or security software may be blocking it).'
    return [pscustomobject]@{ Status = 'failed'; Reason = 'readback-mismatch' }
  }

  Publish-AlphacodeEnvironmentChange
  Write-AlphacodeOk "Added to your $($PathScope.ToLower()) PATH: $BinDir"
  return [pscustomobject]@{ Status = 'added'; Value = $plan.Value }
}

# Kept for compatibility with existing tests and callers.
function Add-AlphacodeToUserPath {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$BinDir, [switch]$DryRun)
  return (Add-AlphacodeToPath -BinDir $BinDir -PathScope 'User' -DryRun:$DryRun)
}

# Make the binary usable in THIS window right now. The registry write cannot
# help the shell the user is standing in, and a one-line installer's whole point
# is that no second terminal is open yet. Prepends to $env:PATH for this process
# only; never persisted. (Prepending the *session* is safe, whereas prepending
# the *stored* PATH would reorder it for every future shell.)
function Add-AlphacodeToSessionPath {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$BinDir)

  $target = ConvertTo-AlphacodeComparablePathEntry $BinDir
  # Choose the separator from the target's syntax as well as the host OS so a
  # simulated `C:\...` install is handled consistently on non-Windows test hosts.
  $usesWindowsSyntax = (Test-AlphacodeWindows) -or ($BinDir -match '^[A-Za-z]:[\\/]')
  $sep = if ($usesWindowsSyntax) { ';' } else { [System.IO.Path]::PathSeparator }
  $entries = @($env:PATH -split [regex]::Escape($sep) | Where-Object { $_ -ne '' })
  foreach ($e in $entries) {
    if ((ConvertTo-AlphacodeComparablePathEntry $e) -ieq $target) {
      return [pscustomobject]@{ Status = 'already-present' }
    }
  }
  $env:PATH = "$BinDir$sep" + $env:PATH
  return [pscustomobject]@{ Status = 'added' }
}

# =============================================================================
# Process helpers and "will a NEW terminal find it?" verification
# =============================================================================

function Invoke-AlphacodeNative {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$FilePath,
    [string]$Arguments = '',
    [hashtable]$Environment,
    [string]$WorkingDirectory,
    [int]$TimeoutMs = 20000
  )

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $FilePath
  $psi.Arguments = $Arguments
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.CreateNoWindow = $true
  if ($WorkingDirectory) { $psi.WorkingDirectory = $WorkingDirectory }
  if ($Environment) {
    foreach ($k in $Environment.Keys) { $psi.EnvironmentVariables[$k] = [string]$Environment[$k] }
  }

  $proc = New-Object System.Diagnostics.Process
  $proc.StartInfo = $psi
  try {
    [void]$proc.Start()
    # Read both streams asynchronously so a full pipe cannot deadlock the wait.
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()
    if (-not $proc.WaitForExit($TimeoutMs)) {
      try { $proc.Kill() } catch { }
      return [pscustomobject]@{ ExitCode = -1; Stdout = ''; Stderr = 'timed out'; TimedOut = $true }
    }
    $proc.WaitForExit()
    return [pscustomobject]@{ ExitCode = $proc.ExitCode; Stdout = $outTask.Result; Stderr = $errTask.Result; TimedOut = $false }
  } finally { $proc.Dispose() }
}

# The PATH a brand-new process would get: machine PATH, then user PATH, each
# expanded, straight from the registry (not from this session).
function Get-AlphacodeFreshPath {
  [CmdletBinding()]
  param()
  $parts = @()
  foreach ($s in @('Machine', 'User')) {
    $raw = Get-AlphacodeRawPath -PathScope $s
    if ($raw.Value) { $parts += [Environment]::ExpandEnvironmentVariables($raw.Value) }
  }
  return ($parts -join ';')
}

# Launch where.exe with the registry PATH to see which alphacode.exe a new
# terminal would run first. Status: ok | shadowed | not-found | unknown.
function Test-AlphacodeFreshShell {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$ExpectedExe)

  try {
    $where = Join-Path $env:SystemRoot 'System32\where.exe'
    if (-not (Test-Path -LiteralPath $where)) { return [pscustomobject]@{ Status = 'unknown'; First = $null } }
    $r = Invoke-AlphacodeNative -FilePath $where -Arguments 'alphacode.exe' `
          -Environment @{ Path = (Get-AlphacodeFreshPath) } -WorkingDirectory $env:SystemRoot
    if ($r.ExitCode -ne 0) { return [pscustomobject]@{ Status = 'not-found'; First = $null } }
    $first = @($r.Stdout -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First 1)[0]
    if ($first -and ($first.Trim() -ieq $ExpectedExe)) { return [pscustomobject]@{ Status = 'ok'; First = $first.Trim() } }
    return [pscustomobject]@{ Status = 'shadowed'; First = $first }
  } catch {
    return [pscustomobject]@{ Status = 'unknown'; First = $null }
  }
}

# Run `<exe> --version` (handled by clap before any application logic) and pull
# the version out. Returns $null when it cannot be determined.
function Get-AlphacodeInstalledVersion {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$Exe)

  # Windows SmartScreen / Defender may need a moment to allow a fresh .exe.
  Start-Sleep -Milliseconds 500
  try {
    $r = Invoke-AlphacodeNative -FilePath $Exe -Arguments '--version' -TimeoutMs 15000
    if ($r.ExitCode -ne 0) {
      $detail = if ($r.Stderr) { $r.Stderr.Trim() } else { "exit code $($r.ExitCode)" }
      Write-AlphacodeWarn "Binary installed but could not verify its version: $detail"
      return $null
    }
    $m = [regex]::Match($r.Stdout, 'alphacode\s+v?(\d[\w.\-+]*)', 'IgnoreCase')
    if ($m.Success) { return $m.Groups[1].Value }
    Write-AlphacodeWarn "Installed, but the version output was not recognised: $($r.Stdout.Trim())"
    return $null
  } catch {
    $msg = $_.Exception.Message
    if ([string]::IsNullOrWhiteSpace($msg)) { $msg = 'process failed to start (check file permissions and antivirus)' }
    Write-AlphacodeWarn "Binary installed but could not verify its version: $msg"
    return $null
  }
}

# =============================================================================
# Getting the binary: release download or source build
# =============================================================================

function Invoke-AlphacodeDownload {
  [CmdletBinding()]
  param([string]$Uri, [string]$OutFile, [int]$Attempts = 3)

  $lastError = $null
  for ($i = 1; $i -le $Attempts; $i++) {
    try {
      Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing -ErrorAction Stop
      return
    } catch {
      $lastError = $_
      # A 404 will not fix itself; do not waste time retrying it.
      $status = $null
      try { $status = [int]$_.Exception.Response.StatusCode } catch { }
      if ($status -eq 404) { break }
      if ($i -lt $Attempts) { Start-Sleep -Seconds (2 * $i) }
    }
  }
  throw $lastError
}

function Expand-AlphacodeZip([string]$ZipPath, [string]$Destination) {
  try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction Stop
    [System.IO.Compression.ZipFile]::ExtractToDirectory($ZipPath, $Destination)
  } catch {
    # Fall back to the archive cmdlet when the .NET path is unavailable.
    Expand-Archive -Path $ZipPath -DestinationPath $Destination -Force -ErrorAction Stop
  }
}

# Download, verify and unpack a release asset. Returns an object with Ok = $true
# and the staged files, or Ok = $false and a Reason (so the caller can fall back
# to a source build). A checksum MISMATCH is never a soft failure: it throws.
function Get-AlphacodeReleaseStage {
  [CmdletBinding()]
  param(
    [string]$Repo,
    [string]$Version,
    [string]$Asset,
    [string]$WorkDir
  )

  # Work out where to download from. For 'latest' the API gives a concrete tag
  # for nicer messages; if the API is rate limited (60/hour unauthenticated)
  # GitHub's /releases/latest/download redirect needs no API call at all.
  $candidates = @()
  if ($Version -eq 'latest') {
    Write-AlphacodeInfo "Resolving latest release from $Repo ..."
    $tag = $null
    try {
      $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" `
              -UserAgent 'alphacode-installer' -TimeoutSec 20 -ErrorAction Stop
      if ($rel -and $rel.tag_name) { $tag = [string]$rel.tag_name }
    } catch { $tag = $null }
    if ($tag) {
      Write-AlphacodeInfo "Latest release: $tag"
      $candidates += [pscustomobject]@{ Tag = $tag; Base = "https://github.com/$Repo/releases/download/$tag" }
    } else {
      $candidates += [pscustomobject]@{ Tag = 'latest'; Base = "https://github.com/$Repo/releases/latest/download" }
    }
  } else {
    $candidates += [pscustomobject]@{ Tag = $Version; Base = "https://github.com/$Repo/releases/download/$Version" }
    # Accept `1.2.3` as well as `v1.2.3`.
    if ($Version -match '^\d') {
      $candidates += [pscustomobject]@{ Tag = "v$Version"; Base = "https://github.com/$Repo/releases/download/v$Version" }
    }
  }

  New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
  $zipPath = Join-Path $WorkDir $Asset
  $chosen = $null
  $lastErr = 'no candidate URL'
  foreach ($c in $candidates) {
    $url = "$($c.Base)/$Asset"
    Write-AlphacodeInfo "Downloading $url"
    try {
      Invoke-AlphacodeDownload -Uri $url -OutFile $zipPath
      $chosen = $c
      break
    } catch {
      $lastErr = $_.Exception.Message
    }
  }
  if (-not $chosen) {
    return [pscustomobject]@{ Ok = $false; Reason = "download failed: $lastErr" }
  }

  # --- Checksum ---------------------------------------------------------------
  # Only the *fetch* of SHA256SUMS is allowed to fail softly (offline, proxy, a
  # release with no manifest). The comparison sits outside the try/catch, so a
  # mismatch is unconditionally fatal. The manifest is saved to a file and read
  # as text: GitHub serves release assets as application/octet-stream, for which
  # Invoke-WebRequest's .Content is a byte[] that cannot be parsed as lines.
  $expectedHash = $null
  try {
    $sumsPath = Join-Path $WorkDir 'SHA256SUMS'
    Invoke-AlphacodeDownload -Uri "$($chosen.Base)/SHA256SUMS" -OutFile $sumsPath -Attempts 2
    foreach ($line in (Get-Content -LiteralPath $sumsPath -ErrorAction Stop)) {
      # sha256sum format: "<digest>  <name>", name optionally prefixed by "*".
      $parts = @($line -split '\s+' | Where-Object { $_ -ne '' })
      if ($parts.Count -lt 2) { continue }
      $name = ($parts[1] -replace '^\*', '') -replace '^\.[\\/]', ''
      if ($name -eq $Asset) { $expectedHash = $parts[0].ToLowerInvariant(); break }
    }
  } catch {
    Write-AlphacodeWarn 'Could not fetch SHA256SUMS (offline or proxy?); continuing unverified.'
  }

  if ($expectedHash) {
    $actualHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expectedHash -ne $actualHash) {
      Stop-AlphacodeInstall ("Checksum mismatch for $Asset. The download does not match the published " +
        "release and has NOT been installed.`n  expected $expectedHash`n  actual   $actualHash`n" +
        'Re-run the installer. If it persists, the download was corrupted or tampered with; do not run this file.')
    }
    Write-AlphacodeOk 'Checksum verified.'
  } else {
    Write-AlphacodeWarn "No published checksum found for $Asset; this download is unverified."
  }

  # --- Extract ----------------------------------------------------------------
  Write-AlphacodeInfo 'Extracting ...'
  $extract = Join-Path $WorkDir 'extract'
  New-Item -ItemType Directory -Force -Path $extract | Out-Null
  try { Expand-AlphacodeZip -ZipPath $zipPath -Destination $extract }
  catch { Stop-AlphacodeInstall "Extract failed: $($_.Exception.Message)" }

  $exe = Get-ChildItem -Path $extract -Recurse -Filter 'alphacode.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $exe) { Stop-AlphacodeInstall "The extracted archive did not contain 'alphacode.exe'." }

  # Release wrapper scripts may ship .bin payload files that must sit beside the exe.
  $payload = @(Get-ChildItem -Path $extract -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })

  return [pscustomobject]@{ Ok = $true; ExePath = $exe.FullName; Payload = $payload; Tag = $chosen.Tag; Source = 'release' }
}

# Fallback: clone the repo, build with cargo, and stage the resulting binary.
# Needs git, cargo, rustc >= 1.91 and a C toolchain; a first build can take
# 5-30 minutes.
function Build-FromSource {
  [CmdletBinding()]
  param([string]$Repo, [string]$SourceRef, [string]$WorkDir)

  if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Stop-AlphacodeInstall 'git is required to build from source (install Git for Windows from https://git-scm.com).'
  }
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Stop-AlphacodeInstall 'cargo is required to build from source (install Rust from https://rustup.rs).'
  }
  if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Stop-AlphacodeInstall 'rustc was not found on PATH (install Rust from https://rustup.rs).'
  }

  $rv = (& rustc --version 2>$null)
  if ($rv -match 'rustc\s+(\d+)\.(\d+)') {
    $major = [int]$Matches[1]; $minor = [int]$Matches[2]
    if ($major -lt 1 -or ($major -eq 1 -and $minor -lt $script:AlphacodeMinRustMinor)) {
      Stop-AlphacodeInstall "rustc $($Matches[0]) is too old; need >= 1.$($script:AlphacodeMinRustMinor) (run 'rustup update')."
    }
  }

  $srcDir = Join-Path $WorkDir 'src'
  Write-AlphacodeInfo "Cloning $Repo into a temporary build directory ..."
  $cloneUrl = "https://github.com/$Repo.git"
  if ($SourceRef) { & git clone --quiet --depth 1 --branch $SourceRef $cloneUrl $srcDir }
  else            { & git clone --quiet --depth 1 $cloneUrl $srcDir }
  if ($LASTEXITCODE -ne 0) {
    $refNote = if ($SourceRef) { " (ref: $SourceRef)" } else { '' }
    Stop-AlphacodeInstall "git clone failed$refNote"
  }

  Write-AlphacodeInfo 'Compiling alphacode (this can take 5-30 minutes on a first build) ...'
  # No --locked on purpose: the committed Cargo.lock does not list
  # platform-conditional deps for every target triple, and CI builds without it.
  & cargo build --release --manifest-path (Join-Path $srcDir 'Cargo.toml')
  if ($LASTEXITCODE -ne 0) {
    Stop-AlphacodeInstall 'cargo build failed (on Windows this usually needs the Visual Studio C++ Build Tools).'
  }

  $built = Join-Path $srcDir 'target\release\alphacode.exe'
  if (-not (Test-Path -LiteralPath $built)) {
    Stop-AlphacodeInstall 'The build succeeded but target\release\alphacode.exe was not produced.'
  }
  return [pscustomobject]@{ Ok = $true; ExePath = $built; Payload = @(); Tag = 'source'; Source = 'source' }
}

# =============================================================================
# Installing the files
# =============================================================================

# Copy a file over an existing one, even when the existing one is a RUNNING
# executable. Windows refuses to overwrite a running .exe but allows renaming
# it, so an in-place update (`alphacode update`) moves the old file aside first.
function Copy-AlphacodeFileReplacing {
  [CmdletBinding()]
  param([string]$Source, [string]$Destination)

  if (Test-Path -LiteralPath $Destination) {
    try {
      Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
      return
    } catch {
      $old = "$Destination.old"
      try {
        if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Force -ErrorAction Stop }
        Move-Item -LiteralPath $Destination -Destination $old -Force -ErrorAction Stop
      } catch {
        Stop-AlphacodeInstall ("Cannot replace $Destination because it is in use. Close every running " +
                               'alphacode window (and any editor terminal using it), then re-run the installer.')
      }
    }
  }
  Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
}

function Install-AlphacodeFiles {
  [CmdletBinding()]
  param([Parameter(Mandatory)]$Stage, [Parameter(Mandatory)][string]$BinDir)

  New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
  $installedExe = Join-Path $BinDir 'alphacode.exe'
  Copy-AlphacodeFileReplacing -Source $Stage.ExePath -Destination $installedExe

  foreach ($p in $Stage.Payload) {
    Copy-AlphacodeFileReplacing -Source $p -Destination (Join-Path $BinDir (Split-Path -Leaf $p))
  }

  # Clear the "downloaded from the internet" mark if present, and tidy any
  # *.old files left behind by a previous in-place update (best effort).
  try { Unblock-File -LiteralPath $installedExe -ErrorAction SilentlyContinue } catch { }
  Get-ChildItem -LiteralPath $BinDir -Filter '*.old' -ErrorAction SilentlyContinue |
    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }

  Write-AlphacodeOk "Installed -> $installedExe$(if ($Stage.Source -eq 'source') { ' (built from source)' })"
  return $installedExe
}

# =============================================================================
# Installer entry point
# =============================================================================
#
# Everything runs inside a function invoked through a handler so a failure ends
# the INSTALLER without ending the user's shell. The preferences below are
# function-local, so they cannot leak into an `iex` caller's session.
function Invoke-AlphacodeInstall {
  $ErrorActionPreference = 'Stop'
  # Windows PowerShell 5.1 renders a progress bar for every download chunk,
  # which makes Invoke-WebRequest many times slower.
  $ProgressPreference = 'SilentlyContinue'

  if (-not (Test-AlphacodeWindows)) {
    Stop-AlphacodeInstall 'This script is for Windows. On Linux/macOS use scripts/install.sh.'
  }
  if ($Scope -eq 'Machine' -and -not (Test-AlphacodeAdmin)) {
    Stop-AlphacodeInstall ('-Scope Machine writes to Program Files and the machine-wide PATH, which needs ' +
                           'administrator rights. Re-run from an elevated PowerShell, or omit -Scope for a per-user install.')
  }

  $repoName = if ($Repo)    { $Repo }    else { 'dragonked2/alphacode' }
  $wanted   = if ($Version) { $Version } else { 'latest' }

  # --- Architecture: the ALPHACODE_ARCH override is applied BEFORE the
  # "unsupported" check, otherwise the error's own advice could never work.
  $arch = $null
  if ($env:ALPHACODE_ARCH) {
    $forced = $env:ALPHACODE_ARCH.Trim().ToLowerInvariant()
    if ($forced -eq 'x86_64' -or $forced -eq 'arm64') {
      $arch = $forced
      Write-AlphacodeInfo "Architecture forced to $arch via ALPHACODE_ARCH."
    } else {
      Write-AlphacodeWarn "Ignoring ALPHACODE_ARCH='$($env:ALPHACODE_ARCH)' (expected x86_64 or arm64)."
    }
  }
  if (-not $arch) {
    $runtimeArch = $null
    try { $runtimeArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString() } catch { }
    $arch = Get-AlphacodeWindowsArch `
      -NativeArch  $env:PROCESSOR_ARCHITECTURE `
      -Wow64Arch   $env:PROCESSOR_ARCHITEW6432 `
      -OsArch      $env:PROCESSOR_ARCHITECTURE_WINDOWS `
      -RuntimeArch $runtimeArch
  }
  if (-not $arch) {
    Stop-AlphacodeInstall ("Unsupported architecture (PROCESSOR_ARCHITECTURE='$env:PROCESSOR_ARCHITECTURE' " +
      "PROCESSOR_ARCHITEW6432='$env:PROCESSOR_ARCHITEW6432' PROCESSOR_ARCHITECTURE_WINDOWS='$env:PROCESSOR_ARCHITECTURE_WINDOWS'). " +
      'Set $env:ALPHACODE_ARCH = ''x86_64'' (or ''arm64'') and re-run, or download the asset manually.')
  }
  $asset = "alphacode-windows-$arch.zip"

  # --- Locations ---------------------------------------------------------------
  $prefixDir = if ($Prefix) { Resolve-AlphacodePath $Prefix } else { Get-AlphacodeDefaultPrefix -InstallScope $Scope }
  $binDir    = if ($BinDir) { Resolve-AlphacodePath $BinDir } else { Join-Path $prefixDir 'bin' }

  # --- Get the binary ----------------------------------------------------------
  $work = Join-Path ([System.IO.Path]::GetTempPath()) ('alphacode-install-' + [System.Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $work | Out-Null

  try {
    $stage = $null
    if ($FromSource) {
      Write-AlphacodeInfo '-FromSource requested; skipping the release download.'
      $stage = Build-FromSource -Repo $repoName -SourceRef $SourceRef -WorkDir $work
    } else {
      $stage = Get-AlphacodeReleaseStage -Repo $repoName -Version $wanted -Asset $asset -WorkDir (Join-Path $work 'release')
      if (-not $stage.Ok) {
        if ($SourceOnly) { Stop-AlphacodeInstall "$($stage.Reason) (and -SourceOnly is set)" }
        Write-AlphacodeWarn "No usable prebuilt release for windows/$arch ($($stage.Reason))."
        Write-AlphacodeWarn 'Falling back to building from source.'
        $stage = Build-FromSource -Repo $repoName -SourceRef $SourceRef -WorkDir $work
      }
    }

    # --- Install ---------------------------------------------------------------
    $installedExe = Install-AlphacodeFiles -Stage $stage -BinDir $binDir
    $installedVersion = Get-AlphacodeInstalledVersion -Exe $installedExe
    if ($installedVersion) { Write-AlphacodeOk "Installed version: $installedVersion" }
  }
  finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
  }

  # --- PATH ----------------------------------------------------------------------
  # Part of a working install by default, not an opt-in flag. See the header for
  # the safety rules. Every step below degrades to a message, never an abort:
  # the binary is already installed.
  $pathResult = $null
  if (-not $NoPath) {
    $pathResult = Add-AlphacodeToPath -BinDir $binDir -PathScope $Scope -DryRun:$PathDryRun
  }

  $sessionOk = $false
  if (-not $PathDryRun) {
    try { [void](Add-AlphacodeToSessionPath -BinDir $binDir); $sessionOk = $true }
    catch { Write-AlphacodeWarn "Could not update this window's PATH: $($_.Exception.Message)" }
  }

  # Prove a NEW terminal will resolve to this exact binary.
  $fresh = $null
  $pathReady = $pathResult -and ($pathResult.Status -eq 'added' -or $pathResult.Status -eq 'already-present')
  if ($pathReady) {
    $fresh = Test-AlphacodeFreshShell -ExpectedExe $installedExe
    switch ($fresh.Status) {
      'ok'        { Write-AlphacodeOk 'Verified: a new terminal will find this alphacode.exe.' }
      'shadowed'  {
        Write-AlphacodeWarn "A different alphacode takes priority in new terminals: $($fresh.First)"
        Write-AlphacodeWarn "Delete it or remove its folder from PATH so the one at $installedExe is used."
      }
      'not-found' { Write-AlphacodeWarn 'The PATH entry was written but a fresh process could not find alphacode.exe.' }
      default     { }
    }
  }

  # --- Summary -------------------------------------------------------------------
  Write-Host ''
  if ($pathResult -and $pathResult.Status -eq 'failed') {
    Write-Host 'Could not update your PATH automatically.' -ForegroundColor Yellow
    Write-Host '  Add this folder by hand: Settings > System > About > Advanced system settings'
    Write-Host '  > Environment Variables > user Path > New:'
    Write-Host "    $binDir"
    if ($sessionOk) { Write-Host '  alphacode already works in THIS window; only new terminals need it.' }
  } elseif ($NoPath) {
    Write-Host "PATH was left alone (-NoPath). Run it by full path: $installedExe" -ForegroundColor Yellow
  } elseif ($sessionOk) {
    Write-AlphacodeOk 'Ready: alphacode works in this window now.'
    if ($pathReady) {
      Write-Host '  New terminals pick it up automatically. If one still says "not recognized", fully close'
      Write-Host '  and reopen that app (Windows Terminal, VS Code and other editors keep the old PATH until restarted).'
    }
  }

  if (-not $PathDryRun) {
    Write-Host ''
    Write-Host 'Next:'
    Write-Host '  alphacode login    # connect a model provider'
    Write-Host '  alphacode          # start the TUI'
    Write-Host '  alphacode doctor   # check your setup'
  }
  Write-AlphacodeInfo 'Done.'
}

# Invoke the installer and report failure correctly for BOTH invocation styles:
#   * `pwsh -File scripts/install.ps1`      -> non-zero exit code for CI/callers.
#   * `irm ... | iex` / script block        -> red message and $LASTEXITCODE = 1;
#                                              the user's terminal SURVIVES.
# Set ALPHACODE_NO_AUTORUN=1 to dot-source the file (e.g. from tests) and use
# its functions without running an install.
if (-not $env:ALPHACODE_NO_AUTORUN) {
  try {
    Invoke-AlphacodeInstall
    $global:LASTEXITCODE = 0
  }
  catch {
    Write-Host "[fail] $($_.Exception.Message)" -ForegroundColor Red
    $global:LASTEXITCODE = 1
    if (-not (Test-AlphacodeHost)) { exit 1 }
    return
  }
}
