@echo off
echo Starting tukurium host...
cd /d "%~dp0"
powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0scripts\start.ps1" -Mode Host
if errorlevel 1 pause
