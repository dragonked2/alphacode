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
function Write-SmokePhase([string]$phase) { [IO.File]::WriteAllText((Join-Path $env:ALPHACODE_GUI_SMOKE_IMAGES 'phase.txt'),$phase) }
function Save-SmokeImage([string]$name, $target = $window) {
    $target.UpdateLayout()
    $bitmap = [Windows.Media.Imaging.RenderTargetBitmap]::new([int]$target.ActualWidth,[int]$target.ActualHeight,96,96,[Windows.Media.PixelFormats]::Pbgra32)
    $bitmap.Render($target)
    $encoder = [Windows.Media.Imaging.PngBitmapEncoder]::new()
    $encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
    $stream = [IO.File]::Create((Join-Path $env:ALPHACODE_GUI_SMOKE_IMAGES $name))
    try { $encoder.Save($stream) } finally { $stream.Dispose() }
}
function Test-SettingsForm {
    $form = New-SettingsWindow
    $form.Show(); $form.UpdateLayout()
    $script:settingsControls.DarkTheme.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
    if ($script:theme -ne 'dark') { throw 'Settings theme card did not update the workspace' }
    $script:settingsControls.LightTheme.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
    $script:settingsControls.TextSize.Value = 18
    if ($controls.Reply.FontSize -ne 18 -or $script:settingsControls.TypePreview.FontSize -ne 18) { throw 'Settings reading-size slider did not update the workspace and preview' }
    $form.UpdateLayout(); Save-SmokeImage 'settings-appearance.png' $form
    $script:settingsControls.TextSize.Value = 14
    Select-SettingsSection 'Workspace'
    foreach ($name in @('ShowSidebar','ShowWork','Topmost')) {
        $control = $script:settingsControls[$name]
        $original = [bool]$control.IsChecked
        $control.IsChecked = -not $original
        $control.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
        $actual = switch ($name) { 'ShowSidebar' { $script:sidebarVisible } 'ShowWork' { $script:workflowVisible } default { $window.Topmost } }
        if ($actual -eq $original) { throw "Settings toggle did not apply: $name" }
        $control.IsChecked = $original
        $control.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
    }
    $form.UpdateLayout(); Save-SmokeImage 'settings-workspace.png' $form
    Select-SettingsSection 'Models'
    $script:settingsControls.ModelSearch.Text = 'test-b'
    if ($script:settingsControls.ModelList.Items.Count -ne 1) { throw 'Settings model search did not filter the list' }
    $script:settingsControls.ModelList.SelectedItem = 'test-b'
    if (-not $script:settingsControls.UseModel.IsEnabled) { throw 'Settings model action is disabled for a ready live chat' }
    $form.UpdateLayout(); Save-SmokeImage 'settings-models.png' $form
    $form.Close()
}
$testStep = 0
$testTimer = New-Object Windows.Threading.DispatcherTimer
$testTimer.Interval = [TimeSpan]::FromSeconds(1)
$testTimer.Add_Tick({
    try {
    [IO.File]::WriteAllText((Join-Path $env:ALPHACODE_GUI_SMOKE_IMAGES 'progress.txt'), "step=$script:testStep pending=$script:pendingKind queued=$($script:queue.Count) selected=$(Get-SelectedSessionId)")
    if ($script:testStep -eq 0 -and -not $controls.ModelPicker.Items.Contains('test-b')) { return }
    $seen = [IO.File]::ReadAllText($env:ALPHACODE_GUI_SMOKE_LOG)
    if ($script:testStep -eq 1 -and (-not $seen.Contains('test-b') -or $script:modelRequestPending)) { return }
    if ($script:testStep -eq 2 -and (-not $seen.Contains('high') -or $script:effortRequestPending)) { return }
    if ($script:testStep -eq 3 -and (-not $seen.Contains('Smoke test prompt') -or $script:pendingKind -in @('prompt','input') -or $script:queue.Count -gt 0)) { return }
    if ($script:testStep -eq 4 -and $script:transcriptFor -ne 'saved-1') { return }
    if ($script:testStep -eq 5 -and -not $seen.Contains('"include_workflow":true')) { return }
    if ($script:testStep -eq 6 -and ($script:modelRequestPending -or @($seen -split "`n" | Where-Object { $_ -match '"action":"model"' }).Count -lt 2)) { return }
    $script:testStep++
    switch ($script:testStep) {
        1 { Open-ModelChooser; $controls.ModelSearch.Text = 'test-b'; $controls.ModelResults.SelectedValue = 'test-b'; Apply-ModelChoice }
        2 { $controls.EffortPicker.SelectedValue = 'high' }
        3 { $controls.Prompt.Text = 'Smoke test prompt'; $controls.Send.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent)); if ($controls.ResponseActivity.Visibility -ne 'Visible' -or $controls.ResponseStage.Text -ne 'Sending your prompt') { throw 'Sending a prompt did not immediately show loading feedback' } }
        4 {
            $controls.Sessions.SelectedValue = '__companion__'
            $controls.Prompt.Text = 'before selected after'
            $controls.Prompt.Select(7,8)
            Insert-ComposerNewline $controls.Prompt
            if ($controls.Prompt.Text -ne ("before " + [Environment]::NewLine + " after") -or $controls.Prompt.CaretIndex -ne 9 -or $controls.Prompt.SelectionLength -ne 0) { throw 'Composer newline lost surrounding text or misplaced the caret' }
            $controls.Prompt.Text = 'Draft for first chat'
            $controls.Sessions.SelectedValue = 'project-1'
            if ($controls.Prompt.Text) { throw 'A draft leaked into another chat' }
            $controls.Prompt.Text = 'Draft for project'
            $controls.Sessions.SelectedValue = '__companion__'
            if ($controls.Prompt.Text -ne 'Draft for first chat') { throw 'First draft was lost' }
            $controls.Sessions.SelectedValue = 'project-1'
            if ($controls.Prompt.Text -ne 'Draft for project') { throw 'Project draft was lost' }
            if ($script:theme -ne 'light' -or $controls.Reply.FontSize -ne 16) { throw 'Startup overwrote saved preferences before reading them' }
            $controls.ThemeToggle.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($script:theme -ne 'dark' -or $controls.ComposerSurface.Background.Color.ToString() -ne '#FF202C39') { throw 'Dark theme did not propagate to the composer' }
            if ($window.Resources['Accent'].Color.ToString() -ne '#FFD4AF37') { throw 'Dark theme lost the shared gold accent' }
            Set-GuiTheme 'light'
            if ($window.Resources['Accent'].Color.ToString() -ne '#FF806515') { throw 'Light theme lost the contrast-safe gold accent' }
            if ($controls.ComposerSurface.Background.Color.ToString() -ne '#FFFFFFFF') { throw 'Light theme did not propagate to the composer' }
            $controls.Prompt.Clear()
            if ($controls.Send.IsEnabled) { throw 'Blank prompts must not enable Send' }
            $controls.Prompt.Text = 'Draft for project'
            if (-not $controls.Send.IsEnabled -or $controls.PromptCount.Text -ne '17 / 16,000') { throw ("Composer state did not follow input: send=$($controls.Send.IsEnabled), count=$($controls.PromptCount.Text), selected=$(Get-SelectedSessionId), pending=$script:pendingKind, queued=$($script:queue.Count)") }
            Write-SmokePhase 'model chooser'
            Open-ModelChooser
            if (-not $controls.ModelPopup.IsOpen -or $controls.ModelResults.Items.Count -ne 2) { throw 'Searchable model chooser did not show the available models' }
            $controls.ModelSearch.Text = 'TEST-B'
            if ($controls.ModelResults.Items.Count -ne 1 -or [string]$controls.ModelResults.Items[0].id -ne 'test-b') { throw 'Model search is not case-insensitive' }
            $window.UpdateLayout(); $controls.ModelPopupSurface.UpdateLayout(); Save-SmokeImage 'model-chooser.png' $controls.ModelPopupSurface
            $controls.ModelSearch.Text = 'missing-model'
            if ($controls.ModelEmpty.Visibility -ne 'Visible' -or $controls.ApplyModelChoice.IsEnabled) { throw 'Model search has no useful empty state' }
            $controls.ModelPopup.IsOpen = $false
            $originalSnapshot = $script:snapshot
            $restricted = ($originalSnapshot | ConvertTo-Json -Depth 30 | ConvertFrom-Json)
            $restricted.models = @('test-b'); $restricted.model = 'signed-out-model'
            Set-Snapshot $restricted
            if ($script:availableModels -contains 'signed-out-model' -or $controls.ModelPicker.Items.Contains('signed-out-model') -or $script:selectedModel) { throw 'An unavailable current model was reinserted or another model falsely marked active' }
            $restricted.models = @(); Set-Snapshot $restricted
            if ($controls.ModelChooser.IsEnabled -or $script:availableModels.Count) { throw 'Empty provider catalogs still offer model selection' }
            $form = New-SettingsWindow; $form.Show(); Select-SettingsSection 'Models'
            if ($script:settingsControls.ModelList.Items.Count -ne 0 -or $script:settingsControls.UseModel.IsEnabled) { throw 'Settings offered a placeholder as a real model' }
            Select-SettingsSection 'Providers'
            Set-ProviderCatalog @([pscustomobject]@{id='openai'; name='OpenAI'; configured=$false; detail='No credentials saved'; oauth=$true; api_key=$true; endpoint=$false; local=$false; setup_label=''})
            if ($script:settingsControls.ProviderBrowser.Visibility -ne 'Visible' -or $script:settingsControls.ProviderKey -isnot [Windows.Controls.PasswordBox]) { throw 'Settings lost browser sign-in or masked API-key input' }
            $script:settingsControls.ProviderKey.Password = 'fixture-secret-not-a-real-key'
            Save-GuiSettings
            if ([IO.File]::ReadAllText((Join-Path $env:LOCALAPPDATA 'AlphaCode/gui.json')).Contains('fixture-secret')) { throw 'Provider credential leaked into GUI preferences' }
            $script:settingsControls.ProviderKey.Clear()
            $form.UpdateLayout(); Save-SmokeImage 'settings-providers.png' $form
            $form.Close(); Set-Snapshot $originalSnapshot
            if (-not $controls.ModelPicker.Items.Contains('test-b') -or $controls.ModelPicker.Items.Count -ne 2) { throw 'Model catalog did not recover after an empty snapshot' }
            Write-SmokePhase 'picker'
            Initialize-NativeWorkspacePicker
            $pickerFolder = Join-Path $env:ALPHACODE_GUI_SMOKE_IMAGES ('picker-' + [string][char]0x627 + [string][char]0x62e + [string][char]0x62a + [string][char]0x628 + [string][char]0x627 + [string][char]0x631 + '-' + [string][char]0x5de5 + [string][char]0x4f5c)
            [IO.Directory]::CreateDirectory($pickerFolder) | Out-Null
            $pickerOptions = [AlphaCode.Desktop.WorkspacePicker]::Probe($pickerFolder)
            if (($pickerOptions -band 0x860) -ne 0x860 -or ($pickerOptions -band 0x200) -ne 0) { throw 'Native picker is not configured for one existing filesystem folder' }
            if ([AlphaCode.Desktop.WorkspacePicker]::ResolveFolder($pickerFolder) -ne $pickerFolder) { throw 'Native picker changed a Unicode folder path' }
            $pickerOwner = [Windows.Interop.WindowInteropHelper]::new($window).Handle
            Write-SmokePhase 'picker cancel'
            if (-not [AlphaCode.Desktop.WorkspacePicker]::ProbeCancel($pickerOwner,$pickerFolder)) { throw 'Cancelling the native picker returned a selection' }
            Write-SmokePhase 'reading'
            Set-ReadingSize 30
            if ($controls.Reply.FontSize -ne 24) { throw 'Reading size exceeded its limit' }
            Set-ReadingSize 14
            if ($controls.CommandInput.FontSize -ne 15 -or $controls.SecretInput.FontSize -ne 15) { throw 'Command inputs did not follow reading size' }
            if ($controls.Prompt.FontSize -ne 15) { throw 'Composer size did not follow reading size' }
            Write-SmokePhase 'settings form'
            Test-SettingsForm
            $controls.Reply.Text = ((1..300 | ForEach-Object { "Response line $_" }) -join "`n")
            $controls.ReplyScroll.Visibility = 'Visible'
            $window.UpdateLayout()
            $controls.ReplyScroll.ScrollToHome()
            $window.UpdateLayout()
            if ($controls.JumpLatest.Visibility -ne 'Visible') { throw 'Scrolled history has no return-to-latest control' }
            $controls.JumpLatest.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            $window.UpdateLayout()
            if ($controls.ReplyScroll.VerticalOffset -lt ($controls.ReplyScroll.ScrollableHeight - 8)) { throw 'Jump to latest did not reach the end' }
            Write-SmokePhase 'rich renderer'
            $sample = [string]::Join("`n", @('You','Can you review this change?','','AlphaCode','# Review summary','','The request uses **one JSON body** and preserves `model_id`.','','- [x] Validate request shape','- [ ] Benchmark the local model','','```rust','let model = "qwen-coder";','println!("{}", model);','```','','```plan','# Verification','1. Run the relevant checks','2. Inspect the model catalog','```','','  ' + [string][char]0x2699 + ' tools: read, bash','','The remaining latency needs a live measurement.'))
            $controls.Reply.Text = $sample
            $controls.EmptyState.Visibility = 'Collapsed'; $controls.ReplyScroll.Visibility = 'Visible'; $controls.Copy.IsEnabled = $true; $controls.FindReply.IsEnabled = $true
            $window.UpdateLayout()
            $codeCards = @($controls.ConversationCards.Children | Where-Object { $_.Tag.kind -eq 'code' })
            if ($codeCards.Count -ne 2 -or $codeCards[1].Tag.copy.Content -ne 'Copy plan') { throw 'Conversation did not render native code and plan cards' }
            $expectedCode = [string]::Join("`n", @('let model = "qwen-coder";','println!("{}", model);'))
            if ([string]$codeCards[0].Tag.copy.Tag -ne $expectedCode) { throw 'Copy-code payload changed the code' }
            $firstCard = $controls.ConversationCards.Children[0]
            $controls.Reply.Text += "`nMore evidence will follow."
            if (-not [object]::ReferenceEquals($firstCard,$controls.ConversationCards.Children[0])) { throw 'Streaming unnecessarily rebuilt stable conversation sections' }
            $unsafe = @(Convert-ResponseBlocks ([string]::Join("`n",@('```text','You','<Button Click="never execute"/>','```'))))
            if ($unsafe.Count -ne 1 -or $unsafe[0].Kind -ne 'code' -or -not $unsafe[0].Body.Contains('<Button')) { throw 'Code content was interpreted as markup or a role boundary' }
            $partial = @(Convert-ResponseBlocks ([string]::Join("`n",@('```rust','let a = 1;'))))
            if ($partial.Count -ne 1 -or $partial[0].Kind -ne 'code') { throw 'A partial streaming fence was lost' }
            $controls.FindReply.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            $controls.ReplySearch.Text = 'model'
            $searchTimer.Stop(); Render-Conversation $true; $window.UpdateLayout()
            if ($script:searchHits.Count -lt 2) { throw 'Conversation search did not find separate matching sections' }
            Move-ConversationHit 1; $firstHit = $script:searchHitIndex; Move-ConversationHit 1
            if ($script:searchHitIndex -eq $firstHit) { throw 'Search navigation did not advance' }
            Close-ConversationSearch
            $controls.ReplyScroll.ScrollToHome(); $window.UpdateLayout()
            Write-SmokePhase 'response activity'
            $activeRow = Get-SelectedSessionRow
            $originalState = $activeRow.state; $originalDetail = $activeRow.detail
            $activeRow.state = 'working'; $activeRow.detail = 'Reading files and preparing a response'
            $controls.Cancel.IsEnabled = $true
            Update-ResponseActivity
            if ($controls.ResponseActivity.Visibility -ne 'Visible' -or -not $responseTimer.IsEnabled -or $controls.ResponseDetail.Text -ne $activeRow.detail) { throw 'Working chat did not show response activity' }
            if ([Windows.SystemParameters]::ClientAreaAnimation -and -not $controls.LoadingDot1.HasAnimatedProperties) { throw 'Loading dots are not animated' }
            $script:responseStarted[(Get-SelectedSessionId)] = [DateTime]::UtcNow.AddSeconds(-73)
            Update-ResponseElapsed
            if ($controls.ResponseElapsed.Text -ne '1m 13s') { throw 'Response elapsed time did not advance' }
            $window.UpdateLayout(); Save-SmokeImage 'response-loading.png'
            $script:responseDisconnected = $true; Update-ResponseActivity
            if ($controls.ResponseActivity.Visibility -ne 'Collapsed' -or $responseTimer.IsEnabled) { throw 'Disconnected bridge retained a working animation' }
            $script:responseDisconnected = $false; Update-ResponseActivity
            $script:inputMode = $true; Update-ResponseActivity
            if ($controls.ResponseActivity.Visibility -ne 'Collapsed' -or $controls.LoadingDot1.HasAnimatedProperties -or $responseTimer.IsEnabled) { throw 'Input request left a loading animation running' }
            $script:inputMode = $false
            $activeRow.state = $originalState; $activeRow.detail = $originalDetail
            Update-ResponseActivity
            if ($controls.ResponseActivity.Visibility -ne 'Collapsed') { throw 'Ready chat retained its response indicator' }
            Save-SmokeImage 'light-rich-conversation.png'
            $controls.ReplyScroll.ScrollToEnd(); $window.UpdateLayout()
            Save-SmokeImage 'light-code-and-plan.png'
            if ($controls.Voice.Visibility -ne 'Collapsed') { throw 'Unavailable voice control should be hidden' }
            Write-SmokePhase 'home toggle'
            Set-SidebarVisibility $true $false
            $homeSelection = Get-SelectedSessionId
            Set-Navigation 'Live'
            $controls.NavHome.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($script:sidebarVisible -or $script:navigation -ne 'Live' -or (Get-SelectedSessionId) -ne $homeSelection -or $controls.NavHome.ToolTip -notlike 'Show sidebar*') { throw ("Home toggle: visible=$script:sidebarVisible nav=$script:navigation selected=$(Get-SelectedSessionId) expected=$homeSelection tooltip=$($controls.NavHome.ToolTip)") }
            $controls.NavHome.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if (-not $script:sidebarVisible -or $controls.NavHome.ToolTip -notlike 'Hide sidebar*') { throw 'Home icon did not restore the sidebar' }
            $controls.FilterSaved.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($controls.Sessions.Items.Count -ne 1 -or (Get-SelectedSessionId) -ne $homeSelection -or $controls.Sessions.Items[0].status_label -ne 'Read-only') { throw 'Saved filter changed the active conversation or lost the read-only label' }
            $controls.FilterLive.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($controls.Sessions.Items.Count -ne 2) { throw 'Live filter included stored history' }
            $controls.FilterAll.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            $controls.SidebarToggle.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($script:sidebarVisible -or $controls.SidebarSurface.Visibility -ne 'Collapsed' -or $controls.SidebarToggle.Content -ne 'Show sidebar') { throw 'Sidebar did not close' }
            $controls.SidebarToggle.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if (-not $script:sidebarVisible -or $controls.SidebarSurface.Visibility -ne 'Visible' -or $controls.SidebarToggle.Content -ne 'Hide sidebar') { throw 'Sidebar did not reopen' }
            Write-SmokePhase 'search selection'
            $controls.Sessions.SelectedValue = '__companion__'
            $searchDraft = $controls.Prompt.Text
            $controls.SessionSearch.Text = 'Sample project'
            if ((Get-SelectedSessionId) -ne '__companion__' -or $controls.Sessions.SelectedValue -or $controls.Prompt.Text -ne $searchDraft) { throw 'Typing a search silently switched the chat or draft' }
            Select-ChatSearchResult 0
            if ($controls.SearchPlaceholder.Visibility -ne 'Collapsed' -or $controls.Sessions.Items.Count -ne 1 -or $controls.Sessions.SelectedValue -ne 'project-1') { throw 'Search did not filter and select the matching chat' }
            Update-ProjectsList
            if ($controls.ProjectsList.Children.Count -ne 1) { throw 'Project list disappeared on an unchanged refresh' }
            $controls.SessionSearch.Text = 'No matching title'
            if ($controls.Sessions.Items.Count -ne 0 -or $controls.SessionsEmpty.Visibility -ne 'Visible') { throw 'Search did not show its empty state' }
            $controls.ClearChatSearch.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            if ($controls.Sessions.SelectedValue -ne 'project-1') { throw 'Clearing search did not restore the active chat selection' }
            Write-SmokePhase 'refresh selection'
            $script:lastSignature = ''; Set-Snapshot $script:snapshot
            if ((Get-SelectedSessionId) -ne 'project-1' -or $controls.Prompt.Text -ne 'Draft for project') { throw 'Refreshing reopened the companion chat or lost the draft' }
            if ($controls.SearchPlaceholder.Visibility -ne 'Visible' -or $controls.SessionsEmpty.Visibility -ne 'Collapsed') { throw 'Search state did not reset after clearing' }
            $before = $controls.Sessions.ItemsSource
            Apply-Navigation
            if (-not [object]::ReferenceEquals($before, $controls.Sessions.ItemsSource)) { throw 'Unchanged refresh rebuilt the chat list' }
            Set-GuiTheme 'dark'
            $window.UpdateLayout()
            Save-SmokeImage 'dark-desktop.png'
            Write-SmokePhase 'work panel'
            $script:snapshot | Add-Member -NotePropertyName selected_session_id -NotePropertyValue (Get-SelectedSessionId) -Force
            $workFixture = '{"available":true,"todo_count":3,"completed":1,"todos":[{"content":"Inspect the repository","status":"completed"},{"content":"Fix the work panel","status":"in_progress"},{"content":"Await dependency","status":"blocked"}],"intention":"Keep task state accurate across every interface.","swarm":[{"name":"worker-one","status":"blocked","role":"reviewer","detail":"Waiting for dependency"}],"swarm_plan":{"version":2,"item_count":2,"completed":0,"failed":1,"blocked":1,"cycles":0,"items":[{"content":"Review changes","status":"failed"},{"content":"Validate result","status":"blocked"}]}}' | ConvertFrom-Json
            $script:snapshot | Add-Member -NotePropertyName workflow -NotePropertyValue $workFixture -Force
            $controls.WorkToggle.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
            Update-Workflow; $window.UpdateLayout()
            if (-not $script:workflowVisible -or -not $controls.WorkflowTodos.Text.Contains('[blocked]') -or -not $controls.WorkflowPlan.Text.Contains('1 failed') -or -not $controls.WorkflowSwarm.Text.Contains('worker-one')) { throw 'Work panel lost todo, plan or swarm state' }
            $controls.WorkColumn.Width = [Windows.GridLength]::new(380)
            $window.UpdateLayout()
            $controls.WorkflowResize.RaiseEvent([Windows.Controls.Primitives.DragCompletedEventArgs]::new(80,0,$false))
            if ([Math]::Abs($script:workflowWidth - 380) -gt 2) { throw 'Work-panel resize was not remembered' }
            Save-SmokeImage 'dark-work-panel.png'
            $controls.WorkflowScroll.ScrollToEnd(); $window.UpdateLayout()
            if ($controls.WorkflowScroll.VerticalOffset -lt $controls.WorkflowScroll.ScrollableHeight - 8) { throw 'Work panel cannot reach the last swarm member' }
            Save-SmokeImage 'dark-work-swarm.png'
            $controls.WorkflowScroll.ScrollToHome()
            Set-WorkflowVisibility $false
            Write-SmokePhase 'compact layout'
            $window.Width = 760; $window.Height = 620
            $window.UpdateLayout(); Update-GuiLayout; $window.UpdateLayout()
            if ($script:sidebarVisible) { throw 'Compact resize did not collapse the sidebar' }
            $sendPoint = $controls.Send.TranslatePoint([Windows.Point]::new(0,0), $window)
            if ($sendPoint.X -lt 0 -or ($sendPoint.X + $controls.Send.ActualWidth) -gt $window.ActualWidth -or $controls.Prompt.ActualHeight -lt 40) { throw 'Compact composer clipped its input or send action' }
            Set-GuiTheme 'light'; $window.UpdateLayout()
            Save-SmokeImage 'light-compact.png'
            if ($controls.NavHome.Foreground.Color.ToString() -ne (Get-GuiBrush 'Muted').Color.ToString() -or $controls.NavHome.ToolTip -notlike 'Show sidebar*') { throw 'Collapsed home icon lost its theme or action label' }
            $window.Width = 640; $window.Height = 540; $window.UpdateLayout(); Update-GuiLayout; $window.UpdateLayout()
            $sendPoint = $controls.Send.TranslatePoint([Windows.Point]::new(0,0), $window)
            if ($sendPoint.X + $controls.Send.ActualWidth -gt $window.ActualWidth -or $sendPoint.Y + $controls.Send.ActualHeight -gt $window.ActualHeight) { throw 'Minimum-size window clipped Send' }
            $titlePoint = $controls.WorkspaceTitle.TranslatePoint([Windows.Point]::new(0,0), $controls.ChatSurface)
            if ($titlePoint.Y -lt 0 -or ($titlePoint.Y + $controls.WorkspaceTitle.ActualHeight) -gt $controls.ChatSurface.RowDefinitions[0].ActualHeight) { throw 'Wrapped header actions clipped the workspace title' }
            if ($controls.ExamplesPanel.Visibility -ne 'Collapsed' -or $controls.EmptyState.ActualHeight -gt $controls.ChatSurface.RowDefinitions[2].ActualHeight) { throw 'Short-window empty state overlapped the composer' }
            $row = Get-SelectedSessionRow; $previousState = $row.state; $row.state = 'working'
            Update-ResponseActivity; $window.UpdateLayout()
            $sendPoint = $controls.Send.TranslatePoint([Windows.Point]::new(0,0), $window)
            $activityPoint = $controls.ResponseActivity.TranslatePoint([Windows.Point]::new(0,0), $window)
            if ($sendPoint.Y + $controls.Send.ActualHeight -gt $window.ActualHeight -or $activityPoint.Y -lt 0) { throw 'Loading feedback clipped the compact composer' }
            Save-SmokeImage 'compact-loading.png'
            $row.state = $previousState; Update-ResponseActivity; $window.UpdateLayout()
            Save-SmokeImage 'light-minimum.png'
            Set-WorkflowVisibility $true; $window.UpdateLayout()
            if ($controls.ConversationColumn.ActualWidth -gt 1 -or $controls.WorkColumn.ActualWidth -lt 400) { throw 'Compact work panel did not replace the crowded conversation columns' }
            Set-WorkflowVisibility $false
            $script:snapshot | Add-Member -NotePropertyName input_request -NotePropertyValue ([pscustomobject]@{ request_id='smoke-input'; prompt='Command needs a value'; is_password=$true }) -Force
            $controls.Sessions.SelectedValue = '__companion__'; Update-SelectedSession '__companion__'
            if ($controls.SecretInput.Visibility -ne 'Visible' -or $controls.NoticeBanner.Visibility -ne 'Visible' -or $controls.NoticeText.Text -ne 'Command needs a value') { throw 'Command request was hidden behind conversation history' }
            $script:snapshot.input_request = $null; Update-SelectedSession '__companion__'
            $window.Width = 1280; $window.Height = 820; $window.UpdateLayout()
            Write-SmokePhase 'saved history'
            $controls.Sessions.SelectedValue = 'saved-1'
        }
        5 {
            if (-not $controls.Reply.Text.Contains('Saved conversation') -or -not $controls.Copy.IsEnabled -or $controls.Send.IsEnabled) { throw 'Saved transcript was not displayed read-only' }
            if (-not $controls.Prompt.IsReadOnly -or $controls.ModelPicker.IsEnabled -or $controls.EffortPicker.IsEnabled -or $controls.NoticeBanner.Visibility -ne 'Collapsed') { throw 'Saved history exposed editable controls or another chat delivery notice' }
            if ($controls.WorkflowTodos.Text.Contains('Inspect the repository')) { throw 'Another chat todo list leaked into saved history' }
            Save-SmokeImage 'light-conversation.png'
            Set-WorkflowVisibility $true
        }
        6 {
            Set-WorkflowVisibility $false
            $controls.Sessions.SelectedValue = '__companion__'
            $form = New-SettingsWindow; $form.Show(); Select-SettingsSection 'Models'
            $script:settingsControls.ModelSearch.Text = 'test-b'; $script:settingsControls.ModelList.SelectedItem = 'test-b'
            $script:settingsControls.UseModel.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent))
        }
        7 {
            if ($script:settingsWindow) { $script:settingsWindow.Close() }
            [IO.File]::WriteAllText($env:ALPHACODE_GUI_SMOKE_DONE, 'ok')
            $testTimer.Stop()
        }
    }
    } catch {
        [IO.File]::WriteAllText((Join-Path $env:ALPHACODE_GUI_SMOKE_IMAGES 'failure.txt'), ($_ | Out-String))
        $testTimer.Stop(); $window.Close()
    }
})
$testTimer.Start()
'@
$source = $source.Replace('[Windows.MessageBox]::Show($_.Exception.Message, ''Could not open AlphaCode GUI'', ''OK'', ''Error'') | Out-Null', 'throw $details')
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
   else Console.WriteLine("{\"ok\":true,\"sessions\":[{\"id\":\"__companion__\",\"title\":\"AlphaCode chat\",\"state\":\"ready\",\"detail\":\"Ready\"},{\"id\":\"project-1\",\"working_dir\":\"C:/workspace/sample\",\"title\":\"Sample project\",\"state\":\"ready\",\"detail\":\"Ready\"}],\"working\":0,\"attention\":0,\"offline\":0,\"connected_servers\":1,\"owned_busy\":false,\"model\":\"test-a\",\"effort\":\"none\",\"models\":[\"test-a\",\"test-b\"],\"model_options\":[{\"id\":\"test-a\",\"provider\":\"Connected test provider\"},{\"id\":\"test-b\",\"provider\":\"Connected test provider\"}],\"reply\":\"\"}");
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
$oldImages = $env:ALPHACODE_GUI_SMOKE_IMAGES
$gui = $null
try {
    $env:LOCALAPPDATA = $testRoot
    $env:ALPHACODE_GUI_SMOKE_IMAGES = $testRoot
    [IO.Directory]::CreateDirectory((Join-Path $testRoot 'AlphaCode')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $testRoot 'AlphaCode/gui.json'), '{"theme":"light","reading_size":16,"left":20,"top":20,"sidebar_visible":true}')
    $env:ALPHACODE_GUI_SMOKE_LOG = $bridgeLog
    $env:ALPHACODE_GUI_SMOKE_PID = Join-Path $testRoot 'bridge.pid'
    $env:ALPHACODE_GUI_SMOKE_DONE = Join-Path $testRoot 'done'
    $gui = Start-Process powershell.exe -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-STA','-File', ('"' + $testScript + '"'), '-Executable', ('"' + $fakeExe + '"')) -WindowStyle Hidden -PassThru
    for ($attempt = 0; $attempt -lt 120 -and -not (Test-Path -LiteralPath $env:ALPHACODE_GUI_SMOKE_DONE); $attempt++) {
        Start-Sleep -Seconds 1
        $gui.Refresh()
        if ($gui.HasExited) { break }
    }
    $gui.Refresh()
    $requests = [IO.File]::ReadAllText($bridgeLog)
    $failure = Join-Path $testRoot 'failure.txt'
    if (Test-Path -LiteralPath $failure) { throw ([IO.File]::ReadAllText($failure)) }
    if ($gui.HasExited -or $gui.MainWindowTitle -ne 'AlphaCode') { throw 'The desktop window did not start' }
    if (-not (Test-Path -LiteralPath $env:ALPHACODE_GUI_SMOKE_DONE)) { throw ('The GUI interaction checks did not complete: ' + [IO.File]::ReadAllText((Join-Path $testRoot 'progress.txt')) + ' phase=' + [IO.File]::ReadAllText((Join-Path $testRoot 'phase.txt'))) }
    if (-not $requests.Contains('"sessions"') -or -not $requests.Contains('"status"')) { throw 'The GUI did not poll both saved chats and status' }
    if (-not $requests.Contains('"model"') -or -not $requests.Contains('test-b')) { throw 'The model control did not send its selection' }
    if (-not $requests.Contains('"effort"') -or -not $requests.Contains('high')) { throw 'The effort control did not send its selection' }
    if (-not $requests.Contains('Smoke test prompt')) { throw 'The send control did not submit the prompt' }
    Write-Output 'GUI smoke passed: connected model options, searchable model chooser, immediate and animated response activity, modern settings form, home/sidebar toggle, stable chat selection, filters, keyboard search, native Explorer picker, rich conversation, code/plan cards, safe rendering, streaming reuse, search, resizable work panel, themes, drafts and bridge actions.'
    Write-Output ("GUI images: " + $testRoot)
} finally {
    if ($gui -and -not $gui.HasExited) { Stop-Process -Id $gui.Id -ErrorAction SilentlyContinue }
    $pidPath = Join-Path $testRoot 'bridge.pid'
    if (Test-Path -LiteralPath $pidPath) { Stop-Process -Id ([int][IO.File]::ReadAllText($pidPath)) -ErrorAction SilentlyContinue }
    $env:LOCALAPPDATA = $oldLocalApp
    $env:ALPHACODE_GUI_SMOKE_LOG = $oldLog
    $env:ALPHACODE_GUI_SMOKE_PID = $oldPidPath
    $env:ALPHACODE_GUI_SMOKE_DONE = $oldDone
    $env:ALPHACODE_GUI_SMOKE_IMAGES = $oldImages
}

