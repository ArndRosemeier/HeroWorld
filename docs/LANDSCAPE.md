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
4. Next: local detail from this skeleton (river polylines, valley carving, ground in the game) and settlements. See the plan.

Solving a planet at `n = 512` takes about 3 s on all cores; do it on a worker thread and cache it by seed and `WORLD_VERSION`.

## Looking at it

```
cargo run --release -p hw-gen --example planet_map -- <seed> screenshots/planet [n]
```

Writes `-world.png` (biomes, shading, rivers), `-rain.png`, `-height.png`, three globe views `-globe0..2.png`, and prints landmasses, rain and height percentiles and the share of each biome. Open the PNGs and look: this is how the climate was tuned.

## Tests

`planet::globe` (same seed same planet, land share, no seams at poles or date line, finer detail keeps the coarse shape), `planet::grid` (cells round-trip, neighbours across face edges), `planet::map` (deterministic, warm equator, every river reaches the sea, rain varies).
