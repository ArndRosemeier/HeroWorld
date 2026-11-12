//! The town from afar: every building past the cells [`TownView`] draws,
//! out to the edge of the view, as its plain mass (the plan's storeys,
//! setbacks, pitched roofs and tower crowns) with windows painted on by a
//! small shader, lit after dark like the real ones. So the skyline stands
//! there when you fly, and the buildings that stream in close by grow out
//! of shapes that were already in place instead of popping out of nothing.
//!
//! The town is cut into tiles of 4 x 4 cells. A worker plans a tile's
//! buildings ([`TownWorld::skyline`], which keeps nothing in the town's
//! caches) and builds two meshes: all buildings, with ground floors and
//! roofs, and a lighter one of the taller ones for kilometres away. Engine
//! draws the pair as a LOD set, so the two cross-fade and the tile fades
//! out at the edge of the view. A tile whose cells are all built hides.
//! Each mass sits a little inside the real building, so where a tile and
//! built cells overlap the real building hides it.
//!
//! [`TownView`]: crate::townmesh::TownView

use crate::townmesh::CELL;
use engine::prelude::*;
use glam::{DVec2, Vec3};
use hw_gen::building::Tier;
use hw_gen::style::{Crown, Ridge, Roof};
use hw_gen::urban::world::{TownWorld, WorldBuilding, WorldRoad};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

/// Cells per tile side.
pub const TILE_CELLS: i64 = 4;
/// Edge of a skyline tile (m).
pub const TILE: f64 = CELL * TILE_CELLS as f64;
/// The tiles' detailed level draws to this camera distance (m from the
/// tile's centre); the light one from there to [`FAR_END`].
const MID_END: f32 = 2600.0;
/// Where the skyline fades out: the edge of the view.
const FAR_END: f32 = crate::settings::VIEW_DISTANCE;
/// Tiles are made out to this distance of their centres from the camera,
/// and dropped past [`DROP`].
const REACH: f64 = FAR_END as f64 + 300.0;
const DROP: f64 = REACH + 900.0;
/// How far a mass stands inside its building's walls (m): enough that the
/// real building, once its cell is drawn, hides it whole.
const INSET: f64 = 0.35;
/// The light level keeps buildings at least this tall (m): lower ones are
/// a pixel or two that far out.
const FAR_MIN_HEIGHT: f64 = 12.0;
/// Tiles handed to Engine per frame (each is two meshes to upload).
const SPAWNS_PER_FRAME: usize = 1;

type TileKey = (i64, i64);

fn tile_of(p: DVec2) -> TileKey {
    ((p.x / TILE).floor() as i64, (p.y / TILE).floor() as i64)
}

fn tile_box(k: TileKey) -> (DVec2, DVec2) {
    let lo = DVec2::new(k.0 as f64, k.1 as f64) * TILE;
    (lo, lo + DVec2::splat(TILE))
}

fn tile_centre(k: TileKey) -> DVec2 {
    let (lo, hi) = tile_box(k);
    (lo + hi) * 0.5
}

fn nearest(eyes: &[DVec2], p: DVec2) -> f64 {
    eyes.iter()
        .map(|e| e.distance(p))
        .fold(f64::INFINITY, f64::min)
}

/// The cells (of [`crate::townmesh`]) that tile `k` covers.
pub fn tile_cells(k: TileKey) -> impl Iterator<Item = (i64, i64)> {
    let (x0, y0) = (k.0 * TILE_CELLS, k.1 * TILE_CELLS);
    (0..TILE_CELLS).flat_map(move |j| (0..TILE_CELLS).map(move |i| (x0 + i, y0 + j)))
}

/// The windows' shader: a grid of panes on the walls (one per bay and
/// storey, from the mesh's uv), glass by day and, after dark, lit by
/// households with the same habits and lamp colours as the real windows
/// (`daylight`). Seen from so far that a pixel covers several windows, it
/// fades to their average instead of shimmering.
///
/// uv: x counts bays along a wall (its whole part differs per wall and
/// building), y counts storeys from the ground floor up; below 0 means no
/// windows (plinths, roofs, crowns).
const WINDOWS: &str = r#"
fn sky_hash(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q = q + dot(q, q.yx + 33.33);
    return fract((q.x + q.y) * q.x);
}

fn sky_lights(habit: i32, m: MaterialIn) -> f32 {
    if (habit == 4) {
        return m.params[1].x;
    }
    return m.params[0][habit];
}

fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    // How many windows one pixel spans (before any branch: derivatives).
    let span = max(fwidth(m.uv.x), fwidth(m.uv.y));
    if (m.uv.y < 0.0 || abs(m.normal.y) > 0.5) {
        return o;
    }
    let cell = floor(m.uv);
    let f = fract(m.uv);
    let pane = step(0.2, f.x) * step(f.x, 0.8) * step(0.3, f.y) * step(f.y, 0.85);
    // Share of a wall that is glass, for the average far out.
    let share = 0.6 * 0.55;
    let blur = smoothstep(0.6, 1.6, span);
    let glassy = mix(pane, share, blur);
    let glass = vec3<f32>(0.035, 0.045, 0.06);
    o.color = vec4<f32>(mix(base.color.rgb, glass, glassy), base.color.a);
    o.roughness = mix(base.roughness, 0.12, glassy);
    o.metallic = mix(base.metallic, 0.45, glassy);
    // Which household: whether its room has a lamp, when it switches it
    // on, and its colour.
    let h = sky_hash(cell + vec2<f32>(17.0, 3.0));
    let lamp = step(h, 0.45);
    let habit = i32(clamp(floor(fract(h * 7.13) * 5.0), 0.0, 4.0));
    let t = fract(h * 13.7);
    var tone = vec3<f32>(1.0, 0.67, 0.30);
    if (t < 0.25) {
        tone = vec3<f32>(0.51, 0.67, 1.0);
    } else if (t < 0.5) {
        tone = vec3<f32>(1.0, 0.51, 0.19);
    }
    let one = tone * lamp * sky_lights(habit, m);
    // The average window: 45 % have lamps, warm on the whole; held well
    // under the true mean, as a wall of faint glow reads as a pale wall
    // where the real town shows dark walls and points of light.
    let lit = (m.params[0].x + m.params[0].y + m.params[0].z + m.params[0].w + m.params[1].x) * 0.2;
    let all = vec3<f32>(0.9, 0.66, 0.38) * 0.45 * lit * 0.3;
    o.emission = mix(one * pane, all * share, blur) * 1.2;
    return o;
}
"#;

/// What a tile is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Detail {
    /// Only the light level (tall buildings and main roads), kilometres
    /// away: what most tiles are, and a fraction of the memory.
    Far,
    /// Both levels: every building and street, cross-fading to the light
    /// one at [`MID_END`].
    Mid,
}

/// Tiles nearer than this (centre to camera, m) are made with both levels;
/// past [`FAR_ONLY`] they go back to the light one alone.
const MID_MAKE: f64 = MID_END as f64 + 500.0;
const FAR_ONLY: f64 = MID_MAKE + 700.0;

/// A tile's meshes, made on a worker, nearer level first. Empty: nothing
/// to draw there.
struct TileMeshes {
    key: TileKey,
    detail: Detail,
    levels: Vec<(PreparedMesh, f32)>,
    buildings: usize,
}

/// One tile in the world.
struct Tile {
    lod: Option<LodId>,
    detail: Detail,
    shown: bool,
}

type Job = (TileKey, Detail);

/// Jobs for the workers, nearest first, and whether they should stop.
#[derive(Default)]
struct Queue {
    jobs: std::collections::VecDeque<Job>,
    /// Queued or being made: a job leaves when its meshes are taken in.
    busy: BTreeSet<Job>,
    closed: bool,
}

struct Workers {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    done: Receiver<TileMeshes>,
    count: usize,
}

impl Workers {
    fn start(world: Arc<TownWorld>) -> Self {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let (tx, done) = channel();
        // One or two threads: the town's own cells come first (they have theirs).
        let count = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .saturating_sub(3)
            .clamp(1, 2);
        for _ in 0..count {
            let (queue, tx, world): (_, Sender<TileMeshes>, _) =
                (queue.clone(), tx.clone(), world.clone());
            std::thread::Builder::new()
                .name("hw-skyline".into())
                .spawn(move || {
                    engine::parallel::background_thread();
                    loop {
                        let job = {
                            let (lock, ready) = &*queue;
                            let Ok(mut q) = lock.lock() else { return };
                            loop {
                                if q.closed {
                                    return;
                                }
                                if let Some(j) = q.jobs.pop_front() {
                                    break j;
                                }
                                q = match ready.wait(q) {
                                    Ok(q) => q,
                                    Err(_) => return,
                                };
                            }
                        };
                        if tx.send(make_tile(&world, job.0, job.1)).is_err() {
                            return;
                        }
                    }
                })
                .expect("skyline worker");
        }
        Self { queue, done, count }
    }

    /// Queues the first jobs of `want` (nearest first) that are not being
    /// made already, a couple per worker, in place of what still waited
    /// (the camera may have moved on). One lock for all of it, so a worker
    /// taking a job meanwhile cannot have it handed out twice.
    fn set_jobs(&self, want: &[Job]) {
        let (lock, ready) = &*self.queue;
        let Ok(mut q) = lock.lock() else { return };
        let waiting: Vec<Job> = q.jobs.drain(..).collect();
        for j in waiting {
            q.busy.remove(&j);
        }
        let room = self.count * 2;
        for j in want {
            if q.jobs.len() >= room {
                break;
            }
            if q.busy.insert(*j) {
                q.jobs.push_back(*j);
            }
        }
        ready.notify_all();
    }

    /// A job's meshes were taken in.
    fn finished(&self, job: Job) {
        if let Ok(mut q) = self.queue.0.lock() {
            q.busy.remove(&job);
        }
    }

    /// Jobs queued or being made.
    fn busy(&self) -> usize {
        self.queue.0.lock().map_or(0, |q| q.busy.len())
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

/// The town's skyline past the built cells.
pub struct SkylineView {
    world: Arc<TownWorld>,
    tiles: BTreeMap<TileKey, Tile>,
    material: Option<CustomMaterialId>,
    workers: Option<Workers>,
    /// Finished tiles waiting to go into the world.
    ready: Vec<TileMeshes>,
    /// What the window lights were last set to.
    lights: [f32; 5],
    /// Buildings in the tiles placed so far (for checks and the log).
    pub placed_buildings: usize,
}

impl SkylineView {
    pub fn new(world: Arc<TownWorld>) -> Self {
        Self {
            world,
            tiles: BTreeMap::new(),
            material: None,
            workers: None,
            ready: Vec::new(),
            lights: [-1.0; 5],
            placed_buildings: 0,
        }
    }

    /// Tiles in the world now, how many of them show, and how many have
    /// both levels.
    pub fn counts(&self) -> (usize, usize, usize) {
        let shown = self.tiles.values().filter(|t| t.shown).count();
        let mid = self
            .tiles
            .values()
            .filter(|t| t.detail == Detail::Mid)
            .count();
        (self.tiles.len(), shown, mid)
    }

    /// Tiles queued or being made on the workers now.
    pub fn making(&self) -> usize {
        self.workers.as_ref().map_or(0, Workers::busy)
    }

    /// Lights the windows for `hour` (as `daylight` does the real ones).
    pub fn set_hour(&mut self, world: &mut World, hour: f32) -> EngineResult<()> {
        let mut l = [0.0f32; 5];
        for (habit, v) in l.iter_mut().enumerate() {
            *v = crate::daylight::lights_on(habit, hour);
        }
        if l == self.lights {
            return Ok(());
        }
        self.lights = l;
        if let Some(m) = self.material {
            world.set_material_param(m, 0, [l[0], l[1], l[2], l[3]])?;
            world.set_material_param(m, 1, [l[4], 0.0, 0.0, 0.0])?;
        }
        Ok(())
    }

    fn material(&mut self, world: &mut World) -> EngineResult<CustomMaterialId> {
        if let Some(m) = self.material {
            return Ok(m);
        }
        let l = self.lights.map(|v| v.max(0.0));
        let m = world.add_material(
            CustomMaterial::new("skyline windows", WINDOWS)
                .param(0, [l[0], l[1], l[2], l[3]])
                .param(1, [l[4], 0.0, 0.0, 0.0]),
        )?;
        self.material = Some(m);
        Ok(m)
    }

    /// The detail tile `k` wants with the camera at `eyes`, keeping what
    /// it has between [`MID_MAKE`] and [`FAR_ONLY`].
    fn wants(&self, k: TileKey, eyes: &[DVec2]) -> Detail {
        let d = nearest(eyes, tile_centre(k));
        let has = self.tiles.get(&k).map(|t| t.detail);
        if d < MID_MAKE || (has == Some(Detail::Mid) && d < FAR_ONLY) {
            Detail::Mid
        } else {
            Detail::Far
        }
    }

    /// Keeps the tiles round `eyes` made and shown: a tile shows unless
    /// `built` says all its cells are drawn in full.
    pub fn update(
        &mut self,
        world: &mut World,
        eyes: &[DVec2],
        built: impl Fn((i64, i64)) -> bool,
    ) -> EngineResult<()> {
        if eyes.is_empty() {
            return Ok(());
        }
        // Drop what is far behind.
        let gone: Vec<TileKey> = self
            .tiles
            .keys()
            .copied()
            .filter(|k| nearest(eyes, tile_centre(*k)) > DROP)
            .collect();
        for k in gone {
            if let Some(t) = self.tiles.remove(&k) {
                if let Some(id) = t.lod {
                    world.despawn_lod(id);
                }
            }
        }
        // Take in what the workers finished; place the nearest, a tile a
        // frame. A tile made for a detail no longer wanted is dropped.
        if let Some(w) = self.workers.as_ref() {
            while let Ok(m) = w.done.try_recv() {
                w.finished((m.key, m.detail));
                self.ready.push(m);
            }
        }
        let ready = std::mem::take(&mut self.ready);
        let mut ready: Vec<TileMeshes> = ready
            .into_iter()
            .filter(|m| {
                nearest(eyes, tile_centre(m.key)) <= DROP && self.wants(m.key, eyes) == m.detail
            })
            .collect();
        ready.sort_by(|a, b| {
            nearest(eyes, tile_centre(b.key)).total_cmp(&nearest(eyes, tile_centre(a.key)))
        });
        for _ in 0..SPAWNS_PER_FRAME {
            let Some(m) = ready.pop() else { break };
            self.place(world, m, &built)?;
        }
        self.ready = ready;
        // Show the tiles not yet covered by built cells (only those near
        // the camera can be).
        let covers = crate::townmesh::LOAD + TILE;
        for (k, t) in self.tiles.iter_mut() {
            let show = nearest(eyes, tile_centre(*k)) > covers || !tile_cells(*k).all(&built);
            if show != t.shown {
                t.shown = show;
                if let Some(id) = t.lod {
                    world.set_lod_places(id, &places(*k, show))?;
                }
            }
        }
        // Hand out the missing tiles and those to redraw, nearest first.
        let h = self.world.half();
        let mut want: Vec<(f64, Job)> = Vec::new();
        let mut seen = BTreeSet::new();
        for e in eyes {
            let (a, b) = (
                tile_of(*e - DVec2::splat(REACH)),
                tile_of(*e + DVec2::splat(REACH)),
            );
            for y in a.1..=b.1 {
                for x in a.0..=b.0 {
                    let k = (x, y);
                    if !seen.insert(k) {
                        continue;
                    }
                    let (lo, hi) = tile_box(k);
                    if lo.x > h || lo.y > h || hi.x < -h || hi.y < -h {
                        continue;
                    }
                    let d = nearest(eyes, tile_centre(k));
                    if d > REACH {
                        continue;
                    }
                    let detail = self.wants(k, eyes);
                    let has = self.tiles.get(&k).map(|t| t.detail);
                    let waiting = self.ready.iter().any(|m| m.key == k && m.detail == detail);
                    if has != Some(detail) && !waiting {
                        want.push((d, (k, detail)));
                    }
                }
            }
        }
        want.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let want: Vec<Job> = want.into_iter().map(|w| w.1).collect();
        self.workers
            .get_or_insert_with(|| Workers::start(self.world.clone()))
            .set_jobs(&want);
        Ok(())
    }

    /// Puts a finished tile into the world, in place of what the tile
    /// showed before (in the same frame: the light level is the same mesh
    /// in both, so a tile gaining its near level changes nothing on screen).
    fn place(
        &mut self,
        world: &mut World,
        m: TileMeshes,
        built: &impl Fn((i64, i64)) -> bool,
    ) -> EngineResult<()> {
        let shown = !tile_cells(m.key).all(built);
        let lod = if m.levels.is_empty() {
            None
        } else {
            let id = world.spawn_lod_prepared(m.levels)?;
            let material = self.material(world)?;
            for e in world.lod_levels(id)? {
                world.set_custom_material(e, material)?;
                world.set_casts_shadow(e, false)?;
            }
            if shown {
                world.set_lod_places(id, &places(m.key, true))?;
            }
            Some(id)
        };
        let old = self.tiles.insert(
            m.key,
            Tile {
                lod,
                detail: m.detail,
                shown,
            },
        );
        if let Some(id) = old.and_then(|t| t.lod) {
            world.despawn_lod(id);
        }
        crate::hitch::count("skyline tiles placed", 1);
        self.placed_buildings += m.buildings;
        Ok(())
    }

    /// Takes every tile out of the world (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        self.workers = None;
        self.ready.clear();
        for (_, t) in std::mem::take(&mut self.tiles) {
            if let Some(id) = t.lod {
                world.despawn_lod(id);
            }
        }
    }
}

/// Where tile `k`'s one instance stands: at its centre while it shows.
fn places(k: TileKey, show: bool) -> Vec<Place> {
    if show {
        let c = tile_centre(k);
        vec![Place::new(c.x as f32, 0.0, c.y as f32)]
    } else {
        Vec::new()
    }
}

/// Plans tile `k`'s buildings and roads and builds its meshes (on a
/// worker): the light level, and for [`Detail::Mid`] the full one before it.
fn make_tile(world: &TownWorld, k: TileKey, detail: Detail) -> TileMeshes {
    let (lo, hi) = tile_box(k);
    let (buildings, roads) = world.skyline_with_roads(lo, hi);
    let centre = tile_centre(k);
    let mut mid = Masses::new(centre);
    let mut far = Masses::new(centre);
    let full = detail == Detail::Mid;
    for r in &roads {
        if full {
            mid.road(world, r, lo, hi);
        }
        if r.main {
            far.road(world, r, lo, hi);
        }
    }
    for (_, b) in &buildings {
        if full {
            mid.building(b, false);
        }
        far.building(b, true);
    }
    let mut levels = Vec::new();
    if full && mid.mesh.face_count() > 0 {
        levels.push((mid.mesh.prepare(), MID_END));
    }
    if far.mesh.face_count() > 0 {
        levels.push((far.mesh.prepare(), FAR_END));
    } else if let Some(l) = levels.last_mut() {
        // Nothing tall here: the full level fades out where the light one
        // would have taken over.
        l.1 = MID_END;
    }
    TileMeshes {
        key: k,
        detail,
        levels,
        buildings: buildings.len(),
    }
}

/// A mesh of building masses, in metres from a tile's centre.
struct Masses {
    mesh: Mesh,
    centre: DVec2,
    /// Every point added (tests check where they are).
    #[cfg(test)]
    points: Vec<Vec3>,
}

/// uv for surfaces without windows.
const BLANK: [f32; 2] = [0.0, -1.0];

impl Masses {
    fn new(centre: DVec2) -> Self {
        Self {
            mesh: Mesh::new(),
            centre,
            #[cfg(test)]
            points: Vec::new(),
        }
    }

    fn p(&self, g: DVec2, y: f64) -> Vec3 {
        Vec3::new(
            (g.x - self.centre.x) as f32,
            y as f32,
            (g.y - self.centre.y) as f32,
        )
    }

    /// A face facing `out` (its points in order round it, either way).
    fn face(&mut self, pts: &[Vec3], uvs: &[[f32; 2]], out: Vec3, color: Color) {
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        let flip = n.dot(out) < 0.0;
        let mut ids = Vec::with_capacity(pts.len());
        for i in 0..pts.len() {
            let i = if flip { pts.len() - 1 - i } else { i };
            let Ok(id) = self.mesh.add_point(pts[i]) else {
                return;
            };
            #[cfg(test)]
            self.points.push(pts[i]);
            let _ = self.mesh.set_point_color(id, color);
            let _ = self.mesh.set_point_uv(id, uvs[i]);
            ids.push(id);
        }
        let _ = self.mesh.add_face(&ids);
    }

    /// The stretches of road `r` whose middles lie in the tile (so
    /// neighbouring tiles meet without overlapping): pavement and
    /// carriageway as flat strips on the ground, a little under where the
    /// town's cells draw them, so the real road covers them.
    fn road(&mut self, world: &TownWorld, r: &WorldRoad, lo: DVec2, hi: DVec2) {
        let mut pts = Vec::with_capacity(r.pts.len() * 2);
        for w in r.pts.windows(2) {
            let n = (w[0].distance(w[1]) / ROAD_STEP).ceil().max(1.0) as usize;
            for i in 0..n {
                pts.push(w[0].lerp(w[1], i as f64 / n as f64));
            }
        }
        if let Some(l) = r.pts.last() {
            pts.push(*l);
        }
        for w in pts.windows(2) {
            let (a, c) = (w[0], w[1]);
            let m = (a + c) * 0.5;
            if m.x < lo.x || m.y < lo.y || m.x >= hi.x || m.y >= hi.y {
                continue;
            }
            let len = a.distance(c);
            if len < 0.01 {
                continue;
            }
            // Over water the road is a bridge the cells draw; from afar the
            // gap is a few pixels.
            if world.ground(m) < hw_gen::urban::world::WATER_LEVEL + 0.2 {
                continue;
            }
            let dir = (c - a) / len;
            let n = dir.perp();
            for (half, lift, color) in [
                (r.half + r.pavement, PAVEMENT_LIFT, pavement()),
                (r.half, ROAD_LIFT, asphalt()),
            ] {
                // Run on past the ends by half the width, so bends close.
                let (a, c) = (a - dir * half.min(len), c + dir * half.min(len));
                let q = [a - n * half, c - n * half, c + n * half, a + n * half];
                let pts = q.map(|g| self.p(g, world.ground(g) + lift));
                self.face(&pts, &[BLANK; 4], Vec3::Y, color);
            }
        }
    }

    /// One building's mass. `light`: the far level (tall buildings only,
    /// walls in one band, no pitched roofs).
    fn building(&mut self, b: &WorldBuilding, light: bool) {
        let s = &b.site;
        if light && s.height() < FAR_MIN_HEIGHT {
            return;
        }
        let look = s.look();
        let tiers = s.massing();
        let ground = s.usage.ground_height();
        let paint = paints(s, &look);
        let (wall, roof) = (paint.wall, paint.roof);
        // The building's own run of window columns, so neighbours differ.
        let seed = (s.id.0 % 4093) as f64 * 37.0;
        let to = |x: f64, z: f64| s.origin + s.along * x + s.inward * z;
        // Storeys counted from the ground floor up, at height y over it.
        let storey = |y: f64| -> f64 {
            if y <= ground {
                y / ground
            } else {
                1.0 + (y - ground) / hw_gen::building::FLOOR_HEIGHT
            }
        };
        let bay = look.bay.max(1.0);
        let last = tiers.len().saturating_sub(1);
        for (ti, t) in tiers.iter().enumerate() {
            let o = t.outline.inset(INSET);
            if o.size().min_element() < 0.5 {
                continue;
            }
            let corners = [
                to(o.min.x, o.min.y),
                to(o.max.x, o.min.y),
                to(o.max.x, o.max.y),
                to(o.min.x, o.max.y),
            ];
            let mid = to(o.center().x, o.center().y);
            let top = t.top - 0.05;
            // Bands of wall: below the ground floor (no windows), the
            // ground floor, the storeys above. The light level: one.
            let mut cuts: Vec<f64> = Vec::new();
            if t.first == 0 {
                cuts.push(b.low - b.base - 0.5);
                if !light {
                    cuts.push(0.0);
                    if ground < top {
                        cuts.push(ground);
                    }
                } else {
                    cuts.push(0.0);
                }
            } else {
                cuts.push(t.bottom);
            }
            cuts.push(top);
            for w in 0..4 {
                let (a, c) = (corners[w], corners[(w + 1) % 4]);
                let len = a.distance(c);
                let u0 = seed + w as f64 * 211.0;
                let u1 = u0 + len / bay;
                let outward = {
                    let m = (a + c) * 0.5 - mid;
                    Vec3::new(m.x as f32, 0.0, m.y as f32)
                };
                for band in cuts.windows(2) {
                    let (y0, y1) = (band[0], band[1]);
                    if y1 - y0 < 0.01 {
                        continue;
                    }
                    let (v0, v1) = if y1 <= 0.0 {
                        (-1.0, -1.0)
                    } else {
                        (storey(y0.max(0.0)), storey(y1))
                    };
                    let pts = [
                        self.p(a, b.base + y0),
                        self.p(c, b.base + y0),
                        self.p(c, b.base + y1),
                        self.p(a, b.base + y1),
                    ];
                    let uv = [
                        [u0 as f32, v0 as f32],
                        [u1 as f32, v0 as f32],
                        [u1 as f32, v1 as f32],
                        [u0 as f32, v1 as f32],
                    ];
                    self.face(&pts, &uv, outward, wall);
                }
            }
            // The roof of this tier (the deck under the next one too).
            let y = b.base + top;
            let pitched = ti == last && matches!(look.roof, Roof::Gable | Roof::Hip);
            if pitched && !light {
                self.hip(&|x, z| to(x, z), t, o, y, &look, roof);
            } else if ti == last && look.roof == Roof::Mansard && !light {
                self.mansard(&|x, z| to(x, z), o, y, roof);
            } else {
                let pts = corners.map(|c| self.p(c, y));
                self.face(&pts, &[BLANK; 4], Vec3::Y, paint.deck);
            }
            if ti == last && look.roof == Roof::Flat {
                self.crown(&|x, z| to(x, z), t, y, &look, wall);
            }
        }
    }

    /// A hip roof over the inset outline `o`, as steep as the real gable or
    /// hip roof over the building (so it stays under it).
    #[allow(clippy::too_many_arguments)]
    fn hip(
        &mut self,
        to: &dyn Fn(f64, f64) -> DVec2,
        t: &Tier,
        o: hw_gen::geom::Rect,
        y: f64,
        look: &hw_gen::style::Look,
        color: Color,
    ) {
        let real = t.outline;
        let along_x = match look.ridge {
            Ridge::Long => real.size().x >= real.size().y,
            Ridge::Front => true,
            Ridge::Deep => false,
        };
        let real_half = if along_x {
            real.size().y
        } else {
            real.size().x
        } * 0.5
            + look.eaves;
        let slope = (real_half * look.pitch).clamp(1.0, 14.0) / real_half;
        let (s0, s1, t0, t1) = if along_x {
            (o.min.x, o.max.x, o.min.y, o.max.y)
        } else {
            (o.min.y, o.max.y, o.min.x, o.max.x)
        };
        let half = (t1 - t0) * 0.5;
        let h = slope * half;
        let tm = (t0 + t1) * 0.5;
        let inset = half.min((s1 - s0) * 0.5);
        let at = |s: f64, tt: f64| if along_x { to(s, tt) } else { to(tt, s) };
        let (a, bb, c, d) = (at(s0, t0), at(s1, t0), at(s1, t1), at(s0, t1));
        let (r0, r1) = (at(s0 + inset, tm), at(s1 - inset, tm));
        let mid = at((s0 + s1) * 0.5, tm);
        let up = |g: DVec2| {
            let m = g - mid;
            Vec3::new(m.x as f32, 1.0, m.y as f32)
        };
        let (pa, pb, pc, pd) = (self.p(a, y), self.p(bb, y), self.p(c, y), self.p(d, y));
        let (q0, q1) = (self.p(r0, y + h), self.p(r1, y + h));
        let side0 = (a + bb) * 0.5;
        let side1 = (c + d) * 0.5;
        if r0.distance(r1) > 0.01 {
            self.face(&[pa, pb, q1, q0], &[BLANK; 4], up(side0), color);
            self.face(&[pd, pc, q1, q0], &[BLANK; 4], up(side1), color);
        } else {
            self.face(&[pa, pb, q0], &[BLANK; 3], up(side0), color);
            self.face(&[pd, pc, q0], &[BLANK; 3], up(side1), color);
        }
        self.face(&[pa, pd, q0], &[BLANK; 3], up((a + d) * 0.5), color);
        self.face(&[pb, pc, q1], &[BLANK; 3], up((bb + c) * 0.5), color);
    }

    /// A mansard: sloping sides up to a flat top, inside the real one.
    fn mansard(
        &mut self,
        to: &dyn Fn(f64, f64) -> DVec2,
        o: hw_gen::geom::Rect,
        y: f64,
        color: Color,
    ) {
        let (inset, h) = (1.6, 2.5);
        let lo = [
            to(o.min.x, o.min.y),
            to(o.max.x, o.min.y),
            to(o.max.x, o.max.y),
            to(o.min.x, o.max.y),
        ];
        let i = o.inset(inset);
        if i.size().min_element() < 0.5 {
            let pts = lo.map(|c| self.p(c, y));
            self.face(&pts, &[BLANK; 4], Vec3::Y, color);
            return;
        }
        let hi = [
            to(i.min.x, i.min.y),
            to(i.max.x, i.min.y),
            to(i.max.x, i.max.y),
            to(i.min.x, i.max.y),
        ];
        let mid = to(o.center().x, o.center().y);
        for k in 0..4 {
            let n = (k + 1) % 4;
            let m = (lo[k] + lo[n]) * 0.5 - mid;
            let pts = [
                self.p(lo[k], y),
                self.p(lo[n], y),
                self.p(hi[n], y + h),
                self.p(hi[k], y + h),
            ];
            self.face(
                &pts,
                &[BLANK; 4],
                Vec3::new(m.x as f32, 1.0, m.y as f32),
                color,
            );
        }
        let pts = hi.map(|c| self.p(c, y + h));
        self.face(&pts, &[BLANK; 4], Vec3::Y, color);
    }

    /// The tiers and needle of a tower's crown (as `buildingmesh::crown`
    /// draws them, a little inside).
    fn crown(
        &mut self,
        to: &dyn Fn(f64, f64) -> DVec2,
        t: &Tier,
        y: f64,
        look: &hw_gen::style::Look,
        wall: Color,
    ) {
        if !matches!(look.crown, Crown::Stepped | Crown::Spire | Crown::Fins) {
            return;
        }
        let o = t.outline;
        let w = o.size().min_element();
        let mut r = o;
        let mut yy = y;
        for k in 0..3 {
            r = r.inset(w * 0.11);
            if r.size().min_element() < 2.0 {
                break;
            }
            let h = 4.2 - k as f64 * 0.7;
            self.block(to, r.inset(INSET), yy, yy + h + 0.25, wall);
            yy += h + 0.3;
        }
        if look.crown == Crown::Spire {
            let base = r.size().min_element() * 0.5;
            let h = (base * 5.0).clamp(10.0, 32.0);
            // A square needle inside the real eight-sided one.
            let a = base * 0.8 * 0.7;
            let c = r.center();
            let tip = self.p(to(c.x, c.y), yy + h * 0.95);
            let ring = [
                to(c.x - a, c.y - a),
                to(c.x + a, c.y - a),
                to(c.x + a, c.y + a),
                to(c.x - a, c.y + a),
            ];
            let mid = to(c.x, c.y);
            for k in 0..4 {
                let n = (k + 1) % 4;
                let m = (ring[k] + ring[n]) * 0.5 - mid;
                let pts = [self.p(ring[k], yy), self.p(ring[n], yy), tip];
                self.face(
                    &pts,
                    &[BLANK; 3],
                    Vec3::new(m.x as f32, 0.3, m.y as f32),
                    shade(wall, 0.7),
                );
            }
        }
    }

    /// A box over the outline `o` from `y0` to `y1`, without windows.
    fn block(
        &mut self,
        to: &dyn Fn(f64, f64) -> DVec2,
        o: hw_gen::geom::Rect,
        y0: f64,
        y1: f64,
        color: Color,
    ) {
        let c = [
            to(o.min.x, o.min.y),
            to(o.max.x, o.min.y),
            to(o.max.x, o.max.y),
            to(o.min.x, o.max.y),
        ];
        let mid = to(o.center().x, o.center().y);
        for k in 0..4 {
            let n = (k + 1) % 4;
            let m = (c[k] + c[n]) * 0.5 - mid;
            let pts = [
                self.p(c[k], y0),
                self.p(c[n], y0),
                self.p(c[n], y1),
                self.p(c[k], y1),
            ];
            self.face(
                &pts,
                &[BLANK; 4],
                Vec3::new(m.x as f32, 0.0, m.y as f32),
                color,
            );
        }
        let pts = c.map(|g| self.p(g, y1));
        self.face(&pts, &[BLANK; 4], Vec3::Y, color);
    }
}

/// A building's colours seen from afar: its paint times the average of
/// the texture it is drawn with (see `facades`), as the real building's
/// mipmaps blend them.
struct Paints {
    wall: Color,
    roof: Color,
    /// Flat roofs and terraces.
    deck: Color,
}

fn paints(site: &hw_gen::building::BuildingSite, look: &hw_gen::style::Look) -> Paints {
    // `facades` reads only the site and the look: a plan without rooms.
    let plan = hw_gen::building::BuildingPlan {
        site: site.clone(),
        core: None,
        floors: Vec::new(),
        openings: Vec::new(),
        layout: None,
        look: look.clone(),
    };
    let f = crate::facades::facade(&plan);
    let times = |c: [u8; 3], t: crate::skins::Tex| {
        let a = rgb(c[0], c[1], c[2]);
        let m = crate::skins::average(t);
        Color {
            r: a.r * m.r,
            g: a.g * m.g,
            b: a.b * m.b,
            a: 1.0,
        }
    };
    let deck = {
        let m = crate::skins::average(f.flat_roof.0);
        let c = f.flat_roof.1;
        Color {
            r: c.r * m.r,
            g: c.g * m.g,
            b: c.b * m.b,
            a: 1.0,
        }
    };
    Paints {
        wall: times(look.wall, f.wall),
        roof: times(look.roof_color, f.roof),
        deck,
    }
}

/// Steps a road's strip is drawn in (m).
const ROAD_STEP: f64 = 24.0;
/// Heights of the far roads over the ground: under the cells' pavements
/// (0.24 m) and carriageways (0.26 m).
const PAVEMENT_LIFT: f64 = 0.08;
const ROAD_LIFT: f64 = 0.12;

fn asphalt() -> Color {
    rgb(64, 66, 72)
}

fn pavement() -> Color {
    rgb(150, 146, 138)
}

/// A colour darkened by `k` (0..1).
fn shade(c: Color, k: f32) -> Color {
    Color {
        r: c.r * k,
        g: c.g * k,
        b: c.b * k,
        a: c.a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hw_core::Seed;
    use hw_gen::urban::TownParams;

    /// Every mass stays inside its real building (its walls and what stands
    /// on its roof), so a building drawn in full hides it whole; and the
    /// tile's meshes come out the same every time.
    #[test]
    #[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
    fn masses_stay_inside_their_buildings() {
        let tw = TownWorld::generate(Seed(5), TownParams::sample(Seed(5), 12_000.0));
        let core = tw.core();
        let (lo, hi) = (core - DVec2::splat(250.0), core + DVec2::splat(250.0));
        let (buildings, _) = tw.skyline_with_roads(lo, hi);
        assert!(buildings.len() > 30, "only {} buildings", buildings.len());
        for (id, b) in &buildings {
            let s = &b.site;
            let mut one = Masses::new(DVec2::ZERO);
            one.building(b, false);
            assert!(one.mesh.face_count() >= 5, "{id:?} has no mass");
            let top = b.base + s.top();
            for p in &one.points {
                assert!(p.y as f64 <= top + 1e-3, "{id:?} pokes out of its roof");
                assert!(p.y as f64 >= b.low - 0.6, "{id:?} hangs below its ground");
                // Within the footprint, in the building's frame.
                let g = DVec2::new(p.x as f64, p.z as f64) - s.origin;
                let (x, z) = (g.dot(s.along), g.dot(s.inward));
                assert!(
                    x > 0.0 && z > 0.0 && x < s.size.x && z < s.size.y,
                    "{id:?}: a corner at ({x:.2}, {z:.2}) outside {:?}",
                    s.size
                );
            }
        }
        let a = make_tile(&tw, tile_of(core), Detail::Mid);
        let b = make_tile(&tw, tile_of(core), Detail::Mid);
        assert_eq!(a.levels.len(), b.levels.len());
        for (x, y) in a.levels.iter().zip(&b.levels) {
            assert_eq!(x.0.triangle_count(), y.0.triangle_count());
        }
    }
}
