@echo off
setlocal EnableExtensions
title OnlyDEVS - Antigravity Builder
set "REPO_ROOT=%~dp0"
set "APP_DIR=%REPO_ROOT%antigravity-x"

if not exist "%APP_DIR%\package.json" (
  echo [ERROR] Source folder not found: %APP_DIR%
  goto :fail
)

set "PATH=%USERPROFILE%\.cargo\bin;%APPDATA%\npm;%LOCALAPPDATA%\pnpm;%ProgramFiles%\nodejs;%PATH%"

where node >nul 2>&1
if errorlevel 1 (
  echo [ERROR] Node.js not found. Install Node.js LTS: https://nodejs.org/
  goto :fail
)

where pnpm >nul 2>&1
if errorlevel 1 (
  echo [INFO] pnpm not found. Installing pnpm globally...
  call npm install -g pnpm
  if errorlevel 1 (
    echo [ERROR] Failed to install pnpm. Run manually: npm install -g pnpm
    goto :fail
  )
)

where cargo >nul 2>&1
if errorlevel 1 (
  echo [ERROR] Rust / cargo not found. Install Rustup: https://rustup.rs/
  goto :fail
)

call "%APP_DIR%\scripts\build.bat"
if errorlevel 1 goto :fail

echo.
echo [DONE] Build completed successfully!
echo   Portable : %APP_DIR%\src-tauri\target\release\OnlyDEVS-Antigravity.exe
echo   Installer: %APP_DIR%\src-tauri\target\release\bundle\nsis\
echo.
echo %CMDCMDLINE% | find /I "/c" >nul && pause
endlocal
exit /b 0

:fail
echo.
echo [FAILED] Build exited with errors.
echo %CMDCMDLINE% | find /I "/c" >nul && pause
endlocal
exit /b 1
