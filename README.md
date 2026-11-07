# HeroWorld

A superhero sandbox in Rust on [Engine](https://github.com/ArndRosemeier/Engine): a procedurally generated,
roughly Earth-sized planet in an early-comics retro-future, played alone or with up to 10 friends, where almost
everything can be broken.

Nothing is built yet. The design comes first:

- [docs/DESIGN.md](docs/DESIGN.md): the game design (vision, world, heroes, combat, the loop, architecture, the first playable city).
- [docs/ENGINE_REQUESTS.md](docs/ENGINE_REQUESTS.md): the features HeroWorld asks Engine for, in build order.

Both are snapshots of living design docs that are linked at the top of each file.

## Building

```
cargo run -p hw-client     # the game (single player, server built in)
HW_HOST=1 cargo run -p hw-client                   # host for friends on UDP 7777
HW_JOIN=192.168.1.20:7777 cargo run -p hw-client   # join a friend
cargo run -p hw-sim --bin hw-headless
scripts/check.sh           # what must pass before a push
```

Crates and rules for contributors (and coding agents): [AGENTS.md](AGENTS.md).

## Controls

WASD run, Space jump, right mouse (or P to lock the pointer) look, R/T zoom (all the way in is first person), Tab free camera, C cut buildings open, F move the cut to where you look, Z/X choose the floor.
