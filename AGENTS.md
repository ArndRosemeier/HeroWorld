# Working in HeroWorld (for coding agents)

HeroWorld is a co-op superhero sandbox on a procedurally generated planet, written in
Rust on Arnd's [Engine](https://github.com/ArndRosemeier/Engine).

**Read first:** [`docs/DESIGN.md`](docs/DESIGN.md) (what the game is and how it is built),
then [`docs/ENGINE_REQUESTS.md`](docs/ENGINE_REQUESTS.md) (what we asked Engine for, and what
it does not do yet). The plan of what to build next, milestone by milestone, and which
milestones can run side by side, is [`docs/MILESTONES.md`](docs/MILESTONES.md). What the game does today, with
the numbers the code uses, is [`docs/GAME_GUIDE.md`](docs/GAME_GUIDE.md): update it when you change a mechanic. The living copies of both are Claude Docs linked at the top of each
file; when they change, re-export the snapshot here.

## Crates

| Crate | Owns | May depend on |
| --- | --- | --- |
| `hw-core` | planet coordinates (f64, planet-centred), seed hashing, IDs | std, glam |
| `hw-gen` | plans as pure data: planet, region, city, building, interior | hw-core |
| `hw-sim` | the server and the truth: entities, bubbles, physics, combat, destruction, saves (bevy_ecs); `host::Host` wraps it with players, interest and a socket; binary `hw-headless` | hw-core, hw-gen, hw-net, engine (`engine::physics::PhysicsWorld` only) |
| `hw-net` | protocol, UDP sockets and the GameHub relay, client connection, reliable messages, replication (per-client acks, change-only updates); later host migration | hw-core |
| `hw-client` | Engine client: rendering, input, camera, UI, prediction; binary `heroworld` | all of the above, engine |
| `hw-launcher` | the launcher (`HeroWorldLauncher.exe`, eframe): installs and updates the game from the `windows-latest` release via `manifest.json` and bundles (`scripts/release-bundles.py`), what's new, Play/Host/Join; own version in its Cargo.toml, bump it when it changes | std, eframe, ureq (no Engine, no hw-*) |
| `dupcheck` (`tools/`) | a dev tool: finds duplicated code (`scripts/dupcheck.sh`) | std only |

## Rules

- **Push straight to `main`.** No branches or pull requests (Arnd's standing rule).
- **Every push bumps the version.** Add 0.001 to `VERSION` (0.001, 0.002, ...) and put a
  `## <version> (<date>)` section at the top of `WHATS_NEW.md` with a few plain lines for
  players. The game shows both (top right, "What's new"); a test checks they agree. If a
  rebase brings in someone else's bump, take the next number.
- **Engine's renderer only in `hw-client`.** Generation never imports Engine; the server uses
  only Engine's headless `PhysicsWorld` (`hw-sim/src/physics.rs`), never `World`, windows or
  the GPU, so it runs without a display. (Engine is one crate, so on Linux every crate that
  links it needs `libasound2-dev`.)
- **The server is the truth.** Game state lives in `hw-sim`; the client mirrors it into Engine
  and predicts only the local hero. No gameplay logic in client rendering code.
- **Same seed, same world.** Generation is deterministic: derive every random choice from the
  world seed with path-keyed hashing (`hw-core`), never from unseeded randomness, `HashMap`
  iteration order, time or thread scheduling. Use `hw_core::det` for `sin`, `cos`, `atan2`
  and the like: std's versions can round differently per platform, and co-op players each
  generate the world themselves. Bump `WORLD_VERSION` when a change alters what
  an existing seed produces.
- **No "+Y is up" assumption** in world code: the planet is round. Positions are f64 from the
  planet centre; local frames give an up vector.
- **Generic needs go to Engine.** If something belongs in a game engine (rendering, physics,
  streaming, destruction), propose it in ENGINE_REQUESTS.md rather than working around it.
- **What is drawn is what collides.** Every new solid or moving thing (a prop, a landmark, a
  vehicle, a robot, a building part) gets its solid shape from the same data it is drawn from
  (`hw_gen::urban::footprint::Footprint`: `Prop::footprints`, `Landmark::footprints` and
  `stairs`), so heroes, NPCs and cars cannot pass through it; raised ground (plinths, decks)
  goes into `Obstacles::ground_at`. Cars drive only `street_lanes`, which keep clear of
  buildings, parked cars and props; people on foot step round props (`citizens.rs`). Extend
  the checks (`street_cars_keep_clear_of_buildings_and_props`, `no_building_stands_on_a_street`,
  `visible_things_are_solid`) for anything new, and never loosen them.
- **Work that takes time runs on a worker thread by default** (Arnd's standing rule): generation,
  mesh and texture building, simulation, anything with a loop over many things. The main thread
  only hands finished results to Engine. Keep something on it only for a strong reason, and say
  why in a comment; never "on the main thread first, move it out later". A worker thread that
  builds things calls `engine::parallel::background_thread()` first thing, so the frame wins the
  cores; work a frame waits for runs on threads that are kept (`engine::parallel::beside_frame`,
  a pool), never on a thread started that frame.
- Measure before and after a speed change: `scripts/bench.sh` (or `bench.bat`; `HW_BENCH=street|flight`,
  see `crates/hw-client/src/bench.rs`) writes frame times and the update's sections to `logs/bench-*.txt`.
- Warnings are errors. Do not silence a lint or skip a test to get green.

## Visual quality bar

Every model the player sees (cars, taxis, vans, robots, props, landmarks, furniture) ships at
**final quality**, never as a low-poly stand-in. Arnd judges the game by its screenshots; a
boxy car is a bug, not a first version. The early box cars, the box ambulance and the cube
people were proofs of concept, not the bar: do not copy them, and do not build a new model
"like the existing ones" without checking they meet this section.

- **Look:** golden-age retro-future (1950s Googie and streamline: long fins, chrome bumpers and
  spears, whitewalls, bubble glass, two-tone paint, rocket and atom motifs). A real designer's
  object of that era, with its details: bumpers, grilles, lamps in bezels, door seams, handles,
  trim, wheel arches, tyres with tread and hubcaps.
- **Shape:** curved things are smooth. Build them as lofted or lathed surfaces with smooth
  normals (`MeshBuilder::grid`, or an AssetGenerator `.glb`), not boxes, tapers or 6-12 sided
  ellipsoids with flat facets. Hard edges only where the real object has them.
- **Close-up budget:** spend what a close-up needs. A car or van near the camera is several
  thousand triangles (about 5 000-15 000); a lamp, hydrant or bench a few hundred to a couple of
  thousand. Round parts get at least 24 sides near the camera.
- **Frame rate comes from LODs,** not from crude meshes: give every model at least three levels
  (close-up, mid at a fifth or so of the triangles, far at a few hundred) and instance them
  (`spawn_lod_at`). Check `ENGINE_FRAME_STATS=1` on a busy street.
- **Look before you push:** take a close-up screenshot of every new or changed model (a
  `HW_VIEW` for it, the camera within 5 m) and compare it with a photo of a real 1950s car or
  object. If it would look out of place in a modern game, it is not done.
- Tools: AssetGenerator (`prop.assembly`, `organic.sculpt`; see M11's `tools/robots/`) for
  complex parts; OpenRouter for reference images and textures (`tools/openrouter/or.py`).

## Checks

```
scripts/check.sh        # before every push: fmt, clippy, blocker tests of the crates you touched
scripts/check.sh --daily # gameplay and slow tests too (CI's first run of the day); see docs/CHECKS.md
scripts/check.sh --on-demand # looks and balance tests (z-fighting, textures, ...): never run automatically
scripts/screenshot.sh   # renders the client headless to screenshots/client.png
cargo run -p hw-gen --example city_map -- 1 screenshots/city.svg   # a city plan as a map
scripts/dupcheck.sh     # duplicated code: same-name functions, body clones, repeated snippets
```

Duplicated code: run `scripts/dupcheck.sh` now and then (after a milestone, before adding a
helper you suspect exists) and read `reports/repeated-code.md` first. It is a reading list, not
a gate; how to read it is in [`tools/dupcheck/README.md`](tools/dupcheck/README.md). Shared
geometry already lives in `hw_gen::geom` (`Polyline`, `signed_area`), `hw_gen::water::segment_distance`
and `hw_gen::urban::roadgrid` (roads near a point): use those rather than writing them again.

Playing together on one machine: `HW_HOST=1 cargo run -p hw-client` in one window,
`HW_JOIN=127.0.0.1:7777 cargo run -p hw-client` in another (or `cargo run -p hw-sim --bin
hw-headless` as the host). `crates/hw-sim/tests/join_over_udp.rs` tests the same.

Z-fighting: `cargo run --release -p hw-client --bin zfight -- <seed> [--cut N]` lists faces that fight
(where, which colours, which building), `-- <seed> --town <people>` a town's streets and junctions;
`tests/no_zfighting.rs` in hw-client keeps the city and town streets clean, and
`ENGINE_ZFIGHT=1` marks spots in the game (the finder is Engine's, `engine::zfight`). Fix overlaps in the geometry, never by loosening the check.

Run `check.sh` before every push (the fast default: blocker tests only; `--daily` when you change
what a daily test covers). Tests have three tiers (docs/CHECKS.md): blockers (crash, saves, same
seed same world, network, memory, walking through solid things) run every time; a gameplay test,
or a blocker over ~10 s in a debug build, is `#[ignore = "daily (gameplay): ..."]` (or
`(slow blocker)`); a test that only checks looks or balance (z-fighting, textures, model budgets,
levelling pace) is `#[ignore = "on demand (looks): scripts/check.sh --on-demand"]` (or `(balance)`)
and nothing runs it automatically. For client changes also take a screenshot and look at it.
On Linux the client needs `libasound2-dev`, and headless screenshots need `xvfb`,
`libxkbcommon-x11-0` and `mesa-vulkan-drivers`.

Commit code, specs and docs; never `target/`, `screenshots/` or `assets/out/`.
