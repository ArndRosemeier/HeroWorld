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
/// Longest stretch of road between height samples (m).
const ROAD_STEP: f64 = 3.0;

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
        crate::undergroundview::density(self.world().underground(), p, ground)
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
    /// Where grass grows (the lawns drawn in each cell).
    pub green: Arc<Greenery>,
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
            green,
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
        if changed {
            if let Some(k) = self.cell_of_building(i) {
                self.rebuild(world, [k]);
            }
        }
    }

    /// Redraws the given cells if they are built.
    pub fn rebuild(&mut self, world: &mut World, cells: impl IntoIterator<Item = CellKey>) {
        for k in cells {
            if let Some(&d) = self.built.get(&k) {
                self.build_cell(world, k, d);
            }
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
        let gone: Vec<CellKey> = self
            .built
            .keys()
            .copied()
            .filter(|k| cell_centre(*k).distance(eye) > UNLOAD)
            .collect();
        for k in gone {
            self.drop_cell(world, k);
        }
        let want = |k: &CellKey| {
            let d = cell_centre(*k).distance(eye);
            let detail = if d < NEAR_DETAIL + CELL * 0.71 {
                Detail::Near
            } else if d < DISTANT_DETAIL {
                Detail::Far
            } else {
                Detail::Distant
            };
            (d, detail)
        };
        let mut todo: Vec<(f64, CellKey, Detail)> = self
            .cells_within(eye, LOAD)
            .into_iter()
            .filter_map(|k| {
                let (d, detail) = want(&k);
                (self.built.get(&k) != Some(&detail)).then_some((d, k, detail))
            })
            .collect();
        todo.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let wait = budget_ms.is_finite();
        let start = std::time::Instant::now();
        let mut later = Vec::new();
        for (_, k, detail) in todo {
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

    /// How much of the town around `eye` is built: (cells built, cells
    /// wanted), for a loading bar.
    pub fn progress(&self, eye: DVec2) -> (usize, usize) {
        let wanted = self.cells_within(eye, LOAD);
        let built = wanted.iter().filter(|k| self.built.contains_key(k)).count();
        (built, wanted.len())
    }

    /// Takes every built cell out of the world (before switching towns).
    pub fn clear(&mut self, world: &mut World) {
        let keys: Vec<CellKey> = self.built.keys().copied().collect();
        for k in keys {
            self.drop_cell(world, k);
        }
    }

    fn drop_cell(&mut self, world: &mut World, k: CellKey) {
        for e in self.entities.remove(&k).unwrap_or_default() {
            world.despawn(e);
        }
        for (i, _) in self.buildings.remove(&k).unwrap_or_default() {
            self.plans.remove(&i);
        }
        self.built.remove(&k);
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
        for e in self.entities.remove(&k).unwrap_or_default() {
            world.despawn(e);
        }
        let (lo, hi) = cell_box(k);
        let mut ids = Vec::new();
        // Roads first, one mesh for the cell.
        let mut b = MeshBuilder::new();
        for r in self.world.roads_in(lo, hi) {
            for run in runs_in(&r, k) {
                draw_road(&mut b, &self.world, &r, &run);
            }
        }
        // Buildings: each drawn at the town's block height, then lifted to
        // its own ground, on a plinth where the ground falls away.
        if !self.buildings.contains_key(&k) {
            let list = self.buildings_of(k);
            self.buildings.insert(k, list);
        }
        let list = self.buildings.get(&k).cloned().unwrap_or_default();
        for (i, wb) in &list {
            let plan = self.plan_of(*i, wb);
            if !self.hidden.contains(i) {
                let mut one = MeshBuilder::new();
                add_building(
                    &mut one,
                    &plan,
                    wb.site.district,
                    self.look_of(*i, wb),
                    detail,
                );
                let lift = Vec3::new(0.0, (wb.base - BLOCK_TOP) as f32, 0.0);
                b.append(one, lift);
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
                let full = std::mem::take(&mut b);
                ids.extend(full.finish_parts().into_iter().map(|m| world.spawn(m)));
            }
        }
        if !b.is_empty() {
            ids.extend(b.finish_parts().into_iter().map(|m| world.spawn(m)));
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
            crate::lawnmesh::lawn(&self.world, &self.green, lo, hi, step)
        };
        for m in lawns {
            let id = world.spawn(m);
            // Flat ground casts no shadow worth its cost.
            let _ = world.set_casts_shadow(id, false);
            ids.push(id);
        }
        // Inland water (a coastal town's comes from Engine's sea).
        if !self.world.coastal() {
            if let Some(mesh) = water_mesh(&self.world, k) {
                ids.push(world.spawn(mesh));
            }
        }
        for &id in &ids {
            world.set_name(id, format!("town cell {} {}", k.0, k.1));
        }
        self.entities.insert(k, ids);
        self.built.insert(k, detail);
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

fn centre(b: &hw_gen::urban::world::WorldBuilding) -> DVec2 {
    let s = &b.site;
    s.origin + s.along * s.size.x * 0.5 + s.inward * s.size.y * 0.5
}

/// The stretches of road `r` drawn by cell `k`: the road cut into steps of
/// at most [`ROAD_STEP`], each step drawn by the cell holding its middle, so
/// neighbouring cells' stretches meet end to end.
fn runs_in(r: &WorldRoad, k: CellKey) -> Vec<Vec<DVec2>> {
    let (lo, hi) = cell_box(k);
    let (lo, hi) = (lo - DVec2::ONE, hi + DVec2::ONE);
    let mut out = Vec::new();
    let mut run: Vec<DVec2> = Vec::new();
    let mut flush = |run: &mut Vec<DVec2>| {
        if run.len() >= 2 {
            out.push(std::mem::take(run));
        }
        run.clear();
    };
    for w in r.pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (s0, s1) = (a.min(b), a.max(b));
        if s1.x < lo.x || s0.x > hi.x || s1.y < lo.y || s0.y > hi.y {
            flush(&mut run);
            continue;
        }
        let n = (a.distance(b) / ROAD_STEP).ceil().max(1.0) as usize;
        for j in 0..n {
            let p = a.lerp(b, j as f64 / n as f64);
            let q = a.lerp(b, (j + 1) as f64 / n as f64);
            if cell_of(p.lerp(q, 0.5)) == k {
                if run.is_empty() {
                    run.push(p);
                }
                run.push(q);
            } else {
                flush(&mut run);
            }
        }
    }
    flush(&mut run);
    out
}

/// Which of four heights a road is drawn at, so crossing roads never fight.
fn road_layer(r: &WorldRoad) -> usize {
    ((r.id.0 ^ (r.id.0 >> 32)).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 62) as usize
}

/// One road run: pavements, then the carriageway a little higher so it wins
/// where roads cross. Strips are mitred along the run, so a bend has no
/// overlap.
fn draw_road(b: &mut MeshBuilder, w: &TownWorld, r: &WorldRoad, run: &[DVec2]) {
    if run.len() < 2 {
        return;
    }
    // Crossing roads at slightly different heights never fight.
    let lift = 0.02 + (road_layer(r) % 4) as f64 * 0.012;
    let asphalt = if r.main {
        rgb(58, 58, 62)
    } else {
        rgb(78, 76, 74)
    };
    let pavement = rgb(165, 160, 150);
    let total = r.half + r.pavement;
    b.tex = Some(GroundLayer::Paving.into());
    strip(b, w, run, -total, total, lift, pavement);
    // Old lanes are cobbled.
    let lane = r.style == Some(hw_gen::urban::streets::StreetStyle::Lanes);
    b.tex = Some(if lane {
        GroundLayer::Cobble.into()
    } else {
        GroundLayer::RoadAsphalt.into()
    });
    let asphalt = if lane { rgb(120, 116, 110) } else { asphalt };
    strip(b, w, run, -r.half, r.half, lift + 0.05, asphalt);
    b.tex = None;
}

/// A ribbon from offset `a` to `b` (left negative) along `pts`, draped on
/// the road height plus `lift`.
fn strip(b: &mut MeshBuilder, w: &TownWorld, pts: &[DVec2], a: f64, bo: f64, lift: f64, c: Color) {
    let n = pts.len();
    let normal = |k: usize| -> DVec2 {
        let d0 = if k > 0 {
            (pts[k] - pts[k - 1]).normalize_or_zero()
        } else {
            DVec2::ZERO
        };
        let d1 = if k + 1 < n {
            (pts[k + 1] - pts[k]).normalize_or_zero()
        } else {
            DVec2::ZERO
        };
        let m = (d0 + d1).normalize_or_zero().perp();
        // Mitre, limited at sharp turns.
        let d = if d1 != DVec2::ZERO { d1 } else { d0 };
        m / m.dot(d.perp()).max(0.5)
    };
    let at = |p: DVec2| DVec3::new(p.x, w.road_height(p) + lift, p.y);
    for k in 0..n - 1 {
        let (n0, n1) = (normal(k), normal(k + 1));
        let (p, q) = (pts[k], pts[k + 1]);
        let quad = [
            at(p + n0 * a),
            at(q + n1 * a),
            at(q + n1 * bo),
            at(p + n0 * bo),
        ];
        b.quad(quad, DVec3::Y, c);
    }
}
