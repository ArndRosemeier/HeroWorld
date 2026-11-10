#!/usr/bin/env bash
# The checks a change must pass before it is pushed. See docs/CHECKS.md.
#
#   scripts/check.sh            fast: fmt, clippy, and the tests of the crates
#                               this change touches (and the crates that use
#                               them), slow tests left out. Run it before every push.
#   scripts/check.sh --all      the same tests for every crate.
#   scripts/check.sh --full     everything, slow tests too: what CI runs.
#                               Not the on-demand tests.
#   scripts/check.sh --on-demand  only the on-demand tests: looks (z-fighting,
#                               textures, model budgets, ...) and balance.
#                               Nothing runs them automatically.
#   scripts/check.sh hw-sim ... the tests of the crates named.
#
# "This change" is everything that differs from origin/main: commits not yet
# pushed, staged and unstaged edits, new files. Set HW_CHECK_BASE to compare
# with another ref.
set -euo pipefail
cd "$(dirname "$0")/.."

mode=touched
named=()
for a in "$@"; do
    case "$a" in
        --full) mode=full ;;
        --on-demand) mode=on-demand ;;
        --all) mode=all ;;
        -h | --help) sed -n '2,17p' "$0"; exit 0 ;;
        -*) echo "unknown option $a" >&2; exit 2 ;;
        *) mode=named; named+=("$a") ;;
    esac
done

# Engine links ALSA and udev on Linux; a fresh cloud sandbox has neither (nor
# zstd, for the prebuilt dependencies below).
if [ "$(uname -s)" = Linux ] && { ! pkg-config --exists alsa libudev 2>/dev/null || ! command -v zstd >/dev/null; }; then
    if [ "$(id -u)" = 0 ] && command -v apt-get >/dev/null; then
        echo "== installing libasound2-dev libudev-dev zstd"
        apt-get install -y -q libasound2-dev libudev-dev zstd >/dev/null 2>&1 ||
            { apt-get update -q >/dev/null 2>&1 && apt-get install -y -q libasound2-dev libudev-dev zstd >/dev/null; }
    else
        echo "Engine needs libasound2-dev and libudev-dev: install them first." >&2
        exit 1
    fi
fi

t0=$SECONDS

# A fresh Linux sandbox: download the prebuilt dependencies (Engine, wgpu,
# ...) instead of compiling them, when CI has built them for exactly this
# Cargo.lock (.github/workflows/dev-cache.yml). Only into an empty target/,
# and only where they were built: as root in /home/user/HeroWorld.
if [ "$(uname -s)" = Linux ] && [ ! -d target/debug/deps ] && [ "$PWD" = /home/user/HeroWorld ] &&
    [ "$(id -u)" = 0 ] && command -v zstd >/dev/null; then
    key="$(scripts/dev-cache-key.sh)"
    url="https://github.com/ArndRosemeier/HeroWorld/releases/download/dev-cache/deps-$key.tar.zst"
    echo "== fetching prebuilt dependencies ($key)"
    if curl -fsSL --retry 3 "$url" | zstd -dq | tar -xf -; then
        echo "== prebuilt dependencies in place ($((SECONDS - t0)) s)"
    else
        rm -rf target
        echo "== none for this Cargo.lock yet (CI builds them after a push): compiling"
    fi
fi
step() { echo "== $1 ($((SECONDS - t0)) s so far)"; }

# On-demand tests: #[ignore = "on demand (...): ..."] on the line(s) before
# the fn. Their names, to skip in --full and to pick in --on-demand.
on_demand=()
while read -r name; do on_demand+=("$name"); done < <(
    grep -rh -A3 --include='*.rs' '#\[ignore = "on demand' crates tools |
        sed -nE 's/^ *(pub )?fn ([a-z0-9_]+)\(.*/\2/p')

if [ "$mode" = on-demand ]; then
    step "cargo test, the ${#on_demand[@]} on-demand tests"
    cargo test --workspace -- --include-ignored "${on_demand[@]}"
    step "done"
    exit 0
fi

step "cargo fmt"
cargo fmt --all -- --check

# Always the whole workspace, in one form: building a subset (-p) resolves
# dependency features differently and rebuilds Engine and wgpu a second time.
step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

if [ "$mode" = full ]; then
    step "cargo test, everything"
    skips=()
    for n in "${on_demand[@]}"; do skips+=(--skip "$n"); done
    cargo test --workspace -- --include-ignored --skip level_twenty_in_about_ten_hours --skip at_the_climb_tests_pace "${skips[@]}"
    step "done"
    exit 0
fi

# Which crates' tests to run.
crate_of() {
    case "$1" in
        crates/*/*) local c="${1#crates/}"; echo "${c%%/*}" ;;
        tools/dupcheck/*) echo dupcheck ;;
        # Every push bumps these; hw-core's test checks they agree.
        VERSION | WHATS_NEW.md) echo hw-core-only ;;
        Cargo.toml | Cargo.lock | .cargo/* | rust-toolchain* | assets/*) echo all ;;
        *) ;; # docs, scripts, tools/robots, packaging: no Rust tests
    esac
}
users_of() {
    case "$1" in
        hw-core) echo hw-core hw-gen hw-net hw-sim hw-client ;;
        hw-gen | hw-net) echo "$1" hw-sim hw-client ;;
        hw-sim) echo hw-sim hw-client ;;
        hw-core-only) echo hw-core ;;
        *) echo "$1" ;;
    esac
}
everything="hw-core hw-gen hw-net hw-sim hw-client dupcheck"
case "$mode" in
    all) want="$everything" ;;
    named) want="${named[*]}" ;;
    touched)
        base="${HW_CHECK_BASE:-origin/main}"
        files="$( (git diff --name-only "$base" -- 2>/dev/null || git diff --name-only HEAD --)
            git ls-files --others --exclude-standard)"
        want=""
        for f in $files; do
            c="$(crate_of "$f")"
            [ -z "$c" ] && continue
            if [ "$c" = all ]; then want="$everything"; break; fi
            want="$want $(users_of "$c")"
        done
        want="$(echo "$want" | tr ' ' '\n' | sed '/^$/d' | sort -u | tr '\n' ' ')"
        if [ -z "$want" ]; then
            step "no Rust crate changed since $base: no tests to run"
            exit 0
        fi
        ;;
esac
echo "== tests of: $want(slow tests left out; --full runs them)"

# Build every test binary of the workspace (the same build as clippy's
# workspace, so nothing is rebuilt twice), then run the wanted crates' ones
# the way cargo test would: from the crate's folder, with its manifest dir.
step "building tests"
list="$(mktemp)"
trap 'rm -f "$list"' EXIT
cargo test --workspace --no-run --message-format=json-render-diagnostics |
    grep '"reason":"compiler-artifact"' | grep -E '"profile":\{[^}]*"test":true' | grep '"executable":"' |
    sed -E 's/.*"package_id":"path\+file:\/\/([^#"]*)#[^"]*".*"executable":"([^"]*)".*/\1\t\2/' >"$list"

step "running tests"
failed=()
while IFS=$'\t' read -r dir exe; do
    case "$dir" in /[A-Za-z]:/*) dir="${dir#/}" ;; esac # file:///C:/... on Windows
    crate="$(basename "$dir")"
    case " $want " in *" $crate "*) ;; *) continue ;; esac
    echo "-- $crate: $(basename "$exe")"
    if ! (cd "$dir" && CARGO_MANIFEST_DIR="$dir" CARGO_PKG_NAME="$crate" "$exe" -q); then
        failed+=("$crate/$(basename "$exe")")
    fi
done <"$list"

if [ ${#failed[@]} -gt 0 ]; then
    echo "== FAILED: ${failed[*]}" >&2
    exit 1
fi
step "done"
