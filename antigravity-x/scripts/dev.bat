@echo off
setlocal
set "ROOT=%~dp0.."
cd /d "%ROOT%"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call pnpm dev
endlocal