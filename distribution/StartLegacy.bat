@echo off
cd /d "%~dp0"
powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0scripts\start.ps1" -Mode Auto
if errorlevel 1 pause
