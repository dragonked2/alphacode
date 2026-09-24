<#
.SYNOPSIS
  Native Windows installer for the /bugbounty skill tools.

.DESCRIPTION
  Uses winget (preferred), then scoop, then chocolatey when available.
  Also installs Go-based recon tools via `go install` when Go is present.
  Never invokes WSL. Works on Windows 10/11 and Windows Server with
  PowerShell 5.1+.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1
  powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1 -DryRun
  powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1 -SkipOsPackages
#>
[CmdletBinding()]
param(
    [switch]$DryRun,
    [switch]$Force,
    [switch]$SkipOsPackages,
    [switch]$SkipGo,
    [switch]$VerifyOnly
)

$ErrorActionPreference = 'Continue'
$LogDir = Join-Path $env:HOME '.bugbounty-tools'
if (-not $env:HOME -or $env:HOME -eq '') { $LogDir = Join-Path $env:USERPROFILE '.bugbounty-tools' }
$LogFile = Join-Path $LogDir ("install-{0}.log" -f (Get-Date -Format 'yyyy-MM-dd_HHmmss'))
$WordlistDir = Join-Path $LogDir 'wordlists'

$script:Succeeded = [System.Collections.Generic.List[string]]::new()
$script:Skipped = [System.Collections.Generic.List[string]]::new()
$script:Failed = [System.Collections.Generic.List[string]]::new()

function Write-Log([string]$Message, [string]$Level = 'INFO') {
    $line = "[{0}] {1}: {2}" -f (Get-Date -Format 's'), $Level, $Message
    Write-Host $line
    try {
        if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Path $LogDir -Force | Out-Null }
        Add-Content -LiteralPath $LogFile -Value $line -Encoding UTF8
    } catch { }
}

function Test-Command([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Get-WingetStatus {
    if (-not (Test-Command 'winget')) { return 'missing' }
    try {
        $null = & winget --version 2>$null
        if ($LASTEXITCODE -eq 0) { return 'ready' }
    } catch { }
    return 'broken'
}

function Get-WslPolicy {
    # Never use WSL unless wsl.exe exists AND a distro is Running.
    if (-not (Test-Path (Join-Path $env:SystemRoot 'System32\wsl.exe'))) {
        return 'no-wsl'
    }
    try {
        $out = & "$env:SystemRoot\System32\wsl.exe" -l -v 2>$null | Out-String
        # wsl -l -v may be UTF-16; Out-String usually normalizes
        if ($out -match 'Running') { return 'ready' }
        if ($out -match 'Stopped|Installing') { return 'not-running' }
        if ($out.Trim().Length -gt 0 -and $out -match '\S') { return 'no-distro' }
        return 'no-distro'
    } catch {
        return 'unknown'
    }
}

function Install-WingetPackage([string]$Id, [string]$Label) {
    if ($DryRun) {
        Write-Log "Would winget install --id $Id"
        $script:Skipped.Add("winget:$Label(dry)")
        return
    }
    Write-Log "winget install $Id"
    & winget install --id $Id -e --accept-package-agreements --accept-source-agreements 2>&1 |
        Tee-Object -FilePath $LogFile -Append | Out-Null
    if ($LASTEXITCODE -eq 0 -or $LASTEXITCODE -eq -1978335189) {
        # 0 = success; -1978335189 = already installed (APPINSTALLER_CLI_ERROR_ALREADY_INSTALLED)
        $script:Succeeded.Add("winget:$Label")
    } else {
        # Retry detection: command now present?
        Write-Log "winget exit $LASTEXITCODE for $Id (treating as skip if present)" 'WARN'
        $script:Skipped.Add("winget:$Label")
    }
}

function Install-ScoopPackage([string]$Name) {
    if (-not (Test-Command 'scoop')) {
        $script:Skipped.Add("scoop:$Name(not-installed)")
        return
    }
    if (-not $Force -and (Test-Command $Name)) {
        $script:Skipped.Add("scoop:$Name")
        return
    }
    if ($DryRun) { Write-Log "Would scoop install $Name"; return }
    & scoop install $Name 2>&1 | Tee-Object -FilePath $LogFile -Append | Out-Null
    if ($LASTEXITCODE -eq 0) { $script:Succeeded.Add("scoop:$Name") }
    else { $script:Failed.Add("scoop:$Name") }
}

function Install-ChocoPackage([string]$Name) {
    if (-not (Test-Command 'choco')) {
        $script:Skipped.Add("choco:$Name(not-installed)")
        return
    }
    if (-not $Force -and (Test-Command $Name)) {
        $script:Skipped.Add("choco:$Name")
        return
    }
    if ($DryRun) { Write-Log "Would choco install -y $Name"; return }
    & choco install -y $Name 2>&1 | Tee-Object -FilePath $LogFile -Append | Out-Null
    if ($LASTEXITCODE -eq 0) { $script:Succeeded.Add("choco:$Name") }
    else { $script:Failed.Add("choco:$Name") }
}

function Install-GoTools {
    if ($SkipGo) { $script:Skipped.Add('go:skipped'); return }
    if (-not (Test-Command 'go')) {
        Write-Log 'Go not found — install GoLang.Go via winget, then re-run' 'WARN'
        $script:Skipped.Add('go:not-installed')
        return
    }
    $goTools = @(
        @{ Bin = 'subfinder'; Path = 'github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest' }
        @{ Bin = 'httpx';     Path = 'github.com/projectdiscovery/httpx/cmd/httpx@latest' }
        @{ Bin = 'nuclei';    Path = 'github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest' }
        @{ Bin = 'katana';    Path = 'github.com/projectdiscovery/katana/cmd/katana@latest' }
        @{ Bin = 'dnsx';      Path = 'github.com/projectdiscovery/dnsx/cmd/dnsx@latest' }
        @{ Bin = 'naabu';     Path = 'github.com/projectdiscovery/naabu/v2/cmd/naabu@latest' }
        @{ Bin = 'assetfinder'; Path = 'github.com/tomnomnom/assetfinder@latest' }
        @{ Bin = 'gau';       Path = 'github.com/lc/gau/v2/cmd/gau@latest' }
        @{ Bin = 'waybackurls'; Path = 'github.com/tomnomnom/waybackurls@latest' }
        @{ Bin = 'anew';      Path = 'github.com/tomnomnom/anew@latest' }
        @{ Bin = 'ffuf';      Path = 'github.com/ffuf/ffuf/v2@latest' }
        @{ Bin = 'gobuster';  Path = 'github.com/OJ/gobuster/v3@latest' }
        @{ Bin = 'dalfox';    Path = 'github.com/hahwul/dalfox/v2@latest' }
        @{ Bin = 'subzy';     Path = 'github.com/Pentestify/subzy@latest' }
        @{ Bin = 'arjun';     Path = 'github.com/s0md3v/Arjun@latest' }
        @{ Bin = 'httprobe';  Path = 'github.com/tomnomnom/httprobe@latest' }
        @{ Bin = 'qsreplace'; Path = 'github.com/tomnomnom/qsreplace@latest' }
        @{ Bin = 'gf';        Path = 'github.com/tomnomnom/gf@latest' }
        @{ Bin = 'trufflehog'; Path = 'github.com/trufflesecurity/trufflehog/v3@latest' }
        @{ Bin = 'gitleaks';  Path = 'github.com/gitleaks/gitleaks/v8@latest' }
    )
    foreach ($t in $goTools) {
        if (-not $Force -and (Test-Command $t.Bin)) {
            $script:Skipped.Add("go:$($t.Bin)")
            continue
        }
        if ($DryRun) {
            Write-Log "Would go install -v $($t.Path)"
            continue
        }
        Write-Log "go install $($t.Bin)"
        & go install -v $t.Path 2>&1 | Tee-Object -FilePath $LogFile -Append | Out-Null
        if ($LASTEXITCODE -eq 0) { $script:Succeeded.Add("go:$($t.Bin)") }
        else { $script:Failed.Add("go:$($t.Bin)") }
    }
    # Ensure GOPATH\bin on user PATH
    $goBin = $env:GOBIN
    if (-not $goBin) {
        $gopath = & go env GOPATH 2>$null
        if ($gopath) { $goBin = Join-Path $gopath 'bin' }
    }
    if ($goBin -and (Test-Path $goBin)) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if ($userPath -notlike "*$goBin*") {
            if ($DryRun) {
                Write-Log "Would add $goBin to user PATH"
            } else {
                [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ';' + $goBin), 'User')
                Write-Log "Added $goBin to user PATH (restart shell to pick up)"
            }
        }
    }
}

function Install-PipPackages {
    $py = $null
    if (Test-Command 'python') { $py = 'python' }
    elseif (Test-Command 'python3') { $py = 'python3' }
    if (-not $py) {
        Write-Log 'Python not found — skip pip packages (winget install Python.Python.3.12)' 'WARN'
        $script:Skipped.Add('pip:not-installed')
        return
    }
    $pkgs = @('sqlmap', 'arjun', 'httpie', 'wafw00f', 'requests', 'semgrep')
    foreach ($pkg in $pkgs) {
        if ($DryRun) { Write-Log "Would $py -m pip install --user $pkg"; continue }
        & $py -m pip install --user $pkg 2>&1 | Tee-Object -FilePath $LogFile -Append | Out-Null
        if ($LASTEXITCODE -eq 0) { $script:Succeeded.Add("pip:$pkg") }
        else { $script:Failed.Add("pip:$pkg") }
    }
}

function Install-Wordlists {
    if (-not (Test-Path $WordlistDir)) {
        if ($DryRun) { Write-Log "Would create $WordlistDir" }
        else { New-Item -ItemType Directory -Path $WordlistDir -Force | Out-Null }
    }
    $items = @(
        @{ Url = 'https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/Web-Content/common.txt'; Name = 'common.txt' }
        @{ Url = 'https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/Web-Content/api/api-endpoints.txt'; Name = 'api-endpoints.txt' }
        @{ Url = 'https://raw.githubusercontent.com/danielmiessler/SecLists/master/Discovery/DNS/subdomains-top1million-5000.txt'; Name = 'subdomains-top1m.txt' }
    )
    foreach ($item in $items) {
        $dest = Join-Path $WordlistDir $item.Name
        if (-not $Force -and (Test-Path $dest) -and ((Get-Item $dest).Length -gt 0)) {
            $script:Skipped.Add("wl:$($item.Name)")
            continue
        }
        if ($DryRun) { Write-Log "Would fetch $($item.Name)"; continue }
        try {
            Invoke-WebRequest -Uri $item.Url -OutFile $dest -TimeoutSec 60 -UseBasicParsing
            $script:Succeeded.Add("wl:$($item.Name)")
            Write-Log "wordlist: $($item.Name)"
        } catch {
            Write-Log "wordlist failed: $($item.Name) — $($_.Exception.Message)" 'WARN'
            $script:Failed.Add("wl:$($item.Name)")
        }
    }
}

function Test-Readiness {
    $checks = @(
        'subfinder', 'httpx', 'nuclei', 'katana', 'naabu', 'dnsx',
        'assetfinder', 'gau', 'waybackurls', 'anew', 'ffuf', 'gobuster',
        'dalfox', 'arjun', 'qsreplace', 'gf', 'jq', 'nmap', 'sqlmap',
        'nikto', 'curl', 'wget', 'whatweb', 'wafw00f', 'semgrep',
        'trufflehog', 'gitleaks', 'go', 'python'
    )
    $found = @(); $missing = @()
    foreach ($c in $checks) {
        if (Test-Command $c) { $found += $c } else { $missing += $c }
    }
    $winget = Get-WingetStatus
    $wsl = Get-WslPolicy
    Write-Host ""
    Write-Host "Platform: windows-native | winget: $winget | WSL: $wsl"
    Write-Host "Found: $($found.Count)  Missing: $($missing.Count)"
    if ($missing.Count -gt 0) {
        Write-Host "Missing:"
        $missing | ForEach-Object { Write-Host "  - $_" }
        Write-Host ""
        Write-Host "Install: powershell -ExecutionPolicy Bypass -File scripts/install_bugbounty_tools.ps1"
        Write-Host "   or:  bash scripts/install_bugbounty_tools.sh all   # Git Bash"
    }
    if ($wsl -ne 'ready') {
        Write-Host "WSL not ready ($wsl) — this installer never uses WSL."
    }
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
Write-Log "=== bugbounty Windows installer (DryRun=$DryRun Force=$Force) ==="
Write-Log "User: $env:USERNAME  PS: $($PSVersionTable.PSVersion)"
$wslPolicy = Get-WslPolicy
Write-Log "WSL policy: $wslPolicy (only 'ready' would allow WSL; we do not use it)"

if ($VerifyOnly) {
    Test-Readiness
    exit 0
}

if (-not $SkipOsPackages) {
    $winget = Get-WingetStatus
    if ($winget -eq 'ready') {
        # Core OS tools (Go/Python/Nmap/jq/curl/Git)
        Install-WingetPackage -Id 'GoLang.Go' -Label 'go'
        Install-WingetPackage -Id 'Python.Python.3.12' -Label 'python'
        Install-WingetPackage -Id 'Insecure.Nmap' -Label 'nmap'
        Install-WingetPackage -Id 'Microsoft jq' -Label 'jq'
        Install-WingetPackage -Id 'cURL.cURL' -Label 'curl'
        Install-WingetPackage -Id 'Git.Git' -Label 'git'
    } else {
        Write-Log "winget not ready ($winget) — trying scoop/choco" 'WARN'
        Install-ScoopPackage 'go'
        Install-ScoopPackage 'python'
        Install-ScoopPackage 'nmap'
        Install-ScoopPackage 'jq'
        Install-ScoopPackage 'curl'
        Install-ScoopPackage 'git'
        Install-ChocoPackage 'golang'
        Install-ChocoPackage 'python'
        Install-ChocoPackage 'nmap'
        Install-ChocoPackage 'jq'
        Install-ChocoPackage 'curl'
        Install-ChocoPackage 'git'
    }
}

Install-GoTools
Install-PipPackages
Install-Wordlists

Write-Host ""
Write-Host "========================================"
Write-Host " Windows Install Summary"
Write-Host "========================================"
Write-Host " Installed: $($script:Succeeded.Count)"
Write-Host " Skipped:   $($script:Skipped.Count)"
Write-Host " Failed:    $($script:Failed.Count)"
if ($script:Failed.Count -gt 0) {
    Write-Host " Failed:"
    $script:Failed | ForEach-Object { Write-Host "   - $_" }
}
Write-Host " Log: $LogFile"
Write-Host "========================================"
Write-Host ""
Write-Host "Post-install readiness:"
Test-Readiness

if ($script:Failed.Count -gt 0) { exit 1 }
exit 0
