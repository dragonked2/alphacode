#!/usr/bin/env pwsh
# install.ps1 — PowerShell installer for Alphacode (Windows + cross-platform pwsh)
#
# Usage:
#   iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
#   iwr -useb ... | iex -Version v1.0.0
#   iwr -useb ... | iex -Prefix "$env:LOCALAPPDATA\Programs\alphacode"
#   iwr -useb ... | iex -FromSource                  # skip release, build locally
#   iwr -useb ... | iex -SourceRef main               # build from a specific ref
#
# By default, tries to download a prebuilt release asset. If no release is
# published (or there is no asset for this OS/arch), it falls back to
# building from source. Requires: git, cargo, rustc >= 1.88.

[CmdletBinding()]
param(
  [string]$Version   = $env:ALPHACODE_VERSION,
  [string]$Repo      = ($env:ALPHACODE_REPO -as [string]),
  [string]$Prefix    = $env:ALPHACODE_PREFIX,
  [string]$BinDir    = $env:ALPHACODE_BIN_DIR,
  [switch]$NoPath,
  [switch]$FromSource,
  [switch]$SourceOnly,
  [string]$SourceRef = $env:ALPHACODE_SOURCE_REF
)

$ErrorActionPreference = 'Stop'

if (-not $Repo)    { $Repo    = 'dragonked2/alphacode' }
if (-not $Version) { $Version = 'latest' }
if (-not $Prefix)  {
  if ($IsWindows) { $Prefix = "$env:LOCALAPPDATA\Programs\alphacode" }
  else            { $Prefix = "$HOME/.local" }
}
if (-not $BinDir)  { $BinDir = Join-Path $Prefix 'bin' }

function Print([string]$msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Warn ([string]$msg) { Write-Host "[warn] $msg" -ForegroundColor Yellow }
function Fail ([string]$msg) { Write-Host "[fail] $msg" -ForegroundColor Red; exit 1 }

# --- build_from_source -------------------------------------------------------
#
# Fallback: no release artifact for this platform/arch. Clone the repo, build
# with cargo, and copy the resulting binary into $BinDir.
#
# Requires: git, cargo, rustc >= 1.88, and a working C toolchain. This can
# take 5-30 minutes on a first build.
function Build-FromSource {
  if (-not (Get-Command git   -ErrorAction SilentlyContinue)) { Fail "git is required to build from source" }
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail "cargo is required to build from source (install Rust from https://rustup.rs)" }

  # rustc >= 1.88 (edition 2024 + current dependency MSRV) check.
  $rv = (& rustc --version) 2>$null
  if ($rv -match 'rustc\s+(\d+)\.(\d+)') {
    $major = [int]$Matches[1]; $minor = [int]$Matches[2]
    if ($major -lt 1 -or ($major -eq 1 -and $minor -lt 88)) {
      Fail "rustc $($Matches[0]) is too old; need >= 1.88 (update via 'rustup update')"
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
    & cargo build --release --locked --manifest-path "$srcDir\src\Cargo.toml"
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

# --- Architecture ------------------------------------------------------------

switch ($env:PROCESSOR_ARCHITECTURE) {
  'AMD64' { $Arch = 'x86_64' }
  'ARM64' { $Arch = 'arm64' }
  default { Fail "unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}

if ($IsWindows -or ($env:OS -eq 'Windows_NT')) {
  $Platform = 'windows'
  $asset   = "alphacode-windows-$Arch.zip"
} else {
  Fail "this script is for Windows. On Linux/macOS use scripts/install.sh."
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
    Warn "no GitHub release found for $Repo — falling back to building from source."
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
  Warn "no prebuilt asset for $Platform/$Arch at $Version — falling back to building from source."
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Tmp
  Build-FromSource
  Print "Done."
  return
}

# Optional checksum verification
try {
  $sums = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$Version/SHA256SUMS" -UseBasicParsing -ErrorAction Stop
  $expected = ($sums.Content -split "`n" | Where-Object { $_ -like "*$asset*" } | Select-Object -First 1)
  if ($expected) {
    $expectedHash = ($expected -split ' ')[0]
    $actualHash   = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLower()
    if ($expectedHash -ne $actualHash) {
      Fail "checksum mismatch (expected $expectedHash, got $actualHash)"
    }
    Print "Checksum verified."
  }
} catch {
  Warn "could not fetch/verify SHA256SUMS — continuing"
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
Print "Installed -> $installedExe"

$installed = & "$BinDir\alphacode.exe" --version 2>$null
if ($installed) { Print "Installed version: $installed" }

if (-not $NoPath) {
  $haveIt = ($env:PATH -split [IO.Path]::PathSeparator) | Where-Object { $_ -ieq $BinDir } | Select-Object -First 1
  if (-not $haveIt) {
    Write-Host ""
    Write-Host "Next step: add the install location to your user PATH." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  See https://github.com/dragonked2/alphacode#install for PATH instructions."
    Write-Host ""
    Write-Host "Then open a new shell and:"
    Write-Host "  alphacode login"
    Write-Host "  alphacode"
  }
}

Remove-Item -Recurse -Force $Tmp
Print "Done."