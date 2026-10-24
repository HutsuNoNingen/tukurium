@echo off
cd /d "%~dp0"
echo Building and starting the current Kokoverse source (debug profile)...
powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0scripts\start-online.ps1" -Profile debug -Build
if errorlevel 1 pause
