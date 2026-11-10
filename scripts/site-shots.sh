#!/usr/bin/env bash
# Clean screenshots for the website (site/media): the game without its HUD
# (HW_HUD=0) at a set of sights, one run each. Best on a real graphics card.
# Usage: scripts/site-shots.sh [out dir] [width height]
# On Linux without a display it runs under xvfb-run; on Windows use Git Bash.
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-screenshots/site}"
w="${2:-1920}"; h="${3:-1080}"
mkdir -p "$out"
cargo build --release -p hw-client --bin heroworld
exe=target/release/heroworld
[ -f "$exe.exe" ] && exe="$exe.exe"
run=()
if [ "$(uname -s)" = Linux ] && [ -z "${DISPLAY:-}" ]; then
    run=(xvfb-run -a -s "-screen 0 ${w}x${h}x24")
fi
# name | frame | environment
shots=(
  "street|240|HW_VIEW=street HW_HOUR=10.5"
  "night|300|HW_VIEW=street HW_HOUR=21.5"
  "sky|240|HW_VIEW=sky HW_HOUR=16"
  "colossus|360|HW_VIEW=colossus HW_HOUR=15"
  "airship|300|HW_VIEW=airship HW_INCIDENT=pirates HW_HOUR=11"
  "liner|300|HW_VIEW=liner HW_HOUR=18"
  "tesla|300|HW_VIEW=tesla HW_HOUR=22"
  "quarter|300|HW_VIEW=quarter HW_HOUR=22"
  "rocket|300|HW_VIEW=rocket HW_HOUR=12"
  "atomic|240|HW_VIEW=atomplant HW_HOUR=9"
  "hall|240|HW_VIEW=hall HW_HOUR=17.5"
  "fight|420|HW_VIEW=fight HW_AUTOFIGHT=1 HW_HOUR=14"
  "molehaven|300|HW_VIEW=molehaven"
  "market|240|HW_VIEW=market HW_HOUR=11"
  "rooftop|240|HW_VIEW=rooftop HW_HOUR=18.5"
)
for s in "${shots[@]}"; do
    IFS='|' read -r name frame vars <<<"$s"
    echo "== $name ($vars)"
    # shellcheck disable=SC2086
    env HW_HUD=0 ENGINE_WIDTH="$w" ENGINE_HEIGHT="$h" $vars \
        ENGINE_SCREENSHOT="$PWD/$out/$name.png" ENGINE_SCREENSHOT_FRAME="$frame" \
        "${run[@]}" "$exe" > "$out/$name.log" 2>&1 || echo "   $name failed, see $out/$name.log"
done
echo "Shots in $out. Pick the good ones, then convert to 1600 px WEBP into site/media."
