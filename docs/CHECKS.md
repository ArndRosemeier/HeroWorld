# Checks before a push

```
scripts/check.sh            # before every push: fmt, clippy, tests of what you touched
scripts/check.sh --all      # the same tests for every crate
scripts/check.sh --full     # everything, slow tests too (CI runs this on every push)
scripts/check.sh hw-gen     # the tests of the crates you name
```

## The fast check (default)

1. `cargo fmt --check` and `cargo clippy --workspace --all-targets -D warnings`, always for
   the whole workspace (clippy is incremental: untouched crates cost nothing).
2. Builds every test binary of the workspace and runs those of the crates that differ from
   `origin/main` (commits not yet pushed, edits, new files), plus the crates that use them:
   `hw-core` → everything; `hw-gen`, `hw-net` → also `hw-sim`, `hw-client`; `hw-sim` →
   also `hw-client`. `VERSION` and `WHATS_NEW.md` run hw-core's tests only; `Cargo.toml`,
   `Cargo.lock` and `assets/` run everything; docs and scripts run no tests.
3. Tests marked `#[ignore = "slow: ..."]` are left out.

On Linux it installs `libasound2-dev`, `libudev-dev` and `zstd` when they are missing and
it runs as root (a fresh cloud sandbox).

## Prebuilt dependencies for fresh sandboxes

Compiling Engine, wgpu, bevy_ecs, rapier and the rest takes about 6 minutes on a 4-core
sandbox. CI builds them once per dependency set (`.github/workflows/dev-cache.yml`, run
when `Cargo.lock`, a `Cargo.toml` or `rust-toolchain.toml` changes) and publishes the
`target/` folder without our own crates as `deps-<key>.tar.zst` on the public release
`dev-cache`. The key (`scripts/dev-cache-key.sh`) hashes those files. When `target/` is
empty, `check.sh` downloads the matching archive (about 115 MB/s in a sandbox) and cargo
only compiles our crates. With no archive for the key yet (a dependency just changed and
CI is still building it) it compiles as before.

Cargo ties its build outputs to paths, so the cache is built as root in
`/home/user/HeroWorld` with `HOME=/root`, where cloud sandboxes have them; anywhere else
(Arnd's PCs, Windows CI) `check.sh` skips it. It is built with an empty environment, so no
token or secret is in it.

## Slow tests

A test that takes more than about 12 s in a debug build is marked
`#[ignore = "slow: scripts/check.sh --full and CI run it"]`. They are not dropped: CI runs
`--full` on every push to main, and you run `--full` yourself when you change what they
cover (memory over long trips, soak and ten-bot play, z-fighting, textures, traffic at
junctions, feet on the ground). Today they are:

| Test | Crate | Debug-build time |
| --- | --- | --- |
| `ten_bots_play_for_ten_minutes` | hw-sim | 72 s |
| `the_empire_waits_until_the_team_can_take_it` | hw-sim | 64 s |
| `feet_stand_on_what_is_drawn` (feet on roads, stairs, lawns, slopes, floors, tunnels) | hw-client | 60 s |
| `the_city_has_no_z_fighting` | hw-client | 57 s |
| `buildings_are_textured` | hw-client | 40 s |
| `an_evening_of_co_op_play` (soak) | hw-sim | 37 s |
| `a_walk_across_a_megacity_keeps_memory_bounded_and_the_world_the_same` | hw-gen | 23 s |
| `people_and_traffic_follow_a_hero_across_a_big_city` | hw-sim | 22 s |
| `cars_give_way_at_junctions_and_keep_moving` | hw-sim | 17 s |
| `towns_by_the_water_have_a_pirate_landing_and_a_mole_gate` | hw-sim | 13 s |
| `a_long_trip_across_a_big_city_keeps_memory_bounded` | hw-sim | 12 s |

A new test that runs that long gets the same mark. The ten-hour climb
(`a_new_hero_reaches_level_twenty_in_about_ten_hours`) stays out even of `--full`; run it
with `--ignored` or the balance example.

To find slow tests (stable Rust, libtest's timing report):

```
RUSTC_BOOTSTRAP=1 target/debug/deps/<binary> -Z unstable-options --report-time
```

## Why it was slow, and what changed (0.093)

Measured in a fresh 4-core cloud sandbox:

- **Debug info.** Every test binary links all of Engine, wgpu and bevy_ecs, and the dev
  profile wrote full debug info for all of them: 630-770 MB per binary, about 30 binaries,
  24 GB. That filled the sandbox disk before the test build finished (after 10 minutes),
  and writing and linking it was much of the build. Now dependencies get no debug info
  and our own crates only line tables (`[profile.dev]` in `Cargo.toml`), so backtraces
  still have line numbers. The whole `target/` is 5.7 GB. To debug inside one library,
  give it debug info for a while:
  `[profile.dev.package.<name>] debug = true`.
- **Building a subset.** `cargo test -p hw-sim` after `cargo test --workspace` rebuilt
  Engine and wgpu (3.5 minutes), because cargo resolves dependency features per selection.
  The script always builds the whole workspace and picks the test binaries to run itself.
- **Slow tests** ran on every push. They now run in CI and with `--full`.

- **Different Rust in CI.** CI took the newest stable Rust while sandboxes had an older
  one, so CI went red (from 0.090) on clippy lints nobody saw before pushing.
  `rust-toolchain.toml` pins one version for everyone (rustup fetches it on first use,
  about 20 s). Bump it on purpose and fix the new lints in the same push.

Release builds (`start.bat`, the Windows zip) are not affected, apart from using the
pinned Rust.
