# Checks before a push

```
scripts/check.sh            # before every push: fmt, clippy, tests of what you touched
scripts/check.sh --all      # the same tests for every crate
scripts/check.sh --full     # everything, slow tests too (CI runs this on every push)
scripts/check.sh --on-demand  # only the looks and balance tests; nothing runs them automatically
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

## How bad a failure is: three tiers

Every test belongs to one of three tiers (Arnd, 2026-11-10: "sort tests by severity of
fail"; z-fighting, for example, is only cosmetic):

1. **Blockers**: a crash, lost saves, co-op players seeing different worlds (same seed,
   same world), the network, memory that keeps growing, walking through solid things.
2. **Gameplay**: a mechanic does the wrong thing (fights, powers, factions, incidents,
   story, traffic, doors, a town plan that is not sound).
3. **Looks and balance**: z-fighting, textures, feet on the ground, model budgets and
   LOD levels, smoothness, colours, UI layout, how fast a hero levels. These are marked
   `#[ignore = "on demand (looks): scripts/check.sh --on-demand"]` (or `(balance)`) and
   **nothing runs them automatically**: not the fast check, not `--full`, not CI. Run
   `scripts/check.sh --on-demand` when you work on how things look or on balance.

Tiers 1 and 2 run before every push (the crates touched) and in CI. A new test that only
checks looks or balance gets the on-demand mark. `check.sh` finds on-demand tests by that
mark, so the mark must sit in the attributes right above the `fn`.

## Slow tests

A tier 1 or 2 test that takes more than about 12 s in a debug build is marked
`#[ignore = "slow: scripts/check.sh --full and CI run it"]`. They are not dropped: CI runs
`--full` on every push to main, and you run `--full` yourself when you change what they
cover (memory over long trips, soak and ten-bot play, traffic at junctions). Measured
2026-11-10 (0.154, 4-core sandbox):

| Test | Crate | Debug-build time |
| --- | --- | --- |
| `the_empire_waits_until_the_team_can_take_it` | hw-sim | 165 s |
| `ten_bots_play_for_ten_minutes` | hw-sim | 76 s |
| `an_evening_of_co_op_play` (soak) | hw-sim | 45 s |
| `a_walk_across_a_megacity_keeps_memory_bounded_and_the_world_the_same` | hw-gen | 34 s |
| `raiders_clear_every_building_in_towns_of_every_size` | hw-sim | 30 s |
| `people_and_traffic_follow_a_hero_across_a_big_city` | hw-sim | 25 s |
| `a_bot_plays_the_doctors_story_end_to_end` | hw-sim | 23 s |
| `cars_give_way_at_junctions_and_keep_moving` | hw-sim | 20 s |
| `towns_by_the_water_have_a_pirate_landing_and_a_mole_gate` | hw-sim | 16 s |
| `a_long_trip_across_a_big_city_keeps_memory_bounded` | hw-sim | 14 s |

The biggest on-demand ones: `the_city_has_no_z_fighting` 1157 s (57 s before the furnished
interiors of 0.134), `buildings_are_textured` 112 s (failing since 0.135),
`feet_stand_on_what_is_drawn` about 50 s.

A new test that runs that long gets the same mark. The ten-hour climb
(`a_new_hero_reaches_level_twenty_in_about_ten_hours`) stays out even of `--full`; run it
with `--ignored` or the balance example. So does the rookies' climb in a real town
(`a_new_hero_near_the_jackets_climbs_at_the_climb_tests_pace`, tests/rookies.rs, about a
minute in release): run it with `cargo test --release -p hw-sim --test rookies -- --ignored`.

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

## Long sessions: does anything pile up?

A frame rate that falls the longer one plays is something made and never freed. Two
tools find it:

- **The hitch log** (`logs/hitches.log`) ends every minute's summary with a `holds:` line,
  Engine's resource census: entities, LOD sets, textures, lamps, decals, crowd agents,
  sounds, GPU meshes and batches, texture-table slots, characters, video memory. Compare
  the lines of a long session: in a streamed world every count should level off.
- **A lap run**: `HW_BENCH=flight HW_BENCH_LAPS=6 HW_BENCH_SECS=60 HW_HOUR=15` flies the
  same route out and back six times. `logs/bench-flight.txt` has a line per lap (frame
  time, memory, census) and names whatever grew from the second lap to the last ("growth:
  none" is the pass). In a cloud sandbox add `ENGINE_FIXED_DT=0.25 ENGINE_WIDTH=320
  ENGINE_HEIGHT=180` and a profile with quality low and 10 000 people (a 4-lap run of
  1.6 km legs takes about 30 minutes and stays under 12 GB); the GPU counts there lag
  by up to 30 frames, so a small GPU-side difference between laps is noise.

Found this way (2026-11-10, Engine 0.048): textures Engine made for meshes were never
freed after their meshes were despawned.
