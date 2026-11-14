@echo off
rem Starts HeroWorld. Double-click it; the first start builds the game (a few minutes).
rem   start.bat          single player
rem   start.bat host     single player, open to friends on UDP 7777
rem   start.bat join 192.168.1.20:7777   join a friend
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is not installed. Get it from https://rustup.rs and run this again.
  pause
  exit /b 1
)

rem The voice codec is built with CMake: install it by itself when it is missing.
where cmake >nul 2>nul
if errorlevel 1 if exist "%ProgramFiles%\CMake\bin\cmake.exe" set "PATH=%ProgramFiles%\CMake\bin;%PATH%"
where cmake >nul 2>nul
if errorlevel 1 (
  echo Installing CMake, the tool that builds the voice codec. This happens once.
  where winget >nul 2>nul
  if not errorlevel 1 winget install -e --id Kitware.CMake --silent --accept-package-agreements --accept-source-agreements
  if exist "%ProgramFiles%\CMake\bin\cmake.exe" set "PATH=%ProgramFiles%\CMake\bin;%PATH%"
)
where cmake >nul 2>nul
if errorlevel 1 (
  echo CMake could not be installed by itself. Get it from https://cmake.org/download and run this again.
  pause
  exit /b 1
)

rem Fetch the newest version if this is a git checkout and nothing is changed locally.
where git >nul 2>nul
if not errorlevel 1 if exist .git git pull --ff-only

rem People are built from the Actors packs: download them once (and again when the version changes).
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-packs.ps1
if errorlevel 1 echo Could not fetch the Actors packs; the game starts without people.

set HW_HOST=
set HW_JOIN=
if /i "%~1"=="host" set HW_HOST=1
if /i "%~1"=="join" set HW_JOIN=%~2

cargo run --release -p hw-client
if errorlevel 1 pause
