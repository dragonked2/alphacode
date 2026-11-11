# Native Windows Desktop verification. The host is copied byte-for-byte; only
# the bridge and settings are fixtures. No provider or installed app is used.
param(
    [Parameter(Mandatory=$true)][string]$HostPath,
    [ValidateRange(0,120)][int]$DurationMinutes = 30,
    [ValidateRange(1,20)][int]$LaunchCount = 5
)
$ErrorActionPreference = 'Stop'
$artifactRoot = if ($env:ALPHACODE_SCRATCH_DIR) { $env:ALPHACODE_SCRATCH_DIR } else { [IO.Path]::GetTempPath() }
$testRoot = Join-Path $artifactRoot ('alphacode-gui-host-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$hostExe = Join-Path $testRoot 'AlphaCode.Desktop.exe'
[IO.File]::Copy([IO.Path]::GetFullPath($HostPath),$hostExe)
$hostHash = (Get-FileHash -LiteralPath $hostExe -Algorithm SHA256).Hash.ToLowerInvariant()
$assembly = [Reflection.Assembly]::LoadFile($hostExe)
$reader = [IO.StreamReader]::new($assembly.GetManifestResourceStream('AlphaCode.UI'))
try { $embedded = $reader.ReadToEnd() } finally { $reader.Dispose() }
$production = [IO.File]::ReadAllText([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../src/cli/gui/gui.ps1')))
if ($embedded -cne $production) { throw 'The supplied host does not embed the current production GUI source' }
$sha = [Security.Cryptography.SHA256]::Create()
try { $scriptHash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($embedded))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
$identity = ([Diagnostics.FileVersionInfo]::GetVersionInfo($hostExe)).ProductVersion
$manifest = [IO.File]::ReadAllText([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../Cargo.toml')))
$versionMatch = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"')
if (-not $versionMatch.Success) { throw 'Could not read the package version from Cargo.toml' }
$expectedVersion = $versionMatch.Groups[1].Value
if ($identity -notmatch ('^v?' + [regex]::Escape($expectedVersion) + '(?:\b|-)')) { throw ('Host identity does not match Cargo.toml version ' + $expectedVersion + ': ' + $identity) }
$bridgeExe = Join-Path $testRoot 'FixtureBridge.exe'
$bridgeSource = Join-Path $testRoot 'FixtureBridge.cs'
[IO.File]::WriteAllText($bridgeSource,@'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
class FixtureBridge {
 static void Main() {
  var root = Environment.GetEnvironmentVariable("ALPHACODE_GUI_HOST_CASE");
  var mode = Environment.GetEnvironmentVariable("ALPHACODE_GUI_HOST_MODE");
  var process = Process.GetCurrentProcess();
  File.AppendAllText(Path.Combine(root,"bridge-starts.log"), process.Id + "\n");
  string line;
  while ((line = Console.ReadLine()) != null) {
   File.AppendAllText(Path.Combine(root,"requests.log"), DateTime.UtcNow.ToString("o") + " " + line + "\n");
   var marker = Path.Combine(root,"failed-once");
   if (mode != "stable" && !File.Exists(marker)) {
    File.WriteAllText(marker,"fixture");
    if (mode == "eof") return;
    if (mode == "timeout") { Thread.Sleep(Timeout.Infinite); return; }
    Console.WriteLine("{fixture-malformed api_key=fixture-native-secret");
    continue;
   }
   if (line.Contains("\"sessions\"")) Console.WriteLine("{\"ok\":true,\"stored\":{\"sessions\":[{\"id\":\"saved-1\",\"title\":\"Fixture saved chat\",\"detail\":\"1 message\",\"saved\":true,\"source\":\"alphacode\"}],\"session_count\":1,\"saved_count\":1,\"returned\":1}}");
   else if (line.Contains("\"transcript\"")) Console.WriteLine("{\"ok\":true,\"session_id\":\"project-1\",\"transcript\":{\"message_count\":1,\"messages\":[{\"role\":\"assistant\",\"text\":\"Fixture conversation\",\"tools\":[]}]}}");
   else Console.WriteLine("{\"ok\":true,\"sessions\":[{\"id\":\"__companion__\",\"title\":\"Fixture chat\",\"state\":\"ready\",\"detail\":\"Ready\"},{\"id\":\"project-1\",\"working_dir\":\"C:/fixture/project\",\"title\":\"Fixture project\",\"state\":\"ready\",\"detail\":\"Ready\"}],\"working\":0,\"attention\":0,\"offline\":0,\"connected_servers\":1,\"owned_busy\":false,\"model\":\"test-a\",\"effort\":\"none\",\"models\":[\"test-a\"],\"reply\":\"\"}");
  }
 }
}
'@)
$compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $compiler /nologo /target:exe "/out:$bridgeExe" $bridgeSource
if ($LASTEXITCODE -ne 0) { throw 'Could not compile the isolated bridge' }
$oldEnvironment = @{}
foreach ($name in @('LOCALAPPDATA','ALPHACODE_GUI_HOST_CASE','ALPHACODE_GUI_HOST_MODE')) { $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name) }
$script:gui = $null
$script:caseRoot = $null
$results = [Collections.Generic.List[object]]::new()

function Assert-Alive {
    $script:gui.Refresh()
    if ($script:gui.HasExited) { throw ('Desktop exited early, code=' + $script:gui.ExitCode) }
    if ($script:gui.MainWindowTitle -ne 'AlphaCode' -or -not $script:gui.Responding) { throw 'Desktop window is missing or nonresponsive' }
}
function Get-Logs { return @(Get-ChildItem -LiteralPath (Join-Path $script:caseRoot 'AlphaCode') -Filter 'gui-*.log' | Where-Object { $_.Length -gt 0 }) }
function Assert-NoLogs { if (@(Get-Logs).Count) { throw ('Unexpected diagnostic log in ' + $script:caseRoot) } }
function Start-Case([string]$name,[string]$mode) {
    $script:caseRoot = Join-Path $testRoot $name
    [IO.Directory]::CreateDirectory((Join-Path $script:caseRoot 'AlphaCode')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $script:caseRoot 'AlphaCode/gui.json'),'{"theme":"light","left":20,"top":20}')
    $env:LOCALAPPDATA = $script:caseRoot
    $env:ALPHACODE_GUI_HOST_CASE = $script:caseRoot
    $env:ALPHACODE_GUI_HOST_MODE = $mode
    $script:gui = Start-Process $hostExe -ArgumentList @('-Executable',('"' + $bridgeExe + '"')) -PassThru
    $wait = [Diagnostics.Stopwatch]::StartNew()
    do {
        Start-Sleep -Milliseconds 250
        $script:gui.Refresh()
        if ($script:gui.HasExited) { throw 'Desktop failed to launch (another live Desktop may hold its normal mutex)' }
    } while ($script:gui.MainWindowTitle -ne 'AlphaCode' -and $wait.Elapsed.TotalSeconds -lt 20)
    Assert-Alive
}
function Get-BridgeProcesses {
    $path = Join-Path $script:caseRoot 'bridge-starts.log'
    if (-not [IO.File]::Exists($path)) { return }
    foreach ($idText in [IO.File]::ReadAllLines($path)) {
        $owned = Get-Process -Id ([int]$idText) -ErrorAction SilentlyContinue
        if ($owned -and $owned.Path -eq $bridgeExe) { $owned }
    }
}
function Close-Case {
    if (-not $script:gui.CloseMainWindow() -or -not $script:gui.WaitForExit(10000) -or $script:gui.ExitCode -ne 0) { throw 'Desktop did not close gracefully with exit 0' }
    Start-Sleep -Milliseconds 250
    if (@(Get-BridgeProcesses).Count) { throw 'Desktop close leaked a fixture bridge process' }
    $script:gui.Dispose(); $script:gui = $null
}
function Add-Result([string]$name,$details) {
    $results.Add([pscustomobject]@{test=$name;utc=[DateTime]::UtcNow.ToString('o');details=$details})
    Write-Output ('ALPHACODE_CHECKPOINT ' + (@{message=('PASS: ' + $name)} | ConvertTo-Json -Compress))
}

try {
    for ($launch = 1; $launch -le $LaunchCount; $launch++) {
        Start-Case ('launch-' + $launch) 'stable'
        Start-Sleep -Seconds 3
        Assert-Alive; Assert-NoLogs
        $requests = [IO.File]::ReadAllText((Join-Path $script:caseRoot 'requests.log'))
        if (-not $requests.Contains('"sessions"') -or -not $requests.Contains('"status"')) { throw 'The native host did not poll the fake bridge' }
        Close-Case
        Add-Result ('repeated-launch-' + $launch) @{exit_code=0;bridge_leaked=$false}
    }
    foreach ($mode in @('malformed','eof','timeout')) {
        Start-Case $mode $mode
        $wait = [Diagnostics.Stopwatch]::StartNew()
        do {
            Start-Sleep -Milliseconds 500
            Assert-Alive
            $starts = Join-Path $script:caseRoot 'bridge-starts.log'
            $recovered = [IO.File]::Exists($starts) -and [IO.File]::ReadAllLines($starts).Count -ge 2
        } while (-not $recovered -and $wait.Elapsed.TotalSeconds -lt 35)
        if (-not $recovered) { throw ('No reconnect after ' + $mode) }
        Start-Sleep -Seconds 3
        $logs = @(Get-Logs)
        if ($logs.Count -ne 1 -or $logs[0].Name -ne 'gui-bridge.log') { throw ('Unexpected diagnostic categories during ' + $mode) }
        $content = [IO.File]::ReadAllText($logs[0].FullName)
        if ($content.Contains('fixture-native-secret')) { throw 'Native bridge diagnostics leaked the fake API key' }
        $records = @([IO.File]::ReadAllLines($logs[0].FullName) | ForEach-Object { $_ | ConvertFrom-Json })
        if ($records.Count -ne 1 -or $records[0].callback -ne 'MainTimer.Tick' -or $records[0].gui_sha256 -ne $scriptHash -or -not $records[0].exception.script_stack) { throw ('Lost native callback identity/stack or a retry loop in ' + $mode) }
        Close-Case
        Add-Result ('bridge-recovery-' + $mode) @{seconds=$wait.Elapsed.TotalSeconds;callback=$records[0].callback;exception_type=$records[0].exception.type}
        $bundle = Join-Path $script:caseRoot 'offline-export'
        $collector = Start-Process $hostExe -ArgumentList @('-CollectDiagnostics',('"' + $bundle + '"')) -PassThru
        if (-not $collector.WaitForExit(10000) -or $collector.ExitCode -ne 0) { throw 'Offline native collector failed' }
        $manifest = [IO.File]::ReadAllText((Join-Path $bundle 'build-info.json')) | ConvertFrom-Json
        if ($manifest.gui_sha256 -ne $scriptHash -or [IO.File]::Exists((Join-Path $bundle 'gui.json'))) { throw 'Offline collector omitted source identity or included settings' }
        foreach ($line in [IO.File]::ReadLines((Join-Path $bundle 'gui-bridge.log'))) { $line | ConvertFrom-Json | Out-Null }
        $collector.Dispose()
    }
    if ($DurationMinutes -gt 0) {
        Start-Case 'unattended' 'stable'
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $cpuBefore = $script:gui.TotalProcessorTime.TotalSeconds
        $firstMemory = $script:gui.PrivateMemorySize64
        $peakMemory = $firstMemory
        $samples = 0
        do {
            Start-Sleep -Seconds 5
            Assert-Alive; Assert-NoLogs
            $samples++
            $peakMemory = [Math]::Max($peakMemory,$script:gui.PrivateMemorySize64)
            if ($samples % 12 -eq 0) {
                Write-Output ('ALPHACODE_PROGRESS ' + (@{current=[Math]::Floor($clock.Elapsed.TotalSeconds);total=($DurationMinutes*60);unit='seconds';message='Unmodified Desktop alive, responsive, polling; no exception logs'} | ConvertTo-Json -Compress))
            }
        } while ($clock.Elapsed.TotalSeconds -lt $DurationMinutes*60)
        $requestCount = [IO.File]::ReadAllLines((Join-Path $script:caseRoot 'requests.log')).Count
        if ($requestCount -lt $DurationMinutes*20) { throw 'The unattended window survived but did not keep polling' }
        $stats = @{elapsed_seconds=$clock.Elapsed.TotalSeconds;samples=$samples;requests=$requestCount;cpu_seconds=($script:gui.TotalProcessorTime.TotalSeconds-$cpuBefore);initial_private_bytes=$firstMemory;final_private_bytes=$script:gui.PrivateMemorySize64;peak_private_bytes=$peakMemory;unexpected_logs=0}
        Close-Case
        Add-Result 'unattended-stability' $stats
    }
    if ((Get-FileHash -LiteralPath $hostExe -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hostHash) { throw 'The artifact changed during verification' }
    $report = @{host_sha256=$hostHash;gui_sha256=$scriptHash;build=$identity;completed_utc=[DateTime]::UtcNow.ToString('o');duration_minutes=$DurationMinutes;results=$results.ToArray();scope='unmodified native host; isolated fake bridge; no live provider/session'}
    [IO.File]::WriteAllText((Join-Path $testRoot 'verification.json'),($report | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
    Write-Output ('Native GUI verification passed. Evidence: ' + $testRoot)
} finally {
    if ($script:gui -and -not $script:gui.HasExited) { Stop-Process -Id $script:gui.Id -ErrorAction SilentlyContinue }
    if ($script:caseRoot) { foreach ($owned in @(Get-BridgeProcesses)) { Stop-Process -Id $owned.Id -ErrorAction SilentlyContinue } }
    foreach ($name in $oldEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name,$oldEnvironment[$name],'Process') }
}
