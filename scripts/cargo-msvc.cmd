@echo off
REM Rerunnable Windows build/test wrapper for alphacode.
REM Loads the MSVC environment in-process, then runs the given cargo arguments.
setlocal
set "VCVARS=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
  echo FAIL: vcvars64.bat not found at "%VCVARS%" 1>&2
  exit /b 2
)
call "%VCVARS%" >nul
if errorlevel 1 (
  echo FAIL: could not initialise the MSVC environment 1>&2
  exit /b 3
)
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\Temp\alphacode-target"
cd /d "%~dp0.."
cargo %*
exit /b %ERRORLEVEL%