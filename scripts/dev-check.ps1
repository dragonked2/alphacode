# Windows verification entry point. Run from any directory with PowerShell 5.1+.
[CmdletBinding()]
param(
    [ValidateSet('all', 'fmt', 'check', 'test', 'integration', 'clippy')]
    [string]$Gate = 'all'
)

$ErrorActionPreference = 'Stop'
$repoPath = Split-Path -Parent $PSScriptRoot
$vcPath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path -LiteralPath $vcPath)) {
    throw "VS 2022 Build Tools are required: $vcPath"
}

cmd /c "`"$vcPath`" >nul 2>&1 && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
    }
}
if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the MSVC environment' }
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'Temp\alphacode-target'

function Invoke-CargoGate {
    param([string[]]$CargoArguments)
    Write-Host "cargo $($CargoArguments -join ' ')"
    & cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArguments -join ' ') failed (exit $LASTEXITCODE)"
    }
}

Push-Location -LiteralPath $repoPath
try {
    # Preserve accessibility settings. Unit buffer fixtures are deterministic;
    # integration tests verify the production environment in isolated processes.
    if ($Gate -in @('all', 'fmt')) {
        Invoke-CargoGate @('fmt', '--all', '--', '--check')
    }
    if ($Gate -in @('all', 'check')) {
        Invoke-CargoGate @('check', '--workspace', '--locked')
        Invoke-CargoGate @('check', '--workspace', '--tests', '--locked')
    }
    if ($Gate -in @('all', 'test')) {
        Invoke-CargoGate @('test', '--lib', '--locked')
        Invoke-CargoGate @('test', '-p', 'alphagrep', '--locked')
    }
    if ($Gate -in @('all', 'integration')) {
        Invoke-CargoGate @('test', '--test', 'provider_request_audit', '--locked')
        Invoke-CargoGate @('test', '--test', 'render_preferences', '--locked')
        Invoke-CargoGate @('test', '--test', 'llamacpp_compat', '--locked')
    }
    if ($Gate -in @('all', 'clippy')) {
        Invoke-CargoGate @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings')
    }
} finally {
    Pop-Location
}
