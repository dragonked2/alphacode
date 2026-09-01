#!/usr/bin/env pwsh
# install.ps1 — PowerShell installer for Alphacode (Windows + cross-platform pwsh)
#
# Usage:
#   iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/install.ps1 | iex
#   iwr -useb ... | iex -Version v1.0.0
#   iwr -useb ... | iex -Prefix "$env:LOCALAPPDATA\Programs\alphacode"

[CmdletBinding()]
param(
  [string]$Version = $env:ALPHACODE_VERSION,
  [string]$Repo    = ($env:ALPHACODE_REPO -as [string]),
  [string]$Prefix  = $env:ALPHACODE_PREFIX,
  [string]$BinDir  = $env:ALPHACODE_BIN_DIR,
  [switch]$NoPath
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

if ($Version -eq 'latest') {
  Print "Resolving latest release from $Repo …"
  $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
  $Version = $rel.tag_name
  if (-not $Version) { Fail "GitHub returned an empty tag_name" }
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
  Fail "download failed: $($_.Exception.Message)"
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

Print "Extracting …"
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
Copy-Item -Path $binary.FullName -Destination (Join-Path $BinDir 'alphacode.exe') -Force
Print "Installed → $(Join-Path $BinDir 'alphacode.exe')"

$installed = & "$BinDir\alphacode.exe" --version 2>$null
if ($installed) { Print "Installed version: $installed" }

if (-not $NoPath) {
  $haveIt = ($env:PATH -split ';' | Where-Object { $_ -ieq $BinDir }) | Select-Object -First 1
  if (-not $haveIt) {
    Write-Host ""
    Write-Host "Next step: add the install location to your user PATH." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  [Environment]::SetEnvironmentVariable('PATH', `"$BinDir; `$env:PATH`", 'User')"
    Write-Host ""
    Write-Host "Then open a new shell and:"
    Write-Host "  alphacode login"
    Write-Host "  alphacode"
  }
}

Remove-Item -Recurse -Force $Tmp
Print "Done."