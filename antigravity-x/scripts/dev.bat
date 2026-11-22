@echo off
setlocal
set "ROOT=%~dp0.."
cd /d "%ROOT%"
set "PATH=%USERPROFILE%\.cargo\bin;%APPDATA%\npm;%LOCALAPPDATA%\pnpm;%ProgramFiles%\nodejs;%PATH%"
call pnpm dev
endlocal
