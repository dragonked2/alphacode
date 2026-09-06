[CmdletBinding()]
param([switch]$Purge)
$ErrorActionPreference = 'Stop'

# --- Determine paths (mirrors install.ps1 and launcher_dir() in Rust) ---
$BinDir = if ($env:ALPHACODE_INSTALL_DIR) {
    $env:ALPHACODE_INSTALL_DIR
} elseif ($env:ALPHACODE_HOME) {
    Join-Path $env:ALPHACODE_HOME 'bin'
} elseif ($env:ALPHACODE_BIN_DIR) {
    $env:ALPHACODE_BIN_DIR
} elseif ($env:LOCALAPPDATA) {
    Join-Path $env:LOCALAPPDATA 'alphacode\bin'
} else {
    Join-Path $env:LOCALAPPDATA 'Programs\alphacode\bin'
}

$target = Join-Path $BinDir 'alphacode.exe'
$LocalAppData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $env:LOCALAPPDATA 'alphacode' }
$AppData = if ($env:APPDATA) { $env:APPDATA } else { $null }
$Home = $env:USERPROFILE

$foundAny = $false

# --- Remove launcher binary from all known locations ---
$candidates = @(
    $target,
    (Join-Path "$LocalAppData\alphacode\bin" 'alphacode.exe'),
    (Join-Path "$LocalAppData\Programs\alphacode\bin" 'alphacode.exe'),
    (Join-Path "$Home\.local\bin" 'alphacode.exe')
)
foreach ($c in $candidates) {
    if ($c -and (Test-Path $c)) {
        Remove-Item -Force $c
        Write-Host "Removed $c"
        $foundAny = $true
    }
}

if (-not $foundAny) {
    Write-Host "No alphacode.exe found — nothing to do."
    exit 0
}

# --- Remove local app data (%LOCALAPPDATA%\alphacode) ---
$localAlpha = Join-Path $LocalAppData 'alphacode'
if (Test-Path $localAlpha) {
    Remove-Item -Recurse -Force $localAlpha
    Write-Host "Removed $localAlpha"
}

# --- Remove app data (%APPDATA%\alphacode) if it exists ---
if ($AppData) {
    $roamingAlpha = Join-Path $AppData 'alphacode'
    if (Test-Path $roamingAlpha) {
        Remove-Item -Recurse -Force $roamingAlpha
        Write-Host "Removed $roamingAlpha"
    }
}

# --- Remove user home data (~/.alphacode) ---
$homeAlpha = Join-Path $Home '.alphacode'
if (Test-Path $homeAlpha) {
    Remove-Item -Recurse -Force $homeAlpha
    Write-Host "Removed $homeAlpha"
}

# --- Remove config directory (~/.config/alphacode) ---
$configDir = Join-Path $Home '.config\alphacode'
if (Test-Path $configDir) {
    Remove-Item -Recurse -Force $configDir
    Write-Host "Removed $configDir"
}

# --- Remove hotkey startup shortcut ---
if ($AppData) {
    $hotkeyLnk = Join-Path $AppData 'Microsoft\Windows\Start Menu\Programs\Startup\alphacode-hotkey.lnk'
    if (Test-Path $hotkeyLnk) {
        Remove-Item -Force $hotkeyLnk
        Write-Host "Removed $hotkeyLnk"
    }
}

# --- Purge: also remove ~/.local/bin leftover ---
if ($Purge) {
    $localBin = Join-Path $Home '.local\bin\alphacode.exe'
    if (Test-Path $localBin) {
        Remove-Item -Force $localBin
        Write-Host "Removed $localBin"
    }
}

Write-Host ""
Write-Host "Alphacode has been uninstalled."
