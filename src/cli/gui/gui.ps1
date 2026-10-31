param([Parameter(Mandatory=$true)][string]$Executable, [switch]$Preview)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Windows.Forms

$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$mutexName = 'Local\AlphaCode.Desktop.' + $identity
if ($Preview) { $mutexName += '.Preview' }
$mutex = New-Object System.Threading.Mutex($false, $mutexName)
if (-not $mutex.WaitOne(0)) { $mutex.Dispose(); return }

$script:process = $null
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
$script:sidebarWidth = 300
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
$settingsDir = Join-Path $env:LOCALAPPDATA 'AlphaCode'
$settingsPath = Join-Path $settingsDir 'gui.json'
$expandedWidth = 1280
$expandedHeight = 820

try {
[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
 xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="AlphaCode"
 Width="1280" Height="820" MinWidth="760" MinHeight="520" WindowStyle="None" ResizeMode="CanResize" WindowStartupLocation="CenterScreen"
 AllowsTransparency="False" Background="#171511" Topmost="False" ShowInTaskbar="True"
 FontFamily="Segoe UI" FontSize="13" UseLayoutRounding="True" SnapsToDevicePixels="True">
 <WindowChrome.WindowChrome>
  <!-- ResizeBorderThickness restores the resize grips and correct maximize
       work-area handling that WindowStyle="None" removes, while
       CaptionHeight="0" keeps the whole client area hit-testable so the
       script owns dragging and double-click-to-maximize. That avoids two drag
       loops (native caption + DragMove) fighting each other. -->
  <WindowChrome CaptionHeight="0" ResizeBorderThickness="6" CornerRadius="0" GlassFrameThickness="0" UseAeroCaptionButtons="False"/>
 </WindowChrome.WindowChrome>
 <Window.Resources>
  <SolidColorBrush x:Key="WindowBg" Color="#171511"/>
  <SolidColorBrush x:Key="RailBg" Color="#12100D"/>
  <SolidColorBrush x:Key="SidebarBg" Color="#201C16"/>
  <SolidColorBrush x:Key="Surface" Color="#2A241B"/>
  <SolidColorBrush x:Key="SurfaceElevated" Color="#30291F"/>
  <SolidColorBrush x:Key="SurfaceHover" Color="#403521"/>
  <SolidColorBrush x:Key="Border" Color="#59482D"/>
  <SolidColorBrush x:Key="Text" Color="#F7F0E0"/>
  <SolidColorBrush x:Key="Muted" Color="#C8BCA5"/>
  <SolidColorBrush x:Key="Dim" Color="#A99A80"/>
  <SolidColorBrush x:Key="Accent" Color="#E8BE60"/>
  <SolidColorBrush x:Key="AccentSoft" Color="#49391F"/>
  <Style TargetType="Button">
   <Setter Property="Foreground" Value="{StaticResource Text}"/>
   <Setter Property="Background" Value="Transparent"/>
   <Setter Property="BorderThickness" Value="0"/>
   <Setter Property="Padding" Value="10,8"/>
   <Setter Property="FontSize" Value="13"/>
   <Setter Property="Cursor" Value="Hand"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
    <Border x:Name="ButtonSurface" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="8" Padding="{TemplateBinding Padding}">
     <ContentPresenter HorizontalAlignment="{TemplateBinding HorizontalContentAlignment}" VerticalAlignment="Center"/>
    </Border>
    <ControlTemplate.Triggers>
     <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ButtonSurface" Property="Background" Value="{StaticResource SurfaceHover}"/><Setter Property="Foreground" Value="{StaticResource Text}"/></Trigger>
     <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="ButtonSurface" Property="BorderBrush" Value="{StaticResource Accent}"/><Setter TargetName="ButtonSurface" Property="BorderThickness" Value="1"/></Trigger>
     <Trigger Property="IsPressed" Value="True"><Setter TargetName="ButtonSurface" Property="Opacity" Value="0.68"/></Trigger>
     <Trigger Property="IsEnabled" Value="False"><Setter TargetName="ButtonSurface" Property="Opacity" Value="0.35"/></Trigger>
    </ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style x:Key="RailButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}">
   <Setter Property="Width" Value="42"/><Setter Property="Height" Value="42"/><Setter Property="Padding" Value="0"/>
   <Setter Property="FontSize" Value="19"/><Setter Property="Foreground" Value="#A7A7A7"/>
  </Style>
  <Style x:Key="NavButton" TargetType="Button" BasedOn="{StaticResource {x:Type Button}}">
   <Setter Property="HorizontalContentAlignment" Value="Left"/><Setter Property="Padding" Value="10,8"/>
   <Setter Property="Foreground" Value="#B4B4B4"/>
  </Style>
  <Style TargetType="ListBox">
   <Setter Property="Background" Value="Transparent"/><Setter Property="BorderThickness" Value="0"/>
   <Setter Property="Foreground" Value="{StaticResource Text}"/>
  </Style>
  <Style TargetType="ListBoxItem">
   <Setter Property="Foreground" Value="#DCE1EA"/><Setter Property="Padding" Value="11,9"/>
   <Setter Property="Margin" Value="0,2,0,2"/><Setter Property="HorizontalContentAlignment" Value="Stretch"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ListBoxItem">
    <Border x:Name="ItemSurface" Background="Transparent" BorderBrush="Transparent" BorderThickness="2,0,0,0" CornerRadius="8" Padding="{TemplateBinding Padding}">
     <ContentPresenter/>
    </Border>
    <ControlTemplate.Triggers>
     <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ItemSurface" Property="Background" Value="#3A3022"/></Trigger>
     <Trigger Property="IsSelected" Value="True"><Setter TargetName="ItemSurface" Property="Background" Value="#514025"/><Setter TargetName="ItemSurface" Property="BorderBrush" Value="{StaticResource Accent}"/><Setter Property="Foreground" Value="#FFF2D1"/></Trigger>
     <Trigger Property="IsKeyboardFocused" Value="True"><Setter TargetName="ItemSurface" Property="BorderBrush" Value="{StaticResource Accent}"/></Trigger>
    </ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style TargetType="ComboBoxItem">
   <Setter Property="Foreground" Value="{StaticResource Text}"/><Setter Property="Padding" Value="10,7"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ComboBoxItem">
    <Border x:Name="OptionSurface" Background="Transparent" Padding="{TemplateBinding Padding}" CornerRadius="5"><ContentPresenter/></Border>
    <ControlTemplate.Triggers>
     <Trigger Property="IsHighlighted" Value="True"><Setter TargetName="OptionSurface" Property="Background" Value="{StaticResource SurfaceHover}"/></Trigger>
     <Trigger Property="IsSelected" Value="True"><Setter TargetName="OptionSurface" Property="Background" Value="{StaticResource AccentSoft}"/></Trigger>
    </ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
  <Style TargetType="ComboBox">
   <Setter Property="Foreground" Value="{StaticResource Text}"/><Setter Property="Background" Value="{StaticResource Surface}"/>
   <Setter Property="BorderBrush" Value="{StaticResource Border}"/><Setter Property="BorderThickness" Value="1"/>
   <Setter Property="Padding" Value="9,4"/><Setter Property="Cursor" Value="Hand"/>
   <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="ComboBox">
    <Grid>
     <ToggleButton x:Name="DropToggle" Focusable="False" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" IsChecked="{Binding IsDropDownOpen, RelativeSource={RelativeSource TemplatedParent}, Mode=TwoWay}">
      <ToggleButton.Template><ControlTemplate TargetType="ToggleButton"><Border x:Name="ToggleSurface" Background="{TemplateBinding Background}" BorderBrush="{TemplateBinding BorderBrush}" BorderThickness="{TemplateBinding BorderThickness}" CornerRadius="7"><Path Data="M 0 0 L 4 4 L 8 0" Stroke="#C8BCA5" StrokeThickness="1.4" HorizontalAlignment="Right" VerticalAlignment="Center" Margin="0,0,10,0"/></Border><ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="ToggleSurface" Property="BorderBrush" Value="{StaticResource Accent}"/></Trigger></ControlTemplate.Triggers></ControlTemplate></ToggleButton.Template>
     </ToggleButton>
     <ContentPresenter Margin="9,0,23,0" VerticalAlignment="Center" HorizontalAlignment="Left" IsHitTestVisible="False" Content="{TemplateBinding SelectionBoxItem}" ContentTemplate="{TemplateBinding SelectionBoxItemTemplate}"/>
     <Popup x:Name="PART_Popup" Placement="Bottom" IsOpen="{TemplateBinding IsDropDownOpen}" AllowsTransparency="True" Focusable="False" PopupAnimation="Fade">
      <Border Background="{StaticResource SurfaceElevated}" BorderBrush="{StaticResource Border}" BorderThickness="1" CornerRadius="8" Padding="4" MinWidth="{Binding ActualWidth, RelativeSource={RelativeSource TemplatedParent}}"><ScrollViewer MaxHeight="280" CanContentScroll="True"><ItemsPresenter/></ScrollViewer></Border>
     </Popup>
    </Grid>
    <ControlTemplate.Triggers><Trigger Property="IsKeyboardFocusWithin" Value="True"><Setter TargetName="DropToggle" Property="BorderBrush" Value="{StaticResource Accent}"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter TargetName="DropToggle" Property="Opacity" Value="0.5"/></Trigger></ControlTemplate.Triggers>
   </ControlTemplate></Setter.Value></Setter>
  </Style>
 </Window.Resources>
 <Border x:Name="WindowFrame" Background="{StaticResource WindowBg}" BorderBrush="#3A3226" BorderThickness="1">
  <Grid Background="{StaticResource WindowBg}">
   <Grid.RowDefinitions><RowDefinition Height="34"/><RowDefinition Height="*"/></Grid.RowDefinitions>
  <Border x:Name="TitleBar" Grid.Row="0" Background="#171511" BorderBrush="#242830" BorderThickness="0,0,0,1">
   <Grid>
    <Grid.ColumnDefinitions><ColumnDefinition Width="150"/><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
    <StackPanel Grid.Column="0" Orientation="Horizontal" Margin="12,0,0,0" WindowChrome.IsHitTestVisibleInChrome="True">
     <Button x:Name="Back" Content="&#x2190;" FontSize="17" Foreground="#8A8A8A" Padding="6,2" ToolTip="Back"/>
     <Button x:Name="Forward" Content="&#x2192;" FontSize="17" Foreground="#8A8A8A" Padding="6,2" ToolTip="Forward"/>
     
    </StackPanel>
    <StackPanel Grid.Column="1" Orientation="Horizontal" HorizontalAlignment="Center" VerticalAlignment="Center"><Ellipse Width="7" Height="7" Fill="#E8BE60" Margin="0,0,7,0"/><TextBlock Text="AlphaCode" Foreground="#E9ECF4" FontSize="12" FontWeight="SemiBold"/><TextBlock Text="  ·  Workspace" Foreground="#687181" FontSize="11"/></StackPanel>
    <StackPanel Grid.Column="2" Orientation="Horizontal" Margin="0,0,8,0" WindowChrome.IsHitTestVisibleInChrome="True">
     <Button x:Name="Minimize" Content="&#x2014;" Padding="11,3" Foreground="#A7A7A7" ToolTip="Minimize"/>
     <Button x:Name="Maximize" Content="&#x25A1;" Padding="11,3" Foreground="#A7A7A7" ToolTip="Maximize"/>
     <Button x:Name="Close" Content="&#x00D7;" Padding="11,3" Foreground="#D0D0D0" ToolTip="Close"/>
    </StackPanel>
   </Grid>
  </Border>
  <Grid x:Name="Panel" Grid.Row="1" Visibility="Visible">
   <Grid.ColumnDefinitions><ColumnDefinition Width="58"/><ColumnDefinition Width="1"/><ColumnDefinition x:Name="SidebarColumn" Width="300"/><ColumnDefinition Width="1"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
   <Border Grid.Column="0" Background="{StaticResource RailBg}">
    <Grid>
     <Grid.RowDefinitions><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
     <StackPanel Grid.Row="0" VerticalAlignment="Top" Margin="8,13,8,0">
      <Button x:Name="NavHome" Style="{StaticResource RailButton}" Content="&#x2302;" ToolTip="Home" AutomationProperties.Name="Home"/>
      <Button x:Name="NavRecents" Style="{StaticResource RailButton}" Content="&#x25F7;" ToolTip="Recents" AutomationProperties.Name="Recent chats"/>
      <Button x:Name="NavProjects" Style="{StaticResource RailButton}" Content="&#x25C7;" ToolTip="Workspace chats" AutomationProperties.Name="Workspace chats"/>
      <Button x:Name="NavArchive" Style="{StaticResource RailButton}" Content="&#x25A4;" ToolTip="Saved chats" AutomationProperties.Name="Saved chats"/>
      <Button x:Name="NavCompanion" Content="&#x25CE;" Style="{StaticResource RailButton}" ToolTip="AlphaCode GUI" AutomationProperties.Name="Open AlphaCode chat"/>
      <Button x:Name="More" Content="&#x2022;&#x2022;&#x2022;" Style="{StaticResource RailButton}" FontSize="12" ToolTip="More" AutomationProperties.Name="More options"/>
     </StackPanel>
    </Grid>
   </Border>
   <Border Grid.Column="1" x:Name="SidebarDivider" Background="{StaticResource Border}"/>
   <Border Grid.Column="2" x:Name="SidebarSurface" Background="{StaticResource SidebarBg}" Padding="20,18,14,14">
    <Grid>
     <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
     <Grid Grid.Row="0" Margin="0,0,0,16">
      <Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
      <TextBlock Text="AlphaCode" Foreground="#F1F3F8" FontSize="19" FontWeight="SemiBold"/>
      <Button x:Name="Search" Grid.Column="1" Content="Search" Padding="7,3" Foreground="#C8BCA5" ToolTip="Search chats (Ctrl+K)" AutomationProperties.Name="Search chats"/>
     </Grid>
     <Button x:Name="NewChat" Grid.Row="1" Content="＋  New chat" HorizontalContentAlignment="Left" FontSize="14" FontWeight="SemiBold" Background="{StaticResource AccentSoft}" Foreground="#FFF0D0" Padding="9,10" Margin="0,0,0,12"/>
     <StackPanel Grid.Row="2" Margin="0,0,0,9">
      <Border Background="#111318" BorderBrush="{StaticResource Border}" BorderThickness="1" CornerRadius="9" Padding="10,0" Margin="0,0,0,16"><Grid><TextBox x:Name="SessionSearch" Height="32" Background="Transparent" BorderThickness="0" Foreground="#E8EBF3" Padding="25,0,5,0" ToolTip="Search chats" AutomationProperties.Name="Search chats"/><TextBlock x:Name="SearchPlaceholder" Text="Search chats" Foreground="#B8A98F" Margin="25,0,0,0" VerticalAlignment="Center" IsHitTestVisible="False"/></Grid></Border>
      <TextBlock Text="Workspaces" Foreground="#B8A98F" FontWeight="SemiBold" FontSize="11" Margin="0,0,0,7"/>
      <StackPanel x:Name="ProjectsList"/>
      <TextBlock Text="Recents" Foreground="#788294" FontWeight="SemiBold" FontSize="11" Margin="0,17,0,7"/>
     </StackPanel>
     <ListBox x:Name="Sessions" Grid.Row="3" SelectedValuePath="id" AutomationProperties.Name="Recent chats">
      <ListBox.ItemTemplate><DataTemplate><StackPanel><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock Text="{Binding title}" FontSize="13" TextTrimming="CharacterEllipsis"/><TextBlock Grid.Column="1" Text="{Binding state}" Foreground="#8E98A9" FontSize="10" Margin="8,0,0,0"/></Grid><TextBlock Text="{Binding detail}" Foreground="#858585" FontSize="10" TextTrimming="CharacterEllipsis" Margin="0,3,0,0"/><TextBlock Text="{Binding working_dir}" Foreground="#667184" FontSize="9" TextTrimming="CharacterEllipsis" Margin="0,3,0,0"/></StackPanel></DataTemplate></ListBox.ItemTemplate>
     </ListBox>
     <TextBlock x:Name="SessionsEmpty" Grid.Row="3" Visibility="Collapsed" Foreground="{StaticResource Muted}" TextWrapping="Wrap" Margin="10,12" IsHitTestVisible="False"/>
     <StackPanel Grid.Row="4" Margin="0,10,0,0">
      <Border Background="#11151A" BorderBrush="{StaticResource Border}" BorderThickness="1" CornerRadius="10" Padding="10,8"><Grid><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><TextBlock x:Name="SidebarStatus" Text="●  Ready" Foreground="#8CE0BD" FontSize="11"/><TextBlock x:Name="SessionCount" Text="0 live" Grid.Column="1" Foreground="#7C8697" FontSize="11"/></Grid></Border>
      <TextBlock x:Name="SidebarHint" Text="Your local workspace is ready." Foreground="#7C8697" FontSize="10" Margin="2,8,0,0" TextWrapping="Wrap"/>
     </StackPanel>
    </Grid>
   </Border>
   <Border Grid.Column="3" Background="{StaticResource Border}"/>
   <Grid Grid.Column="4" Background="{StaticResource WindowBg}">
    <Grid.RowDefinitions><RowDefinition Height="66"/><RowDefinition Height="*"/><RowDefinition Height="214"/><RowDefinition Height="30"/></Grid.RowDefinitions>
    <Grid Grid.Row="0" Margin="30,0,28,0">
     <Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions>
     <StackPanel VerticalAlignment="Center"><TextBlock x:Name="WorkspaceTitle" Text="New chat" Foreground="#F0F2F7" FontSize="16" FontWeight="SemiBold"/><TextBlock x:Name="WorkspaceSubtitle" Text="AlphaCode workspace" Foreground="#7D8798" FontSize="11" Margin="0,3,0,0"/></StackPanel>
     <StackPanel Grid.Column="1" Orientation="Horizontal" VerticalAlignment="Center"><Button x:Name="SidebarToggle" Content="Hide sidebar" Foreground="#D7C5A6" Padding="7,4" ToolTip="Show or hide the sidebar (Ctrl+B)" AutomationProperties.Name="Show or hide sidebar"/><Button x:Name="OpenTerminal" Content="Terminal ↗" Padding="10,5" Foreground="#E8BE60" ToolTip="Open the terminal interface for all AlphaCode commands" AutomationProperties.Name="Open terminal interface"/><Border x:Name="StatePill" Background="#1C4039" CornerRadius="10" Padding="8,4" Margin="0,0,7,0"><TextBlock x:Name="StateText" Text="Ready" Foreground="#8CE0BD" FontSize="10"/></Border><Button x:Name="Cancel" Content="&#x25A0;" Padding="8,3" Foreground="#E0A0A0" ToolTip="Cancel the running turn" AutomationProperties.Name="Cancel running turn"/><Button x:Name="Rename" Content="&#x270E;" Padding="8,3" Foreground="#8F8F8F" ToolTip="Rename this chat" AutomationProperties.Name="Rename chat"/><Button x:Name="Archive" Content="&#x2606;" Padding="8,3" Foreground="#8F8F8F" ToolTip="Save or unsave this chat" AutomationProperties.Name="Save chat"/><Button x:Name="Refresh" Content="&#x21BB;" Padding="8,3" Foreground="#8F8F8F" ToolTip="Refresh"/></StackPanel>
    </Grid>
    <Grid Grid.Row="1" x:Name="EmptyState" VerticalAlignment="Center" HorizontalAlignment="Center">
     <StackPanel HorizontalAlignment="Center" MaxWidth="680" Margin="24,0">
      <TextBlock x:Name="Activity" Text="What can AlphaCode help you with?" Foreground="#F4F6FA" FontSize="30" HorizontalAlignment="Center" TextAlignment="Center" TextWrapping="Wrap" FontWeight="SemiBold"/>
      <TextBlock x:Name="Detail" Text="Describe a task or ask a question about your workspace." Foreground="#8E98A9" FontSize="13" HorizontalAlignment="Center" Margin="0,11,0,0" TextAlignment="Center" TextWrapping="Wrap"/>
      <TextBlock x:Name="Timeline" Text="" Foreground="#6D7788" FontSize="11" HorizontalAlignment="Center" Margin="0,10,0,0" TextAlignment="Center" TextWrapping="Wrap"/>
      <WrapPanel HorizontalAlignment="Center" Margin="0,26,0,0">
       <Button x:Name="ExampleExplore" Content="Explore this workspace" Background="{StaticResource Surface}" BorderBrush="{StaticResource Border}" BorderThickness="1" Foreground="{StaticResource Text}" Padding="14,10" Margin="4" ToolTip="Start with a workspace overview"/>
       <Button x:Name="ExampleReview" Content="Review my changes" Background="{StaticResource Surface}" BorderBrush="{StaticResource Border}" BorderThickness="1" Foreground="{StaticResource Text}" Padding="14,10" Margin="4" ToolTip="Ask AlphaCode to review local changes"/>
       <Button x:Name="ExampleTest" Content="Run the tests" Background="{StaticResource Surface}" BorderBrush="{StaticResource Border}" BorderThickness="1" Foreground="{StaticResource Text}" Padding="14,10" Margin="4" ToolTip="Ask AlphaCode to check the project"/>
      </WrapPanel>
     </StackPanel>
    </Grid>
    <ScrollViewer x:Name="ReplyScroll" Grid.Row="1" VerticalScrollBarVisibility="Auto" Margin="46,32,46,24" Visibility="Collapsed">
     <TextBox x:Name="Reply" Text="" IsReadOnly="True" TextWrapping="Wrap" AcceptsReturn="True" BorderThickness="0" Background="Transparent" Foreground="#E4E7EE" Padding="0" FontSize="14" AutomationProperties.Name="Conversation response"/>
    </ScrollViewer>
    <Button x:Name="JumpLatest" Grid.Row="1" Content="Jump to latest ↓" HorizontalAlignment="Right" VerticalAlignment="Bottom" Margin="0,0,48,10" Background="{StaticResource SurfaceElevated}" BorderBrush="{StaticResource Accent}" BorderThickness="1" Visibility="Collapsed" ToolTip="Return to the latest response (Ctrl+End)" AutomationProperties.Name="Jump to latest response"/>
    <Border Grid.Row="2" Background="{StaticResource SurfaceElevated}" BorderBrush="{StaticResource Border}" BorderThickness="1" CornerRadius="18" Margin="30,8,30,16" Padding="16,13,16,12">
     <Grid>
      <Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
      <Grid Grid.Row="0" Margin="0,0,0,6"><Grid.ColumnDefinitions><ColumnDefinition/><ColumnDefinition Width="Auto"/></Grid.ColumnDefinitions><StackPanel Orientation="Horizontal"><TextBlock Text="Workspace context" Foreground="#E0E4ED" FontSize="12" FontWeight="SemiBold"/><TextBlock Text="  ·  Enter to send · Shift+Enter for a new line" Foreground="#C8BCA5" FontSize="11" Margin="8,0,0,0"/></StackPanel><Button x:Name="ComposerSettings" Content="&#x2699;" Grid.Column="1" Padding="5,0" Foreground="#9AA3B3" FontSize="15" ToolTip="Composer settings" AutomationProperties.Name="Composer settings"/></Grid>
      <Grid Grid.Row="1"><TextBox x:Name="Prompt" Height="84" MaxLength="16000" AcceptsReturn="True" TextWrapping="Wrap" VerticalScrollBarVisibility="Auto" Background="Transparent" Foreground="#F7F0E0" CaretBrush="#F2D286" BorderThickness="0" Padding="0" FontSize="15" AutomationProperties.Name="Prompt"/><TextBox x:Name="CommandInput" Height="84" MaxLength="16000" Visibility="Collapsed" AcceptsReturn="True" TextWrapping="Wrap" Background="Transparent" Foreground="#F7F0E0" BorderThickness="0" Padding="0" FontSize="15" AutomationProperties.Name="Command input"/><PasswordBox x:Name="SecretInput" Height="84" MaxLength="16000" Visibility="Collapsed" Background="Transparent" Foreground="#F7F0E0" BorderThickness="0" Padding="0" FontSize="15" AutomationProperties.Name="Password for running command"/><TextBlock x:Name="Placeholder" Text="Ask AlphaCode anything…" Foreground="#B8A98F" FontSize="15" IsHitTestVisible="False" Margin="0,0,0,0" VerticalAlignment="Top"/></Grid>
      <DockPanel Grid.Row="2" Margin="0,8,0,0" LastChildFill="False"><StackPanel Orientation="Horizontal" DockPanel.Dock="Left"><Button x:Name="AddContext" Content="＋" Padding="3,0" FontSize="22" Foreground="#D6D6D6" ToolTip="Use the active workspace as context" AutomationProperties.Name="Add context"/><Border Background="#191919" CornerRadius="9" Padding="8,4" Margin="8,0,0,0"><TextBlock x:Name="WorkspacePath" Text="Choose workspace" Foreground="#E8BE60" FontSize="11" MaxWidth="145" TextTrimming="CharacterEllipsis"/></Border></StackPanel><StackPanel Orientation="Horizontal" DockPanel.Dock="Right"><ComboBox x:Name="ModelPicker" Width="140" Height="32" Margin="0,0,6,0" ToolTip="Choose model" AutomationProperties.Name="Model"/><ComboBox x:Name="EffortPicker" Width="84" Height="32" Margin="0,0,7,0" SelectedValuePath="Tag" ToolTip="Choose reasoning effort" AutomationProperties.Name="Reasoning effort"><ComboBoxItem Content="Auto" Tag="none"/><ComboBoxItem Content="Low" Tag="low"/><ComboBoxItem Content="Medium" Tag="medium"/><ComboBoxItem Content="High" Tag="high"/><ComboBoxItem Content="Max" Tag="max"/></ComboBox><Button x:Name="Voice" Content="&#x266B;" Padding="6,0" Foreground="#777777" IsEnabled="False" ToolTip="Voice input is not available in this local build" AutomationProperties.Name="Voice input unavailable"/><Button x:Name="Copy" Content="Copy" Padding="8,4" Foreground="#A5A5A5" Margin="4,0,4,0" ToolTip="Copy response" AutomationProperties.Name="Copy response"/><Button x:Name="Send" Content="&#x2191;" Background="#E8BE60" Foreground="#241B0C" Padding="9,4" FontSize="16" ToolTip="Send prompt (Enter; Shift+Enter for a new line)" AutomationProperties.Name="Send prompt"/></StackPanel></DockPanel>
     </Grid>
    </Border>
    <TextBlock x:Name="FooterStatus" Grid.Row="3" Text="Ready  ·  Enter to send  ·  Shift+Enter for a new line" Foreground="#B0A189" FontSize="11" Margin="30,0,30,0" VerticalAlignment="Center"/>
   </Grid>
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

function Write-GuiStartupFailure([string]$details) {
    try {
        [IO.Directory]::CreateDirectory($settingsDir) | Out-Null
        [IO.File]::WriteAllText((Join-Path $settingsDir 'gui-startup.log'), $details)
    } catch { }
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
            Write-GuiStartupFailure $details
            throw $details
        }
    }
}

$window = Load-GuiWindow $xaml
$controls = @{}
foreach ($name in @('Panel','TitleBar','Back','Forward','SidebarToggle','SidebarColumn','SidebarDivider','SidebarSurface','Minimize','Maximize','Close','Cancel','Rename','Archive','Sessions','SessionsEmpty','ProjectsList','SessionSearch','SearchPlaceholder','ModelPicker','EffortPicker','WorkspacePath','OpenTerminal','ExampleExplore','ExampleReview','ExampleTest','Activity','Detail','Timeline','Reply','ReplyScroll','Prompt','CommandInput','SecretInput','Placeholder','EmptyState','Send','Copy','Refresh','AddContext','Voice','NewChat','NavHome','NavRecents','NavProjects','NavArchive','NavCompanion','More','Search','ComposerSettings','SidebarHint','SidebarStatus','SessionCount','StatePill','StateText','FooterStatus','WorkspaceTitle','WorkspaceSubtitle')) {
    $controls[$name] = $window.FindName($name)
}

function Save-GuiSettings {
    if ($Preview) { return }
    try {
        [IO.Directory]::CreateDirectory($settingsDir) | Out-Null
        $settings = @{ left=$window.Left; top=$window.Top; topmost_opt_in=$window.Topmost; model=[string]$script:selectedModel; effort=[string]$script:selectedEffort; sidebar_visible=$script:sidebarVisible; sidebar_width=$script:sidebarWidth; reading_size=$controls.Reply.FontSize }
        $temporary = $settingsPath + '.' + $PID + '.tmp'
        [IO.File]::WriteAllText($temporary, ($settings | ConvertTo-Json -Compress))
        Move-Item -LiteralPath $temporary -Destination $settingsPath -Force
    } catch { }
}

function Set-TextDirection($textControl, [string]$text) {
    $hasArabic = $text -match '[\u0600-\u06FF\u0750-\u077F\u08A0-\u08FF]'
    if ($hasArabic) {
        $textControl.FlowDirection = 'RightToLeft'
        $textControl.Language = [System.Windows.Markup.XmlLanguage]::GetLanguage('ar-EG')
        $textControl.TextAlignment = 'Right'
    } else {
        $textControl.FlowDirection = 'LeftToRight'
        $textControl.Language = [System.Windows.Markup.XmlLanguage]::GetLanguage('en-US')
        $textControl.TextAlignment = 'Left'
    }
}

function Start-Bridge {
    if ($Preview) { return }
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Executable
    $start.Arguments = '--no-update --quiet --no-selfdev gui --bridge'
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
    $projects = @($script:allSessions | Where-Object { $_.id -ne '__companion__' })
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
        $empty.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#B8A98F')
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
        $button.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#D4D4D4')
        $button.Padding = [Windows.Thickness]::new(7,7,7,7)
        $button.ToolTip = if ($session.working_dir) { [string]$session.working_dir } else { [string]$session.detail }
        $button.Add_Click({ param($sender) Set-Navigation 'Home'; $controls.Sessions.SelectedValue = [string]$sender.Tag })
        $controls.ProjectsList.Children.Add($button) | Out-Null
    }
}

function Apply-Navigation {
    $selected = $controls.Sessions.SelectedValue
    $visible = @(switch ($script:navigation) {
        'Projects' { @($script:allSessions | Where-Object { $_.id -ne '__companion__' -and -not [string]::IsNullOrWhiteSpace([string]$_.working_dir) }) }
        'Recents' { @($script:allSessions | Where-Object { $_.id -ne '__companion__' }) }
        'Archive' { @($script:allSessions | Where-Object { [bool]$_.saved }) }
        default { @($script:allSessions) }
    })
    $query = [string]$controls.SessionSearch.Text
    if (-not [string]::IsNullOrWhiteSpace($query)) {
        $pattern = [regex]::Escape($query.Trim())
        $visible = @($visible | Where-Object { ([string]$_.title -match $pattern) -or ([string]$_.detail -match $pattern) -or ([string]$_.working_dir -match $pattern) })
    }
    $script:suppressSelectionEvents = $true
    $listSignature = ($script:navigation + "`n" + $query + "`n" + (@($visible | ForEach-Object { "$($_.id)|$($_.title)|$($_.state)|$($_.detail)|$($_.working_dir)|$($_.saved)" }) -join "`n"))
    if ($script:navigationListSignature -eq $listSignature) {
        $script:suppressSelectionEvents = $false
        return
    }
    $script:navigationListSignature = $listSignature
    $controls.Sessions.ItemsSource = $visible
    $controls.SessionsEmpty.Visibility = if ($visible.Count -eq 0) { 'Visible' } else { 'Collapsed' }
    $controls.SessionsEmpty.Text = if (-not [string]::IsNullOrWhiteSpace($query)) { 'No matching chats. Try another search.' } elseif ($script:navigation -eq 'Archive') { 'No saved chats yet.' } else { 'No chats in this view.' }
    if ($visible.Count -gt 0) {
        $ids = @($visible | ForEach-Object { $_.id })
        $controls.Sessions.SelectedValue = if ($ids -contains $selected) { $selected } elseif ($ids -contains '__companion__') { '__companion__' } else { $ids[0] }
    } else {
        $controls.Sessions.SelectedValue = $null
    }
    $script:suppressSelectionEvents = $false
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
    $script:ownedBusy = [bool]$snapshot.owned_busy
    $script:working = [int]$snapshot.working
    $firstLiveSnapshot = $script:lastSignature -eq ''
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
    if ($firstLiveSnapshot -and $script:navigation -eq 'Home' -and [string]::IsNullOrWhiteSpace([string]$controls.SessionSearch.Text) -and @($controls.Sessions.ItemsSource | ForEach-Object { $_.id }) -contains '__companion__') {
        $controls.Sessions.SelectedValue = '__companion__'
    }
    $liveCount = @($script:allSessions | Where-Object { $_.id -ne '__companion__' -and $_.state -ne 'stored' }).Count
    $storedCount = @($script:allSessions | Where-Object { $_.state -eq 'stored' }).Count
    if ($storedCount -gt 0) {
        $controls.SessionCount.Text = "$liveCount live · $storedCount saved"
    } elseif ($liveCount -eq 1) {
        $controls.SessionCount.Text = '1 live'
    } else {
        $controls.SessionCount.Text = "$liveCount live"
    }
    $models = @($snapshot.models | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } | ForEach-Object { [string]$_ })
    if (-not [string]::IsNullOrWhiteSpace([string]$snapshot.model) -and $models -notcontains [string]$snapshot.model) {
        $models = @([string]$snapshot.model) + $models
    }
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
        $targetModel = if ($script:modelRequestPending -and $script:preferredModel -and $models -contains $script:preferredModel) { $script:preferredModel } elseif ($snapshot.model -and $models -contains [string]$snapshot.model) { [string]$snapshot.model } else { $models[0] }
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
        $script:suppressSelectionEvents = $true
        $placeholder = if ($selected -eq '__companion__' -and [string]::IsNullOrWhiteSpace([string]$snapshot.owned_session_id)) { 'Connecting…' } else { 'Default model' }
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
    $script:snapshot = $snapshot
    $badge = if ([int]$snapshot.attention -gt 0) { 'Needs you' } elseif ([int]$snapshot.offline -gt 0) { 'Reconnecting' } elseif ($script:working -gt 0) { "$($script:working) working" } else { 'Ready' }
    $color = if ([int]$snapshot.attention -gt 0) { '#F3C777' } elseif ($script:working -gt 0) { '#A7DFFC' } else { '#8CE0BD' }
    $script:dictationAvailable = [bool]$snapshot.dictation_available
    # Keep the affordance usable even when dictation is not configured: the
    # click explains the one-line config needed instead of leaving a dead,
    # permanently disabled control in the composer.
    $controls.Voice.Visibility = if ($script:dictationAvailable) { 'Visible' } else { 'Collapsed' }
    $controls.Voice.IsEnabled = $script:dictationAvailable -and -not $script:inputMode
    $controls.Voice.ToolTip = if ($script:dictationAvailable) { 'Run configured dictation and insert the transcript' } else { 'Configure [dictation].command to enable voice input' }
    $controls.StateText.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString($color)
    $controls.StateText.Text = $badge
    $controls.StatePill.Background = [Windows.Media.BrushConverter]::new().ConvertFromString($(if ($badge -eq 'Needs you') { '#49371F' } elseif ($badge -eq 'Reconnecting') { '#3B2C29' } else { '#193A3A' }))
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
        if ($script:draftSession) { $script:drafts[$script:draftSession] = $controls.Prompt.Text }
        $script:draftSession = [string]$selected
        $controls.Prompt.Text = [string]$script:drafts[$script:draftSession]
        $controls.Prompt.CaretIndex = $controls.Prompt.Text.Length
        $controls.ReplyScroll.ScrollToHome()
    }
    $storedOnly = [string]$session.state -eq 'stored'
    $controls.WorkspaceTitle.Text = $session.title
    $controls.WorkspaceSubtitle.Text = switch ($session.state) {
        'working' { 'AlphaCode is actively working in this workspace' }
        'attention' { 'This workspace needs a decision from you' }
        'offline' { 'The workspace is reconnecting to its local daemon' }
        'stored' { 'Saved chat from the CLI session list · read-only here' }
        default { if ($session.working_dir) { [string]$session.working_dir } else { 'Ready to work in your workspace' } }
    }
    $controls.SidebarHint.Text = "$($session.title)`n$($session.detail)"
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
    $controls.Send.Content = if ($script:inputMode) { 'Send input' } else { 'Send prompt' }
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
        $controls.Activity.Text = if ($selected -eq '__companion__') { 'What can AlphaCode help you with?' } else { 'Ready for your next idea' }
    }
    if ($controls.Reply.Text -ne $reply) {
        $following = $controls.ReplyScroll.VerticalOffset -ge ($controls.ReplyScroll.ScrollableHeight - 8)
        $controls.Reply.Text = $reply
        Set-TextDirection $controls.Reply $reply
        if ($following) { $controls.ReplyScroll.ScrollToEnd() }
    }
    $controls.Copy.IsEnabled = $hasReply
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
    $controls.Archive.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString($(if ([bool]$session.saved) { '#F3C777' } else { '#8F8F8F' }))
    $controls.Send.ToolTip = if ($storedOnly) { 'Resume this chat from the terminal to continue it' } else { 'Send prompt (Enter; Shift+Enter for a new line)' }
    $controls.Send.IsEnabled = -not $storedOnly -and [string]$session.state -ne 'offline' -and -not ($script:ownedBusy -and $selected -eq '__companion__' -and -not $script:inputMode) -and $script:pendingKind -notin @('prompt','input') -and $script:queue.Count -eq 0
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
    $dialog.Background = [Windows.Media.BrushConverter]::new().ConvertFromString('#201C16')
    $dock = New-Object Windows.Controls.DockPanel
    $dock.Margin = [Windows.Thickness]::new(16)
    $text = New-Object Windows.Controls.TextBox
    $text.Text = $initial
    $text.FontSize = 14
    $text.Padding = [Windows.Thickness]::new(6)
    $text.Background = [Windows.Media.BrushConverter]::new().ConvertFromString('#1E2128')
    $text.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#F0F2F7')
    $text.BorderBrush = [Windows.Media.BrushConverter]::new().ConvertFromString('#333A46')
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
    $label.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#8E98A9')
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

$controls.Prompt.Add_TextChanged({ $controls.Placeholder.Visibility = if ($controls.Prompt.Text.Length) { 'Collapsed' } else { 'Visible' }; Set-TextDirection $controls.Prompt $controls.Prompt.Text })
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
    if ($persist) {
        Save-GuiSettings
        Show-GuiNotice "Reading size: $size pt · Ctrl+plus / Ctrl+minus · Ctrl+0 to reset"
    }
}
$controls.Prompt.Add_PreviewKeyDown({
    param($sender,$event)
    if ($event.Key -eq 'Return' -and -not ([Windows.Input.Keyboard]::Modifiers -band [Windows.Input.ModifierKeys]::Shift)) {
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
# Present a folder picker and return the chosen path, or $null when cancelled.
#
# The legacy FolderBrowserDialog is a fixed-size tree control with no address
# bar, no breadcrumb, and no shell view — on Windows 11 it looks a decade old.
# Prefer the modern shell picker (CommonOpenFileDialog / OpenFolderDialog) and
# fall back to the legacy dialog only when the shell view cannot be loaded, so
# the button never fails outright.
function Select-AlphacodeWorkspace([string]$Description) {
    # 1. WPF's own folder dialog (Windows 10 1809+ / .NET 8). Cheapest modern path.
    try {
        $type = [System.Type]::GetType('Microsoft.Win32.OpenFolderDialog, PresentationFramework')
        if ($type) {
            $modern = [Activator]::CreateInstance($type)
            $modern.Title = $Description
            $modern.Multiselect = $false
            if ($modern.ShowDialog() -eq $true -and $modern.FolderName) {
                return [string]$modern.FolderName
            }
            return $null
        }
    } catch { }

    # 2. Windows API Code Pack shell picker, when the assembly is present.
    try {
        Add-Type -AssemblyName 'Microsoft.WindowsAPICodePack.Shell' -ErrorAction Stop
        $modern = New-Object Microsoft.WindowsAPICodePack.Dialogs.CommonOpenFileDialog
        $modern.IsFolderPicker = $true
        $modern.Title = $Description
        if ($modern.ShowDialog() -eq [Microsoft.WindowsAPICodePack.Dialogs.CommonFileDialogResult]::Ok) {
            return [string]$modern.FileName
        }
        return $null
    } catch { }

    # 3. Legacy fallback: still functional, just not pretty.
    $legacy = New-Object Windows.Forms.FolderBrowserDialog
    $legacy.Description = $Description
    $legacy.ShowNewFolderButton = $false
    try {
        if ($legacy.ShowDialog() -eq [Windows.Forms.DialogResult]::OK) {
            return [string]$legacy.SelectedPath
        }
        return $null
    } finally { $legacy.Dispose() }
}

$controls.AddContext.Add_Click({
    if ($script:ownedBusy) { Show-GuiNotice 'Finish or cancel the current GUI turn first'; return }
    $selectedPath = Select-AlphacodeWorkspace 'Choose a workspace for a new AlphaCode chat'
    if ($selectedPath) {
        $controls.Sessions.SelectedValue = '__companion__'
        Queue-Action 'new_chat' '__companion__' @{ working_dir = $selectedPath }
        Show-GuiNotice "Opening workspace: $selectedPath"
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
function Set-SidebarVisibility([bool]$visible) {
    if (-not $visible -and $script:sidebarVisible -and $controls.SidebarColumn.ActualWidth -gt 60) {
        $script:sidebarWidth = [double]$controls.SidebarColumn.ActualWidth
    }
    $script:sidebarVisible = $visible
    $controls.SidebarColumn.Width = if ($visible) { [Windows.GridLength]::new($script:sidebarWidth) } else { [Windows.GridLength]::new(0) }
    $controls.SidebarSurface.Visibility = if ($visible) { 'Visible' } else { 'Collapsed' }
    $controls.SidebarDivider.Visibility = if ($visible) { 'Visible' } else { 'Collapsed' }
    $controls.SidebarToggle.Content = if ($visible) { 'Hide sidebar' } else { 'Show sidebar' }
    $controls.SidebarHint.Text = if ($visible) { 'Sidebar visible · Ctrl+K jumps to chats' } else { 'Sidebar hidden · use the rail to navigate' }
    $controls.FooterStatus.Text = if ($visible) { 'Sidebar restored' } else { 'Sidebar hidden · Ctrl+B or the rail restores it' }
    Save-GuiSettings
}
function Set-Navigation([string]$section, [bool]$record = $true) {
    if ($record -and $script:navigation -ne $section) {
        $script:navigationBack.Push($script:navigation)
        $script:navigationForward.Clear()
    }
    $script:navigation = $section
    $controls.Back.IsEnabled = $script:navigationBack.Count -gt 0
    $controls.Forward.IsEnabled = $script:navigationForward.Count -gt 0
    foreach ($item in @($controls.NavHome,$controls.NavRecents,$controls.NavProjects,$controls.NavArchive)) {
        $item.Background = [Windows.Media.BrushConverter]::new().ConvertFromString('#171511')
        $item.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#C8BCA5')
    }
    $active = switch ($section) { 'Recents' { $controls.NavRecents } 'Projects' { $controls.NavProjects } 'Archive' { $controls.NavArchive } default { $controls.NavHome } }
    $active.Background = [Windows.Media.BrushConverter]::new().ConvertFromString('#514025')
    $active.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#FFF0C9')
    $controls.FooterStatus.Text = switch ($section) {
        'Recents' { 'Recent workspaces  ·  live local sessions stay in sync' }
        'Projects' { 'Workspace chats  ·  sessions with a known workspace folder' }
        'Archive' { 'Saved chats  ·  keep important work easy to revisit' }
        default { 'Home  ·  local sockets protected  ·  model follows your AlphaCode settings' }
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
$controls.NavCompanion.Add_Click({ Set-Navigation 'Home'; $controls.Sessions.SelectedValue = '__companion__' })
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
    Update-ProjectsList
    Apply-Navigation
})
$controls.ComposerSettings.Add_Click({
    $menu.PlacementTarget = $controls.ComposerSettings
    $menu.IsOpen = $true
})
$controls.NavHome.Add_Click({ Set-Navigation 'Home' })
$controls.NavRecents.Add_Click({ Set-Navigation 'Recents' })
$controls.NavProjects.Add_Click({ Set-Navigation 'Projects' })
$controls.NavArchive.Add_Click({ Set-Navigation 'Archive' })
$window.Add_PreviewKeyDown({
    param($sender,$event)
    $modifiers = [Windows.Input.Keyboard]::Modifiers
    if (($modifiers -band [Windows.Input.ModifierKeys]::Control) -and $event.Key -in @('OemPlus','Add','OemMinus','Subtract','D0','NumPad0')) {
        $size = switch ([string]$event.Key) { { $_ -in @('OemPlus','Add') } { $controls.Reply.FontSize + 1 } { $_ -in @('OemMinus','Subtract') } { $controls.Reply.FontSize - 1 } default { 14 } }
        Set-ReadingSize $size
        $event.Handled = $true
    } elseif ($event.Key -eq 'End' -and ($modifiers -band [Windows.Input.ModifierKeys]::Control) -and $controls.Reply.IsKeyboardFocusWithin) {
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
    # SelectedValue may still be the previous row during this routed event.
    if ($event.AddedItems.Count -gt 0) { Update-SelectedSession ([string]$event.AddedItems[0].id) }
})

$menu = New-Object Windows.Controls.ContextMenu
$pin = New-Object Windows.Controls.MenuItem
$pin.Header = 'Always on top'; $pin.IsCheckable = $true; $pin.IsChecked = $false
$pin.Add_Click({ $window.Topmost = $pin.IsChecked; Save-GuiSettings })
$menu.Items.Add($pin) | Out-Null
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

# Open the full desktop workspace on every launch.
Set-Navigation 'Home'

$timer = New-Object Windows.Threading.DispatcherTimer
$timer.Interval = [TimeSpan]::FromMilliseconds(100)
$timer.Add_Tick({
    try {
        $timer.Interval = [TimeSpan]::FromMilliseconds($(if ($script:working -gt 0) { 100 } else { 400 }))
        if ($Preview) { return }
        if ($script:pending) {
            if ($script:pending.IsCompleted) {
                $line = $script:pending.GetAwaiter().GetResult()
                if ($null -eq $line) { throw 'The local GUI connection closed.' }
                $response = $line | ConvertFrom-Json
                if ($response.sessions) {
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
                    if ($script:pendingKind -eq 'model') { $script:modelRequestPending = $false; $script:preferredModel = ''; $script:lastPoll = [DateTime]::MinValue }
                    if ($script:pendingKind -eq 'effort') { $script:effortRequestPending = $false; $script:preferredEffort = ''; $script:lastPoll = [DateTime]::MinValue }
                    if ($script:pendingKind -eq 'capabilities') { $script:capabilitiesRequestPending = $false }
                    if ($script:pendingKind -eq 'dictate') { $controls.Voice.IsEnabled = -not $script:inputMode }
                    $controls.ModelPicker.IsEnabled = $true
                    $controls.EffortPicker.IsEnabled = $true
                    $script:submittedPrompt = ''
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
            $request = @{ action='status' }
            if ($script:expanded) { $request.session_id = [string]$controls.Sessions.SelectedValue }
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
        $controls.StateText.Text = 'Reconnecting'
        $script:deliveryNotice = $_.Exception.Message
        $script:submittedPrompt = ''
        $script:noticeUntil = [DateTime]::MaxValue
        $controls.Detail.Text = $script:deliveryNotice
        $controls.Send.IsEnabled = $true
        $script:modelRequestPending = $false
        $script:effortRequestPending = $false
        $script:capabilitiesRequestPending = $false
        $controls.ModelPicker.IsEnabled = $true
        $controls.EffortPicker.IsEnabled = $true
        $script:pending = $null; $script:pendingKind = ''; $script:queue.Clear()
        if ($script:inputWriter) { $script:inputWriter.Dispose(); $script:inputWriter = $null }
        if ($script:process) { try { $script:process.Kill() } catch { }; $script:process.Dispose(); $script:process = $null }
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
    $timer.Stop(); Save-GuiSettings
    if ($script:inputWriter) { $script:inputWriter.Dispose(); $script:inputWriter = $null }
    if ($script:process) { try { $script:process.Kill() } catch { }; $script:process.Dispose() }
})

if ($Preview) {
    # Expose the shell in app switchers for accessibility/visual QA without
    # inventing conversations, projects, replies, or daemon activity.
    $window.ShowInTaskbar = $true
    Set-Snapshot ([pscustomobject]@{ owned_busy=$false; working=0; attention=0; offline=0; connected_servers=0; reply=''; input_request=$null; selected_session_id='__companion__'; selected_reply=''; activity=@(); model=''; provider=''; effort='none'; models=@(); sessions=@(
        [pscustomobject]@{ id='__companion__'; title='AlphaCode chat'; state='ready'; detail='Connect AlphaCode to start a live workspace'; saved=$false }
    ) })
}
$window.Add_ContentRendered({ $controls.Prompt.Focus() | Out-Null })
$timer.Start()
$window.ShowDialog() | Out-Null
} catch {
    $details = $_.Exception.ToString() + [Environment]::NewLine + $_.ScriptStackTrace
    Write-GuiStartupFailure $details
    [Console]::Error.WriteLine($details)
    [Windows.MessageBox]::Show($_.Exception.Message, 'Could not open AlphaCode GUI', 'OK', 'Error') | Out-Null
} finally {
    try { $mutex.ReleaseMutex() } catch { }
    $mutex.Dispose()
}


