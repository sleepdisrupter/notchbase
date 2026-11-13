@echo off
setlocal
cd /d "%~dp0"
set "code=0"
echo Building notchbase...
cargo build --release --target x86_64-pc-windows-msvc
if errorlevel 1 goto failed
echo Installing notchbase...
start "" /wait "target\x86_64-pc-windows-msvc\release\notchbase.exe" --install
if errorlevel 1 goto failed
echo notchbase is installed. It's in Start and opens when you sign in.
goto done
:failed
echo Installing notchbase failed.
set "code=1"
:done
echo %cmdcmdline% | find /i "%~nx0" >nul && pause
exit /b %code%
