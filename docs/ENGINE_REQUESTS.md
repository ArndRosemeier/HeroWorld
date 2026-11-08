# Engine requests for HeroWorld

As of 2026-11-07. The living copy is the [Engine requests for HeroWorld](https://claude.ai/artifact/EoQvNE9daGb3jNP1NTvNME) doc; this file is a snapshot of it.

HeroWorld is a superhero sandbox in Rust on Engine: a procedurally generated, roughly Earth-sized planet, played alone or with up to 10 friends, where almost everything breaks. It asks Engine for ten features before its first playable city. The full game design is in the [HeroWorld design](DESIGN.md) doc.

**How HeroWorld uses Engine.** The game keeps its own simulation state (in `bevy_ecs`) on a server that runs inside the game, or alone without a window. Engine is the client's renderer, scene (entities, prefabs, animation) and cosmetic physics; the client mirrors the game state into Engine's `World` each frame. The server also needs Engine's physics, but without a renderer. Everything visible is generated from a seed: buildings made of breakable elements, interiors in every building, streets, terrain.

This list was checked against Engine `main` at commit 6191cf5 (animation blending and foot IK). Save and load, parent/child entities, prefabs, foot IK, f64 global positions and the floating origin already exist and are used as they are.

## Status (2026-11-07)

All ten are in Engine `main`, each with an example (listed in Engine's `run_examples.py`), tests and a README section.

| # | Feature | Engine PR | API to start from | Example |
| --- | --- | --- | --- | --- |
| 1 | Physics without a renderer | [#49](https://github.com/ArndRosemeier/Engine/pull/49) | `engine::PhysicsWorld`, `BodyId` | tests |
| 2 | Fixed tick with interpolation | [#51](https://github.com/ArndRosemeier/Engine/pull/51) | `World::set_sim_rate`, `Frame.sim`, `set_sim_place`, `sim::Headless` | `fixed_tick` |
| 3 | Building destruction | [#52](https://github.com/ArndRosemeier/Engine/pull/52) | `engine::building` (`BuildingPlan`, `ElementBits`), `World::spawn_building`, `hit_building`, `apply_building_state` | `buildings` |
| 4 | Ragdolls | [#53](https://github.com/ArndRosemeier/Engine/pull/53) | `World::knock_down`, `get_up`, `pick_character` | `ragdolls` |
| 5 | Indoor visibility culling | [#54](https://github.com/ArndRosemeier/Engine/pull/54) | `engine::rooms`, rooms and gaps in `BuildingPlan` | `interiors` |
| 6 | Reflections | [#55](https://github.com/ArndRosemeier/Engine/pull/55) | `ScreenSpaceReflections`, `World::add_reflection_probe` | `reflections` |
| 7 | Decals | [#56](https://github.com/ArndRosemeier/Engine/pull/56) | `World::spawn_decal(Decal::new(..).on(entity))`, `set_decal_budget` | `decals` |
| 8 | Colour grading and fog by zone | [#57](https://github.com/ArndRosemeier/Engine/pull/57) | `ColorGrade`, `World::add_fog_volume(FogVolume)`, `set_zone_transition` | `fog_volumes` |
| 9 | Crowds | [#58](https://github.com/ArndRosemeier/Engine/pull/58) | `World::add_crowd(model, CrowdProfile)`, `add_crowd_agent`, `set_crowd_agent` | `crowds` |
| 10 | Several moons | [#60](https://github.com/ArndRosemeier/Engine/pull/60) | `World::set_moons(vec![MoonSpec..])` | `moons` |

Notes:

- Crowds: far people use the character simplified to about 5% of its triangles (67.6k to 4.1k vertices for the Actors sample), or `CrowdProfile::far_model` with an export of Actors' "Crowd" level. Measured on an RTX 5070 at 2560×1440: 1,000 people run at 169 fps walking through the crowd and 189 fps with everyone in view; 3,000 people run at 117 fps.
- Reflections: a little hatching shows at the bottom of curved chrome where screen-space rays run out; probes cover the rest.

## Priorities

Build in this order. The first two unblock HeroWorld's server and networking, so they matter most.

| # | Feature | Why HeroWorld needs it |
| --- | --- | --- |
| 1 | Physics without a renderer | the game server simulates gameplay physics headless |
| 2 | Fixed simulation tick with interpolation | server authority, client prediction, smooth rendering |
| 3 | Structural destruction of buildings | breakable cities, element by element, with collapses |
| 4 | Ragdolls | knockback and slapstick in every fight |
| 5 | Indoor visibility culling | every building has an interior |
| 6 | Reflections | the chrome and glass of the utopia |
| 7 | Decals | scorch marks, cracks, frost, power marks |
| 8 | Colour grading and fog by zone | utopia and dystopia read differently at a glance |
| 9 | Crowds | living streets full of people |
| 10 | Several moons in the sky | the planet has more than one moon |

## The features

Each entry says what it must do and how to tell it's done. API names are suggestions; Engine's style wins.

### 1. Physics without a renderer

Today `Physics` is `pub(crate)` and lives inside `World`, which needs a window. HeroWorld's server must run the same physics with no window and no GPU.

- A public physics world that runs alone: add and remove bodies, colliders from meshes and shapes, step, rays, impacts, blasts, and terrain colliders from a height or voxel function.
- The same collider building as the client's `World`, so server and client agree on shapes.
- A body handle the game owns (not tied to a rendered entity).
- **Done when:** a test builds a scene, steps it 10 s with no window or GPU, and gets the same result as the same scene inside `World`.

### 2. Fixed simulation tick with interpolation

- A fixed-rate simulation step (for example 30 or 60 Hz) separate from the render frame, with `frame` reporting how many sim steps ran and the blend factor.
- Entities can be given a previous and a current transform, and rendering interpolates between them.
- A way to run the update loop with no window, for the dedicated server and tests.
- **Done when:** an example renders smoothly at 144 Hz from a 30 Hz simulation, and the same simulation runs headless in a test.

### 3. Structural destruction of buildings

Scale did this in TypeScript (`Scale/src/destruction`, `docs/ARCHITECTURE.md`, "Destruction"); it's the model to follow.

- A building made of **elements** (wall panels per window bay per floor, slabs, columns, roof pieces, interior walls), each with a material strength.
- Breaking an element is a state change (one bit), not a geometry re-upload: the shader hides dead elements.
- **Support rules:** floor by floor; even loss of support pancakes the floors above, one-sided loss topples the upper part around a hinge. The falling part becomes a rigid chunk that shatters on impact.
- Debris from pre-fractured templates in a fixed body pool that freezes into static rubble, so cost stays bounded.
- **Rubble mounds** as deterministic shapes computed from the collapse, identical on every machine (the cosmetic debris may differ).
- The element state is plain data the game can read, store and send, and apply again.
- **Done when:** an example lets you break panels, cause a pancake collapse and a topple, and a test shows that applying the same element bits to a fresh building gives the same building and rubble.

### 4. Ragdolls

Scale had these on Rapier in TypeScript (`Scale/src/physics/ragdoll`).

- A ragdoll for any skinned character: about 14 bodies with limited joints, sized from the skeleton.
- Seeded from the current animated pose and launched by an impulse; blended back into animation without a pop.
- Get-up from front and back, and staying down when the game says so.
- A cap on simultaneous ragdolls, with settled ones dropping their physics but keeping the pose.
- **Done when:** an example knocks characters flying with a click; they tumble, settle and get up.

### 5. Indoor visibility culling

- Rooms and the openings between them (doors, windows, holes made by destruction) so that rooms you can't see are not drawn.
- Openings change at runtime when walls break.
- **Done when:** a city block where every building has a full interior renders at the same frame rate as the block without interiors, from the street.

### 6. Reflections

- Screen-space reflections for all opaque surfaces (water already has them), plus placed reflection probes as fallback.
- **Done when:** chrome, glass towers and wet streets reflect their surroundings convincingly in an example.

### 7. Decals

- Projected decals on any surface, including skinned and moving ones: scorch, cracks, frost, impact marks, graffiti. PBR (albedo, normal, roughness), fading and pooled.
- **Done when:** an example sprays decals on terrain, walls and a moving body, with a fixed budget.

### 8. Colour grading and fog by zone

- Volumes that change colour grading, fog density and fog colour, blended smoothly as the camera moves between them.
- **Done when:** flying from a "warm" volume into a "smog" volume blends over a few seconds with no pop.

### 9. Crowds

- Hundreds of animated people at distance: instanced, with animation baked or computed on the GPU, switching to full skinned characters up close. Scale did this with vertex-animated instanced crowds (`Scale/src/sim/CrowdRenderer.ts`).
- **Done when:** an example shows 1,000 walking people at 60 fps, swapping near ones to full characters without a visible pop.

### 10. Several moons in the sky

- The physical sky takes a list of moons, each with size, orbit and phase, lighting the night by their phases.
- **Done when:** an example night shows two moons in different phases with their combined moonlight.

## Later

Not needed for the first playable city, but the design depends on them eventually. Keep them in mind so earlier work doesn't block them.

- **Planet terrain:** a cube-sphere with quadtree LOD and 64-bit positions from the planet's centre, so a whole Earth-sized planet can be streamed. Nothing should assume the world is flat or that "up" is always +Y.
- **Sky and atmosphere at altitude:** flying high enough to see the curvature and the atmosphere's edge.
- **Digging and craters in built-up ground:** the voxel terrain exists; this is about craters in streets and cities. Its design is still to be discussed.
- **Touch and gamepad input**, and quality presets for Mac and iPad (wgpu on Metal).
- **A physics-only build** (added 2026-11-08): HeroWorld's server now uses `PhysicsWorld`, but `engine` is one crate, so the server also compiles wgpu, winit, egui and rodio (and needs `libasound2-dev` on Linux). Cargo features (say `render` and `audio`, on by default) would let a dedicated server link only physics.

- **Breakable buildings, from HeroWorld's first use** (added 2026-11-08, M8):
  - Rubble mounds of a collapse that starts above the ground floor sit on the standing storey
    as a wide flat cap that overhangs the walls; they would look better kept to the footprint,
    or poured down its sides.
  - A breakable building cannot carry textures (`BuildingPlan::add` takes a mesh's colours, not
    its albedo or material maps), so a damaged building loses its brick and stone texture.
  - `BuildingPlan::add` re-indexes the whole plan per element (quadratic); fine for ~1000
    elements (2 ms), but a bulk `add_all` would let big towers be planned off the frame.

- **Leaner static meshes** (added 2026-11-08, from a memory check on a 40,000-person town): the
  town's buildings are about 1.2 million faces, and each GPU vertex carries a full
  `SurfaceMaterial` (four float4s, about 64 bytes) next to its position, normal, colour and UV,
  while `World` also keeps the CPU `BuiltMesh` (another ~100 bytes per vertex) after upload.
  The game took about 4.8 GB steady in the cloud's software renderer (where "GPU" memory is RAM
  too). HeroWorld now gives faces their own normals so they build indexed (4 vertices per quad,
  not 6) and merges distant windows, which brought it to about 4.2 GB. What would help most:
  - a per-vertex material index into a small per-mesh table (most meshes use 1 to 4 materials)
    instead of the whole material per vertex;
  - a way to drop the CPU copy of a static mesh after upload (or keep only what picking and the
    z-fight finder need);
  - flat-shaded meshes built indexed without having to set a normal on every point.

- **Articulated models that share their maps** (added 2026-11-08, M11): the colossus is AssetGenerator
  parts on pivots (four thighs, four shins, four knees, the hull, turret, cannon and more),
  and `World::place` takes each placed mesh by value, so every leg keeps its own CPU copy of the
  part's albedo, normal and ORM maps, and the GPU shares a map only between meshes whose
  material numbers also match. With 2048-texel maps the screenshot run ran out of 15 GB, so
  the robot maps are capped at 1024 (2048 for the hull). What would help:
  - placing one loaded model (`ModelScene`) many times, attached to pivots, sharing its maps
    and vertex buffers (like `spawn_lod_like` does for LOD sets);
  - LOD levels for such an attached model, swapped by camera distance, so close-ups can carry
    4096-texel maps and many triangles.
  - Seen in the cloud's software renderer: looking nearly straight down from 15-25 m, shiny
    parts (the robots' chrome and brass, some rooftop domes) render as white glare with black
    specks (NaN-looking); the same parts from the side, or from 50 m up, look right.

- **Lighting a whole town** (added 2026-11-08, from the lighting pass, 0.066):
  - **Far sun shadows for meshes.** Three cascades end at a few hundred metres (HeroWorld sets
    500 m on High), so from the air the town beyond that has no building shadows at all and
    looks like a model. A fourth, cached cascade (re-rendered only when the sun moves or a
    building breaks) or a coarse far shadow map over the content distance would fix it.
  - **Fog volumes that keep the base grade.** A `FogVolume` with a grade lerps from the base
    toward its own; HeroWorld's always-on dystopia volume used `NEUTRAL` and so silently wiped
    any `set_color_grade`. A grade that composes with the base (or an "inherit" option) would
    stop that trap.
  - **A day that runs.** Lit windows are baked into the building meshes, so the hour is fixed
    per session; an emissive strength per mesh or material that can change at runtime would
    let windows switch on at dusk and allow a slow day and night cycle.

## Ground rules

These stay in HeroWorld, not in Engine, so Engine stays a general game engine:

- Networking, replication and game state ownership (HeroWorld uses its own server and `bevy_ecs`).
- Gameplay rules: powers, combat, enemies, incidents, karma.
- City, building and interior generation. Engine provides the destructible building type (feature 3) and the visibility for interiors (feature 5); HeroWorld decides what the buildings look like.

Each feature should come with an example and tests like the rest of Engine, and work headless where it can (features 1 to 3).
