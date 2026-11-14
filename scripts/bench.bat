@echo off
rem Measures how smoothly HeroWorld runs: the hero runs along the main roads,
rem then flies over the town, each for a minute, while every frame is timed.
rem Each route runs twice: first with Engine's GPU and CPU timings on (they
rem slow the frames), then plain for the frame times.
rem Reports: logs\bench-street.txt, logs\bench-flight.txt (all runs are also
rem added to logs\bench.log); Engine's timings: logs\bench-*-engine.txt.
rem   scripts\bench.bat            both routes
rem   scripts\bench.bat flight     one route
rem Keep the window in front and leave mouse and keyboard alone meanwhile.
cd /d "%~dp0\.."
cargo build --release -p hw-client || (pause & exit /b 1)
if not exist logs mkdir logs
set ROUTES=street flight
if not "%~1"=="" set ROUTES=%~1
for %%r in (%ROUTES%) do (
  set HW_BENCH=%%r
  echo Benchmark %%r with Engine's timings ...
  set ENGINE_GPU_PROFILE_EVERY=120
  set ENGINE_CPU_TRACE=1
  set ENGINE_FRAME_STATS=1
  cargo run -q --release -p hw-client 2> logs\bench-%%r-engine.txt
  set ENGINE_GPU_PROFILE_EVERY=
  set ENGINE_CPU_TRACE=
  set ENGINE_FRAME_STATS=
  echo Benchmark %%r ...
  cargo run -q --release -p hw-client
)
set HW_BENCH=
echo Done. The reports are in the logs folder.
