# The landscape

The plan lives in the [HeroWorld landscape plan](https://claude.ai/artifact/2EUBMoi59Qj9ZKbGZmXYxC); this file says what is built.

## Layers

1. **Globe** (`hw-gen::planet::globe`). The planet's shape as one analytic function of a point on the unit sphere: `Globe::elevation(unit, finest_m)` is the height above sea level in metres, with detail down to about `finest_m`. Sixteen tectonic plates (four in ten continental) give continents, ocean floors, mountain belts where plates close, island arcs and trenches at sea, rifts where they part. Seamless 3D noise (`hw_core::Noise3`) breaks them into several landmasses with ragged coasts, adds hills, wandering mountain chains and worn inland ranges. Sea level is calibrated per seed so 30% of the surface is land. Finer detail only adds octaves; the coarse shape does not change.
2. **Grid** (`planet::grid`). A cube-sphere of `n x n` cells per face, equiangular, so cells are nearly equal and there are no poles. `n = 512` gives cells of about 20 km. `par_map` runs a function over all cells on every core, in order.
3. **PlanetMap** (`planet::map`). What needs the whole planet, solved once on the grid:
   - Temperature: warm at the equator, cold at the poles, 6.5 °C a km up.
   - Rain: moist air carried by the trade winds, westerlies and polar easterlies, picked up over the sea and dropped over rising ground, so windward coasts and mountains are wet and the far side dry. Latitude bands make the equatorial and sub-polar wet belts and the dry subtropics.
   - Rivers and lakes: Priority-Flood fills every basin (so every river reaches the sea), the rain runs off and adds up downhill in m³/s. 60 m³/s makes a river on the map; a basin filled more than 6 m is a lake.
   - Biomes (`planet::biome`): ice, tundra, taiga, steppe, grassland, shrubland, temperate forest, desert, savanna, seasonal forest, rainforest, alpine, plus ocean and lake. `Biome::tree_cover` says how wooded each is.
4. **Drama** (Arnd 2026-11-14: the planet should be more dramatic than a real one, light supernatural common, special things rare). `Globe::elevation` has tall ranges (to 12 km), tiered mesas with sheer cliffs and deep canyons. `Globe::aether(unit)` is the Aether field, 0 to 1: most land sits around 0.2 to 0.5 and thin ley lines climb towards 1. `Globe::solid(p, above_ground)` is the truly 3D part (floating rock in strong Aether, caves and overhangs in rocky ground), a density to add to the ground, meant for Engine's `WorldGenerator::density`. `planet::wonders` places 16 named wonders (two of each of 8 kinds, 1700 km apart) where the Aether is strongest and the land suits them. The start region is in a temperate zone and stays close to normal.
5. **Home and ground in the game** (`planet::home`, 0.306). `choose_home` picks a temperate river mouth with weak Aether and mountains within 50 km. `PlanetPatch` is the planet's ground in a place's local metres (x east, z south): `ground(p)` is the height with the planet's curve (`d²/2R`), `site` says what the place is like (site kind, relief, river width and direction, which way and how far the sea lies). The patch stands where the real coast is about 700 m off. `TownParams::on_planet` takes those as the town's setting (`seaward`, `coast_distance`, `flow` override the town's own picks). `TownWorld::generate_on_or_stop` blends the town's own land into the planet's ground from 72% of the planned land's half-width outward, and `TownWorld::ground` is what the client, the server and collision all read. `GeneratedTown::generate_on_planet*` is what the game uses; `generate` (flat, seed-made) stays for tests. `planet_map` also writes `-home.png`: 60 km around the home with the town's box in red.
   Since 0.307 the town's land is the planet's: `TownParams::planet` hands the patch to `Land::generate`, which marks every cell below sea level as sea (the real, ragged coast; `Land::sea` only says whether there is one) and takes the planet's heights for its hills, with banks falling to the water no steeper than 1 in 3. `urban::outer::OuterRivers` carries each river on upstream of the town along the lowest ground (to 25 km, or to where the land stands 150 m high), narrowing to a brook, and `TownWorld::ground` cuts its valley into the planet's ground. `choose_home` and the patch look for open sea (still under water 1.5 and 3 km out), not a pond.
   Known limits: all water lies at sea level, so a river cannot climb; rivers that fall (and waterfalls) need per-point water heights in Engine's far water. The planet's own rivers (from `PlanetMap`) are not yet the ones drawn.
6. Next: local detail from this skeleton (river polylines, valley carving, ground in the game) and settlements. See the plan.

Solving a planet at `n = 512` takes about 3 s on all cores; do it on a worker thread and cache it by seed and `WORLD_VERSION`.

## Looking at it

```
cargo run --release -p hw-gen --example planet_map -- <seed> screenshots/planet [n]
```

Writes `-world.png` (biomes, shading, rivers), `-rain.png`, `-height.png`, three globe views `-globe0..2.png`, and prints landmasses, rain and height percentiles and the share of each biome. Open the PNGs and look: this is how the climate was tuned.

## Tests

`planet::globe` (same seed same planet, land share, no seams at poles or date line, finer detail keeps the coarse shape), `planet::grid` (cells round-trip, neighbours across face edges), `planet::map` (deterministic, warm equator, every river reaches the sea, rain varies).
