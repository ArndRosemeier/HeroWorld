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
use hw_gen::urban::world::{TownWorld, WorldRoad, WATER_LEVEL};
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

/// The town's ground and water for Engine's world streaming.
pub struct TownGround(pub Arc<TownWorld>);

impl WorldGenerator for TownGround {
    fn height(&self, x: f32, z: f32) -> f32 {
        self.0.ground(DVec2::new(x as f64, z as f64)) as f32
    }

    /// The utility tunnels, lift shafts and Molehaven carved out of the rock.
    fn density(&self, p: glam::Vec3, ground: f32) -> f32 {
        crate::undergroundview::density(&self.0.underground, p, ground)
    }

    fn sea(&self) -> Option<engine::water::Sea> {
        self.0
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
        self.0.coastal()
    }
}

type CellKey = (i64, i64);

fn cell_of(p: DVec2) -> CellKey {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

fn cell_centre(k: CellKey) -> DVec2 {
    DVec2::new((k.0 as f64 + 0.5) * CELL, (k.1 as f64 + 0.5) * CELL)
}

/// One run of a road inside one cell.
struct RoadRun {
    road: usize,
    pts: Vec<DVec2>,
}

/// The roads and buildings around the camera.
pub struct TownView {
    pub world: Arc<TownWorld>,
    /// Buildings per cell (indices into `world.buildings`).
    buildings: BTreeMap<CellKey, Vec<usize>>,
    roads: BTreeMap<CellKey, Vec<RoadRun>>,
    /// Plans made so far (they take a moment each).
    plans: BTreeMap<usize, Arc<BuildingPlan>>,
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
    hidden: BTreeSet<usize>,
}

impl TownView {
    pub fn new(world: Arc<TownWorld>) -> Self {
        let mut buildings: BTreeMap<CellKey, Vec<usize>> = BTreeMap::new();
        for (i, b) in world.buildings.iter().enumerate() {
            buildings.entry(cell_of(centre(b))).or_default().push(i);
        }
        let mut roads: BTreeMap<CellKey, Vec<RoadRun>> = BTreeMap::new();
        for (ri, r) in world.roads.iter().enumerate() {
            // Cut the road where it crosses into another cell, so each cell
            // draws its own stretch and the stretches meet end to end.
            let pts = densify(&r.pts, ROAD_STEP);
            let mut run = vec![pts[0]];
            let mut key = cell_of(pts[0].lerp(pts[1], 0.5));
            for w in pts.windows(2) {
                let k = cell_of(w[0].lerp(w[1], 0.5));
                if k != key {
                    roads.entry(key).or_default().push(RoadRun {
                        road: ri,
                        pts: std::mem::replace(&mut run, vec![w[0]]),
                    });
                    key = k;
                }
                run.push(w[1]);
            }
            roads
                .entry(key)
                .or_default()
                .push(RoadRun { road: ri, pts: run });
        }
        let mut v = Self {
            world,
            buildings,
            roads,
            plans: BTreeMap::new(),
            entities: BTreeMap::new(),
            built: BTreeMap::new(),
            cut_on: false,
            cut_focus: DVec2::ZERO,
            cut_level: 1,
            inside: None,
            hidden: BTreeSet::new(),
        };
        v.cut_focus = v
            .tallest_near(v.world.spawn_line().0, 300.0)
            .map_or(DVec2::ZERO, |i| centre(&v.world.buildings[i]));
        v
    }

    pub fn building_count(&self) -> usize {
        self.world.buildings.len()
    }

    /// The tallest building within `r` of `p` (most floors).
    pub fn tallest_near(&self, p: DVec2, r: f64) -> Option<usize> {
        self.world
            .buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| centre(b).distance(p) < r)
            .max_by_key(|(i, b)| (b.site.floors, std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
    }

    pub fn centre_of(&self, i: usize) -> DVec2 {
        centre(&self.world.buildings[i])
    }

    /// The plan of building `i`, made on first use.
    pub fn plan(&mut self, i: usize) -> Arc<BuildingPlan> {
        let site = &self.world.buildings[i].site;
        self.plans
            .entry(i)
            .or_insert_with(|| Arc::new(BuildingPlan::generate(site)))
            .clone()
    }

    fn look_of(&self, i: usize) -> Look {
        if let Some(inside) = self.inside {
            if inside.building == i {
                return Look::Cut {
                    level: inside.level,
                };
            }
        }
        if self.cut_on && self.centre_of(i).distance(self.cut_focus) < CUT_RADIUS {
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

    pub fn cell_of_building(&self, i: usize) -> CellKey {
        cell_of(self.centre_of(i))
    }

    /// Leaves building `i` out of its cell (`true`) or draws it again, and
    /// redraws the cell.
    pub fn set_hidden(&mut self, world: &mut World, i: usize, hidden: bool) {
        let changed = if hidden {
            self.hidden.insert(i)
        } else {
            self.hidden.remove(&i)
        };
        if changed {
            let k = self.cell_of_building(i);
            self.rebuild(world, [k]);
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

    /// Builds cells coming into range and drops those out of range, nearest
    /// first, for at most `budget_ms` per call (at least one cell).
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
            .buildings
            .keys()
            .chain(self.roads.keys())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|k| {
                let (d, detail) = want(&k);
                (d < LOAD && self.built.get(&k) != Some(&detail)).then_some((d, k, detail))
            })
            .collect();
        todo.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let start = std::time::Instant::now();
        for (_, k, detail) in todo {
            self.build_cell(world, k, detail);
            if start.elapsed().as_secs_f64() * 1000.0 > budget_ms {
                break;
            }
        }
    }

    /// Builds every cell in range at once (the first frame).
    pub fn build_all(&mut self, world: &mut World, eye: DVec2) {
        self.update(world, eye, f64::INFINITY);
    }

    /// How much of the town around `eye` is built: (cells built, cells
    /// wanted), for a loading bar.
    pub fn progress(&self, eye: DVec2) -> (usize, usize) {
        let wanted: BTreeSet<CellKey> = self
            .buildings
            .keys()
            .chain(self.roads.keys())
            .copied()
            .filter(|k| cell_centre(*k).distance(eye) < LOAD)
            .collect();
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
        self.built.remove(&k);
    }

    fn build_cell(&mut self, world: &mut World, k: CellKey, detail: Detail) {
        for e in self.entities.remove(&k).unwrap_or_default() {
            world.despawn(e);
        }
        let mut ids = Vec::new();
        // Roads first, one mesh for the cell.
        let mut b = MeshBuilder::new();
        if let Some(runs) = self.roads.get(&k) {
            for run in runs {
                draw_road(
                    &mut b,
                    &self.world,
                    &self.world.roads[run.road],
                    run,
                    run.road,
                );
            }
        }
        // Buildings: each drawn at the town's block height, then lifted to
        // its own ground, on a plinth where the ground falls away.
        let list = self.buildings.get(&k).cloned().unwrap_or_default();
        for i in list {
            let plan = self.plan(i);
            let wb = &self.world.buildings[i];
            if !self.hidden.contains(&i) {
                let mut one = MeshBuilder::new();
                add_building(&mut one, &plan, wb.site.district, self.look_of(i), detail);
                let lift = Vec3::new(0.0, (wb.base - BLOCK_TOP) as f32, 0.0);
                b.append(one, lift);
            }
            if wb.base - wb.low > 0.05 {
                b.prism(
                    &wb.site.corners(),
                    wb.low - 1.0,
                    wb.base,
                    rgb(150, 142, 128),
                );
            }
            if b.face_count() > MESH_FACES {
                let full = std::mem::take(&mut b);
                ids.extend(full.finish_parts().into_iter().map(|m| world.spawn(m)));
            }
        }
        if !b.is_empty() {
            ids.extend(b.finish_parts().into_iter().map(|m| world.spawn(m)));
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

/// Points along a polyline no more than `step` apart.
fn densify(pts: &[DVec2], step: f64) -> Vec<DVec2> {
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let n = (w[0].distance(w[1]) / step).ceil().max(1.0) as usize;
        for k in 1..=n {
            out.push(w[0].lerp(w[1], k as f64 / n as f64));
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 1e-6);
    out
}

/// One road run: pavements, then the carriageway a little higher so it wins
/// where roads cross. Strips are mitred along the run, so a bend has no
/// overlap.
fn draw_road(b: &mut MeshBuilder, w: &TownWorld, r: &WorldRoad, run: &RoadRun, index: usize) {
    if run.pts.len() < 2 {
        return;
    }
    // Crossing roads at slightly different heights never fight.
    let lift = 0.02 + (index % 4) as f64 * 0.012;
    let asphalt = if r.main {
        rgb(58, 58, 62)
    } else {
        rgb(78, 76, 74)
    };
    let pavement = rgb(165, 160, 150);
    let total = r.half + r.pavement;
    b.tex = Some(GroundLayer::Paving.into());
    strip(b, w, &run.pts, -total, total, lift, pavement);
    // Old lanes are cobbled.
    let lane = r.style == Some(hw_gen::urban::streets::StreetStyle::Lanes);
    b.tex = Some(if lane {
        GroundLayer::Cobble.into()
    } else {
        GroundLayer::RoadAsphalt.into()
    });
    let asphalt = if lane { rgb(120, 116, 110) } else { asphalt };
    strip(b, w, &run.pts, -r.half, r.half, lift + 0.05, asphalt);
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
