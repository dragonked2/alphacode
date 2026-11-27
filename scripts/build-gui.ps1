<#
.SYNOPSIS
    Builds the AlphaCode Desktop GUI (React + Vite frontend embedded into `alphacode`).
.DESCRIPTION
    1. Runs `npm run build` inside `src/cli/gui/ui` so `build.rs` embeds `ui/dist` into `alphacode`.
    2. Configures MSVC build environment on Windows if `rc.exe` is not already in PATH.
    3. Builds the single `alphacode` binary (`cargo build --bin alphacode`).
    4. Optionally launches `alphacode --gui` when `-Launch` is specified.
#>
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$Launch
)

$ErrorActionPreference = 'Stop'
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

Write-Host "[1/3] Building frontend UI (src/cli/gui/ui)..." -ForegroundColor Yellow
$uiDir = Join-Path $repoRoot 'src\cli\gui\ui'
Push-Location $uiDir
try {
    if (-not (Test-Path 'node_modules')) {
        if (Test-Path 'package-lock.json') {
            npm ci
            if ($LASTEXITCODE -ne 0) {
                throw "npm ci failed with exit code $LASTEXITCODE"
            }
        }
        else {
            npm install
            if ($LASTEXITCODE -ne 0) {
                throw "npm install failed with exit code $LASTEXITCODE"
            }
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        throw "Frontend build failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

Write-Host "[2/3] Configuring MSVC and Cargo environment..." -ForegroundColor Yellow
if (-not (Get-Command rc.exe -ErrorAction SilentlyContinue)) {
    $vsRoots = @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Professional",
        "${env:ProgramFiles}\Microsoft Visual Studio\2022\Enterprise",
        "${env:ProgramFiles}\Microsoft Visual Studio\18\Community"
    )
    foreach ($root in $vsRoots) {
        $vcBat = Join-Path $root 'VC\Auxiliary\Build\vcvars64.bat'
        if (Test-Path $vcBat) {
            cmd /c "`"$vcBat`" >nul 2>&1 && set" | ForEach-Object {
                if ($_ -match '^([^=]+)=(.*)$') {
                    [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
                }
            }
            break
        }
    }
}

if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = Join-Path ([System.IO.Path]::GetTempPath()) 'alphacode-target'
}

$profile = if ($Release) { 'release' } else { 'debug' }
Write-Host "[3/3] Building alphacode ($profile)..." -ForegroundColor Yellow
$cargoArgs = @('build', '--bin', 'alphacode')
if ($Release) {
    $cargoArgs += '--release'
}
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE"
}

$builtExe = Join-Path $env:CARGO_TARGET_DIR "$profile\alphacode.exe"
if (-not (Test-Path $builtExe)) {
    throw "Expected binary not found at: $builtExe"
}

$releaseDir = Join-Path $repoRoot 'release'
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null

# Stop any running AlphaCode server/app process inside release\ or installed AppData channels
$stopPrefixes = @(
    ([System.IO.Path]::GetFullPath($releaseDir)).TrimEnd('\') + '\',
    (Join-Path $env:LOCALAPPDATA 'alphacode\builds\'),
    (Join-Path $env:LOCALAPPDATA 'Programs\alphacode\')
)
Get-Process -ErrorAction SilentlyContinue | Where-Object {
    try {
        $procPath = $_.Path
        $procPath -and ($stopPrefixes | Where-Object { $procPath.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase) } | Select-Object -First 1)
    } catch { $false }
} | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

# Remove legacy stale binaries/assets from earlier prototype layouts
foreach ($stale in @('AlphaCode-Desktop.exe', 'alphacode-desktop.exe', 'AlphaCode.exe')) {
    $stalePath = Join-Path $releaseDir $stale
    if (Test-Path $stalePath) {
        Remove-Item -Force $stalePath
    }
}
$staleUi = Join-Path $releaseDir 'ui'
if (Test-Path $staleUi) {
    Remove-Item -Recurse -Force $staleUi
}

$stagedExe = Join-Path $releaseDir 'alphacode.exe'
Copy-Item $builtExe $stagedExe -Force

foreach ($hostName in @('AlphaCode.Desktop.exe', 'AlphaCode.Legacy.exe')) {
    $builtHost = Join-Path $env:CARGO_TARGET_DIR "$profile\$hostName"
    if (Test-Path $builtHost) {
        Copy-Item $builtHost (Join-Path $releaseDir $hostName) -Force
    }
}

Write-Host "Build complete: $stagedExe" -ForegroundColor Green
if ($Launch) {
    Write-Host "Launching AlphaCode GUI..." -ForegroundColor Cyan
    & $stagedExe --gui
}
