[CmdletBinding()]
param([switch]$Purge)

# uninstall.ps1 - remove Alphacode from a Windows machine.
#
# Invocation patterns:
#   iwr -useb https://raw.githubusercontent.com/dragonked2/alphacode/main/scripts/uninstall.ps1 | iex
#   .\uninstall.ps1            # local file
#   .\uninstall.ps1 -Purge     # also remove ~/.local/bin\alphacode.exe
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

function Invoke-Uninstall {
    [CmdletBinding()]
    param([switch]$Purge)

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

    # Purge: also remove ~/.local/bin leftover.
    if ($Purge -and $UserProfile) {
        $localBin = Join-Path $UserProfile '.local\bin\alphacode.exe'
        if (Remove-IfExists -Path $localBin) { $foundAny = $true }
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
$rc = Invoke-Uninstall -Purge:$Purge
$global:LASTEXITCODE = $rc
return $rc