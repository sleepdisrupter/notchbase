@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0release-github.ps1" %*
set "code=%errorlevel%"
echo %cmdcmdline% | find /i "%~nx0" >nul && pause
exit /b %code%
