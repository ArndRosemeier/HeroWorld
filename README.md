# HeroWorld

A superhero sandbox in Rust on [Engine](https://github.com/ArndRosemeier/Engine): a procedurally generated,
roughly Earth-sized planet in an early-comics retro-future, played alone or with up to 10 friends, where almost
everything can be broken.

A first city is playable: walk the streets, enter the buildings and ride their lifts, alone or with friends.
The design:

- [docs/DESIGN.md](docs/DESIGN.md): the game design (vision, world, heroes, combat, the loop, architecture, the first playable city).
- [docs/ENGINE_REQUESTS.md](docs/ENGINE_REQUESTS.md): the features HeroWorld asks Engine for, in build order.

Both are snapshots of living design docs that are linked at the top of each file.

## Playing on Windows

Download `HeroWorld-windows.zip` from the release
[windows-latest](https://github.com/ArndRosemeier/HeroWorld/releases/tag/windows-latest), unzip it
and double-click `heroworld.exe`. It is built every night the version changed (or by hand:
Actions, "Windows build", Run workflow).

## Building

On Windows, double-click `start.bat` (needs [Rust](https://rustup.rs)); `start.bat host` opens the game to friends,
`start.bat join <address>` joins one.

```
cargo run -p hw-client     # the game (single player, server built in)
HW_HOST=1 cargo run -p hw-client                   # host for friends on UDP 7777
HW_JOIN=192.168.1.20:7777 cargo run -p hw-client   # join a friend
HW_NO_UPNP=1 HW_HOST=1 cargo run -p hw-client       # host without asking the router to forward the port
cargo run -p hw-sim --bin hw-headless              # a dedicated server: the game's town (HW_SEED, HW_POP)
scripts/check.sh           # what must pass before a push
cargo run --release -p hw-client --bin zfight -- 1 --svg screenshots/zfight.svg   # find z-fighting in a city
ENGINE_ZFIGHT=1 cargo run -p hw-client             # ...or see it in the game as magenta posts
```

Crates and rules for contributors (and coding agents): [AGENTS.md](AGENTS.md).

## Controls

WASD run, Space jump (in a lift: rise; Ctrl sinks), V fly (steer with WASD and the camera, Space/Ctrl climb and sink, Shift faster), right mouse (or P to lock the pointer) look, R autorun (S or R stops), mouse wheel zoom (all the way in is first person), N minimap, M town map, Tab free camera, C cut buildings open, F move the cut to where you look, Z/X choose the floor.
