@echo off
echo Starting Kokoverse...
cd /d "%~dp0"
powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0scripts\start-instance.ps1"
if errorlevel 1 pause
