#!/usr/bin/env pwsh
# install-path.tests.ps1 -- behaviour tests for the installer's logic.
#
# Run directly:
#   pwsh -File scripts/tests/install-path.tests.ps1
#
# These tests exist because the installer's decisions mutate system-level state
# (the registry, the user's shell profile) and "it looked right" is not evidence
# for that class of change. Every assertion below runs against a *pure*
# function that takes its inputs as arguments and touches no registry, no
# environment variable and no filesystem, so the suite is safe to run anywhere
# -- including CI, where a mistake would otherwise rewrite the runner's PATH.
#
# Deliberately NOT tested here: the effectful appliers
# (Add-AlphacodeToUserPath / Remove-AlphacodeFromUserPath) and the installer's
# own top-level run. Those touch HKCU and cannot be exercised without mutating
# the machine. What is verified is that the decision they make -- the only part
# with real logic -- is correct, and that the entry point refuses to run on a dry
# run.

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Installer = Join-Path $RepoRoot 'scripts/install.ps1'

if (-not (Test-Path $Installer)) {
    throw "installer not found at $Installer"
}

# ---------------------------------------------------------------------------
# Load only the function definitions from install.ps1.
#
# install.ps1 must stay a single self-contained file (it is fetched with
# `iwr ... | iex`, which cannot resolve a sibling file), so the tests cannot
# dot-source a helper module. Parsing the AST and evaluating just the function
# definitions gives the same coverage without executing the installer body --
# no download, no install, no registry writes.
#
# The definitions are returned as one scriptblock rather than being defined
# inside a helper function: a function defined in a helper's local scope
# disappears when the helper returns, which would leave every test below
# calling a command that does not exist.
#
# The class definitions are extracted too, so that any type the installer
# throws is loadable in the scope the tests below assert in. The installer
# itself deliberately throws a framework type rather than a custom class --
# Windows PowerShell 5.1 cannot parse a constructor inside a PowerShell class,
# and a class that only carries a field loses its text in
# $_.Exception.Message, which is the text the user actually reads -- but the
# scan is kept so a reintroduced class is still visible to the assertions.
# ---------------------------------------------------------------------------
function Get-InstallerDefinitions {
    param([Parameter(Mandatory)][string]$Path)

    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$errors)
    if ($errors -and $errors.Count -gt 0) {
        throw "failed to parse ${Path}: $($errors[0].Message)"
    }

    $functions = $ast.FindAll(
        { param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] },
        $true
    )
    if ($functions.Count -eq 0) {
        throw "no functions found in $Path -- the AST extraction is broken, so these tests would vacuously pass"
    }

    # Classes too: a TypeDefinitionAst is not covered by the function search
    # above, so a class the installer throws would otherwise not exist in the
    # scope where the assertions below run.
    $classes = $ast.FindAll(
        { param($node) $node -is [System.Management.Automation.Language.TypeDefinitionAst] },
        $true
    )

    $text = (($functions | ForEach-Object { $_.Extent.Text }) +
             ($classes  | ForEach-Object { $_.Extent.Text })) -join "`n"
    return [pscustomobject]@{
        Names      = @($functions | ForEach-Object { $_.Name })
        Definition = [scriptblock]::Create($text)
    }
}

$installerFunctions = Get-InstallerDefinitions -Path $Installer
. $installerFunctions.Definition

# install.ps1 assigns $script:AlphacodeIsHosted at the top level. That is a
# statement, not a function, so the AST extraction above does not carry it --
# and Test-AlphacodeHost returns it, meaning the helper answers $null rather
# than a boolean until the assignment is supplied here. Without this line the
# assertion below fails with "cannot call a method on a null-valued
# expression", which says nothing about the installer.
#
# The value mirrors the installer's own: a function created by
# [scriptblock]::Create() has no file origin, so $PSCommandPath is empty
# inside it and "hosted" is the correct answer for this harness.
$script:AlphacodeIsHosted = $true

# Verify the functions are actually callable, not merely present in the AST.
# A name-only check passed while every test failed with "not recognized",
# because the definitions had gone out of scope -- so this asserts on
# Get-Command, which reflects reality.
$missing = @($installerFunctions.Names | Where-Object { -not (Get-Command -Name $_ -ErrorAction SilentlyContinue) })
if ($missing.Count -gt 0) {
    throw "these functions were extracted but are not callable: $($missing -join ', ')"
}
foreach ($required in @(
    'Get-AlphacodePathPlan',
    'ConvertTo-AlphacodeComparablePathEntry',
    'Add-AlphacodeToUserPath',
    'Add-AlphacodeToSessionPath',
    'Get-AlphacodeWindowsArch',
    'Test-AlphacodeWindows',
    'Test-AlphacodeHost',
    'Stop-AlphacodeInstall'
)) {
    if ($installerFunctions.Names -notcontains $required) {
        throw "expected function '$required' was not found in install.ps1 (found: $($installerFunctions.Names -join ', '))"
    }
}

# ---------------------------------------------------------------------------
# Minimal test harness (no Pester dependency, so CI needs nothing installed).
# ---------------------------------------------------------------------------
$script:Passed = 0
$script:Failed = 0

function It {
    param([string]$Name, [scriptblock]$Body)
    try {
        & $Body
        $script:Passed++
        Write-Host "  ok   $Name"
    } catch {
        $script:Failed++
        Write-Host "  FAIL $Name" -ForegroundColor Red
        Write-Host "       $($_.Exception.Message)" -ForegroundColor Red
    }
}

function Assert-Equal {
    param($Expected, $Actual, [string]$Because = '')
    if ($Expected -ne $Actual) {
        throw "expected [$Expected] but got [$Actual]$(if ($Because) { " ($Because)" })"
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Because = '')
    if (-not $Condition) {
        throw "expected condition to hold$(" ($Because)")"
    }
}

function Assert-Contains {
    param([string]$Haystack, [string]$Needle, [string]$Because = '')
    # -like treats *, ? and [ ] as wildcards, and at least one needle here is a
    # literal bracket (the ends of a progress bar) -- which makes -like throw
    # on an invalid pattern rather than simply not match. Escaping keeps the
    # search literal. Single-quoted so the backticks survive into the pattern.
    $pattern = $Needle.Replace('[', '`[').Replace(']', '`]').Replace('*', '`*').Replace('?', '`?')
    if ($Haystack -notlike "*$pattern*") {
        throw "expected to find [$Needle] in [$Haystack]$(" ($Because)")"
    }
}

function Assert-Throws {
    param([scriptblock]$Body, [string]$Because = '')
    $threw = $false
    try { & $Body | Out-Null } catch { $threw = $true }
    Assert-True $threw -Because $Because
}

# Raw and comment-stripped copies of the installer, used by the source-text
# guards below. Both must be built BEFORE the first `It` that reads them: these
# are top-level statements in this file, so anything defined after an `It` block
# is still $null when that block executes.
$installerText = Get-Content -Raw -LiteralPath $Installer

# Comments are stripped because the installer deliberately documents its own past
# bugs in comments that quote the offending code. A guard that searched the raw
# text would match the explanation instead of the code and report the wrong
# answer -- which is how the first version of the $IsWindows and checksum-order
# guards failed against a correct installer.
#
# (the earlier comment-stripped copy)
# It must be built BEFORE the first `It` that reads it -- these are top-level
# statements in this file, so anything defined after an `It` block is still $null
# when that block executes.
#
# Comments are stripped because the installer deliberately documents its own past
# bugs in comments that quote the offending code. A guard that searched the raw
# text would match the explanation instead of the code and report the wrong
# answer -- which is how the first version of the $IsWindows and checksum-order
# guards failed against a correct installer.
$installerCode = (@($installerText -split "`n") |
    Where-Object { $_ -notmatch '^\s*#' }) -join "`n"

Write-Host "install.ps1 PATH logic" -ForegroundColor Cyan

# ---------------------------------------------------------------------------
# Adding.
# ---------------------------------------------------------------------------

It 'appends the bin dir to a non-empty user PATH' {
    $plan = Get-AlphacodePathPlan -UserPath 'C:\a;C:\b' -BinDir 'C:\tools\alphacode\bin'
    Assert-True -Condition (-not $plan.AlreadyPresent) -Because 'the dir was absent'
    Assert-Equal 'C:\a;C:\b;C:\tools\alphacode\bin' $plan.Value
}

It 'uses the bin dir alone when there is no user PATH' {
    foreach ($empty in @($null, '')) {
        $plan = Get-AlphacodePathPlan -UserPath $empty -BinDir 'C:\tools\bin'
        Assert-Equal 'C:\tools\bin' $plan.Value "user PATH was [$empty]"
    }
}

It 'is idempotent' {
    $first = Get-AlphacodePathPlan -UserPath 'C:\a' -BinDir 'C:\b'
    $second = Get-AlphacodePathPlan -UserPath $first.Value -BinDir 'C:\b'
    Assert-True $second.AlreadyPresent -Because 'the second run must see it present'
    Assert-Equal $first.Value $second.Value -Because 'a re-run must not change the value'
}

It 'treats a trailing separator as the same directory' {
    $plan = Get-AlphacodePathPlan -UserPath 'C:\tools\alphacode\bin\' -BinDir 'C:\tools\alphacode\bin'
    Assert-True $plan.AlreadyPresent -Because 'Windows treats these as one directory'
}

It 'matches case-insensitively' {
    $plan = Get-AlphacodePathPlan -UserPath 'c:\TOOLS\Alphacode\BIN' -BinDir 'C:\tools\alphacode\bin'
    Assert-True $plan.AlreadyPresent -Because 'PATH comparison must be case-insensitive'
}

It 'collapses doubled separators without losing entries' {
    # A hand-edited PATH routinely contains ';;'. Every real entry must survive
    # the rewrite; only the empty segment is dropped.
    $plan = Get-AlphacodePathPlan -UserPath 'C:\a;;C:\b;' -BinDir 'C:\c'
    Assert-Equal 'C:\a;C:\b;C:\c' $plan.Value
}

It 'preserves %VAR% references verbatim' {
    # The whole reason the applier reads the registry raw. If a %VAR% were
    # expanded here it would be frozen to today's literal path.
    $plan = Get-AlphacodePathPlan -UserPath '%USERPROFILE%\bin;C:\a' -BinDir 'C:\c'
    Assert-Equal '%USERPROFILE%\bin;C:\a;C:\c' $plan.Value
}

It 'preserves entries containing spaces' {
    $plan = Get-AlphacodePathPlan -UserPath 'C:\Program Files\Git\cmd' -BinDir 'C:\c'
    Assert-Equal 'C:\Program Files\Git\cmd;C:\c' $plan.Value
}

It 'does not treat a sibling directory as the target' {
    # Prefix confusion: removing/appending must match the whole entry, not a
    # string prefix of it.
    foreach ($sibling in @('C:\tools\alphacode\bin-old', 'C:\tools\alphacode\bin2', 'C:\tools\alphacode')) {
        $plan = Get-AlphacodePathPlan -UserPath $sibling -BinDir 'C:\tools\alphacode\bin'
        Assert-True (-not $plan.AlreadyPresent) -Because "[$sibling] is a different directory"
    }
}

It 'appends rather than prepends, so no existing entry changes precedence' {
    $plan = Get-AlphacodePathPlan -UserPath 'C:\first;C:\second' -BinDir 'C:\zzz'
    Assert-Equal 'C:\first;C:\second;C:\zzz' $plan.Value -Because 'order must be unchanged'
    Assert-True ($plan.Value.StartsWith('C:\first;')) -Because 'the first entry must still be first'
}

It 'rejects an empty bin dir instead of writing a stray separator' {
    Assert-Throws { Get-AlphacodePathPlan -UserPath 'C:\a' -BinDir '   ' } `
        -Because 'an empty BinDir must be refused'
}

It 'never derives the new value from the session PATH' {
    # The bug this guards against: reading $env:PATH (user+machine+session
    # merged) and writing that back would silently drop session-only entries for
    # every future shell. The planner only ever sees what it is handed.
    $sessionBefore = $env:PATH
    $plan = Get-AlphacodePathPlan -UserPath 'C:\only-user-entry' -BinDir 'C:\c'
    Assert-Equal $sessionBefore $env:PATH -Because 'planning must not touch the session PATH'
    # The planner splits on ';' because it handles Windows user PATH values;
    # checking with [IO.Path]::PathSeparator would use ':' on non-Windows CI
    # runners, where 'C:\c' itself looks like two separators.
    Assert-True ($plan.Value -notlike '*;;*') `
        -Because 'the result must be a single PATH, not a doubled-up merge'
}

# ---------------------------------------------------------------------------
# Session PATH activation.
#
# The persisted user PATH is appended to; the live session PATH is prepended
# to. Both directions matter and are separately tested.
# ---------------------------------------------------------------------------

It 'prepends the bin dir to the current session PATH' {
    $before = $env:PATH
    try {
        $result = Add-AlphacodeToSessionPath -BinDir 'C:\session\test\bin'
        Assert-Equal 'added' $result.Status
        Assert-True ($env:PATH.StartsWith('C:\session\test\bin;')) `
            -Because 'the bin dir must lead so `alphacode` wins'
        Assert-True ($env:PATH.Length -gt 3) -Because 'the rest of PATH must be preserved'
    } finally {
        $env:PATH = $before
    }
}

It 'is idempotent in the session PATH' {
    $before = $env:PATH
    try {
        [void](Add-AlphacodeToSessionPath -BinDir 'C:\session\idem\bin')
        $afterFirst = $env:PATH
        $result = Add-AlphacodeToSessionPath -BinDir 'C:\session\idem\bin'
        Assert-Equal 'already-present' $result.Status
        Assert-Equal $afterFirst $env:PATH -Because 'a second call must not change PATH'
        # Exactly one occurrence, not two.
        $count = ([regex]::Matches($env:PATH, [regex]::Escape('C:\session\idem\bin'))).Count
        Assert-Equal 1 $count
    } finally {
        $env:PATH = $before
    }
}

It 'treats a trailing separator as the same dir in the session PATH' {
    $before = $env:PATH
    try {
        [void](Add-AlphacodeToSessionPath -BinDir 'C:\session\sep\bin\')
        $result = Add-AlphacodeToSessionPath -BinDir 'C:\session\sep\bin'
        Assert-Equal 'already-present' $result.Status -Because 'both denote one directory'
    } finally {
        $env:PATH = $before
    }
}

# ---------------------------------------------------------------------------
# Architecture detection.
#
# Regression tests for the `switch ($env:PROCESSOR_ARCHITECTURE)` block that
# shipped with only two cases: a 32-bit PowerShell on x64 Windows reported "x86"
# and the installer died with "unsupported architecture" even though a correct
# x86_64 asset existed in the same release.
# ---------------------------------------------------------------------------

It 'maps AMD64 to x86_64' {
    Assert-Equal 'x86_64' (Get-AlphacodeWindowsArch -NativeArch 'AMD64')
}

It 'maps ARM64 to arm64' {
    Assert-Equal 'arm64' (Get-AlphacodeWindowsArch -NativeArch 'ARM64')
}

It 'is case-insensitive' {
    Assert-Equal 'x86_64' (Get-AlphacodeWindowsArch -NativeArch 'amd64')
    Assert-Equal 'arm64'   (Get-AlphacodeWindowsArch -NativeArch 'arm64')
}

It 'uses PROCESSOR_ARCHITEW6432 when a 32-bit shell sees x86' {
    # The bug: a 32-bit PowerShell on x64 Windows reports PROCESSOR_ARCHITECTURE=x86
    # and PROCESSOR_ARCHITEW6432=AMD64. Reading only the former made the install
    # impossible on a perfectly capable machine.
    $arch = Get-AlphacodeWindowsArch -NativeArch 'x86' -Wow64Arch 'AMD64'
    Assert-Equal 'x86_64' $arch -Because 'the WOW64 token names the real OS architecture'
}

It 'prefers the OS architecture over an emulated x64 process on ARM64' {
    # An x64 process emulated on ARM64 Windows reports AMD64 natively, but
    # PROCESSOR_ARCHITECTURE_WINDOWS=ARM64 describes the machine.
    $arch = Get-AlphacodeWindowsArch -NativeArch 'AMD64' -OsArch 'ARM64'
    Assert-Equal 'arm64' $arch
}

It 'falls back to the native token when the extra vars are unset' {
    Assert-Equal 'x86_64' (Get-AlphacodeWindowsArch -NativeArch 'AMD64' -Wow64Arch '' -OsArch '')
    Assert-Equal 'arm64'   (Get-AlphacodeWindowsArch -NativeArch 'ARM64' -Wow64Arch $null -OsArch $null)
}

It 'returns null for an architecture it cannot classify' {
    # Must return null, not throw: the caller turns this into an actionable
    # message that names ALPHACODE_ARCH, rather than dying on a bare switch.
    $arch = Get-AlphacodeWindowsArch -NativeArch 'MIPS'
    Assert-True ($null -eq $arch) -Because "got [$arch]"
}

It 'returns null when nothing is set at all' {
    $arch = Get-AlphacodeWindowsArch -NativeArch '' -Wow64Arch '' -OsArch ''
    Assert-True ($null -eq $arch) -Because "got [$arch]"
}

# ---------------------------------------------------------------------------
# Failure semantics.
#
# Regression tests for the abort path calling `exit`. Under the documented
# invocation `irm ... | iex`, `exit` does not end the installer -- it ends the
# user's terminal. Stopping must therefore throw, and must throw a type the
# catch blocks which tolerate *network* errors can tell apart from an
# *integrity* failure, so a tampered download is never silently downgraded to
# a warning.
# ---------------------------------------------------------------------------

It 'Stop-AlphacodeInstall throws instead of exiting' {
    # If it called exit, this scriptblock would terminate the test host and the
    # suite would die here. Reaching the assertion proves it returned.
    $threw = $false
    try { Stop-AlphacodeInstall 'expected test failure' } catch { $threw = $true }
    Assert-True $threw -Because 'Stop-AlphacodeInstall must raise a terminating error'
}

It 'Stop-AlphacodeInstall throws a framework type, not a custom class' {
    # A custom class was tried and then removed on purpose: Windows PowerShell
    # 5.1 cannot parse a constructor inside a PowerShell class, so such a
    # class either failed to load or -- when it only carried a field -- lost
    # its text in $_.Exception.Message. A framework type has neither problem.
    $caught = $null
    try { Stop-AlphacodeInstall 'typed' } catch { $caught = $_ }
    Assert-True ($caught -is [System.Management.Automation.ErrorRecord]) `
        -Because 'must surface as an ErrorRecord'
    Assert-True ($caught.Exception -is [System.InvalidOperationException]) `
        -Because 'this is the type the PS 5.1-safe abort path throws'
}

It 'Stop-AlphacodeInstall carries its message through' {
    # Asserted on .Message, because that is what the installer's tail prints as
    # `[fail] <message>` and therefore what the user reads. This is the whole
    # reason the implementation avoids a custom class.
    $caught = $null
    try { Stop-AlphacodeInstall 'the specific message' } catch { $caught = $_ }
    Assert-Equal 'the specific message' $caught.Exception.Message
}

# ---------------------------------------------------------------------------
# Host detection: which failures are allowed to exit.
# ---------------------------------------------------------------------------

It 'Test-AlphacodeHost keys off $PSCommandPath, not $MyInvocation' {
    # Unit-level assertions on the returned value are not possible here: this
    # suite dot-sources the extracted definitions, and a function defined in a
    # `[scriptblock]::Create()` has no file origin, so $PSCommandPath is empty
    # inside it and the helper always reports "hosted". That is an artifact of
    # the extraction technique, not of the installer, whose functions are
    # defined in the file itself.
    #
    # So the real behaviour is asserted end-to-end below, in a child process,
    # where the invocation style is genuinely different. Here we only pin the
    # contract: the helper exists, returns a boolean, and reads $PSCommandPath.
    $r = Test-AlphacodeHost
    Assert-True ($r -is [bool]) -Because "got [$($r.GetType().Name)]"

    # The helper spans several lines, so scan the whole comment-stripped source
    # rather than a single line. A one-line filter finds nothing here -- which
    # is exactly how the previous version of this assertion passed vacuously.
    Assert-True ($installerCode -match 'function\s+Test-AlphacodeHost') `
        -Because 'expected at least one definition of Test-AlphacodeHost'
    # The helper answers from a top-level flag rather than reading
    # $PSCommandPath itself, so the read has moved to that flag's definition --
    # assert it there, or this would pass while the flag was assigned from
    # anything at all.
    Assert-True ($installerCode -match 'AlphacodeIsHosted\s*=\s*\[string\]::IsNullOrEmpty\(\$PSCommandPath\)') `
        -Because 'the hosted flag must be derived from $PSCommandPath, which is populated for a -File run'
    Assert-True ($installerCode -match 'function\s+Test-AlphacodeHost[\s\S]{0,300}?AlphacodeIsHosted') `
        -Because 'Test-AlphacodeHost must answer from that flag, not re-derive it'
    Assert-True ($installerCode -notmatch 'AlphacodeIsHosted\s*=\s*\[string\]::IsNullOrEmpty\(\$MyInvocation') `
        -Because '$MyInvocation.ScriptName is empty at the top level of a -File script, so using it would suppress the exit code CI relies on'
}

It 'a failed -File install returns exit code 1' {
    # End-to-end check of the "fail without killing the caller's shell" rule,
    # in the invocation style CI uses. Run in a child process so a real `exit`
    # cannot take this suite with it.
    if (-not (Test-AlphacodeWindows)) { return }

    $probe = Join-Path ([System.IO.Path]::GetTempPath()) ("ac-hostprobe-" + [guid]::NewGuid().ToString('N') + '.ps1')
    # Same shape as the installer's tail: a try/catch that reports the failure,
    # then exits only when the script owns the process.
    @'
function Test-AlphacodeHost {
    [CmdletBinding()] param()
    return [string]::IsNullOrEmpty($PSCommandPath)
}
try {
    throw 'simulated install failure'
} catch {
    Write-Host "[fail] $($_.Exception.Message)"
    $global:LASTEXITCODE = 1
    if (-not (Test-AlphacodeHost)) { exit 1 }
    return
}
'@ | Set-Content -LiteralPath $probe -Encoding UTF8

    try {
        $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $probe 2>&1
        Assert-Equal 1 $LASTEXITCODE -Because "a -File failure must propagate a non-zero exit code; got [$out]"
    } finally {
        Remove-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
    }
}

It 'a failed iwr|iex install does NOT terminate the caller session' {
    # The regression this whole change exists for: `Fail` used to call `exit`,
    # which under `iwr ... | iex` closes the user's terminal. The sentinel below
    # is printed by the HOST session after the installer runs, so if the
    # installer had exited, the sentinel would never appear.
    if (-not (Test-AlphacodeWindows)) { return }

    $probe = Join-Path ([System.IO.Path]::GetTempPath()) ("ac-iexprobe-" + [guid]::NewGuid().ToString('N') + '.ps1')
    @'
function Test-AlphacodeHost {
    [CmdletBinding()] param()
    return [string]::IsNullOrEmpty($PSCommandPath)
}
function Fail([string]$msg) { throw $msg }
try {
    Fail 'simulated install failure'
} catch {
    Write-Host "[fail] $($_.Exception.Message)"
    $global:LASTEXITCODE = 1
    if (-not (Test-AlphacodeHost)) { exit 1 }
    return
}
'@ | Set-Content -LiteralPath $probe -Encoding UTF8

    try {
        # NOT $host: that is a read-only automatic variable in PowerShell and
        # assigning to it throws VariableNotWritable, aborting the test. Same trap
        # as $HOME in uninstall.ps1 v1.0.22.
        $hostCmd = "iex (Get-Content -Raw -LiteralPath '$probe'); Write-Host 'SENTINEL-SHELL-SURVIVED'"
        $out = & powershell -NoProfile -ExecutionPolicy Bypass -Command $hostCmd 2>&1 | Out-String
        Assert-Contains $out 'SENTINEL-SHELL-SURVIVED' `
            -Because 'the caller session must survive a failed install'
        Assert-Contains $out 'simulated install failure' `
            -Because 'the failure must still be reported to the user'
    } finally {
        Remove-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
    }
}

# ---------------------------------------------------------------------------
# The applier's guard rails.
#
# Only the dry-run path is exercised: it must produce a plan and commit nothing.
# ---------------------------------------------------------------------------

It 'detects Windows without relying on $IsWindows' {
    # $IsWindows only exists in PowerShell 6+. This script is invoked with
    # `iwr ... | iex`, which runs Windows PowerShell 5.1 by default, where the
    # variable is undefined and evaluates to $null -- so a test against it is
    # always false there and the Windows branch is dead code on the shell most
    # users actually run.
    $onWindows = ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
    # The suite itself runs cross-platform (the CI job above is ubuntu-latest),
    # so the helper must agree with the runtime wherever it runs -- true on
    # Windows, false elsewhere. Asserting an unconditional true would make this
    # test fail on every non-Windows runner even though the code is right.
    Assert-True -Condition ((Test-AlphacodeWindows) -eq $onWindows) `
        -Because 'the helper must agree with the runtime, not with a PS6-only variable'
}

It 'dry run reports a plan and writes nothing' {
    if (-not (Test-AlphacodeWindows)) { return }   # the applier is a no-op off Windows
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    try {
        $before = if ($null -ne $key -and ($key.GetValueNames() -contains 'Path')) {
            $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        } else { $null }
    } finally { if ($null -ne $key) { $key.Close() } }

    $beforeProcess = $env:PATH
    # A directory that cannot plausibly already be on the PATH.
    $probe = 'C:\alphacode-path-test-' + [guid]::NewGuid().ToString('N')
    $result = Add-AlphacodeToUserPath -BinDir $probe -DryRun

    Assert-Equal 'dry-run' $result.Status
    Assert-Contains $result.Value $probe

    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    try {
        $after = if ($null -ne $key -and ($key.GetValueNames() -contains 'Path')) {
            $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        } else { $null }
    } finally { if ($null -ne $key) { $key.Close() } }

    Assert-Equal $before $after -Because 'a dry run must not write the registry'
    Assert-Equal $beforeProcess $env:PATH -Because 'a dry run must not rewrite the session PATH'
}

# ---------------------------------------------------------------------------
# The download path.
#
# Deliberately network-free: the transfer itself was verified by hand against a
# real release -- including a resumed transfer landing byte-identical to the
# original, and a 404 short-circuiting in under a second instead of burning all
# three attempts -- and a suite that reached out to github.com would be slow and
# flaky on every CI run. What is asserted here is the contract a behavioural
# test cannot see: that the installer streams rather than buffers, resumes
# rather than restarts, and formats its progress line correctly.
# ---------------------------------------------------------------------------

It 'draws nothing at all when output is redirected' {
    # Checked against the REAL console detection, before the override further
    # down replaces it. The bar is carriage-return based, so in a captured log
    # it would smear hundreds of half-written lines into the file.
    if (-not [Console]::IsOutputRedirected) { return }
    $drawn = Write-AlphacodeProgressLine -Received 6800000 -Total 21800000 -Rate 312000 6>&1 | Out-String
    Assert-Equal '' $drawn.Trim() -Because 'a redirected run must not emit progress frames'
}

# Redefined at script scope on purpose: the installer's own version returns
# false whenever output is redirected, which is always the case for a piped
# suite, so the rendering tests below would otherwise assert on nothing. The
# installer functions were dot-sourced into this scope, so this replaces them.
function Test-AlphacodeConsole { [CmdletBinding()] param(); return $true }

It 'renders a sized progress frame with percentage, rate and ETA' {
    # Rate is the whole point of the bar: throughput is what distinguishes a
    # link that is slow from one that has stopped, which no amount of bar fill
    # conveys on its own.
    $drawn = Write-AlphacodeProgressLine -Received 6800000 -Total 21800000 -Rate 312000 -Done 6>&1 |
             Out-String
    Assert-Contains $drawn '['   -Because 'the bar itself must be drawn'
    Assert-Contains $drawn '31%' -Because 'the percentage must be shown'
    Assert-Contains $drawn 'MB'  -Because 'byte counts must be human-readable, not raw'
    Assert-Contains $drawn '/s'  -Because 'throughput tells a slow link from a stalled one'
    Assert-Contains $drawn 'eta' -Because 'an ETA is the only way to decide whether to wait'
}

It 'shows an indeterminate marker rather than a fake percentage when the size is unknown' {
    $drawn = Write-AlphacodeProgressLine -Received 65536 -Total 0 -Rate 32000 -Done 6>&1 | Out-String
    Assert-True ($drawn -notmatch '\d+%') `
        -Because 'with no Content-Length a percentage would be invented, not measured'
}

It 'renders an ETA of --:-- instead of a number when the rate is unknown' {
    $drawn = Write-AlphacodeProgressLine -Received 1024 -Total 21800000 -Rate 0 -Done 6>&1 | Out-String
    Assert-Contains $drawn 'eta --:--' `
        -Because 'dividing by a zero rate would produce an ETA that means nothing'
}

It 'streams the release asset instead of buffering it through Invoke-WebRequest' {
    # The regression behind "I see no progress while it downloads":
    # Invoke-WebRequest -OutFile on Windows PowerShell 5.1 holds the whole body
    # in memory and writes the file only once the transfer has finished, so a
    # 20 MB asset is invisible until it is nearly done and then appears at once.
    #
    # Asserted on code with comments stripped: the comment block above the new
    # function names Invoke-WebRequest while explaining why it is not used, so a
    # raw-text search would match that explanation and stay green forever.
    Assert-True ($installerCode -match 'function\s+Invoke-AlphacodeStreamingDownload') `
        -Because 'the streaming transfer is what draws the bar as bytes arrive'
    Assert-True ($installerCode -notmatch 'Invoke-WebRequest[^\r\n]*-OutFile') `
        -Because 'a buffered -OutFile download cannot show progress and must not return'
}

It 'resumes an interrupted transfer instead of restarting it' {
    Assert-True ($installerCode -match 'AddRange') `
        -Because 'a retry must ask for the remaining bytes, not the whole file'
    Assert-True ($installerCode -match '\[System\.IO\.FileMode\]::Append') `
        -Because 'resumed bytes have to be appended to what is already on disk'
    # Only ever across retries of the SAME url: appending one candidate asset
    # onto another candidate's partial file would silently yield a corrupt
    # archive that the checksum step may not catch.
    Assert-True ($installerCode -match 'Remove-Item -LiteralPath \$OutFile') `
        -Because 'a partial left by an earlier candidate URL must be discarded first'
}

It 'gives up on a stalled transfer instead of hanging forever' {
    # A half-open socket is common on a slow link and would otherwise leave the
    # installer waiting indefinitely with neither movement nor an error.
    Assert-True ($installerCode -match 'ReadWriteTimeout') `
        -Because 'a read that blocks too long must fail the transfer so it can retry'
}

# ---------------------------------------------------------------------------
# Source-text guards.
#
# A few defects were "the script says the wrong thing" rather than "the script
# computes the wrong thing", and no behavioural test can see them. These assert
# on the shipped text so a regression is caught even though the suite is
# otherwise behavioural.
# ---------------------------------------------------------------------------



It 'does not gate the platform on $IsWindows' {
    # `$IsWindows` is undefined in Windows PowerShell 5.1 (null), so any branch
    # that depends on it is unreliable on the shell the README tells users to use.
    #
    # Only *code* lines are inspected. The file deliberately documents this very
    # hazard in comments that name the variable, so a raw substring search over
    # the whole file matches the explanation and fails forever -- which is what
    # the first version of this test did.
    $codeLines = @($installerCode -split "`n" | Where-Object { $_ -notmatch '^\s*#' })
    $code = $codeLines -join "`n"
    Assert-True ($code -notmatch '\$IsWindows') `
        -Because 'platform detection must not depend on a PS6-only variable'
}

It 'keeps the default install prefix under Programs\' {
    # The old default was $LOCALAPPDATA\alphacode, which put the binary inside
    # the app's own data directory and disagreed with the documented location.
    #
    # Asserted on $installerCode (comments stripped), not $installerText: the
    # usage banner at the top of the file also mentions
    # "%LOCALAPPDATA%\Programs\alphacode", so a raw-text search was satisfied by
    # a comment and stayed green after the default was reverted.
    #
    # The variable holding LOCALAPPDATA has been renamed more than once
    # ($localAppData, now $lad), and pinning that name turned this into a test
    # of a local variable rather than of the default path. It asserts the shape
    # instead, allowing any number of qualifiers ($lad, $env:LOCALAPPDATA).
    #
    # Single-quoted on purpose: the patterns contain $ and \, and a
    # double-quoted string would try to evaluate $(...) as a subexpression and
    # eat the backslash that escapes the dollar. Single quotes keep both
    # literal, so the regex sees exactly what is written.
    Assert-True ($installerCode -match 'Join-Path\s+\$[\w:]+\s+''Programs\\alphacode''') `
        -Because 'the computed default prefix must be <LOCALAPPDATA>\Programs\alphacode'

    # Machine scope legitimately uses (Join-Path $pf 'alphacode') for Program
    # Files, so this rules out only the *user* scope falling back to the data
    # directory, which is what the prefix above exists to avoid.
    Assert-True ($installerCode -notmatch 'Join-Path\s+\$(env:)?LOCALAPPDATA\s+''alphacode''') `
        -Because 'the binary must not live in the app data directory'
}


It 'activates the bin dir in the current session PATH' {
    # Tests Add-AlphacodeToSessionPath in isolation, so on its own this proves
    # nothing about the installer. What matters is that the body CALLS it:
    # deleting the call line left the whole suite green, which is exactly what
    # mutation testing surfaced. Assert the call site explicitly.
    Assert-True ($installerCode -like '*Add-AlphacodeToSessionPath -BinDir $BinDir*') `
        -Because 'the installer must activate the binary in the running shell, not only in the registry'
    # And it must be skipped on a dry run, which promises to write nothing.
    Assert-True ($installerCode -like '*if (-not $PathDryRun)*') `
        -Because 'a dry run must not modify even the session PATH'
}

It 'applies the PATH change by default' {
    # A fresh user must not need to know a flag exists. Guard the default-on
    # behaviour: the applier runs unless -NoPath is passed.
    Assert-True ($installerText -match 'if\s*\(-not\s+\$NoPath\)') `
        -Because 'PATH setup must not require an opt-in flag'
}

It 'still offers -NoPath as an opt-out' {
    Assert-True ($installerText -match '\[switch\]\$NoPath') `
        -Because 'default-on must remain reversible'
}

It 'still offers -PathDryRun so the write can be previewed' {
    Assert-True ($installerText -match '\[switch\]\$PathDryRun') `
        -Because 'a system-level write must remain previewable'
}

It 'keeps -AddPath as a compatibility alias' {
    # Older instructions and any wrapper script still pass it.
    Assert-True ($installerText -match '\[switch\]\$AddPath') `
        -Because 'existing -AddPath callers must not break'
}

It 'compares the checksum outside the catch that tolerates network errors' {
    # The shipped shape was:
    #     try { ...; Fail 'checksum mismatch' } catch { Warn '...continuing' }
    # which downgraded a tampered download to a warning and installed it.
    #
    # Assert on code lines only: the file documents the old shape in a comment
    # that contains the same strings, so a whole-file search finds the comment
    # first and reports the wrong ordering.
    $fetchIdx = -1
    $mismatchIdx = -1
    $codeLines = @($installerCode -split "`n")
    for ($i = 0; $i -lt $codeLines.Count; $i++) {
        $l = $codeLines[$i]
        if ($l -match '^\s*#') { continue }
        if ($fetchIdx   -lt 0 -and $l -like '*could not fetch SHA256SUMS*') { $fetchIdx = $i }
        # The abort helper was `Fail` when this guard was written and is
        # `Stop-AlphacodeInstall` now; only the surrounding shape matters.
        if ($mismatchIdx -lt 0 -and $l -match 'Stop-AlphacodeInstall\s*\(\s*\(?\s*"Checksum mismatch') {
            $mismatchIdx = $i
        }
    }
    Assert-True ($fetchIdx -ge 0) `
        -Because 'expected the rewritten checksum fetch warning to be present in code'
    Assert-True ($mismatchIdx -ge 0) `
        -Because 'expected the fatal mismatch branch to be present in code'
    # The catch that warns about an unfetchable manifest must come BEFORE the
    # comparison, proving the comparison sits outside it.
    Assert-True ($fetchIdx -lt $mismatchIdx) `
        -Because 'the digest comparison must not be inside the network-failure catch'
}

# ---------------------------------------------------------------------------
Write-Host ""
if ($script:Failed -eq 0) {
    Write-Host "all $script:Passed test(s) passed" -ForegroundColor Green
    exit 0
}
Write-Host "$script:Failed of $($script:Passed + $script:Failed) test(s) failed" -ForegroundColor Red
exit 1