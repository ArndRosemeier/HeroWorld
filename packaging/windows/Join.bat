@echo off
rem Joins a friend: Join.bat ROOMCODE (or Join.bat 192.168.1.20:7777)
cd /d "%~dp0"
if "%~1"=="" (
  echo Usage: Join.bat ROOMCODE
  pause
  exit /b 1
)
set HW_JOIN=%~1
start "" heroworld.exe
