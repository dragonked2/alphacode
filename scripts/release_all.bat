@echo off
setlocal enabledelayedexpansion

REM === ALPHACODE ENTERPRISE RELEASE AUTOMATOR ===
REM Just edit the VERSION below and double click this file
set "VERSION=1.0.89"

if "%VERSION%"=="" (
    echo [ERROR] VERSION is empty. Edit this file and set VERSION.
    pause
    exit /b 1
)

echo [RELEASE] Starting release automation for v%VERSION%
echo.

REM Strip leading v if present
if "%VERSION:~0,1%"=="v" set "VERSION=%VERSION:~1%"

REM Check git status
git diff --quiet && git diff --cached --quiet
if %errorlevel% neq 0 (
    echo [ERROR] Working tree is not clean. Commit or stash changes first.
    git status --short
    pause
    exit /b 1
)

echo [1/9] Updating Cargo.toml version to %VERSION%...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$f='Cargo.toml'; $t=Get-Content $f -Raw; $t2=$t -replace '(?m)^version\s*=\s*\".*?\"', 'version = \"%VERSION%\"'; if($t2 -eq $t){exit 1}; Set-Content $f $t2 -NoNewline" || (echo [ERROR] Failed to update Cargo.toml & pause & exit /b 1)

echo [2/9] Generating Cargo.lock...
cargo generate-lockfile || (echo [ERROR] cargo generate-lockfile failed & pause & exit /b 1)

echo [3/9] Running quick sanity test...
cargo test --locked --lib --quiet || (echo [ERROR] Sanity test failed & pause & exit /b 1)

echo [4/9] Adding files to git...
git add Cargo.toml Cargo.lock

echo [5/9] Committing changes...
git commit -m "(Release) v%VERSION%" || (echo [INFO] Nothing to commit? Or commit failed; attempt to continue & git status --short)

echo [6/9] Pushing main branch...
git push origin main || (echo [ERROR] Failed to push main & pause & exit /b 1)

echo [7/9] Deleting old tag if exists locally/remote...
git tag -d v%VERSION% 2>nul
git push origin :refs/tags/v%VERSION% 2>nul

echo [8/9] Creating and pushing new tag...
git tag v%VERSION% || (echo [ERROR] Failed to create tag & pause & exit /b 1)
git push origin v%VERSION% || (echo [ERROR] Failed to push tag & pause & exit /b 1)

echo [9/9] Done!
echo.
echo ========================================
echo Release v%VERSION% completed successfully!
echo Monitor GitHub Actions:
echo https://github.com/dragonked2/alphacode/actions
echo ========================================
echo.
pause
exit /b 0
