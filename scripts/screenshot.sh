#!/usr/bin/env bash
# Render the client once without a display and save a PNG.
# Usage: [HW_VIEW=overview|street|cutaway] scripts/screenshot.sh [out.png] [frame]
# Linux needs: xvfb, libxkbcommon-x11-0, mesa-vulkan-drivers, libasound2-dev.
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-screenshots/client.png}"
frame="${2:-10}"
mkdir -p "$(dirname "$out")"
xvfb-run -a -s "-screen 0 1280x720x24" \
  env ENGINE_SCREENSHOT="$PWD/$out" ENGINE_SCREENSHOT_FRAME="$frame" \
  cargo run -q -p hw-client --bin heroworld
