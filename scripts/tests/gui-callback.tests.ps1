# Windows PowerShell 5.1 regression: exercise the production exception callback,
# not a restatement of its source. No bridge or provider is involved.
param([string]$SourcePath = (Join-Path $PSScriptRoot '../../src/cli/gui/gui.ps1'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName WindowsBase,PresentationFramework
Add-Type -Path (Join-Path $PSScriptRoot '../../src/cli/gui/GuiDiagnostics.cs') -ReferencedAssemblies @([Management.Automation.PSObject].Assembly.Location,'System.Web.Extensions','System.Core')
$artifactRoot = if ($env:ALPHACODE_SCRATCH_DIR) { $env:ALPHACODE_SCRATCH_DIR } else { [IO.Path]::GetTempPath() }
$testRoot = Join-Path $artifactRoot ('alphacode-gui-callback-tests-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$oldLocalApp = $env:LOCALAPPDATA
$env:LOCALAPPDATA = $testRoot
$source = [IO.File]::ReadAllText([IO.Path]::GetFullPath($SourcePath))
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
$registration = $ast.Find({ param($node) $node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -eq 'add_UnhandledException' -and $node.Expression.Extent.Text -eq '$window.Dispatcher' },$true)
if (-not $registration) { throw 'Production Dispatcher exception handler not found' }
foreach ($name in @('Get-GuiDiagnosticState','Write-GuiFailure','Show-GuiFailureNotice','Invoke-GuiCallback','Stop-GuiResources')) {
    $definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name },$true)
    if ($definition) { . ([scriptblock]::Create($definition.Extent.Text)) }
}
$handlerArgument = $registration.Arguments[0]
if ($handlerArgument -is [Management.Automation.Language.VariableExpressionAst]) {
    $assignment = $ast.Find({ param($node) $node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -eq $handlerArgument.Extent.Text },$true)
    $handlerArgument = $assignment.Right.Find({ param($node) $node -is [Management.Automation.Language.ScriptBlockExpressionAst] },$true)
}
$handlerText = $handlerArgument.ScriptBlock.Extent.Text
$handler = [scriptblock]::Create($handlerText.Substring(1,$handlerText.Length-2))
$dispatcher = [Windows.Threading.Dispatcher]::CurrentDispatcher
[AlphaCode.Desktop.GuiDiagnostics]::Initialize([AlphaCode.Desktop.GuiDiagnostics].Assembly,$source)
$controls = @{}
Add-Type -ReferencedAssemblies ([Windows.Threading.Dispatcher].Assembly.Location) -TypeDefinition @'
using System;
using System.Windows.Threading;
public static class GuiCallbackFixture {
 public static void Raise() { throw new InvalidOperationException("fixture-original-idle-callback"); }
 public static Action Callback() { return Raise; }
}
'@
try {
    $dispatcher.add_UnhandledException($handler)
    try {
        $dispatcher.Invoke([Windows.Threading.DispatcherPriority]::Normal,[Delegate][GuiCallbackFixture]::Callback()) | Out-Null
    } finally {
        $dispatcher.remove_UnhandledException($handler)
    }
    $log = Join-Path $testRoot 'AlphaCode/gui-dispatcher.log'
    if (-not [IO.File]::Exists($log)) { throw 'Dispatcher exception was not diagnosed' }
    $details = [IO.File]::ReadAllText($log)
    if (-not $details.Contains('fixture-original-idle-callback') -or -not $details.Contains('System.InvalidOperationException') -or -not $details.Contains('GuiCallbackFixture.Raise')) { throw ('Original exception/type/stack was lost: ' + $details) }
    Write-Output 'PASS: production Dispatcher handler receives the real event args and preserves the original exception.'
    $record = ($details.Trim() -split "`r?`n" | Select-Object -Last 1) | ConvertFrom-Json
    if (-not $record.timestamp_utc -or $record.callback -ne 'Dispatcher.UnhandledException' -or $record.gui_sha256.Length -ne 64) { throw 'Missing diagnostic timestamp/callback/build identity' }

    $failureTimer = [Windows.Threading.DispatcherTimer]::new()
    $failureTimer.Start()
    Invoke-GuiCallback 'ResponseTimer.Tick' { throw [InvalidOperationException]::new('fixture-timer-failure',[ArgumentException]::new('fixture-inner-error')) } $failureTimer
    if ($failureTimer.IsEnabled) { throw 'A failed periodic callback was left repeating' }
    $callbackLog = [IO.File]::ReadAllText((Join-Path $testRoot 'AlphaCode/gui-callback.log'))
    $callbackRecord = ($callbackLog.Trim() -split "`r?`n" | Select-Object -Last 1) | ConvertFrom-Json
    if ($callbackRecord.callback -ne 'ResponseTimer.Tick' -or -not $callbackLog.Contains('fixture-inner-error') -or -not $callbackRecord.exception.script_stack) { throw 'Timer callback/original PowerShell inner/stack diagnostics were lost' }
    Write-Output 'PASS: named timer callback retains PowerShell error/inner/stack and stops the failed timer.'

    $privatePrompt = 'private-composer-fixture'
    $controls.Prompt = [Windows.Controls.TextBox]::new(); $controls.Prompt.Text = $privatePrompt
    Invoke-GuiCallback 'Fixture.Redaction' { throw ('api_key=fixture-api-secret password=fixture-password Bearer fixture-bearer-secret user@example.com C:\Users\Private\data.txt ' + $privatePrompt) }
    $callbackLog = [IO.File]::ReadAllText((Join-Path $testRoot 'AlphaCode/gui-callback.log'))
    foreach ($secret in @('fixture-api-secret','fixture-password','fixture-bearer-secret','user@example.com','C:\Users\Private','private-composer-fixture')) {
        if ($callbackLog.Contains($secret)) { throw ('Sensitive diagnostic data was not redacted: ' + $secret) }
    }
    $state = @{ pending_kind='status'; queue_count=2; prompt='never-log-payload'; api_key='never-log-key' }
    [AlphaCode.Desktop.GuiDiagnostics]::Write('callback','Fixture.State',[Exception]::new('state-test'),$state,$null)
    $stateLog = [IO.File]::ReadAllText((Join-Path $testRoot 'AlphaCode/gui-callback.log'))
    if ($stateLog.Contains('never-log')) { throw 'Diagnostics serialized non-allowlisted state' }
    Write-Output 'PASS: API key/password/bearer/email/path/composer redaction and state allowlisting.'
    $redacted = [AlphaCode.Desktop.GuiDiagnostics]::Redact('password="fixture multi word password" api_key=''fixture key with spaces'' token=fixture-query-token&next=1 zz',$null)
    foreach ($value in @('fixture multi word password','fixture key with spaces','fixture-query-token')) { if ($redacted.Contains($value)) { throw 'Quoted/query secret leaked' } }
    if ([AlphaCode.Desktop.GuiDiagnostics]::Redact('short-secret=zz',@('zz')).Contains('zz')) { throw 'Short known secret leaked' }

    $env:LOCALAPPDATA = Join-Path $testRoot 'unwritable'
    [IO.File]::WriteAllText($env:LOCALAPPDATA,'fixture')
    $dispatcher.add_UnhandledException($handler)
    try { $dispatcher.Invoke([Windows.Threading.DispatcherPriority]::Normal,[Delegate][GuiCallbackFixture]::Callback()) | Out-Null }
    finally { $dispatcher.remove_UnhandledException($handler); $env:LOCALAPPDATA = $testRoot }
    Write-Output 'PASS: failed log I/O does not create another Dispatcher exception.'

    if (-not [AlphaCode.Desktop.GuiDiagnostics]::IsFatal([OutOfMemoryException]::new()) -or [AlphaCode.Desktop.GuiDiagnostics]::IsFatal([InvalidOperationException]::new())) { throw 'Fatal/recoverable classification regressed' }
    $propagated = $false
    try { Invoke-GuiCallback 'Fixture.Fatal' { throw [OutOfMemoryException]::new('fixture-fatal-exception') } }
    catch { $propagated = [AlphaCode.Desktop.GuiDiagnostics]::IsFatal($_.Exception) }
    if (-not $propagated) { throw 'The callback wrapper swallowed a fatal exception' }
    $script:closed = $true
    Invoke-GuiCallback 'Fixture.Closed' { throw 'callback-ran-after-close' }
    $script:closed = $false
    $timer = [Windows.Threading.DispatcherTimer]::new(); $timer.Start()
    Stop-GuiResources
    if (-not $script:closed -or $timer.IsEnabled) { throw 'Close did not stop timer resources' }
    Write-Output 'PASS: fatal classification and close/timer lifecycle.'

    [IO.File]::WriteAllText((Join-Path $testRoot 'AlphaCode/gui.json'),'{"api_key":"excluded-configuration"}')
    $export = [AlphaCode.Desktop.GuiDiagnostics]::Export((Join-Path $testRoot 'export'))
    if (-not [IO.File]::Exists((Join-Path $export 'build-info.json')) -or [IO.File]::Exists((Join-Path $export 'gui.json'))) { throw 'Diagnostic collection included configuration or omitted identity' }
    foreach ($file in Get-ChildItem -LiteralPath $export -Filter '*.log') { foreach ($line in [IO.File]::ReadLines($file.FullName)) { $line | ConvertFrom-Json | Out-Null } }
    [IO.File]::AppendAllText($log,'legacy-unparseable api_key=fixture-legacy-key' + [Environment]::NewLine)
    $export = [AlphaCode.Desktop.GuiDiagnostics]::Export((Join-Path $testRoot 'export'))
    $exported = [IO.File]::ReadAllText((Join-Path $export 'gui-dispatcher.log'))
    if ($exported.Contains('fixture-legacy-key')) { throw 'Unparseable legacy payload leaked during export' }
    foreach ($line in [IO.File]::ReadLines((Join-Path $export 'gui-dispatcher.log'))) { $line | ConvertFrom-Json | Out-Null }
    Write-Output 'PASS: offline collection preserves parseable JSON, excludes configuration and omits unsafe legacy records.'

    $bounded = Join-Path $testRoot 'AlphaCode/gui-callback.log'
    [IO.File]::WriteAllText($bounded,('x' * (1024 * 1024)))
    [AlphaCode.Desktop.GuiDiagnostics]::Write('callback','Fixture.Rotation',[Exception]::new('rotation-test'),$null,$null)
    if (-not [IO.File]::Exists($bounded + '.1') -or ([IO.FileInfo]$bounded).Length -gt 65536) { throw 'Log rotation did not bound the active diagnostic log' }
    Write-Output 'PASS: bounded diagnostic log rotation.'
} catch {
    [Console]::Error.WriteLine('GUI_CALLBACK_REGRESSION_FAILURE ' + $_.Exception.ToString())
    [Console]::Error.WriteLine('PS_STACK ' + $_.ScriptStackTrace)
    $log = Join-Path $testRoot 'AlphaCode/gui-dispatcher.log'
    if ([IO.File]::Exists($log)) { [Console]::Error.WriteLine('DISPATCHER_LOG ' + [IO.File]::ReadAllText($log)) }
    throw
} finally {
    $env:LOCALAPPDATA = $oldLocalApp
    Remove-Item -LiteralPath $testRoot -Recurse -Force
}
