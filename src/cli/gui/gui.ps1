param([Parameter(Mandatory=$true)][string]$Executable, [switch]$Preview)
$ErrorActionPreference = 'Stop'

# The compiled host already contains this type. Direct script launches (tests
# and development) load the same implementation, not a separate logging path.
if (-not ('AlphaCode.Desktop.GuiDiagnostics' -as [type])) {
    Add-Type -Path (Join-Path $PSScriptRoot 'GuiDiagnostics.cs') -ReferencedAssemblies @([Management.Automation.PSObject].Assembly.Location,'System.Web.Extensions','System.Core')
    [AlphaCode.Desktop.GuiDiagnostics]::Initialize([AlphaCode.Desktop.GuiDiagnostics].Assembly,[IO.File]::ReadAllText($PSCommandPath))
}
[AlphaCode.Desktop.GuiDiagnostics]::InstallProcessHandlers()
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Windows.Forms

function Get-GuiDiagnosticState {
    $bridgeRunning = $false
    try { $bridgeRunning = $script:process -and -not $script:process.HasExited } catch { }
    return @{
        preview=[bool]$Preview; closed=[bool]$script:closed; navigation=[string]$script:navigation
        pending_kind=[string]$script:pendingKind; queue_count=$(if ($script:queue) { $script:queue.Count } else { 0 })
        session_count=@($script:allSessions).Count; working_count=[int]$script:working
        input_mode=[bool]$script:inputMode; response_active=[bool]$script:responseSession; bridge_running=[bool]$bridgeRunning
        main_timer_enabled=[bool]($timer -and $timer.IsEnabled); main_timer_ms=$(if ($timer) { $timer.Interval.TotalMilliseconds } else { 0 })
        response_timer_enabled=[bool]($responseTimer -and $responseTimer.IsEnabled); response_timer_ms=$(if ($responseTimer) { $responseTimer.Interval.TotalMilliseconds } else { 0 })
        search_timer_enabled=[bool]($searchTimer -and $searchTimer.IsEnabled); search_timer_ms=$(if ($searchTimer) { $searchTimer.Interval.TotalMilliseconds } else { 0 })
    }
}

function Write-GuiFailure([string]$category, [string]$callback, $failure) {
    # State is allowlisted in C#. Never serialize snapshots, requests, session
    # titles, composer/search text or credentials. Redact UI values if an
    # exception embeds them in its message.
    try {
        $privateValues = New-Object 'System.Collections.Generic.List[string]'
        if ($controls) {
            foreach ($name in @('Prompt','CommandInput','SecretInput','Reply','ReplySearch','SessionSearch','WorkspacePath')) {
                $control = $controls[$name]
                if ($control -is [Windows.Controls.PasswordBox]) { $privateValues.Add($control.Password) }
                elseif ($control -and $control.Text) { $privateValues.Add([string]$control.Text) }
            }
        }
        if ($script:settingsControls -and $script:settingsControls.ProviderKey) { $privateValues.Add($script:settingsControls.ProviderKey.Password) }
        [AlphaCode.Desktop.GuiDiagnostics]::Write($category,$callback,$failure,(Get-GuiDiagnosticState),$privateValues.ToArray())
    } catch {
        # The native writer cannot throw. Preserve the logging failure rather
        # than fall back to an unredacted original exception/UI payload.
        [AlphaCode.Desktop.GuiDiagnostics]::Write('host','Write-GuiFailure', $_, $null, $null)
    }
}

function Show-GuiFailureNotice {
    # Do not render raw exception messages (which may contain private data).
    try {
        if ($controls.FooterStatus) { $controls.FooterStatus.Text = 'An operation failed · diagnostics saved in LocalAppData/AlphaCode' }
    } catch { }
}

function Invoke-GuiCallback([string]$name, [scriptblock]$callback, [Windows.Threading.DispatcherTimer]$failureTimer = $null) {
    if ($script:closed) { return }
    try { & $callback }
    catch {
        $failure = $_
        Write-GuiFailure 'callback' $name $failure
        if ([AlphaCode.Desktop.GuiDiagnostics]::IsFatal($failure.Exception)) { throw }
        if ($failureTimer) { $failureTimer.Stop() }
        Show-GuiFailureNotice
    }
}

function Stop-GuiResources {
    $script:closed = $true
    foreach ($activeTimer in @($timer,$searchTimer,$responseTimer)) {
        if ($activeTimer) { $activeTimer.Stop() }
    }
    if ($script:inputWriter) {
        try { $script:inputWriter.Dispose() }
        catch { Write-GuiFailure 'callback' 'Window.Close.InputWriter' $_ }
        finally { $script:inputWriter = $null }
    }
    if ($script:process) {
        try { if (-not $script:process.HasExited) { $script:process.Kill() } }
        catch { Write-GuiFailure 'callback' 'Window.Close.Bridge' $_ }
        finally { $script:process.Dispose(); $script:process = $null }
    }
}

$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$mutexName = 'Local\AlphaCode.Desktop.' + $identity
if ($Preview) { $mutexName += '.Preview' }
$mutex = New-Object System.Threading.Mutex($false, $mutexName)
if (-not $mutex.WaitOne(0)) { $mutex.Dispose(); return }

$script:process = $null
$script:closed = $false
$script:inputWriter = $null
$script:pending = $null
$script:pendingKind = ''
$script:lastPoll = [DateTime]::MinValue
$script:requestStarted = [DateTime]::MinValue
$script:queue = New-Object System.Collections.Queue
$script:expanded = $true
$script:ownedBusy = $false
$script:working = 0
$script:lastSignature = ''
$script:hasLiveSnapshot = $false
$script:userSelectedChat = $false
$script:retryAt = [DateTime]::MinValue
$script:deliveryNotice = ''
$script:noticeUntil = [DateTime]::MaxValue
# Short-lived feedback for control actions (model/effort/sidebar/copy/...).
# Without this the 1s status poll overwrote FooterStatus immediately, so every
# button looked like it did nothing even when the request succeeded.
$script:actionNotice = ''
$script:actionNoticeUntil = [DateTime]::MinValue
$script:contextPrimed = $false
$script:submittedPrompt = ''
$script:submittedInput = $false
$script:inputMode = $false
$script:passwordMode = $false
$script:currentInputRequest = ''
$script:navigation = 'Home'
$script:navigationBack = New-Object System.Collections.Stack
$script:navigationForward = New-Object System.Collections.Stack
$script:sidebarVisible = $true
$script:sidebarWidth = 272
$script:allSessions = @()
$script:liveSessions = @()
$script:storedSessions = @()
$script:lastStoredPoll = [DateTime]::MinValue
$script:lastStoredCount = -1
$script:projectsListSignature = $null
$script:navigationListSignature = $null
$script:lastSelectedSessionId = ''
$script:drafts = @{}
$script:draftSession = ''
$script:submittedSession = ''
$script:transcript = ''
$script:transcriptFor = ''
$script:lastTranscriptPoll = [DateTime]::MinValue
$script:availableModels = @()
$script:modelRows = @()
$script:providerRows = @()
$script:providerConnection = $null
$script:providersRefreshRequested = $false
$script:lastProviderPhase = ''
$script:modelChooserSignature = $null
$script:responseStarted = @{}
$script:pendingResponseSession = ''
$script:responseSession = ''
$script:responseDisconnected = $false
$script:modelListSignature = $null
$script:modelRequestPending = $false
$script:effortRequestPending = $false
$script:capabilitiesRequestPending = $false
$script:preferredModel = ''
$script:preferredEffort = 'none'
$script:selectedModel = ''
$script:selectedEffort = 'none'
$script:suppressSelectionEvents = $false
$script:dictationAvailable = $false
$script:dictationAutoSend = $false
$script:isArabic = $false
$script:initializing = $true
$script:theme = 'dark'
$script:sidebarAutoHidden = $false
$script:compactLayout = $false
$script:workflowVisible = $false
$script:workflowWidth = 340
$script:renderedBlocks = @()
$script:searchHits = @()
$script:searchHitIndex = -1
$script:renderedQuery = ''
$settingsDir = Join-Path $env:LOCALAPPDATA 'AlphaCode'
$settingsPath = Join-Path $settingsDir 'gui.json'
$expandedWidth = 1280
$expandedHeight = 820

try {
[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="AlphaCode"
 Width="1280" Height="820" MinWidth="640" MinHeight="540" WindowStyle="None" ResizeMode="CanResize" WindowStartupLocation="CenterScreen" TextOptions.TextFormattingMode="Display"
 AllowsTransparency="False" Background="{DynamicResource WindowBg}" Topmost="False" ShowInTaskbar="True"
 FontFamily="Segoe UI Variable Text, Segoe UI" Foreground="{DynamicResource Text}" FontSize="13" UseLayoutRounding="True" SnapsToDevicePixels="True">
 <WindowChrome.WindowChrome>
  <!-- ResizeBorderThickness restores the resize grips and correct maximize
       work-area handling that WindowStyle="None" removes, while
       CaptionHeight="0" keeps the whole client area hit-testable so the
       script owns dragging and double-click-to-maximize. That avoids two drag
       loops (native caption + DragMove) fighting each other. -->
  <WindowChrome CaptionHeight="0" ResizeBorderThickness="6" CornerRadius="0" GlassFrameThickness="0" UseAeroCaptionButtons="False"/>
 </WindowChrome.WindowChrome>
 <Window.Resources>
  <Style TargetType="TextBox"><Setter Property="SelectionBrush" Value="{DynamicResource Accent}"/><Setter Property="SelectionOpacity" Value="0.35"/><Setter Property="CaretBrush" Value="{DynamicResource Accent}"/></Style>
  <Style TargetType="ToolTip"><Setter Property="Background" Value="{DynamicResource SurfaceElevated}"/><Setter Property="Foreground" Value="{DynamicResource Text}"/><Setter Property="BorderBrush" Value="{DynamicResource Border}"/><Setter Property="Padding" Value="12,8"/><Setter Property="MaxWidth" Value="360"/><Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ToolTip"><Border Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="1" CornerRadius="8" Padding="{TemplateBinding Padding}"><ContentPresenter><ContentPresenter.Resources><Style TargetType="TextBlock"><Setter Property="TextWrapping" Value="Wrap"/></Style></ContentPresenter.Resources></ContentPresenter></Border></ControlTemplate></Setter.Value></Setter></Style>
  <Style x:Key="ComposerInput" TargetType="TextBox">
   <Setter Property="Background" Value="Transparent"/><Setter Property="Foreground" Value="{DynamicResource Text}"/>
   <Setter Property="CaretBrush" Value="{DynamicResource Accent}"/><Setter Property="SelectionBrush" Value="{DynamicResource Accent}"/><Setter Property="SelectionOpacity" Value="0.35"/>
   <Setter Property="BorderThickness" Value="0"/><Setter Property="Padding" Value="0"/><Setter Property="FontSize" Value="15"/>
   <Setter Property="FocusVisualStyle" Value="{x:Null}"/><Setter Property="VerticalContentAlignment" Value="Top"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="TextBox"><ScrollViewer x:Name="PART_ContentHost" Background="Transparent" BorderThickness="0" Padding="0" Focusable="False" CanContentScroll="False" HorizontalScrollBarVisibility="Disabled" VerticalScrollBarVisibility="Auto"/></ControlTemplate></Setter.Value></Setter>
  </Style>
  <!-- Design brief: a calm, keyboard-first workspace for developers moving
       between chat, project context and agent work. Conversation and composing
       take priority over chrome. Native WPF, vector icons and incremental
       rendering keep input responsive; gold remains the AlphaCode accent.
       Semantic tokens below are shared by the shell, overlays and settings.
       No provider/user content is interpreted as XAML. -->
  <SolidColorBrush x:Key="WindowBg" Color="#0C0E14"/>
  <SolidColorBrush x:Key="RailBg" Color="#0C0E14"/>
  <SolidColorBrush x:Key="SidebarBg" Color="#12151D"/>
  <SolidColorBrush x:Key="Canvas" Color="#181B24"/>
  <SolidColorBrush x:Key="Surface" Color="#202530"/>
  <SolidColorBrush x:Key="SurfaceElevated" Color="#252B37"/>
  <SolidColorBrush x:Key="SurfaceHover" Color="#303747"/>
  <SolidColorBrush x:Key="CodeSurface" Color="#10131B"/>
  <SolidColorBrush x:Key="UserMessage" Color="#232A38"/>
  <SolidColorBrush x:Key="Border" Color="#3B4455"/>
  <SolidColorBrush x:Key="Text" Color="#F3F4F7"/>
  <SolidColorBrush x:Key="Muted" Color="#BEC5D2"/>
  <SolidColorBrush x:Key="Dim" Color="#98A4B7"/>
  <SolidColorBrush x:Key="Accent" Color="#F1C86A"/>
  <SolidColorBrush x:Key="AccentSoft" Color="#393124"/>
  <SolidColorBrush x:Key="AccentText" Color="#211909"/>
  <SolidColorBrush x:Key="Success" Color="#7EE2B8"/>
  <SolidColorBrush x:Key="SuccessSoft" Color="#183B31"/>
  <SolidColorBrush x:Key="Warning" Color="#F5CC84"/>
  <SolidColorBrush x:Key="WarningSoft" Color="#443522"/>
  <SolidColorBrush x:Key="Danger" Color="#FFACB7"/>
  <SolidColorBrush x:Key="DangerSoft" Color="#44252E"/>
  <Style x:Key="Icon" TargetType="Path">
   <Setter Property="Width" Value="18"/><Setter Property="Height" Value="18"/><Setter Property="Stretch" Value="Uniform"/>
   <Setter Property="Stroke" Value="{Binding Foreground, RelativeSource={RelativeSource AncestorType={x:Type Button}}}"/>
   <Setter Property="StrokeThickness" Value="1.7"/><Setter Property="StrokeLineJoin" Value="Round"/>
   <Setter Property="StrokeStartLineCap" Value="Round"/><Setter Property="StrokeEndLineCap" Value="Round"/>
   <Setter Property="HorizontalAlignment" Value="Center"/><Setter Property="VerticalAlignment" Value="Center"/>
  </Style>
  <Style TargetType="Button">
   <Setter Property="Foreground" Value="{DynamicResource Text}"/>
   <Setter Property="Background" Value="Transparent"/>
   <Setter Property="BorderThickness" Value="0"/>
   <Setter Property="Padding" Value="12,8"/>
   <Setter Property="FontSize" Value="13"/>
   <Setter Property="MinHeight" Value="36"/>
   <Setter Property="FocusVisualStyle" Value="{x:Null}"/>
   <Setter Property="BorderBrush" Value="Transparent"/>
   <Setter Property="BorderThickness" Value="1"/>
   <Setter Property="Cursor" Value="Hand"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
    <Border x:Name="ButtonSurface" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="10" Padding="{TemplateBinding Padding}">
     <ContentPresenter HorizontalAlignment="{TemplateBinding HorizontalContentAlignment}" VerticalAlignment="Center"/>
    </Border>
    <ControlTemplate.Triggers>
     <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ButtonSurface" Property="Background" Value="{DynamicResource SurfaceHover}"/><Setter Property="Foreground" Value="{DynamicResource Text}"/></Trigger>
     <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="ButtonSurface" Property="BorderBrush" Value="{DynamicResource Accent}"/><Setter TargetName="ButtonSurface" Property="BorderThickness" Value="1"/></Trigger>
     <Trigger Property="IsPressed" Value="True"><Setter TargetName="ButtonSurface" Property="Opacity" Value="0.68"/></Trigger>
     <Trigger Property="IsEnabled" Value="False"><Setter TargetName="ButtonSurface" Property="Opacity" Value="0.35"/></Trigger>
    </ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style x:Key="PrimaryButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}">
   <Setter Property="Background" Value="{DynamicResource Accent}"/><Setter Property="Foreground" Value="{DynamicResource AccentText}"/>
   <Setter Property="FontWeight" Value="SemiBold"/><Setter Property="MinHeight" Value="40"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
    <Border x:Name="PrimarySurface" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="1" CornerRadius="10" Padding="{TemplateBinding Padding}"><ContentPresenter HorizontalAlignment="{TemplateBinding HorizontalContentAlignment}" VerticalAlignment="Center"/></Border>
    <ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="PrimarySurface" Property="Opacity" Value="0.88"/></Trigger><Trigger Property="IsPressed" Value="True"><Setter TargetName="PrimarySurface" Property="Opacity" Value="0.7"/></Trigger><Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="PrimarySurface" Property="BorderBrush" Value="{DynamicResource Text}"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter TargetName="PrimarySurface" Property="Opacity" Value="0.4"/></Trigger></ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style x:Key="SecondaryButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}"><Setter Property="Background" Value="{DynamicResource Surface}"/><Setter Property="BorderBrush" Value="{DynamicResource Border}"/></Style>
  <Style x:Key="IconButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}"><Setter Property="Width" Value="36"/><Setter Property="Height" Value="36"/><Setter Property="Padding" Value="8"/><Setter Property="Foreground" Value="{DynamicResource Muted}"/></Style>
  <Style x:Key="RailButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}">
   <Setter Property="Width" Value="44"/><Setter Property="Height" Value="44"/><Setter Property="Padding" Value="12"/><Setter Property="Margin" Value="0,3"/>
   <Setter Property="Foreground" Value="{DynamicResource Muted}"/>
  </Style>
  <Style x:Key="NavButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}">
   <Setter Property="HorizontalContentAlignment" Value="Left"/><Setter Property="Padding" Value="12,8"/>
   <Setter Property="Foreground" Value="{DynamicResource Muted}"/>
  </Style>
  <Style TargetType="ListBox">
   <Setter Property="Background" Value="Transparent"/><Setter Property="BorderThickness" Value="0"/>
   <Setter Property="ScrollViewer.HorizontalScrollBarVisibility" Value="Disabled"/>
   <Setter Property="ScrollViewer.CanContentScroll" Value="True"/>
   <Setter Property="VirtualizingPanel.IsVirtualizing" Value="True"/>
   <Setter Property="VirtualizingPanel.VirtualizationMode" Value="Recycling"/>
   <Setter Property="Foreground" Value="{DynamicResource Text}"/>
  </Style>
  <Style TargetType="ListBoxItem">
   <Setter Property="Foreground" Value="{DynamicResource Text}"/><Setter Property="Padding" Value="12,12"/>
   <Setter Property="Margin" Value="0,3,0,3"/><Setter Property="HorizontalContentAlignment" Value="Stretch"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ListBoxItem">
    <Border x:Name="ItemSurface" Background="Transparent" BorderBrush="Transparent" BorderThickness="1" CornerRadius="10" Padding="{TemplateBinding Padding}">
     <Grid><Grid.ColumnDefinitions><ColumnDefinition Width="4"/><ColumnDefinition/></Grid.ColumnDefinitions><Border x:Name="SelectionMarker" Background="{DynamicResource Accent}" CornerRadius="2" Visibility="Collapsed" Margin="0,0,3,0"/><ContentPresenter Grid.Column="1"/></Grid>
    </Border>
    <ControlTemplate.Triggers>
     <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ItemSurface" Property="Background" Value="{DynamicResource SurfaceHover}"/></Trigger>
     <Trigger Property="IsSelected" Value="True"><Setter TargetName="ItemSurface" Property="Background" Value="{DynamicResource AccentSoft}"/><Setter TargetName="ItemSurface" Property="BorderBrush" Value="{DynamicResource Accent}"/><Setter TargetName="SelectionMarker" Property="Visibility" Value="Visible"/><Setter Property="Foreground" Value="{DynamicResource Text}"/></Trigger>
     <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="ItemSurface" Property="BorderBrush" Value="{DynamicResource Accent}"/></Trigger>
    </ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style TargetType="ComboBoxItem">
   <Setter Property="Foreground" Value="{DynamicResource Text}"/><Setter Property="Padding" Value="10,7"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ComboBoxItem">
    <Border x:Name="OptionSurface" Background="Transparent" Padding="{TemplateBinding Padding}" CornerRadius="8"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="20"/><ColumnDefinition/></Grid.ColumnDefinitions><TextBlock x:Name="OptionCheck" Text="✓" Visibility="Hidden" Foreground="{DynamicResource Accent}"/><ContentPresenter Grid.Column="1"/></Grid></Border>
    <ControlTemplate.Triggers>
     <Trigger Property="IsHighlighted" Value="True"><Setter TargetName="OptionSurface" Property="Background" Value="{DynamicResource SurfaceHover}"/></Trigger>
     <Trigger Property="IsSelected" Value="True"><Setter TargetName="OptionSurface" Property="Background" Value="{DynamicResource AccentSoft}"/><Setter TargetName="OptionCheck" Property="Visibility" Value="Visible"/></Trigger>
    </ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style TargetType="ComboBox">
   <Setter Property="Foreground" Value="{DynamicResource Text}"/><Setter Property="Background" Value="{DynamicResource Surface}"/>
   <Setter Property="BorderBrush" Value="{DynamicResource Border}"/><Setter Property="BorderThickness" Value="1"/>
   <Setter Property="Padding" Value="9,4"/><Setter Property="Cursor" Value="Hand"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ComboBox">
    <Grid>
     <ToggleButton x:Name="DropToggle" Focusable="False" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" IsChecked="{Binding IsDropDownOpen, RelativeSource={RelativeSource TemplatedParent}, Mode=TwoWay}">
      <ToggleButton.Template><ControlTemplate TargetType="ToggleButton"><Border x:Name="ToggleSurface" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="8"><Path Data="M 0 0 L 4 4 L 8 0" Stroke="{DynamicResource Muted}" StrokeThickness="1.4" HorizontalAlignment="Right" VerticalAlignment="Center" Margin="0,0,10,0"/></Border><ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ToggleSurface" Property="BorderBrush" Value="{DynamicResource Accent}"/></Trigger></ControlTemplate.Triggers></ControlTemplate></ToggleButton.Template>
     </ToggleButton>
     <ContentPresenter ClipToBounds="True" Margin="9,0,23,0" VerticalAlignment="Center" HorizontalAlignment="Left" IsHitTestVisible="False" Content="{TemplateBinding SelectionBoxItem}" ContentTemplate="{TemplateBinding SelectionBoxItemTemplate}"/>
     <Popup x:Name="PART_Popup" Placement="Bottom" IsOpen="{TemplateBinding IsDropDownOpen}" AllowsTransparency="True" Focusable="False" PopupAnimation="Fade">
      <Border Background="{DynamicResource SurfaceElevated}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="4" MinWidth="{Binding ActualWidth, RelativeSource={RelativeSource TemplatedParent}}"><ScrollViewer MaxHeight="280" CanContentScroll="True"><ItemsPresenter/></ScrollViewer></Border>
     </Popup>
    </Grid>
    <ControlTemplate.Triggers><Trigger Property="IsKeyboardFocusWithin" Value="True"><Setter TargetName="DropToggle" Property="BorderBrush" Value="{DynamicResource Accent}"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter TargetName="DropToggle" Property="Opacity" Value="0.5"/></Trigger></ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style TargetType="ScrollBar">
   <Setter Property="Background" Value="Transparent"/><Setter Property="Width" Value="10"/><Setter Property="Focusable" Value="False"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ScrollBar"><Grid Background="{TemplateBinding Background}"><Track x:Name="PART_Track" Orientation="{TemplateBinding Orientation}" Minimum="{TemplateBinding Minimum}" Maximum="{TemplateBinding Maximum}" ViewportSize="{TemplateBinding ViewportSize}" Value="{Binding Value, RelativeSource={RelativeSource TemplatedParent}, Mode=TwoWay}" IsDirectionReversed="True">
    <Track.DecreaseRepeatButton><RepeatButton x:Name="PageBefore" Command="{x:Static ScrollBar.PageUpCommand}" Focusable="False" Opacity="0"/></Track.DecreaseRepeatButton>
    <Track.Thumb><Thumb x:Name="ScrollThumb" MinHeight="24"><Thumb.Template><ControlTemplate TargetType="Thumb"><Border x:Name="ThumbSurface" Background="{DynamicResource Border}" CornerRadius="4" Margin="2"/><ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ThumbSurface" Property="Background" Value="{DynamicResource Accent}"/></Trigger></ControlTemplate.Triggers></ControlTemplate></Thumb.Template></Thumb></Track.Thumb>
    <Track.IncreaseRepeatButton><RepeatButton x:Name="PageAfter" Command="{x:Static ScrollBar.PageDownCommand}" Focusable="False" Opacity="0"/></Track.IncreaseRepeatButton>
   </Track></Grid><ControlTemplate.Triggers><Trigger Property="Orientation" Value="Horizontal"><Setter Property="Width" Value="Auto"/><Setter Property="Height" Value="10"/><Setter TargetName="PART_Track" Property="IsDirectionReversed" Value="False"/><Setter TargetName="PageBefore" Property="Command" Value="{x:Static ScrollBar.PageLeftCommand}"/><Setter TargetName="PageAfter" Property="Command" Value="{x:Static ScrollBar.PageRightCommand}"/><Setter TargetName="ScrollThumb" Property="MinHeight" Value="0"/><Setter TargetName="ScrollThumb" Property="MinWidth" Value="24"/></Trigger></ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style TargetType="ContextMenu"><Setter Property="Background" Value="{DynamicResource SurfaceElevated}"/><Setter Property="Foreground" Value="{DynamicResource Text}"/><Setter Property="BorderBrush" Value="{DynamicResource Border}"/><Setter Property="BorderThickness" Value="1"/><Setter Property="Padding" Value="6"/></Style>
  <Style TargetType="MenuItem"><Setter Property="Foreground" Value="{DynamicResource Text}"/><Setter Property="Padding" Value="12,8"/><Setter Property="Template"><Setter.Value><ControlTemplate TargetType="MenuItem"><Border x:Name="MenuSurface" Background="Transparent" CornerRadius="8" Padding="{TemplateBinding Padding}"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="20"/><ColumnDefinition/></Grid.ColumnDefinitions><TextBlock x:Name="Checkmark" Text="✓" Visibility="Hidden" Foreground="{DynamicResource Accent}"/><ContentPresenter Grid.Column="1" ContentSource="Header" RecognizesAccessKey="True"/></Grid></Border><ControlTemplate.Triggers><Trigger Property="IsHighlighted" Value="True"><Setter TargetName="MenuSurface" Property="Background" Value="{DynamicResource SurfaceHover}"/></Trigger><Trigger Property="IsChecked" Value="True"><Setter TargetName="Checkmark" Property="Visibility" Value="Visible"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter Property="Opacity" Value="0.4"/></Trigger></ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter></Style>
 </Window.Resources>
 <Border x:Name="WindowFrame" Background="{DynamicResource WindowBg}" BorderBrush="{DynamicResource Border}" BorderThickness="1">
  <Grid Background="{DynamicResource WindowBg}">
   <Grid.RowDefinitions><RowDefinition Height="52"/><RowDefinition Height="*"/></Grid.RowDefinitions>
  <Border x:Name="TitleBar" Grid.Row="0" Background="{DynamicResource WindowBg}">
   <Grid>
    <Grid.ColumnDefinitions><ColumnDefinition Width="250"/><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
    <StackPanel Grid.Column="0" Orientation="Horizontal" Margin="16,0,0,0" VerticalAlignment="Center" WindowChrome.IsHitTestVisibleInChrome="True">
     <Border Width="28" Height="28" CornerRadius="8" Background="{DynamicResource AccentSoft}" Margin="0,0,10,0"><Path Data="M 2 18 L 10 2 L 18 18 M 6 12 L 14 12" Width="16" Height="16" Stretch="Uniform" Stroke="{DynamicResource Accent}" StrokeThickness="2" StrokeLineJoin="Round" VerticalAlignment="Center" HorizontalAlignment="Center"/></Border>
     <TextBlock Text="AlphaCode" Foreground="{DynamicResource Text}" FontSize="14" FontWeight="SemiBold" VerticalAlignment="Center" Margin="0,0,18,0"/>
     <Button x:Name="Back" Style="{StaticResource IconButton}" Width="30" Height="30" MinHeight="30" ToolTip="Back" AutomationProperties.Name="Back"><Path Style="{StaticResource Icon}" Width="14" Data="M 16 10 L 2 10 M 7 4 L 1 10 L 7 16"/></Button>
     <Button x:Name="Forward" Style="{StaticResource IconButton}" Width="30" Height="30" MinHeight="30" ToolTip="Forward" AutomationProperties.Name="Forward"><Path Style="{StaticResource Icon}" Width="14" Data="M 2 10 L 16 10 M 11 4 L 17 10 L 11 16"/></Button>
    </StackPanel>
    <Button x:Name="CommandMenu" Grid.Column="1" MaxWidth="420" Height="34" MinHeight="34" Margin="16,0" Padding="12,0" HorizontalContentAlignment="Stretch" Background="{DynamicResource SidebarBg}" BorderBrush="{DynamicResource Border}" Foreground="{DynamicResource Muted}" ToolTip="Search all actions (Ctrl+Shift+P)" AutomationProperties.Name="Open command palette"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><Path Style="{StaticResource Icon}" Width="14" Height="14" Data="M 14 14 L 20 20 M 16 9 A 7 7 0 1 1 2 9 A 7 7 0 1 1 16 9" Margin="0,0,10,0"/><TextBlock Grid.Column="1" Text="Search commands…" FontSize="12" VerticalAlignment="Center" TextTrimming="CharacterEllipsis"/><Border Grid.Column="2" Background="{DynamicResource Surface}" CornerRadius="4" Padding="6,3" Margin="10,0,0,0"><TextBlock Text="Ctrl ⇧ P" Foreground="{DynamicResource Dim}" FontSize="10"/></Border></Grid></Button>
    <StackPanel Grid.Column="2" Orientation="Horizontal" Margin="0,0,8,0" WindowChrome.IsHitTestVisibleInChrome="True">
     <Button x:Name="Minimize" Content="&#x2014;" Padding="11,3" Foreground="{DynamicResource Muted}" ToolTip="Minimize"/>
     <Button x:Name="Maximize" Content="&#x25A1;" Padding="11,3" Foreground="{DynamicResource Muted}" ToolTip="Maximize"/>
     <Button x:Name="Close" Content="&#x00D7;" Padding="11,3" Foreground="{DynamicResource Text}" ToolTip="Close"/>
    </StackPanel>
   </Grid>
  </Border>
  <Grid x:Name="Panel" Grid.Row="1" Visibility="Visible">
   <Grid.ColumnDefinitions><ColumnDefinition Width="68"/><ColumnDefinition Width="1"/><ColumnDefinition x:Name="SidebarColumn" Width="272"/><ColumnDefinition Width="1"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
   <Border Grid.Column="0" Background="{DynamicResource RailBg}">
    <Grid>
     <Grid.RowDefinitions><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
     <StackPanel Grid.Row="1" HorizontalAlignment="Center" Margin="0,0,0,14">
      <Button x:Name="More" Style="{StaticResource RailButton}" ToolTip="More options" AutomationProperties.Name="More options"><Path Style="{StaticResource Icon}" Data="M 3 10 L 3.1 10 M 10 10 L 10.1 10 M 17 10 L 17.1 10" StrokeThickness="3"/></Button>
      <Button x:Name="OpenSettings" Style="{StaticResource RailButton}" ToolTip="Settings" AutomationProperties.Name="Open settings"><Path Style="{StaticResource Icon}" Data="M 3 5 L 17 5 M 3 15 L 17 15 M 7 2 L 7 8 M 13 12 L 13 18"/></Button>
      <Button x:Name="ThemeToggle" Style="{StaticResource RailButton}" ToolTip="Switch to light theme" AutomationProperties.Name="Switch color theme"><Path Style="{StaticResource Icon}" Data="M 16 13 A 8 8 0 1 1 7 3 A 6 6 0 0 0 16 13 Z"/></Button>
     </StackPanel>
     <StackPanel Grid.Row="0" VerticalAlignment="Top" HorizontalAlignment="Center" Margin="0,14,0,0">
      <Button x:Name="NavHome" Style="{StaticResource RailButton}" ToolTip="Hide sidebar (Ctrl+B)" AutomationProperties.Name="Hide sidebar"><Path Style="{StaticResource Icon}" Data="M 2 2 L 18 2 L 18 18 L 2 18 Z M 7 2 L 7 18"/></Button>
      <Border Height="1" Background="{DynamicResource Border}" Margin="8,12"/>
      <Button x:Name="NavRecents" Style="{StaticResource RailButton}" ToolTip="Live chats" AutomationProperties.Name="Live chats"><Path Style="{StaticResource Icon}" Data="M 3 2 L 17 2 Q 19 2 19 4 L 19 13 Q 19 15 17 15 L 8 15 L 3 19 L 3 15 Q 1 15 1 13 L 1 4 Q 1 2 3 2 Z M 5 7 L 15 7 M 5 11 L 11 11"/></Button>
      <Button x:Name="NavProjects" Style="{StaticResource RailButton}" ToolTip="Workspace chats" AutomationProperties.Name="Workspace chats"><Path Style="{StaticResource Icon}" Data="M 1 5 L 8 5 L 10 8 L 19 8 L 19 18 L 1 18 Z M 1 8 L 1 3 L 7 3 L 9 5 L 17 5 L 17 8"/></Button>
      <Button x:Name="NavArchive" Style="{StaticResource RailButton}" ToolTip="Saved chats" AutomationProperties.Name="Saved chats"><Path Style="{StaticResource Icon}" Data="M 4 2 L 16 2 L 16 19 L 10 15 L 4 19 Z"/></Button>
      <Button x:Name="NavCompanion" Style="{StaticResource RailButton}" ToolTip="Open AlphaCode chat" AutomationProperties.Name="Open AlphaCode chat"><Path Style="{StaticResource Icon}" Data="M 10 1 L 12.5 7.5 L 19 10 L 12.5 12.5 L 10 19 L 7.5 12.5 L 1 10 L 7.5 7.5 Z"/></Button>
     </StackPanel>
    </Grid>
   </Border>
   <Border Grid.Column="1" x:Name="SidebarDivider" Background="{DynamicResource Border}"/>
   <Border Grid.Column="2" x:Name="SidebarSurface" Background="{DynamicResource SidebarBg}" Padding="16,20,16,14" CornerRadius="14,0,0,0">
    <Grid>
     <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
     <Grid Grid.Row="0" Margin="0,0,0,16">
      <Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
      <StackPanel><TextBlock Text="Your workspace" Foreground="{DynamicResource Dim}" FontSize="11" Margin="0,0,0,5"/><TextBlock Text="Conversations" Foreground="{DynamicResource Text}" FontSize="19" FontWeight="SemiBold"/></StackPanel>
      <Button x:Name="Search" Grid.Column="1" Content="Ctrl K" MinHeight="28" VerticalAlignment="Bottom" Padding="6,3" Foreground="{DynamicResource Dim}" FontSize="10" ToolTip="Search chats (Ctrl+K)" AutomationProperties.Name="Search chats"/>
     </Grid>
     <Button x:Name="NewChat" Grid.Row="1" Style="{StaticResource PrimaryButton}" HorizontalContentAlignment="Stretch" Padding="12,10" Margin="0,0,0,16" ToolTip="Start a new chat" AutomationProperties.Name="New chat"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition/></Grid.ColumnDefinitions><Path Style="{StaticResource Icon}" Width="16" Height="16" Data="M 10 2 L 10 18 M 2 10 L 18 10" Margin="0,0,10,0"/><TextBlock Grid.Column="1" Text="New conversation" VerticalAlignment="Center"/></Grid></Button>
     <StackPanel Grid.Row="2" Margin="0,0,0,9">
      <Border Background="{DynamicResource WindowBg}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="10,0" Margin="0,0,0,10"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock Text="⌕" Foreground="{DynamicResource Muted}" FontSize="19" VerticalAlignment="Center" Margin="0,0,7,0"/><TextBox x:Name="SessionSearch" Grid.Column="1" Height="34" Background="Transparent" BorderThickness="0" Foreground="{DynamicResource Text}" Padding="0" VerticalContentAlignment="Center" ToolTip="Find a chat · Enter to open · Down to select" AutomationProperties.Name="Search chats"/><TextBlock x:Name="SearchPlaceholder" Grid.Column="1" Text="Find a chat…" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" IsHitTestVisible="False"/><Button x:Name="ClearChatSearch" Grid.Column="2" Content="×" Padding="4,0" Visibility="Collapsed" ToolTip="Clear chat search" AutomationProperties.Name="Clear chat search"/></Grid></Border>
      <Border Background="{DynamicResource Surface}" CornerRadius="10" Padding="3" Margin="0,0,0,20"><UniformGrid Rows="1" Columns="3"><Button x:Name="FilterAll" Content="All" Padding="6,4" MinHeight="28" FontSize="12" Margin="0,0,3,0" ToolTip="All chats"/><Button x:Name="FilterLive" Content="Live" Padding="6,4" MinHeight="28" FontSize="12" Margin="0,0,3,0" ToolTip="Connected chats"/><Button x:Name="FilterSaved" Content="Saved" Padding="6,4" MinHeight="28" FontSize="12" ToolTip="Saved chats"/></UniformGrid></Border>
      <TextBlock Text="Workspaces" Foreground="{DynamicResource Muted}" FontWeight="SemiBold" FontSize="11" Margin="0,0,0,7"/>
      <StackPanel x:Name="ProjectsList"/>
      <TextBlock x:Name="ChatsHeading" Text="Chats" Foreground="{DynamicResource Dim}" FontWeight="SemiBold" FontSize="11" Margin="0,17,0,7"/>
     </StackPanel>
     <ListBox x:Name="Sessions" Grid.Row="3" SelectedValuePath="id" AutomationProperties.Name="Recent chats">
      <ListBox.ItemTemplate><DataTemplate><Grid ToolTip="{Binding row_tooltip}"><Grid.ColumnDefinitions><ColumnDefinition Width="22"/><ColumnDefinition/></Grid.ColumnDefinitions><TextBlock x:Name="ChatIcon" Text="{Binding row_icon}" Foreground="{DynamicResource Muted}" VerticalAlignment="Top" FontSize="13" Margin="0,1,6,0"/><StackPanel Grid.Column="1"><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock Text="{Binding title}" FontSize="13" FontWeight="SemiBold" TextTrimming="CharacterEllipsis"/><StackPanel Grid.Column="1" Orientation="Horizontal" Margin="7,0,0,0"><TextBlock Text="{Binding saved_marker}" Foreground="{DynamicResource Accent}" FontSize="10" Margin="0,0,4,0"/><TextBlock x:Name="ChatStatus" Text="{Binding status_label}" Foreground="{DynamicResource Muted}" FontSize="10"/></StackPanel></Grid><TextBlock x:Name="ChatSubtitle" Text="{Binding row_subtitle}" Foreground="{DynamicResource Dim}" FontSize="11" TextTrimming="CharacterEllipsis" Margin="0,4,0,0"/></StackPanel></Grid><DataTemplate.Triggers><DataTrigger Binding="{Binding state}" Value="working"><Setter TargetName="ChatStatus" Property="Foreground" Value="{DynamicResource Accent}"/><Setter TargetName="ChatIcon" Property="Foreground" Value="{DynamicResource Accent}"/></DataTrigger><DataTrigger Binding="{Binding state}" Value="attention"><Setter TargetName="ChatStatus" Property="Foreground" Value="{DynamicResource Warning}"/><Setter TargetName="ChatIcon" Property="Foreground" Value="{DynamicResource Warning}"/></DataTrigger><DataTrigger Binding="{Binding state}" Value="offline"><Setter TargetName="ChatStatus" Property="Foreground" Value="{DynamicResource Danger}"/></DataTrigger><DataTrigger Binding="{Binding row_subtitle}" Value=""><Setter TargetName="ChatSubtitle" Property="Visibility" Value="Collapsed"/></DataTrigger></DataTemplate.Triggers></DataTemplate></ListBox.ItemTemplate>
     </ListBox>
     <TextBlock x:Name="SessionsEmpty" Grid.Row="3" Visibility="Collapsed" Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Margin="10,12" IsHitTestVisible="False"/>
     <StackPanel Grid.Row="4" Margin="0,10,0,0">
      <Border Background="{DynamicResource WindowBg}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="10,8"><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock x:Name="SidebarStatus" Text="●  Ready" Foreground="{DynamicResource Success}" FontSize="11"/><TextBlock x:Name="SessionCount" Text="0 live" Grid.Column="1" Foreground="{DynamicResource Dim}" FontSize="11"/></Grid></Border>
      <TextBlock x:Name="SidebarHint" Text="Your local workspace is ready." Foreground="{DynamicResource Dim}" FontSize="10" Margin="2,8,0,0" TextWrapping="Wrap"/>
     </StackPanel>
    </Grid>
   </Border>
   <Border Grid.Column="3" Background="{DynamicResource Border}"/>
   <Grid x:Name="ChatSurface" Grid.Column="4" Background="{DynamicResource Canvas}">
    <Grid.RowDefinitions><RowDefinition Height="Auto" MinHeight="104"/><RowDefinition Height="Auto"/><RowDefinition Height="*" MinHeight="80"/><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="28"/></Grid.RowDefinitions>
    <Grid x:Name="ChatHeader" Grid.Row="0" Margin="28,20,28,12">
     <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
     <Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
      <StackPanel VerticalAlignment="Center" Margin="0,0,16,0"><TextBlock x:Name="WorkspaceTitle" Text="New conversation" Foreground="{DynamicResource Text}" FontSize="20" FontWeight="SemiBold" TextTrimming="CharacterEllipsis"/><TextBlock x:Name="WorkspaceSubtitle" Text="Your workspace, in focus" Foreground="{DynamicResource Muted}" FontSize="12" Margin="0,5,0,0" TextTrimming="CharacterEllipsis"/></StackPanel>
      <Border Grid.Column="1" x:Name="StatePill" Background="{DynamicResource SuccessSoft}" CornerRadius="12" Padding="12,6" VerticalAlignment="Center"><TextBlock x:Name="StateText" AutomationProperties.LiveSetting="Polite" Text="Connecting" Foreground="{DynamicResource Success}" FontSize="11" FontWeight="SemiBold"/></Border>
     </Grid>
     <WrapPanel Grid.Row="1" Margin="-6,12,0,0" KeyboardNavigation.TabNavigation="Continue">
      <Button x:Name="SidebarToggle" Content="Hide sidebar" Foreground="{DynamicResource Muted}" Padding="8,4" FontSize="12" ToolTip="Show or hide the sidebar (Ctrl+B)" AutomationProperties.Name="Show or hide sidebar"/>
      <Button x:Name="WorkToggle" Content="Show work" Padding="8,4" FontSize="12" Foreground="{DynamicResource Muted}" ToolTip="Show todos, plan and swarm status (Ctrl+J)" AutomationProperties.Name="Show or hide work panel"/>
      <Button x:Name="FocusMode" Content="Focus" Padding="8,4" FontSize="12" Foreground="{DynamicResource Muted}" ToolTip="Focus on the conversation (Ctrl+Shift+F)" AutomationProperties.Name="Toggle focus mode"/>
      <Border Width="1" Height="18" Background="{DynamicResource Border}" Margin="8,0" VerticalAlignment="Center"/>
      <Button x:Name="FindReply" Style="{StaticResource IconButton}" ToolTip="Find in conversation (Ctrl+F)" AutomationProperties.Name="Find in conversation"><Path Style="{StaticResource Icon}" Width="16" Height="16" Data="M 14 14 L 20 20 M 16 9 A 7 7 0 1 1 2 9 A 7 7 0 1 1 16 9"/></Button>
      <Button x:Name="OpenTerminal" Padding="10,4" FontSize="12" Foreground="{DynamicResource Muted}" ToolTip="Continue with the full terminal interface" AutomationProperties.Name="Open terminal interface"><StackPanel Orientation="Horizontal"><Path Style="{StaticResource Icon}" Width="16" Height="14" Data="M 2 2 L 7 7 L 2 12 M 10 12 L 18 12" Margin="0,0,8,0"/><TextBlock Text="Terminal" VerticalAlignment="Center"/></StackPanel></Button>
      <Button x:Name="Cancel" Content="Stop" Padding="10,4" FontSize="12" Foreground="{DynamicResource Danger}" ToolTip="Cancel the running turn" AutomationProperties.Name="Cancel running turn" IsEnabled="False"/>
      <Button x:Name="Rename" Style="{StaticResource IconButton}" ToolTip="Rename chat" AutomationProperties.Name="Rename chat" IsEnabled="False"><Path Style="{StaticResource Icon}" Width="16" Height="16" Data="M 3 13 L 13 3 L 17 7 L 7 17 L 2 18 Z M 11 5 L 15 9"/></Button>
      <Button x:Name="Archive" Style="{StaticResource IconButton}" Content="&#x2606;" FontSize="20" ToolTip="Save chat" AutomationProperties.Name="Save chat" IsEnabled="False"/>
      <Button x:Name="Refresh" Style="{StaticResource IconButton}" ToolTip="Refresh connection and chats" AutomationProperties.Name="Refresh connection and chats"><Path Style="{StaticResource Icon}" Width="16" Height="16" Data="M 17 8 A 8 8 0 1 0 18 13 M 17 2 L 17 8 L 11 8"/></Button>
     </WrapPanel>
    </Grid>
    <Border x:Name="NoticeBanner" Grid.Row="1" Margin="24,4,24,0" Padding="12,9" CornerRadius="8" Background="{DynamicResource WarningSoft}" Visibility="Collapsed"><TextBlock x:Name="NoticeText" Foreground="{DynamicResource Warning}" TextWrapping="Wrap" FontSize="12" AutomationProperties.LiveSetting="Polite"/></Border>
    <Grid Grid.Row="2"><Grid.ColumnDefinitions><ColumnDefinition x:Name="ConversationColumn" Width="*"/><ColumnDefinition x:Name="WorkDividerColumn" Width="0"/><ColumnDefinition x:Name="WorkColumn" Width="0"/></Grid.ColumnDefinitions><Grid Grid.Column="0">
     <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="*"/></Grid.RowDefinitions>
     <Border x:Name="ReplySearchBar" Grid.Row="0" Visibility="Collapsed" Margin="28,8,28,0" Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,3"><DockPanel LastChildFill="True"><Button x:Name="CloseReplySearch" DockPanel.Dock="Right" Content="×" Padding="8,0" ToolTip="Close conversation search (Esc)" AutomationProperties.Name="Close conversation search"/><TextBlock x:Name="ReplySearchCount" DockPanel.Dock="Right" Foreground="{DynamicResource Muted}" FontSize="11" Margin="8,0" VerticalAlignment="Center"/><TextBox x:Name="ReplySearch" Background="Transparent" Foreground="{DynamicResource Text}" BorderThickness="0" VerticalContentAlignment="Center" ToolTip="Find in conversation · Enter / Shift+Enter for next / previous section" AutomationProperties.Name="Find in conversation"/></DockPanel></Border>
    <Grid Grid.Row="1" x:Name="EmptyState" VerticalAlignment="Center" HorizontalAlignment="Center">
     <StackPanel HorizontalAlignment="Center" MaxWidth="700" Margin="24,8">
      <Border x:Name="WelcomeMark" Width="56" Height="56" CornerRadius="18" Background="{DynamicResource AccentSoft}" HorizontalAlignment="Center" Margin="0,0,0,20"><Path Data="M 12 1 L 15 9 L 23 12 L 15 15 L 12 23 L 9 15 L 1 12 L 9 9 Z" Stroke="{DynamicResource Accent}" StrokeThickness="1.6" StrokeLineJoin="Round" Width="26" Height="26" Stretch="Uniform" HorizontalAlignment="Center" VerticalAlignment="Center"/></Border>
      <TextBlock x:Name="Activity" Text="What will you build today?" Foreground="{DynamicResource Text}" FontFamily="Segoe UI Variable Display, Segoe UI" FontSize="30" HorizontalAlignment="Center" TextAlignment="Center" TextWrapping="Wrap" FontWeight="SemiBold"/>
      <TextBlock x:Name="Detail" Text="A thinking partner for your code, ideas, and next big change." Foreground="{DynamicResource Muted}" FontSize="14" HorizontalAlignment="Center" Margin="0,12,0,0" MaxWidth="500" TextAlignment="Center" TextWrapping="Wrap"/>
      <TextBlock x:Name="Timeline" Text="" Foreground="{DynamicResource Dim}" FontSize="11" HorizontalAlignment="Center" Margin="0,10,0,0" TextAlignment="Center" TextWrapping="Wrap"/>
      <WrapPanel x:Name="ExamplesPanel" HorizontalAlignment="Center" Margin="0,28,0,0">
       <Button x:Name="ExampleExplore" Style="{StaticResource SecondaryButton}" Width="190" HorizontalContentAlignment="Left" Padding="16" Margin="5" ToolTip="Start with a workspace overview"><StackPanel><Path Style="{StaticResource Icon}" Width="20" Height="20" HorizontalAlignment="Left" Stroke="{DynamicResource Accent}" Data="M 1 5 L 8 5 L 10 8 L 19 8 L 19 18 L 1 18 Z M 1 8 L 1 3 L 7 3 L 9 5 L 17 5 L 17 8" Margin="0,0,0,16"/><TextBlock Text="Explore workspace" FontWeight="SemiBold"/><TextBlock Text="Find your way through the code." FontSize="12" Foreground="{DynamicResource Muted}" Margin="0,7,0,0" TextWrapping="Wrap"/></StackPanel></Button>
       <Button x:Name="ExampleReview" Style="{StaticResource SecondaryButton}" Width="190" HorizontalContentAlignment="Left" Padding="16" Margin="5" ToolTip="Ask AlphaCode to review local changes"><StackPanel><Path Style="{StaticResource Icon}" Width="20" Height="20" HorizontalAlignment="Left" Stroke="{DynamicResource Accent}" Data="M 5 1 L 17 1 L 17 19 L 3 19 L 3 1 Z M 7 6 L 13 6 M 7 10 L 13 10 M 7 14 L 11 14" Margin="0,0,0,16"/><TextBlock Text="Review changes" FontWeight="SemiBold"/><TextBlock Text="Spot issues before they ship." FontSize="12" Foreground="{DynamicResource Muted}" Margin="0,7,0,0" TextWrapping="Wrap"/></StackPanel></Button>
       <Button x:Name="ExampleTest" Style="{StaticResource SecondaryButton}" Width="190" HorizontalContentAlignment="Left" Padding="16" Margin="5" ToolTip="Ask AlphaCode to check the project"><StackPanel><Path Style="{StaticResource Icon}" Width="20" Height="20" HorizontalAlignment="Left" Stroke="{DynamicResource Accent}" Data="M 2 10 L 7 15 L 18 4 M 18 10 L 18 18 L 2 18 L 2 2 L 12 2" Margin="0,0,0,16"/><TextBlock Text="Check the project" FontWeight="SemiBold"/><TextBlock Text="Test, investigate, and improve." FontSize="12" Foreground="{DynamicResource Muted}" Margin="0,7,0,0" TextWrapping="Wrap"/></StackPanel></Button>
      </WrapPanel>
     </StackPanel>
    </Grid>
    <ScrollViewer x:Name="ReplyScroll" Grid.Row="1" VerticalScrollBarVisibility="Auto" Margin="28,20,28,16" Visibility="Collapsed">
     <Grid MaxWidth="840"><TextBox x:Name="Reply" Visibility="Collapsed" IsReadOnly="True" FontSize="14" AutomationProperties.Name="Raw conversation"/><StackPanel x:Name="ConversationCards"/></Grid>
    </ScrollViewer>
    <Button x:Name="JumpLatest" Grid.Row="1" Content="Jump to latest ↓" HorizontalAlignment="Right" VerticalAlignment="Bottom" Margin="0,0,48,10" Background="{DynamicResource SurfaceElevated}" BorderBrush="{DynamicResource Accent}" BorderThickness="1" Visibility="Collapsed" ToolTip="Return to the latest response (Ctrl+End)" AutomationProperties.Name="Jump to latest response"/>
    </Grid>
    <GridSplitter x:Name="WorkflowResize" Grid.Column="1" HorizontalAlignment="Stretch" VerticalAlignment="Stretch" Background="Transparent" ResizeDirection="Columns" ResizeBehavior="PreviousAndNext" ToolTip="Drag to resize the work panel" AutomationProperties.Name="Resize work panel"/>
    <Border x:Name="WorkflowSurface" Grid.Column="2" Visibility="Collapsed" Background="{DynamicResource SidebarBg}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Margin="12,16,24,8" Padding="14">
     <ScrollViewer x:Name="WorkflowScroll" VerticalScrollBarVisibility="Auto" HorizontalScrollBarVisibility="Disabled">
      <StackPanel>
       <TextBlock Text="Work overview" Foreground="{DynamicResource Text}" FontSize="16" FontWeight="SemiBold"/>
       <TextBlock x:Name="WorkflowSummary" Text="Open a live chat to see its work." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" FontSize="11" Margin="0,6,0,16" AutomationProperties.LiveSetting="Polite"/>
<Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="12" Margin="0,0,0,12"><StackPanel><TextBlock Text="Tasks" Foreground="{DynamicResource Text}" FontWeight="SemiBold"/><TextBox IsReadOnly="True" Background="Transparent" BorderThickness="0" Padding="0,8,0,0" x:Name="WorkflowTodos" Text="Tasks appear here as work progresses." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" /></StackPanel></Border>
<Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="12" Margin="0,0,0,12"><StackPanel><TextBlock Text="Plan" Foreground="{DynamicResource Text}" FontWeight="SemiBold"/><TextBox IsReadOnly="True" Background="Transparent" BorderThickness="0" Padding="0,8,0,0" x:Name="WorkflowPlan" Text="A plan will appear when the agent creates one." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" /></StackPanel></Border>
<Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="12" Margin="0,0,0,12"><StackPanel><TextBlock Text="Agents" Foreground="{DynamicResource Text}" FontWeight="SemiBold"/><TextBox IsReadOnly="True" Background="Transparent" BorderThickness="0" Padding="0,8,0,0" x:Name="WorkflowSwarm" Text="Agent activity appears here during parallel work." Foreground="{DynamicResource Muted}" TextWrapping="Wrap"/></StackPanel></Border>
      </StackPanel>
     </ScrollViewer>
    </Border></Grid>
    <Border x:Name="ResponseActivity" Grid.Row="3" Visibility="Collapsed" Background="{DynamicResource AccentSoft}" CornerRadius="8" Margin="24,0,24,4" Padding="14,10" MaxWidth="1000" AutomationProperties.LiveSetting="Polite"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="Auto"/><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel Orientation="Horizontal" VerticalAlignment="Center" Margin="0,0,12,0"><Ellipse x:Name="LoadingDot1" Width="5" Height="5" Fill="{DynamicResource Accent}" Opacity="0.3" Margin="0,0,4,0"/><Ellipse x:Name="LoadingDot2" Width="5" Height="5" Fill="{DynamicResource Accent}" Opacity="0.3" Margin="0,0,4,0"/><Ellipse x:Name="LoadingDot3" Width="5" Height="5" Fill="{DynamicResource Accent}" Opacity="0.3"/></StackPanel><StackPanel Grid.Column="1"><TextBlock x:Name="ResponseStage" Foreground="{DynamicResource Accent}" FontWeight="SemiBold" FontSize="12" Text="AlphaCode is working"/><TextBlock x:Name="ResponseDetail" Foreground="{DynamicResource Muted}" FontSize="11" TextTrimming="CharacterEllipsis" Margin="0,3,0,0"/></StackPanel><StackPanel Grid.Column="2" Orientation="Horizontal" VerticalAlignment="Center" Margin="10,0,0,0"><TextBlock x:Name="ResponseElapsed" Foreground="{DynamicResource Muted}" FontSize="11" VerticalAlignment="Center"/><Button x:Name="ResponseStop" Content="Stop" Foreground="{DynamicResource Danger}" Padding="8,4" Margin="8,0,0,0" ToolTip="Stop the current turn" AutomationProperties.Name="Stop response"/></StackPanel></Grid></Border>
    <Border x:Name="ComposerSurface" Grid.Row="4" Background="{DynamicResource SurfaceElevated}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="16" Margin="28,12,28,8" Padding="0" MaxWidth="840">
     <Grid>
      <Grid x:Name="ComposerContent" Margin="16,14">
       <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
       <Grid Grid.Row="0">
        <TextBox x:Name="Prompt" Style="{DynamicResource ComposerInput}" MinHeight="40" MaxHeight="160" MaxLength="16000" AcceptsReturn="True" TextWrapping="Wrap" VerticalScrollBarVisibility="Auto" AutomationProperties.Name="Prompt"/>
        <TextBox x:Name="CommandInput" Style="{DynamicResource ComposerInput}" MinHeight="40" MaxHeight="160" MaxLength="16000" Visibility="Collapsed" AcceptsReturn="True" TextWrapping="Wrap" VerticalScrollBarVisibility="Auto" AutomationProperties.Name="Command input"/>
        <PasswordBox x:Name="SecretInput" MinHeight="40" MaxLength="16000" Visibility="Collapsed" Background="Transparent" Foreground="{DynamicResource Text}" CaretBrush="{DynamicResource Accent}" BorderThickness="0" FocusVisualStyle="{x:Null}" Padding="0" FontSize="15" AutomationProperties.Name="Password for running command"/>
        <TextBlock x:Name="Placeholder" Text="Message AlphaCode…" Foreground="{DynamicResource Muted}" FontSize="15" TextWrapping="Wrap" Margin="1,1,0,0" IsHitTestVisible="False"/>
       </Grid>
       <Grid Grid.Row="1" Margin="0,8,0,0">
        <Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
        <WrapPanel VerticalAlignment="Center" Margin="-4,0,12,0">
         <Grid Margin="0,0,4,0">
          <ComboBox x:Name="ModelPicker" Visibility="Collapsed" IsTextSearchEnabled="True" AutomationProperties.Name="Model"/>
          <Button x:Name="ModelChooser" Width="220" Height="32" HorizontalContentAlignment="Stretch" Background="Transparent" Foreground="{DynamicResource Text}" Padding="8,4" ToolTip="Models from connected providers (Ctrl+M)" AutomationProperties.Name="Choose model"><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock x:Name="ModelChoiceLabel" Text="Choose model" TextTrimming="CharacterEllipsis" VerticalAlignment="Center" FontSize="12"/><TextBlock Grid.Column="1" Text="⌄" Margin="8,0,0,0" Foreground="{DynamicResource Muted}"/></Grid></Button>
          <Popup x:Name="ModelPopup" PlacementTarget="{Binding ElementName=ModelChooser}" Placement="Top" StaysOpen="False" AllowsTransparency="True" PopupAnimation="Fade"><Border x:Name="ModelPopupSurface" Width="400" Background="{DynamicResource SurfaceElevated}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="16" Margin="0,0,0,8"><StackPanel><TextBlock Text="Choose a model" Foreground="{DynamicResource Text}" FontSize="16" FontWeight="SemiBold"/><TextBlock Text="Connected providers only · search to narrow the list" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,4,0,12"/><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="10,7"><TextBox x:Name="ModelSearch" Background="Transparent" Foreground="{DynamicResource Text}" BorderThickness="0" Padding="0" ToolTip="Search models" AutomationProperties.Name="Search models"/></Border><TextBlock x:Name="ModelResultCount" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,8,0,6"/><ListBox x:Name="ModelResults" SelectedValuePath="id" MaxHeight="252" MinHeight="48" ScrollViewer.CanContentScroll="True" VirtualizingPanel.IsVirtualizing="True" VirtualizingPanel.VirtualizationMode="Recycling" AutomationProperties.Name="Connected provider models"><ListBox.ItemTemplate><DataTemplate><Grid Margin="2,4"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="{Binding name}" FontWeight="SemiBold" TextTrimming="CharacterEllipsis" ToolTip="{Binding id}"/><TextBlock Text="{Binding provider}" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,3,0,0" TextTrimming="CharacterEllipsis"/></StackPanel><TextBlock Grid.Column="1" Text="✓" Foreground="{DynamicResource Accent}" Margin="12,0,4,0" VerticalAlignment="Center"><TextBlock.Style><Style TargetType="TextBlock"><Setter Property="Visibility" Value="Collapsed"/><Style.Triggers><DataTrigger Binding="{Binding RelativeSource={RelativeSource AncestorType=ListBoxItem},Path=IsSelected}" Value="True"><Setter Property="Visibility" Value="Visible"/></DataTrigger></Style.Triggers></Style></TextBlock.Style></TextBlock></Grid></DataTemplate></ListBox.ItemTemplate></ListBox><TextBlock x:Name="ModelEmpty" Text="No matching models. Try another search." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Visibility="Collapsed" Margin="0,10"/><DockPanel LastChildFill="False" Margin="0,12,0,0"><TextBlock Text="↑ ↓ select · Enter apply · Esc close" Foreground="{DynamicResource Dim}" FontSize="10" VerticalAlignment="Center"/><Button x:Name="ApplyModelChoice" DockPanel.Dock="Right" Content="Use model" Background="{DynamicResource Accent}" Foreground="{DynamicResource AccentText}" Padding="12,6"/></DockPanel></StackPanel></Border></Popup>
         </Grid>
         <ComboBox x:Name="EffortPicker" Width="88" Height="32" Margin="0,0,8,0" SelectedValuePath="Tag" ToolTip="Reasoning effort" AutomationProperties.Name="Reasoning effort"><ComboBoxItem Content="Auto" Tag="none"/><ComboBoxItem Content="Low" Tag="low"/><ComboBoxItem Content="Medium" Tag="medium"/><ComboBoxItem Content="High" Tag="high"/><ComboBoxItem Content="Max" Tag="max"/></ComboBox>
         <Button x:Name="AddContext" Style="{StaticResource IconButton}" Width="32" Height="32" MinHeight="32" ToolTip="Choose a workspace folder" AutomationProperties.Name="Choose workspace folder"><Path Style="{StaticResource Icon}" Width="16" Height="16" Data="M 1 5 L 8 5 L 10 8 L 19 8 L 19 18 L 1 18 Z M 5 13 L 15 13 M 10 9 L 10 17"/></Button>
         <TextBlock x:Name="WorkspacePath" Text="Workspace" Foreground="{DynamicResource Muted}" FontSize="11" MaxWidth="120" TextTrimming="CharacterEllipsis" VerticalAlignment="Center" Margin="0,0,6,0"/>
         <Button x:Name="ComposerSettings" Style="{StaticResource IconButton}" Width="32" Height="32" MinHeight="32" ToolTip="Chat options" AutomationProperties.Name="Chat options"><Path Style="{StaticResource Icon}" Width="16" Height="16" Data="M 3 5 L 17 5 M 3 15 L 17 15 M 7 2 L 7 8 M 13 12 L 13 18"/></Button>
        </WrapPanel>
        <StackPanel Grid.Column="1" Orientation="Horizontal" VerticalAlignment="Bottom">
         <Button x:Name="Voice" Content="&#x266B;" Width="30" Padding="0" Visibility="Collapsed" IsEnabled="False" ToolTip="Voice input" AutomationProperties.Name="Voice input"/>
         <Button x:Name="Copy" Content="Copy" Padding="8,4" Foreground="{DynamicResource Muted}" Margin="0,0,8,0" ToolTip="Copy response (Ctrl+Shift+C)" AutomationProperties.Name="Copy response" IsEnabled="False"/>
         <Button x:Name="Send" Style="{StaticResource PrimaryButton}" Content="↑" Width="40" Height="40" Padding="0" FontSize="22" ToolTip="Send prompt (Enter; Shift+Enter or Alt+Enter for a new line)" AutomationProperties.Name="Send prompt" IsEnabled="False"/>
        </StackPanel>
       </Grid>
       <Grid Grid.Row="2" Margin="0,8,0,0"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
        <TextBlock x:Name="ComposerHint" Foreground="{DynamicResource Dim}" FontSize="10" Text="Enter to send · Shift+Enter or Alt+Enter for a new line" TextTrimming="CharacterEllipsis"/>
        <TextBlock x:Name="PromptCount" Grid.Column="1" Foreground="{DynamicResource Dim}" FontSize="10" Text="0 / 16,000" Visibility="Collapsed"/>
       </Grid>
      </Grid>
      <Border x:Name="ComposerFocusRing" BorderBrush="{DynamicResource Accent}" BorderThickness="2" CornerRadius="16" Opacity="0" IsHitTestVisible="False"/>
     </Grid>
    </Border>
    <TextBlock x:Name="FooterStatus" Grid.Row="5" Text="Ready  ·  Enter to send  ·  Shift+Enter or Alt+Enter for a new line" Foreground="{DynamicResource Muted}" FontSize="11" Margin="30,0,30,0" VerticalAlignment="Center"/>
   </Grid>
  </Grid>
  <Grid x:Name="CommandOverlay" Grid.RowSpan="2" Panel.ZIndex="100" Visibility="Collapsed" Background="#99000000" KeyboardNavigation.TabNavigation="Cycle">
   <Border x:Name="CommandPaletteSurface" MaxWidth="600" VerticalAlignment="Top" HorizontalAlignment="Stretch" Margin="24,80,24,24" Background="{DynamicResource SurfaceElevated}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="20">
    <StackPanel>
     <Grid Margin="0,0,0,14"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="What would you like to do?" FontSize="20" FontWeight="SemiBold"/><TextBlock Text="Search actions, navigation, and settings" Foreground="{DynamicResource Muted}" Margin="0,5,0,0"/></StackPanel><Button x:Name="CloseCommands" Grid.Column="1" Content="×" FontSize="20" Padding="8,0" VerticalAlignment="Top" ToolTip="Close commands (Esc)" AutomationProperties.Name="Close command palette"/></Grid>
     <Border Background="{DynamicResource WindowBg}" BorderBrush="{DynamicResource Accent}" BorderThickness="1" CornerRadius="8" Padding="12,10"><TextBox x:Name="CommandSearch" Background="Transparent" BorderThickness="0" Foreground="{DynamicResource Text}" FontSize="15" AutomationProperties.Name="Search commands" ToolTip="Type an action, such as theme, model, or workspace"/></Border>
     <ListBox x:Name="CommandResults" Margin="0,12,0,8" MaxHeight="340" ScrollViewer.VerticalScrollBarVisibility="Auto" AutomationProperties.Name="Available commands"><ListBox.ItemTemplate><DataTemplate><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="{Binding Title}" FontWeight="SemiBold"/><TextBlock Text="{Binding Description}" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,4,12,0" TextWrapping="Wrap"/></StackPanel><TextBlock Grid.Column="1" Text="{Binding Shortcut}" Foreground="{DynamicResource Dim}" FontSize="11" VerticalAlignment="Center"/></Grid></DataTemplate></ListBox.ItemTemplate></ListBox>
     <TextBlock x:Name="CommandEmpty" Text="No matching actions. Try model, theme, or chat." Visibility="Collapsed" Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Margin="8,14"/>
     <TextBlock Text="↑ ↓ navigate    Enter run    Esc close" Foreground="{DynamicResource Dim}" FontSize="11" Margin="8,6,0,0"/>
    </StackPanel>
   </Border>
  </Grid>
  </Grid>
 </Border>
</Window>
'@
function Get-ExceptionChain([Exception]$exception) {
    $parts = New-Object System.Collections.Generic.List[string]
    $current = $exception
    while ($null -ne $current) {
        if ($current.Message) { $parts.Add(('{0}: {1}' -f $current.GetType().FullName, $current.Message)) }
        $current = $current.InnerException
    }
    return ($parts -join [Environment]::NewLine)
}

function Write-GuiStartupFailure($failure) {
    Write-GuiFailure 'startup' 'Window.Initialize/ShowDialog' $failure
}

function Load-GuiWindow([xml]$document) {
    try {
        $reader = New-Object System.Xml.XmlNodeReader $document
        return [Windows.Markup.XamlReader]::Load($reader)
    } catch {
        $originalError = Get-ExceptionChain $_.Exception

        # Some WPF environments reject layered windows during construction
        # (remote sessions, restricted graphics drivers, and older desktop
        # composition settings). Retry with an opaque surface; all controls and
        # behavior remain available.
        try {
            $fallbackText = $document.OuterXml.Replace('AllowsTransparency="True"', 'AllowsTransparency="False"').Replace('Background="Transparent"', 'Background="#1A1D2D"')
            [xml]$fallback = $fallbackText
            $fallbackReader = New-Object System.Xml.XmlNodeReader $fallback
            $loaded = [Windows.Markup.XamlReader]::Load($fallbackReader)
            $script:startupCompatibilityNotice = 'Transparency is unavailable, so the GUI is using compatibility rendering.'
            return $loaded
        } catch {
            $fallbackError = Get-ExceptionChain $_.Exception
            $details = "AlphaCode GUI could not load its WPF window.$([Environment]::NewLine)$([Environment]::NewLine)Original XAML load:$([Environment]::NewLine)$originalError$([Environment]::NewLine)$([Environment]::NewLine)Compatibility retry:$([Environment]::NewLine)$fallbackError"
            Write-GuiStartupFailure $_
            throw $details
        }
    }
}

$window = Load-GuiWindow $xaml

$dispatcherExceptionHandler = [Windows.Threading.DispatcherUnhandledExceptionEventHandler]{
        # $args is PowerShell's automatic remaining-argument array, even when
        # named in param(). It is NOT the Dispatcher event and has no Handled.
        param($sender, [Windows.Threading.DispatcherUnhandledExceptionEventArgs]$eventArgs)
        if ($null -eq $eventArgs) { return }
        $recoverable = -not [AlphaCode.Desktop.GuiDiagnostics]::IsFatal($eventArgs.Exception)
        try {
            Write-GuiFailure 'dispatcher' 'Dispatcher.UnhandledException' $eventArgs.Exception
            if ($recoverable) { Show-GuiFailureNotice }
        } finally { $eventArgs.Handled = $recoverable }
}
$window.Dispatcher.add_UnhandledException($dispatcherExceptionHandler)

$controls = @{}
foreach ($name in @('CommandOverlay','CommandPaletteSurface','CommandSearch','CommandResults','CommandEmpty','CloseCommands','FocusMode','CommandMenu','ResponseActivity','ResponseStage','ResponseDetail','ResponseElapsed','ResponseStop','LoadingDot1','LoadingDot2','LoadingDot3','ModelChooser','ModelChoiceLabel','ModelPopup','ModelPopupSurface','ModelSearch','ModelResults','ModelResultCount','ModelEmpty','ApplyModelChoice','ClearChatSearch','FilterAll','FilterLive','FilterSaved','ChatsHeading','ReplySearchBar','ReplySearch','ReplySearchCount','CloseReplySearch','FindReply','ConversationCards','WorkflowResize','WorkDividerColumn','WorkflowScroll','WorkToggle','WorkColumn','ConversationColumn','WorkflowSurface','WorkflowSummary','WorkflowTodos','WorkflowPlan','WorkflowSwarm','ExamplesPanel','NoticeBanner','NoticeText','ChatSurface','ComposerSurface','ComposerContent','ComposerFocusRing','ComposerHint','PromptCount','ThemeToggle','JumpLatest','Panel','TitleBar','Back','Forward','SidebarToggle','SidebarColumn','SidebarDivider','SidebarSurface','Minimize','Maximize','Close','Cancel','Rename','Archive','Sessions','SessionsEmpty','ProjectsList','SessionSearch','SearchPlaceholder','ModelPicker','EffortPicker','WorkspacePath','OpenTerminal','ExampleExplore','ExampleReview','ExampleTest','Activity','Detail','Timeline','Reply','ReplyScroll','Prompt','CommandInput','SecretInput','Placeholder','EmptyState','Send','Copy','Refresh','AddContext','Voice','NewChat','NavHome','NavRecents','NavProjects','NavArchive','NavCompanion','More','Search','ComposerSettings','SidebarHint','SidebarStatus','SessionCount','StatePill','StateText','FooterStatus','WorkspaceTitle','WorkspaceSubtitle')) {
    $controls[$name] = $window.FindName($name)
}
foreach ($name in @('OpenSettings','ChatHeader','WelcomeMark')) { $controls[$name] = $window.FindName($name) }


function Toggle-FocusMode {
    if (-not $script:focusMode) {
        $script:focusSidebar = $script:sidebarVisible
        $script:focusWork = $script:workflowVisible
        $script:sidebarAutoHidden = $false
        $script:focusMode = $true
        Set-SidebarVisibility $false $false
        Set-WorkflowVisibility $false $false
    } else {
        $script:focusMode = $false
        Set-SidebarVisibility $script:focusSidebar $false
        Set-WorkflowVisibility $script:focusWork $false
    }
    $controls.FocusMode.Content = if ($script:focusMode) { 'Exit focus' } else { 'Focus' }
    $controls.FocusMode.SetResourceReference([Windows.Controls.Control]::ForegroundProperty, $(if ($script:focusMode) { 'Accent' } else { 'Muted' }))
    $controls.FocusMode.SetValue([Windows.Automation.AutomationProperties]::ItemStatusProperty, $(if ($script:focusMode) { 'Enabled' } else { 'Disabled' }))
}

# A single discoverable entry point for existing, working desktop actions.
function Update-CommandResults {
    $query = $controls.CommandSearch.Text.Trim()
    $terms = @($query -split '\s+' | Where-Object { $_ })
    $matches = @($script:commandActions | Where-Object {
        $candidate = $_; $haystack = ($candidate.Title + ' ' + $candidate.Description + ' ' + $candidate.Shortcut)
        $include = $true
        foreach ($term in $terms) { if ($haystack.IndexOf($term, [StringComparison]::OrdinalIgnoreCase) -lt 0) { $include = $false; break } }
        $include
    })
    $controls.CommandResults.ItemsSource = $matches
    $controls.CommandEmpty.Visibility = if ($matches.Count) { 'Collapsed' } else { 'Visible' }
    if ($matches.Count) { $controls.CommandResults.SelectedIndex = 0 }
}
function Close-CommandPalette {
    $controls.CommandOverlay.Visibility = 'Collapsed'
    $controls.Panel.IsEnabled = $true
    if ($script:commandPreviousFocus -and $script:commandPreviousFocus.IsVisible -and $script:commandPreviousFocus.IsEnabled) { $script:commandPreviousFocus.Focus() | Out-Null }
}
function Open-CommandPalette {
    $script:commandPreviousFocus = [Windows.Input.Keyboard]::FocusedElement
    $controls.ModelPopup.IsOpen = $false
    $script:commandActions = @(
        [pscustomobject]@{ Title='New chat'; Description='Start a fresh conversation in this workspace'; Shortcut='Ctrl+N'; Target='NewChat' }
        [pscustomobject]@{ Title='Search chats'; Description='Find a recent or saved conversation'; Shortcut='Ctrl+K'; Target='Search' }
        [pscustomobject]@{ Title='Choose model'; Description='Search models from connected providers'; Shortcut='Ctrl+M'; Target='ModelChooser' }
        [pscustomobject]@{ Title='Find in conversation'; Description='Search the open conversation'; Shortcut='Ctrl+F'; Target='FindReply' }
        [pscustomobject]@{ Title='Toggle focus mode'; Description='Hide side panels and concentrate on the conversation'; Shortcut='Ctrl+Shift+F'; Target='FocusMode' }
        [pscustomobject]@{ Title='Toggle work panel'; Description='Follow todos, plans, and agent activity'; Shortcut='Ctrl+J'; Target='WorkToggle' }
        [pscustomobject]@{ Title='Toggle sidebar'; Description='Make more room for your conversation'; Shortcut='Ctrl+B'; Target='SidebarToggle' }
        [pscustomobject]@{ Title='Switch color theme'; Description='Change between light and dark appearance'; Shortcut=''; Target='ThemeToggle' }
        [pscustomobject]@{ Title='Settings'; Description='Appearance, reading size, workspace, and keyboard'; Shortcut=''; Target='ComposerSettings' }
        [pscustomobject]@{ Title='Choose workspace folder'; Description='Start a chat for another project'; Shortcut=''; Target='AddContext' }
        [pscustomobject]@{ Title='Open terminal'; Description='Continue with the full terminal interface'; Shortcut=''; Target='OpenTerminal' }
        [pscustomobject]@{ Title='Copy response'; Description='Copy the current response to your clipboard'; Shortcut='Ctrl+Shift+C'; Target='Copy' }
        [pscustomobject]@{ Title='Stop response'; Description='Cancel the running turn'; Shortcut=''; Target='Cancel' }
    ) | Where-Object { $controls[$_.Target].IsEnabled }
    $controls.CommandSearch.Clear()
    Update-CommandResults
    $controls.Panel.IsEnabled = $false
    $controls.CommandOverlay.Visibility = 'Visible'
    $controls.CommandSearch.Focus() | Out-Null
}
function Invoke-SelectedCommand {
    $command = $controls.CommandResults.SelectedItem
    if (-not $command) { return }
    $target = $controls[$command.Target]
    Close-CommandPalette
    if ($target.IsEnabled) { $target.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent)) }
}

function Get-GuiBrush([string]$role) { return $window.Resources[$role] }


# Render model output as data. No response text is evaluated as XAML or script.
function Convert-ResponseBlocks([string]$text) {
    $blocks = New-Object System.Collections.Generic.List[object]
    $buffer = New-Object System.Collections.Generic.List[string]
    $codeLines = New-Object System.Collections.Generic.List[string]
    $fence = ''; $language = ''; $role = 'AlphaCode'
    $flush = {
        if ($buffer.Count) { $blocks.Add([pscustomobject]@{ Kind='text'; Body=($buffer -join "`n"); Language=''; Role=$role }); $buffer.Clear() }
    }
    foreach ($line in [regex]::Split($text, '\r?\n')) {
        if ($fence) {
            $closing = '^\s*' + [regex]::Escape([string]$fence[0]) + '{' + $fence.Length + ',}\s*$'
            if ($line -match $closing) {
                $blocks.Add([pscustomobject]@{ Kind='code'; Body=($codeLines -join "`n"); Language=$language; Role=$role })
                $codeLines.Clear(); $fence = ''; $language = ''
            } else { $codeLines.Add($line) }
            continue
        }
        $open = [regex]::Match($line, '^\s*(`{3,}|~{3,})(.*)$')
        if ($open.Success) {
            & $flush
            $fence = $open.Groups[1].Value
            $language = (($open.Groups[2].Value.Trim() -split '\s+')[0]).ToLowerInvariant()
            continue
        }
        if ([string]::IsNullOrWhiteSpace($line)) { & $flush; continue }
        if ($line -in @('You','AlphaCode','Session')) {
            & $flush; $role = $line
            $blocks.Add([pscustomobject]@{ Kind='role'; Body=$line; Language=''; Role=$role }); continue
        }
        $heading = [regex]::Match($line, '^(#{1,6})\s+(.+)$')
        if ($heading.Success) {
            & $flush
            $blocks.Add([pscustomobject]@{ Kind='heading'; Body=$heading.Groups[2].Value; Language=$heading.Groups[1].Value.Length; Role=$role }); continue
        }
        if ($line -match '^\s*⚙ tools:\s*(.+)$') {
            & $flush
            $blocks.Add([pscustomobject]@{ Kind='tools'; Body=$Matches[1]; Language=''; Role=$role }); continue
        }
        $display = $line -replace '^\s*[-*+]\s+\[ \]\s*','☐ ' -replace '^\s*[-*+]\s+\[x\]\s*','☑ ' -replace '^\s*[-*+]\s+','• '
        $buffer.Add($display)
    }
    & $flush
    if ($fence) { $blocks.Add([pscustomobject]@{ Kind='code'; Body=($codeLines -join "`n"); Language=$language; Role=$role }) }
    return $blocks.ToArray()
}

function Add-DisplayRuns($paragraph, [string]$text, [bool]$strong = $false, [bool]$inlineCode = $false) {
    $query = [string]$controls.ReplySearch.Text
    $matches = if ($query) { [regex]::Matches($text, [regex]::Escape($query), [Text.RegularExpressions.RegexOptions]::IgnoreCase) } else { @() }
    $position = 0
    foreach ($match in $matches) {
        if ($match.Index -gt $position) {
            $run = [Windows.Documents.Run]::new($text.Substring($position, $match.Index - $position))
            if ($strong) { $run.FontWeight = [Windows.FontWeights]::SemiBold }
            if ($inlineCode) { $run.FontFamily = [Windows.Media.FontFamily]::new('Consolas'); $run.SetResourceReference([Windows.Documents.TextElement]::BackgroundProperty, 'Surface') }
            $paragraph.Inlines.Add($run)
        }
        $hit = [Windows.Documents.Run]::new($match.Value)
        $hit.SetResourceReference([Windows.Documents.TextElement]::BackgroundProperty, 'AccentSoft')
        $hit.SetResourceReference([Windows.Documents.TextElement]::ForegroundProperty, 'Accent')
        $hit.FontWeight = [Windows.FontWeights]::Bold
        $paragraph.Inlines.Add($hit)
        $position = $match.Index + $match.Length
    }
    if ($position -lt $text.Length) {
        $run = [Windows.Documents.Run]::new($text.Substring($position))
        if ($strong) { $run.FontWeight = [Windows.FontWeights]::SemiBold }
        if ($inlineCode) { $run.FontFamily = [Windows.Media.FontFamily]::new('Consolas'); $run.SetResourceReference([Windows.Documents.TextElement]::BackgroundProperty, 'Surface') }
        $paragraph.Inlines.Add($run)
    }
}

function New-ReadableText([string]$text) {
    $view = New-Object Windows.Controls.RichTextBox
    $view.IsReadOnly = $true; $view.IsReadOnlyCaretVisible = $true
    $view.BorderThickness = [Windows.Thickness]::new(0); $view.Padding = [Windows.Thickness]::new(0)
    $view.Background = [Windows.Media.Brushes]::Transparent
    $view.SetResourceReference([Windows.Controls.Control]::ForegroundProperty, 'Text')
    $view.VerticalScrollBarVisibility = 'Disabled'; $view.HorizontalScrollBarVisibility = 'Disabled'
    $document = New-Object Windows.Documents.FlowDocument
    $document.PagePadding = [Windows.Thickness]::new(0)
    $document.FontSize = $controls.Reply.FontSize
    $document.FontFamily = [Windows.Media.FontFamily]::new('Segoe UI Variable Text, Segoe UI')
    if ($text -match '[\u0600-\u06FF\u0750-\u077F\u08A0-\u08FF]') { $document.FlowDirection = 'RightToLeft'; $document.TextAlignment = 'Right' }
    $paragraph = New-Object Windows.Documents.Paragraph
    $paragraph.Margin = [Windows.Thickness]::new(0); $paragraph.LineHeight = $controls.Reply.FontSize * 1.6
    $position = 0
    foreach ($token in [regex]::Matches($text, '(\*\*[^*\r\n]+\*\*|`[^`\r\n]+`)')) {
        if ($token.Index -gt $position) { Add-DisplayRuns $paragraph ($text.Substring($position, $token.Index - $position)) }
        if ($token.Value.StartsWith('**')) { Add-DisplayRuns $paragraph ($token.Value.Substring(2,$token.Value.Length-4)) $true }
        else { Add-DisplayRuns $paragraph ($token.Value.Substring(1,$token.Value.Length-2)) $false $true }
        $position = $token.Index + $token.Length
    }
    if ($position -lt $text.Length) { Add-DisplayRuns $paragraph ($text.Substring($position)) }
    $document.Blocks.Add($paragraph); $view.Document = $document
    return $view
}

function New-ConversationBlock($block) {
    $surface = New-Object Windows.Controls.Border
    $surface.Margin = [Windows.Thickness]::new(0,0,0,12)
    $surface.CornerRadius = [Windows.CornerRadius]::new(12)
    $surface.Tag = @{ kind=$block.Kind; body=$block.Body }
    if ($block.Kind -eq 'code') {
        $isPlan = $block.Language -eq 'plan'
        $surface.SetResourceReference([Windows.Controls.Border]::BackgroundProperty, $(if ($isPlan) { 'AccentSoft' } else { 'CodeSurface' }))
        $surface.SetResourceReference([Windows.Controls.Border]::BorderBrushProperty, 'Border')
        $surface.BorderThickness = [Windows.Thickness]::new(1); $surface.Padding = [Windows.Thickness]::new(18,12,18,16)
        $stack = New-Object Windows.Controls.StackPanel
        $header = New-Object Windows.Controls.DockPanel
        $copy = New-Object Windows.Controls.Button
        $copy.Content = if ($isPlan) { 'Copy plan' } else { 'Copy code' }
        $copy.Tag = [string]$block.Body; $copy.FontSize = 11; $copy.MinHeight = 26
        $copy.Padding = [Windows.Thickness]::new(9,3,9,3)
        $copy.SetValue([Windows.Automation.AutomationProperties]::NameProperty, [string]$copy.Content)
        $copy.Add_Click({ param($sender) if ([string]$sender.Tag) { [Windows.Clipboard]::SetText([string]$sender.Tag); Show-GuiNotice 'Block copied' } })
        [Windows.Controls.DockPanel]::SetDock($copy,'Right'); [void]$header.Children.Add($copy)
        $label = New-Object Windows.Controls.TextBlock
        $language = if ($block.Language) { [string]$block.Language } else { 'code' }
        $label.Text = $language.Substring(0,[Math]::Min(24,$language.Length)).ToUpperInvariant()
        $label.FontSize = 10; $label.FontWeight = [Windows.FontWeights]::SemiBold; $label.VerticalAlignment = 'Center'
        $label.SetResourceReference([Windows.Controls.TextBlock]::ForegroundProperty, $(if ($isPlan) { 'Accent' } else { 'Muted' }))
        [void]$header.Children.Add($label); [void]$stack.Children.Add($header)
        $code = New-Object Windows.Controls.TextBox
        $code.Text = $block.Body; $code.IsReadOnly = $true; $code.AcceptsReturn = $true
        $code.FontSize = $controls.Reply.FontSize; $code.FontFamily = [Windows.Media.FontFamily]::new($(if ($isPlan) { 'Segoe UI' } else { 'Cascadia Code, Consolas' }))
        $code.TextWrapping = if ($isPlan) { 'Wrap' } else { 'NoWrap' }
        $code.Background = [Windows.Media.Brushes]::Transparent; $code.BorderThickness = [Windows.Thickness]::new(0)
        $code.Padding = [Windows.Thickness]::new(0); $code.Margin = [Windows.Thickness]::new(0,8,0,0)
        $code.SetResourceReference([Windows.Controls.Control]::ForegroundProperty, 'Text')
        $code.HorizontalScrollBarVisibility = 'Auto'; $code.VerticalScrollBarVisibility = 'Auto'; $code.MaxHeight = 360
        [void]$stack.Children.Add($code); $surface.Child = $stack; $surface.Tag.copy = $copy
    } elseif ($block.Kind -in @('role','heading')) {
        $label = New-Object Windows.Controls.TextBlock
        $label.Text = $block.Body
        $label.FontSize = if ($block.Kind -eq 'role') { 11 } else { $controls.Reply.FontSize + [Math]::Max(2,8-[int]$block.Language) }
        $label.FontWeight = [Windows.FontWeights]::SemiBold; $label.TextWrapping = 'Wrap'
        $label.SetResourceReference([Windows.Controls.TextBlock]::ForegroundProperty, $(if ($block.Kind -eq 'role' -and $block.Role -ne 'You') { 'Accent' } else { 'Text' }))
        $label.Margin = [Windows.Thickness]::new(0,8,0,0)
        if ($block.Kind -eq 'role') {
            $surface.Margin = [Windows.Thickness]::new(0,24,0,12)
            $surface.Padding = [Windows.Thickness]::new(0)
            $label.Margin = [Windows.Thickness]::new(0)
            $label.FontSize = 12; $label.VerticalAlignment = 'Center'
            $roleHeader = New-Object Windows.Controls.StackPanel
            $roleHeader.Orientation = 'Horizontal'
            $avatar = New-Object Windows.Controls.Border
            $avatar.Width = 28; $avatar.Height = 28; $avatar.CornerRadius = [Windows.CornerRadius]::new(9)
            $avatar.Margin = [Windows.Thickness]::new(0,0,10,0)
            $avatar.SetResourceReference([Windows.Controls.Border]::BackgroundProperty, $(if ($block.Role -eq 'You') { 'UserMessage' } else { 'AccentSoft' }))
            $initial = New-Object Windows.Controls.TextBlock
            $initial.Text = if ($block.Role -eq 'You') { 'Y' } else { 'A' }
            $initial.FontWeight = [Windows.FontWeights]::SemiBold; $initial.FontSize = 12
            $initial.HorizontalAlignment = 'Center'; $initial.VerticalAlignment = 'Center'
            $initial.SetResourceReference([Windows.Controls.TextBlock]::ForegroundProperty, $(if ($block.Role -eq 'You') { 'Muted' } else { 'Accent' }))
            $avatar.Child = $initial
            [void]$roleHeader.Children.Add($avatar); [void]$roleHeader.Children.Add($label)
            $surface.Child = $roleHeader
        } else {
            $surface.Child = $label
        }
    } elseif ($block.Kind -eq 'tools') {
        $stack = New-Object Windows.Controls.StackPanel
        $details = New-ReadableText $block.Body; $details.Visibility = 'Collapsed'
        $toggle = New-Object Windows.Controls.Button
        $toggle.Content = '▸ Tool calls'; $toggle.HorizontalAlignment = 'Left'; $toggle.Tag = $details
        $toggle.SetResourceReference([Windows.Controls.Control]::ForegroundProperty, 'Muted')
        $toggle.Add_Click({ param($sender) $open = $sender.Tag.Visibility -ne 'Visible'; $sender.Tag.Visibility = if ($open) { 'Visible' } else { 'Collapsed' }; $sender.Content = if ($open) { '▾ Tool calls' } else { '▸ Tool calls' } })
        [void]$stack.Children.Add($toggle); [void]$stack.Children.Add($details); $surface.Child = $stack; $surface.Tag.toggle = $toggle
    } else {
        $surface.Child = New-ReadableText $block.Body
        if ($block.Role -eq 'You') { $surface.SetResourceReference([Windows.Controls.Border]::BackgroundProperty, 'UserMessage'); $surface.Padding = [Windows.Thickness]::new(18,14,18,14); $surface.Margin = [Windows.Thickness]::new(38,0,0,12) }
    }
    return $surface
}

function Render-Conversation([bool]$force = $false) {
    $blocks = @(Convert-ResponseBlocks ([string]$controls.Reply.Text))
    $query = [string]$controls.ReplySearch.Text
    $entries = @($blocks | ForEach-Object { [pscustomobject]@{ Key=($_.Kind + "`0" + $_.Role + "`0" + $_.Language + "`0" + $_.Body + "`0" + $controls.Reply.FontSize + "`0" + $query); Block=$_ } })
    $common = 0
    if (-not $force) {
        while ($common -lt $entries.Count -and $common -lt $script:renderedBlocks.Count -and $entries[$common].Key -eq $script:renderedBlocks[$common].Key) { $common++ }
    }
    while ($controls.ConversationCards.Children.Count -gt $common) { $controls.ConversationCards.Children.RemoveAt($controls.ConversationCards.Children.Count-1) }
    for ($index = $common; $index -lt $entries.Count; $index++) { [void]$controls.ConversationCards.Children.Add((New-ConversationBlock $entries[$index].Block)) }
    $script:renderedBlocks = $entries
    $script:searchHits = @()
    if ($query) {
        for ($index = 0; $index -lt $entries.Count; $index++) {
            if ($entries[$index].Block.Body.IndexOf($query,[StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $script:searchHits += $index
                if ($entries[$index].Block.Kind -eq 'tools') { $controls.ConversationCards.Children[$index].Tag.toggle.Tag.Visibility = 'Visible'; $controls.ConversationCards.Children[$index].Tag.toggle.Content = '▾ Tool calls' }
                $controls.ConversationCards.Children[$index].SetResourceReference([Windows.Controls.Border]::BorderBrushProperty,'Accent')
                $controls.ConversationCards.Children[$index].BorderThickness = [Windows.Thickness]::new(1)
            }
        }
    }
    if ($script:renderedQuery -ne $query) { $script:searchHitIndex = -1 } else { $script:searchHitIndex = [Math]::Min($script:searchHitIndex,$script:searchHits.Count-1) }
    $script:renderedQuery = $query
    $controls.ReplySearchCount.Text = if ($query) { "$($script:searchHits.Count) sections" } else { 'Type to find' }
}

function Move-ConversationHit([int]$direction = 1) {
    if (-not $script:searchHits.Count) { return }
    $controls.ConversationCards.UpdateLayout()
    $script:searchHitIndex = if ($script:searchHitIndex -lt 0 -and $direction -lt 0) { $script:searchHits.Count-1 } else { ($script:searchHitIndex + $direction + $script:searchHits.Count) % $script:searchHits.Count }
    $controls.ConversationCards.Children[$script:searchHits[$script:searchHitIndex]].BringIntoView()
    $controls.ReplySearchCount.Text = "$($script:searchHitIndex + 1) / $($script:searchHits.Count) sections"
}

function Open-ConversationSearch {
    if (-not $controls.Reply.Text) { Show-GuiNotice 'The conversation is empty'; return }
    $controls.ReplySearchBar.Visibility = 'Visible'
    $controls.ReplySearch.Focus() | Out-Null; $controls.ReplySearch.SelectAll()
}

function Close-ConversationSearch {
    $controls.ReplySearchBar.Visibility = 'Collapsed'; $controls.ReplySearch.Clear()
    if ($searchTimer) { $searchTimer.Stop() }
    Render-Conversation $true
}

function Set-GuiTheme([string]$theme, [bool]$persist = $true) {
    if ($theme -notin @('dark','light')) { $theme = 'dark' }
    $script:theme = $theme
    $palette = if ($theme -eq 'light') {
        @{WindowBg='#E9EDF3'; RailBg='#E9EDF3'; SidebarBg='#F3F5F8'; Canvas='#FAFBFD'; Surface='#EDF0F5'; SurfaceElevated='#FFFFFF'; SurfaceHover='#E2E7EF'; CodeSurface='#F0F3F8'; UserMessage='#EDF1F7'; Border='#C9D1DE'; Text='#202632'; Muted='#525F73'; Dim='#657287'; Accent='#806116'; AccentSoft='#F5ECD7'; AccentText='#FFFFFF'; Success='#176B50'; SuccessSoft='#E1F2E9'; Warning='#845115'; WarningSoft='#FCEDD5'; Danger='#AA2948'; DangerSoft='#FBE5EB'}
    } else {
        @{WindowBg='#0C0E14'; RailBg='#0C0E14'; SidebarBg='#12151D'; Canvas='#181B24'; Surface='#202530'; SurfaceElevated='#252B37'; SurfaceHover='#303747'; CodeSurface='#10131B'; UserMessage='#232A38'; Border='#3B4455'; Text='#F3F4F7'; Muted='#BEC5D2'; Dim='#98A4B7'; Accent='#F1C86A'; AccentSoft='#393124'; AccentText='#211909'; Success='#7EE2B8'; SuccessSoft='#183B31'; Warning='#F5CC84'; WarningSoft='#443522'; Danger='#FFACB7'; DangerSoft='#44252E'}
    }
    foreach ($key in $palette.Keys) {
        $brush = [Windows.Media.BrushConverter]::new().ConvertFromString($palette[$key])
        $window.Resources[$key] = $brush
        if ($menu) { $menu.Resources[$key] = $brush }
    }
    if ($menu) {
        $menu.Style = $window.Resources[[type][Windows.Controls.ContextMenu]]
        $menu.Resources[[type][Windows.Controls.MenuItem]] = $window.Resources[[type][Windows.Controls.MenuItem]]
    }
    $controls.ThemeToggle.ToolTip = if ($theme -eq 'dark') { 'Switch to light theme' } else { 'Switch to dark theme' }
    $controls.ThemeToggle.SetValue([Windows.Automation.AutomationProperties]::NameProperty, $controls.ThemeToggle.ToolTip)
    $controls.ComposerSurface.SetResourceReference([Windows.Controls.Border]::BorderBrushProperty, 'Border')
    $script:projectsListSignature = $null
    Update-ProjectsList
    Set-Navigation $script:navigation $false
    if ($script:snapshot) { Set-Snapshot $script:snapshot }
    if ($persist) { Save-GuiSettings; Show-GuiNotice ("Theme: " + $theme) }
}

function Update-GuiLayout {
    # Hysteresis avoids repeated hide/show on the breakpoint. Explicit toggles
    # remain available in compact mode, including Ctrl+K search.
    if ($window.ActualWidth -lt 1000 -and -not $script:compactLayout) {
        $script:compactLayout = $true
        if ($script:sidebarVisible) {
            $script:sidebarAutoHidden = $true
            Set-SidebarVisibility $false $false
        }
    } elseif ($window.ActualWidth -ge 1080 -and $script:compactLayout -and -not $script:focusMode) {
        $script:compactLayout = $false
        if ($script:sidebarAutoHidden) {
            $script:sidebarAutoHidden = $false
            Set-SidebarVisibility $true $false
        }
    }
    if ($script:workflowVisible) {
        $compactWork = $controls.ChatSurface.ActualWidth -lt 900
        $controls.ConversationColumn.Width = if ($compactWork) { [Windows.GridLength]::new(0) } else { [Windows.GridLength]::new(1, [Windows.GridUnitType]::Star) }
        $controls.WorkDividerColumn.Width = [Windows.GridLength]::new($(if ($compactWork) { 0 } else { 5 }))
        $controls.WorkflowResize.Visibility = if ($compactWork) { 'Collapsed' } else { 'Visible' }
        $controls.WorkColumn.Width = if ($compactWork) { [Windows.GridLength]::new(1, [Windows.GridUnitType]::Star) } else { [Windows.GridLength]::new($script:workflowWidth) }
    } else {
        $controls.ConversationColumn.Width = [Windows.GridLength]::new(1, [Windows.GridUnitType]::Star)
        $controls.WorkColumn.Width = [Windows.GridLength]::new(0)
        $controls.WorkDividerColumn.Width = [Windows.GridLength]::new(0)
        $controls.WorkflowResize.Visibility = 'Collapsed'
    }
    $controls.CommandResults.MaxHeight = [Math]::Max(80, [Math]::Min(340, $window.ActualHeight - 320))
    $short = $window.ActualHeight -lt 680
    $controls.ExamplesPanel.Visibility = if ($short) { 'Collapsed' } else { 'Visible' }
    $controls.Timeline.Visibility = if ($short) { 'Collapsed' } else { 'Visible' }
    $controls.WelcomeMark.Visibility = if ($short) { 'Collapsed' } else { 'Visible' }
    $controls.Activity.FontSize = if ($short) { 22 } else { 30 }
    # Keep the composer aligned with the reading column at every window size.
    $compactChat = $controls.ChatSurface.ActualWidth -lt 620
    $gutter = if ($compactChat) { 16 } else { 28 }
    $controls.ChatHeader.Margin = [Windows.Thickness]::new($gutter, $(if ($short) { 12 } else { 20 }), $gutter, 12)
    $controls.CommandMenu.Visibility = if ($window.ActualWidth -lt 780) { 'Collapsed' } else { 'Visible' }
    $controls.ComposerSurface.Margin = [Windows.Thickness]::new($gutter,12,$gutter,8)
    $controls.ReplyScroll.Margin = [Windows.Thickness]::new($gutter + 4,20,$gutter + 4,16)
    $controls.ComposerContent.Margin = if ($compactChat) { [Windows.Thickness]::new(12,12,12,12) } else { [Windows.Thickness]::new(16,14,16,14) }
    $available = [Math]::Max(260, $controls.ChatSurface.ActualWidth - 80)
    $controls.ModelChooser.Width = [Math]::Max(132, [Math]::Min(210, $available - 270))
    $controls.ModelPopupSurface.Width = [Math]::Max(280, [Math]::Min(420, $window.ActualWidth - 48))
    $controls.Prompt.MinHeight = 40
    $controls.Prompt.MaxHeight = if ($short) { 96 } else { 160 }
    $controls.WorkspacePath.MaxWidth = 120
    $controls.WorkspacePath.Visibility = if ($compactChat) { 'Collapsed' } else { 'Visible' }
    $controls.Copy.Visibility = if ($available -lt 470) { 'Collapsed' } else { 'Visible' }
}


function Set-WorkflowVisibility([bool]$visible, [bool]$persist = $true) {
    $script:workflowVisible = $visible
    $controls.WorkflowSurface.Visibility = if ($visible) { 'Visible' } else { 'Collapsed' }
    $controls.WorkToggle.Content = if ($visible) { 'Hide work' } else { 'Show work' }
    $script:lastPoll = [DateTime]::MinValue
    Update-GuiLayout
    if ($persist) { Save-GuiSettings }
}

function Update-Workflow {
    $selected = Get-SelectedSessionId
    $workflow = if ($script:snapshot -and [string]$script:snapshot.selected_session_id -eq $selected) { $script:snapshot.workflow } else { $null }
    if (-not $workflow -or -not $workflow.available) {
        $controls.WorkflowSummary.Text = if ($workflow.detail) { [string]$workflow.detail } else { 'Updating work for this chat…' }
        $controls.WorkflowTodos.Text = 'No todo snapshot available.'
        $controls.WorkflowPlan.Text = 'No plan snapshot available.'
        $controls.WorkflowSwarm.Text = 'No swarm snapshot available.'
        return
    }
    $controls.WorkflowSummary.Text = "$($workflow.completed) / $($workflow.todo_count) todos completed"
    $todos = @($workflow.todos | Where-Object { $null -ne $_ })
    if ($todos.Count -lt [int]$workflow.todo_count) { $controls.WorkflowSummary.Text += " · showing $($todos.Count)" }
    $controls.WorkflowTodos.Text = if ($todos.Count) { (@($todos | ForEach-Object { "[$($_.status)] $($_.content)" }) -join "`n`n") } else { 'No todos yet.' }
    $plan = [string]$workflow.intention
    if ($workflow.swarm_plan) {
        $graph = $workflow.swarm_plan
        $plan += "`n`nSwarm plan v$($graph.version) · $($graph.completed)/$($graph.item_count) completed · $($graph.failed) failed · $($graph.blocked) blocked"
        if ([int]$graph.cycles -gt 0) { $plan += "`nDependency cycle: $($graph.cycles) items" }
        $plan += "`n`n" + (@($graph.items | ForEach-Object { "[$($_.status)] $($_.content)" }) -join "`n`n")
    }
    $controls.WorkflowPlan.Text = if ([string]::IsNullOrWhiteSpace($plan)) { 'No plan yet.' } else { $plan.Trim() }
    $members = @($workflow.swarm | Where-Object { $null -ne $_ })
    $controls.WorkflowSwarm.Text = if ($members.Count) { (@($members | ForEach-Object { "$($_.name) · $($_.status)`n$($_.role) $($_.detail)" }) -join "`n`n") } else { 'No swarm members.' }
}

function Get-ModelDisplayName([string]$id) {
    if ($id.Contains('/')) { return $id.Substring($id.LastIndexOf('/') + 1) }
    return $id
}

function Update-ModelChooser {
    $controls.ModelChooser.IsEnabled = $controls.ModelPicker.IsEnabled -and $script:availableModels.Count -gt 0
    $controls.ModelChoiceLabel.Text = if ($script:modelRequestPending) { 'Switching model…' } elseif ($script:selectedModel) { Get-ModelDisplayName $script:selectedModel } elseif ($script:availableModels.Count) { 'Choose model' } else { 'Connect a provider' }
    $controls.ModelChooser.ToolTip = if ($script:selectedModel) { "$script:selectedModel · Ctrl+M to change" } else { 'Settings → Providers to connect an account' }
    if (-not $controls.ModelPopup.IsOpen) { return }
    if (-not $controls.ModelChooser.IsEnabled) { $controls.ModelPopup.IsOpen = $false; return }
    $query = $controls.ModelSearch.Text.Trim()
    $rows = @($script:modelRows | Where-Object { -not $query -or ($_.id + ' ' + $_.provider).IndexOf($query,[StringComparison]::OrdinalIgnoreCase) -ge 0 })
    $signature = $query + "`n" + (@($rows | ForEach-Object { $_.id + '|' + $_.provider }) -join "`n")
    if ($signature -ne $script:modelChooserSignature) {
        $chosen = [string]$controls.ModelResults.SelectedValue
        $script:modelChooserSignature = $signature
        $controls.ModelResults.ItemsSource = $rows
        if (@($rows.id) -ccontains $chosen) { $controls.ModelResults.SelectedValue = $chosen }
        elseif (@($rows.id) -ccontains $script:selectedModel) { $controls.ModelResults.SelectedValue = $script:selectedModel }
        elseif ($rows.Count) { $controls.ModelResults.SelectedIndex = 0 }
    }
    $controls.ModelResultCount.Text = if ($rows.Count -eq 1) { '1 connected model' } else { "$($rows.Count) connected models" }
    $controls.ModelEmpty.Visibility = if ($rows.Count) { 'Collapsed' } else { 'Visible' }
    $controls.ApplyModelChoice.IsEnabled = $null -ne $controls.ModelResults.SelectedValue -and (Can-QueueAction)
}

function Open-ModelChooser {
    Update-ModelChooser
    if (-not $controls.ModelChooser.IsEnabled) { Show-GuiNotice 'No selectable models. Sign in to a provider or configure a local endpoint, then refresh.'; return }
    $controls.ModelSearch.Clear()
    $script:modelChooserSignature = $null
    $controls.ModelPopup.IsOpen = $true
    Update-ModelChooser
    $controls.ModelSearch.Focus() | Out-Null
}

function Apply-ModelChoice {
    $model = [string]$controls.ModelResults.SelectedValue
    if ($script:availableModels -notcontains $model -or -not $controls.ApplyModelChoice.IsEnabled) { return }
    $controls.ModelPopup.IsOpen = $false
    if ($model -ne $script:selectedModel) { Queue-ModelChange $model }
    Update-ModelChooser
    $controls.Prompt.Focus() | Out-Null
}

function Update-ResponseElapsed {
    if ($script:closed -or -not $controls.ResponseElapsed -or -not $script:responseSession -or -not $script:responseStarted.ContainsKey($script:responseSession)) { return }
    $seconds = [Math]::Max(0,[int]([DateTime]::UtcNow - $script:responseStarted[$script:responseSession]).TotalSeconds)
    $controls.ResponseElapsed.Text = if ($seconds -ge 60) { '{0}m {1:00}s' -f [Math]::Floor($seconds/60),($seconds%60) } else { "${seconds}s" }
}

function Update-ResponseActivity {
    $session = Get-SelectedSessionRow
    $id = Get-SelectedSessionId
    $sending = $id -and $id -eq $script:pendingResponseSession
    $busy = $session -and ($session.state -eq 'working' -or ($id -eq '__companion__' -and $script:ownedBusy))
    $active = -not $script:responseDisconnected -and $session -and $session.state -notin @('stored','offline','attention') -and -not $script:inputMode -and ($sending -or $busy)
    if (-not $active) {
        if ($id -and -not $busy -and -not $sending) { $script:responseStarted.Remove($id) }
        $script:responseSession = ''
        if ($controls.ResponseActivity.Visibility -ne 'Collapsed') {
            $controls.ResponseActivity.Visibility = 'Collapsed'
            foreach ($name in @('LoadingDot1','LoadingDot2','LoadingDot3')) { $controls[$name].BeginAnimation([Windows.UIElement]::OpacityProperty,$null) }
        }
        if ($responseTimer) { $responseTimer.Stop() }
        return
    }
    if (-not $script:responseStarted.ContainsKey($id)) { $script:responseStarted[$id] = [DateTime]::UtcNow }
    $script:responseSession = $id
    $controls.ResponseStage.Text = if ($sending -and -not $busy) { 'Sending your prompt' } else { 'AlphaCode is working' }
    $detail = [string]$session.detail
    $controls.ResponseDetail.Text = if ($sending -and -not $busy) { 'Preparing the response…' } elseif ($detail -and $detail -notin @('Ready','Working','working')) { $detail } else { 'Waiting for the next response · you can stop at any time' }
    $controls.ResponseStop.IsEnabled = $controls.Cancel.IsEnabled
    if ($controls.ResponseActivity.Visibility -ne 'Visible') {
        $controls.ResponseActivity.Visibility = 'Visible'
        $index = 0
        foreach ($name in @('LoadingDot1','LoadingDot2','LoadingDot3')) {
            if ([Windows.SystemParameters]::ClientAreaAnimation) {
                $pulse = New-Object Windows.Media.Animation.DoubleAnimation
                $pulse.From = 0.25; $pulse.To = 1
                $pulse.Duration = [Windows.Duration]::new([TimeSpan]::FromMilliseconds(450))
                $pulse.BeginTime = [TimeSpan]::FromMilliseconds($index * 150)
                $pulse.AutoReverse = $true
                $pulse.RepeatBehavior = [Windows.Media.Animation.RepeatBehavior]::Forever
                $controls[$name].BeginAnimation([Windows.UIElement]::OpacityProperty,$pulse)
            }
            $index++
        }
    }
    Update-ResponseElapsed
    if ($responseTimer) { $responseTimer.Start() }
}

function Update-ComposerState {
    $count = $controls.Prompt.Text.Length
    $controls.PromptCount.Text = '{0:N0} / 16,000' -f $count
    $controls.PromptCount.Visibility = if ($count -gt 0) { 'Visible' } else { 'Collapsed' }
    $controls.PromptCount.Foreground = Get-GuiBrush $(if ($count -ge 15000) { 'Warning' } else { 'Dim' })
    $session = Get-SelectedSessionRow
    $selected = Get-SelectedSessionId
    $hasContent = $script:inputMode -or -not [string]::IsNullOrWhiteSpace($controls.Prompt.Text)
    $controls.Send.IsEnabled = $null -ne $session -and $hasContent -and [string]$session.state -notin @('stored','offline') -and -not ($script:ownedBusy -and $selected -eq '__companion__' -and -not $script:inputMode) -and $script:pendingKind -notin @('prompt','input') -and $script:queue.Count -eq 0
}

function Save-GuiSettings {
    if ($Preview -or $script:initializing) { return }
    try {
        [IO.Directory]::CreateDirectory($settingsDir) | Out-Null
        $settings = @{ theme=$script:theme; work_width=$script:workflowWidth; work_visible=$script:workflowVisible; width=$window.RestoreBounds.Width; height=$window.RestoreBounds.Height; left=$window.Left; top=$window.Top; topmost_opt_in=$window.Topmost; model=[string]$script:selectedModel; effort=[string]$script:selectedEffort; sidebar_visible=($script:sidebarVisible -or $script:sidebarAutoHidden); sidebar_width=$script:sidebarWidth; reading_size=$controls.Reply.FontSize }
        if ($script:focusMode) {
            $settings.work_visible = $script:focusWork
            $settings.sidebar_visible = $script:focusSidebar
        }
        $temporary = $settingsPath + '.' + $PID + '.tmp'
        [IO.File]::WriteAllText($temporary, ($settings | ConvertTo-Json -Compress))
        Move-Item -LiteralPath $temporary -Destination $settingsPath -Force
    } catch { }
}

function Set-TextDirection($textControl, [string]$text) {
    # Base direction follows the first strong letter, so adding Arabic to an
    # English sentence does not flip the entire editor while the user types.
    $first = [regex]::Match($text, '[A-Za-z\u0600-\u06FF\u0750-\u077F\u08A0-\u08FF]')
    $rtl = $first.Success -and $first.Value -notmatch '[A-Za-z]'
    $direction = if ($rtl) { [Windows.FlowDirection]::RightToLeft } else { [Windows.FlowDirection]::LeftToRight }
    if ($textControl.FlowDirection -ne $direction) { $textControl.FlowDirection = $direction }
    $alignment = if ($rtl) { [Windows.TextAlignment]::Right } else { [Windows.TextAlignment]::Left }
    if ($textControl.TextAlignment -ne $alignment) { $textControl.TextAlignment = $alignment }
}

function Start-Bridge {
    if ($Preview) { return }
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Executable
    $start.Arguments = '--quiet --no-selfdev gui --bridge'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = New-Object Text.UTF8Encoding($false)
    $script:process = [Diagnostics.Process]::Start($start)
    # .NET Framework's ProcessStartInfo has no StandardInputEncoding setter.
    # Wrap the pipe explicitly so Arabic, CJK, and emoji reach Rust as UTF-8.
    $script:inputWriter = [IO.StreamWriter]::new($script:process.StandardInput.BaseStream, [Text.UTF8Encoding]::new($false))
    $script:inputWriter.AutoFlush = $true
    $script:stderrDrain = $script:process.StandardError.ReadToEndAsync()
}

function Show-GuiNotice([string]$text, [int]$seconds = 6) {
    $script:actionNotice = $text
    $script:actionNoticeUntil = [DateTime]::UtcNow.AddSeconds($seconds)
    $controls.FooterStatus.Text = $text
}

function Clear-GuiNotice {
    $script:actionNotice = ''
    $script:actionNoticeUntil = [DateTime]::MinValue
}

function Update-ProjectsList {
    $projects = @($script:allSessions | Where-Object { $_.id -ne '__companion__' -and -not [string]::IsNullOrWhiteSpace([string]$_.working_dir) })
    $query = [string]$controls.SessionSearch.Text
    if (-not [string]::IsNullOrWhiteSpace($query)) {
        $pattern = [regex]::Escape($query.Trim())
        $projects = @($projects | Where-Object { ([string]$_.title -match $pattern) -or ([string]$_.detail -match $pattern) -or ([string]$_.working_dir -match $pattern) })
    }
    # Keep this quick-access section bounded and group chats by workspace.
    # The complete session list below remains scrollable and searchable.
    $seenWorkspaces = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $projects = @($projects | Where-Object {
        $key = if ([string]::IsNullOrWhiteSpace([string]$_.working_dir)) { 'session:' + [string]$_.id } else { [string]$_.working_dir }
        $seenWorkspaces.Add($key)
    } | Select-Object -First 3)
    $signature = (@($projects | ForEach-Object { "$($_.id)|$($_.title)|$($_.working_dir)|$($_.detail)" }) -join "`n") + '#' + $query
    if ($signature -eq $script:projectsListSignature) { return }
    $script:projectsListSignature = $signature
    $controls.ProjectsList.Children.Clear()
    if ($projects.Count -eq 0) {
        $empty = New-Object Windows.Controls.TextBlock
        $empty.Text = if ([string]::IsNullOrWhiteSpace($query)) { 'No active projects' } else { 'No matching projects' }
        $empty.Foreground = (Get-GuiBrush 'Muted')
        $empty.FontSize = 11
        $empty.Margin = [Windows.Thickness]::new(7,2,0,5)
        $controls.ProjectsList.Children.Add($empty) | Out-Null
        return
    }
    foreach ($session in $projects) {
        $button = New-Object Windows.Controls.Button
        $button.Content = if ($session.working_dir) { Split-Path -Leaf ([string]$session.working_dir) } else { [string]$session.title }
        $button.Tag = $session.id
        $button.HorizontalContentAlignment = 'Left'
        $button.Foreground = (Get-GuiBrush 'Text')
        $button.Padding = [Windows.Thickness]::new(7,7,7,7)
        $button.ToolTip = if ($session.working_dir) { [string]$session.working_dir } else { [string]$session.detail }
        $button.Add_Click({ param($sender) $controls.SessionSearch.Clear(); Set-Navigation 'Home'; $controls.Sessions.SelectedValue = [string]$sender.Tag; Focus-SelectedChat })
        $controls.ProjectsList.Children.Add($button) | Out-Null
    }
}


function Initialize-ProviderSettings($dialog) {
    $nav = New-Object Windows.Controls.Button
    $nav.Tag = 'Providers'
    $nav.HorizontalContentAlignment = 'Left'; $nav.Padding = [Windows.Thickness]::new(12)
    $nav.Margin = [Windows.Thickness]::new(0,0,0,6)
    $content = New-Object Windows.Controls.StackPanel
    $title = New-Object Windows.Controls.TextBlock; $title.Text = 'Providers'; $title.FontWeight = [Windows.FontWeights]::SemiBold
    $description = New-Object Windows.Controls.TextBlock; $description.Text = 'Accounts and connections'; $description.FontSize = 11
    $description.SetResourceReference([Windows.Controls.TextBlock]::ForegroundProperty,'Muted'); $description.Margin = [Windows.Thickness]::new(0,5,0,0)
    [void]$content.Children.Add($title); [void]$content.Children.Add($description); $nav.Content = $content
    $nav.SetValue([Windows.Automation.AutomationProperties]::NameProperty,'Provider settings')
    $script:settingsControls.ModelsNav.Parent.Children.Insert(2,$nav)
    $script:settingsControls.ProvidersNav = $nav
    [xml]$pageXaml = @'
<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="ProvidersPage" Visibility="Collapsed">
 <TextBlock Text="Provider connections" FontSize="21" FontWeight="SemiBold" Margin="0,0,0,6"/>
 <TextBlock Text="Use the same accounts and credential storage as the CLI." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Margin="0,0,0,20"/>
 <TextBlock Text="Provider" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,0,0,6"/>
 <ComboBox x:Name="ProviderChoice" SelectedValuePath="id" Height="36" IsTextSearchEnabled="True" Margin="0,0,0,8" AutomationProperties.Name="Provider to connect"><ComboBox.ItemTemplate><DataTemplate><TextBlock Text="{Binding name}" TextTrimming="CharacterEllipsis"/></DataTemplate></ComboBox.ItemTemplate></ComboBox>
 <TextBlock x:Name="ProviderState" Text="Loading connections…" Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Margin="0,0,0,16"/>
 <StackPanel x:Name="ProviderEndpointGroup" Visibility="Collapsed"><TextBlock Text="API endpoint (optional)" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,0,0,6"/><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="12,10" Margin="0,0,0,12"><TextBox x:Name="ProviderEndpoint" Background="Transparent" Foreground="{DynamicResource Text}" BorderThickness="0" Padding="0" AutomationProperties.Name="Provider API endpoint"/></Border></StackPanel>
 <StackPanel x:Name="ProviderSetupGroup" Visibility="Collapsed"><TextBlock x:Name="ProviderSetupLabel" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,0,0,6"/><TextBox x:Name="ProviderSetup" Background="{DynamicResource Surface}" Foreground="{DynamicResource Text}" BorderBrush="{DynamicResource Border}" Padding="12,10" Margin="0,0,0,12" AutomationProperties.Name="Provider configuration"/></StackPanel>
 <Button x:Name="ProviderBrowser" Style="{DynamicResource PrimaryButton}" Content="Sign in with browser" HorizontalAlignment="Left" Padding="16,9" Margin="0,0,0,12"/>
 <StackPanel x:Name="ProviderKeyGroup"><TextBlock Text="API key" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,0,0,6"/><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="12,10"><PasswordBox x:Name="ProviderKey" Background="Transparent" Foreground="{DynamicResource Text}" CaretBrush="{DynamicResource Accent}" BorderThickness="0" MaxLength="16000" AutomationProperties.Name="Provider API key"/></Border><Button x:Name="ProviderConnectKey" Content="Connect API key" HorizontalAlignment="Left" Padding="16,9" Margin="0,8,0,14"/></StackPanel>
 <Button x:Name="ProviderLocal" Content="Connect endpoint" HorizontalAlignment="Left" Padding="16,9" Margin="0,0,0,12" Visibility="Collapsed"/>
 <Border x:Name="ProviderProgress" Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="12" Visibility="Collapsed" Margin="0,4,0,12"><StackPanel><TextBlock x:Name="ProviderProgressTitle" Foreground="{DynamicResource Accent}" FontWeight="SemiBold"/><TextBox x:Name="ProviderProgressText" IsReadOnly="True" AcceptsReturn="True" TextWrapping="Wrap" MaxHeight="150" VerticalScrollBarVisibility="Auto" Background="Transparent" Foreground="{DynamicResource Muted}" BorderThickness="0" Padding="0,8,0,0"/>
 <StackPanel x:Name="ProviderInputGroup" Margin="0,12,0,0"><TextBlock Text="Verification code or additional setup input" Foreground="{DynamicResource Muted}" FontSize="11" TextWrapping="Wrap" Margin="0,0,0,6"/><PasswordBox x:Name="ProviderInput" Background="{DynamicResource SurfaceElevated}" Foreground="{DynamicResource Text}" BorderBrush="{DynamicResource Border}" MaxLength="16000" Padding="8" AutomationProperties.Name="Provider verification code"/><WrapPanel Margin="0,8,0,0"><Button x:Name="ProviderContinue" Content="Continue" Margin="0,0,8,0"/><Button x:Name="ProviderCancel" Content="Cancel sign-in" Foreground="{DynamicResource Danger}"/></WrapPanel></StackPanel></StackPanel></Border>
 <Button x:Name="ProviderRefresh" Content="Refresh connections" HorizontalAlignment="Left" Foreground="{DynamicResource Muted}"/>
</StackPanel>
'@
    $page = [Windows.Markup.XamlReader]::Load([Xml.XmlNodeReader]::new($pageXaml))
    [void]$script:settingsControls.AppearancePage.Parent.Children.Add($page)
    $script:settingsControls.ProvidersPage = $page
    foreach ($name in @('ProviderChoice','ProviderState','ProviderEndpointGroup','ProviderEndpoint','ProviderSetupGroup','ProviderSetupLabel','ProviderSetup','ProviderBrowser','ProviderKeyGroup','ProviderKey','ProviderConnectKey','ProviderLocal','ProviderProgress','ProviderProgressTitle','ProviderProgressText','ProviderInputGroup','ProviderInput','ProviderContinue','ProviderCancel','ProviderRefresh')) { $script:settingsControls[$name] = $page.FindName($name) }
    $script:settingsControls.ProviderChoice.Add_SelectionChanged({
        $script:settingsControls.ProviderKey.Clear(); $script:settingsControls.ProviderEndpoint.Clear(); $script:settingsControls.ProviderSetup.Clear(); Update-ProviderSettings
    })
    $script:settingsControls.ProviderBrowser.Add_Click({ Start-ProviderConnection 'oauth' })
    $script:settingsControls.ProviderConnectKey.Add_Click({ Start-ProviderConnection 'api_key' })
    $script:settingsControls.ProviderLocal.Add_Click({ Start-ProviderConnection 'local' })
    $script:settingsControls.ProviderRefresh.Add_Click({ $script:providersRefreshRequested = $true })
    $script:settingsControls.ProviderContinue.Add_Click({
        $code = $script:settingsControls.ProviderInput.Password
        Queue-Action 'provider_input' '' @{prompt=$code}
        $script:settingsControls.ProviderInput.Clear()
    })
    $script:settingsControls.ProviderCancel.Add_Click({ Queue-Action 'provider_cancel' '' })
    if ($script:providerRows.Count) { Set-ProviderCatalog $script:providerRows }
}

function Set-ProviderCatalog($rows) {
    $script:providerRows = @($rows)
    if (-not $script:settingsWindow -or -not $script:settingsControls.ProviderChoice) { return }
    $chosen = [string]$script:settingsControls.ProviderChoice.SelectedValue
    $script:settingsControls.ProviderChoice.ItemsSource = $script:providerRows
    if ($chosen -and @($script:providerRows.id) -contains $chosen) { $script:settingsControls.ProviderChoice.SelectedValue = $chosen }
    else {
        $first = $script:providerRows | Where-Object { $_.configured } | Select-Object -First 1
        if (-not $first) { $first = $script:providerRows | Select-Object -First 1 }
        if ($first) { $script:settingsControls.ProviderChoice.SelectedValue = $first.id }
    }
    Update-ProviderSettings
}

function Update-ProviderSettings {
    if (-not $script:settingsWindow -or -not $script:settingsControls.ProviderChoice) { return }
    $form = $script:settingsControls
    $row = $form.ProviderChoice.SelectedItem
    $busy = $script:providerConnection -and $script:providerConnection.phase -eq 'connecting'
    $form.ProviderState.Text = if ($row) { $(if ($row.configured) { 'Configured' } else { 'Not connected' }) + ' · ' + [string]$row.detail } else { 'Select a provider to connect' }
    $form.ProviderState.Foreground = Get-GuiBrush $(if ($row.configured) { 'Success' } else { 'Muted' })
    $form.ProviderBrowser.Visibility = if ($row.oauth) { 'Visible' } else { 'Collapsed' }
    $form.ProviderKeyGroup.Visibility = if ($row.api_key) { 'Visible' } else { 'Collapsed' }
    $form.ProviderEndpointGroup.Visibility = if ($row.endpoint) { 'Visible' } else { 'Collapsed' }
    $form.ProviderSetupGroup.Visibility = if ($row.setup_label) { 'Visible' } else { 'Collapsed' }
    $form.ProviderSetupLabel.Text = [string]$row.setup_label
    $form.ProviderLocal.Visibility = if ($row.local) { 'Visible' } else { 'Collapsed' }
    foreach ($control in @($form.ProviderChoice,$form.ProviderBrowser,$form.ProviderConnectKey,$form.ProviderLocal)) { $control.IsEnabled = -not $busy -and $null -ne $row }
    $form.ProviderProgress.Visibility = if ($script:providerConnection -and $script:providerConnection.phase) { 'Visible' } else { 'Collapsed' }
    $form.ProviderProgressTitle.Text = if ($busy) { 'Sign-in in progress' } elseif ($script:providerConnection.phase -eq 'completed') { 'Sign-in finished' } elseif ($script:providerConnection.phase -eq 'failed') { 'Connection needs attention' } else { 'Connection status' }
    $form.ProviderProgressText.Text = [string]$script:providerConnection.detail
    $form.ProviderInputGroup.Visibility = if ($busy) { 'Visible' } else { 'Collapsed' }
}

function Start-ProviderConnection([string]$method) {
    if (-not (Can-QueueAction)) { return }
    $form = $script:settingsControls
    $row = $form.ProviderChoice.SelectedItem
    if (-not $row) { return }
    $key = if ($method -eq 'api_key') { $form.ProviderKey.Password } else { '' }
    if ($method -eq 'api_key' -and [string]::IsNullOrWhiteSpace($key)) { $form.ProviderState.Text = 'Enter an API key first.'; return }
    Queue-Action 'provider_connect' '' @{provider=[string]$row.id; auth_method=$method; api_key=$key; api_base=$form.ProviderEndpoint.Text; setup_value=$form.ProviderSetup.Text}
    $form.ProviderKey.Clear()
    $script:providerConnection = [pscustomobject]@{provider=$row.id; phase='connecting'; detail='Starting sign-in…'}
    Update-ProviderSettings
}

function Set-ProviderProgress($connection) {
    $script:providerConnection = $connection
    if ($connection.phase -ne $script:lastProviderPhase) {
        $script:lastProviderPhase = [string]$connection.phase
        if ($connection.phase -in @('completed','failed','cancelled')) { $script:providersRefreshRequested = $true; $script:lastPoll = [DateTime]::MinValue }
    }
    Update-ProviderSettings
}

function Select-SettingsSection([string]$section) {
    if ($section -eq 'Providers' -and $script:settingsSection -ne 'Providers') { $script:providersRefreshRequested = $true }
    $script:settingsSection = $section
    foreach ($name in @('Appearance','Models','Providers','Workspace','Keyboard')) {
        $script:settingsControls[$name + 'Page'].Visibility = if ($name -eq $section) { 'Visible' } else { 'Collapsed' }
        $button = $script:settingsControls[$name + 'Nav']
        $button.Background = if ($name -eq $section) { Get-GuiBrush 'AccentSoft' } else { [Windows.Media.Brushes]::Transparent }
        $button.Foreground = Get-GuiBrush $(if ($name -eq $section) { 'Accent' } else { 'Muted' })
    }
}

function Update-SettingsModels {
    if (-not $script:settingsWindow) { return }
    $form = $script:settingsControls
    $query = ([string]$form.ModelSearch.Text).Trim()
    $models = @($script:availableModels | ForEach-Object { [string]$_ } | Where-Object { $_ -and ([string]::IsNullOrWhiteSpace($query) -or $_.IndexOf($query,[StringComparison]::OrdinalIgnoreCase) -ge 0) })
    $signature = $query + "`n" + ($models -join "`n")
    if ($signature -ne $script:settingsModelsSignature) {
        $chosen = [string]$form.ModelList.SelectedItem
        $script:settingsModelsSignature = $signature
        $form.ModelList.ItemsSource = $models
        if ($models -contains $chosen) { $form.ModelList.SelectedItem = $chosen }
        elseif ($models -contains $script:selectedModel) { $form.ModelList.SelectedItem = $script:selectedModel }
    }
    $form.ModelMatches.Text = if (-not $script:availableModels.Count) { 'No models available. Sign in to a provider and refresh.' } elseif ($models.Count -eq 1) { '1 model' } else { "$($models.Count) models" }
    $form.UseModel.IsEnabled = $controls.ModelPicker.IsEnabled -and $null -ne $form.ModelList.SelectedItem -and $script:queue.Count -eq 0
}

function Update-SettingsForm {
    if (-not $script:settingsWindow) { return }
    $script:settingsUpdating = $true
    try {
        $form = $script:settingsControls
        $form.TextSize.Value = $controls.Reply.FontSize
        $form.TextSizeLabel.Text = "$($controls.Reply.FontSize) pt"
        $form.TypePreview.FontSize = $controls.Reply.FontSize
        $form.ShowSidebar.IsChecked = $script:sidebarVisible
        $form.ShowWork.IsChecked = $script:workflowVisible
        $form.Topmost.IsChecked = $window.Topmost
        foreach ($entry in @(@($form.DarkTheme,'dark'),@($form.LightTheme,'light'))) {
            $entry[0].BorderBrush = Get-GuiBrush $(if ($script:theme -eq $entry[1]) { 'Accent' } else { 'Border' })
            $entry[0].BorderThickness = [Windows.Thickness]::new($(if ($script:theme -eq $entry[1]) { 2 } else { 1 }))
        }
        $row = Get-SelectedSessionRow
        $form.WorkspacePath.Text = if ($row.working_dir) { [string]$row.working_dir } else { 'No workspace folder selected' }
        $form.OpenFolder.IsEnabled = $row -and $row.working_dir -and [IO.Directory]::Exists([string]$row.working_dir)
        $form.ChooseFolder.IsEnabled = $controls.AddContext.IsEnabled
        $form.ActiveModel.Text = if ($script:modelRequestPending) { 'Switching model…' } elseif ($row.state -eq 'stored') { 'Saved history · model changes are unavailable' } elseif ($script:selectedModel) { "Active: $script:selectedModel" } elseif ($script:availableModels.Count) { 'Current model unavailable · choose a connected model below' } else { 'Sign in to a provider or configure a local endpoint, then refresh' }
        Update-SettingsModels
        $efforts = @($controls.EffortPicker.Items | ForEach-Object { [string]$_.Tag })
        $signature = $efforts -join '|'
        if ($signature -ne $script:settingsEffortsSignature) {
            $script:settingsEffortsSignature = $signature
            $form.Efforts.Children.Clear()
            foreach ($effort in $efforts) {
                $button = New-Object Windows.Controls.Button
                $button.Tag = $effort; $button.Content = if ($effort -eq 'none') { 'Auto' } else { (Get-Culture).TextInfo.ToTitleCase($effort) }
                $button.Margin = [Windows.Thickness]::new(0,0,5,5)
                $button.Add_Click({ param($sender) Queue-EffortChange ([string]$sender.Tag); Update-SettingsForm })
                [void]$form.Efforts.Children.Add($button)
            }
        }
        foreach ($button in $form.Efforts.Children) {
            $button.IsEnabled = $controls.EffortPicker.IsEnabled -and $script:queue.Count -eq 0
            $button.Background = Get-GuiBrush $(if ($button.Tag -eq $script:selectedEffort) { 'AccentSoft' } else { 'Surface' })
            $button.Foreground = Get-GuiBrush $(if ($button.Tag -eq $script:selectedEffort) { 'Accent' } else { 'Text' })
        }
        $form.Feedback.Text = if ($Preview) { 'Preview mode · preferences are not saved' } elseif ($script:modelRequestPending -or $script:effortRequestPending) { 'Applying model settings…' } elseif ($script:deliveryNotice -and [DateTime]::UtcNow -lt $script:noticeUntil -and $script:deliveryNotice -ne 'Prompt delivered') { $script:deliveryNotice } elseif ($script:actionNotice -and [DateTime]::UtcNow -lt $script:actionNoticeUntil) { $script:actionNotice } else { 'Preferences save automatically. Model changes use the open live chat.' }
        Select-SettingsSection $script:settingsSection
    } finally { $script:settingsUpdating = $false }
}

function New-SettingsWindow {
    [xml]$definition = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="AlphaCode Settings" Width="900" Height="740" MinWidth="640" MinHeight="500" WindowStyle="None" ResizeMode="CanResize" WindowStartupLocation="CenterOwner" ShowInTaskbar="False" FontFamily="Segoe UI Variable Text, Segoe UI" FontSize="13" Foreground="{DynamicResource Text}" Background="{DynamicResource Canvas}" UseLayoutRounding="True">
 <WindowChrome.WindowChrome><WindowChrome CaptionHeight="0" ResizeBorderThickness="6" GlassFrameThickness="0" CornerRadius="0" UseAeroCaptionButtons="False"/></WindowChrome.WindowChrome>
 <Window.Resources><Style x:Key="SettingsSwitch" TargetType="ToggleButton"><Setter Property="Width" Value="44"/><Setter Property="Height" Value="26"/><Setter Property="Cursor" Value="Hand"/><Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ToggleButton"><Border x:Name="Track" Background="{DynamicResource SurfaceHover}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="13"><Ellipse x:Name="Knob" Width="18" Height="18" Margin="3" HorizontalAlignment="Left" Fill="{DynamicResource Muted}"/></Border><ControlTemplate.Triggers><Trigger Property="IsChecked" Value="True"><Setter TargetName="Track" Property="Background" Value="{DynamicResource Accent}"/><Setter TargetName="Knob" Property="HorizontalAlignment" Value="Right"/><Setter TargetName="Knob" Property="Fill" Value="{DynamicResource AccentText}"/></Trigger><Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="Track" Property="BorderBrush" Value="{DynamicResource Text}"/><Setter TargetName="Track" Property="BorderThickness" Value="2"/></Trigger></ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter></Style></Window.Resources>
 <Border BorderBrush="{DynamicResource Border}" BorderThickness="1"><Grid><Grid.RowDefinitions><RowDefinition Height="88"/><RowDefinition/><RowDefinition Height="64"/></Grid.RowDefinitions>
  <Grid x:Name="TitleBar" Margin="28,18,20,14"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="Settings" FontSize="24" FontWeight="SemiBold"/><TextBlock Text="A workspace that works your way." Foreground="{DynamicResource Muted}" FontSize="12" Margin="0,5,0,0"/></StackPanel><Button x:Name="Close" Grid.Column="1" Content="×" FontSize="22" Padding="12,0" VerticalAlignment="Top" ToolTip="Close settings" AutomationProperties.Name="Close settings"/></Grid>
  <Grid Grid.Row="1"><Grid.ColumnDefinitions><ColumnDefinition Width="200"/><ColumnDefinition/></Grid.ColumnDefinitions>
   <Border Background="{DynamicResource SidebarBg}" Padding="12,16" BorderBrush="{DynamicResource Border}" BorderThickness="0,1,1,0"><StackPanel><Button x:Name="AppearanceNav" Tag="Appearance" HorizontalContentAlignment="Left" Margin="0,0,0,6" Padding="12"><StackPanel><TextBlock Text="Appearance" FontWeight="SemiBold"/><TextBlock Text="Theme and typography" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,5,0,0"/></StackPanel></Button><Button x:Name="ModelsNav" Tag="Models" HorizontalContentAlignment="Left" Margin="0,0,0,6" Padding="12"><StackPanel><TextBlock Text="Models" FontWeight="SemiBold"/><TextBlock Text="Model and reasoning" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,5,0,0"/></StackPanel></Button><Button x:Name="WorkspaceNav" Tag="Workspace" HorizontalContentAlignment="Left" Margin="0,0,0,6" Padding="12"><StackPanel><TextBlock Text="Workspace" FontWeight="SemiBold"/><TextBlock Text="Layout and folders" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,5,0,0"/></StackPanel></Button><Button x:Name="KeyboardNav" Tag="Keyboard" HorizontalContentAlignment="Left" Padding="12"><StackPanel><TextBlock Text="Keyboard" FontWeight="SemiBold"/><TextBlock Text="Shortcuts and actions" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,5,0,0"/></StackPanel></Button></StackPanel></Border>
   <ScrollViewer Grid.Column="1" VerticalScrollBarVisibility="Auto" HorizontalScrollBarVisibility="Disabled" Padding="28,24" BorderBrush="{DynamicResource Border}" BorderThickness="0,1,0,0"><Grid>
    <StackPanel x:Name="AppearancePage"><TextBlock Text="Appearance" FontSize="22" FontWeight="SemiBold" Margin="0,0,0,8"/><TextBlock Text="Set the tone for your next idea." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Margin="0,0,0,24"/><TextBlock Text="Color theme" FontWeight="SemiBold" Margin="0,0,0,12"/>
     <UniformGrid Rows="1" Columns="2">
      <Button x:Name="DarkTheme" Style="{DynamicResource SecondaryButton}" HorizontalContentAlignment="Stretch" Padding="12" Margin="0,0,12,0" AutomationProperties.Name="Use dark theme"><StackPanel><Border Background="#181B24" BorderBrush="#3B4455" BorderThickness="1" CornerRadius="8" Height="94" Padding="10"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="32"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="#12151D" CornerRadius="4" Margin="0,0,7,0"><StackPanel Margin="5,8"><Border Height="5" Background="#F1C86A" CornerRadius="2" Margin="0,0,0,6"/><Border Height="4" Background="#3B4455" CornerRadius="2" Margin="0,0,0,5"/><Border Height="4" Background="#3B4455" CornerRadius="2"/></StackPanel></Border><Grid Grid.Column="1"><Grid.RowDefinitions><RowDefinition Height="14"/><RowDefinition/><RowDefinition Height="20"/></Grid.RowDefinitions><Border Background="#BEC5D2" Height="4" Width="46" HorizontalAlignment="Left" CornerRadius="2"/><StackPanel Grid.Row="1" Margin="0,5,0,0"><Border Background="#393124" Height="9" CornerRadius="3" Margin="10,0,0,5"/><Border Background="#3B4455" Height="4" CornerRadius="2" Margin="0,0,12,5"/><Border Background="#3B4455" Height="4" CornerRadius="2" Margin="0,0,24,0"/></StackPanel><Border Grid.Row="2" Background="#252B37" CornerRadius="5" BorderBrush="#3B4455" BorderThickness="1"><Border Width="10" Height="10" Background="#F1C86A" CornerRadius="3" HorizontalAlignment="Right" Margin="4"/></Border></Grid></Grid></Border><TextBlock Text="Dark · Graphite" FontWeight="SemiBold" Margin="0,12,0,0"/><TextBlock Text="Quiet focus, warm highlights" Foreground="{DynamicResource Muted}" FontSize="11" TextWrapping="Wrap" Margin="0,5,0,0"/></StackPanel></Button>
      <Button x:Name="LightTheme" Style="{DynamicResource SecondaryButton}" HorizontalContentAlignment="Stretch" Padding="12" AutomationProperties.Name="Use light theme"><StackPanel><Border Background="#FAFBFD" BorderBrush="#C9D1DE" BorderThickness="1" CornerRadius="8" Height="94" Padding="10"><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="32"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="#EDF0F5" CornerRadius="4" Margin="0,0,7,0"><StackPanel Margin="5,8"><Border Height="5" Background="#806116" CornerRadius="2" Margin="0,0,0,6"/><Border Height="4" Background="#C9D1DE" CornerRadius="2" Margin="0,0,0,5"/><Border Height="4" Background="#C9D1DE" CornerRadius="2"/></StackPanel></Border><Grid Grid.Column="1"><Grid.RowDefinitions><RowDefinition Height="14"/><RowDefinition/><RowDefinition Height="20"/></Grid.RowDefinitions><Border Background="#525F73" Height="4" Width="46" HorizontalAlignment="Left" CornerRadius="2"/><StackPanel Grid.Row="1" Margin="0,5,0,0"><Border Background="#F5ECD7" Height="9" CornerRadius="3" Margin="10,0,0,5"/><Border Background="#C9D1DE" Height="4" CornerRadius="2" Margin="0,0,12,5"/><Border Background="#C9D1DE" Height="4" CornerRadius="2" Margin="0,0,24,0"/></StackPanel><Border Grid.Row="2" Background="#FFFFFF" CornerRadius="5" BorderBrush="#C9D1DE" BorderThickness="1"><Border Width="10" Height="10" Background="#806116" CornerRadius="3" HorizontalAlignment="Right" Margin="4"/></Border></Grid></Grid></Border><TextBlock Text="Light · Paper" FontWeight="SemiBold" Margin="0,12,0,0"/><TextBlock Text="Clear space, crisp contrast" Foreground="{DynamicResource Muted}" FontSize="11" TextWrapping="Wrap" Margin="0,5,0,0"/></StackPanel></Button>
     </UniformGrid>
     <Grid Margin="0,28,0,12"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock Text="Reading size" FontWeight="SemiBold"/><TextBlock x:Name="TextSizeLabel" Grid.Column="1" Foreground="{DynamicResource Accent}"/></Grid><Slider x:Name="TextSize" Minimum="12" Maximum="24" TickFrequency="1" IsSnapToTickEnabled="True" SmallChange="1" LargeChange="2" Foreground="{DynamicResource Accent}" AutomationProperties.Name="Reading size"/><Border Background="{DynamicResource SurfaceElevated}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="12" Padding="20" Margin="0,18,0,10"><StackPanel><TextBlock Text="Reading preview" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,0,0,10"/><TextBlock x:Name="TypePreview" Text="Read, reason, build.&#10;Your ideas deserve room to breathe." TextWrapping="Wrap"/></StackPanel></Border><Button x:Name="ResetSize" Content="Reset to 14 pt" HorizontalAlignment="Left" Foreground="{DynamicResource Accent}"/>
    </StackPanel>
    <StackPanel x:Name="ModelsPage" Visibility="Collapsed"><TextBlock Text="Model &amp; reasoning" FontSize="21" FontWeight="SemiBold"/><TextBlock x:Name="ActiveModel" Foreground="{DynamicResource Muted}" TextWrapping="Wrap" Margin="0,8,0,18"/><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="10,6"><TextBox x:Name="ModelSearch" Background="Transparent" Foreground="{DynamicResource Text}" BorderThickness="0" ToolTip="Search available models" AutomationProperties.Name="Search available models"/></Border><TextBlock x:Name="ModelMatches" Foreground="{DynamicResource Dim}" FontSize="11" Margin="0,6,0,6"/><ListBox x:Name="ModelList" Height="180" AutomationProperties.Name="Available models"><ListBox.ItemTemplate><DataTemplate><TextBlock Text="{Binding}" TextTrimming="CharacterEllipsis" ToolTip="{Binding}"/></DataTemplate></ListBox.ItemTemplate></ListBox><Button x:Name="UseModel" Content="Use selected model" Background="{DynamicResource Accent}" Foreground="{DynamicResource AccentText}" HorizontalAlignment="Left" Padding="14,8" Margin="0,12,0,22"/><TextBlock Text="Reasoning effort" FontWeight="SemiBold" Margin="0,0,0,10"/><WrapPanel x:Name="Efforts"/><TextBlock Text="Choose the balance between response time and reasoning depth for the open chat." Foreground="{DynamicResource Muted}" TextWrapping="Wrap" FontSize="11" Margin="0,6,0,0"/></StackPanel>
    <StackPanel x:Name="WorkspacePage" Visibility="Collapsed"><TextBlock Text="Workspace" FontSize="21" FontWeight="SemiBold"/><Border Background="{DynamicResource Surface}" CornerRadius="8" Padding="14" Margin="0,18,0,10"><TextBlock x:Name="WorkspacePath" TextWrapping="Wrap"/></Border><WrapPanel Margin="0,0,0,24"><Button x:Name="ChooseFolder" Content="Choose folder…" Background="{DynamicResource AccentSoft}" Foreground="{DynamicResource Accent}" Margin="0,0,8,0"/><Button x:Name="OpenFolder" Content="Open in Explorer"/></WrapPanel><TextBlock Text="Choosing a folder starts a new workspace chat." TextWrapping="Wrap" Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,-12,0,24"/>
     <Grid Margin="0,0,0,20"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="Show sidebar" FontWeight="SemiBold"/><TextBlock Text="Keep chats and workspace shortcuts visible." Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,4,12,0"/></StackPanel><ToggleButton x:Name="ShowSidebar" Grid.Column="1" Style="{DynamicResource SettingsSwitch}" AutomationProperties.Name="Show sidebar"/></Grid>
     <Grid Margin="0,0,0,20"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="Show work panel" FontWeight="SemiBold"/><TextBlock Text="Follow todos, plans, and swarm activity." Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,4,12,0"/></StackPanel><ToggleButton x:Name="ShowWork" Grid.Column="1" Style="{DynamicResource SettingsSwitch}" AutomationProperties.Name="Show work panel"/></Grid>
     <Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel><TextBlock Text="Always on top" FontWeight="SemiBold"/><TextBlock Text="Keep this window above other apps." Foreground="{DynamicResource Muted}" FontSize="11" Margin="0,4,12,0"/></StackPanel><ToggleButton x:Name="Topmost" Grid.Column="1" Style="{DynamicResource SettingsSwitch}" AutomationProperties.Name="Always on top"/></Grid>
    </StackPanel>
    <StackPanel x:Name="KeyboardPage" Visibility="Collapsed"><TextBlock Text="Keyboard shortcuts" FontSize="21" FontWeight="SemiBold" Margin="0,0,0,20"/><StackPanel><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+Shift+P" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Command palette" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+Shift+F" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Focus mode" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+K" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Search chats" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+F" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Find in conversation" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+J" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Show or hide work" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+B" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Show or hide sidebar" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+M" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Choose a model" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+N" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="New chat" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+L" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Focus composer" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Enter" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Send message" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Shift+Enter" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Insert a new line" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+Shift+C" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Copy response" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+plus / minus" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Adjust reading size" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid><Grid Margin="0,0,0,12"><Grid.ColumnDefinitions><ColumnDefinition Width="154"/><ColumnDefinition/></Grid.ColumnDefinitions><Border Background="{DynamicResource Surface}" BorderBrush="{DynamicResource Border}" BorderThickness="1" CornerRadius="8" Padding="8,5" HorizontalAlignment="Left"><TextBlock Text="Ctrl+0" FontSize="11" Foreground="{DynamicResource Text}"/></Border><TextBlock Grid.Column="1" Text="Reset reading size" Foreground="{DynamicResource Muted}" VerticalAlignment="Center" TextWrapping="Wrap" Margin="10,0,0,0"/></Grid></StackPanel></StackPanel>
   </Grid></ScrollViewer>
  </Grid>
  <Border Grid.Row="2" Background="{DynamicResource SidebarBg}" BorderBrush="{DynamicResource Border}" BorderThickness="0,1,0,0" Padding="24,10"><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock x:Name="Feedback" Foreground="{DynamicResource Muted}" FontSize="11" TextWrapping="Wrap" VerticalAlignment="Center" Margin="0,0,20,0"/><Button x:Name="Done" Style="{DynamicResource PrimaryButton}" Grid.Column="1" Content="Done" Padding="28,6"/></Grid></Border>
 </Grid></Border>
</Window>
'@
    $dialog = [Windows.Markup.XamlReader]::Load([Xml.XmlNodeReader]::new($definition))
    $dialog.Resources.MergedDictionaries.Add($window.Resources)
    $dialog.Owner = $window
    $dialog.Height = [Math]::Min($dialog.Height,[Windows.SystemParameters]::WorkArea.Height-32)
    $script:settingsWindow = $dialog; $script:settingsControls = @{}
    foreach ($name in @('TitleBar','Close','Done','AppearanceNav','ModelsNav','WorkspaceNav','KeyboardNav','AppearancePage','ModelsPage','WorkspacePage','KeyboardPage','DarkTheme','LightTheme','TextSize','TextSizeLabel','TypePreview','ResetSize','ActiveModel','ModelSearch','ModelMatches','ModelList','UseModel','Efforts','WorkspacePath','ChooseFolder','OpenFolder','ShowSidebar','ShowWork','Topmost','Feedback')) { $script:settingsControls[$name] = $dialog.FindName($name) }
    $script:settingsModelsSignature = $null; $script:settingsEffortsSignature = $null; $script:settingsSection = 'Appearance'
    Initialize-ProviderSettings $dialog
    foreach ($name in @('Appearance','Models','Providers','Workspace','Keyboard')) { $script:settingsControls[$name+'Nav'].Add_Click({ param($sender) Select-SettingsSection ([string]$sender.Tag) }) }
    $script:settingsControls.DarkTheme.Add_Click({ Set-GuiTheme 'dark'; Update-SettingsForm })
    $script:settingsControls.LightTheme.Add_Click({ Set-GuiTheme 'light'; Update-SettingsForm })
    $script:settingsControls.TextSize.Add_ValueChanged({ param($sender) if (-not $script:settingsUpdating) { Set-ReadingSize ([Math]::Round($sender.Value)); Update-SettingsForm } })
    $script:settingsControls.ResetSize.Add_Click({ $script:settingsControls.TextSize.Value = 14 })
    $script:settingsControls.ShowSidebar.Add_Click({ param($sender) Set-SidebarVisibility ([bool]$sender.IsChecked); Update-SettingsForm })
    $script:settingsControls.ShowWork.Add_Click({ param($sender) Set-WorkflowVisibility ([bool]$sender.IsChecked); Update-SettingsForm })
    $script:settingsControls.Topmost.Add_Click({ param($sender) $window.Topmost = [bool]$sender.IsChecked; $pin.IsChecked = $window.Topmost; Save-GuiSettings; Update-SettingsForm })
    $script:settingsControls.ModelSearch.Add_TextChanged({ Update-SettingsModels })
    $script:settingsControls.ModelList.Add_SelectionChanged({ if (-not $script:settingsUpdating) { Update-SettingsModels } })
    $script:settingsControls.UseModel.Add_Click({ if ($script:settingsControls.UseModel.IsEnabled) { Queue-ModelChange ([string]$script:settingsControls.ModelList.SelectedItem); Update-SettingsForm } })
    $script:settingsControls.ChooseFolder.Add_Click({ $controls.AddContext.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent)); Update-SettingsForm })
    $script:settingsControls.OpenFolder.Add_Click({ Open-SelectedWorkspace })
    $script:settingsControls.Close.Add_Click({ $script:settingsWindow.Close() })
    $script:settingsControls.Done.Add_Click({ $script:settingsWindow.Close() })
    $script:settingsControls.TitleBar.Add_MouseLeftButtonDown({ param($sender,$event) if ($event.ChangedButton -eq [Windows.Input.MouseButton]::Left) { $script:settingsWindow.DragMove() } })
    $dialog.Add_PreviewKeyDown({ param($sender,$event) if ($event.Key -eq 'Escape') { $sender.Close(); $event.Handled = $true } })
    $dialog.Add_Closed({ Save-GuiSettings; $script:settingsWindow = $null; $script:settingsControls = @{} })
    Update-SettingsForm
    return $dialog
}

function Show-Settings {
    try {
        if ($script:settingsWindow) { $script:settingsWindow.Activate() | Out-Null; return }
        $dialog = New-SettingsWindow
        $dialog.ShowDialog() | Out-Null
        Focus-SelectedChat
    } catch { Show-GuiNotice ("Could not open settings: " + $_.Exception.GetBaseException().Message) 12 }
}

function Get-ChatListScroller {
    $pending = New-Object System.Collections.Queue
    $pending.Enqueue($controls.Sessions)
    while ($pending.Count) {
        $visual = $pending.Dequeue()
        if ($visual -is [Windows.Controls.ScrollViewer]) { return $visual }
        for ($i=0; $i -lt [Windows.Media.VisualTreeHelper]::GetChildrenCount($visual); $i++) { $pending.Enqueue([Windows.Media.VisualTreeHelper]::GetChild($visual,$i)) }
    }
    return $null
}

function Focus-SelectedChat {
    $row = Get-SelectedSessionRow
    if ($row -and $row.state -eq 'stored') { $controls.ReplyScroll.Focus() | Out-Null }
    elseif ($script:passwordMode) { $controls.SecretInput.Focus() | Out-Null }
    elseif ($script:inputMode) { $controls.CommandInput.Focus() | Out-Null }
    else { $controls.Prompt.Focus() | Out-Null }
}

function Select-ChatSearchResult([int]$index = 0, [bool]$focusComposer = $true) {
    if (-not $controls.Sessions.Items.Count) { return }
    $controls.Sessions.SelectedIndex = [Math]::Max(0,[Math]::Min($index,$controls.Sessions.Items.Count-1))
    $controls.Sessions.ScrollIntoView($controls.Sessions.SelectedItem)
    if ($focusComposer) { Focus-SelectedChat } else { $controls.Sessions.Focus() | Out-Null }
}

function Update-SidebarChrome {
    $controls.NavHome.Background = Get-GuiBrush $(if ($script:sidebarVisible) { 'AccentSoft' } else { 'RailBg' })
    $controls.NavHome.BorderThickness = [Windows.Thickness]::new(2,0,0,0)
    $controls.NavHome.BorderBrush = if ($script:sidebarVisible) { Get-GuiBrush 'Accent' } else { [Windows.Media.Brushes]::Transparent }
    $controls.NavHome.Foreground = Get-GuiBrush $(if ($script:sidebarVisible) { 'Accent' } else { 'Muted' })
    $controls.NavHome.ToolTip = if ($script:sidebarVisible) { 'Hide sidebar (Ctrl+B)' } else { 'Show sidebar (Ctrl+B)' }
    $controls.NavHome.SetValue([Windows.Automation.AutomationProperties]::NameProperty, $(if ($script:sidebarVisible) { 'Hide sidebar' } else { 'Show sidebar' }))
    $controls.NavHome.SetValue([Windows.Automation.AutomationProperties]::ItemStatusProperty, $(if ($script:sidebarVisible) { 'Expanded' } else { 'Collapsed' }))
    foreach ($pair in @(@($controls.FilterAll,'Home'),@($controls.FilterLive,'Live'),@($controls.FilterSaved,'Archive'))) {
        $active = $script:navigation -eq $pair[1]
        $pair[0].Background = Get-GuiBrush $(if ($active) { 'AccentSoft' } else { 'Surface' })
        $pair[0].Foreground = Get-GuiBrush $(if ($active) { 'Accent' } else { 'Muted' })
        $pair[0].SetValue([Windows.Automation.AutomationProperties]::ItemStatusProperty, $(if ($active) { 'Selected' } else { '' }))
    }
}

function Apply-Navigation {
    $selected = Get-SelectedSessionId
    $visible = @(switch ($script:navigation) {
        'Projects' { @($script:allSessions | Where-Object { $_.id -ne '__companion__' -and -not [string]::IsNullOrWhiteSpace([string]$_.working_dir) }) }
        'Live' { @($script:allSessions | Where-Object { $_.state -ne 'stored' }) }
        'Recents' { @($script:allSessions | Where-Object { $_.id -ne '__companion__' }) }
        'Archive' { @($script:allSessions | Where-Object { [bool]$_.saved }) }
        default { @($script:allSessions) }
    })
    $query = ([string]$controls.SessionSearch.Text).Trim()
    if ($query) {
        $pattern = [regex]::Escape($query)
        $visible = @($visible | Where-Object { ([string]$_.title -match $pattern) -or ([string]$_.detail -match $pattern) -or ([string]$_.working_dir -match $pattern) })
        $ranked = for ($index=0; $index -lt $visible.Count; $index++) {
            $title = [string]$visible[$index].title
            $rank = if ($title.Equals($query,[StringComparison]::OrdinalIgnoreCase)) { 0 } elseif ($title.StartsWith($query,[StringComparison]::OrdinalIgnoreCase)) { 1 } elseif ($title.IndexOf($query,[StringComparison]::OrdinalIgnoreCase) -ge 0) { 2 } else { 3 }
            [pscustomobject]@{ Row=$visible[$index]; Rank=$rank; Index=$index }
        }
        $visible = @($ranked | Sort-Object Rank,Index | ForEach-Object { $_.Row })
    }
    foreach ($row in $visible) {
        $status = switch ([string]$row.state) { 'working' { 'Working' } 'attention' { 'Needs you' } 'offline' { 'Offline' } 'stored' { 'Read-only' } default { 'Ready' } }
        $icon = switch ([string]$row.state) { 'working' { '●' } 'attention' { '!' } 'offline' { '↻' } 'stored' { '▤' } default { '○' } }
        $subtitle = if ($row.working_dir) { $leaf = Split-Path -Leaf ([string]$row.working_dir); if ($leaf) { $leaf } else { [string]$row.working_dir } } elseif ($row.detail -and [string]$row.detail -ne 'Ready') { [string]$row.detail } else { '' }
        $tooltip = [string]$row.title
        if ($row.working_dir) { $tooltip += "`n" + [string]$row.working_dir }
        if ($row.detail) { $tooltip += "`n" + [string]$row.detail }
        $row | Add-Member -NotePropertyName row_tooltip -NotePropertyValue $tooltip -Force
        $row | Add-Member -NotePropertyName status_label -NotePropertyValue $status -Force
        $row | Add-Member -NotePropertyName row_icon -NotePropertyValue $icon -Force
        $row | Add-Member -NotePropertyName row_subtitle -NotePropertyValue $subtitle -Force
        $row | Add-Member -NotePropertyName saved_marker -NotePropertyValue $(if ($row.saved) { '★' } else { '' }) -Force
    }
    $label = if ($query) { 'Search results' } else { switch ($script:navigation) { 'Live' { 'Live chats' } 'Archive' { 'Saved chats' } 'Projects' { 'Workspace chats' } default { 'Chats' } } }
    $controls.ChatsHeading.Text = "$label · $($visible.Count)"
    Update-SidebarChrome
    $listSignature = ($script:navigation + "`n" + $query + "`n" + (@($visible | ForEach-Object { "$($_.id)|$($_.title)|$($_.state)|$($_.detail)|$($_.working_dir)|$($_.saved)" }) -join "`n"))
    if ($script:navigationListSignature -eq $listSignature) { return }
    $script:navigationListSignature = $listSignature
    $scroller = Get-ChatListScroller
    $offset = if ($scroller) { $scroller.VerticalOffset } else { 0 }
    $priorSuppression = $script:suppressSelectionEvents
    $script:suppressSelectionEvents = $true
    try {
        $controls.Sessions.ItemsSource = $visible
        $controls.SessionsEmpty.Visibility = if ($visible.Count -eq 0) { 'Visible' } else { 'Collapsed' }
        $controls.SessionsEmpty.Text = if ($query) { 'No matching chats. Clear the search or try another title or workspace.' } elseif ($script:navigation -eq 'Archive') { 'No saved chats yet. Use the star to save a live chat.' } else { 'No chats in this view.' }
        $visibleIds = @($visible | ForEach-Object { $_.id })
        $allIds = @($script:allSessions | ForEach-Object { $_.id })
        if ($visibleIds -contains $selected) { $controls.Sessions.SelectedValue = $selected }
        elseif (-not $selected -or $allIds -notcontains $selected) {
            if ($visibleIds -contains '__companion__') { $controls.Sessions.SelectedValue = '__companion__' }
            elseif ($visible.Count) { $controls.Sessions.SelectedIndex = 0 }
            else { $controls.Sessions.SelectedValue = $null }
        } else { $controls.Sessions.SelectedValue = $null }
        if ($scroller) { $scroller.ScrollToVerticalOffset($offset) }
    } finally { $script:suppressSelectionEvents = $priorSuppression }
    Update-SelectedSession
}

# Convert the bounded transcript projection into readable chat text. The
# projection is read-only and already truncated server-side, so this only
# formats roles and tool traces.
function Format-Transcript($transcript) {
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($message in @($transcript.messages)) {
        $role = switch ([string]$message.role) {
            'user' { 'You' }
            'assistant' { 'AlphaCode' }
            default { 'Session' }
        }
        $body = [string]$message.text
        if (-not [string]::IsNullOrWhiteSpace($body)) {
            $lines.Add("$role`n$body")
        }
        $tools = @($message.tools | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
        if ($tools.Count -gt 0) {
            $lines.Add("  ⚙ tools: " + ($tools -join ', '))
        }
    }
    return ($lines -join "`n`n")
}

# Merge live daemon sessions with the canonical on-disk session index so the
# sidebar shows the same chats the CLI session picker shows. Live sessions win
# when an id appears in both, because a live session also carries runtime state
# (working/attention/offline) and a local socket the desktop can drive.
function Get-SessionRows {
    $rows = New-Object System.Collections.Generic.List[object]
    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($live in @($script:liveSessions)) {
        [void]$rows.Add($live)
        if ($live.id) { [void]$seen.Add([string]$live.id) }
    }
    foreach ($stored in @($script:storedSessions)) {
        $id = [string]$stored.id
        if ([string]::IsNullOrWhiteSpace($id) -or $seen.Contains($id)) { continue }
        [void]$seen.Add($id)
        $rows.Add([pscustomobject]@{
            id = $id
            title = [string]$stored.title
            detail = [string]$stored.detail
            state = 'stored'
            saved = [bool]$stored.saved
            save_label = $stored.save_label
            source = [string]$stored.source
            model = $stored.model
            working_dir = $stored.working_dir
            last_active_at = $stored.last_active_at
            last_message_time = [string]$stored.last_message_time
        })
    }
    return $rows.ToArray()
}

# Apply the canonical on-disk session index (the same list the CLI session
# picker shows). Rows are only re-merged when the set actually changes, so the
# selected chat and scroll position survive the periodic refresh.
function Set-StoredSessions($stored) {
    $rows = @($stored.sessions)
    $cursor = "$($stored.session_count):$($stored.saved_count):$($stored.returned)"
    if ($cursor -eq $script:lastStoredCount -and $rows.Count -eq $script:storedSessions.Count) {
        $same = $true
        for ($i = 0; $i -lt $rows.Count; $i++) {
            if ([string]$rows[$i].id -ne [string]$script:storedSessions[$i].id -or
                [string]$rows[$i].title -ne [string]$script:storedSessions[$i].title -or
                [bool]$rows[$i].saved -ne [bool]$script:storedSessions[$i].saved -or
                [string]$rows[$i].detail -ne [string]$script:storedSessions[$i].detail -or
                [string]$rows[$i].working_dir -ne [string]$script:storedSessions[$i].working_dir -or
                [string]$rows[$i].last_message_time -ne [string]$script:storedSessions[$i].last_message_time) { $same = $false; break }
        }
        if ($same) { return }
    }
    $script:storedSessions = $rows
    $script:lastStoredCount = $cursor
    $script:allSessions = @(Get-SessionRows)
    Update-ProjectsList
    Apply-Navigation
}

# The effort picker ships five presets, but a runtime can report an effective
# effort outside that set ("minimal", "xhigh", "swarm"). ComboBox.SelectedValue
# then matches nothing and the control renders blank, so options are extended
# on demand and the dynamic ones are pruned whenever the model list changes.
function Ensure-EffortOption([string]$effort) {
    if ([string]::IsNullOrWhiteSpace($effort)) { return }
    $known = @($controls.EffortPicker.Items | ForEach-Object { [string]$_.Tag })
    if ($known -contains $effort) { return }
    $item = New-Object Windows.Controls.ComboBoxItem
    $item.Content = (Get-Culture).TextInfo.ToTitleCase($effort)
    $item.Tag = $effort
    [void]$controls.EffortPicker.Items.Add($item)
}

function Reset-EffortOptions {
    $presets = @('none','low','medium','high','max')
    $items = @($controls.EffortPicker.Items)
    for ($index = $items.Count - 1; $index -ge 0; $index--) {
        $item = $items[$index]
        if ($item -is [Windows.Controls.ComboBoxItem] -and $presets -notcontains [string]$item.Tag) {
            $controls.EffortPicker.Items.Remove($item)
        }
    }
}

function Set-Snapshot($snapshot) {
    if ($snapshot.provider_connection) { Set-ProviderProgress $snapshot.provider_connection }
    $firstConnection = -not $script:hasLiveSnapshot
    $script:hasLiveSnapshot = $true
    $script:ownedBusy = [bool]$snapshot.owned_busy
    $script:responseDisconnected = $false
    $script:working = [int]$snapshot.working
    $selected = $controls.Sessions.SelectedValue
    # Include fields that affect navigation and context actions. A live
    # session can keep the same id while its title, state, or workspace moves.
    $signature = (@($snapshot.sessions | ForEach-Object { $_.id + ':' + $_.title + ':' + $_.state + ':' + $_.detail + ':' + $_.working_dir + ':' + $_.saved }) -join '|')
    if ($signature -ne $script:lastSignature) {
        # Keep the picker predictable: the AlphaCode chat first, actionable sessions
        # next, then the remaining sessions in locale-aware title order.
        # Sort-Object hashtable entries accept only Expression/Ascending/Descending/Unique;
        # its string comparison already follows the current UI culture.
        $script:liveSessions = @($snapshot.sessions | Sort-Object `
            @{ Expression = { if ($_.id -eq '__companion__') { 0 } elseif ($_.state -in @('working','attention')) { 1 } else { 2 } } }, `
            @{ Expression = { $_.title } })
        $script:lastSignature = $signature
    }
    $script:allSessions = @(Get-SessionRows)
    Update-ProjectsList
    Apply-Navigation
    if ($firstConnection -and -not $script:userSelectedChat -and $script:navigation -eq 'Home' -and [string]::IsNullOrWhiteSpace([string]$controls.SessionSearch.Text) -and @($controls.Sessions.ItemsSource | ForEach-Object { $_.id }) -contains '__companion__') { $controls.Sessions.SelectedValue = '__companion__' }
    $liveCount = @($script:allSessions | Where-Object { $_.id -ne '__companion__' -and $_.state -ne 'stored' }).Count
    $storedCount = @($script:allSessions | Where-Object { $_.state -eq 'stored' }).Count
    if ($storedCount -gt 0) {
        $controls.SessionCount.Text = "$liveCount live · $storedCount saved"
    } elseif ($liveCount -eq 1) {
        $controls.SessionCount.Text = '1 live'
    } else {
        $controls.SessionCount.Text = "$liveCount live"
    }
    $allowedModels = @($snapshot.models | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } | ForEach-Object { [string]$_ })
    $options = if ($null -ne $snapshot.model_options) { @($snapshot.model_options) } elseif ($snapshot.provider -and $snapshot.model -and $allowedModels -ccontains [string]$snapshot.model) { @([pscustomobject]@{id=[string]$snapshot.model; provider=[string]$snapshot.provider}) } else { @() }
    $seenModels = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
    $script:modelRows = @(foreach ($option in $options) {
        $id = [string]$option.id
        if ($allowedModels -ccontains $id -and $seenModels.Add($id)) {
            [pscustomobject]@{id=$id; name=(Get-ModelDisplayName $id); provider=[string]$option.provider}
        }
    })
    $script:availableModels = @($script:modelRows | ForEach-Object { $_.id })
    $models = $script:availableModels
    if ($models.Count -gt 0) {
        $script:suppressSelectionEvents = $true
        # Rebuilding ItemsSource on every 1s poll collapsed an open drop-down and
        # discarded the user's selection mid-interaction, so only rebuild when
        # the underlying list actually changes.
        $listSignature = $models -join "`n"
        if ($listSignature -ne $script:modelListSignature) {
            $script:modelListSignature = $listSignature
            $controls.ModelPicker.ItemsSource = $models
            Reset-EffortOptions
        }
        $targetModel = if ($script:modelRequestPending -and $script:preferredModel -and $models -contains $script:preferredModel) { $script:preferredModel } elseif ($snapshot.model -and $models -contains [string]$snapshot.model) { [string]$snapshot.model } else { $null }
        if ([string]$controls.ModelPicker.SelectedItem -ne [string]$targetModel) { $controls.ModelPicker.SelectedItem = $targetModel }
        $script:selectedModel = [string]$targetModel
        $targetEffort = if ($script:effortRequestPending -and $script:preferredEffort) { $script:preferredEffort } elseif ([string]$snapshot.effort) { [string]$snapshot.effort } else { 'none' }
        Ensure-EffortOption $targetEffort
        if ([string]$controls.EffortPicker.SelectedValue -ne $targetEffort) { $controls.EffortPicker.SelectedValue = $targetEffort }
        $script:selectedEffort = $targetEffort
        $script:suppressSelectionEvents = $false
        $controls.ModelPicker.IsEnabled = -not $script:modelRequestPending
        $controls.EffortPicker.IsEnabled = -not $script:effortRequestPending
    } else {
        $script:modelListSignature = $null
        $script:suppressSelectionEvents = $true
        $placeholder = if ($selected -eq '__companion__' -and [string]::IsNullOrWhiteSpace([string]$snapshot.owned_session_id)) { 'Connecting…' } else { 'Sign in to a provider' }
        if ([string]$controls.ModelPicker.SelectedItem -ne $placeholder) {
            $controls.ModelPicker.ItemsSource = @($placeholder)
            $controls.ModelPicker.SelectedIndex = 0
        }
        $controls.ModelPicker.IsEnabled = $false
        $controls.EffortPicker.SelectedValue = 'none'
        $controls.EffortPicker.IsEnabled = $false
        $script:selectedModel = ''
        $script:suppressSelectionEvents = $false
    }
    if ($script:pendingResponseSession -and $script:pendingKind -notin @('prompt','input') -and $script:queue.Count -eq 0) { $script:pendingResponseSession = '' }
    $script:snapshot = $snapshot
    $badge = if ([int]$snapshot.attention -gt 0) { 'Needs you' } elseif ([int]$snapshot.offline -gt 0) { 'Reconnecting' } elseif ($script:working -gt 0) { "$($script:working) working" } else { 'Ready' }
    $color = if ([int]$snapshot.attention -gt 0 -or [int]$snapshot.offline -gt 0) { 'Warning' } elseif ($script:working -gt 0) { 'Accent' } else { 'Success' }
    $script:dictationAvailable = [bool]$snapshot.dictation_available
    # Keep the affordance usable even when dictation is not configured: the
    # click explains the one-line config needed instead of leaving a dead,
    # permanently disabled control in the composer.
    $controls.Voice.Visibility = if ($script:dictationAvailable) { 'Visible' } else { 'Collapsed' }
    $controls.Voice.IsEnabled = $script:dictationAvailable -and -not $script:inputMode
    $controls.Voice.ToolTip = if ($script:dictationAvailable) { 'Run configured dictation and insert the transcript' } else { 'Configure [dictation].command to enable voice input' }
    $controls.StateText.Foreground = Get-GuiBrush $color
    $controls.SidebarStatus.Foreground = Get-GuiBrush $color
    $controls.StateText.Text = $badge
    $controls.StatePill.Background = Get-GuiBrush $(if ($badge -in @('Needs you','Reconnecting')) { 'WarningSoft' } else { 'SuccessSoft' })
    $controls.SidebarStatus.Text = "●  $badge  ·  $($snapshot.connected_servers) server(s)"
    if ($script:actionNotice -and [DateTime]::UtcNow -lt $script:actionNoticeUntil) {
        $controls.FooterStatus.Text = $script:actionNotice
    } else {
        $script:actionNotice = ''
        $controls.FooterStatus.Text = "${badge}  ·  local sockets protected  ·  model follows your AlphaCode settings"
    }
    Update-SelectedSession
}

function Update-SelectedSession([string]$sessionId = '') {
    if (-not $script:snapshot) { return }
    $selected = if ($sessionId) { $sessionId } else { $controls.Sessions.SelectedValue }
    if (-not [string]::IsNullOrWhiteSpace([string]$selected)) {
        if ($script:lastSelectedSessionId -ne [string]$selected) { $script:lastTranscriptPoll = [DateTime]::MinValue }
        $script:lastSelectedSessionId = [string]$selected
    }
    elseif ($script:lastSelectedSessionId) { $selected = $script:lastSelectedSessionId }
    if ($script:transcriptFor -and $script:transcriptFor -ne $selected) {
        $script:transcript = ''
        $script:transcriptFor = ''
    }
    $session = @($script:allSessions | Where-Object { $_.id -eq $selected }) | Select-Object -First 1
    if (-not $session) { return }
    if ($script:draftSession -ne [string]$selected) {
        if ($controls.ReplySearchBar.Visibility -eq 'Visible') { Close-ConversationSearch }
        $script:deliveryNotice = ''
        $script:noticeUntil = [DateTime]::MinValue
        if ($script:draftSession) { $script:drafts[$script:draftSession] = $controls.Prompt.Text }
        $script:draftSession = [string]$selected
        $controls.Prompt.Text = [string]$script:drafts[$script:draftSession]
        $controls.Prompt.CaretIndex = $controls.Prompt.Text.Length
        $controls.ReplyScroll.ScrollToHome()
    }
    $storedOnly = [string]$session.state -eq 'stored'
    $controls.Prompt.IsReadOnly = $storedOnly
    $controls.Placeholder.Text = if ($storedOnly) { 'Saved chat · open Terminal to continue' } else { 'Describe a task. Ask a question. Start here.' }
    $controls.ModelPicker.IsEnabled = -not $storedOnly -and $script:availableModels.Count -gt 0 -and -not $script:modelRequestPending
    $controls.EffortPicker.IsEnabled = -not $storedOnly -and -not [string]::IsNullOrWhiteSpace($script:selectedModel) -and -not $script:effortRequestPending
    $controls.WorkspaceTitle.Text = $session.title
    $controls.WorkspaceSubtitle.Text = switch ($session.state) {
        'working' { if ($session.detail) { [string]$session.detail } else { 'Working in this workspace…' } }
        'attention' { 'This workspace needs a decision from you' }
        'offline' { 'The workspace is reconnecting to its local daemon' }
        'stored' { 'Saved chat from the CLI session list · read-only here' }
        default { if ($session.working_dir) { [string]$session.working_dir } else { 'Ready to work in your workspace' } }
    }
    $controls.SidebarHint.Text = "Open: $($session.title)"
    $controls.SidebarHint.ToolTip = [string]$session.detail
    $controls.Activity.Text = switch ($session.state) { 'working' { 'Working on it' } 'attention' { 'Needs your attention' } 'offline' { 'Reconnecting to session' } 'stored' { 'Saved chat' } default { 'Ready for your next idea' } }
    $controls.Detail.Text = $session.detail
    Set-TextDirection $controls.Detail $session.detail
    if ($storedOnly) { $controls.Timeline.Text = if ([string]$session.source -eq 'alphacode') { 'Read-only history · resume this chat from the terminal to continue' } else { 'Resume this imported chat from the terminal to view and continue it' } }
    elseif ($script:snapshot.activity -and @($script:snapshot.activity).Count -gt 0) { $controls.Timeline.Text = (@($script:snapshot.activity) -join "`n") } else { $controls.Timeline.Text = 'Live activity will appear here' }
    $inputRequest = if ($selected -eq '__companion__') { $script:snapshot.input_request } else { $null }
    $script:inputMode = $null -ne $inputRequest
    $script:passwordMode = $script:inputMode -and [bool]$inputRequest.is_password
    $controls.Voice.IsEnabled = $script:dictationAvailable -and -not $script:inputMode
    if ($script:inputMode -and $script:currentInputRequest -ne $inputRequest.request_id) {
        $controls.CommandInput.Clear(); $controls.SecretInput.Clear()
        $script:currentInputRequest = $inputRequest.request_id
    }
    if (-not $script:inputMode) {
        $controls.CommandInput.Clear(); $controls.SecretInput.Clear()
        $script:currentInputRequest = ''
    }
    $controls.Prompt.Visibility = if ($script:inputMode) { 'Collapsed' } else { 'Visible' }
    $controls.CommandInput.Visibility = if ($script:inputMode -and -not $script:passwordMode) { 'Visible' } else { 'Collapsed' }
    $controls.SecretInput.Visibility = if ($script:passwordMode) { 'Visible' } else { 'Collapsed' }
    $controls.Placeholder.Visibility = if ($script:inputMode -or $controls.Prompt.Text.Length) { 'Collapsed' } else { 'Visible' }
    $controls.Send.Content = '↑'
    $controls.Send.ToolTip = if ($script:inputMode) { 'Send command input (Enter)' } else { 'Send message (Enter)' }
    if ($script:inputMode) { $controls.Activity.Text = 'A command needs your input'; $controls.Detail.Text = $inputRequest.prompt }
    if ($script:deliveryNotice -and [DateTime]::UtcNow -lt $script:noticeUntil) {
        $controls.Detail.Text = $script:deliveryNotice
        $controls.FooterStatus.Text = $script:deliveryNotice
    } else { $script:deliveryNotice = '' }
    if ($selected -eq '__companion__') {
        $reply = if ($script:snapshot.reply) { $script:snapshot.reply } else { '' }
    } else {
        $reply = if ($script:snapshot.selected_session_id -eq $selected -and $script:snapshot.selected_reply) { $script:snapshot.selected_reply } else { '' }
    }
    if ($script:transcriptFor -eq $selected -and [string]$script:transcript) { $reply = $script:transcript }
    $hasReply = -not [string]::IsNullOrWhiteSpace($reply)
    $controls.EmptyState.Visibility = if ($hasReply) { 'Collapsed' } else { 'Visible' }
    $controls.ReplyScroll.Visibility = if ($hasReply) { 'Visible' } else { 'Collapsed' }
    if (-not $hasReply) { $controls.JumpLatest.Visibility = 'Collapsed' }
    if (-not $hasReply -and -not $script:inputMode -and $session.state -eq 'ready') {
        $controls.Activity.Text = if ($selected -eq '__companion__') { 'What will you build today?' } else { 'Ready for your next idea' }
    }
    if ($controls.Reply.Text -ne $reply) {
        $following = $controls.ReplyScroll.VerticalOffset -ge ($controls.ReplyScroll.ScrollableHeight - 8)
        $controls.Reply.Text = $reply
        Set-TextDirection $controls.Reply $reply
        if ($following) { $controls.ReplyScroll.ScrollToEnd() }
    }
    $controls.Copy.IsEnabled = $hasReply
    $controls.FindReply.IsEnabled = $hasReply
    $controls.Cancel.IsEnabled = -not $storedOnly -and (($selected -eq '__companion__' -and $script:ownedBusy) -or ($selected -ne '__companion__' -and [string]$session.state -eq 'working'))
    $ownedSessionReady = $null -ne $script:snapshot -and -not [string]::IsNullOrWhiteSpace([string]$script:snapshot.owned_session_id)
    $canManage = -not $storedOnly -and [string]$session.state -notin @('offline','working') -and ($selected -ne '__companion__' -or $ownedSessionReady)
    $controls.Rename.IsEnabled = $canManage
    $controls.Archive.IsEnabled = $canManage
    $controls.Rename.ToolTip = 'Rename this chat'
    $controls.Archive.ToolTip = 'Save or unsave this chat'
    $controls.AddContext.IsEnabled = -not $script:ownedBusy
    $controls.AddContext.ToolTip = 'Choose a workspace folder for a new chat'
    $workspace = [string]$session.working_dir
    $controls.WorkspacePath.Text = if ($workspace) { Split-Path -Leaf $workspace } else { 'Choose workspace' }
    $controls.WorkspacePath.ToolTip = if ($workspace) { $workspace } else { 'Choose a folder with the + button' }
    $controls.Archive.Content = if ([bool]$session.saved) { [char]0x2605 } else { [char]0x2606 }
    $controls.Archive.Foreground = Get-GuiBrush $(if ([bool]$session.saved) { 'Warning' } else { 'Muted' })
    $controls.Send.ToolTip = if ($storedOnly) { 'Resume this chat from the terminal to continue it' } else { 'Send prompt (Enter; Shift+Enter or Alt+Enter for a new line)' }
    $notice = if ($script:inputMode) { [string]$inputRequest.prompt } elseif ($script:deliveryNotice -and [DateTime]::UtcNow -lt $script:noticeUntil) { $script:deliveryNotice } else { '' }
    $noticeRole = if (-not $script:inputMode -and $notice -eq 'Prompt delivered') { 'Success' } else { 'Warning' }
    $controls.NoticeText.Foreground = Get-GuiBrush $noticeRole
    $controls.NoticeBanner.Background = Get-GuiBrush ($noticeRole + 'Soft')
    $controls.NoticeText.Text = $notice
    $controls.NoticeBanner.Visibility = if ($notice) { 'Visible' } else { 'Collapsed' }
    Update-ComposerState
    Update-Workflow
    Update-SettingsForm
    Update-ModelChooser
    Update-ResponseActivity
}

function Send-Prompt {
    if (-not $controls.Send.IsEnabled) { return }
    $content = if ($script:passwordMode) { $controls.SecretInput.Password } elseif ($script:inputMode) { $controls.CommandInput.Text } else { $controls.Prompt.Text }
    if (-not $script:inputMode -and [string]::IsNullOrWhiteSpace($content)) { return }
    if ($Preview) { $controls.Detail.Text = 'Preview only. No prompt was sent.'; return }
    if ($script:queue.Count -gt 0 -or $script:pendingKind -in @('prompt','input')) { return }
    $script:deliveryNotice = ''
    $script:noticeUntil = [DateTime]::MaxValue
    $script:submittedInput = $script:inputMode
    $script:queue.Enqueue(@{ action=$(if ($script:inputMode) { 'input' } else { 'prompt' }); session_id=(Get-SelectedSessionId); request_id=$script:currentInputRequest; prompt=$content })
    $controls.Send.IsEnabled = $false
    $controls.Detail.Text = 'Sending your prompt…'
    $script:pendingResponseSession = Get-SelectedSessionId
    Update-ResponseActivity
}

# Queue a non-composer bridge request (cancel/rename/archive/new_chat).
#
# These share the one-in-flight-request invariant the timer loop enforces: at
# most one queued request and one pending response, so the bridge's strictly
# sequential stdin/stdout protocol stays framed.
function Can-QueueAction {
    if ($script:queue.Count -gt 0 -or ($script:pending -and $script:pendingKind -notin @('status','sessions','transcript'))) {
        $controls.FooterStatus.Text = 'Please wait for the current action to finish.'
        return $false
    }
    return $true
}

function Queue-Action([string]$action, [string]$sessionId, [hashtable]$extra = @{}) {
    if ($Preview) { $controls.Detail.Text = "Preview only. '$action' was not sent."; return }
    if (-not (Can-QueueAction)) { return }
    $request = @{ action=$action; session_id=$sessionId }
    foreach ($key in $extra.Keys) { $request[$key] = $extra[$key] }
    $script:queue.Enqueue($request)
    $controls.Send.IsEnabled = $false
}

# Small modal input dialog used by Rename and Archive. Returns through the
# supplied script block so callers keep control of what they queue.
function Show-ValueDialog([string]$title, [string]$hint, [string]$initial, [scriptblock]$onSubmit) {
    if ($Preview) { & $onSubmit $initial; return }
    $dialog = New-Object Windows.Window
    $dialog.Title = $title
    $dialog.Width = 420; $dialog.Height = 178
    $dialog.WindowStartupLocation = 'CenterOwner'
    $dialog.Owner = $window
    $dialog.ResizeMode = 'NoResize'
    $dialog.ShowInTaskbar = $false
    $dialog.Background = (Get-GuiBrush 'SidebarBg')
    $dock = New-Object Windows.Controls.DockPanel
    $dock.Margin = [Windows.Thickness]::new(16)
    $text = New-Object Windows.Controls.TextBox
    $text.Text = $initial
    $text.FontSize = 14
    $text.Padding = [Windows.Thickness]::new(6)
    $text.Background = (Get-GuiBrush 'Surface')
    $text.Foreground = (Get-GuiBrush 'Text')
    $text.BorderBrush = (Get-GuiBrush 'Border')
    [Windows.Controls.DockPanel]::SetDock($text, 'Top')
    $buttons = New-Object Windows.Controls.StackPanel
    $buttons.Orientation = 'Horizontal'
    $buttons.HorizontalAlignment = 'Right'
    $buttons.Margin = [Windows.Thickness]::new(0,12,0,0)
    [Windows.Controls.DockPanel]::SetDock($buttons, 'Bottom')
    $ok = New-Object Windows.Controls.Button
    $ok.Content = 'Save'; $ok.Padding = [Windows.Thickness]::new(16,5,16,5)
    $ok.Margin = [Windows.Thickness]::new(0,0,8,0)
    $ok.IsDefault = $true
    $cancel = New-Object Windows.Controls.Button
    $cancel.Content = 'Cancel'; $cancel.Padding = [Windows.Thickness]::new(16,5,16,5)
    $cancel.IsCancel = $true
    $label = New-Object Windows.Controls.TextBlock
    $label.Text = $hint
    $label.Foreground = (Get-GuiBrush 'Muted')
    $label.FontSize = 11
    $label.Margin = [Windows.Thickness]::new(0,0,0,8)
    [Windows.Controls.DockPanel]::SetDock($label, 'Top')
    $ok.Add_Click({
        $dialog.DialogResult = $true
        $dialog.Close()
    })
    [void]$buttons.Children.Add($ok)
    [void]$buttons.Children.Add($cancel)
    [void]$dock.Children.Add($label)
    [void]$dock.Children.Add($text)
    [void]$dock.Children.Add($buttons)
    $dialog.Content = $dock
    $dialog.Add_ContentRendered({ $text.Focus() | Out-Null; $text.SelectAll() })
    if ($dialog.ShowDialog() -eq $true) { & $onSubmit $text.Text }
}

function Queue-ModelChange([string]$model) {
    if ($script:availableModels -notcontains $model) { Show-GuiNotice 'That model is no longer available. Refresh the connected provider list.'; return }
    if ($Preview -or $script:suppressSelectionEvents -or [string]::IsNullOrWhiteSpace($model)) { return }
    if ($script:modelRequestPending -or -not (Can-QueueAction)) { return }
    $script:selectedModel = $model
    $script:preferredModel = $model
    $script:modelRequestPending = $true
    $script:queue.Enqueue(@{ action='model'; session_id=(Get-SelectedSessionId); model=$model })
    $controls.Send.IsEnabled = $false
    $controls.ModelPicker.IsEnabled = $false
    $controls.EffortPicker.IsEnabled = $false
    Show-GuiNotice "Switching model to $model…" 20
}

function Queue-EffortChange([string]$effort) {
    if ($Preview -or $script:suppressSelectionEvents -or [string]::IsNullOrWhiteSpace($effort)) { return }
    if ($script:effortRequestPending -or -not (Can-QueueAction)) { return }
    $script:selectedEffort = $effort
    $script:preferredEffort = $effort
    $script:effortRequestPending = $true
    $script:queue.Enqueue(@{ action='effort'; session_id=(Get-SelectedSessionId); effort=$effort })
    $controls.Send.IsEnabled = $false
    $controls.ModelPicker.IsEnabled = $false
    $controls.EffortPicker.IsEnabled = $false
    Show-GuiNotice "Updating reasoning effort to $effort…" 20
}

function Queue-Capabilities([string]$sessionId) {
    if ($Preview -or $script:capabilitiesRequestPending -or -not (Can-QueueAction)) { return }
    $script:capabilitiesRequestPending = $true
    $script:queue.Enqueue(@{ action='capabilities'; session_id=$sessionId })
    Show-GuiNotice 'Loading tools and skills…' 20
}

function Open-SelectedWorkspace {
    $selected = Get-SelectedSessionId
    $row = @($script:allSessions | Where-Object { $_.id -eq $selected } | Select-Object -First 1)
    $path = if ($row.Count -gt 0) { [string]$row[0].working_dir } else { '' }
    if ([string]::IsNullOrWhiteSpace($path) -or -not (Test-Path -LiteralPath $path -PathType Container)) {
        Show-GuiNotice 'No accessible workspace folder is attached to this chat'
        return
    }
    Start-Process -FilePath 'explorer.exe' -ArgumentList @("`"$path`"") | Out-Null
    Show-GuiNotice "Opened workspace folder · $path"
}

function Queue-Dictation {
    if (-not $script:dictationAvailable) {
        Show-GuiNotice 'Voice input needs [dictation].command in your AlphaCode config'
        return
    }
    if (-not (Can-QueueAction)) { return }
    $script:queue.Enqueue(@{ action='dictate' })
    $controls.Voice.IsEnabled = $false
    $controls.FooterStatus.Text = 'Listening… complete the configured dictation command'
}

$controls.Prompt.Add_TextChanged({ $controls.Placeholder.Visibility = if ($controls.Prompt.Text.Length) { 'Collapsed' } else { 'Visible' }; Set-TextDirection $controls.Prompt $controls.Prompt.Text; Update-ComposerState })
$controls.JumpLatest = $window.FindName('JumpLatest')
$controls.JumpLatest.Add_Click({ $controls.ReplyScroll.ScrollToEnd() })
$controls.ReplyScroll.Add_ScrollChanged({
    $controls.JumpLatest.Visibility = if ($controls.ReplyScroll.Visibility -eq 'Visible' -and $controls.ReplyScroll.ScrollableHeight -gt 0 -and $controls.ReplyScroll.VerticalOffset -lt ($controls.ReplyScroll.ScrollableHeight - 8)) { 'Visible' } else { 'Collapsed' }
})
# Enter sends the prompt. The handler works for every chat, including a live
# terminal session that is selected in the picker; Send-Prompt routes a prompt
# to whichever action the selection requires (owned chat vs. live session).
function Set-PromptFromExample([string]$text) {
    if (-not $script:sidebarVisible) { Set-SidebarVisibility $true }
    $controls.Prompt.Text = $text
    $controls.Prompt.Focus() | Out-Null
    $controls.Prompt.CaretIndex = $controls.Prompt.Text.Length
}
function Set-ReadingSize([double]$size, [bool]$persist = $true) {
    if ([double]::IsNaN($size) -or [double]::IsInfinity($size)) { $size = 14 }
    $size = [Math]::Max(12, [Math]::Min(24, $size))
    $controls.Reply.FontSize = $size
    $controls.Prompt.FontSize = $size + 1
    $controls.Placeholder.FontSize = $size + 1
    $controls.CommandInput.FontSize = $size + 1
    $controls.SecretInput.FontSize = $size + 1
    foreach ($workText in @($controls.WorkflowTodos,$controls.WorkflowPlan,$controls.WorkflowSwarm)) { $workText.FontSize = $size }
    Render-Conversation $true
    if ($persist) {
        Save-GuiSettings
        Show-GuiNotice "Reading size: $size pt · Ctrl+plus / Ctrl+minus · Ctrl+0 to reset"
    }
}
function Insert-ComposerNewline($textBox) {
    $start = $textBox.SelectionStart
    $textBox.SelectedText = [Environment]::NewLine
    $textBox.CaretIndex = $start + [Environment]::NewLine.Length
    $textBox.SelectionLength = 0
}
$controls.Prompt.Add_PreviewKeyDown({
    param($sender,$event)
    $modifiers = [Windows.Input.Keyboard]::Modifiers
    # Alt+Enter is a newline in the terminal composer too. WPF reports Alt
    # combinations as System keys; consume it before it can activate a menu.
    if (($event.Key -eq 'Return' -or $event.SystemKey -eq 'Return') -and ($modifiers -band [Windows.Input.ModifierKeys]::Alt)) {
        Insert-ComposerNewline $sender
        $event.Handled = $true
    } elseif ($event.Key -eq 'Return' -and -not ($modifiers -band [Windows.Input.ModifierKeys]::Shift)) {
        Send-Prompt; $event.Handled = $true
    }
})
foreach ($inputControl in @($controls.CommandInput, $controls.SecretInput)) {
    $inputControl.Add_PreviewKeyDown({
        param($sender,$event)
        if ($event.Key -eq 'Return' -and -not ([Windows.Input.Keyboard]::Modifiers -band [Windows.Input.ModifierKeys]::Shift)) {
            Send-Prompt; $event.Handled = $true
        }
    })
}
$controls.ExampleExplore.Add_Click({ Set-PromptFromExample 'Explore this workspace and explain its structure and how to get started.' })
$controls.ExampleReview.Add_Click({ Set-PromptFromExample 'Review my current changes and identify any bugs or risks.' })
$controls.ExampleTest.Add_Click({ Set-PromptFromExample 'Find and run the appropriate tests for this project, then explain the results.' })
$controls.Send.Add_Click({ Send-Prompt })
$controls.Voice.Add_Click({ Queue-Dictation })
$controls.ModelPicker.Add_SelectionChanged({
    if (-not $script:suppressSelectionEvents -and $null -ne $controls.ModelPicker.SelectedItem) {
        Queue-ModelChange ([string]$controls.ModelPicker.SelectedItem)
    }
})
$controls.EffortPicker.Add_SelectionChanged({
    param($sender, $event)
    # SelectedValue can still contain the previous item during this event.
    if (-not $script:suppressSelectionEvents -and $event.AddedItems.Count -gt 0) {
        Queue-EffortChange ([string]$event.AddedItems[0].Tag)
    }
})
$controls.Copy.Add_Click({
    if ($controls.Reply.Text) {
        [Windows.Clipboard]::SetText([string]$controls.Reply.Text)
        Show-GuiNotice 'Response copied to clipboard  ·  ready for your next prompt'
    } else {
        Show-GuiNotice 'There is no response to copy yet'
    }
})
# The native common item dialog is available on the bundled Framework host.
function Initialize-NativeWorkspacePicker {
    if ('AlphaCode.Desktop.WorkspacePicker' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
namespace AlphaCode.Desktop {
 public static class WorkspacePicker {
  [ComImport, Guid("D57C7288-D4AD-4768-BE02-9D969532D960"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  private interface IFileOpenDialog {
   [PreserveSig] int Show(IntPtr owner);
   void SetFileTypes(uint count, IntPtr filters);
   void SetFileTypeIndex(uint index); void GetFileTypeIndex(out uint index);
   void Advise(IntPtr events, out uint cookie); void Unadvise(uint cookie);
   void SetOptions(uint options); void GetOptions(out uint options);
   void SetDefaultFolder(IShellItem folder); void SetFolder(IShellItem folder);
   void GetFolder(out IShellItem folder); void GetCurrentSelection(out IShellItem item);
   void SetFileName([MarshalAs(UnmanagedType.LPWStr)] string name);
   void GetFileName([MarshalAs(UnmanagedType.LPWStr)] out string name);
   void SetTitle([MarshalAs(UnmanagedType.LPWStr)] string title);
   void SetOkButtonLabel([MarshalAs(UnmanagedType.LPWStr)] string label);
   void SetFileNameLabel([MarshalAs(UnmanagedType.LPWStr)] string label);
   void GetResult(out IShellItem item); void AddPlace(IShellItem item, int alignment);
   void SetDefaultExtension([MarshalAs(UnmanagedType.LPWStr)] string extension);
   void Close(int result); void SetClientGuid(ref Guid guid); void ClearClientData(); void SetFilter(IntPtr filter);
   void GetResults(out IntPtr items); void GetSelectedItems(out IntPtr items);
  }
  [ComImport, Guid("43826D1E-E718-42EE-BC55-A1E261C37BFE"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  private interface IShellItem {
   void BindToHandler(IntPtr context, ref Guid handler, ref Guid iid, out IntPtr result);
   void GetParent(out IShellItem parent); void GetDisplayName(uint kind, out IntPtr name);
   void GetAttributes(uint mask, out uint attributes); void Compare(IShellItem other, uint hint, out int order);
  }
  [DllImport("shell32.dll", CharSet=CharSet.Unicode, PreserveSig=true)]
  private static extern int SHCreateItemFromParsingName(string path, IntPtr context, ref Guid iid, out IShellItem item);
  private static void Release(object value) { if (value != null && Marshal.IsComObject(value)) Marshal.ReleaseComObject(value); }
  private static IShellItem Folder(string path) {
   IShellItem item; var iid = typeof(IShellItem).GUID;
   Marshal.ThrowExceptionForHR(SHCreateItemFromParsingName(Path.GetFullPath(path), IntPtr.Zero, ref iid, out item)); return item;
  }
  private static string PathOf(IShellItem item) {
   IntPtr name = IntPtr.Zero;
   try { item.GetDisplayName(0x80058000, out name); return Marshal.PtrToStringUni(name); }
   finally { if (name != IntPtr.Zero) Marshal.FreeCoTaskMem(name); }
  }
  private static IFileOpenDialog Create(string title, string initial) {
   var dialog = (IFileOpenDialog)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("DC1C5A9C-E88A-4DDE-A5A1-60F82A20AEF7")));
   try {
    uint options; dialog.GetOptions(out options);
    // PICKFOLDERS | FORCEFILESYSTEM | PATHMUSTEXIST, single selection.
    dialog.SetOptions((options | 0x20u | 0x40u | 0x800u) & ~0x200u);
    dialog.SetTitle(title); dialog.SetOkButtonLabel("Use workspace"); dialog.SetFileNameLabel("Folder");
    var client = new Guid("C3E49B36-2437-49B5-9C8A-0B9B22D100C9"); dialog.SetClientGuid(ref client);
    if (!String.IsNullOrWhiteSpace(initial) && Directory.Exists(initial)) {
     var folder = Folder(initial); try { dialog.SetFolder(folder); } finally { Release(folder); }
    }
    return dialog;
   } catch { Release(dialog); throw; }
  }
  private delegate bool EnumWindow(IntPtr window, IntPtr state);
  [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] private static extern bool EnumThreadWindows(uint thread, EnumWindow callback, IntPtr state);
  [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr window, uint command);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int capacity);
  [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
  public static string Show(IntPtr owner, string title, string initial) { return ShowCore(owner, title, initial, 0); }
  private static string ShowCore(IntPtr owner, string title, string initial, uint cancelAfter) {
   var dialog = Create(title, initial);
   System.Threading.Timer cancelTimer = null;
   if (cancelAfter > 0) {
    uint thread = GetCurrentThreadId();
    // Test-only cancellation targets our own modal window by owner, thread,
    // and exact title. The production Show path does not create this timer.
    cancelTimer = new System.Threading.Timer(delegate {
     EnumThreadWindows(thread, delegate(IntPtr candidate, IntPtr state) {
      var text = new System.Text.StringBuilder(256); GetWindowText(candidate, text, text.Capacity);
      if (GetWindow(candidate, 4) == owner && text.ToString() == title) { PostMessage(candidate, 0x0010, IntPtr.Zero, IntPtr.Zero); return false; }
      return true;
     }, IntPtr.Zero);
    }, null, cancelAfter, cancelAfter);
   }
   try {
    int result = dialog.Show(owner);
    if (result == unchecked((int)0x800704C7)) return null;
    Marshal.ThrowExceptionForHR(result);
    IShellItem item; dialog.GetResult(out item);
    try {
     var path = PathOf(item);
     if (String.IsNullOrWhiteSpace(path) || !Directory.Exists(path)) throw new IOException("The selected folder is no longer available.");
     return path;
    } finally { Release(item); }
   } finally { if (cancelTimer != null) cancelTimer.Dispose(); Release(dialog); }
  }
  public static bool ProbeCancel(IntPtr owner, string initial) { return ShowCore(owner, "AlphaCode workspace picker test", initial, 800) == null; }
  // Noninteractive probes use the same native object and options as Show.
  public static uint Probe(string initial) {
   var dialog = Create("AlphaCode workspace", initial);
   try { uint options; dialog.GetOptions(out options); return options; } finally { Release(dialog); }
  }
  public static string ResolveFolder(string path) {
   var folder = Folder(path); try { return PathOf(folder); } finally { Release(folder); }
  }
 }
}
'@
}
function Select-AlphacodeWorkspace([string]$Description) {
    try {
        Initialize-NativeWorkspacePicker
        $selected = Get-SelectedSessionRow
        $initial = if ($selected -and $selected.working_dir -and [IO.Directory]::Exists([string]$selected.working_dir)) { [string]$selected.working_dir } else { [Environment]::CurrentDirectory }
        $pickerWindow = if ($script:settingsWindow -and $script:settingsWindow.IsVisible) { $script:settingsWindow } else { $window }
        $owner = [Windows.Interop.WindowInteropHelper]::new($pickerWindow).Handle
        return [AlphaCode.Desktop.WorkspacePicker]::Show($owner, $Description, $initial)
    } catch {
        Show-GuiNotice ("Could not open the workspace picker: " + $_.Exception.GetBaseException().Message) 12
        return $null
    }
}

$controls.AddContext.Add_Click({
    try {
        if ($script:ownedBusy) { Show-GuiNotice 'Finish or cancel the current GUI turn first'; return }
        $selectedPath = $null
        try {
            $selectedPath = Select-AlphacodeWorkspace 'Choose a workspace for a new AlphaCode chat'
        } catch {
            Show-GuiNotice ("Could not open the workspace picker: " + $_.Exception.Message) 12
            return
        }
        if ($selectedPath) {
            try { $controls.Sessions.SelectedValue = '__companion__' } catch {}
            try { Queue-Action 'new_chat' '__companion__' @{ working_dir = $selectedPath } } catch {
                Show-GuiNotice ("Could not start the workspace chat: " + $_.Exception.Message) 12
                return
            }
            Show-GuiNotice "Opening workspace: $selectedPath"
        }
    } catch {
        try { Show-GuiNotice ("Workspace picker error: " + $_.Exception.Message) 12 } catch {}
    }
})
$controls.OpenTerminal.Add_Click({
    try {
        $selected = @($script:allSessions | Where-Object { $_.id -eq (Get-SelectedSessionId) }) | Select-Object -First 1
        $directory = if ($selected -and $selected.working_dir -and (Test-Path -LiteralPath $selected.working_dir -PathType Container)) { [string]$selected.working_dir } else { [Environment]::CurrentDirectory }
        $sessionId = if ($selected.id -eq '__companion__') { [string]$script:snapshot.owned_session_id } else { [string]$selected.id }
        # Only pass canonical native IDs as arguments. Imported session IDs can
        # refer to another application's store and need the terminal picker.
        if ($sessionId -match '^[a-zA-Z0-9][a-zA-Z0-9_.:-]*$' -and ($selected.state -ne 'stored' -or $selected.source -eq 'alphacode')) {
            Start-Process -FilePath $Executable -WorkingDirectory $directory -ArgumentList @('--resume', $sessionId)
            Show-GuiNotice 'Opened this chat in a terminal'
        } else {
            Start-Process -FilePath $Executable -WorkingDirectory $directory
            Show-GuiNotice 'Opened AlphaCode in a terminal'
        }
    } catch { Show-GuiNotice "Could not open terminal: $($_.Exception.Message)" }
})
$controls.Refresh.Add_Click({
    $script:lastPoll = [DateTime]::MinValue; $script:lastStoredPoll = [DateTime]::MinValue; $script:lastTranscriptPoll = [DateTime]::MinValue
    $script:lastSignature = ''; $script:projectsListSignature = $null
    if (-not $Preview) { Queue-Action 'reconnect' '__companion__' }
    Show-GuiNotice 'Refreshing workspace and connection…' 20
})
function Start-NewChat {
    if ($script:ownedBusy) {
        Show-GuiNotice 'Finish or cancel the current GUI turn first'
        return
    }
    if (-not (Can-QueueAction)) { return }
    Clear-GuiNotice
    # The companion chat only appears in the unfiltered Home list; leaving the
    # picker on Archive/Recents would hide the chat the user just started.
    $controls.SessionSearch.Clear()
    Set-Navigation 'Home'
    if (@($controls.Sessions.ItemsSource | ForEach-Object { $_.id }) -contains '__companion__') {
        $controls.Sessions.SelectedValue = '__companion__'
    }
    $controls.SessionSearch.Clear()
    $controls.Prompt.Clear(); $controls.CommandInput.Clear(); $controls.SecretInput.Clear(); $controls.Prompt.Focus()
    $script:lastSignature = ''
    $script:transcript = ''; $script:transcriptFor = ''
    Queue-Action 'new_chat' '__companion__'
    $controls.WorkspaceTitle.Text = 'New chat'
    $controls.WorkspaceSubtitle.Text = 'Starting a fresh chat...'
}
$controls.NewChat.Add_Click({ Start-NewChat })
# Single owner of the sidebar geometry. Collapsing the column to a fixed 1px
# rule (rather than 0) keeps the rail/surface divider visible and avoids a
# rounding-dependent 0.5px sliver; the width itself is remembered so a resize
# while collapsed cannot become the restore value.
function Set-SidebarVisibility([bool]$visible, [bool]$persist = $true) {
    if ($persist) { $script:sidebarAutoHidden = $false }
    if (-not $visible -and $script:sidebarVisible -and $controls.SidebarColumn.ActualWidth -gt 60) {
        $script:sidebarWidth = [double]$controls.SidebarColumn.ActualWidth
    }
    $script:sidebarVisible = $visible
    $controls.SidebarColumn.Width = if ($visible) { [Windows.GridLength]::new($script:sidebarWidth) } else { [Windows.GridLength]::new(0) }
    $controls.SidebarSurface.Visibility = if ($visible) { 'Visible' } else { 'Collapsed' }
    $controls.SidebarDivider.Visibility = if ($visible) { 'Visible' } else { 'Collapsed' }
    $controls.SidebarToggle.Content = if ($visible) { 'Hide sidebar' } else { 'Show sidebar' }
    Update-SidebarChrome
    $controls.SidebarHint.Text = if ($visible) { 'Sidebar visible · Ctrl+K jumps to chats' } else { 'Sidebar hidden · use the rail to navigate' }
    $controls.FooterStatus.Text = if ($visible) { 'Sidebar restored' } else { 'Sidebar hidden · Ctrl+B or the rail restores it' }
    if ($persist) { Save-GuiSettings }
}
function Set-Navigation([string]$section, [bool]$record = $true) {
    if ($record -and $script:navigation -ne $section) {
        $script:navigationBack.Push($script:navigation)
        $script:navigationForward.Clear()
    }
    $script:navigation = $section
    $controls.Back.IsEnabled = $script:navigationBack.Count -gt 0
    $controls.Forward.IsEnabled = $script:navigationForward.Count -gt 0
    foreach ($item in @($controls.NavRecents,$controls.NavProjects,$controls.NavArchive)) {
        $item.SetValue([Windows.Automation.AutomationProperties]::ItemStatusProperty, '')
        $item.Background = (Get-GuiBrush 'RailBg')
        $item.Foreground = (Get-GuiBrush 'Muted')
        $item.BorderBrush = [Windows.Media.Brushes]::Transparent
        $item.BorderThickness = [Windows.Thickness]::new(2,0,0,0)
    }
    $active = switch ($section) { 'Live' { $controls.NavRecents } 'Recents' { $controls.NavRecents } 'Projects' { $controls.NavProjects } 'Archive' { $controls.NavArchive } default { $null } }
    if ($active) {
        $active.Background = Get-GuiBrush 'AccentSoft'
        $active.Foreground = Get-GuiBrush 'Accent'
        $active.BorderBrush = Get-GuiBrush 'Accent'
        $active.SetValue([Windows.Automation.AutomationProperties]::ItemStatusProperty, 'Selected')
    }
    Update-SidebarChrome
    $controls.FooterStatus.Text = switch ($section) {
        'Live' { 'Live chats · connected workspaces' }
        'Recents' { 'Recent workspaces  ·  live local sessions stay in sync' }
        'Projects' { 'Workspace chats  ·  sessions with a known workspace folder' }
        'Archive' { 'Saved chats  ·  keep important work easy to revisit' }
        default { 'All chats · Ctrl+K to search · Enter to open' }
    }
    if ($record) { Clear-GuiNotice }
    Apply-Navigation
}
# The picker's SelectedValue is the current target. Recent/Archive filters can
# legitimately empty the list, so fall back to the remembered id instead of
# dropping the selection entirely (which disabled Cancel/Rename/Archive and the
# composer for a chat that is still live).
function Get-SelectedSessionId {
    $selected = [string]$controls.Sessions.SelectedValue
    if (-not [string]::IsNullOrWhiteSpace($selected)) { return $selected }
    if (-not [string]::IsNullOrWhiteSpace($script:lastSelectedSessionId)) { return $script:lastSelectedSessionId }
    return ''
}

function Get-SelectedSessionRow {
    $id = Get-SelectedSessionId
    if ([string]::IsNullOrWhiteSpace($id)) { return $null }
    return @($script:allSessions | Where-Object { $_.id -eq $id }) | Select-Object -First 1
}

$controls.Cancel.Add_Click({
    $selected = Get-SelectedSessionId
    if ([string]::IsNullOrWhiteSpace($selected)) { Show-GuiNotice 'Select a chat to cancel first'; return }
    $current = Get-SelectedSessionRow
    if ($selected -ne '__companion__' -and [string]$current.state -ne 'working') {
        Show-GuiNotice 'That workspace is not running a turn'
        return
    }
    Clear-GuiNotice
    Queue-Action 'cancel' $selected
    $controls.FooterStatus.Text = 'Cancelling the running turn...'
})

# Rename and Archive both need a value, so they open a small labeled input
# dialog and queue the request with that value on submit.
$controls.Rename.Add_Click({
    $selected = Get-SelectedSessionId
    if ([string]::IsNullOrWhiteSpace($selected)) { Show-GuiNotice 'Select a chat to rename first'; return }
    Clear-GuiNotice
    $current = Get-SelectedSessionRow
    Show-ValueDialog 'Rename chat' 'New title (leave empty to restore the generated title)' ([string]$current.title) {
        param($value)
        Queue-Action 'rename' $selected @{ title = $value }
        $controls.FooterStatus.Text = 'Renaming chat...'
    }
})

$controls.Archive.Add_Click({
    $selected = Get-SelectedSessionId
    if ([string]::IsNullOrWhiteSpace($selected)) { Show-GuiNotice 'Select a chat to save first'; return }
    Clear-GuiNotice
    $current = Get-SelectedSessionRow
    if ([bool]$current.saved) {
        Queue-Action 'archive' $selected @{ saved = $false }
        $controls.FooterStatus.Text = 'Removing from saved chats...'
        return
    }
    Show-ValueDialog 'Save chat' 'Optional label for this saved chat' '' {
        param($value)
        Queue-Action 'archive' $selected @{ saved = $true; title = $value }
        $controls.FooterStatus.Text = 'Saving chat...'
    }
})

$controls.Back.Add_Click({
    if ($script:navigationBack.Count -eq 0) { return }
    $script:navigationForward.Push($script:navigation)
    Set-Navigation ([string]$script:navigationBack.Pop()) $false
    $controls.Back.IsEnabled = $script:navigationBack.Count -gt 0
    $controls.Forward.IsEnabled = $script:navigationForward.Count -gt 0
})
$controls.Forward.Add_Click({
    if ($script:navigationForward.Count -eq 0) { return }
    $script:navigationBack.Push($script:navigation)
    Set-Navigation ([string]$script:navigationForward.Pop()) $false
    $controls.Back.IsEnabled = $script:navigationBack.Count -gt 0
    $controls.Forward.IsEnabled = $script:navigationForward.Count -gt 0
})
$controls.SidebarToggle.Add_Click({ Set-SidebarVisibility (-not $script:sidebarVisible) })
$controls.NavCompanion.Add_Click({ $controls.SessionSearch.Clear(); Set-Navigation 'Home'; $controls.Sessions.SelectedValue = '__companion__'; Focus-SelectedChat })
$controls.More.Add_Click({
    $menu.PlacementTarget = $controls.More
    $menu.IsOpen = $true
})
$controls.Search.Add_Click({
    if (-not $script:sidebarVisible) { Set-SidebarVisibility $true }
    $controls.SessionSearch.Focus() | Out-Null
    Show-GuiNotice 'Search ready · filter live and saved chat titles'
})
$controls.SessionSearch.Add_TextChanged({
    $controls.SearchPlaceholder.Visibility = if ($controls.SessionSearch.Text.Length) { 'Collapsed' } else { 'Visible' }
    $controls.ClearChatSearch.Visibility = if ($controls.SessionSearch.Text.Length) { 'Visible' } else { 'Collapsed' }
    Update-ProjectsList
    Apply-Navigation
})
$controls.ClearChatSearch.Add_Click({ $controls.SessionSearch.Clear(); $controls.SessionSearch.Focus() | Out-Null })
$controls.FilterAll.Add_Click({ Set-Navigation 'Home' })
$controls.FilterLive.Add_Click({ Set-Navigation 'Live' })
$controls.FilterSaved.Add_Click({ Set-Navigation 'Archive' })
$controls.SessionSearch.Add_PreviewKeyDown({ param($sender,$event)
    if ($event.Key -eq 'Return') { Select-ChatSearchResult $(if ($controls.Sessions.SelectedIndex -ge 0) { $controls.Sessions.SelectedIndex } else { 0 }); $event.Handled = $true }
    elseif ($event.Key -eq 'Down') { Select-ChatSearchResult 0 $false; $event.Handled = $true }
    elseif ($event.Key -eq 'Up') { Select-ChatSearchResult ($controls.Sessions.Items.Count-1) $false; $event.Handled = $true }
})
$controls.Sessions.Add_PreviewKeyDown({ param($sender,$event) if ($event.Key -eq 'Return') { Focus-SelectedChat; $event.Handled = $true } })
$controls.ModelChooser.Add_Click({ Open-ModelChooser })
$controls.ModelSearch.Add_TextChanged({ Update-ModelChooser })
$controls.ModelResults.Add_SelectionChanged({ $controls.ApplyModelChoice.IsEnabled = $null -ne $controls.ModelResults.SelectedValue -and (Can-QueueAction) })
$controls.ModelResults.Add_MouseDoubleClick({ Apply-ModelChoice })
$controls.ApplyModelChoice.Add_Click({ Apply-ModelChoice })
$controls.ModelPopup.Add_PreviewKeyDown({ param($sender,$event)
    if ($event.Key -eq 'Escape') { $controls.ModelPopup.IsOpen = $false; $controls.ModelChooser.Focus() | Out-Null; $event.Handled = $true }
    elseif ($event.Key -eq 'Return') { Apply-ModelChoice; $event.Handled = $true }
    elseif ($event.Key -eq 'Down' -and $controls.ModelSearch.IsKeyboardFocusWithin -and $controls.ModelResults.Items.Count) { $controls.ModelResults.Focus() | Out-Null; $controls.ModelResults.SelectedIndex = 0; $event.Handled = $true }
})
$controls.ResponseStop.Add_Click({ if ($controls.Cancel.IsEnabled) { $controls.Cancel.RaiseEvent([Windows.RoutedEventArgs]::new([Windows.Controls.Primitives.ButtonBase]::ClickEvent)) } })
$controls.ComposerSettings.Add_Click({ Show-Settings })
$controls.OpenSettings.Add_Click({ Show-Settings })
$controls.NavHome.Add_Click({ Set-SidebarVisibility (-not $script:sidebarVisible) })
$controls.NavRecents.Add_Click({ Set-SidebarVisibility $true; Set-Navigation 'Live' })
$controls.NavProjects.Add_Click({ Set-SidebarVisibility $true; Set-Navigation 'Projects' })
$controls.NavArchive.Add_Click({ Set-SidebarVisibility $true; Set-Navigation 'Archive' })
$window.Add_PreviewKeyDown({
    param($sender,$event)
    $modifiers = [Windows.Input.Keyboard]::Modifiers
    if ($controls.CommandOverlay.Visibility -eq 'Visible') {
        if ($event.Key -eq 'Escape') { Close-CommandPalette; $event.Handled = $true }
        elseif ($event.Key -eq 'Enter') { Invoke-SelectedCommand; $event.Handled = $true }
        elseif ($event.Key -in @('Down','Up')) {
            $count = $controls.CommandResults.Items.Count
            if ($count -gt 0) {
                $step = if ($event.Key -eq 'Down') { 1 } else { -1 }
                $controls.CommandResults.SelectedIndex = ($controls.CommandResults.SelectedIndex + $step + $count) % $count
                $controls.CommandResults.ScrollIntoView($controls.CommandResults.SelectedItem)
            }
            $event.Handled = $true
        }
        return
    }
    if ($event.Key -eq 'F' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control) -and ($modifiers -band [Windows.Input.ModifierKeys]::Shift)) { Toggle-FocusMode; $event.Handled = $true; return }
    if ($event.Key -eq 'P' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control) -and ($modifiers -band [Windows.Input.ModifierKeys]::Shift)) { Open-CommandPalette; $event.Handled = $true; return }
    if ($event.Key -eq 'Escape' -and $controls.ModelPopup.IsOpen) { $controls.ModelPopup.IsOpen = $false; $controls.ModelChooser.Focus() | Out-Null; $event.Handled = $true; return }
    if ($event.Key -eq 'Escape' -and $controls.ReplySearchBar.Visibility -eq 'Visible') { Close-ConversationSearch; $event.Handled = $true; return }
    if ($modifiers -band [Windows.Input.ModifierKeys]::Control) {
        if ($event.Key -eq 'F') { Open-ConversationSearch; $event.Handled = $true; return }
        if ($event.Key -eq 'J') { Set-WorkflowVisibility (-not $script:workflowVisible); $event.Handled = $true; return }
        if ($event.Key -eq 'M') { Open-ModelChooser; $event.Handled = $true; return }
    }
    if (($modifiers -band [Windows.Input.ModifierKeys]::Control) -and $event.Key -in @('OemPlus','Add','OemMinus','Subtract','D0','NumPad0')) {
        $size = switch ([string]$event.Key) { { $_ -in @('OemPlus','Add') } { $controls.Reply.FontSize + 1 } { $_ -in @('OemMinus','Subtract') } { $controls.Reply.FontSize - 1 } default { 14 } }
        Set-ReadingSize $size
        $event.Handled = $true
    } elseif ($event.Key -eq 'End' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control) -and $controls.ConversationCards.IsKeyboardFocusWithin) {
        $controls.ReplyScroll.ScrollToEnd()
        $event.Handled = $true
    } elseif ($event.Key -eq 'N' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control)) {
        Start-NewChat
        $event.Handled = $true
    } elseif ($event.Key -eq 'Escape' -and $controls.SessionSearch.IsKeyboardFocusWithin) {
        $controls.SessionSearch.Clear()
        if ($script:passwordMode) { $controls.SecretInput.Focus() | Out-Null } elseif ($script:inputMode) { $controls.CommandInput.Focus() | Out-Null } else { $controls.Prompt.Focus() | Out-Null }
        $event.Handled = $true
    } elseif ($event.Key -eq 'K' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control)) {
        if (-not $script:sidebarVisible) { Set-SidebarVisibility $true }
        $controls.SessionSearch.Focus() | Out-Null
        $event.Handled = $true
    } elseif ($event.Key -eq 'B' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control)) {
        # Same control the header button drives, so the rail stays truthful
        # about the sidebar state no matter which affordance was used.
        Set-SidebarVisibility (-not $script:sidebarVisible)
        $event.Handled = $true
    } elseif ($event.Key -eq 'L' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control)) {
        if ($script:passwordMode) { $controls.SecretInput.Focus() | Out-Null } elseif ($script:inputMode) { $controls.CommandInput.Focus() | Out-Null } else { $controls.Prompt.Focus() | Out-Null }
        $event.Handled = $true
    } elseif ($event.Key -eq 'C' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control) -and ($modifiers -band [Windows.Input.ModifierKeys]::Shift)) {
        if ($controls.Copy.IsEnabled -and $controls.Reply.Text) {
            [Windows.Clipboard]::SetText([string]$controls.Reply.Text)
            Show-GuiNotice 'Response copied to clipboard  ·  ready for your next prompt'
        }
        $event.Handled = $true
    } elseif ($event.Key -eq 'F5') {
        $script:lastPoll = [DateTime]::MinValue
        Show-GuiNotice 'Refreshing workspace…'
        $event.Handled = $true
    }
})
$controls.Minimize.Add_Click({ $window.WindowState = [Windows.WindowState]::Minimized })
$controls.Maximize.Add_Click({
    $window.WindowState = if ($window.WindowState -eq [Windows.WindowState]::Maximized) { [Windows.WindowState]::Normal } else { [Windows.WindowState]::Maximized }
})
$controls.Close.Add_Click({ $window.Close() })
$controls.TitleBar.Add_MouseLeftButtonDown({
    if ($_.OriginalSource -is [Windows.Controls.Button]) { return }
    if ($_.ClickCount -eq 2) {
        $window.WindowState = if ($window.WindowState -eq [Windows.WindowState]::Maximized) { [Windows.WindowState]::Normal } else { [Windows.WindowState]::Maximized }
    } else {
        $window.DragMove()
    }
})
$controls.Sessions.Add_SelectionChanged({
    param($sender, $event)
    if ($script:suppressSelectionEvents) { return }
    $script:userSelectedChat = $true
    # SelectedValue may still be the previous row during this routed event.
    if ($event.AddedItems.Count -gt 0) { Update-SelectedSession ([string]$event.AddedItems[0].id) }
})

$menu = New-Object Windows.Controls.ContextMenu
$pin = New-Object Windows.Controls.MenuItem
$pin.Header = 'Always on top'; $pin.IsCheckable = $true; $pin.IsChecked = $false
$pin.Add_Click({ $window.Topmost = $pin.IsChecked; Save-GuiSettings })
$menu.Items.Add($pin) | Out-Null
$settingsMenu = New-Object Windows.Controls.MenuItem
$settingsMenu.Header = 'Settings…'
$settingsMenu.Add_Click({ Show-Settings })
$menu.Items.Add($settingsMenu) | Out-Null
$quit = New-Object Windows.Controls.MenuItem
$quit.Header = 'Quit AlphaCode GUI'
$quit.Add_Click({
    if ($script:ownedBusy) {
        $answer = [Windows.MessageBox]::Show('AlphaCode chat is still working. Quit and disconnect it?', 'AlphaCode GUI', 'YesNo', 'Question')
        if ($answer -ne 'Yes') { return }
    }
    $window.Close()
})
$menu.Items.Add($quit) | Out-Null
$menu.Items.Add((New-Object Windows.Controls.Separator)) | Out-Null
$newChatMenu = New-Object Windows.Controls.MenuItem
$newChatMenu.Header = 'New chat'
$newChatMenu.Add_Click({ Start-NewChat })
$menu.Items.Add($newChatMenu) | Out-Null
$refreshMenu = New-Object Windows.Controls.MenuItem
$refreshMenu.Header = 'Refresh workspaces'
$refreshMenu.Add_Click({
    $script:lastPoll = [DateTime]::MinValue
    $script:lastStoredPoll = [DateTime]::MinValue
    $script:lastTranscriptPoll = [DateTime]::MinValue
    $controls.FooterStatus.Text = 'Refreshing workspaces…'
})
$menu.Items.Add($refreshMenu) | Out-Null
$capabilitiesMenu = New-Object Windows.Controls.MenuItem
$capabilitiesMenu.Header = 'Inspect tools and skills'
$capabilitiesMenu.Add_Click({ Queue-Capabilities ([string]$controls.Sessions.SelectedValue) })
$menu.Items.Add($capabilitiesMenu) | Out-Null
$folderMenu = New-Object Windows.Controls.MenuItem
$folderMenu.Header = 'Open workspace folder'
$folderMenu.Add_Click({ Open-SelectedWorkspace })
$menu.Items.Add($folderMenu) | Out-Null
$sidebarMenu = New-Object Windows.Controls.MenuItem
$sidebarMenu.Header = 'Toggle sidebar'
$sidebarMenu.Add_Click({ Set-SidebarVisibility (-not $script:sidebarVisible) })
$menu.Items.Add($sidebarMenu) | Out-Null
$largerTextMenu = New-Object Windows.Controls.MenuItem
$largerTextMenu.Header = 'Larger reading text (Ctrl+plus)'
$largerTextMenu.Add_Click({ Set-ReadingSize ($controls.Reply.FontSize + 1) })
$menu.Items.Add($largerTextMenu) | Out-Null
$smallerTextMenu = New-Object Windows.Controls.MenuItem
$smallerTextMenu.Header = 'Smaller reading text (Ctrl+minus)'
$smallerTextMenu.Add_Click({ Set-ReadingSize ($controls.Reply.FontSize - 1) })
$menu.Items.Add($smallerTextMenu) | Out-Null

$work = [Windows.SystemParameters]::WorkArea
$responseTimer = New-Object Windows.Threading.DispatcherTimer
$responseTimer.Interval = [TimeSpan]::FromSeconds(1)
$responseTimer.Add_Tick({ Invoke-GuiCallback 'ResponseTimer.Tick' { Update-ResponseElapsed } $responseTimer })
$window.MinWidth = [Math]::Min($window.MinWidth, $work.Width)
$window.MinHeight = [Math]::Min($window.MinHeight, $work.Height)
$window.Width = [Math]::Max($window.MinWidth, [Math]::Min($window.Width, $work.Width - 32))
$window.Height = [Math]::Max($window.MinHeight, [Math]::Min($window.Height, $work.Height - 32))
if ($window.Width -lt 1100) { Set-SidebarVisibility $false }
$window.Left = $work.Right - $window.Width - 24
$window.Top = $work.Bottom - $window.Height - 20
try {
    if (-not $Preview -and (Test-Path -LiteralPath $settingsPath)) {
        $saved = [IO.File]::ReadAllText($settingsPath) | ConvertFrom-Json
        if ($null -ne $saved.work_width) { $script:workflowWidth = [Math]::Max(260, [Math]::Min(480, [double]$saved.work_width)) }
        if ($null -ne $saved.work_visible) { Set-WorkflowVisibility ([bool]$saved.work_visible) }
        if ($saved.theme -in @('dark','light')) { $script:theme = [string]$saved.theme }
        if ($null -ne $saved.width -and [double]$saved.width -ge $window.MinWidth) { $window.Width = [Math]::Min([double]$saved.width, $work.Width) }
        if ($null -ne $saved.height -and [double]$saved.height -ge $window.MinHeight) { $window.Height = [Math]::Min([double]$saved.height, $work.Height) }
        if ($null -ne $saved.reading_size) { Set-ReadingSize ([double]$saved.reading_size) $false }
        if ([double]::IsNaN([double]$saved.left) -or [double]::IsNaN([double]$saved.top)) { throw 'Invalid saved position' }
        $window.Left = [Math]::Max($work.Left, [Math]::Min([double]$saved.left, $work.Right - $window.Width))
        $window.Top = [Math]::Max($work.Top, [Math]::Min([double]$saved.top, $work.Bottom - $window.Height))
        # Topmost is opt-in. Older builds persisted `topmost=true` by default,
        # so only honor the explicit opt-in key introduced by the desktop shell.
        $window.Topmost = [bool]$saved.topmost_opt_in; $pin.IsChecked = $window.Topmost
        $script:preferredModel = if ($null -ne $saved.model) { [string]$saved.model } else { '' }
        $script:preferredEffort = if ($null -ne $saved.effort) { [string]$saved.effort } else { 'none' }
        $script:selectedModel = $script:preferredModel
        $script:selectedEffort = $script:preferredEffort
        if ($null -ne $saved.sidebar_width -and [double]$saved.sidebar_width -ge 180 -and [double]$saved.sidebar_width -le 500) { $script:sidebarWidth = [double]$saved.sidebar_width }
        if ($null -ne $saved.sidebar_visible) { Set-SidebarVisibility ([bool]$saved.sidebar_visible) }
        # Launch into the full workspace even if the last window was compact.
    }
} catch { }

# Apply settings only after restoration; startup layout must not overwrite them.
Set-GuiTheme $script:theme $false
$script:initializing = $false
Set-Navigation 'Home'

$searchTimer = New-Object Windows.Threading.DispatcherTimer
$searchTimer.Interval = [TimeSpan]::FromMilliseconds(150)
$searchTimer.Add_Tick({ Invoke-GuiCallback 'SearchTimer.Tick' { $searchTimer.Stop(); Render-Conversation $true; Move-ConversationHit 1 } $searchTimer })
$timer = New-Object Windows.Threading.DispatcherTimer
$timer.Interval = [TimeSpan]::FromMilliseconds(100)
$timer.Add_Tick({
    if ($script:closed) { return }
    try {
        $timer.Interval = [TimeSpan]::FromMilliseconds($(if ($script:working -gt 0) { 100 } else { 400 }))
        if ($Preview) { return }
        if ($script:providersRefreshRequested -and -not $script:pending -and $script:queue.Count -eq 0) {
            $script:providersRefreshRequested = $false
            $script:queue.Enqueue(@{action='providers'})
        }
        if ($script:pending) {
            if ($script:pending.IsCompleted) {
                $line = $script:pending.GetAwaiter().GetResult()
                if ($null -eq $line) { throw 'The local GUI connection closed.' }
                $response = $line | ConvertFrom-Json
                if ($null -ne $response.providers) {
                    Set-ProviderCatalog $response.providers
                    if ($response.connection) { Set-ProviderProgress $response.connection }
                }
                elseif ($response.ok -and $response.connection) { Set-ProviderProgress $response.connection }
                elseif ($response.sessions) {
                    Set-Snapshot $response
                }
                elseif ($response.stored) {
                    Set-StoredSessions $response.stored
                }
                elseif ($response.ok -and $script:pendingKind -eq 'transcript') {
                    $payload = $response.transcript
                    if ($payload -and $payload.message_count -ne $null) {
                        $script:transcriptFor = [string]$response.session_id
                        $script:transcript = Format-Transcript $payload
                    } elseif ($payload -and $payload.error) {
                        $controls.FooterStatus.Text = [string]$payload.error
                    }
                    Update-SelectedSession
                }
                elseif ($response.ok -and $script:pendingKind -eq 'capabilities') {
                    $script:capabilitiesRequestPending = $false
                    $tools = @($response.tools)
                    $skills = @($response.skills.skills)
                    $mcp = @($response.mcp.servers)
                    $toolText = if ($tools.Count) { "$($tools.Count) tools" } else { 'no tools' }
                    $skillText = if ($skills.Count) { "$($skills.Count) skills" } else { 'no skills' }
                    $mcpText = if ($mcp.Count) { "$($mcp.Count) MCP servers" } else { 'no MCP servers' }
                    $controls.SidebarHint.Text = "Capabilities · $toolText · $skillText · $mcpText"
                    $controls.FooterStatus.Text = "Capabilities loaded · $toolText · $skillText · $mcpText"
                }
                elseif ($response.ok -and $script:pendingKind -eq 'dictate') {
                    $text = [string]$response.text
                    if ($text) {
                        switch ([string]$response.mode) {
                            'send' { $controls.Prompt.Text = $text; $script:dictationAutoSend = $true }
                            'replace' { $controls.Prompt.Text = $text }
                            default {
                                if ([string]::IsNullOrWhiteSpace($controls.Prompt.Text)) { $controls.Prompt.Text = $text } else { $controls.Prompt.Text = ($controls.Prompt.Text.TrimEnd() + ' ' + $text) }
                            }
                        }
                        $controls.FooterStatus.Text = 'Dictation inserted into the prompt'
                    }
                    $controls.Voice.IsEnabled = $true
                }
                elseif ($response.ok -and $script:pendingKind -eq 'cancel') {
                    $controls.FooterStatus.Text = if ($response.cancelled) { 'Cancel request sent to the session' } else { 'Nothing was running to cancel' }
                    $script:lastPoll = [DateTime]::MinValue
                }
                elseif ($response.ok -and $script:pendingKind -eq 'reconnect') {
                    $script:lastPoll = [DateTime]::MinValue
                    Show-GuiNotice 'Reconnecting to AlphaCode…' 20
                }
                elseif ($response.ok -and $script:pendingKind -eq 'new_chat') {
                    Show-GuiNotice 'New chat ready  ·  type a prompt to begin'
                    $controls.Prompt.Focus() | Out-Null
                    $script:lastPoll = [DateTime]::MinValue
                }
                elseif ($response.ok -and $script:pendingKind -eq 'archive') {
                    $verb = if ([bool]$response.saved) { 'Saved' } else { 'Unsaved' }
                    $controls.FooterStatus.Text = "$verb the chat"
                    $script:lastPoll = [DateTime]::MinValue
                    $script:lastStoredPoll = [DateTime]::MinValue
                }
                elseif ($response.ok -and $script:pendingKind -eq 'rename') {
                    $name = $response.detail.display_title
                    $controls.FooterStatus.Text = if ($name) { "Renamed the chat to '$name'" } else { 'Chat name updated' }
                    $script:lastPoll = [DateTime]::MinValue
                    $script:lastStoredPoll = [DateTime]::MinValue
                }
                elseif ($response.ok -and $script:pendingKind -eq 'model') {
                    $script:modelRequestPending = $false
                    $controls.ModelPicker.IsEnabled = $true
                    $controls.EffortPicker.IsEnabled = $true
                    Show-GuiNotice "Model active: $($response.model)"
                    $script:lastPoll = [DateTime]::MinValue
                } elseif ($response.ok -and $script:pendingKind -eq 'effort') {
                    $script:effortRequestPending = $false
                    $controls.ModelPicker.IsEnabled = $true
                    $controls.EffortPicker.IsEnabled = $true
                    Show-GuiNotice "Reasoning effort active: $($response.effort)"
                    $script:lastPoll = [DateTime]::MinValue
                } elseif ($response.ok -and $response.sent) {
                    if ($script:submittedInput) { $controls.CommandInput.Clear(); $controls.SecretInput.Clear() }
                    else {
                        if ($script:draftSession -eq $script:submittedSession -and $controls.Prompt.Text -eq $script:submittedPrompt) { $controls.Prompt.Clear() }
                        if ([string]$script:drafts[$script:submittedSession] -eq $script:submittedPrompt) { $script:drafts.Remove($script:submittedSession) }
                    }
                    $script:deliveryNotice = if ($script:submittedInput) { 'Command input delivered' } else { 'Prompt delivered' }
                    $script:submittedPrompt = ''
                    $script:submittedInput = $false
                    $script:noticeUntil = [DateTime]::UtcNow.AddSeconds(3)
                    $script:lastPoll = [DateTime]::MinValue
                } elseif (-not $response.ok) {
                    if ($script:pendingKind -in @('provider_connect','provider_input','provider_cancel')) { Set-ProviderProgress ([pscustomobject]@{phase='failed'; detail=[string]$response.error}) }
                    if ($script:pendingKind -eq 'model') { $script:modelRequestPending = $false; $script:preferredModel = ''; $script:lastPoll = [DateTime]::MinValue }
                    if ($script:pendingKind -eq 'effort') { $script:effortRequestPending = $false; $script:preferredEffort = ''; $script:lastPoll = [DateTime]::MinValue }
                    if ($script:pendingKind -eq 'capabilities') { $script:capabilitiesRequestPending = $false }
                    if ($script:pendingKind -eq 'dictate') { $controls.Voice.IsEnabled = -not $script:inputMode }
                    $controls.ModelPicker.IsEnabled = $true
                    $controls.EffortPicker.IsEnabled = $true
                    $script:submittedPrompt = ''
                    $script:pendingResponseSession = ''
                    $script:deliveryNotice = $response.error
                    $script:noticeUntil = [DateTime]::MaxValue
                }
                $script:pending = $null; $script:pendingKind = ''
                Update-SelectedSession
                if ($script:dictationAutoSend) {
                    $script:dictationAutoSend = $false
                    Send-Prompt
                }
            } elseif (([DateTime]::UtcNow - $script:requestStarted).TotalSeconds -gt $(if ($script:pendingKind -eq 'dictate') { 90 } elseif ($script:pendingKind -in @('prompt','input')) { 75 } else { 12 })) {
                throw 'Connection timed out. Check the session before resending; delivery may have succeeded.'
            }
            return
        }
        if ([DateTime]::UtcNow -lt $script:retryAt) { return }
        if (-not $script:process -or $script:process.HasExited) { Start-Bridge }
        $request = $null
        if ($script:queue.Count -gt 0) { $request = $script:queue.Dequeue() }
        elseif (([DateTime]::UtcNow - $script:lastTranscriptPoll).TotalSeconds -ge 4) {
            # Real conversation for the selected live session, so the desktop
            # shows the same chat the terminal does.
            $target = [string]$controls.Sessions.SelectedValue
            $targetRow = @($script:allSessions | Where-Object { $_.id -eq $target }) | Select-Object -First 1
            # Advance even when there is no eligible live chat. Otherwise this
            # branch starves status polling for a new or saved-only workspace.
            $script:lastTranscriptPoll = [DateTime]::UtcNow
            $canLoadStored = $targetRow -and [string]$targetRow.state -eq 'stored' -and [string]$targetRow.source -eq 'alphacode' -and $script:transcriptFor -ne $target
            if (-not [string]::IsNullOrWhiteSpace($target) -and $target -ne '__companion__' -and $targetRow -and ([string]$targetRow.state -ne 'stored' -or $canLoadStored)) {
                $request = @{ action='transcript'; session_id=$target }
            }
        }
        elseif (([DateTime]::UtcNow - $script:lastStoredPoll).TotalSeconds -ge 15) {
            # The on-disk session index is the expensive call (full directory
            # scan), so it refreshes on its own slower cadence than live status.
            $request = @{ action='sessions'; limit=400 }
            $script:lastStoredPoll = [DateTime]::UtcNow
        }
        elseif (([DateTime]::UtcNow - $script:lastPoll).TotalSeconds -ge $(if ($script:expanded) { 1 } else { 2 })) {
            $request = @{ action='status'; include_workflow=$script:workflowVisible }
            if ($script:expanded) { $request.session_id = Get-SelectedSessionId }
            $script:lastPoll = [DateTime]::UtcNow
        }
        if ($request) {
            $script:pendingKind = $request.action
            if ($request.action -in @('prompt','input')) { $script:submittedPrompt = $request.prompt; $script:submittedSession = [string]$request.session_id }
            $script:inputWriter.WriteLine(($request | ConvertTo-Json -Compress))
            $script:pending = $script:process.StandardOutput.ReadLineAsync()
            $script:requestStarted = [DateTime]::UtcNow
        }
    } catch {
        $failure = $_
        Write-GuiFailure 'bridge' 'MainTimer.Tick' $failure
        if ([AlphaCode.Desktop.GuiDiagnostics]::IsFatal($failure.Exception)) { throw }
        $script:responseDisconnected = $true
        $controls.StateText.Text = 'Reconnecting'
        $script:deliveryNotice = [AlphaCode.Desktop.GuiDiagnostics]::Redact($failure.Exception.Message,$null)
        $script:submittedPrompt = ''
        $script:noticeUntil = [DateTime]::MaxValue
        $controls.Detail.Text = $script:deliveryNotice
        $controls.NoticeText.Foreground = Get-GuiBrush 'Warning'
        $controls.NoticeBanner.Background = Get-GuiBrush 'WarningSoft'
        $controls.NoticeText.Text = $script:deliveryNotice
        $controls.NoticeBanner.Visibility = 'Visible'
        $controls.FooterStatus.Text = 'Connection interrupted · reconnecting in 5 seconds'
        $controls.Send.IsEnabled = $false
        $script:modelRequestPending = $false
        $script:effortRequestPending = $false
        $script:capabilitiesRequestPending = $false
        $controls.ModelPicker.IsEnabled = $true
        $controls.EffortPicker.IsEnabled = $true
        $script:pending = $null; $script:pendingKind = ''; $script:queue.Clear()
        if ($script:inputWriter) { try { $script:inputWriter.Dispose() } catch { Write-GuiFailure 'bridge' 'MainTimer.Reconnect.InputWriter' $_ }; $script:inputWriter = $null }
        if ($script:process) { try { $script:process.Kill() } catch { }; $script:process.Dispose(); $script:process = $null }
        $script:pendingResponseSession = ''
        Update-ResponseActivity
        $script:retryAt = [DateTime]::UtcNow.AddSeconds(5)
    }
})
$window.Add_Closing({
    param($sender, $event)
    if ($script:ownedBusy) {
        $answer = [Windows.MessageBox]::Show('AlphaCode is still working. Close this window and disconnect the chat?', 'Close AlphaCode', 'YesNo', 'Question')
        if ($answer -ne 'Yes') { $event.Cancel = $true }
    }
})
$window.Add_Closed({
    try { Save-GuiSettings } finally { Stop-GuiResources }
})

if ($Preview) {
    # Expose the shell in app switchers for accessibility/visual QA without
    # inventing conversations, projects, replies, or daemon activity.
    $window.ShowInTaskbar = $true
    Set-Snapshot ([pscustomobject]@{ owned_busy=$false; working=0; attention=0; offline=0; connected_servers=0; reply=''; input_request=$null; selected_session_id='__companion__'; selected_reply=''; activity=@(); model=''; provider=''; effort='none'; models=@(); sessions=@(
        [pscustomobject]@{ id='__companion__'; title='AlphaCode chat'; state='ready'; detail='Connect AlphaCode to start a live workspace'; saved=$false }
    ) })
}
$controls.Reply.Add_TextChanged({ Invoke-GuiCallback 'Reply.TextChanged' { Render-Conversation } })
$controls.FindReply.Add_Click({ Open-ConversationSearch })
$controls.CloseReplySearch.Add_Click({ Close-ConversationSearch })
$controls.ReplySearch.Add_TextChanged({ $searchTimer.Stop(); $searchTimer.Start() })
$controls.ReplySearch.Add_PreviewKeyDown({ param($sender,$event) if ($event.Key -eq 'Return') { $searchTimer.Stop(); if ($script:renderedQuery -ne $controls.ReplySearch.Text) { Render-Conversation $true }; Move-ConversationHit $(if ([Windows.Input.Keyboard]::Modifiers -band [Windows.Input.ModifierKeys]::Shift) { -1 } else { 1 }); $event.Handled = $true } })
$controls.WorkflowResize.Add_DragCompleted({ $script:workflowWidth = [Math]::Max(260,[Math]::Min(480,$controls.WorkColumn.ActualWidth)); Update-GuiLayout; Save-GuiSettings })
$controls.WorkToggle.Add_Click({ Set-WorkflowVisibility (-not $script:workflowVisible) })
$controls.ThemeToggle.Add_Click({ Set-GuiTheme $(if ($script:theme -eq 'dark') { 'light' } else { 'dark' }) })
$controls.FocusMode.Add_Click({ Toggle-FocusMode })
$controls.CommandMenu.Add_Click({ Open-CommandPalette })
$controls.CloseCommands.Add_Click({ Close-CommandPalette })
$controls.CommandSearch.Add_TextChanged({ Update-CommandResults })
$controls.CommandResults.Add_MouseDoubleClick({ Invoke-SelectedCommand })
$controls.CommandOverlay.Add_MouseDown({ param($sender,$event) if ($event.OriginalSource -eq $controls.CommandOverlay) { Close-CommandPalette; $event.Handled = $true } })
$controls.ChatSurface.Add_SizeChanged({ Update-GuiLayout })
$window.Add_SizeChanged({ Update-GuiLayout })
$controls.ComposerSurface.Add_IsKeyboardFocusWithinChanged({
    $targetOpacity = if ($controls.ComposerSurface.IsKeyboardFocusWithin) { 1.0 } else { 0.0 }
    $transition = [Windows.Media.Animation.DoubleAnimation]::new($targetOpacity,[Windows.Duration]::new([TimeSpan]::FromMilliseconds(140)))
    $controls.ComposerFocusRing.BeginAnimation([Windows.UIElement]::OpacityProperty,$transition)
})
$window.Add_ContentRendered({ Update-GuiLayout; $controls.Prompt.Focus() | Out-Null })
$timer.Start()
$window.ShowDialog() | Out-Null
} catch {
    Write-GuiStartupFailure $_
    [Console]::Error.WriteLine('AlphaCode GUI failed. Redacted diagnostics are saved in LocalAppData/AlphaCode.')
    [Windows.MessageBox]::Show('The desktop could not continue. Diagnostics were saved in LocalAppData/AlphaCode.', 'Could not open AlphaCode GUI', 'OK', 'Error') | Out-Null
    throw
} finally {
    Stop-GuiResources
    if ($window -and $dispatcherExceptionHandler) { $window.Dispatcher.remove_UnhandledException($dispatcherExceptionHandler) }
    try { $mutex.ReleaseMutex() } catch { }
    $mutex.Dispose()
}


