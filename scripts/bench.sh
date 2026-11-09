#!/usr/bin/env bash
# Measures how smoothly HeroWorld runs (see scripts/bench.bat and
# crates/hw-client/src/bench.rs). Usage: scripts/bench.sh [street|flight]...
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p hw-client
mkdir -p logs
routes=("${@:-street flight}")
for r in ${routes[@]}; do
  echo "Benchmark $r with Engine's timings ..."
  HW_BENCH=$r ENGINE_GPU_PROFILE_EVERY=120 ENGINE_CPU_TRACE=1 ENGINE_FRAME_STATS=1 \
    cargo run -q --release -p hw-client 2> "logs/bench-$r-engine.txt"
  echo "Benchmark $r ..."
  HW_BENCH=$r cargo run -q --release -p hw-client
done
