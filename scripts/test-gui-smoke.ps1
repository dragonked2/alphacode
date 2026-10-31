# Windows-only smoke test of the WPF shell and JSON-lines polling. The fake
# bridge has no network access and the window uses an isolated settings folder.
$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('alphacode-gui-smoke-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$fakeSource = Join-Path $testRoot 'Bridge.cs'
$fakeExe = Join-Path $testRoot 'Bridge.exe'
$bridgeLog = Join-Path $testRoot 'requests.log'
$testScript = Join-Path $testRoot 'gui.ps1'
$source = [IO.File]::ReadAllText([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../src/cli/gui/gui.ps1')))
# Keep the smoke test independent of any already-running desktop window.
$source = $source.Replace("if (`$Preview) { `$mutexName += '.Preview' }", "`$mutexName += '.Smoke.' + `$PID")
$interaction = @'
$testStep = 0
$testTimer = New-Object Windows.Threading.DispatcherTimer
$testTimer.Interval = [TimeSpan]::FromSeconds(1)
$testTimer.Add_Tick({
    if ($script:testStep -eq 0 -and -not $controls.ModelPicker.Items.Contains('test-b')) { return }
    $seen = [IO.File]::ReadAllText($env:ALPHACODE_GUI_SMOKE_LOG)
    if ($script:testStep -eq 1 -and (-not $seen.Contains('test-b') -or $script:modelRequestPending)) { return }
    if ($script:testStep -eq 2 -and (-not $seen.Contains('high') -or $script:effortRequestPending)) { return }
    if ($script:testStep -eq 3 -and -not $seen.Contains('Smoke test prompt')) { return }
    if ($script:testStep -eq 4 -and $script:transcriptFor -ne 'saved-1') { return }
    $script:testStep++
    switch ($script:testStep) {
        1 { $controls.ModelPicker.SelectedItem = 'test-b' }
        2 { $controls.EffortPicker.SelectedValue = 'high' }
        3 { $controls.Prompt.Text = 'Smoke test prompt'; $controls.Send.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent)) }
        4 {
            $controls.Sessions.SelectedValue = '__companion__'
            $controls.Prompt.Text = 'Draft for first chat'
            $controls.Sessions.SelectedValue = 'project-1'
            if ($controls.Prompt.Text) { throw 'A draft leaked into another chat' }
            $controls.Prompt.Text = 'Draft for project'
            $controls.Sessions.SelectedValue = '__companion__'
            if ($controls.Prompt.Text -ne 'Draft for first chat') { throw 'First draft was lost' }
            $controls.Sessions.SelectedValue = 'project-1'
            if ($controls.Prompt.Text -ne 'Draft for project') { throw 'Project draft was lost' }
            Set-ReadingSize 30
            if ($controls.Reply.FontSize -ne 24) { throw 'Reading size exceeded its limit' }
            Set-ReadingSize 14
            if ($controls.Prompt.FontSize -ne 15) { throw 'Composer size did not follow reading size' }
            $controls.Reply.Text = ((1..300 | ForEach-Object { "Response line $_" }) -join "`n")
            $controls.ReplyScroll.Visibility = 'Visible'
            $window.UpdateLayout()
            $controls.ReplyScroll.ScrollToHome()
            $window.UpdateLayout()
            if ($controls.JumpLatest.Visibility -ne 'Visible') { throw 'Scrolled history has no return-to-latest control' }
            $controls.JumpLatest.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            $window.UpdateLayout()
            if ($controls.ReplyScroll.VerticalOffset -lt ($controls.ReplyScroll.ScrollableHeight - 8)) { throw 'Jump to latest did not reach the end' }
            if ($controls.Voice.Visibility -ne 'Collapsed') { throw 'Unavailable voice control should be hidden' }
            $controls.SidebarToggle.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($script:sidebarVisible -or $controls.SidebarSurface.Visibility -ne 'Collapsed' -or $controls.SidebarToggle.Content -ne 'Show sidebar') { throw 'Sidebar did not close' }
            $controls.SidebarToggle.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if (-not $script:sidebarVisible -or $controls.SidebarSurface.Visibility -ne 'Visible' -or $controls.SidebarToggle.Content -ne 'Hide sidebar') { throw 'Sidebar did not reopen' }
            $controls.SessionSearch.Text = 'Sample project'
            if ($controls.SearchPlaceholder.Visibility -ne 'Collapsed' -or $controls.Sessions.Items.Count -ne 1 -or $controls.Sessions.SelectedValue -ne 'project-1') { throw 'Search did not filter and select the matching chat' }
            Update-ProjectsList
            if ($controls.ProjectsList.Children.Count -ne 1) { throw 'Project list disappeared on an unchanged refresh' }
            $controls.SessionSearch.Text = 'No matching title'
            if ($controls.Sessions.Items.Count -ne 0 -or $controls.SessionsEmpty.Visibility -ne 'Visible') { throw 'Search did not show its empty state' }
            $controls.SessionSearch.Clear()
            if ($controls.SearchPlaceholder.Visibility -ne 'Visible' -or $controls.SessionsEmpty.Visibility -ne 'Collapsed') { throw 'Search state did not reset after clearing' }
            $before = $controls.Sessions.ItemsSource
            Apply-Navigation
            if (-not [object]::ReferenceEquals($before, $controls.Sessions.ItemsSource)) { throw 'Unchanged refresh rebuilt the chat list' }
            $controls.Sessions.SelectedValue = 'saved-1'
        }
        5 {
            if (-not $controls.Reply.Text.Contains('Saved conversation') -or -not $controls.Copy.IsEnabled -or $controls.Send.IsEnabled) { throw 'Saved transcript was not displayed read-only' }
            [IO.File]::WriteAllText($env:ALPHACODE_GUI_SMOKE_DONE, 'ok')
            $testTimer.Stop()
        }
    }
})
$testTimer.Start()
'@
$source = $source.Replace('$timer.Start()', ('$timer.Start()' + "`r`n" + $interaction))
[IO.File]::WriteAllText($testScript, $source, [Text.UTF8Encoding]::new($true))
[IO.File]::WriteAllText($fakeSource, @'
using System;
using System.IO;
class FakeBridge {
 static void Main() {
  File.WriteAllText(Environment.GetEnvironmentVariable("ALPHACODE_GUI_SMOKE_PID"), System.Diagnostics.Process.GetCurrentProcess().Id.ToString());
  string line;
  while ((line = Console.ReadLine()) != null) {
   File.AppendAllText(Environment.GetEnvironmentVariable("ALPHACODE_GUI_SMOKE_LOG"), line + "\n");
   if (line.Contains("\"sessions\"")) Console.WriteLine("{\"ok\":true,\"stored\":{\"sessions\":[{\"id\":\"saved-1\",\"title\":\"Saved chat\",\"detail\":\"1 message\",\"saved\":true,\"source\":\"alphacode\"}],\"session_count\":1,\"saved_count\":1,\"returned\":1}}");
   else if (line.Contains("\"transcript\"")) Console.WriteLine("{\"ok\":true,\"session_id\":\"saved-1\",\"transcript\":{\"message_count\":1,\"messages\":[{\"role\":\"assistant\",\"text\":\"Saved conversation\",\"tools\":[]}]}}");
   else if (line.Contains("\"model\"")) Console.WriteLine("{\"ok\":true,\"model\":\"test-b\"}");
   else if (line.Contains("\"effort\"")) Console.WriteLine("{\"ok\":true,\"effort\":\"high\"}");
   else if (line.Contains("\"prompt\"")) Console.WriteLine("{\"ok\":true,\"sent\":true}");
   else Console.WriteLine("{\"ok\":true,\"sessions\":[{\"id\":\"__companion__\",\"title\":\"AlphaCode chat\",\"state\":\"ready\",\"detail\":\"Ready\"},{\"id\":\"project-1\",\"title\":\"Sample project\",\"state\":\"ready\",\"detail\":\"Ready\"}],\"working\":0,\"attention\":0,\"offline\":0,\"connected_servers\":1,\"owned_busy\":false,\"model\":\"test-a\",\"effort\":\"none\",\"models\":[\"test-a\",\"test-b\"],\"reply\":\"\"}");
  }
 }
}
'@)
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $compiler /nologo /target:exe "/out:$fakeExe" $fakeSource
if ($LASTEXITCODE -ne 0) { throw 'Could not compile the fake GUI bridge' }
[IO.File]::WriteAllText($bridgeLog, '')
$oldLocalApp = $env:LOCALAPPDATA
$oldLog = $env:ALPHACODE_GUI_SMOKE_LOG
$oldPidPath = $env:ALPHACODE_GUI_SMOKE_PID
$oldDone = $env:ALPHACODE_GUI_SMOKE_DONE
$gui = $null
try {
    $env:LOCALAPPDATA = $testRoot
    $env:ALPHACODE_GUI_SMOKE_LOG = $bridgeLog
    $env:ALPHACODE_GUI_SMOKE_PID = Join-Path $testRoot 'bridge.pid'
    $env:ALPHACODE_GUI_SMOKE_DONE = Join-Path $testRoot 'done'
    $gui = Start-Process powershell.exe -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-STA','-File', ('"' + $testScript + '"'), '-Executable', ('"' + $fakeExe + '"')) -WindowStyle Hidden -PassThru
    for ($attempt = 0; $attempt -lt 45 -and -not (Test-Path -LiteralPath $env:ALPHACODE_GUI_SMOKE_DONE); $attempt++) {
        Start-Sleep -Seconds 1
        $gui.Refresh()
        if ($gui.HasExited) { break }
    }
    $gui.Refresh()
    $requests = [IO.File]::ReadAllText($bridgeLog)
    if ($gui.HasExited -or $gui.MainWindowTitle -ne 'AlphaCode') { throw 'The desktop window did not start' }
    if (-not (Test-Path -LiteralPath $env:ALPHACODE_GUI_SMOKE_DONE)) { throw 'The GUI interaction checks did not complete' }
    if (-not $requests.Contains('"sessions"') -or -not $requests.Contains('"status"')) { throw 'The GUI did not poll both saved chats and status' }
    if (-not $requests.Contains('"model"') -or -not $requests.Contains('test-b')) { throw 'The model control did not send its selection' }
    if (-not $requests.Contains('"effort"') -or -not $requests.Contains('high')) { throw 'The effort control did not send its selection' }
    if (-not $requests.Contains('Smoke test prompt')) { throw 'The send control did not submit the prompt' }
    Write-Output 'GUI opened, polled chats and status, and sent model, effort and prompt actions successfully.'
} finally {
    if ($gui -and -not $gui.HasExited) { Stop-Process -Id $gui.Id -ErrorAction SilentlyContinue }
    $pidPath = Join-Path $testRoot 'bridge.pid'
    if (Test-Path -LiteralPath $pidPath) { Stop-Process -Id ([int][IO.File]::ReadAllText($pidPath)) -ErrorAction SilentlyContinue }
    $env:LOCALAPPDATA = $oldLocalApp
    $env:ALPHACODE_GUI_SMOKE_LOG = $oldLog
    $env:ALPHACODE_GUI_SMOKE_PID = $oldPidPath
    $env:ALPHACODE_GUI_SMOKE_DONE = $oldDone
}

