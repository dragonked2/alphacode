[CmdletBinding()]
param(
    [switch]$Purge,
    # Remove the bin dir from the user PATH. Defaults to on: install.ps1 -AddPath
    # adds the entry, so leaving it behind here would leave the user's PATH
    # pointing at a directory that no longer exists, which shows up later as a
    # confusing "command not found" for a program that was uninstalled.
    [switch]$KeepPath
)

# uninstall.ps1 - remove Alphacode from a Windows machine.
#
# Invocation patterns:
#   iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.ps1 | iex
#   .\uninstall.ps1            # local file
#   .\uninstall.ps1 -Purge     # also remove ~/.local/bin\alphacode.exe
#   .\uninstall.ps1 -KeepPath  # leave the user PATH entry alone
#
# IMPORTANT history (do not regress):
#   v1.0.22 shipped `$Home = $env:USERPROFILE` near the top. $HOME is a
#   read-only automatic variable in PowerShell; assigning to it throws
#   `VariableNotWritable (SessionStateUnauthorizedAccessException)` and
#   aborts the script. The user-visible failure was
#     iex : Cannot overwrite variable HOME because it is read-only or constant.
#   which made the one-liner above completely fail. This script now uses
#   $UserProfile (a normal variable) instead, never assigns to $HOME /
#   $Home, and wraps every Remove-Item in try/catch so a single locked
#   file (e.g. an in-use alphacode.exe) does not abort the rest of the
#   cleanup. Exit codes are returned via `return`, never `exit`, so
#   `iwr ... | iex` does not terminate the user's PowerShell session.

# Declared at script (outermost) scope so the nested Remove-IfExists
# helper can mutate it via normal lexical scoping. Declaring it inside
# Invoke-Uninstall and trying to reach it from Remove-IfExists via
# $script:skippedList would look up the wrong scope and produce a
# NullReferenceException ("You cannot call a method on a null-valued
# expression") the first time the helper tried to append.
$script:SkippedPaths = New-Object System.Collections.Generic.List[string]

# --- user PATH removal -------------------------------------------------------
#
# Deliberately mirrors install.ps1's rules so the pair cannot disagree:
#   * HKCU\Environment\Path only. The machine PATH is never touched;
#   * $env:PATH is never rewritten. Removing the entry from the *session* would
#     change the behaviour of the very shell the user is running this from, and
#     the entry is harmless once the directory is gone;
#   * the raw value is read with DoNotExpandEnvironmentNames and written back
#     with its original kind, so a PATH using %USERPROFILE%\bin keeps working
#     instead of being frozen to today's literal path;
#   * only the exact bin dir is removed -- a sibling directory that merely shares
#     a prefix (...\alphacode\bin-old) is left alone.
# Is this Windows?
#
# Deliberately NOT `$IsWindows`: that variable only exists in PowerShell 6+, and
# this script is invoked with `iwr ... | iex`, which runs Windows PowerShell
# 5.1 on a default Windows machine, where the variable is undefined and every
# test against it is false.
function Test-UninstallWindows {
    [CmdletBinding()]
    param()
    return ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
}

function Remove-AlphacodeFromUserPath {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$BinDir)

    if (-not (Test-UninstallWindows)) {
        Write-Host "[warn] PATH removal only applies on Windows." -ForegroundColor Yellow
        return
    }

    $target = $BinDir.Trim().TrimEnd('\', '/')
    if ([string]::IsNullOrWhiteSpace($target)) { return }

    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    if ($null -eq $key) { return }
    try {
        if (-not ($key.GetValueNames() -contains 'Path')) { return }
        $kind = $key.GetValueKind('Path')
        $raw = $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    } finally {
        $key.Close()
    }
    if ([string]::IsNullOrEmpty($raw)) { return }

    $entries = @($raw -split ';')
    $kept = @($entries | Where-Object {
        $_ -eq '' -or ($_.Trim().TrimEnd('\', '/') -ine $target)
    })
    if ($kept.Count -eq $entries.Count) { return }   # not present; nothing to do

    $newValue = ($kept -join ';').TrimEnd(';')

    $writable = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    if ($null -eq $writable) {
        Write-Host "[warn] Could not open HKCU\Environment for writing; PATH left unchanged." -ForegroundColor Yellow
        return
    }
    try {
        $writable.SetValue('Path', $newValue, $kind)
    } finally {
        $writable.Close()
    }

    try {
        if (-not ('AlphacodeUninstaller.Native' -as [type])) {
            Add-Type -Namespace 'AlphacodeUninstaller' -Name 'Native' -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(
    IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam,
    uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
        }
        $result = [UIntPtr]::Zero
        [void][AlphacodeUninstaller.Native]::SendMessageTimeout(
            [IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 5000, [ref]$result)
    } catch {
        Write-Host "[warn] Could not broadcast the environment change: $($_.Exception.Message)" -ForegroundColor Yellow
    }

    Write-Host "Removed from your user PATH: $target" -ForegroundColor Cyan
    Write-Host "    Open a new terminal for this to take effect (this one is unchanged on purpose)."
}

function Invoke-Uninstall {
    [CmdletBinding()]
    param([switch]$Purge, [switch]$KeepPath)

    $ErrorActionPreference = 'Continue'

    $UserProfile = $env:USERPROFILE

    $BinDir = if ($env:ALPHACODE_INSTALL_DIR) {
        $env:ALPHACODE_INSTALL_DIR
    } elseif ($env:ALPHACODE_HOME) {
        Join-Path $env:ALPHACODE_HOME 'bin'
    } elseif ($env:ALPHACODE_BIN_DIR) {
        $env:ALPHACODE_BIN_DIR
    } elseif ($env:LOCALAPPDATA) {
        Join-Path $env:LOCALAPPDATA 'alphacode\bin'
    } else {
        # Fall back to the Programs location only if LOCALAPPDATA is unset.
        Join-Path $env:ProgramFiles 'Programs\alphacode\bin'
    }

    $target = Join-Path $BinDir 'alphacode.exe'

    # Defensive env fallbacks. If LOCALAPPDATA / APPDATA / USERPROFILE are
    # missing in this session (rare, but happens in constrained CI shells),
    # fall back to $null and let the per-candidate guards below skip empty
    # paths instead of crashing on Join-Path $null foo.
    $LocalAppData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { $null }
    $AppData      = if ($env:APPDATA)      { $env:APPDATA }      else { $null }

    $foundAny = $false

    function Remove-IfExists([string]$Path) {
        # -LiteralPath so paths containing [ ] ? are not interpreted as
        # wildcards by the FileSystem provider. We pass $Path positionally
        # and then append -Recurse / -Force as named switches so the binder
        # never has to guess between -LiteralPath and -Force. The inner
        # Remove-Item calls use -ErrorAction SilentlyContinue so individual
        # per-file errors (e.g. a locked alphacode.exe) do not spam the
        # console with the raw Red error stream - our catch block prints a
        # single, cleaner yellow "Skipped" line and appends to the script-
        # scoped $script:SkippedPaths list for the end-of-run summary.
        if (-not $Path) { return $false }
        if (-not (Test-Path -LiteralPath $Path)) { return $false }
        try {
            # Probe the item type defensively. Get-Item throws on files that
            # are open with FileShare::None (locked binary), so wrap the
            # probe too. If we cannot determine the type, default to file
            # semantics - the only case that would misclassify is an
            # ACL-locked directory, and the recursive Remove-Item will
            # surface its own error and the catch block will record it.
            $isDir = $false
            try {
                $item = Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
                if ($item) { $isDir = ($item -is [System.IO.DirectoryInfo]) }
            } catch { $isDir = $false }
            if ($isDir) {
                Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
            } else {
                Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
            }
            if (Test-Path -LiteralPath $Path) {
                # Remove-Item returned without throwing but the path is still
                # here (file was locked, ACL denied, etc.). Treat as skipped
                # but still mark "found" via the dedicated $foundAny variable
                # the caller manages, so we do not fall through to the
                # misleading "nothing to do" branch when the user just has
                # an in-use binary on disk.
                Write-Host "Skipped $Path (still present after removal attempt)" -ForegroundColor Yellow
                $script:SkippedPaths.Add($Path)
                return $false
            }
            Write-Host "Removed $Path"
            return $true
        } catch {
            $msg = $_.Exception.Message
            Write-Host "Skipped $Path ($msg)" -ForegroundColor Yellow
            $script:SkippedPaths.Add($Path)
            return $false
        }
    }

    # Remove launcher binary from all known locations.
    # Use a hashtable to deduplicate: $target is computed from the same
    # $BinDir default as one of the manual candidates below, and listing
    # it twice would cause confusing duplicate "Skipped" lines.
    $candidates = [ordered]@{}
    if ($target)              { $candidates[$target] = $true }
    if ($LocalAppData) {
        $candidates[(Join-Path "$LocalAppData\alphacode\bin"        'alphacode.exe')] = $true
        $candidates[(Join-Path "$LocalAppData\Programs\alphacode\bin" 'alphacode.exe')] = $true
    }
    if ($UserProfile) {
        $candidates[(Join-Path "$UserProfile\.local\bin" 'alphacode.exe')] = $true
    }

    foreach ($c in $candidates.Keys) {
        # Any path that exists counts as "found" - whether we successfully
        # removed it or it is locked. A locked binary is still an install
        # the user wants to know about; falling through to "nothing to do"
        # would be misleading and would silently skip every cleanup below.
        $existed = Test-Path -LiteralPath $c
        if ($existed) { $foundAny = $true }
        [void](Remove-IfExists -Path $c)
    }

    if (-not $foundAny) {
        Write-Host "No alphacode.exe found - nothing to do."
        return 0
    }

    # Remove local app data (%LOCALAPPDATA%\alphacode).
    if ($LocalAppData) {
        $localAlpha = Join-Path $LocalAppData 'alphacode'
        if (Remove-IfExists -Path $localAlpha) { $foundAny = $true }
    }

    # Remove app data (%APPDATA%\alphacode) if it exists.
    if ($AppData) {
        $roamingAlpha = Join-Path $AppData 'alphacode'
        if (Remove-IfExists -Path $roamingAlpha) { $foundAny = $true }
    }

    # Remove user home data (~/.alphacode) and config dir (~/.config/alphacode).
    if ($UserProfile) {
        $homeAlpha = Join-Path $UserProfile '.alphacode'
        if (Remove-IfExists -Path $homeAlpha) { $foundAny = $true }

        $configDir = Join-Path $UserProfile '.config\alphacode'
        if (Remove-IfExists -Path $configDir) { $foundAny = $true }
    }

    # Remove hotkey startup shortcut.
    if ($AppData) {
        $hotkeyLnk = Join-Path $AppData 'Microsoft\Windows\Start Menu\Programs\Startup\alphacode-hotkey.lnk'
        if (Remove-IfExists -Path $hotkeyLnk) { $foundAny = $true }
    }

    # Remove the GUI shortcuts and the registered application identity the
    # installer created (Start menu + Desktop). The Desktop may be redirected
    # (OneDrive), so ask the shell for its real location rather than assuming
    # %USERPROFILE%\Desktop.
    if ($AppData) {
        $guiLnk = Join-Path $AppData 'Microsoft\Windows\Start Menu\Programs\AlphaCode\AlphaCode.lnk'
        if (Remove-IfExists -Path $guiLnk) { $foundAny = $true }
        $guiFolder = Join-Path $AppData 'Microsoft\Windows\Start Menu\Programs\AlphaCode'
        if ((Test-Path -LiteralPath $guiFolder) -and
            -not (Get-ChildItem -LiteralPath $guiFolder -Force -ErrorAction SilentlyContinue)) {
            [void](Remove-IfExists -Path $guiFolder)
        }
        $desktop = $null
        try {
            $desktop = [System.Environment]::ExpandEnvironmentVariables(
                (Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders' -Name 'Desktop' -ErrorAction Stop).Desktop)
        } catch { }
        if (-not $desktop) { $desktop = Join-Path $UserProfile 'Desktop' }
        $desktopLnk = Join-Path $desktop 'AlphaCode.lnk'
        if (Remove-IfExists -Path $desktopLnk) { $foundAny = $true }
    }

    # Drop the per-user application registration (AppUserModelID / "Open with").
    foreach ($appName in @('AlphaCode.Desktop.exe', 'alphacode.exe')) {
        $regKey = "HKCU:\Software\Classes\Applications\$appName"
        try {
            if (Test-Path -LiteralPath $regKey) { Remove-Item -LiteralPath $regKey -Recurse -Force -ErrorAction Stop }
        } catch { Write-Host "Skipped registry cleanup for $appName" -ForegroundColor Yellow }
    }

    # Purge: also remove ~/.local/bin leftover.
    if ($Purge -and $UserProfile) {
        $localBin = Join-Path $UserProfile '.local\bin\alphacode.exe'
        if (Remove-IfExists -Path $localBin) { $foundAny = $true }
    }

    if (-not $KeepPath) {
        Remove-AlphacodeFromUserPath -BinDir $BinDir
    }

    Write-Host ""
    Write-Host "Alphacode has been uninstalled."

    if ($script:SkippedPaths.Count -gt 0) {
        Write-Host ""
        Write-Host "[warn] $($script:SkippedPaths.Count) path(s) could not be removed (likely in use):" -ForegroundColor Yellow
        foreach ($s in $script:SkippedPaths) { Write-Host "  - $s" -ForegroundColor Yellow }
        Write-Host "Close any running alphacode / TUI sessions and rerun the uninstaller." -ForegroundColor Yellow
        return 1
    }

    return 0
}

# Top-level: invoke the function and capture its return code in the host
# without calling `exit` (which would terminate the user's PowerShell
# session when this script is run via `iwr ... | iex`).
$rc = Invoke-Uninstall -Purge:$Purge -KeepPath:$KeepPath
$global:LASTEXITCODE = $rc
return $rc
