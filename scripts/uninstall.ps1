[CmdletBinding()]
param([switch]$Purge)
$ErrorActionPreference = 'Stop'

$BinDir = if ($env:ALPHACODE_BIN_DIR) { $env:ALPHACODE_BIN_DIR } else { "$env:LOCALAPPDATA\Programs\alphacode\bin" }
$target = Join-Path $BinDir 'alphacode.exe'

if (-not (Test-Path $target)) {
  Write-Host "No alphacode.exe at $target — nothing to do."
  exit 0
}
Remove-Item -Force $target
Write-Host "Removed $target"

if ($Purge) {
  foreach ($p in @("$env:APPDATA\alphacode", "$env:LOCALAPPDATA\alphacode")) {
    if (Test-Path $p) { Remove-Item -Recurse -Force $p; Write-Host "Removed $p" }
  }
}