@echo off
setlocal
cd /d "%~dp0"
set "code=0"
echo Building the notchbase installer...
cargo build --release --target x86_64-pc-windows-msvc -p notchbase-setup
if errorlevel 1 goto failed
if not exist dist mkdir dist
copy /y "target\x86_64-pc-windows-msvc\release\notchbase-setup.exe" "dist\notchbase-setup.exe" >nul
if errorlevel 1 goto failed
echo Built dist\notchbase-setup.exe
echo It downloads and installs the newest published notchbase every time it runs.
goto done
:failed
echo Building the installer failed.
set "code=1"
:done
echo %cmdcmdline% | find /i "%~nx0" >nul && pause
exit /b %code%
