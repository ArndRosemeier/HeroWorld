//! A whole town made ready for the game: ground heights (river beds and the
//! sea floor below the water), every road with its width, every building
//! with the height it stands at.
//!
//! Only the big picture is made up front (the town's plan, the ground, the
//! main roads and landmarks); side streets, buildings and props come one
//! unit at a time when asked for ([`super::quarter`]), so a megacity costs
//! no more than a village around the people in it.
//!
//! The town lies in its own local frame: plan (x, y) is ground (x, z), and
//! heights are metres above the water. All water stands at
//! [`WATER_LEVEL`]; the land rises from it.

use super::area::plan_all_or_stop;
use super::deep::Deep;
use super::growth::CentreKind;
use super::land::{distance_to, Grid};
use super::landmarks::{plan_landmarks, Landmark};
use super::plots::{plan_buildings, road_reach_at, side_reach};
use super::props::{plan_props, Prop};
use super::quarter::{
    bounds, overlap, plan_streets, Cache, DetailId, Quarter, Split, Streets, Unit, UnitKind,
    PIECE_ROADS, VERGE_TILE,
};
use super::roadgrid::RoadGrid;
use super::roads::RoadClass;
use super::streets::StreetStyle;
use super::surface::{SurfaceTile, KEEP_SURFACE_TILES, SURFACE_TILE};
use super::underground::{plan_underground, underground_box, Underground};
use super::waterfront::plan_waterfront;
use super::{Town, TownParams};
use crate::building::BuildingSite;
use glam::DVec2;
use hw_core::{Cancel, Cancelled, Seed};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Height of every water surface (sea, lakes, rivers).
pub const WATER_LEVEL: f64 = 0.0;
/// How far a road surface stands above the ground under it.
pub const ROAD_LIFT: f64 = 0.06;
/// Bridge decks and roads by the water never dip below this.
pub const DECK: f64 = 1.0;

/// A road ready to draw: centre line, half the carriageway, and the
/// pavement on each side.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldRoad {
    /// Stable name: main roads in [`Unit::MAIN`], district roads in
    /// their piece's [`PIECE_ROADS`] unit, side streets in their district.
    pub id: DetailId,
    pub pts: Vec<DVec2>,
    pub half: f64,
    pub pavement: f64,
    /// Main road (true) or side street.
    pub main: bool,
    /// A main road's class.
    pub class: Option<RoadClass>,
    /// A side street's style.
    pub style: Option<StreetStyle>,
    /// Which of four heights it is drawn at, unlike every road it meets
    /// (see [`super::surface::assign_layers`]).
    pub layer: u8,
}

/// Half the width of a moving car, with its mirrors (m).
pub const CAR_HALF: f64 = 1.1;
/// How far in from the kerb parked cars reach (m): the widest parked
/// car, from its place 1.15 m in, with a hand's breadth to spare.
pub const PARKED_ROOM: f64 = 2.4;
/// Between a moving car and anything beside it (m).
const CAR_SPARE: f64 = 0.15;
/// Half the room a person walking takes (m).
pub const WALKER_HALF: f64 = 0.3;

impl WorldRoad {
    /// Whether cars drive here: a lane each way, each wide enough for a car.
    pub fn has_traffic(&self) -> bool {
        self.pts.len() >= 2 && self.half >= 2.0 * (CAR_HALF + CAR_SPARE) + 0.2
    }

    /// Whether cars may park along both kerbs and still leave a lane each
    /// way beside them.
    pub fn parking_fits(&self) -> bool {
        self.half - PARKED_ROOM - CAR_SPARE >= 2.0 * CAR_HALF + 2.0 * CAR_SPARE
    }

    /// The middle of the lane each way, right of the centre line (m): half
    /// way to the kerb, or clear of the parked cars where there are any.
    pub fn lane_offset(&self) -> f64 {
        let free = if self.parking_fits() {
            self.half - PARKED_ROOM - CAR_SPARE
        } else {
            self.half
        };
        (self.half * 0.5)
            .min(free - CAR_HALF - CAR_SPARE)
            .max(CAR_HALF + CAR_SPARE)
    }

    /// The line people walk along the pavement, from the centre line (m):
    /// a little nearer the houses than the kerb, where the street lamps,
    /// trees and hydrants stand.
    pub fn walk_offset(&self) -> f64 {
        self.half + self.pavement * 0.55
    }

    /// Where along the kerb lamps, hydrants and tree pits stand, from the
    /// kerb (m).
    pub fn kerb_spot(&self) -> f64 {
        (self.pavement * 0.2).min(0.45)
    }
}

/// A building on the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldBuilding {
    pub site: BuildingSite,
    /// Height of its ground floor.
    pub base: f64,
    /// Lowest ground under it: a plinth fills the gap on a slope.
    pub low: f64,
}

/// A bridge deck: centre line ends, half width, deck height.
#[derive(Clone, Debug, PartialEq)]
pub struct Bridge {
    pub a: DVec2,
    pub b: DVec2,
    pub half: f64,
    pub deck: f64,
}

/// Grid cell of the indexes of pieces and main roads (m).
const INDEX_CELL: f64 = 512.0;
/// How far around a unit its planning looks for roads (m): far enough for
/// any street that reaches onto its ground.
const NEAR_ROADS: f64 = 60.0;
/// How many units' streets, buildings and splits are kept at once. Units
/// are a kilometre or less across, so this is a few square kilometres of
/// town around the people in it.
const KEEP_SPLITS: usize = 256;
const KEEP_STREETS: usize = 512;
const KEEP_QUARTERS: usize = 160;

/// The town, ready for the game.
pub struct TownWorld {
    pub seed: Seed,
    pub town: Town,
    /// The main roads, all of them (ids [`Unit::MAIN`], by index).
    pub main_roads: Vec<WorldRoad>,
    /// Where main roads cross water.
    pub main_bridges: Vec<Bridge>,
    /// The Aether Spire, Hero Hall, the Tesla tower, the rocket port and,
    /// by the sea, a lighthouse.
    pub landmarks: Vec<Landmark>,
    /// The deep underground, planned tile by tile.
    deep: Deep,
    /// Ground heights, river beds and sea floor included.
    terrain: Grid<f32>,
    /// The planet's ground beyond the town's land, if the town stands on a
    /// planet (see [`TownWorld::generate_on_or_stop`]).
    planet: Option<Arc<crate::planet::PlanetPatch>>,
    /// Bounding boxes of the town's pieces, and the pieces by grid cell.
    piece_boxes: Vec<(DVec2, DVec2)>,
    piece_grid: BTreeMap<(i64, i64), Vec<u32>>,
    /// Main road segments by grid cell: (road, segment).
    main_grid: BTreeMap<(i64, i64), Vec<(u32, u32)>>,
    splits: Cache<usize, Split>,
    streets: Cache<Unit, Streets>,
    quarters: Cache<Unit, Quarter>,
    /// Road surfaces by tile, for [`Self::surface`].
    surfaces: Cache<(i64, i64), SurfaceTile>,
    /// Utility tunnels, lifts and Molehaven, below the centre (planned on
    /// first use).
    underground: OnceLock<Underground>,
    /// Units being planned in the background (see [`Self::prefetch`]).
    pending: Mutex<std::collections::BTreeSet<Unit>>,
}

/// Side streets planned for one look from afar (see [`TownWorld::skyline`]).
type StreetsMemo = BTreeMap<Unit, Option<Arc<Streets>>>;

fn cell_of(p: DVec2) -> (i64, i64) {
    (
        (p.x / INDEX_CELL).floor() as i64,
        (p.y / INDEX_CELL).floor() as i64,
    )
}

fn cells(lo: DVec2, hi: DVec2) -> impl Iterator<Item = (i64, i64)> {
    let (a, b) = (cell_of(lo), cell_of(hi));
    (a.0..=b.0).flat_map(move |x| (a.1..=b.1).map(move |y| (x, y)))
}

impl TownWorld {
    pub fn generate(seed: Seed, params: TownParams) -> TownWorld {
        Self::generate_or_stop(seed, params, &Cancel::new()).expect("nobody can cancel this")
    }

    /// [`TownWorld::generate`] that gives up between its steps once
    /// `cancel` is set. The same world when it is not cancelled.
    pub fn generate_or_stop(
        seed: Seed,
        params: TownParams,
        cancel: &Cancel,
    ) -> Result<TownWorld, Cancelled> {
        Self::generate_on_or_stop(seed, params, None, cancel)
    }

    /// The town on a planet: its land is the town's own near the centre and
    /// fades into the planet's ground towards the edge of the planned land
    /// and beyond, so the town never ends at a wall of flat plain.
    pub fn generate_on_or_stop(
        seed: Seed,
        params: TownParams,
        planet: Option<Arc<crate::planet::PlanetPatch>>,
        cancel: &Cancel,
    ) -> Result<TownWorld, Cancelled> {
        let town = Town::generate_or_stop(seed, params, cancel)?;
        cancel.check()?;
        let land = &town.land;
        // Water beds: deeper away from the shore.
        let to_land = distance_to(&land.cover, |c| !c.is_water());
        let mut terrain = land.height.clone();
        for j in 0..terrain.h {
            for i in 0..terrain.w {
                if land.cover.at(i, j).is_water() {
                    let d = *to_land.at(i, j) as f64;
                    let depth = 1.5 + (d * 0.12).min(12.0);
                    terrain.set(i, j, (WATER_LEVEL - depth) as f32);
                }
            }
        }
        let piece_boxes: Vec<(DVec2, DVec2)> =
            town.pieces.iter().map(|p| bounds(&p.face.pts)).collect();
        let mut piece_grid: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
        for (i, b) in piece_boxes.iter().enumerate() {
            for c in cells(b.0, b.1) {
                piece_grid.entry(c).or_default().push(i as u32);
            }
        }
        let mut out = TownWorld {
            seed,
            town,
            main_roads: Vec::new(),
            main_bridges: Vec::new(),
            landmarks: Vec::new(),
            deep: Deep::new(),
            terrain,
            planet,
            piece_boxes,
            piece_grid,
            main_grid: BTreeMap::new(),
            splits: Cache::new(KEEP_SPLITS),
            streets: Cache::new(KEEP_STREETS),
            quarters: Cache::new(KEEP_QUARTERS),
            surfaces: Cache::new(KEEP_SURFACE_TILES),
            underground: OnceLock::new(),
            pending: Mutex::new(Default::default()),
        };
        cancel.check()?;
        cancel.stage("Laying the main roads", 0.08, 0.09);
        let mut mains = Vec::new();
        for (i, r) in out.town.network.roads.iter().enumerate() {
            out.main_runs(&r.pts, r.class, i, &mut mains);
        }
        // Ids by their place in the list.
        for (n, r) in mains.iter_mut().enumerate() {
            r.id = DetailId::new(Unit::MAIN, n);
        }
        for (ri, r) in mains.iter().enumerate() {
            for (si, s) in r.pts.windows(2).enumerate() {
                let reach = r.half + r.pavement;
                let lo = s[0].min(s[1]) - DVec2::splat(reach);
                let hi = s[0].max(s[1]) + DVec2::splat(reach);
                for c in cells(lo, hi) {
                    out.main_grid
                        .entry(c)
                        .or_default()
                        .push((ri as u32, si as u32));
                }
            }
        }
        cancel.check()?;
        cancel.stage("Crossing the main roads", 0.09, 0.095);
        super::surface::assign_layers(&[], &mut mains);
        cancel.stage("Building the bridges", 0.095, 0.1);
        out.main_bridges = out.bridges_of(&mains);
        out.main_roads = mains;
        cancel.check()?;
        out.landmarks = plan_landmarks(&out, cancel)?;
        Ok(out)
    }

    /// A main road (or district road) split into runs of one width: in an
    /// old town even a main road is narrow.
    fn main_runs(&self, pts: &[DVec2], class: RoadClass, _i: usize, out: &mut Vec<WorldRoad>) {
        let reach_at = |p: DVec2| road_reach_at(&self.town, class, p);
        let mut run: Vec<DVec2> = vec![pts[0]];
        let mut cur = reach_at(pts[0]);
        for w in pts.windows(2) {
            let here = reach_at((w[0] + w[1]) * 0.5);
            if (here - cur).abs() > 1e-9 && run.len() >= 2 {
                out.push(road(
                    DetailId::default(),
                    std::mem::take(&mut run),
                    cur,
                    Some(class),
                    None,
                ));
                run.push(w[0]);
            }
            cur = here;
            run.push(w[1]);
        }
        if run.len() >= 2 {
            out.push(road(DetailId::default(), run, cur, Some(class), None));
        }
    }

    /// The deep underground's tile cache and network.
    pub(crate) fn deep_state(&self) -> &Deep {
        &self.deep
    }

    /// The shallow underground: utility tunnels, lifts, Molehaven.
    pub fn core_underground(&self) -> &Underground {
        self.underground()
    }

    /// Ground height at a point (below [`WATER_LEVEL`] under water).
    pub fn ground(&self, p: DVec2) -> f64 {
        let town = self.terrain.sample(p);
        let Some(planet) = &self.planet else {
            return town;
        };
        // The town's own land to 72% of the way to the edge, the planet's
        // beyond it, a smooth blend between.
        let edge = p.x.abs().max(p.y.abs()) / self.terrain.half();
        let w = super::land::smooth(0.72, 1.0, edge);
        if w <= 0.0 {
            town
        } else {
            town + (planet.ground(p) - town) * w
        }
    }

    /// The planet this town stands on, if any.
    pub fn planet(&self) -> Option<&Arc<crate::planet::PlanetPatch>> {
        self.planet.as_ref()
    }

    /// Height of a road surface at a point: the ground, lifted a little,
    /// and never below a deck over the water.
    pub fn road_height(&self, p: DVec2) -> f64 {
        (self.ground(p) + ROAD_LIFT).max(DECK + ROAD_LIFT)
    }

    /// The road surface over tile `k` (see [`Self::surface`]), made on
    /// first ask and kept while there is room.
    pub(super) fn surface_tile(&self, k: (i64, i64)) -> Arc<SurfaceTile> {
        self.surfaces.get(k, || {
            let lo = DVec2::new(k.0 as f64, k.1 as f64) * SURFACE_TILE;
            let hi = lo + DVec2::splat(SURFACE_TILE);
            // Mitred bends reach out further than a road's width.
            let m = DVec2::splat(16.0);
            SurfaceTile::new(&self.roads_in(lo - m, hi + m), k)
        })
    }

    /// Does the town meet the sea?
    pub fn coastal(&self) -> bool {
        !self.town.land.sea.is_empty()
    }

    /// Half the side of the planned land around the town.
    pub fn half(&self) -> f64 {
        self.terrain.half()
    }

    /// The town's core (its oldest centre).
    pub fn core(&self) -> DVec2 {
        self.town
            .growth
            .centres
            .iter()
            .find(|c| c.kind == CentreKind::Core)
            .map_or(DVec2::ZERO, |c| c.pos)
    }

    /// A piece cut into its districts (cached).
    pub fn split(&self, piece: usize) -> Arc<Split> {
        self.splits.get(piece, || {
            let pc = &self.town.pieces[piece];
            let (districts, cuts) = super::districts(self.seed, &self.town, pc);
            let mut roads = Vec::new();
            for (i, r) in cuts.iter().enumerate() {
                self.main_runs(&r.pts, r.class, i, &mut roads);
            }
            let unit = Unit::district(piece, PIECE_ROADS as usize);
            for (n, r) in roads.iter_mut().enumerate() {
                r.id = DetailId::new(unit, n);
            }
            let (lo, hi) = super::surface::roads_box(&roads);
            let mains: Vec<WorldRoad> = self
                .main_in(lo, hi)
                .into_iter()
                .map(|i| self.main_roads[i].clone())
                .collect();
            super::surface::assign_layers(&mains, &mut roads);
            let bridges = self.bridges_of(&roads);
            Split {
                boxes: districts.iter().map(|d| bounds(&d.face.pts)).collect(),
                districts,
                roads,
                bridges,
            }
        })
    }

    /// A district's side streets (cached); `None` for a verge tile.
    pub fn streets(&self, unit: Unit) -> Option<Arc<Streets>> {
        let UnitKind::District { piece, district } = unit.kind() else {
            return None;
        };
        let split = self.split(piece);
        let d = split.districts.get(district)?;
        Some(self.streets.get(unit, || self.plan_streets_of(unit, d)))
    }

    /// A district's side streets, planned now (not cached).
    fn plan_streets_of(&self, unit: Unit, d: &super::Piece) -> Streets {
        plan_streets(self.seed, &self.town, unit, d, |lines, plan| {
            let reach = side_reach(plan.style);
            let mut roads: Vec<WorldRoad> = lines
                .into_iter()
                .filter(|s| s.len() >= 2)
                .enumerate()
                .map(|(n, s)| road(DetailId::new(unit, n), s, reach, None, Some(plan.style)))
                .collect();
            // Unlike the main and district roads they meet (of any piece),
            // and each other.
            let (lo, hi) = super::surface::roads_box(&roads);
            let mut fixed: Vec<WorldRoad> = self
                .main_in(lo, hi)
                .into_iter()
                .map(|i| self.main_roads[i].clone())
                .collect();
            for piece in self.pieces_in(lo, hi) {
                fixed.extend(self.split(piece).roads.iter().cloned());
            }
            super::surface::assign_layers(&fixed, &mut roads);
            let bridges = self.bridges_of(&roads);
            (roads, bridges)
        })
    }

    /// [`Self::streets`] for a look from afar: what the cache holds, else
    /// planned now and kept only in `memo` (the cache stays as it was, so
    /// the streets the heroes walk are never pushed out by a distant view).
    fn streets_for_a_look(&self, unit: Unit, memo: &mut StreetsMemo) -> Option<Arc<Streets>> {
        if let Some(s) = memo.get(&unit) {
            return s.clone();
        }
        let s = match unit.kind() {
            UnitKind::District { piece, district } => self.streets.peek(unit).or_else(|| {
                let split = self.split(piece);
                let d = split.districts.get(district)?;
                Some(Arc::new(self.plan_streets_of(unit, d)))
            }),
            _ => None,
        };
        memo.insert(unit, s.clone());
        s
    }

    /// The buildings whose centres lie in the box `lo`..`hi`, with their
    /// ids: exactly what the quarters there hold, for drawing the town from
    /// afar. Quarters planned already are read; the rest are planned here
    /// as far as their buildings and forgotten again, so neither the
    /// quarter nor the street cache changes (a skyline kilometres wide must
    /// not push out the detail round the heroes). Thread-safe; slow (a few
    /// milliseconds per district): call it on a worker.
    pub fn skyline(&self, lo: DVec2, hi: DVec2) -> Vec<(DetailId, WorldBuilding)> {
        self.skyline_with_roads(lo, hi).0
    }

    /// [`Self::skyline`], and every road that comes into the box (main
    /// roads, district roads and side streets), planned the same way.
    pub fn skyline_with_roads(
        &self,
        lo: DVec2,
        hi: DVec2,
    ) -> (Vec<(DetailId, WorldBuilding)>, Vec<WorldRoad>) {
        let mut memo = StreetsMemo::new();
        let mut out = Vec::new();
        let inside = |b: &WorldBuilding| {
            let c = b.site.origin
                + (b.site.along * b.site.size.x + b.site.inward * b.site.size.y) * 0.5;
            c.x >= lo.x && c.y >= lo.y && c.x < hi.x && c.y < hi.y
        };
        for unit in self.units_in(lo, hi) {
            if !matches!(unit.kind(), UnitKind::District { .. }) {
                continue;
            }
            if let Some(q) = self.quarters.peek(unit) {
                out.extend(
                    q.buildings_with_ids()
                        .filter(|(_, b)| inside(b))
                        .map(|(i, b)| (i, b.clone())),
                );
                continue;
            }
            let Some(streets) = self.streets_for_a_look(unit, &mut memo) else {
                continue;
            };
            let m = DVec2::splat(NEAR_ROADS);
            let roads = self.roads_in_with(streets.lo - m, streets.hi + m, &mut |u| {
                self.streets_for_a_look(u, &mut memo)
            });
            for (i, b) in self
                .buildings_on(unit, Some(&streets), &roads)
                .into_iter()
                .enumerate()
            {
                if inside(&b) {
                    out.push((DetailId::new(unit, i), b));
                }
            }
        }
        out.sort_by_key(|(id, _)| *id);
        let roads = self.roads_in_with(lo, hi, &mut |u| self.streets_for_a_look(u, &mut memo));
        (out, roads)
    }

    /// The units whose ground comes within the box `lo`..`hi`: districts,
    /// and the verge tiles there that hold a main road.
    pub fn units_in(&self, lo: DVec2, hi: DVec2) -> Vec<Unit> {
        let mut out = Vec::new();
        for p in self.pieces_in(lo, hi) {
            let split = self.split(p);
            for (d, b) in split.boxes.iter().enumerate() {
                if overlap(*b, (lo, hi), 0.0) {
                    out.push(Unit::district(p, d));
                }
            }
        }
        let t = |v: f64| (v / VERGE_TILE).floor() as i64;
        for y in t(lo.y)..=t(hi.y) {
            for x in t(lo.x)..=t(hi.x) {
                let a = DVec2::new(x as f64, y as f64) * VERGE_TILE;
                if !self.main_in(a, a + DVec2::splat(VERGE_TILE)).is_empty() {
                    out.push(Unit::verge(x, y));
                }
            }
        }
        out
    }

    /// Pieces whose boxes come within the box.
    fn pieces_in(&self, lo: DVec2, hi: DVec2) -> Vec<usize> {
        let mut out: Vec<usize> = cells(lo, hi)
            .filter_map(|c| self.piece_grid.get(&c))
            .flatten()
            .map(|&i| i as usize)
            .filter(|&i| overlap(self.piece_boxes[i], (lo, hi), 0.0))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Main roads (by index) with a segment near the box.
    fn main_in(&self, lo: DVec2, hi: DVec2) -> Vec<usize> {
        let mut out: Vec<usize> = cells(lo, hi)
            .filter_map(|c| self.main_grid.get(&c))
            .flatten()
            .filter(|&&(r, s)| {
                let road = &self.main_roads[r as usize];
                let (a, b) = (road.pts[s as usize], road.pts[s as usize + 1]);
                let reach = road.half + road.pavement;
                overlap(
                    (
                        a.min(b) - DVec2::splat(reach),
                        a.max(b) + DVec2::splat(reach),
                    ),
                    (lo, hi),
                    0.0,
                )
            })
            .map(|&(r, _)| r as usize)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Every road reaching into the box: main roads, district roads and
    /// side streets (a road appears whole).
    pub fn roads_in(&self, lo: DVec2, hi: DVec2) -> Vec<WorldRoad> {
        self.roads_in_with(lo, hi, &mut |u| self.streets(u))
    }

    /// [`Self::roads_in`] with the side streets from `streets`.
    fn roads_in_with(
        &self,
        lo: DVec2,
        hi: DVec2,
        streets: &mut dyn FnMut(Unit) -> Option<Arc<Streets>>,
    ) -> Vec<WorldRoad> {
        let mut out: Vec<WorldRoad> = self
            .main_in(lo, hi)
            .into_iter()
            .map(|i| self.main_roads[i].clone())
            .collect();
        for p in self.pieces_in(lo, hi) {
            let split = self.split(p);
            out.extend(
                split
                    .roads
                    .iter()
                    .filter(|r| road_touches(r, lo, hi))
                    .cloned(),
            );
            for (d, b) in split.boxes.iter().enumerate() {
                if !overlap(*b, (lo, hi), NEAR_ROADS) {
                    continue;
                }
                if let Some(s) = streets(Unit::district(p, d)) {
                    out.extend(s.roads.iter().filter(|r| road_touches(r, lo, hi)).cloned());
                }
            }
        }
        // A street reaching into two districts' boxes is found twice:
        // once is enough (twice is drawn twice, over itself).
        let mut seen = std::collections::BTreeSet::new();
        out.retain(|r| seen.insert(r.id));
        out
    }

    /// Plans, on every core, the district cuts and side streets that
    /// [`Self::roads_in`] over the box will ask for one by one, so that it
    /// finds them cached (a big city's box takes seconds on one thread).
    pub(super) fn warm_roads(
        &self,
        lo: DVec2,
        hi: DVec2,
        cancel: &Cancel,
    ) -> Result<(), Cancelled> {
        let pieces = self.pieces_in(lo, hi);
        // First every piece's cut into districts, then every district's
        // streets: a big city has few pieces but many districts, so the
        // second step is what spreads over the cores.
        self.in_parallel(pieces.len(), cancel, |k| {
            self.split(pieces[k]);
        })?;
        let mut units = Vec::new();
        for &p in &pieces {
            for (d, b) in self.split(p).boxes.iter().enumerate() {
                if overlap(*b, (lo, hi), NEAR_ROADS) {
                    units.push(Unit::district(p, d));
                }
            }
        }
        self.in_parallel(units.len(), cancel, |k| {
            self.streets(units[k]);
        })
    }

    /// Runs `job(0..n)` on every core, stopping early once cancelled, and
    /// reports the share done as the stage's progress.
    fn in_parallel(
        &self,
        n: usize,
        cancel: &Cancel,
        job: impl Fn(usize) + Sync,
    ) -> Result<(), Cancelled> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let next = AtomicUsize::new(0);
        let threads = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .clamp(1, 8);
        std::thread::scope(|s| {
            let jobs: Vec<_> = (0..threads)
                .map(|_| {
                    s.spawn(|| -> Result<(), Cancelled> {
                        loop {
                            let k = next.fetch_add(1, Ordering::Relaxed);
                            if k >= n {
                                return Ok(());
                            }
                            cancel.check()?;
                            job(k);
                            cancel.set_fraction((k + 1) as f32 / n as f32);
                        }
                    })
                })
                .collect();
            let mut out = Ok(());
            for j in jobs {
                out = out.and(j.join().expect("planning in parallel"));
            }
            out
        })
    }

    /// Every bridge deck in the box.
    pub fn bridges_in(&self, lo: DVec2, hi: DVec2) -> Vec<Bridge> {
        let near = |b: &&Bridge| overlap((b.a.min(b.b), b.a.max(b.b)), (lo, hi), b.half);
        let mut out: Vec<Bridge> = self.main_bridges.iter().filter(near).cloned().collect();
        for p in self.pieces_in(lo, hi) {
            let split = self.split(p);
            out.extend(split.bridges.iter().filter(near).cloned());
            for (d, b) in split.boxes.iter().enumerate() {
                if overlap(*b, (lo, hi), NEAR_ROADS) {
                    if let Some(s) = self.streets(Unit::district(p, d)) {
                        out.extend(s.bridges.iter().filter(near).cloned());
                    }
                }
            }
        }
        out
    }

    /// The district or verge tile that owns the ground at `p`.
    pub fn unit_at(&self, p: DVec2) -> Unit {
        for i in self.pieces_in(p, p) {
            if !super::contains(&self.town.pieces[i].face.pts, p) {
                continue;
            }
            let split = self.split(i);
            for (d, dist) in split.districts.iter().enumerate() {
                if super::contains(&dist.face.pts, p) {
                    return Unit::district(i, d);
                }
            }
        }
        Unit::verge_at(p)
    }

    /// Is `p` on the ground of a piece (not the verge)?
    pub fn in_a_piece(&self, p: DVec2) -> bool {
        self.pieces_in(p, p)
            .into_iter()
            .any(|i| super::contains(&self.town.pieces[i].face.pts, p))
    }

    /// One unit's buildings, props and quay walls (planned on first ask,
    /// then kept while there is room).
    pub fn quarter(&self, unit: Unit) -> Arc<Quarter> {
        self.quarters.get(unit, || self.plan_quarter(unit))
    }

    /// Every unit's quarter in the box.
    pub fn quarters_in(&self, lo: DVec2, hi: DVec2) -> Vec<Arc<Quarter>> {
        self.units_in(lo, hi)
            .into_iter()
            .map(|u| self.quarter(u))
            .collect()
    }

    /// Are all the quarters in the box planned already (so asking for
    /// them costs nothing)?
    pub fn quarters_ready(&self, lo: DVec2, hi: DVec2) -> bool {
        self.units_in(lo, hi)
            .into_iter()
            .all(|u| self.quarters.contains(u))
    }

    /// Is `unit` one of this town's units that can hold things?
    fn holds_things(&self, unit: Unit) -> bool {
        match unit.kind() {
            UnitKind::District { piece, district } => {
                piece < self.town.pieces.len()
                    && district != PIECE_ROADS as usize
                    && district < self.split(piece).districts.len()
            }
            UnitKind::Verge { x, y } => {
                let reach = (self.half() / VERGE_TILE).ceil() as i64 + 2;
                x.abs() <= reach && y.abs() <= reach
            }
            UnitKind::Main => false,
        }
    }

    /// A building by id.
    pub fn building(&self, id: DetailId) -> Option<WorldBuilding> {
        if !self.holds_things(id.unit()) {
            return None;
        }
        self.quarter(id.unit()).buildings.get(id.index()).cloned()
    }

    /// A prop by id.
    pub fn prop(&self, id: DetailId) -> Option<Prop> {
        if !self.holds_things(id.unit()) {
            return None;
        }
        self.quarter(id.unit()).props.get(id.index()).copied()
    }

    /// A road by id.
    pub fn road(&self, id: DetailId) -> Option<WorldRoad> {
        let u = id.unit();
        match u.kind() {
            UnitKind::Main => self.main_roads.get(id.index()).cloned(),
            UnitKind::District { piece, district } if district == PIECE_ROADS as usize => {
                self.split(piece).roads.get(id.index()).cloned()
            }
            UnitKind::District { .. } => self.streets(u)?.roads.get(id.index()).cloned(),
            UnitKind::Verge { .. } => None,
        }
    }

    /// Every unit of the town. Tools and tests only: planning them all
    /// is what streaming avoids.
    pub fn all_units(&self) -> Vec<Unit> {
        let h = self.half();
        self.units_in(DVec2::splat(-h), DVec2::splat(h))
    }

    /// The whole town at once. Tools and tests only: this plans every unit,
    /// which is what streaming avoids.
    pub fn everything(&self) -> Everything {
        let mut all = Everything {
            roads: self.main_roads.clone(),
            bridges: self.main_bridges.clone(),
            ..Default::default()
        };
        for p in 0..self.town.pieces.len() {
            let split = self.split(p);
            all.roads.extend(split.roads.iter().cloned());
            all.bridges.extend(split.bridges.iter().cloned());
        }
        for u in self.all_units() {
            let q = self.quarter(u);
            if let Some(s) = &q.streets {
                all.roads.extend(s.roads.iter().cloned());
                all.bridges.extend(s.bridges.iter().cloned());
                all.streets.push(s.clone());
            }
            all.buildings
                .extend(q.buildings_with_ids().map(|(i, b)| (i, b.clone())));
            all.props.extend(q.props_with_ids().map(|(i, p)| (i, *p)));
            all.quays.extend(q.quays.iter().cloned());
            all.water_steps.extend(q.water_steps.iter().copied());
        }
        all
    }

    /// How many quarters are planned and kept now.
    pub fn quarters_kept(&self) -> usize {
        self.quarters.len()
    }

    /// Plans the quarters around `points` (within `r`) on a background
    /// thread, so they are ready before anyone asks.
    pub fn prefetch(self: &Arc<Self>, points: &[DVec2], r: f64) {
        let mut want = Vec::new();
        for p in points {
            for u in self.units_in(*p - DVec2::splat(r), *p + DVec2::splat(r)) {
                want.push(u);
            }
        }
        want.sort_unstable();
        want.dedup();
        let want: Vec<Unit> = {
            let mut pending = self.pending.lock().expect("pending lock");
            let mut map = Vec::new();
            for u in want {
                if !self.quarters.contains(u) && pending.insert(u) {
                    map.push(u);
                }
            }
            map
        };
        if want.is_empty() {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            for u in want {
                me.quarter(u);
                me.pending.lock().expect("pending lock").remove(&u);
            }
        });
    }

    /// How many units are being planned in the background now.
    pub fn planning(&self) -> usize {
        self.pending.lock().expect("pending lock").len()
    }

    /// The underground (planned on first use, from the quarters round
    /// the centre).
    pub fn underground(&self) -> &Underground {
        self.underground.get_or_init(|| plan_underground(self))
    }

    /// [`TownWorld::underground`], planning the detail it reads first on
    /// every core, where `cancel` can stop it between units.
    pub fn underground_or_stop(
        self: &Arc<Self>,
        cancel: &Cancel,
    ) -> Result<&Underground, Cancelled> {
        if let Some(u) = self.underground.get() {
            return Ok(u);
        }
        let (lo, hi) = underground_box(self);
        let units = self.units_in(lo, hi);
        cancel.stage("Digging the underground", 0.2, 0.4);
        plan_all_or_stop(self, &units, cancel)?;
        cancel.check()?;
        Ok(self.underground())
    }

    /// Where heroes start: on a street near the town's core, and the way
    /// along it.
    pub fn spawn_line(&self) -> (DVec2, DVec2) {
        let core = self.core();
        let mut best: Option<(f64, DVec2, DVec2)> = None;
        for reach in [150.0, 400.0, 1000.0, 2500.0] {
            let d = DVec2::splat(reach);
            for r in self.roads_in(core - d, core + d).iter().filter(|r| !r.main) {
                for w in r.pts.windows(2) {
                    let m = (w[0] + w[1]) * 0.5;
                    if w[0].distance(w[1]) < 4.0 || self.ground(m) < DECK {
                        continue;
                    }
                    let d = m.distance(core);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, m, (w[1] - w[0]).normalize()));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map_or((core, DVec2::X), |b| (b.1, b.2))
    }

    /// Plans one unit.
    fn plan_quarter(&self, unit: Unit) -> Quarter {
        let streets = self.streets(unit);
        let (lo, hi) = match (&streets, unit.kind()) {
            (Some(s), _) => (s.lo, s.hi),
            (None, UnitKind::Verge { x, y }) => {
                let a = DVec2::new(x as f64, y as f64) * VERGE_TILE;
                (a, a + DVec2::splat(VERGE_TILE))
            }
            _ => (DVec2::ZERO, DVec2::ZERO),
        };
        let m = DVec2::splat(NEAR_ROADS);
        let roads = self.roads_in(lo - m, hi + m);
        let bridges = self.bridges_in(lo - m, hi + m);
        let owner = Owner {
            world: self,
            unit,
            district: streets.as_ref().map(|s| s.piece.face.pts.clone()),
            lo,
            hi,
        };
        let buildings = self.buildings_on(unit, streets.as_deref(), &roads);
        let local = Local {
            world: self,
            roads,
            bridges,
            owner,
            streets: streets.clone(),
            buildings,
        };
        let (quays, water_steps) = plan_waterfront(&local);
        let props = plan_props(&local, &quays, &water_steps);
        Quarter {
            unit,
            lo,
            hi,
            streets,
            buildings: local.buildings,
            props,
            quays,
            water_steps,
        }
    }

    /// A district's buildings, given its streets and the roads near it
    /// (none on a verge tile).
    fn buildings_on(
        &self,
        unit: Unit,
        streets: Option<&Streets>,
        roads: &[WorldRoad],
    ) -> Vec<WorldBuilding> {
        let mut buildings = Vec::new();
        if let (Some(s), UnitKind::District { piece, .. }) = (streets, unit.kind()) {
            let grid = RoadGrid::new(roads, 0.0);
            for site in plan_buildings(self.seed, &self.town, piece, &s.piece, &s.plan) {
                // Never on a street, not even a neighbouring district's (a
                // metre's grace on its pavement, as round a bend).
                if on_a_street(&grid, &site) {
                    continue;
                }
                let corners = site.corners();
                let hs: Vec<f64> = corners.iter().map(|c| self.ground(*c)).collect();
                let hi = hs.iter().copied().fold(f64::MIN, f64::max);
                let lo = hs.iter().copied().fold(f64::MAX, f64::min);
                // A building never stands in the water.
                if lo < WATER_LEVEL + 0.3 {
                    continue;
                }
                // Nor on a landmark's plinth.
                if self.landmarks.iter().any(|l| l.overlaps(&site, 1.5)) {
                    continue;
                }
                buildings.push(WorldBuilding {
                    site,
                    base: hi + 0.1,
                    low: lo,
                });
            }
        }
        buildings
    }

    /// Every stretch of road over water is a bridge.
    fn bridges_of(&self, roads: &[WorldRoad]) -> Vec<Bridge> {
        let mut out = Vec::new();
        for r in roads {
            let wet = |p: DVec2| self.ground(p) < DECK;
            let mut start: Option<DVec2> = None;
            let step = 2.0;
            let mut last = r.pts[0];
            for w in r.pts.windows(2) {
                let l = w[0].distance(w[1]);
                let n = (l / step).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let p = w[0].lerp(w[1], k as f64 / n as f64);
                    match (start, wet(p)) {
                        (None, true) => start = Some(last),
                        (Some(a), false) => {
                            if a.distance(p) > 1.0 {
                                out.push(Bridge {
                                    a,
                                    b: p,
                                    half: r.half + r.pavement,
                                    deck: DECK + ROAD_LIFT,
                                });
                            }
                            start = None;
                        }
                        _ => {}
                    }
                    last = p;
                }
            }
        }
        out
    }
}

/// The whole town at once ([`TownWorld::everything`]).
#[derive(Default)]
pub struct Everything {
    /// Main roads, district roads and side streets.
    pub roads: Vec<WorldRoad>,
    pub bridges: Vec<Bridge>,
    pub buildings: Vec<(DetailId, WorldBuilding)>,
    pub props: Vec<(DetailId, Prop)>,
    pub quays: Vec<super::waterfront::QuayWall>,
    pub water_steps: Vec<super::waterfront::WaterSteps>,
    /// Every district's streets.
    pub streets: Vec<Arc<Streets>>,
}

/// Which ground a unit owns: a district's outline, or a verge tile's
/// square outside every piece.
pub(super) struct Owner<'a> {
    world: &'a TownWorld,
    pub unit: Unit,
    district: Option<Vec<DVec2>>,
    pub lo: DVec2,
    pub hi: DVec2,
}

impl Owner<'_> {
    /// Does this unit own the ground at `p`?
    pub fn owns(&self, p: DVec2) -> bool {
        if p.x < self.lo.x || p.y < self.lo.y || p.x > self.hi.x || p.y > self.hi.y {
            return false;
        }
        match &self.district {
            Some(pts) => super::contains(pts, p),
            None => !self.world.in_a_piece(p),
        }
    }
}

/// What planning one unit sees: the town, every road and bridge near it,
/// the ground it owns, and its own buildings.
pub(super) struct Local<'a> {
    pub world: &'a TownWorld,
    pub roads: Vec<WorldRoad>,
    pub bridges: Vec<Bridge>,
    pub owner: Owner<'a>,
    /// A district's streets (none in a verge tile).
    pub streets: Option<Arc<Streets>>,
    pub buildings: Vec<WorldBuilding>,
}

/// Does any part of the road come into the box (with its width)?
fn road_touches(r: &WorldRoad, lo: DVec2, hi: DVec2) -> bool {
    let reach = r.half + r.pavement;
    r.pts.windows(2).any(|s| {
        overlap(
            (
                s[0].min(s[1]) - DVec2::splat(reach),
                s[0].max(s[1]) + DVec2::splat(reach),
            ),
            (lo, hi),
            0.0,
        )
    })
}

/// Does a building's outline come onto any street's carriageway or more
/// than a metre into its pavement?
fn on_a_street(roads: &RoadGrid, site: &BuildingSite) -> bool {
    let c = site.corners();
    (0..4).any(|k| {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        let n = (a.distance(b) / 0.8).ceil().max(1.0) as usize;
        (0..n).any(|j| roads.touches(a.lerp(b, j as f64 / n as f64), -1.0, None))
    })
}

fn road(
    id: DetailId,
    pts: Vec<DVec2>,
    reach: f64,
    class: Option<RoadClass>,
    style: Option<StreetStyle>,
) -> WorldRoad {
    // Pavements: wide on grand roads, a strip in old lanes.
    let pavement = match class {
        Some(RoadClass::Boulevard) => (reach * 0.3).min(5.0),
        Some(RoadClass::Country) => (reach * 0.2).min(1.5),
        _ => (reach * 0.3).clamp(1.2, 4.0),
    };
    WorldRoad {
        id,
        pts,
        half: (reach - pavement).max(2.0),
        pavement,
        main: class.is_some(),
        class,
        style,
        layer: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buildings_stand_on_dry_ground_and_roads_above_water() {
        let tw = TownWorld::generate(Seed(3), TownParams::sample(Seed(3), 20_000.0));
        let w = tw.everything();
        assert!(w.buildings.len() > 500, "{}", w.buildings.len());
        for (_, b) in &w.buildings {
            assert!(b.base >= b.low && b.low > WATER_LEVEL);
            assert!(b.base - b.low < 25.0, "plinth {}", b.base - b.low);
        }
        for r in &w.roads {
            for p in &r.pts {
                assert!(tw.road_height(*p) > WATER_LEVEL);
            }
        }
        let (p, _) = tw.spawn_line();
        assert!(tw.ground(p) > WATER_LEVEL);
    }

    /// The skyline shows the very buildings the quarters hold, ids and all,
    /// and planning it keeps nothing: no quarter, no streets.
    #[test]
    fn skyline_is_the_quarters_buildings_and_keeps_nothing() {
        let tw = TownWorld::generate(Seed(2), TownParams::sample(Seed(2), 20_000.0));
        let core = tw.core();
        let (lo, hi) = (core - DVec2::splat(300.0), core + DVec2::splat(300.0));
        let (quarters, streets) = (tw.quarters.len(), tw.streets.len());
        let far = tw.skyline(lo, hi);
        assert_eq!(tw.quarters.len(), quarters, "the skyline kept quarters");
        assert_eq!(tw.streets.len(), streets, "the skyline kept streets");
        assert!(far.len() > 50, "only {} buildings", far.len());
        let m = DVec2::splat(80.0);
        let mut near: Vec<(DetailId, WorldBuilding)> = tw
            .quarters_in(lo - m, hi + m)
            .iter()
            .flat_map(|q| {
                q.buildings_with_ids()
                    .map(|(i, b)| (i, b.clone()))
                    .collect::<Vec<_>>()
            })
            .filter(|(_, b)| {
                let c = b.site.origin
                    + (b.site.along * b.site.size.x + b.site.inward * b.site.size.y) * 0.5;
                c.x >= lo.x && c.y >= lo.y && c.x < hi.x && c.y < hi.y
            })
            .collect();
        near.sort_by_key(|(i, _)| *i);
        assert_eq!(far, near);
        // Read from the planned quarters, the same again; the roads too.
        let (again, roads) = tw.skyline_with_roads(lo, hi);
        assert_eq!(again, near);
        assert_eq!(roads, tw.roads_in(lo, hi));
    }

    /// A unit comes out the same whichever units were planned before it,
    /// and whatever the cache forgot in between: what streaming relies on.
    #[test]
    fn units_are_the_same_in_any_order() {
        let p = TownParams::sample(Seed(1), 40_000.0);
        let (a, b) = (
            TownWorld::generate(Seed(1), p.clone()),
            TownWorld::generate(Seed(1), p),
        );
        let units = a.all_units();
        assert!(units.len() > 30, "{} units", units.len());
        let first: Vec<_> = units.iter().map(|u| a.quarter(*u)).collect();
        for (u, q) in units.iter().rev().zip(first.iter().rev()) {
            let r = b.quarter(*u);
            assert_eq!(q.buildings, r.buildings, "{u:?}");
            assert_eq!(q.props, r.props, "{u:?}");
            assert_eq!(q.quays, r.quays, "{u:?}");
        }
        // Planning everything keeps only so much at once.
        assert!(a.quarters_kept() <= KEEP_QUARTERS);
    }

    /// No piece of a megacity is too big to plan when someone comes near.
    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn districts_stay_small_in_a_megacity() {
        let w = TownWorld::generate(Seed(1), TownParams::sample(Seed(1), 2_000_000.0));
        let core = w.core();
        let r = DVec2::splat(1500.0);
        let units = w.units_in(core - r, core + r);
        assert!(!units.is_empty());
        for u in units {
            if let Some(s) = w.streets(u) {
                let ext = s.hi - s.lo;
                assert!(
                    ext.max_element() < 2.5 * super::super::PIECE_MAX,
                    "{u:?} {ext:?}"
                );
            }
        }
    }

    /// A walk right across a megacity: what is kept stays bounded however
    /// far one goes, and coming back finds the same buildings and props,
    /// by the same ids, as on the way out.
    #[test]
    #[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_walk_across_a_megacity_keeps_memory_bounded_and_the_world_the_same() {
        let w = TownWorld::generate(Seed(2), TownParams::sample(Seed(2), 2_000_000.0));
        let core = w.core();
        let r = DVec2::splat(500.0);
        let look = |p: DVec2| {
            let mut out: Vec<(u8, DetailId, (i64, i64))> = Vec::new();
            for q in w.quarters_in(p - r, p + r) {
                for (id, b) in q.buildings_with_ids() {
                    let o = b.site.origin;
                    out.push((0, id, ((o.x * 100.0) as i64, (o.y * 100.0) as i64)));
                }
                for (id, pr) in q.props_with_ids() {
                    out.push((
                        1,
                        id,
                        ((pr.pos.x * 100.0) as i64, (pr.pos.y * 100.0) as i64),
                    ));
                }
            }
            out.sort();
            out
        };
        let first = look(core);
        assert!(first.len() > 1000, "{} things round the core", first.len());
        let mut most = 0;
        let span = (w.half() * 0.8).min(9000.0);
        let mut x = -span;
        while x <= span {
            let p = core + DVec2::new(x, x * 0.4);
            let seen = look(p);
            // Ids are unique wherever one stands (buildings and props
            // count apart).
            let mut ids: Vec<(u8, DetailId)> = seen.iter().map(|s| (s.0, s.1)).collect();
            ids.dedup();
            assert_eq!(ids.len(), seen.len(), "an id twice at {p}");
            most = most.max(w.quarters_kept());
            x += 700.0;
        }
        assert!(most <= KEEP_QUARTERS, "{most} quarters kept");
        assert_eq!(look(core), first, "the core changed after the walk");
    }

    /// No building stands on a carriageway, dead ends and other pieces'
    /// streets included (a fifth of them did before 0.065, and cars drove
    /// through them), checked against the drawn width of every road.
    #[test]
    fn no_building_stands_on_a_street() {
        let w = TownWorld::generate(Seed(1), TownParams::sample(Seed(1), 40_000.0)).everything();
        let boxes: Vec<(DVec2, DVec2)> = w
            .roads
            .iter()
            .map(|r| {
                let lo = r.pts.iter().fold(DVec2::MAX, |m, p| m.min(*p));
                let hi = r.pts.iter().fold(DVec2::MIN, |m, p| m.max(*p));
                (lo - DVec2::splat(r.half), hi + DVec2::splat(r.half))
            })
            .collect();
        for (_, b) in &w.buildings {
            let c = b.site.corners();
            for k in 0..4 {
                for j in 0..=10 {
                    let q = c[k].lerp(c[(k + 1) % 4], j as f64 / 10.0);
                    let near = |i: &usize| {
                        let (lo, hi) = boxes[*i];
                        q.x >= lo.x && q.y >= lo.y && q.x <= hi.x && q.y <= hi.y
                    };
                    for r in (0..w.roads.len()).filter(near).map(|i| &w.roads[i]) {
                        let d = r
                            .pts
                            .windows(2)
                            .map(|s| crate::water::segment_distance(q, s[0], s[1]))
                            .fold(f64::MAX, f64::min);
                        assert!(d >= r.half, "a building on a street at {q:?}");
                    }
                }
            }
        }
    }
}
