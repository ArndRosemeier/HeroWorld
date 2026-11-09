//! The town's props in the game: one instanced LOD set per model, so tens
//! of thousands of lamps, trees and cars cost a few draws.
//!
//! Trees and shrubs come from Engine's vegetation. Everything else is made
//! here, in the golden age's taste: lanterns, deco double lamps and atomic
//! saucer lamps; finned two-tone cruisers, bubble cars and hover cars;
//! rocket mailboxes, chrome hydrants, deco benches.

use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use engine::vegetation::{shrub_model, tree_model, TreeSpecies};
use glam::{DVec2, DVec3};
use hw_gen::urban::props::{CarModel, Prop, PropKind, Species};
use hw_gen::urban::quarter::DetailId;
use hw_gen::urban::world::TownWorld;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use std::sync::Arc;

/// Edge of one tile of props (m).
const TILE: f64 = 128.0;
/// Props are loaded out to this distance from the camera (beyond the
/// farthest LOD of nearly every model), and dropped a bit beyond it.
pub const PROP_LOAD: f64 = 1000.0;
const PROP_UNLOAD: f64 = PROP_LOAD + 160.0;

type TileKey = (i64, i64);

fn tile_of(p: DVec2) -> TileKey {
    ((p.x / TILE).floor() as i64, (p.y / TILE).floor() as i64)
}

fn tile_box(k: TileKey) -> (DVec2, DVec2) {
    (
        DVec2::new(k.0 as f64, k.1 as f64) * TILE,
        DVec2::new(k.0 as f64 + 1.0, k.1 as f64 + 1.0) * TILE,
    )
}

fn tile_centre(k: TileKey) -> DVec2 {
    (tile_box(k).0 + tile_box(k).1) * 0.5
}

/// The props of one tile.
struct PropTile {
    props: Vec<(DetailId, Prop)>,
    /// Per set: the tile's instances (prop id, place as it stands).
    places: BTreeMap<usize, Vec<(u64, Place)>>,
}

/// The town's props around the camera, streamed in tiles: one instanced
/// LOD set per kind of prop (every kind made up front, so any part of the
/// world can come in), whose instances are the props of the tiles loaded
/// now. A broken prop is the exception that is remembered after its tile
/// goes: it lies broken again when the tile comes back.
pub struct PropView {
    pub world: Arc<TownWorld>,
    pub sets: Vec<LodId>,
    /// The kind each set draws (sorted).
    kinds: Vec<PropKind>,
    /// Each set's model's bounds.
    bounds: Vec<(Vec3, Vec3)>,
    tiles: BTreeMap<TileKey, PropTile>,
    /// Broken props, and which way each fell.
    broken: BTreeMap<u64, u8>,
    /// Sets whose places changed since the last [`PropView::flush`].
    dirty: BTreeSet<usize>,
    /// Goes up whenever tiles come or go (lights and signals follow it).
    pub generation: u64,
}

impl PropView {
    pub fn spawn(world: &mut World, town: Arc<TownWorld>) -> EngineResult<Self> {
        let mut sets = Vec::new();
        let mut bounds = Vec::new();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        for levels in Self::models(threads)? {
            bounds.push(model_bounds(&levels));
            sets.push(world.spawn_lod_at(levels, &[])?);
        }
        Ok(Self::with_sets(town, sets, bounds))
    }

    /// Every kind's model levels, in the order of [`PropKind::all`], grown
    /// on `threads` threads. Pure data, no `World`: the start makes them on
    /// a worker thread.
    pub fn models(threads: usize) -> EngineResult<Vec<Vec<LodLevel>>> {
        let kinds = PropKind::all();
        let threads = threads.clamp(1, kinds.len().max(1));
        // Thread `t` grows kinds t, t + threads, ...; put back in order after.
        let mut grown: Vec<Vec<(usize, EngineResult<Vec<LodLevel>>)>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let kinds = &kinds;
                    s.spawn(move || {
                        (t..kinds.len())
                            .step_by(threads)
                            .map(|i| (i, model_levels(kinds[i])))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("model thread"))
                .collect()
        });
        let mut all: Vec<(usize, EngineResult<Vec<LodLevel>>)> =
            grown.iter_mut().flat_map(std::mem::take).collect();
        all.sort_by_key(|(i, _)| *i);
        assert_eq!(all.len(), kinds.len(), "a model per prop kind");
        all.into_iter().map(|(_, l)| l).collect()
    }

    /// The view over sets already in the world: one per kind of
    /// [`PropKind::all`], in that order, with each model's bounds.
    pub fn with_sets(town: Arc<TownWorld>, sets: Vec<LodId>, bounds: Vec<(Vec3, Vec3)>) -> Self {
        let kinds = PropKind::all();
        assert_eq!(sets.len(), kinds.len(), "one prop set per kind");
        assert_eq!(bounds.len(), kinds.len(), "bounds for every prop set");
        Self {
            world: town,
            sets,
            kinds,
            bounds,
            tiles: BTreeMap::new(),
            broken: BTreeMap::new(),
            dirty: BTreeSet::new(),
            generation: 0,
        }
    }

    /// The LOD set drawing `kind`.
    pub fn set_for(&self, kind: PropKind) -> Option<LodId> {
        self.kinds.binary_search(&kind).ok().map(|k| self.sets[k])
    }

    /// Props loaded now.
    pub fn count(&self) -> usize {
        self.tiles.values().map(|t| t.props.len()).sum()
    }

    /// The loaded props within `r` of `p`, with their ids.
    pub fn near(&self, p: DVec2, r: f64) -> impl Iterator<Item = (DetailId, &Prop)> {
        let (a, b) = (tile_of(p - DVec2::splat(r)), tile_of(p + DVec2::splat(r)));
        self.tiles
            .range((a.0, i64::MIN)..=(b.0, i64::MAX))
            .filter(move |(k, _)| k.1 >= a.1 && k.1 <= b.1)
            .flat_map(|(_, t)| t.props.iter())
            .filter(move |(_, q)| q.pos.distance(p) <= r)
            .map(|(id, q)| (*id, q))
    }

    /// Is prop `id` broken?
    pub fn is_broken(&self, id: u64) -> bool {
        self.broken.contains_key(&id)
    }

    /// Loads tiles coming into range and drops those out of range, nearest
    /// first, for at most `budget_ms` (unlimited: everything now). A tile
    /// whose ground the town has not planned yet waits while it is planned
    /// in the background.
    pub fn update(&mut self, eye: DVec2, budget_ms: f64) {
        let gone: Vec<TileKey> = self
            .tiles
            .keys()
            .copied()
            .filter(|k| tile_centre(*k).distance(eye) > PROP_UNLOAD)
            .collect();
        for k in gone {
            if let Some(t) = self.tiles.remove(&k) {
                self.dirty.extend(t.places.keys().copied());
                self.generation += 1;
            }
        }
        let h = self.world.half() + TILE;
        let (lo, hi) = (
            (eye - DVec2::splat(PROP_LOAD)).max(DVec2::splat(-h)),
            (eye + DVec2::splat(PROP_LOAD)).min(DVec2::splat(h)),
        );
        if lo.x > hi.x || lo.y > hi.y {
            return;
        }
        let (a, b) = (tile_of(lo), tile_of(hi));
        let mut todo: Vec<(f64, TileKey)> = Vec::new();
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                let d = tile_centre((x, z)).distance(eye);
                if d < PROP_LOAD && !self.tiles.contains_key(&(x, z)) {
                    todo.push((d, (x, z)));
                }
            }
        }
        todo.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let wait = budget_ms.is_finite();
        let start = std::time::Instant::now();
        let mut later = Vec::new();
        for (_, k) in todo {
            let (lo, hi) = tile_box(k);
            if wait && !self.world.quarters_ready(lo, hi) {
                later.push(tile_centre(k));
                continue;
            }
            self.load(k);
            crate::hitch::count("prop tiles loaded", 1);
            if start.elapsed().as_secs_f64() * 1000.0 > budget_ms {
                break;
            }
        }
        if !later.is_empty() {
            self.world.prefetch(&later, TILE * 0.5);
        }
    }

    /// How much of the props around `eye` is loaded: (tiles loaded, tiles
    /// wanted).
    pub fn progress(&self, eye: DVec2) -> (usize, usize) {
        let r = DVec2::splat(PROP_LOAD);
        let (a, b) = (tile_of(eye - r), tile_of(eye + r));
        let (mut want, mut have) = (0, 0);
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                if tile_centre((x, z)).distance(eye) < PROP_LOAD {
                    want += 1;
                    have += self.tiles.contains_key(&(x, z)) as usize;
                }
            }
        }
        (have, want)
    }

    fn load(&mut self, k: TileKey) {
        let (lo, hi) = tile_box(k);
        let mut props: Vec<(DetailId, Prop)> = self
            .world
            .quarters_in(lo, hi)
            .iter()
            .flat_map(|q| {
                q.props_with_ids()
                    .filter(|(_, p)| tile_of(p.pos) == k)
                    .map(|(id, p)| (id, *p))
                    .collect::<Vec<_>>()
            })
            .collect();
        props.sort_by_key(|(id, _)| *id);
        let mut places: BTreeMap<usize, Vec<(u64, Place)>> = BTreeMap::new();
        for (id, p) in &props {
            let Ok(set) = self.kinds.binary_search(&p.kind) else {
                continue;
            };
            places
                .entry(set)
                .or_default()
                .push((id.0, self.place_of(set, p, self.broken.get(&id.0).copied())));
        }
        self.dirty.extend(places.keys().copied());
        self.tiles.insert(k, PropTile { props, places });
        self.generation += 1;
    }

    fn place_of(&self, set: usize, p: &Prop, fell: Option<u8>) -> Place {
        let at = prop_place(p);
        match fell {
            Some(fell) => broken_place(p, at, self.bounds[set], fell),
            None => at,
        }
    }

    /// Draws prop `id` broken, fallen the way `fell` says (a yaw byte, see
    /// `hw_sim::props`), or whole again (`None`). Remembered while it is
    /// broken, also when its tile is not loaded.
    pub fn set_broken(&mut self, id: u64, fell: Option<u8>) {
        match fell {
            Some(f) => self.broken.insert(id, f),
            None => self.broken.remove(&id),
        };
        // Only a loaded prop needs drawing again (one loaded later lies as
        // `broken` says); a far one is not looked up, which would plan its
        // ground.
        let Some((k, p)) = self.tiles.iter().find_map(|(k, t)| {
            t.props
                .iter()
                .find(|(i, _)| i.0 == id)
                .map(|(_, p)| (*k, *p))
        }) else {
            return;
        };
        let Ok(set) = self.kinds.binary_search(&p.kind) else {
            return;
        };
        let place = self.place_of(set, &p, fell);
        if let Some(slot) = self
            .tiles
            .get_mut(&k)
            .and_then(|t| t.places.get_mut(&set))
            .and_then(|v| v.iter_mut().find(|(i, _)| *i == id))
        {
            slot.1 = place;
            self.dirty.insert(set);
        }
    }

    /// Sends changed places to Engine.
    pub fn flush(&mut self, world: &mut World) -> EngineResult<()> {
        for k in std::mem::take(&mut self.dirty) {
            let places: Vec<Place> = self
                .tiles
                .values()
                .filter_map(|t| t.places.get(&k))
                .flatten()
                .map(|(_, p)| *p)
                .collect();
            crate::hitch::count("prop kinds re-sent", 1);
            crate::hitch::count("prop places re-sent", places.len() as u64);
            world.set_lod_places(self.sets[k], &places)?;
        }
        Ok(())
    }
}

/// Where a broken prop lies: posts and trees fallen over (their foot where
/// it stood, laid along `fell`), cars on their roofs, benches, bins, shrubs
/// and kiosks crushed flat.
fn broken_place(p: &Prop, whole: Place, (_, hi): (Vec3, Vec3), fell: u8) -> Place {
    use hw_sim::props::{byte_to_yaw, toughness, Breaks};
    let Some((_, how)) = toughness(p.kind) else {
        return whole;
    };
    let yaw = byte_to_yaw(fell);
    let s = p.scale as f32;
    match how {
        Breaks::Topples => {
            // Pitch +90 turns the model's +y onto its +z, which the yaw turns
            // along the fall. Trees lean on
            // their crowns rather than lying flat.
            // (Its +z, the front, ends up underneath.)
            let tree = matches!(p.kind, PropKind::Tree(..));
            let pitch = if tree { 76.0 } else { 90.0 };
            let lift = if tree { 0.0 } else { hi.z.max(0.05) * s };
            Place {
                position: whole.position + Vec3::Y * lift,
                yaw_degrees: yaw.to_degrees() as f32,
                pitch_degrees: pitch,
                ..whole
            }
        }
        Breaks::Flips => {
            let shove = Vec3::new(yaw.sin() as f32, 0.0, yaw.cos() as f32) * 0.6;
            Place {
                position: whole.position + shove + Vec3::Y * (hi.y * s),
                roll_degrees: 180.0,
                ..whole
            }
        }
        Breaks::Smashed => Place {
            stretch: Vec3::new(1.15, 0.18, 1.15),
            tint: Color::rgb(140, 133, 128),
            ..whole
        },
    }
}

/// Where a prop stands, as Engine places it.
pub fn prop_place(p: &Prop) -> Place {
    let yaw = p.dir.x.atan2(p.dir.y).to_degrees();
    let at = Place::new(p.pos.x as f32, p.y as f32, p.pos.y as f32)
        .with_yaw_deg(yaw as f32)
        .with_scale(p.scale as f32);
    match p.kind {
        // Bushes grown in a row, clipped into a hedge.
        // Clipped box is a deeper, warmer green than the loose bushes.
        PropKind::Hedge(_) => Place {
            stretch: HEDGE_STRETCH,
            tint: Color::rgb01_unchecked(0.82, 1.0, 0.62),
            ..at
        },
        _ => at,
    }
}

/// How a hedge's bushes are clipped (see [`hedge_levels`]).
const HEDGE_STRETCH: Vec3 = Vec3::new(0.62, 0.58, 0.34);

/// Box bushes planted close in a row: one piece of hedge
/// ([`hw_gen::urban::props::HEDGE_LEN`] long once clipped by
/// [`HEDGE_STRETCH`]), from Engine's evergreen shrubs.
fn hedge_levels(v: u8) -> EngineResult<Vec<LodLevel>> {
    // Odd shrub variants are evergreen boxwood.
    let bushes: Vec<_> = (0..3)
        .map(|k| shrub_model(1 + 2 * ((v as u32 + k) % 4)))
        .collect::<EngineResult<_>>()?;
    let row = |pick: &dyn Fn(&engine::vegetation::TreeModel) -> &Mesh| -> EngineResult<Mesh> {
        let mut m = pick(&bushes[0]).clone();
        m.append_translated(pick(&bushes[1]), Vec3::new(-1.25, 0.0, 0.05))?;
        m.append_translated(pick(&bushes[2]), Vec3::new(1.25, 0.0, -0.05))?;
        Ok(m)
    };
    Ok(vec![
        LodLevel::new(row(&|t| &t.near)?, 30.0),
        LodLevel::new(row(&|t| &t.mid)?, 90.0),
        LodLevel::new(row(&|t| &t.far)?, 350.0),
    ])
}

fn species(s: Species) -> TreeSpecies {
    match s {
        Species::Plane => TreeSpecies::Plane,
        Species::Linden => TreeSpecies::Linden,
        Species::Maple => TreeSpecies::Maple,
        Species::Oak => TreeSpecies::Oak,
        Species::Birch => TreeSpecies::Birch,
        Species::Pine => TreeSpecies::Pine,
        Species::Palm => TreeSpecies::Palm,
        Species::Cypress => TreeSpecies::Cypress,
        Species::Chestnut => TreeSpecies::Chestnut,
        Species::Ginkgo => TreeSpecies::Ginkgo,
    }
}

/// The detail levels of one kind of prop.
/// A model's bounds, from its closest level.
pub fn model_bounds(levels: &[LodLevel]) -> (Vec3, Vec3) {
    levels
        .first()
        .and_then(|l| l.mesh.bounds())
        .unwrap_or((Vec3::ZERO, Vec3::ONE))
}

pub fn model_levels(kind: PropKind) -> EngineResult<Vec<LodLevel>> {
    use crate::propmesh as pm;
    let three = |make: &dyn Fn(Lod) -> Mesh, d: [f32; 3]| {
        vec![
            LodLevel::new(make(Lod::Near), d[0]),
            LodLevel::new(make(Lod::Mid), d[1]),
            LodLevel::new(make(Lod::Far), d[2]),
        ]
    };
    Ok(match kind {
        PropKind::Tree(s, v) => tree_model(species(s), v as u32)?.lod_levels(),
        PropKind::Shrub(v) => shrub_model(v as u32)?.lod_levels(),
        PropKind::Hedge(v) => hedge_levels(v)?,
        PropKind::Lamp(style) => three(&|l| pm::lamp(style, l), [30.0, 140.0, 700.0]),
        PropKind::Car(m, paint) => parked_car_levels(m, paint),
        PropKind::TrafficLight => three(&pm::traffic_light, [30.0, 160.0, 500.0]),
        PropKind::Fountain => three(&pm::fountain, [40.0, 200.0, 900.0]),
        PropKind::Bench => three(&pm::bench, [25.0, 80.0, 160.0]),
        PropKind::Bin => three(&pm::bin, [20.0, 60.0, 120.0]),
        PropKind::Hydrant => three(&pm::hydrant, [20.0, 60.0, 120.0]),
        PropKind::Mailbox => three(&pm::mailbox, [25.0, 80.0, 180.0]),
        PropKind::Bollard => three(&pm::bollard, [20.0, 60.0, 150.0]),
        PropKind::Kiosk => three(&pm::kiosk, [40.0, 200.0, 600.0]),
        PropKind::AdPillar => three(&pm::ad_pillar, [30.0, 120.0, 250.0]),
        PropKind::Statue(pose) => vec![LodLevel::new(statue_mesh(pose), 500.0)],
        PropKind::Barricade(piece) => crate::edgeview::piece_levels(piece),
    })
}

// ---------------------------------------------------------------- shapes

fn bx(b: &mut MeshBuilder, lo: DVec3, hi: DVec3, c: Color) {
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    b.oriented_box((lo + hi) * 0.5, DVec3::X, (hi - lo) * 0.5, c);
}

/// A closed ellipsoid.
fn ellipsoid(b: &mut MeshBuilder, c: DVec3, r: DVec3, seg: usize, rings: usize, col: Color) {
    // Smooth, with at least enough sides to read round up close.
    let seg = (seg * 2).max(16);
    let rings = (rings * 2).max(8);
    let rows: Vec<Vec<DVec3>> = (0..=rings)
        .map(|k| {
            let e = std::f64::consts::PI * (k as f64 / rings as f64 - 0.5);
            (0..seg)
                .map(|i| {
                    let a = TAU * i as f64 / seg as f64;
                    c + DVec3::new(
                        r.x * e.cos() * a.cos(),
                        r.y * e.sin(),
                        r.z * e.cos() * a.sin(),
                    )
                })
                .collect()
        })
        .collect();
    let surf = b.surf;
    crate::carmesh::surface(b, &rows, true, |_, _| Some((surf, col)));
}

fn metal(b: &mut MeshBuilder, f: impl FnOnce(&mut MeshBuilder)) {
    b.surf = Surf::Metal;
    f(b);
    b.surf = Surf::Plain;
}

// ---------------------------------------------------------------- cars

pub use crate::carmesh::wheel_layout;
use crate::carmesh::{ambulance_into, car_into, taxi_into, wheel, Lod};

fn car_mesh(model: CarModel, paint: u8, lod: Lod, wheels: bool) -> Mesh {
    let mut b = MeshBuilder::new();
    car_into(&mut b, model, paint, lod, wheels);
    b.finish()
}

/// A parked golden-age car, its nose to +z: close up, down the street, far.
fn parked_car_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_mesh(model, paint, Lod::Near, true), 35.0),
        LodLevel::new(car_mesh(model, paint, Lod::Mid, true), 140.0),
        LodLevel::new(car_mesh(model, paint, Lod::Far, true), 650.0),
    ]
}

/// A parked car whose doors open: the car without them (wheels on), with
/// [`car_door_levels`] drawn at their hinges.
pub fn parked_car_shell_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        crate::carmesh::car_shell_into(&mut b, model, paint, lod, true);
        b.finish()
    };
    vec![
        LodLevel::new(make(Lod::Near), 35.0),
        LodLevel::new(make(Lod::Mid), 140.0),
        LodLevel::new(make(Lod::Far), 650.0),
    ]
}

/// A car's opening doors (see [`crate::carmesh::car_doors`]), each as LOD
/// levels in its hinge's frame, with the hinge's place on the car, the axis
/// a positive angle opens it about, and how far it opens (radians).
pub fn car_door_levels(model: CarModel, paint: u8) -> Vec<(Vec<LodLevel>, DVec3, DVec3, f64)> {
    use crate::carmesh::car_doors;
    let near = car_doors(model, paint, Lod::Near);
    let mid = car_doors(model, paint, Lod::Mid);
    let far = car_doors(model, paint, Lod::Far);
    near.into_iter()
        .zip(mid)
        .zip(far)
        .map(|((n, m), f)| {
            (
                vec![
                    LodLevel::new(n.mesh, 35.0),
                    LodLevel::new(m.mesh, 140.0),
                    LodLevel::new(f.mesh, 650.0),
                ],
                n.hinge,
                n.axis,
                n.open,
            )
        })
        .collect()
}

/// A car driving in traffic: its body (the wheels turn on their own, see
/// [`wheel_levels`], as far as traffic draws them), then far off with
/// wheels of its own.
pub fn moving_car_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_mesh(model, paint, Lod::Near, false), 35.0),
        LodLevel::new(car_mesh(model, paint, Lod::Mid, false), 120.0),
        LodLevel::new(car_mesh(model, paint, Lod::Far, true), 700.0),
    ]
}

/// The ambulance that waits for a rescue's hurt: a long 1950s coach-built
/// ambulance (see [`crate::carmesh::ambulance_into`]). Nose to +z, wheels
/// on the ground; close up, down the street and far.
pub fn ambulance_levels() -> Vec<LodLevel> {
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        ambulance_into(&mut b, lod);
        b.finish()
    };
    vec![
        LodLevel::new(make(Lod::Near), 40.0),
        LodLevel::new(make(Lod::Mid), 160.0),
        LodLevel::new(make(Lod::Far), 700.0),
    ]
}

/// One wheel of `model`, centred, its axle along x: tyre, whitewalls and a
/// chrome hubcap with a spinner on both faces, so it can be seen to turn.
pub fn wheel_levels(model: CarModel) -> Option<Vec<LodLevel>> {
    let (_, _, r) = wheel_layout(model)?;
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        wheel(&mut b, DVec3::ZERO, r, 0.22, lod);
        b.finish()
    };
    Some(vec![
        LodLevel::new(make(Lod::Near), 35.0),
        LodLevel::new(make(Lod::Mid), 260.0),
    ])
}

/// Flying taxis: close up, in the next lane, far.
pub fn taxi_levels() -> Vec<LodLevel> {
    taxi_body(true)
}

/// A taxi without its canopy, for one whose canopy opens: draw it with
/// [`taxi_door_levels`].
pub fn taxi_open_levels() -> Vec<LodLevel> {
    taxi_body(false)
}

fn taxi_body(canopy: bool) -> Vec<LodLevel> {
    let make = |lod: Lod| {
        let mut b = MeshBuilder::new();
        taxi_into(&mut b, lod, canopy);
        b.finish()
    };
    vec![
        LodLevel::new(make(Lod::Near), 50.0),
        LodLevel::new(make(Lod::Mid), 250.0),
        LodLevel::new(make(Lod::Far), 1200.0),
    ]
}

/// The taxi's canopy as its own model, in its hinge's frame, with the
/// hinge's place on the taxi and its axis (see [`crate::carmesh::taxi_door`]).
pub fn taxi_door_levels() -> (Vec<LodLevel>, DVec3, DVec3) {
    let (near, hinge, axis) = crate::carmesh::taxi_door(Lod::Near);
    let levels = vec![
        LodLevel::new(near, 50.0),
        LodLevel::new(crate::carmesh::taxi_door(Lod::Mid).0, 250.0),
        LodLevel::new(crate::carmesh::taxi_door(Lod::Far).0, 1200.0),
    ];
    (levels, hinge, axis)
}

/// A private hover car flying in a sky lane.
pub fn sky_car_levels(paint: u8) -> Vec<LodLevel> {
    vec![
        LodLevel::new(car_mesh(CarModel::Hover, paint, Lod::Near, false), 50.0),
        LodLevel::new(car_mesh(CarModel::Hover, paint, Lod::Mid, false), 250.0),
        LodLevel::new(car_mesh(CarModel::Hover, paint, Lod::Far, false), 1200.0),
    ]
}

// ---------------------------------------------------------------- street

/// A bronze hero on a stone plinth, facing +z.
pub fn statue_mesh(pose: u8) -> Mesh {
    let mut b = MeshBuilder::new();
    statue(&mut b, pose);
    b.finish()
}

/// A bronze hero on a stone plinth: fist to the sky (0), hands on hips (1)
/// or taking off, one arm forward (2).
pub fn statue(b: &mut MeshBuilder, pose: u8) {
    let stone = rgb(200, 194, 182);
    let bronze = rgb(126, 92, 52);
    bx(
        b,
        DVec3::new(-1.0, 0.0, -1.0),
        DVec3::new(1.0, 0.25, 1.0),
        stone,
    );
    bx(
        b,
        DVec3::new(-0.8, 0.25, -0.8),
        DVec3::new(0.8, 1.6, 0.8),
        stone,
    );
    bx(
        b,
        DVec3::new(-0.9, 1.6, -0.9),
        DVec3::new(0.9, 1.75, 0.9),
        stone,
    );
    // A gold plaque in front.
    metal(b, |b| {
        bx(
            b,
            DVec3::new(-0.45, 0.7, 0.8),
            DVec3::new(0.45, 1.15, 0.83),
            rgb(214, 170, 72),
        )
    });
    let y0 = 1.75;
    let fly = pose == 2;
    metal(b, |b| {
        // Legs apart (one bent back when taking off).
        for side in [-1.0, 1.0] {
            let back = if fly && side > 0.0 { -0.35 } else { 0.0 };
            ellipsoid(
                b,
                DVec3::new(0.2 * side, y0 + 0.55, back),
                DVec3::new(0.13, 0.55, 0.14),
                8,
                5,
                bronze,
            );
        }
        // Torso, chest out; head.
        ellipsoid(
            b,
            DVec3::new(0.0, y0 + 1.45, 0.0),
            DVec3::new(0.36, 0.48, 0.22),
            10,
            6,
            bronze,
        );
        ellipsoid(
            b,
            DVec3::new(0.0, y0 + 2.1, 0.02),
            DVec3::new(0.13, 0.16, 0.14),
            8,
            5,
            bronze,
        );
        // Arms.
        let arm = |b: &mut MeshBuilder, side: f64, to: DVec3| {
            let sh = DVec3::new(0.42 * side, y0 + 1.75, 0.0);
            // Three beads from shoulder to fist.
            for t in [0.15, 0.5, 0.85] {
                let p = sh.lerp(to, t);
                ellipsoid(b, p, DVec3::splat(0.1 + 0.02 * (1.0 - t)), 6, 4, bronze);
            }
        };
        match pose {
            0 => {
                arm(b, 1.0, DVec3::new(0.55, y0 + 2.6, 0.05));
                arm(b, -1.0, DVec3::new(-0.38, y0 + 1.2, 0.1));
            }
            1 => {
                arm(b, 1.0, DVec3::new(0.3, y0 + 1.2, -0.05));
                arm(b, -1.0, DVec3::new(-0.3, y0 + 1.2, -0.05));
            }
            _ => {
                arm(b, 1.0, DVec3::new(0.35, y0 + 2.35, 0.6));
                arm(b, -1.0, DVec3::new(-0.45, y0 + 1.25, -0.2));
            }
        }
        // Fists.
        let fist = match pose {
            0 => DVec3::new(0.55, y0 + 2.62, 0.05),
            1 => DVec3::new(0.3, y0 + 1.15, -0.05),
            _ => DVec3::new(0.35, y0 + 2.4, 0.65),
        };
        ellipsoid(b, fist, DVec3::splat(0.12), 6, 4, bronze);
        // The cape, flowing back.
        let sway = if fly { 0.9 } else { 0.45 };
        b.quad(
            [
                DVec3::new(-0.38, y0 + 1.85, -0.2),
                DVec3::new(0.38, y0 + 1.85, -0.2),
                DVec3::new(0.55, y0 + 0.35, -0.2 - sway),
                DVec3::new(-0.55, y0 + 0.35, -0.2 - sway),
            ],
            -DVec3::Z + DVec3::Y * 0.3,
            bronze,
        );
        b.quad(
            [
                DVec3::new(-0.38, y0 + 1.85, -0.18),
                DVec3::new(0.38, y0 + 1.85, -0.18),
                DVec3::new(0.55, y0 + 0.35, -0.18 - sway),
                DVec3::new(-0.55, y0 + 0.35, -0.18 - sway),
            ],
            DVec3::Z - DVec3::Y * 0.3,
            bronze,
        );
    });
}

/// A cast-iron mooring bollard: a squat post with a flared cap.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_made_model_has_faces() {
        use hw_gen::urban::props::{CarModel::*, LampStyle::*};
        let kinds = [
            PropKind::Lamp(Lantern),
            PropKind::Lamp(Deco),
            PropKind::Lamp(Atomic),
            PropKind::Car(Cruiser, 0),
            PropKind::Car(Bubble, 3),
            PropKind::Car(Hover, 7),
            PropKind::TrafficLight,
            PropKind::Bench,
            PropKind::Bin,
            PropKind::Hydrant,
            PropKind::Mailbox,
            PropKind::Fountain,
            PropKind::Bollard,
            PropKind::Kiosk,
            PropKind::AdPillar,
            PropKind::Statue(0),
            PropKind::Statue(1),
            PropKind::Statue(2),
        ];
        for k in kinds {
            let levels = model_levels(k).unwrap();
            assert!(levels.iter().all(|l| l.mesh.face_count() > 0), "{k:?}");
        }
    }
}
