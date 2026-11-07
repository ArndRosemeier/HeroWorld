# Working in HeroWorld (for coding agents)

HeroWorld is a co-op superhero sandbox on a procedurally generated planet, written in
Rust on Arnd's [Engine](https://github.com/ArndRosemeier/Engine).

**Read first:** [`docs/DESIGN.md`](docs/DESIGN.md) (what the game is and how it is built),
then [`docs/ENGINE_REQUESTS.md`](docs/ENGINE_REQUESTS.md) (what we asked Engine for, and what
it does not do yet). The living copies of both are Claude Docs linked at the top of each
file; when they change, re-export the snapshot here.

## Crates

| Crate | Owns | May depend on |
| --- | --- | --- |
| `hw-core` | planet coordinates (f64, planet-centred), seed hashing, IDs | std, glam |
| `hw-gen` | plans as pure data: planet, region, city, building, interior | hw-core |
| `hw-sim` | the server and the truth: entities, combat, destruction, saves (bevy_ecs); binary `hw-headless` | hw-core, hw-gen |
| `hw-net` | protocol, UDP, deltas, host migration | hw-core |
| `hw-client` | Engine client: rendering, input, camera, UI, prediction; binary `heroworld` | all of the above, engine |

## Rules

- **Push straight to `main`.** No branches or pull requests (Arnd's standing rule).
- **Engine only in `hw-client`.** Generation and simulation never import Engine, so the server
  runs headless and tests run anywhere. Server physics will come from Engine's
  physics-without-renderer once it exists (ENGINE_REQUESTS.md #1).
- **The server is the truth.** Game state lives in `hw-sim`; the client mirrors it into Engine
  and predicts only the local hero. No gameplay logic in client rendering code.
- **Same seed, same world.** Generation is deterministic: derive every random choice from the
  world seed with path-keyed hashing (`hw-core`), never from unseeded randomness, `HashMap`
  iteration order, time or thread scheduling. Bump `WORLD_VERSION` when a change alters what
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
```

Run `check.sh` before every push. For client changes also take a screenshot and look at it.
On Linux the client needs `libasound2-dev`, and headless screenshots need `xvfb`,
`libxkbcommon-x11-0` and `mesa-vulkan-drivers`.

Commit code, specs and docs; never `target/`, `screenshots/` or `assets/out/`.
