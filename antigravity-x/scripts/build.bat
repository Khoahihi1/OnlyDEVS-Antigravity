@echo off
setlocal
set "ROOT=%~dp0.."
cd /d "%ROOT%"

set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where pnpm >nul 2>&1
if errorlevel 1 (
  echo [ERROR] pnpm not found. Run: npm install -g pnpm
  exit /b 1
)

where cargo >nul 2>&1
if errorlevel 1 (
  echo [ERROR] cargo not found. Run: winget install Rustlang.Rustup
  exit /b 1
)

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
echo [OK] Binary & Installer output:
echo   %ROOT%\src-tauri\target\release\OnlyDEVS-Antigravity.exe
echo   %ROOT%\src-tauri\target\release\bundle\
echo.
endlocal
exit /b 0