//! A town in the game: Engine streams its ground and water from the town's
//! heights ([`TownGround`]), and [`TownView`] keeps the roads and buildings
//! around the camera built, in square cells, full detail close by.

use crate::buildingmesh::Detail;
use crate::citymesh::{add_building, Look, MeshBuilder, BLOCK_TOP};
use engine::prelude::*;
use engine::stream::WorldGenerator;
use engine::texgen::GroundLayer;
use glam::{DVec2, DVec3, Vec3};
use hw_gen::building::BuildingPlan;
use hw_gen::urban::greenery::Greenery;
use hw_gen::urban::props::PropKind;
use hw_gen::urban::world::{TownWorld, WorldBuilding, WorldRoad, WATER_LEVEL};
use hw_sim::collision::Inside;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Edge of one cell of roads and buildings (m).
pub const CELL: f64 = 192.0;
/// Cells are built out to this distance from the camera, and dropped a bit
/// beyond it.
pub const LOAD: f64 = 1300.0;
const UNLOAD: f64 = LOAD + 250.0;
/// Buildings within about this distance of the camera get full detail.
pub const NEAR_DETAIL: f64 = 220.0;
/// Beyond about this distance windows merge into bands.
pub const DISTANT_DETAIL: f64 = 620.0;
/// Buildings within this distance of the cut focus are cut open.
pub const CUT_RADIUS: f64 = 110.0;
/// Faces per mesh, well inside a GPU buffer.
const MESH_FACES: usize = 250_000;

/// The town's ground and water for Engine's world streaming, and the woods
/// of the open country, streamed in tiles around the camera.
pub struct TownGround {
    pub town: Arc<TownWorld>,
    pub green: Arc<Greenery>,
    /// The LOD set each tree kind is drawn with (templates the tiles copy).
    pub trees: BTreeMap<PropKind, LodId>,
}

impl TownGround {
    fn world(&self) -> &TownWorld {
        &self.town
    }
}

impl WorldGenerator for TownGround {
    fn height(&self, x: f32, z: f32) -> f32 {
        self.town.ground(DVec2::new(x as f64, z as f64)) as f32
    }

    /// Woods and field trees outside the town.
    fn populate(&self, tile: &mut engine::stream::Tile) -> EngineResult<()> {
        let (lo, hi) = (tile.min(), tile.max());
        let trees = self.green.wild_trees(
            &self.town,
            DVec2::new(lo.x as f64, lo.y as f64),
            DVec2::new(hi.x as f64, hi.y as f64),
        );
        for t in trees {
            if let Some(&model) = self.trees.get(&t.kind) {
                tile.place(model, crate::propview::prop_place(&t));
            }
        }
        Ok(())
    }

    /// The utility tunnels, lift shafts and Molehaven carved out of the rock.
    fn density(&self, p: glam::Vec3, ground: f32) -> f32 {
        crate::deepview::density(self.world(), p, ground)
    }

    fn sea(&self) -> Option<engine::water::Sea> {
        self.world()
            .coastal()
            .then(|| engine::water::Sea::calm(WATER_LEVEL as f32))
    }

    fn water_level(&self, _x: f32, _z: f32) -> Option<f32> {
        // Rivers and lakes stand at sea level; the land rises from them.
        Some(WATER_LEVEL as f32)
    }

    /// Engine's water fills a window around the camera and draws the sea to
    /// the horizon: a coastal town uses it for all its water. Inland, the
    /// rivers and lakes are drawn by [`TownView`] across the whole town.
    fn has_water(&self) -> bool {
        self.world().coastal()
    }
}

type CellKey = (i64, i64);

fn cell_of(p: DVec2) -> CellKey {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

fn cell_centre(k: CellKey) -> DVec2 {
    DVec2::new((k.0 as f64 + 0.5) * CELL, (k.1 as f64 + 0.5) * CELL)
}

fn cell_box(k: CellKey) -> (DVec2, DVec2) {
    (
        DVec2::new(k.0 as f64, k.1 as f64) * CELL,
        DVec2::new(k.0 as f64 + 1.0, k.1 as f64 + 1.0) * CELL,
    )
}

/// How far a building's centre can lie from its district's box (m).
const BUILDING_MARGIN: f64 = 60.0;

/// Is `id` a building of the town's streamed detail (not a landmark or
/// another fixed thing)?
fn town_building(id: u64) -> bool {
    id >> 32 < hw_sim::collision::FIXED >> 32
}

/// The roads and buildings around the camera. Nothing is kept for the
/// whole town: each cell asks the town for the detail on its ground when it
/// comes into range (the town plans it then, or has it ready), and forgets
/// it when it leaves.
pub struct TownView {
    pub world: Arc<TownWorld>,
    /// The buildings of each built cell, by id.
    buildings: BTreeMap<CellKey, Vec<(u64, WorldBuilding)>>,
    /// Plans of the buildings in built cells (they take a moment each).
    plans: BTreeMap<u64, Arc<BuildingPlan>>,
    entities: BTreeMap<CellKey, Vec<EntityId>>,
    /// Detail each built cell was drawn in.
    built: BTreeMap<CellKey, Detail>,
    pub cut_on: bool,
    pub cut_focus: DVec2,
    pub cut_level: u32,
    /// The building and floor our hero is in: drawn open at that floor.
    pub inside: Option<Inside>,
    /// Damaged buildings: Engine draws them breakable instead (see
    /// `damageview`), so the cells leave them out.
    hidden: BTreeSet<u64>,
    /// The lit rooms of the floors drawn open, and what they were for
    /// (see [`Self::room_lamps`]).
    rooms: (Option<RoomsKey>, Vec<engine::lamps::Lamp>),
    /// Places to stand indoors, by the cell that drew them.
    spots: BTreeMap<CellKey, Vec<IndoorSpots>>,
    /// Doors and fixed entrance glass each cell shows (see `doorview`).
    doors: BTreeMap<CellKey, CellDoors>,
    /// Goes up whenever a cell's doors come or go.
    door_generation: u64,
    /// Where grass grows (the lawns drawn in each cell).
    pub green: Arc<Greenery>,
    /// Worker threads drawing cells off the main thread, so a frame never
    /// waits for a cell (one took 40 to 800 ms on the main thread).
    workers: Option<Workers>,
    /// The newest job handed out for each cell: an older one coming back
    /// late (the cut moved again meanwhile) is thrown away.
    stamps: BTreeMap<CellKey, u64>,
    next_stamp: u64,
    /// A finished cell going into the world a few meshes per frame (see
    /// [`Placing`]).
    placing: Option<Placing>,
}

/// Meshes of one cell put into the world per frame. Engine uploads each new
/// mesh in the frame after its spawn (about a millisecond each); a whole
/// cell (25 to 40 meshes) at once stopped a frame for 30 to 250 ms.
const MESHES_PER_FRAME: usize = 6;

/// Where a cell's meshes wait, out of sight, until all of them are uploaded:
/// then they move into place together and the old drawing goes in the same
/// frame, so a cell never shows half drawn or twice.
const PARKED_Y: f32 = -100_000.0;

/// A finished cell going into the world over several frames.
struct Placing {
    key: CellKey,
    detail: Detail,
    /// Still to spawn: the mesh and whether it is flat ground (no shadow).
    meshes: Vec<(PreparedMesh, bool)>,
    /// Spawned so far, parked at [`PARKED_Y`].
    ids: Vec<EntityId>,
    plans: Vec<(u64, Arc<BuildingPlan>)>,
    spots: Vec<IndoorSpots>,
    doors: CellDoors,
}

/// The doors and fixed entrance glass of one cell.
#[derive(Default)]
struct CellDoors {
    doors: Vec<crate::doorview::DoorSpot>,
    panes: Vec<crate::doorview::PaneSpot>,
}

/// What one cell needs to be drawn, sent to a worker.
struct CellJob {
    key: CellKey,
    stamp: u64,
    detail: Detail,
    world: Arc<TownWorld>,
    green: Arc<Greenery>,
    roads: Vec<WorldRoad>,
    buildings: Vec<CellBuilding>,
}

struct CellBuilding {
    id: u64,
    building: WorldBuilding,
    /// Its plan, if one was made already (the worker makes the rest).
    plan: Option<Arc<BuildingPlan>>,
    look: Look,
    hidden: bool,
}

/// A cell's meshes, ready to be put into the world.
struct CellMeshes {
    key: CellKey,
    stamp: u64,
    detail: Detail,
    /// Prepared on the worker (`Mesh::prepare`): taking a cell into the
    /// world was 20-60 ms on the main thread when the meshes were built
    /// and their facade maps hashed there.
    solid: Vec<PreparedMesh>,
    lawns: Vec<PreparedMesh>,
    water: Option<PreparedMesh>,
    /// Plans the worker had to make, for the cache.
    plans: Vec<(u64, Arc<BuildingPlan>)>,
    /// Places to stand on the floors drawn open: building, floor, places.
    spots: Vec<IndoorSpots>,
    doors: CellDoors,
}

/// The places to stand on one building's floor drawn open.
#[derive(Clone, Debug)]
pub struct IndoorSpots {
    pub building: u64,
    pub level: u32,
    pub spots: Vec<crate::citymesh::Spot>,
}

/// Jobs waiting for a worker: redraws (the cut, walking into a building)
/// go in front of the cells streaming in.
#[derive(Default)]
struct Queue {
    jobs: std::collections::VecDeque<CellJob>,
    closed: bool,
}

/// A few threads turning cell jobs into meshes. Dropping it stops them.
struct Workers {
    queue: Arc<(std::sync::Mutex<Queue>, std::sync::Condvar)>,
    done: std::sync::mpsc::Receiver<CellMeshes>,
    busy: BTreeSet<CellKey>,
    count: usize,
}

impl Workers {
    fn start() -> Self {
        let queue = Arc::new((
            std::sync::Mutex::new(Queue::default()),
            std::sync::Condvar::new(),
        ));
        let (tx, done) = std::sync::mpsc::channel();
        // Leave a core or two for the frame itself.
        let count = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .saturating_sub(2)
            .clamp(1, 4);
        for _ in 0..count {
            let (queue, tx) = (queue.clone(), tx.clone());
            std::thread::spawn(move || loop {
                let job = {
                    let (lock, ready) = &*queue;
                    let Ok(mut q) = lock.lock() else { return };
                    loop {
                        if q.closed {
                            return;
                        }
                        if let Some(job) = q.jobs.pop_front() {
                            break job;
                        }
                        q = match ready.wait(q) {
                            Ok(q) => q,
                            Err(_) => return,
                        };
                    }
                };
                if tx.send(make_cell(job)).is_err() {
                    return;
                }
            });
        }
        Self {
            queue,
            done,
            busy: BTreeSet::new(),
            count,
        }
    }

    /// Hands out a job; `urgent` ones are drawn before the rest.
    fn send(&mut self, job: CellJob, urgent: bool) {
        self.busy.insert(job.key);
        let (lock, ready) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            if urgent {
                q.jobs.push_front(job);
            } else {
                q.jobs.push_back(job);
            }
            ready.notify_one();
        }
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        let (lock, ready) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.closed = true;
            q.jobs.clear();
        }
        ready.notify_all();
    }
}

/// Draws one cell: roads, buildings, lawns and inland water. Pure, so it
/// runs on any thread.
fn make_cell(job: CellJob) -> CellMeshes {
    let CellJob {
        key: k,
        stamp,
        detail,
        world,
        green,
        roads,
        buildings,
    } = job;
    let (lo, hi) = cell_box(k);
    let mut solid = Vec::new();
    let mut plans = Vec::new();
    let mut spots = Vec::new();
    let mut doors = CellDoors::default();
    // Roads first, one mesh for the cell.
    let mut b = MeshBuilder::new();
    for r in &roads {
        for run in runs_in(r, k) {
            draw_road(&mut b, &world, r, &run);
        }
    }
    // Where road ends meet at an angle, a round plate fills the wedges
    // between their square ends; each plate drawn by the cell holding it.
    for j in hw_gen::urban::surface::road_joins(&roads) {
        if cell_of(j.at) == k {
            draw_join(&mut b, &world, &roads, &j);
        }
    }
    // Buildings: each drawn at the town's block height, then lifted to
    // its own ground, on a plinth where the ground falls away.
    for cb in buildings {
        let wb = &cb.building;
        let plan = match cb.plan {
            Some(p) => p,
            None => {
                let p = Arc::new(BuildingPlan::generate(&wb.site));
                plans.push((cb.id, p.clone()));
                p
            }
        };
        if !cb.hidden {
            let mut one = MeshBuilder::new();
            add_building(&mut one, &plan, wb.site.district, cb.look, detail);
            let lift = Vec3::new(0.0, (wb.base - BLOCK_TOP) as f32, 0.0);
            if let Look::Cut { level } = cb.look {
                let mut list = std::mem::take(&mut one.spots);
                for s in &mut list {
                    s.at.y += lift.y as f64;
                }
                if !list.is_empty() {
                    spots.push(IndoorSpots {
                        building: cb.id,
                        level: level.min(plan.floors.len().saturating_sub(1) as u32),
                        spots: list,
                    });
                }
            }
            b.append(one, lift);
            // Up close, the doors are their own moving leaves.
            if detail == Detail::Near {
                let (d, p) = crate::doorview::door_spots(&plan, cb.id, wb.base, cb.look);
                doors.doors.extend(d);
                doors.panes.extend(p);
            }
        }
        if wb.base - wb.low > 0.05 {
            // The plinth is the building's own base stone.
            b.tex = Some(crate::facades::facade(&plan).base);
            b.prism(
                &wb.site.corners(),
                wb.low - 1.0,
                wb.base,
                rgb(150, 142, 128),
            );
            b.tex = None;
        }
        if b.face_count() > MESH_FACES {
            solid.extend(std::mem::take(&mut b).finish_parts());
        }
    }
    if !b.is_empty() {
        solid.extend(b.finish_parts());
    }
    // Lawns and meadows, finer close by.
    // (Across town, past DISTANT_DETAIL, Engine's own green ground is
    // enough under the haze.)
    let step = match detail {
        Detail::Near => 2.0,
        Detail::Far => 5.0,
        Detail::Distant => 0.0,
    };
    let lawns = if step == 0.0 || std::env::var("HW_LAWN").is_ok_and(|v| v == "0") {
        Vec::new()
    } else {
        crate::lawnmesh::lawn(&world, &green, lo, hi, step)
    };
    // Inland water (a coastal town's comes from Engine's sea).
    let water = if world.coastal() {
        None
    } else {
        water_mesh(&world, k)
    };
    CellMeshes {
        key: k,
        stamp,
        detail,
        solid: solid.into_iter().map(Mesh::prepare).collect(),
        lawns: lawns.into_iter().map(Mesh::prepare).collect(),
        water: water.map(Mesh::prepare),
        plans,
        spots,
        doors,
    }
}

impl TownView {
    pub fn new(world: Arc<TownWorld>) -> Self {
        let green = Arc::new(Greenery::new(&world));
        let mut v = Self {
            world,
            buildings: BTreeMap::new(),
            plans: BTreeMap::new(),
            entities: BTreeMap::new(),
            built: BTreeMap::new(),
            cut_on: false,
            cut_focus: DVec2::ZERO,
            cut_level: 1,
            inside: None,
            hidden: BTreeSet::new(),
            rooms: (None, Vec::new()),
            spots: BTreeMap::new(),
            doors: BTreeMap::new(),
            door_generation: 0,
            green,
            workers: None,
            stamps: BTreeMap::new(),
            placing: None,
            next_stamp: 0,
        };
        v.cut_focus = v
            .tallest_near(v.world.spawn_line().0, 300.0)
            .and_then(|i| v.centre_of(i))
            .unwrap_or(DVec2::ZERO);
        v
    }

    /// Buildings in the cells built now.
    pub fn building_count(&self) -> usize {
        self.buildings.values().map(Vec::len).sum()
    }

    /// Is `id` a building of this town?
    pub fn has_building(&self, id: u64) -> bool {
        town_building(id)
            && self
                .world
                .building(hw_gen::urban::quarter::DetailId(id))
                .is_some()
    }

    /// Engine lamps for the lit rooms of the floors drawn open (the cut
    /// view and the building our hero is in), nearest `eye` first, and a
    /// key that changes when they do. Picked again only when the cut, the
    /// floor or the quarter hour change; the plans are already built, so
    /// this is a short walk over a few dozen buildings.
    pub fn room_lamps(&mut self, eye: DVec2, hour: f32) -> (u64, &[engine::lamps::Lamp]) {
        const MOST: usize = 32;
        let key = RoomsKey {
            cut: self.cut_on.then(|| {
                (
                    (self.cut_focus.x / 8.0).round() as i64,
                    (self.cut_focus.y / 8.0).round() as i64,
                    self.cut_level,
                )
            }),
            inside: self.inside.map(|i| (i.building, i.level)),
            hour: (hour * 20.0).round() as i32,
            eye: ((eye.x / 16.0).round() as i64, (eye.y / 16.0).round() as i64),
        };
        if self.rooms.0 != Some(key) {
            let mut open: Vec<(u64, u32)> = Vec::new();
            if let Some(i) = self.inside {
                open.push((i.building, i.level));
            }
            if self.cut_on {
                let (p, r) = (self.cut_focus, CUT_RADIUS);
                let m = DVec2::splat(r + BUILDING_MARGIN);
                let mut near: Vec<(f64, u64)> = self
                    .world
                    .quarters_in(p - m, p + m)
                    .iter()
                    .flat_map(|q| {
                        q.buildings_with_ids()
                            .map(|(id, b)| (centre(b).distance(eye), id.0, centre(b).distance(p)))
                            .filter(|(_, _, d)| *d < r)
                            .map(|(e, id, _)| (e, id))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                open.extend(near.into_iter().take(12).map(|(_, i)| (i, self.cut_level)));
            }
            let mut lamps: Vec<(f64, engine::lamps::Lamp)> = Vec::new();
            for (i, level) in open {
                if self.hidden.contains(&i) {
                    continue;
                }
                let (Some(wb), Some(plan)) = (self.building(i), self.plan(i)) else {
                    continue;
                };
                let level = (level as usize).min(plan.floors.len() - 1);
                let floor =
                    wb.base + plan.floors[level].elevation + crate::citymesh::ROOM_FLOOR_TOP;
                for l in crate::interior::room_lights(&plan, level) {
                    let on = crate::daylight::lights_on(l.habit, hour);
                    if on < 0.05 {
                        continue;
                    }
                    let c = plan
                        .site
                        .to_city(DVec3::new(l.at.x, floor + l.at.y, l.at.z));
                    let d = DVec2::new(c.x, c.z).distance(eye);
                    lamps.push((
                        d,
                        engine::lamps::Lamp::point(c.as_vec3(), l.color, l.intensity * on, l.range),
                    ));
                }
            }
            lamps.sort_by(|a, b| a.0.total_cmp(&b.0));
            lamps.truncate(MOST);
            self.rooms = (Some(key), lamps.into_iter().map(|(_, l)| l).collect());
        }
        let mut h = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&key, &mut h);
        (std::hash::Hasher::finish(&h), &self.rooms.1)
    }

    /// The places to stand on the floors drawn open (the cut view, the
    /// building our hero is in), in the cells drawn now.
    pub fn indoor_spots(&self) -> impl Iterator<Item = &IndoorSpots> {
        self.spots.values().flatten()
    }

    /// The tallest building within `r` of `p` (most floors).
    pub fn tallest_near(&self, p: DVec2, r: f64) -> Option<u64> {
        let m = DVec2::splat(r + BUILDING_MARGIN);
        self.world
            .quarters_in(p - m, p + m)
            .iter()
            .flat_map(|q| {
                q.buildings_with_ids()
                    .filter(|(_, b)| centre(b).distance(p) < r)
                    .map(|(id, b)| (b.site.floors, std::cmp::Reverse(id.0)))
                    .collect::<Vec<_>>()
            })
            .max()
            .map(|(_, std::cmp::Reverse(id))| id)
    }

    /// The nearest building to `p` (within `r`) with a room of `kind`
    /// above the ground floor if it can (for close-up views): the
    /// building, the floor and the room's index on it.
    pub fn find_room(
        &mut self,
        p: DVec2,
        r: f64,
        kind: hw_gen::building::RoomKind,
    ) -> Option<(u64, usize, usize)> {
        let m = DVec2::splat(r + BUILDING_MARGIN);
        let mut near: Vec<(f64, u64)> = self
            .world
            .quarters_in(p - m, p + m)
            .iter()
            .flat_map(|q| {
                q.buildings_with_ids()
                    .map(|(id, b)| (centre(b).distance(p), id.0))
                    .filter(|(d, _)| *d < r)
                    .collect::<Vec<_>>()
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, i) in near {
            let Some(plan) = self.plan(i) else { continue };
            let mut found = None;
            for (level, f) in plan.floors.iter().enumerate() {
                if let Some(k) = f.rooms.iter().position(|room| room.kind == kind) {
                    found = Some((i, level, k));
                    if level > 0 {
                        break;
                    }
                }
            }
            if found.is_some() {
                return found;
            }
        }
        None
    }

    fn building(&self, i: u64) -> Option<WorldBuilding> {
        if !town_building(i) {
            return None;
        }
        self.world.building(hw_gen::urban::quarter::DetailId(i))
    }

    pub fn centre_of(&self, i: u64) -> Option<DVec2> {
        self.building(i).map(|b| centre(&b))
    }

    /// The plan of building `i`, made on first use.
    pub fn plan(&mut self, i: u64) -> Option<Arc<BuildingPlan>> {
        if let Some(p) = self.plans.get(&i) {
            return Some(p.clone());
        }
        let b = self.building(i)?;
        Some(self.plan_of(i, &b))
    }

    fn plan_of(&mut self, i: u64, b: &WorldBuilding) -> Arc<BuildingPlan> {
        self.plans
            .entry(i)
            .or_insert_with(|| Arc::new(BuildingPlan::generate(&b.site)))
            .clone()
    }

    fn look_of(&self, i: u64, b: &WorldBuilding) -> Look {
        if let Some(inside) = self.inside {
            if inside.building == i {
                return Look::Cut {
                    level: inside.level,
                };
            }
        }
        if self.cut_on && centre(b).distance(self.cut_focus) < CUT_RADIUS {
            Look::Cut {
                level: self.cut_level,
            }
        } else {
            Look::Shell
        }
    }

    /// The cells with buildings near `p` (to redraw when the cut moves).
    pub fn cells_near(&self, p: DVec2, r: f64) -> BTreeSet<CellKey> {
        let (a, b) = (cell_of(p - DVec2::splat(r)), cell_of(p + DVec2::splat(r)));
        let mut out = BTreeSet::new();
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                out.insert((x, z));
            }
        }
        out
    }

    /// The cell that draws building `i` (none for a landmark).
    pub fn cell_of_building(&self, i: u64) -> Option<CellKey> {
        self.centre_of(i).map(cell_of)
    }

    /// Leaves building `i` out of its cell (`true`) or draws it again, and
    /// redraws the cell.
    pub fn set_hidden(&mut self, world: &mut World, i: u64, hidden: bool) {
        let changed = if hidden {
            self.hidden.insert(i)
        } else {
            self.hidden.remove(&i)
        };
        // Now, not on the workers: Engine's breakable copy appears this
        // frame, and both must not stand there together.
        if changed {
            if let Some(k) = self.cell_of_building(i) {
                if let Some(&d) = self.built.get(&k) {
                    self.build_cell(world, k, d);
                }
            }
        }
    }

    /// Every cell built now (to redraw them all).
    pub fn built_cells(&self) -> Vec<CellKey> {
        self.built.keys().copied().collect()
    }

    /// Redraws the given cells if they are built: on the workers when
    /// they run (the old drawing stays until the new one is in), else now.
    pub fn rebuild(&mut self, world: &mut World, cells: impl IntoIterator<Item = CellKey>) {
        for k in cells {
            let Some(&d) = self.built.get(&k) else {
                continue;
            };
            if self.workers.is_some() {
                let job = self.cell_job(k, d);
                if let Some(w) = self.workers.as_mut() {
                    w.send(job, true);
                }
            } else {
                self.build_cell(world, k, d);
            }
        }
    }

    /// Draws the cells around `p` in full detail now, on this thread (for
    /// checks and tools that need a piece of the town, not a stream).
    pub fn build_around(&mut self, world: &mut World, p: DVec2, r: f64) {
        for k in self.cells_within(p, r + CELL * 0.75) {
            self.build_cell(world, k, Detail::Near);
        }
    }

    /// The cells within `r` of `eye` on the town's ground.
    fn cells_within(&self, eye: DVec2, r: f64) -> Vec<CellKey> {
        let h = self.world.half();
        let (lo, hi) = (
            (eye - DVec2::splat(r)).max(DVec2::splat(-h)),
            (eye + DVec2::splat(r)).min(DVec2::splat(h)),
        );
        if lo.x > hi.x || lo.y > hi.y {
            return Vec::new();
        }
        let (a, b) = (cell_of(lo), cell_of(hi));
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for z in a.1..=b.1 {
                if cell_centre((x, z)).distance(eye) < r {
                    out.push((x, z));
                }
            }
        }
        out
    }

    /// Builds cells coming into range and drops those out of range, nearest
    /// first, for at most `budget_ms` per call (at least one cell). A cell
    /// whose detail the town has not planned yet waits a frame or two
    /// while the town plans it in the background (unless the budget is
    /// unlimited: then it is planned on the spot).
    pub fn update(&mut self, world: &mut World, eye: DVec2, budget_ms: f64) {
        self.drop_far(world, &[eye]);
        // A cell the background path left half placed goes in now.
        while self.placing.is_some() {
            self.step_placing(world, usize::MAX, &[eye]);
        }
        let wait = budget_ms.is_finite();
        let start = std::time::Instant::now();
        let mut later = Vec::new();
        for (_, k, detail) in self.todo(&[eye]) {
            if wait && !self.built.contains_key(&k) && !self.ready(k) {
                later.push(cell_centre(k));
                continue;
            }
            self.build_cell(world, k, detail);
            if start.elapsed().as_secs_f64() * 1000.0 > budget_ms {
                break;
            }
        }
        if !later.is_empty() {
            self.world.prefetch(&later, CELL * 0.5 + BUILDING_MARGIN);
        }
    }

    /// Like `update`, but cells are drawn on worker threads: each call only
    /// puts finished cells into the world (for at most `budget_ms`) and
    /// hands out more work, so a frame never waits for a cell.
    pub fn update_background(&mut self, world: &mut World, eye: DVec2, budget_ms: f64) {
        self.update_background_around(world, &[eye], budget_ms);
    }

    /// Like [`Self::update_background`] with several places kept loaded (the
    /// camera and where it is flying to): a cell stays while it is near
    /// any, and its detail follows the nearest.
    pub fn update_background_around(&mut self, world: &mut World, eyes: &[DVec2], budget_ms: f64) {
        self.drop_far(world, eyes);
        let mut workers = self.workers.take().unwrap_or_else(Workers::start);
        let start = std::time::Instant::now();
        // A few meshes per frame: one cell at a time, the next only when the
        // last one is in.
        let mut spawned = self.step_placing(world, MESHES_PER_FRAME, eyes);
        while self.placing.is_none()
            && spawned < MESHES_PER_FRAME
            && start.elapsed().as_secs_f64() * 1000.0 <= budget_ms
        {
            let Ok(m) = workers.done.try_recv() else {
                break;
            };
            workers.busy.remove(&m.key);
            // A cell that went out of range while it was drawn is dropped,
            // and so is a drawing that a newer job replaces.
            let newest = self.stamps.get(&m.key) == Some(&m.stamp);
            if newest && nearest(eyes, cell_centre(m.key)) <= UNLOAD {
                self.start_placing(m);
                spawned += self.step_placing(world, MESHES_PER_FRAME - spawned, eyes);
            } else if !newest {
                crate::hitch::count("town cells drawn too late", 1);
            }
        }
        let mut room = (workers.count * 2).saturating_sub(workers.busy.len());
        let mut later = Vec::new();
        for (_, k, detail) in self.todo(eyes) {
            if room == 0 {
                break;
            }
            if workers.busy.contains(&k) {
                continue;
            }
            if !self.built.contains_key(&k) && !self.ready(k) {
                later.push(cell_centre(k));
                continue;
            }
            workers.send(self.cell_job(k, detail), false);
            room -= 1;
        }
        if !later.is_empty() {
            self.world.prefetch(&later, CELL * 0.5 + BUILDING_MARGIN);
        }
        self.workers = Some(workers);
    }

    /// Stops the worker threads (cells still being drawn are forgotten).
    pub fn stop_background(&mut self) {
        self.workers = None;
    }

    /// Cells being drawn on the workers now (for the hitch log).
    pub fn drawing(&self) -> usize {
        self.workers.as_ref().map_or(0, |w| w.busy.len())
    }

    fn drop_far(&mut self, world: &mut World, eyes: &[DVec2]) {
        let gone: Vec<CellKey> = self
            .built
            .keys()
            .copied()
            .filter(|k| nearest(eyes, cell_centre(*k)) > UNLOAD)
            .collect();
        for k in gone {
            self.drop_cell(world, k);
        }
    }

    /// Cells to build or redraw at another detail, nearest first.
    fn todo(&self, eyes: &[DVec2]) -> Vec<(f64, CellKey, Detail)> {
        let want = |k: &CellKey| {
            let d = nearest(eyes, cell_centre(*k));
            let detail = if d < NEAR_DETAIL + CELL * 0.71 {
                Detail::Near
            } else if d < DISTANT_DETAIL {
                Detail::Far
            } else {
                Detail::Distant
            };
            (d, detail)
        };
        let cells: BTreeSet<CellKey> = eyes
            .iter()
            .flat_map(|&eye| self.cells_within(eye, LOAD))
            .collect();
        let mut todo: Vec<(f64, CellKey, Detail)> = cells
            .into_iter()
            .filter_map(|k| {
                let (d, detail) = want(&k);
                (self.built.get(&k) != Some(&detail)).then_some((d, k, detail))
            })
            .collect();
        todo.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        todo
    }

    /// Has the town planned what cell `k` shows?
    fn ready(&self, k: CellKey) -> bool {
        let (lo, hi) = cell_box(k);
        let m = DVec2::splat(BUILDING_MARGIN);
        self.world.quarters_ready(lo - m, hi + m)
    }

    /// Builds every cell in range at once (the first frame).
    pub fn build_all(&mut self, world: &mut World, eye: DVec2) {
        self.update(world, eye, f64::INFINITY);
    }

    /// Is everything within `r` of `p` drawn, at the detail it wants with
    /// the camera kept at `eyes`, and nothing there half placed?
    pub fn ready_near(&self, eyes: &[DVec2], p: DVec2, r: f64) -> bool {
        let placing_here = self
            .placing
            .as_ref()
            .is_some_and(|q| cell_centre(q.key).distance(p) < r);
        !placing_here
            && self
                .todo(eyes)
                .iter()
                .all(|(_, k, _)| cell_centre(*k).distance(p) >= r)
    }

    /// How much of the town around `eye` is built: (cells built, cells
    /// wanted), for a loading bar.
    pub fn progress(&self, eye: DVec2) -> (usize, usize) {
        let wanted = self.cells_within(eye, LOAD);
        let built = wanted.iter().filter(|k| self.built.contains_key(k)).count();
        (built, wanted.len())
    }

    /// Takes every built cell out of the world (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        self.workers = None;
        let keys: Vec<CellKey> = self.built.keys().copied().collect();
        for k in keys {
            self.drop_cell(world, k);
        }
    }

    fn drop_cell(&mut self, world: &mut World, k: CellKey) {
        self.abandon_placing(world, k);
        for e in self.entities.remove(&k).unwrap_or_default() {
            world.despawn(e);
        }
        for (i, _) in self.buildings.remove(&k).unwrap_or_default() {
            self.plans.remove(&i);
        }
        self.built.remove(&k);
        self.stamps.remove(&k);
        self.spots.remove(&k);
        if self.doors.remove(&k).is_some() {
            self.door_generation += 1;
        }
    }

    fn set_doors(&mut self, k: CellKey, d: CellDoors) {
        self.door_generation += 1;
        if d.doors.is_empty() && d.panes.is_empty() {
            self.doors.remove(&k);
        } else {
            self.doors.insert(k, d);
        }
    }

    /// The doors the built cells show (see `doorview`).
    pub fn door_spots(&self) -> impl Iterator<Item = &crate::doorview::DoorSpot> {
        self.doors.values().flat_map(|d| d.doors.iter())
    }

    /// The fixed glass of entrances the built cells show.
    pub fn pane_spots(&self) -> impl Iterator<Item = &crate::doorview::PaneSpot> {
        self.doors.values().flat_map(|d| d.panes.iter())
    }

    /// Changes whenever the doors shown change.
    pub fn door_generation(&self) -> u64 {
        self.door_generation
    }

    /// The buildings whose centres stand in cell `k`.
    fn buildings_of(&self, k: CellKey) -> Vec<(u64, WorldBuilding)> {
        let (lo, hi) = cell_box(k);
        let m = DVec2::splat(BUILDING_MARGIN);
        let mut out: Vec<(u64, WorldBuilding)> = self
            .world
            .quarters_in(lo - m, hi + m)
            .iter()
            .flat_map(|q| {
                q.buildings_with_ids()
                    .filter(|(_, b)| cell_of(centre(b)) == k)
                    .map(|(id, b)| (id.0, b.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    fn build_cell(&mut self, world: &mut World, k: CellKey, detail: Detail) {
        let job = self.cell_job(k, detail);
        crate::hitch::count("town cells drawn on the main thread", 1);
        self.place_cell(world, make_cell(job));
    }

    /// Everything a worker needs to draw cell `k`.
    fn cell_job(&mut self, k: CellKey, detail: Detail) -> CellJob {
        let (lo, hi) = cell_box(k);
        self.next_stamp += 1;
        self.stamps.insert(k, self.next_stamp);
        if !self.buildings.contains_key(&k) {
            let list = self.buildings_of(k);
            self.buildings.insert(k, list);
        }
        let buildings = self
            .buildings
            .get(&k)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, building)| CellBuilding {
                id,
                plan: self.plans.get(&id).cloned(),
                look: self.look_of(id, &building),
                hidden: self.hidden.contains(&id),
                building,
            })
            .collect();
        CellJob {
            key: k,
            stamp: self.next_stamp,
            detail,
            world: self.world.clone(),
            green: self.green.clone(),
            roads: self.world.roads_in(lo, hi),
            buildings,
        }
    }

    /// Takes a finished cell to put in over the next frames.
    fn start_placing(&mut self, m: CellMeshes) {
        debug_assert!(self.placing.is_none(), "one cell is placed at a time");
        let mut meshes: Vec<(PreparedMesh, bool)> = Vec::new();
        meshes.extend(m.solid.into_iter().map(|mesh| (mesh, false)));
        meshes.extend(m.lawns.into_iter().map(|mesh| (mesh, true)));
        meshes.extend(m.water.into_iter().map(|mesh| (mesh, false)));
        // Spawned from the end: keep the order the cell listed them in.
        meshes.reverse();
        self.placing = Some(Placing {
            key: m.key,
            detail: m.detail,
            ids: Vec::with_capacity(meshes.len()),
            meshes,
            plans: m.plans,
            spots: m.spots,
            doors: m.doors,
        });
    }

    /// Spawns up to `max` meshes of the cell being placed (parked out of
    /// sight); once all are spawned and Engine has had a frame to upload
    /// the last ones, moves them into place and retires the old drawing.
    /// Returns the meshes spawned.
    fn step_placing(&mut self, world: &mut World, max: usize, eyes: &[DVec2]) -> usize {
        let Some(p) = self.placing.as_mut() else {
            return 0;
        };
        if nearest(eyes, cell_centre(p.key)) > UNLOAD {
            let k = p.key;
            self.abandon_placing(world, k);
            return 0;
        }
        if p.meshes.is_empty() {
            // Every mesh had a frame to upload: show the cell.
            let p = self.placing.take().expect("checked above");
            self.finish_placing(world, p);
            return 0;
        }
        let mut n = 0;
        while n < max {
            let Some((mesh, flat)) = p.meshes.pop() else {
                break;
            };
            let id = world.spawn_prepared(mesh);
            let _ = world.set_place(id, Place::new(0.0, PARKED_Y, 0.0));
            if flat {
                // Flat ground casts no shadow worth its cost.
                let _ = world.set_casts_shadow(id, false);
            }
            world.set_name(id, format!("town cell {} {}", p.key.0, p.key.1));
            p.ids.push(id);
            n += 1;
        }
        n
    }

    /// The cell's meshes are all uploaded: into place, old drawing out.
    fn finish_placing(&mut self, world: &mut World, p: Placing) {
        let k = p.key;
        for e in self.entities.remove(&k).unwrap_or_default() {
            world.despawn(e);
        }
        for &id in &p.ids {
            let _ = world.set_place(id, Place::new(0.0, 0.0, 0.0));
        }
        if self.buildings.contains_key(&k) {
            self.plans.extend(p.plans);
        }
        if p.spots.is_empty() {
            self.spots.remove(&k);
        } else {
            self.spots.insert(k, p.spots);
        }
        self.set_doors(k, p.doors);
        crate::hitch::count("town cells placed", 1);
        crate::hitch::count("town meshes spawned", p.ids.len() as u64);
        self.entities.insert(k, p.ids);
        self.built.insert(k, p.detail);
    }

    /// Forgets a half-placed drawing of cell `k` (it went out of range, or
    /// a newer drawing is put in on the spot).
    fn abandon_placing(&mut self, world: &mut World, k: CellKey) {
        if self.placing.as_ref().is_some_and(|p| p.key == k) {
            let p = self.placing.take().expect("checked above");
            for id in p.ids {
                world.despawn(id);
            }
        }
    }

    fn place_cell(&mut self, world: &mut World, m: CellMeshes) {
        let k = m.key;
        self.abandon_placing(world, k);
        for e in self.entities.remove(&k).unwrap_or_default() {
            world.despawn(e);
        }
        // Keep plans only for buildings of cells still listed.
        if self.buildings.contains_key(&k) {
            self.plans.extend(m.plans);
        }
        if m.spots.is_empty() {
            self.spots.remove(&k);
        } else {
            self.spots.insert(k, m.spots);
        }
        self.set_doors(k, m.doors);
        let mut ids: Vec<EntityId> = m
            .solid
            .into_iter()
            .map(|mesh| world.spawn_prepared(mesh))
            .collect();
        for mesh in m.lawns {
            let id = world.spawn_prepared(mesh);
            // Flat ground casts no shadow worth its cost.
            let _ = world.set_casts_shadow(id, false);
            ids.push(id);
        }
        if let Some(mesh) = m.water {
            ids.push(world.spawn_prepared(mesh));
        }
        for &id in &ids {
            world.set_name(id, format!("town cell {} {}", k.0, k.1));
        }
        crate::hitch::count("town cells placed", 1);
        crate::hitch::count("town meshes spawned", ids.len() as u64);
        self.entities.insert(k, ids);
        self.built.insert(k, m.detail);
    }

    /// Where a ray from `origin` along `dir` first meets the ground.
    pub fn ground_hit(&self, origin: Vec3, dir: Vec3) -> Option<DVec2> {
        let (o, d) = (origin.as_dvec3(), dir.as_dvec3());
        let mut t = 0.0;
        while t < 3000.0 {
            let p = o + d * t;
            let q = DVec2::new(p.x, p.z);
            if p.y <= self.world.ground(q).max(WATER_LEVEL) {
                return Some(q);
            }
            t += 1.0 + t * 0.01;
        }
        None
    }
}

/// Flat water over the rivers and lakes of one cell: a square per land-grid
/// cell that is under water or next to it, cut off where the banks rise.
fn water_mesh(w: &TownWorld, k: CellKey) -> Option<Mesh> {
    let g = &w.town.land.height;
    let (lo, hi) = (
        DVec2::new(k.0 as f64, k.1 as f64) * CELL,
        DVec2::new(k.0 as f64 + 1.0, k.1 as f64 + 1.0) * CELL,
    );
    let wet = |i: i64, j: i64| {
        i >= 0
            && j >= 0
            && (i as usize) < g.w
            && (j as usize) < g.h
            && w.ground(g.center(i as usize, j as usize)) < WATER_LEVEL
    };
    let mut b = MeshBuilder::new();
    // Smooth and reflective, like still water.
    b.surf = crate::citymesh::Surf::Glass;
    let i0 = ((lo.x - g.origin.x) / g.cell).floor() as i64;
    let i1 = ((hi.x - g.origin.x) / g.cell).ceil() as i64;
    let j0 = ((lo.y - g.origin.y) / g.cell).floor() as i64;
    let j1 = ((hi.y - g.origin.y) / g.cell).ceil() as i64;
    let y = WATER_LEVEL - 0.05;
    for j in j0..j1 {
        for i in i0..i1 {
            let near = (-1..=1).any(|dj| (-1..=1).any(|di| wet(i + di, j + dj)));
            if !near {
                continue;
            }
            // Each grid square belongs to the cell holding its centre.
            let c = g.origin + DVec2::new(i as f64 + 0.5, j as f64 + 0.5) * g.cell;
            if cell_of(c) != k {
                continue;
            }
            let (a, z) = (
                c - DVec2::splat(g.cell * 0.5),
                c + DVec2::splat(g.cell * 0.5),
            );
            b.quad(
                [
                    DVec3::new(a.x, y, a.y),
                    DVec3::new(z.x, y, a.y),
                    DVec3::new(z.x, y, z.y),
                    DVec3::new(a.x, y, z.y),
                ],
                DVec3::Y,
                rgb(46, 92, 112),
            );
        }
    }
    (!b.is_empty()).then(|| b.finish())
}

/// What [`TownView::room_lamps`] were picked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RoomsKey {
    cut: Option<(i64, i64, u32)>,
    inside: Option<(u64, u32)>,
    hour: i32,
    eye: (i64, i64),
}

fn centre(b: &hw_gen::urban::world::WorldBuilding) -> DVec2 {
    let s = &b.site;
    s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5
}

/// The stretches of road `r` drawn by cell `k`: the road's strip points
/// ([`WorldRoad::strip_points`], steps of at most `ROAD_STEP`), each step
/// drawn by the cell holding its middle, so neighbouring cells' stretches
/// meet end to end.
fn runs_in(r: &WorldRoad, k: CellKey) -> Vec<Vec<(DVec2, DVec2)>> {
    let mut out = Vec::new();
    let mut run: Vec<(DVec2, DVec2)> = Vec::new();
    for w in r.strip_points().windows(2) {
        if cell_of(w[0].0.lerp(w[1].0, 0.5)) == k {
            if run.is_empty() {
                run.push(w[0]);
            }
            run.push(w[1]);
        } else if run.len() >= 2 {
            out.push(std::mem::take(&mut run));
        } else {
            run.clear();
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

/// One road run: pavements, then the carriageway a little higher so it wins
/// where roads cross; both at the heights people stand on
/// ([`TownWorld::surface`] reads the same lifts and strips).
fn draw_road(b: &mut MeshBuilder, w: &TownWorld, r: &WorldRoad, run: &[(DVec2, DVec2)]) {
    if run.len() < 2 {
        return;
    }
    let asphalt = if r.main {
        rgb(58, 58, 62)
    } else {
        rgb(78, 76, 74)
    };
    let pavement = rgb(165, 160, 150);
    let total = r.half + r.pavement;
    b.tex = Some(GroundLayer::Paving.into());
    b.mottle = 0.08;
    strip(b, w, run, -total, total, r.pavement_lift(), pavement);
    // Old lanes are cobbled.
    let lane = r.style == Some(hw_gen::urban::streets::StreetStyle::Lanes);
    b.tex = Some(if lane {
        GroundLayer::Cobble.into()
    } else {
        GroundLayer::RoadAsphalt.into()
    });
    let asphalt = if lane { rgb(120, 116, 110) } else { asphalt };
    if lane {
        // Setts repeat every two metres; patches of worn, sandy and damp
        // stone over them hide the repeat.
        b.mottle = 0.22;
        fine_strip(b, w, run, -r.half, r.half, r.carriage_lift(), asphalt);
    } else {
        b.mottle = 0.06;
        strip(b, w, run, -r.half, r.half, r.carriage_lift(), asphalt);
    }
    b.mottle = 0.0;
    b.tex = None;
}

/// A plate under meeting road ends ([`hw_gen::urban::surface::RoadJoin`]):
/// a disc of pavement and a disc of carriageway, in the look of the
/// narrowest road there (whose width the plate has).
fn draw_join(
    b: &mut MeshBuilder,
    w: &TownWorld,
    roads: &[WorldRoad],
    j: &hw_gen::urban::surface::RoadJoin,
) {
    let meets = |r: &&WorldRoad| {
        r.pts.len() >= 2
            && (r.pts[0].distance(j.at) < 0.5 || r.pts[r.pts.len() - 1].distance(j.at) < 0.5)
    };
    let Some(r) = roads
        .iter()
        .filter(meets)
        .min_by(|a, b| a.half.total_cmp(&b.half))
    else {
        return;
    };
    let lane = r.style == Some(hw_gen::urban::streets::StreetStyle::Lanes);
    let asphalt = if lane {
        rgb(120, 116, 110)
    } else if r.main {
        rgb(58, 58, 62)
    } else {
        rgb(78, 76, 74)
    };
    b.tex = Some(GroundLayer::Paving.into());
    b.mottle = 0.08;
    disc(b, w, j.at, j.pavement, j.pavement_lift, rgb(165, 160, 150));
    b.tex = Some(if lane {
        GroundLayer::Cobble.into()
    } else {
        GroundLayer::RoadAsphalt.into()
    });
    b.mottle = if lane { 0.22 } else { 0.06 };
    disc(b, w, j.at, j.carriage, j.carriage_lift, asphalt);
    b.mottle = 0.0;
    b.tex = None;
}

/// A flat disc draped on the road height plus `lift`, in rings so it
/// follows the ground.
fn disc(b: &mut MeshBuilder, w: &TownWorld, at: DVec2, r: f64, lift: f64, c: Color) {
    const SIDES: usize = 32;
    let rings = ((r / hw_gen::urban::surface::ROAD_STEP).ceil() as usize).max(1);
    let at3 = |p: DVec2| DVec3::new(p.x, w.road_height(p) + lift, p.y);
    let pt = |ring: usize, k: usize| {
        let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
        at + DVec2::new(hw_core::det::cos(a), hw_core::det::sin(a)) * r * ring as f64 / rings as f64
    };
    for k in 0..SIDES {
        b.tri([at3(at), at3(pt(1, k)), at3(pt(1, k + 1))], DVec3::Y, c);
        for ring in 1..rings {
            b.quad(
                [
                    at3(pt(ring, k)),
                    at3(pt(ring + 1, k)),
                    at3(pt(ring + 1, k + 1)),
                    at3(pt(ring, k + 1)),
                ],
                DVec3::Y,
                c,
            );
        }
    }
}

/// Like [`strip`], cut into cells about 1.5 m on a side so vertex colours
/// can carry the ground's mottling.
fn fine_strip(
    b: &mut MeshBuilder,
    w: &TownWorld,
    pts: &[(DVec2, DVec2)],
    a: f64,
    bo: f64,
    lift: f64,
    c: Color,
) {
    const CELL: f64 = 1.5;
    let across = (((bo - a) / CELL).ceil() as usize).max(1);
    let at = |p: DVec2| DVec3::new(p.x, w.road_height(p) + lift, p.y);
    for k in 0..pts.len().saturating_sub(1) {
        let ((p, n0), (q, n1)) = (pts[k], pts[k + 1]);
        let along = ((p.distance(q) / CELL).ceil() as usize).clamp(1, 400);
        let pt = |i: usize, j: usize| {
            let u = i as f64 / along as f64;
            let o = a + (bo - a) * j as f64 / across as f64;
            let base = p.lerp(q, u);
            let n = n0.lerp(n1, u);
            at(base + n * o)
        };
        for i in 0..along {
            for j in 0..across {
                b.quad(
                    [pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1)],
                    DVec3::Y,
                    c,
                );
            }
        }
    }
}

/// A ribbon from offset `a` to `b` (left negative) along strip points
/// `pts`, draped on the road height plus `lift`.
fn strip(
    b: &mut MeshBuilder,
    w: &TownWorld,
    pts: &[(DVec2, DVec2)],
    a: f64,
    bo: f64,
    lift: f64,
    c: Color,
) {
    let n = pts.len();
    let at = |p: DVec2| DVec3::new(p.x, w.road_height(p) + lift, p.y);
    for k in 0..n - 1 {
        let ((p, n0), (q, n1)) = (pts[k], pts[k + 1]);
        let quad = [
            at(p + n0 * a),
            at(q + n1 * a),
            at(q + n1 * bo),
            at(p + n0 * bo),
        ];
        b.quad(quad, DVec3::Y, c);
    }
}

/// Distance from `p` to the nearest of `eyes` (infinite with none).
fn nearest(eyes: &[DVec2], p: DVec2) -> f64 {
    eyes.iter()
        .map(|e| e.distance(p))
        .fold(f64::INFINITY, f64::min)
}
