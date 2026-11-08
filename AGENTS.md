# Working in HeroWorld (for coding agents)

HeroWorld is a co-op superhero sandbox on a procedurally generated planet, written in
Rust on Arnd's [Engine](https://github.com/ArndRosemeier/Engine).

**Read first:** [`docs/DESIGN.md`](docs/DESIGN.md) (what the game is and how it is built),
then [`docs/ENGINE_REQUESTS.md`](docs/ENGINE_REQUESTS.md) (what we asked Engine for, and what
it does not do yet). The plan of what to build next, milestone by milestone, and which
milestones can run side by side, is [`docs/MILESTONES.md`](docs/MILESTONES.md). The living copies of both are Claude Docs linked at the top of each
file; when they change, re-export the snapshot here.

## Crates

| Crate | Owns | May depend on |
| --- | --- | --- |
| `hw-core` | planet coordinates (f64, planet-centred), seed hashing, IDs | std, glam |
| `hw-gen` | plans as pure data: planet, region, city, building, interior | hw-core |
| `hw-sim` | the server and the truth: entities, bubbles, physics, combat, destruction, saves (bevy_ecs); `host::Host` wraps it with players, interest and a socket; binary `hw-headless` | hw-core, hw-gen, hw-net, engine (`engine::physics::PhysicsWorld` only) |
| `hw-net` | protocol, UDP sockets and the GameHub relay, client connection, reliable messages, replication (per-client acks, change-only updates); later host migration | hw-core |
| `hw-client` | Engine client: rendering, input, camera, UI, prediction; binary `heroworld` | all of the above, engine |
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
- Warnings are errors. Do not silence a lint or skip a test to get green.

## Checks

```
scripts/check.sh        # cargo fmt --check, clippy -D warnings, cargo test --workspace
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
(where, which colours, which building); `tests/no_zfighting.rs` in hw-client keeps the city clean, and
`ENGINE_ZFIGHT=1` marks spots in the game (the finder is Engine's, `engine::zfight`). Fix overlaps in the geometry, never by loosening the check.

Run `check.sh` before every push. For client changes also take a screenshot and look at it.
On Linux the client needs `libasound2-dev`, and headless screenshots need `xvfb`,
`libxkbcommon-x11-0` and `mesa-vulkan-drivers`.

Commit code, specs and docs; never `target/`, `screenshots/` or `assets/out/`.
