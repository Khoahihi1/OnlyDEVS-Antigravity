@echo off
setlocal
set "ROOT=%~dp0.."
cd /d "%ROOT%"

set "PATH=%USERPROFILE%\.cargo\bin;%APPDATA%\npm;%LOCALAPPDATA%\pnpm;%ProgramFiles%\nodejs;%PATH%"

where pnpm >nul 2>&1
if errorlevel 1 (
  echo [ERROR] pnpm not found in PATH. Run: npm install -g pnpm
  exit /b 1
)

where cargo >nul 2>&1
if errorlevel 1 (
  echo [ERROR] cargo not found in PATH. Run: winget install Rustlang.Rustup
  exit /b 1
)

taskkill /F /IM OnlyDEVS-Antigravity.exe >nul 2>&1

echo [OnlyDEVS-Antigravity] pnpm install...
call pnpm install
if errorlevel 1 exit /b 1

echo [OnlyDEVS-Antigravity] typecheck...
call pnpm typecheck
if errorlevel 1 exit /b 1

echo [OnlyDEVS-Antigravity] tauri build...
call pnpm tauri build
if errorlevel 1 exit /b 1

echo.
echo [OK] Binary and Installer output:
echo   %ROOT%\src-tauri\target\release\OnlyDEVS-Antigravity.exe
echo   %ROOT%\src-tauri\target\release\bundle\nsis\
echo.
endlocal
exit /b 0
