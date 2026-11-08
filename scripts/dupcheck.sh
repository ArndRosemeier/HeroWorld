#!/usr/bin/env bash
# Lists duplicated code (a reading list, not a gate): writes
# reports/duplicate-candidates.md and reports/repeated-code.md.
# Options pass through, e.g. scripts/dupcheck.sh --max 50, --help.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --release -q -p dupcheck -- "$@"
