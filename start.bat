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

rem Fetch the newest version if this is a git checkout and nothing is changed locally.
where git >nul 2>nul
if not errorlevel 1 if exist .git git pull --ff-only

set HW_HOST=
set HW_JOIN=
if /i "%~1"=="host" set HW_HOST=1
if /i "%~1"=="join" set HW_JOIN=%~2

cargo run --release -p hw-client
if errorlevel 1 pause
