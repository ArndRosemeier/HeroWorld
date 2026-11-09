//! What heroes bump into: the city's buildings as stacks of boxes (one per
//! run of floors with the same outline, so towers keep their setbacks).
//!
//! Server and client use the same code and the same city, so a predicted
//! hero lands exactly where the server's does.

use glam::{DVec2, DVec3};
use hw_gen::building::BuildingPlan;
use hw_gen::building::BuildingSite;
use hw_gen::city::CityPlan;
use hw_gen::geom::Rect;
use hw_gen::urban::footprint::Footprint;
use hw_gen::urban::greenery::Greenery;
use hw_gen::urban::quarter::{DetailId, Quarter, UnitKind, PIECE_ROADS};
use hw_gen::urban::underground::Underground;
use hw_gen::urban::world::{TownWorld, WATER_LEVEL};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

/// Ground grid cell for lookups (m).
const CELL: f64 = 32.0;
/// A town's solids are loaded in square tiles of this side (m), where
/// something comes near, and forgotten when nothing has for a while.
const TILE: f64 = 128.0;
/// How many tiles stay loaded (about four square kilometres).
pub const KEEP_TILES: usize = 256;

/// Names a collider: a town building by its [`DetailId`], a town prop by its
/// id with [`PROP_BIT`], a side street's bridge with [`BRIDGE_BIT`], and
/// everything else (fixed colliders) by [`FIXED`] and its number.
pub type ColliderId = u64;
/// Set in the ids of props' colliders.
pub const PROP_BIT: u64 = 1 << 31;
/// Set in the ids of bridge decks of side streets and district roads.
pub const BRIDGE_BIT: u64 = 1 << 30;
/// The ids of fixed colliders: this, plus their number.
pub const FIXED: u64 = 0xFFFF_FFFE << 32;

/// The collider of a town prop.
pub fn prop_collider(prop: DetailId) -> ColliderId {
    prop.0 | PROP_BIT
}
/// A moving solid reaches at most this far from its origin (m).
const MOVER_REACH: f64 = 3.5;

/// One solid box: a rectangle in a building's frame, from `y0` to `y1`.
#[derive(Clone, Debug, PartialEq)]
pub struct Solid {
    pub origin: DVec2,
    pub along: DVec2,
    pub inward: DVec2,
    /// Rectangle in building coordinates (x along, z inward).
    pub min: DVec2,
    pub max: DVec2,
    pub y0: f64,
    pub y1: f64,
}

impl Solid {
    fn to_local(&self, p: DVec2) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.along), d.dot(self.inward))
    }

    pub fn to_world(&self, l: DVec2) -> DVec2 {
        self.origin + self.along * l.x + self.inward * l.y
    }

    fn corners(&self) -> [DVec2; 4] {
        [
            self.to_world(self.min),
            self.to_world(DVec2::new(self.max.x, self.min.y)),
            self.to_world(self.max),
            self.to_world(DVec2::new(self.min.x, self.max.y)),
        ]
    }
}

/// One building for collision: its plan, and its solids made on first use
/// (walls with door gaps, floor slabs, terraces, the roof). Only buildings
/// something comes near ever get built, on the server and every client alike.
#[derive(Debug)]
struct Collider {
    /// The building's plan, made from its site on first use.
    plan: OnceLock<Option<Arc<BuildingPlan>>>,
    site: Option<BuildingSite>,
    base: f64,
    /// Bottom of the plinth under a building on a slope.
    plinth: Option<f64>,
    lo: DVec2,
    hi: DVec2,
    solids: OnceLock<Arc<[Solid]>>,
    /// A damaged building's solids instead of the plan's (see
    /// [`crate::destruction`]).
    damage: RwLock<Option<Damaged>>,
    /// Goes up whenever the damage changes, so copies (physics) know to rebuild.
    version: AtomicU64,
}

/// What is left of a damaged building, for collision.
#[derive(Clone, Debug)]
struct Damaged {
    solids: Arc<[Solid]>,
    /// The lift still works (the building has not collapsed).
    lift: bool,
}

/// The solids of one collider: its own, or a damaged building's shared copy.
pub enum SolidSet<'a> {
    Plain(&'a [Solid]),
    Shared(Arc<[Solid]>),
}

impl std::ops::Deref for SolidSet<'_> {
    type Target = [Solid];
    fn deref(&self) -> &[Solid] {
        match self {
            SolidSet::Plain(s) => s,
            SolidSet::Shared(s) => s,
        }
    }
}

impl Collider {
    fn new(site: Option<BuildingSite>, base: f64, lo: DVec2, hi: DVec2) -> Self {
        Self {
            plan: OnceLock::new(),
            site,
            base,
            plinth: None,
            lo,
            hi,
            solids: OnceLock::new(),
            damage: RwLock::new(None),
            version: AtomicU64::new(0),
        }
    }

    fn set(&self) -> SolidSet<'static> {
        match self.damage.read().expect("damage lock").as_ref() {
            Some(d) => SolidSet::Shared(d.solids.clone()),
            None => SolidSet::Shared(self.solids().clone()),
        }
    }

    fn plinth_solid(&self) -> Option<Solid> {
        let y0 = self.plinth?;
        let s = &self.plan()?.site;
        Some(Solid {
            origin: s.origin,
            along: s.along,
            inward: s.inward,
            min: DVec2::ZERO,
            max: s.size,
            y0,
            y1: self.base,
        })
    }

    fn plan(&self) -> Option<&Arc<BuildingPlan>> {
        self.plan
            .get_or_init(|| {
                self.site
                    .as_ref()
                    .map(|s| Arc::new(BuildingPlan::generate(s)))
            })
            .as_ref()
    }

    fn solids(&self) -> &Arc<[Solid]> {
        self.solids.get_or_init(|| match self.plan() {
            Some(p) => {
                let mut out = building_solids(p, self.base);
                out.extend(self.plinth_solid());
                out.into()
            }
            None => Vec::new().into(),
        })
    }
}

/// Ground height by position, for worlds with terrain.
#[derive(Clone)]
pub struct Terrain(pub Arc<dyn Fn(DVec2) -> f64 + Send + Sync>);

impl std::fmt::Debug for Terrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Terrain")
    }
}

/// All solids of a city with a grid to find nearby ones fast.
#[derive(Debug, Default)]
pub struct Obstacles {
    colliders: Vec<Collider>,
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    /// Height of open ground.
    pub ground: f64,
    /// Rivers and sea: open ground there is the water surface.
    water: Option<Arc<WaterMap>>,
    /// Hills and valleys (open ground is then the terrain, or the water
    /// surface at `ground` where the terrain dips below it).
    terrain: Option<Terrain>,
    /// Tunnels, lift shafts and caverns below the ground.
    under: Option<Arc<Underground>>,
    /// A town's buildings, props and bridges, loaded tile by tile.
    town: Option<TownSolids>,
    /// Things that move: the street cars near the heroes, set each tick
    /// (see [`Self::set_movers`]).
    movers: RwLock<Arc<[Solid]>>,
    /// Landmarks, whose paved plinths are raised ground.
    plinths: Vec<hw_gen::urban::landmarks::Landmark>,
    /// The country's woods, made cell by cell where bodies go.
    woods: Option<Woods>,
}

/// The trunks of each wood cell made so far.
type WoodCells = BTreeMap<(i64, i64), Arc<[Solid]>>;

/// Tree trunks of the open country ([`Greenery::wild_trees`]), worked out
/// for a cell the first time something moves there: nothing is made for
/// land nobody visits.
struct Woods {
    town: Arc<TownWorld>,
    green: Greenery,
    cells: RwLock<WoodCells>,
}

impl std::fmt::Debug for Woods {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Woods").finish_non_exhaustive()
    }
}

/// Edge of a cell of woods (m).
const WOOD_CELL: f64 = 64.0;

impl Woods {
    fn cell(&self, k: (i64, i64)) -> Arc<[Solid]> {
        if let Some(c) = self.cells.read().expect("woods lock").get(&k) {
            return c.clone();
        }
        let lo = DVec2::new(k.0 as f64, k.1 as f64) * WOOD_CELL;
        let solids: Arc<[Solid]> = self
            .green
            .wild_trees(&self.town, lo, lo + DVec2::splat(WOOD_CELL))
            .iter()
            .flat_map(|t| t.footprints())
            .flat_map(|f| solids_of(&f))
            .collect();
        let mut cells = self.cells.write().expect("woods lock");
        // Cheap to make again: forget them all now and then, so travelling
        // far does not pile them up.
        if cells.len() > 1024 {
            cells.clear();
        }
        cells.insert(k, solids.clone());
        solids
    }

    fn near(&self, p: DVec2, r: f64) -> Vec<Arc<[Solid]>> {
        let (x0, z0) = (
            ((p.x - r) / WOOD_CELL).floor() as i64,
            ((p.y - r) / WOOD_CELL).floor() as i64,
        );
        let (x1, z1) = (
            ((p.x + r) / WOOD_CELL).floor() as i64,
            ((p.y + r) / WOOD_CELL).floor() as i64,
        );
        let mut out = Vec::new();
        for x in x0..=x1 {
            for z in z0..=z1 {
                let c = self.cell((x, z));
                if !c.is_empty() {
                    out.push(c);
                }
            }
        }
        out
    }
}

/// A town's solids, loaded where needed. What changed (damaged buildings,
/// broken props) is kept by id, so it outlives its tile.
struct TownSolids {
    world: Arc<TownWorld>,
    tiles: RwLock<BTreeMap<(i64, i64), Arc<Tile>>>,
    /// Colliders alive in some tile, by id (a building on two tiles is one
    /// collider).
    live: Mutex<BTreeMap<ColliderId, Weak<Collider>>>,
    /// Damage and its version, by id.
    changed: RwLock<BTreeMap<ColliderId, (Option<Damaged>, u64)>>,
    tick: AtomicU64,
}

impl std::fmt::Debug for TownSolids {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TownSolids")
    }
}

/// One loaded tile: its colliders and a grid over them.
struct Tile {
    colliders: Vec<(ColliderId, Arc<Collider>)>,
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    used: AtomicU64,
}

fn tile_of(p: DVec2) -> (i64, i64) {
    ((p.x / TILE).floor() as i64, (p.y / TILE).floor() as i64)
}

impl TownSolids {
    /// The tile at `k`, loading it if needed.
    fn tile(&self, k: (i64, i64)) -> Arc<Tile> {
        let t = self.tick.fetch_add(1, Ordering::Relaxed);
        if let Some(tile) = self.tiles.read().expect("tiles lock").get(&k) {
            tile.used.store(t, Ordering::Relaxed);
            return tile.clone();
        }
        let tile = Arc::new(self.load(k, t));
        let mut tiles = self.tiles.write().expect("tiles lock");
        let tile = tiles.entry(k).or_insert(tile).clone();
        if tiles.len() > KEEP_TILES {
            let mut by_use: Vec<((i64, i64), u64)> = tiles
                .iter()
                .map(|(k, v)| (*k, v.used.load(Ordering::Relaxed)))
                .collect();
            by_use.sort_by_key(|e| e.1);
            for (old, _) in by_use.iter().take(tiles.len() - KEEP_TILES * 3 / 4) {
                if *old != k {
                    tiles.remove(old);
                }
            }
            drop(tiles);
            self.live
                .lock()
                .expect("live lock")
                .retain(|_, w| w.strong_count() > 0);
        }
        tile
    }

    /// The collider `id`, alive already or made now.
    fn collider(&self, id: ColliderId, make: impl FnOnce() -> Collider) -> Arc<Collider> {
        let mut live = self.live.lock().expect("live lock");
        if let Some(c) = live.get(&id).and_then(|w| w.upgrade()) {
            return c;
        }
        let c = make();
        if let Some((d, v)) = self.changed.read().expect("changed lock").get(&id) {
            *c.damage.write().expect("damage lock") = d.clone();
            c.version.store(*v, Ordering::Relaxed);
        }
        let c = Arc::new(c);
        live.insert(id, Arc::downgrade(&c));
        c
    }

    fn load(&self, k: (i64, i64), t: u64) -> Tile {
        let lo = DVec2::new(k.0 as f64, k.1 as f64) * TILE;
        let hi = lo + DVec2::splat(TILE);
        let inside = |a: DVec2, b: DVec2| a.x <= hi.x && b.x >= lo.x && a.y <= hi.y && b.y >= lo.y;
        let mut colliders: Vec<(ColliderId, Arc<Collider>)> = Vec::new();
        let quarters: Vec<Arc<Quarter>> = self.world.quarters_in(lo, hi);
        for q in &quarters {
            for (id, b) in q.buildings_with_ids() {
                let corners = b.site.corners();
                let (a, z) = (lo_of(&corners), hi_of(&corners));
                if !inside(a, z) {
                    continue;
                }
                colliders.push((
                    id.0,
                    self.collider(id.0, || {
                        let mut c = Collider::new(Some(b.site.clone()), b.base, a, z);
                        c.plinth = (b.base - b.low > 0.05).then_some(b.low - 1.0);
                        c
                    }),
                ));
            }
            // Street props, one collider each while it stands: lamp posts,
            // tree trunks, benches, kiosks, statues, fountains, parked cars,
            // barricades.
            for (id, p) in q.props_with_ids() {
                let solids: Vec<Solid> = p.footprints().iter().flat_map(solids_of).collect();
                if solids.is_empty() {
                    continue;
                }
                let c = fixed_collider(solids);
                if !inside(c.lo, c.hi) {
                    continue;
                }
                let id = prop_collider(id);
                colliders.push((id, self.collider(id, || c)));
            }
            // Bridge decks of the side streets and district roads.
            let mut decks: Vec<(ColliderId, &hw_gen::urban::world::Bridge)> = q
                .bridges()
                .iter()
                .enumerate()
                .map(|(n, b)| (DetailId::new(q.unit, n).0 | BRIDGE_BIT, b))
                .collect();
            let split;
            if let UnitKind::District { piece, .. } = q.unit.kind() {
                split = self.world.split(piece);
                let u = hw_gen::urban::quarter::Unit::district(piece, PIECE_ROADS as usize);
                decks.extend(
                    split
                        .bridges
                        .iter()
                        .enumerate()
                        .map(|(n, b)| (DetailId::new(u, n).0 | BRIDGE_BIT, b)),
                );
            }
            for (id, br) in decks {
                if let Some(s) = deck(br) {
                    let c = fixed_collider(vec![s]);
                    if inside(c.lo, c.hi) {
                        colliders.push((id, self.collider(id, || c)));
                    }
                }
            }
        }
        colliders.sort_by_key(|c| c.0);
        colliders.dedup_by_key(|c| c.0);
        let mut grid: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
        for (i, (_, c)) in colliders.iter().enumerate() {
            for key in cells_of(c.lo.max(lo), c.hi.min(hi)) {
                grid.entry(key).or_default().push(i as u32);
            }
        }
        Tile {
            colliders,
            grid,
            used: AtomicU64::new(t),
        }
    }
}

/// The grid cells a box covers.
fn cells_of(lo: DVec2, hi: DVec2) -> impl Iterator<Item = (i64, i64)> {
    let (x0, z0) = ((lo.x / CELL).floor() as i64, (lo.y / CELL).floor() as i64);
    let (x1, z1) = ((hi.x / CELL).floor() as i64, (hi.y / CELL).floor() as i64);
    (x0..=x1).flat_map(move |x| (z0..=z1).map(move |z| (x, z)))
}

/// A collider of fixed boxes.
fn fixed_collider(solids: Vec<Solid>) -> Collider {
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for c in solids.iter().flat_map(|s| s.corners()) {
        lo = lo.min(c);
        hi = hi.max(c);
    }
    let mut c = Collider::new(None, 0.0, lo, hi);
    c.plan = OnceLock::from(None);
    c.solids = OnceLock::from(Arc::from(solids));
    c
}

/// A bridge deck's box.
fn deck(br: &hw_gen::urban::world::Bridge) -> Option<Solid> {
    let d = br.b - br.a;
    let len = d.length();
    if len < 1e-6 {
        return None;
    }
    let along = d / len;
    let inward = along.perp();
    Some(Solid {
        origin: br.a - inward * br.half,
        along,
        inward,
        min: DVec2::ZERO,
        max: DVec2::new(len, br.half * 2.0),
        y0: br.deck - 0.6,
        y1: br.deck,
    })
}

/// A collider found near a point: one of the fixed ones, or a town's.
enum Found<'a> {
    Fixed(&'a Collider),
    Town(Arc<Collider>),
}

impl std::ops::Deref for Found<'_> {
    type Target = Collider;
    fn deref(&self) -> &Collider {
        match self {
            Found::Fixed(c) => c,
            Found::Town(c) => c,
        }
    }
}

/// Where the open ground is water, and the bridges over it.
#[derive(Debug)]
struct WaterMap {
    water: hw_gen::water::CityWater,
    /// Deck centre line ends and half width.
    bridges: Vec<(DVec2, DVec2, f64)>,
}

impl WaterMap {
    fn is_water(&self, p: DVec2) -> bool {
        self.water.is_water(p)
            && !self
                .bridges
                .iter()
                .any(|(a, b, w)| hw_gen::water::segment_distance(p, *a, *b) <= *w)
    }
}

/// A body that moves through the city: feet position, velocity.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Body {
    pub pos: DVec3,
    pub vel: DVec3,
    pub grounded: bool,
}

/// Radius and height of a person.
pub const BODY_RADIUS: f64 = 0.4;
pub const BODY_HEIGHT: f64 = 1.8;
/// Ledges lower than this are stepped onto.
pub const STEP: f64 = 0.45;
/// Thickness of floor slabs, outside walls and inside walls.
pub const SLAB: f64 = 0.3;
pub const OUTER_WALL: f64 = 0.3;
pub const INNER_WALL: f64 = 0.2;

/// Where a point is inside a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inside {
    /// The building's collider id (a town building's [`DetailId`]).
    pub building: ColliderId,
    pub level: u32,
}

impl Obstacles {
    /// Open ground only.
    pub fn flat(ground: f64) -> Self {
        Self {
            ground,
            ..Default::default()
        }
    }

    /// Every building of a city (one plan per `city.buildings`, same order),
    /// standing on `base`.
    pub fn from_city(city: &CityPlan, plans: &[BuildingPlan], ground: f64, base: f64) -> Self {
        let mut out = Self::flat(ground);
        out.water = Some(Arc::new(WaterMap {
            water: city.water.clone(),
            bridges: city
                .bridges
                .iter()
                .map(|b| (b.a, b.b, city.streets[b.street as usize].reach()))
                .collect(),
        }));
        for (_, p) in city.buildings.iter().zip(plans) {
            let corners = p.site.corners();
            let mut c = Collider::new(None, base, lo_of(&corners), hi_of(&corners));
            c.plan = OnceLock::from(Some(Arc::new(p.clone())));
            out.insert(c);
        }
        out
    }

    /// A town on its terrain: open ground follows `terrain` (water stands at
    /// `water`), each building stands at its own base on a plinth, bridge
    /// decks span the water. The town's buildings, props and side bridges
    /// are loaded tile by tile where something comes near, its country's
    /// woods cell by cell ([`Woods`]); building plans are made when first
    /// needed.
    pub fn from_town(world: &Arc<TownWorld>, terrain: Terrain) -> Self {
        let mut out = Self::flat(WATER_LEVEL);
        out.woods = Some(Woods {
            town: world.clone(),
            green: Greenery::new(world),
            cells: RwLock::new(BTreeMap::new()),
        });
        out.terrain = Some(terrain);
        out.under = Some(Arc::new(world.underground().clone()));
        for br in &world.main_bridges {
            if let Some(s) = deck(br) {
                out.add(s);
            }
        }
        // Landmarks: the plinth is raised ground; the tower, its base, the
        // statue and the stairs are solid.
        for l in &world.landmarks {
            out.plinths.push(l.clone());
            let mut parts = l.footprints();
            parts.extend(l.stairs.iter().flat_map(|s| s.treads()));
            out.add_all(parts.iter().flat_map(solids_of).collect());
        }
        out.town = Some(TownSolids {
            world: world.clone(),
            tiles: RwLock::new(BTreeMap::new()),
            live: Mutex::new(BTreeMap::new()),
            changed: RwLock::new(BTreeMap::new()),
            tick: AtomicU64::new(0),
        });
        out
    }

    /// A blocking street prop broken (its box gone) or whole again.
    /// Has the town plan its detail within `r` of each point in the
    /// background (nothing for a city or flat ground).
    pub fn prefetch(&self, points: &[DVec3], r: f64) {
        if let Some(t) = &self.town {
            let at: Vec<DVec2> = points.iter().map(|p| DVec2::new(p.x, p.z)).collect();
            t.world.prefetch(&at, r);
        }
    }

    /// What the town's collision holds now: (tiles, live colliders,
    /// changed buildings); for the memory tests.
    pub fn kept(&self) -> (usize, usize, usize) {
        self.town.as_ref().map_or((0, 0, 0), |t| {
            (
                t.tiles.read().expect("tiles lock").len(),
                t.live.lock().expect("live lock").len(),
                t.changed.read().expect("changed lock").len(),
            )
        })
    }

    pub fn set_prop_broken(&self, prop: DetailId, broken: bool) {
        self.set_damage(prop_collider(prop), broken.then(Vec::new), true);
    }

    /// How many town tiles are loaded now.
    pub fn tiles_loaded(&self) -> usize {
        self.town
            .as_ref()
            .map_or(0, |t| t.tiles.read().expect("tiles lock").len())
    }

    /// Adds a free-standing box (tests, props).
    pub fn add(&mut self, solid: Solid) {
        self.add_all(vec![solid]);
    }

    /// Adds boxes that stand and fall together as one collider.
    pub fn add_all(&mut self, solids: Vec<Solid>) {
        let c = fixed_collider(solids);
        self.insert(c);
    }

    fn insert(&mut self, c: Collider) {
        let idx = self.colliders.len() as u32;
        let (x0, z0) = (
            (c.lo.x / CELL).floor() as i64,
            (c.lo.y / CELL).floor() as i64,
        );
        let (x1, z1) = (
            (c.hi.x / CELL).floor() as i64,
            (c.hi.y / CELL).floor() as i64,
        );
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.grid.entry((x, z)).or_default().push(idx);
            }
        }
        self.colliders.push(c);
    }

    /// Colliders whose boxes come within `r` of `p`, by id (sorted).
    fn near_colliders(&self, p: DVec2, r: f64) -> Vec<(ColliderId, Found<'_>)> {
        let mut out = Vec::new();
        let close = |c: &Collider| {
            p.x + r >= c.lo.x && p.x - r <= c.hi.x && p.y + r >= c.lo.y && p.y - r <= c.hi.y
        };
        for key in cells_of(p - DVec2::splat(r), p + DVec2::splat(r)) {
            if let Some(v) = self.grid.get(&key) {
                out.extend(
                    v.iter()
                        .filter(|&&i| close(&self.colliders[i as usize]))
                        .map(|&i| (FIXED | i as u64, Found::Fixed(&self.colliders[i as usize]))),
                );
            }
        }
        if let Some(town) = &self.town {
            let (a, b) = (tile_of(p - DVec2::splat(r)), tile_of(p + DVec2::splat(r)));
            for tx in a.0..=b.0 {
                for tz in a.1..=b.1 {
                    let tile = town.tile((tx, tz));
                    for key in cells_of(p - DVec2::splat(r), p + DVec2::splat(r)) {
                        if let Some(v) = tile.grid.get(&key) {
                            for &i in v {
                                let (id, c) = &tile.colliders[i as usize];
                                if close(c) {
                                    out.push((*id, Found::Town(c.clone())));
                                }
                            }
                        }
                    }
                }
            }
        }
        out.sort_unstable_by_key(|e| e.0);
        out.dedup_by_key(|e| e.0);
        out
    }

    /// The solids near a point, a set per collider (builds colliders on
    /// first touch).
    fn near(&self, p: DVec2, r: f64) -> Vec<SolidSet<'_>> {
        let mut sets: Vec<SolidSet<'_>> = self
            .near_colliders(p, r)
            .into_iter()
            .map(|(_, c)| c.set())
            .collect();
        let movers = self.movers.read().expect("movers lock").clone();
        if movers
            .iter()
            .any(|m| m.origin.distance(p) < r + MOVER_REACH)
        {
            sets.push(SolidSet::Shared(movers));
        }
        if let Some(w) = &self.woods {
            sets.extend(w.near(p, r).into_iter().map(SolidSet::Shared));
        }
        sets
    }

    /// The moving solids now (street cars near the heroes): heroes bump
    /// into them and can stand on them. Replaces the last set.
    pub fn set_movers(&self, solids: Vec<Solid>) {
        *self.movers.write().expect("movers lock") = solids.into();
    }

    /// The solids of every collider within `r` of `p`, by collider id
    /// (building colliders are made on first touch). For the physics world.
    pub fn solids_near(&self, p: DVec2, r: f64) -> Vec<(ColliderId, SolidSet<'_>)> {
        self.near_colliders(p, r)
            .into_iter()
            .map(|(i, c)| (i, c.set()))
            .collect()
    }

    /// Buildings (by collider id, as [`Inside::building`] names them) whose
    /// ground boxes come within `r` of `p`.
    pub fn buildings_near(&self, p: DVec2, r: f64) -> Vec<ColliderId> {
        self.near_colliders(p, r)
            .into_iter()
            .filter(|(_, c)| c.site.is_some() || c.plan.get().is_some_and(|p| p.is_some()))
            .map(|(i, _)| i)
            .collect()
    }

    /// A fixed collider by id.
    fn fixed(&self, i: ColliderId) -> Option<&Collider> {
        if i & FIXED != FIXED {
            return None;
        }
        self.colliders.get((i & 0xFFFF_FFFF) as usize)
    }

    /// The collider `i`: a fixed one, a loaded town one, or (for a town
    /// building nobody is near) one made for the asking.
    fn find(&self, i: ColliderId) -> Option<Found<'_>> {
        if let Some(c) = self.fixed(i) {
            return Some(Found::Fixed(c));
        }
        let town = self.town.as_ref()?;
        if let Some(c) = town
            .live
            .lock()
            .expect("live lock")
            .get(&i)
            .and_then(|w| w.upgrade())
        {
            return Some(Found::Town(c));
        }
        if i & (PROP_BIT | BRIDGE_BIT) != 0 {
            return None;
        }
        let b = town.world.building(DetailId(i))?;
        let corners = b.site.corners();
        let mut c = Collider::new(
            Some(b.site.clone()),
            b.base,
            lo_of(&corners),
            hi_of(&corners),
        );
        c.plinth = (b.base - b.low > 0.05).then_some(b.low - 1.0);
        Some(Found::Town(town.collider(i, || c)))
    }

    /// Replaces building `i`'s solids with what is left of it after damage
    /// (`None`: whole again). `lift`: its lift still works. Its plinth stays.
    pub fn set_damage(&self, i: ColliderId, solids: Option<Vec<Solid>>, lift: bool) {
        let c = self.find(i);
        let d = solids.map(|mut s| {
            if let Some(c) = &c {
                s.extend(c.plinth_solid());
            }
            Damaged {
                solids: s.into(),
                lift,
            }
        });
        if let Some(town) = self.town.as_ref().filter(|_| i & FIXED != FIXED) {
            let mut changed = town.changed.write().expect("changed lock");
            if d.is_none() {
                // Whole again: nothing to remember (a version going back to
                // 0 still reads as a change to whoever saw it damaged).
                changed.remove(&i);
            } else {
                // Versions come from one counter, so a building damaged,
                // repaired and damaged again never repeats one.
                let v = town.tick.fetch_add(1, Ordering::Relaxed) + 1;
                changed.insert(i, (d.clone(), v));
            }
        }
        if let Some(c) = c {
            *c.damage.write().expect("damage lock") = d;
            c.version.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Is building `i` damaged?
    pub fn is_damaged(&self, i: ColliderId) -> bool {
        if let Some(c) = self.fixed(i) {
            return c.damage.read().expect("damage lock").is_some();
        }
        self.town.as_ref().is_some_and(|t| {
            t.changed
                .read()
                .expect("changed lock")
                .get(&i)
                .is_some_and(|e| e.0.is_some())
        })
    }

    /// Changes whenever collider `i`'s solids change.
    pub fn version(&self, i: ColliderId) -> u64 {
        if let Some(c) = self.fixed(i) {
            return c.version.load(Ordering::Relaxed);
        }
        self.town.as_ref().map_or(0, |t| {
            t.changed
                .read()
                .expect("changed lock")
                .get(&i)
                .map_or(0, |e| e.1)
        })
    }

    /// Does open ground follow terrain or water (not one flat height)?
    pub fn has_relief(&self) -> bool {
        self.terrain.is_some() || self.water.is_some()
    }

    /// Is `p` inside any solid?
    pub fn solid_at(&self, p: DVec3) -> bool {
        // Rock round the underground's open spaces.
        if let Some(u) = &self.under {
            let q = DVec2::new(p.x, p.z);
            if p.y < self.ground_at(q) - 0.2 && !u.near(q, 4.0).is_empty() && !u.open_at(p) {
                return true;
            }
        }
        let sets = self.near(DVec2::new(p.x, p.z), 0.0);
        let found = sets.iter().flat_map(|x| x.iter()).any(|s| {
            let l = s.to_local(DVec2::new(p.x, p.z));
            p.y > s.y0
                && p.y < s.y1
                && l.x > s.min.x
                && l.x < s.max.x
                && l.y > s.min.y
                && l.y < s.max.y
        });
        found
    }

    /// The building and floor a point is in, if any.
    pub fn inside(&self, p: DVec3) -> Option<Inside> {
        let q = DVec2::new(p.x, p.z);
        for (i, c) in self.near_colliders(q, 0.0) {
            let Some(plan) = c.plan() else { continue };
            let s = &plan.site;
            let d = q - s.origin;
            let l = DVec2::new(d.dot(s.along), d.dot(s.inward));
            if l.x < 0.0 || l.y < 0.0 || l.x > s.size.x || l.y > s.size.y {
                continue;
            }
            let y = p.y - c.base;
            if let Some(f) = plan
                .floors
                .iter()
                .find(|f| y >= f.elevation - 0.05 && y < f.elevation + f.height)
            {
                if f.outline.min.x <= l.x
                    && l.x <= f.outline.max.x
                    && f.outline.min.y <= l.y
                    && l.y <= f.outline.max.y
                {
                    return Some(Inside {
                        building: i,
                        level: f.level,
                    });
                }
            }
        }
        None
    }

    /// The plan of building `i` (as [`Inside::building`] counts), made on
    /// first use, and the height it stands at.
    pub fn building(&self, i: ColliderId) -> Option<(Arc<BuildingPlan>, f64)> {
        let c = self.find(i)?;
        c.plan().map(|p| (p.clone(), c.base))
    }

    /// Is `p` (feet) inside a lift shaft?
    pub fn in_lift(&self, p: DVec3) -> bool {
        let q = DVec2::new(p.x, p.z);
        if let Some(u) = &self.under {
            if u.in_shaft(p, self.ground_at(q)) {
                return true;
            }
        }
        self.near_colliders(q, 0.0).into_iter().any(|(_, c)| {
            let Some(plan) = c.plan() else { return false };
            let Some(core) = plan.core else { return false };
            if c.damage
                .read()
                .expect("damage lock")
                .as_ref()
                .is_some_and(|d| !d.lift)
            {
                return false;
            }
            let s = &plan.site;
            let d = q - s.origin;
            let l = DVec2::new(d.dot(s.along), d.dot(s.inward));
            let top = c.base + plan.height();
            l.x > core.min.x
                && l.x < core.max.x
                && l.y > core.min.y
                && l.y < core.max.y
                && p.y >= c.base - 0.1
                && p.y < top
        })
    }

    /// Height of the open ground at `p`: the water surface over rivers and
    /// the sea (bridges excepted).
    pub fn ground_at(&self, p: DVec2) -> f64 {
        if let Some(top) = self.plinths.iter().find_map(|l| l.on_plinth(p)) {
            return top;
        }
        if let Some(t) = &self.terrain {
            return (t.0)(p).max(self.ground);
        }
        match &self.water {
            Some(w) if w.is_water(p) => hw_gen::water::WATER_LEVEL,
            _ => self.ground,
        }
    }

    /// Height of the open ground for something at height `y` over `p`:
    /// a tunnel's or cavern's floor when it is down in one, else the
    /// ground (or water) surface.
    pub fn ground_for(&self, p: DVec2, y: f64) -> f64 {
        self.under
            .as_ref()
            .and_then(|u| u.floor_for(p, y))
            .unwrap_or_else(|| self.ground_at(p))
    }

    /// The underground, if the town has one.
    pub fn underground(&self) -> Option<&Underground> {
        self.under.as_deref()
    }

    /// Height of whatever is under `p` (x, z) no higher than `max_y`.
    pub fn floor_under(&self, p: DVec2, max_y: f64) -> f64 {
        let mut best = self.ground_for(p, max_y - STEP);
        for s in self.near(p, 0.0).iter().flat_map(|x| x.iter()) {
            let l = s.to_local(p);
            if l.x >= s.min.x && l.x <= s.max.x && l.y >= s.min.y && l.y <= s.max.y && s.y1 <= max_y
            {
                best = best.max(s.y1);
            }
        }
        best
    }

    /// Pushes a body standing at `pos` out of every wall it overlaps; returns
    /// the corrected position and the outward normals of walls it touched.
    fn push_out(&self, mut pos: DVec3) -> (DVec3, Vec<DVec2>) {
        let mut normals = Vec::new();
        // A few passes settle corners between neighbouring boxes.
        for _ in 0..3 {
            let mut moved = false;
            let sets = self.near(DVec2::new(pos.x, pos.z), BODY_RADIUS);
            for s in sets.iter().flat_map(|x| x.iter()) {
                // Only walls: the body overlaps the box vertically, is not
                // standing on top of it (or stepping up onto it), and it is not a
                // ceiling just above the head (handled in move_body).
                if pos.y + BODY_HEIGHT <= s.y0
                    || pos.y >= s.y1 - STEP
                    || s.y0 > pos.y + BODY_HEIGHT - 0.6
                {
                    continue;
                }
                let l = s.to_local(DVec2::new(pos.x, pos.z));
                let closest = l.clamp(s.min, s.max);
                let d = l - closest;
                let dist = d.length();
                let push_local = if dist > 1e-9 {
                    if dist >= BODY_RADIUS {
                        continue;
                    }
                    d / dist * (BODY_RADIUS - dist)
                } else {
                    // Centre inside the box: leave by the nearest side.
                    let exits = [
                        (l.x - s.min.x, DVec2::new(-1.0, 0.0)),
                        (s.max.x - l.x, DVec2::new(1.0, 0.0)),
                        (l.y - s.min.y, DVec2::new(0.0, -1.0)),
                        (s.max.y - l.y, DVec2::new(0.0, 1.0)),
                    ];
                    let (depth, dir) = exits
                        .iter()
                        .copied()
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .expect("four sides");
                    dir * (depth + BODY_RADIUS)
                };
                let push = s.along * push_local.x + s.inward * push_local.y;
                pos.x += push.x;
                pos.z += push.y;
                normals.push(push.normalize_or_zero());
                moved = true;
            }
            if !moved {
                break;
            }
        }
        (pos, normals)
    }

    /// Moves a body by its velocity for `dt` seconds: walls stop it, roofs
    /// and ledges catch it, gravity is the caller's business.
    pub fn move_body(&self, body: &mut Body, dt: f64) {
        // Sub-steps keep fast bodies from passing through thin walls.
        let travel = body.vel.length() * dt;
        let steps = ((travel / (BODY_RADIUS * 0.8)).ceil() as usize).clamp(1, 32);
        let h = dt / steps as f64;
        body.grounded = false;
        for _ in 0..steps {
            let mut next = body.pos + body.vel * h;
            let (pushed, normals) = self.push_out(next);
            next = pushed;
            for n in normals {
                let n3 = DVec3::new(n.x, 0.0, n.y);
                let into = body.vel.dot(n3);
                if into < 0.0 {
                    body.vel -= n3 * into;
                }
            }
            // Quay walls: the bank stands too high to step onto from the water.
            let q = DVec2::new(next.x, next.z);
            if self.ground_for(q, body.pos.y.max(next.y)) > body.pos.y.max(next.y) + STEP {
                next.x = body.pos.x;
                next.z = body.pos.z;
                body.vel.x = 0.0;
                body.vel.z = 0.0;
            }
            // Ceilings stop a rising body.
            if body.vel.y > 0.0 {
                let head = next.y + BODY_HEIGHT;
                let q = DVec2::new(next.x, next.z);
                for s in self.near(q, 0.0).iter().flat_map(|x| x.iter()) {
                    let l = s.to_local(q);
                    let over = l.x >= s.min.x && l.x <= s.max.x && l.y >= s.min.y && l.y <= s.max.y;
                    if over && s.y0 >= body.pos.y + BODY_HEIGHT - 0.6 && head > s.y0 {
                        next.y = s.y0 - BODY_HEIGHT;
                        body.vel.y = 0.0;
                    }
                }
                // And so do tunnel and cavern roofs.
                if let Some(c) = self
                    .under
                    .as_ref()
                    .and_then(|u| u.ceiling_for(q, body.pos.y))
                {
                    if head > c {
                        next.y = (c - BODY_HEIGHT).max(body.pos.y);
                        body.vel.y = 0.0;
                    }
                }
            }
            // Land on whatever is below, stepping up small ledges.
            let floor = self.floor_under(DVec2::new(next.x, next.z), next.y.max(body.pos.y) + STEP);
            if next.y <= floor {
                next.y = floor;
                body.vel.y = body.vel.y.max(0.0);
                body.grounded = true;
            }
            body.pos = next;
        }
    }
}

/// The boxes for one solid part: a box as it is, a disc as a few
/// rectangles inside its circle (their union reaches 98% of the radius
/// every way round).
pub fn solids_of(f: &Footprint) -> Vec<Solid> {
    match *f {
        Footprint::Box {
            at,
            dir,
            half,
            y0,
            y1,
        } => {
            let inward = dir;
            let along = DVec2::new(dir.y, -dir.x);
            vec![Solid {
                origin: at - along * half.x - inward * half.y,
                along,
                inward,
                min: DVec2::ZERO,
                max: half * 2.0,
                y0,
                y1,
            }]
        }
        Footprint::Disc { at, r, y0, y1 } => (0..4)
            .map(|k| {
                let a = (k as f64 + 0.5) * std::f64::consts::FRAC_PI_8;
                let (hx, hz) = (r * hw_core::det::cos(a), r * hw_core::det::sin(a));
                Solid {
                    origin: at - DVec2::new(hx, hz),
                    along: DVec2::X,
                    inward: DVec2::Y,
                    min: DVec2::ZERO,
                    max: DVec2::new(hx, hz) * 2.0,
                    y0,
                    y1,
                }
            })
            .collect(),
    }
}

fn lo_of(c: &[DVec2]) -> DVec2 {
    c.iter().fold(DVec2::splat(f64::MAX), |a, c| a.min(*c))
}

fn hi_of(c: &[DVec2]) -> DVec2 {
    c.iter().fold(DVec2::splat(f64::MIN), |a, c| a.max(*c))
}

/// The solids of one building, from its plan.
fn building_solids(plan: &BuildingPlan, base: f64) -> Vec<Solid> {
    let site = &plan.site;
    let mut out = Vec::new();
    let mut push = |r: Rect, y0: f64, y1: f64| {
        if r.size().x > 1e-6 && r.size().y > 1e-6 && y1 > y0 {
            out.push(Solid {
                origin: site.origin,
                along: site.along,
                inward: site.inward,
                min: r.min,
                max: r.max,
                y0,
                y1,
            });
        }
    };
    let n = plan.floors.len();
    for (k, f) in plan.floors.iter().enumerate() {
        let y = base + f.elevation;
        let top = if k + 1 < n {
            y + f.height - SLAB
        } else {
            y + f.height
        };
        // The slab under this floor covers the floor below (its terrace too),
        // with a hole for the lift shaft.
        if k > 0 {
            let below = plan.floors[k - 1].outline;
            for r in minus(below, plan.core) {
                push(r, y - SLAB, y);
            }
        }
        // Walls: every room edge, minus the doors on it. Shared edges come
        // twice; that is harmless for collision.
        for (ri, room) in f.rooms.iter().enumerate() {
            let r = room.rect;
            let edges = [
                (true, r.min.y, r.min.x, r.max.x),
                (true, r.max.y, r.min.x, r.max.x),
                (false, r.min.x, r.min.y, r.max.y),
                (false, r.max.x, r.min.y, r.max.y),
            ];
            for (along_x, fixed, t0, t1) in edges {
                let outer = if along_x {
                    (fixed - f.outline.min.y).abs() < 1e-6 || (fixed - f.outline.max.y).abs() < 1e-6
                } else {
                    (fixed - f.outline.min.x).abs() < 1e-6 || (fixed - f.outline.max.x).abs() < 1e-6
                };
                let mut gaps: Vec<(f64, f64)> = f
                    .doors
                    .iter()
                    .filter(|d| (d.a == ri || d.b == Some(ri)) && d.along_x == along_x)
                    .filter(|d| (if along_x { d.at.y } else { d.at.x } - fixed).abs() < 1e-6)
                    .map(|d| {
                        let c = if along_x { d.at.x } else { d.at.y };
                        (c - d.size.x * 0.5, c + d.size.x * 0.5)
                    })
                    .collect();
                gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
                let half = if outer { OUTER_WALL } else { INNER_WALL } * 0.5;
                let mut t = t0;
                let mut pieces = Vec::new();
                for (g0, g1) in gaps {
                    if g0 > t {
                        pieces.push((t, g0.min(t1)));
                    }
                    t = t.max(g1);
                }
                if t < t1 {
                    pieces.push((t, t1));
                }
                for (a, b) in pieces {
                    if b - a < 0.05 {
                        continue;
                    }
                    let rect = if along_x {
                        Rect::from_xz(a, fixed - half, b, fixed + half)
                    } else {
                        Rect::from_xz(fixed - half, a, fixed + half, b)
                    };
                    push(rect, y, top);
                }
            }
        }
        if k + 1 == n {
            push(f.outline, y + f.height - SLAB, y + f.height);
        }
    }
    out
}

/// A rectangle minus an optional hole, as up to four rectangles.
fn minus(r: Rect, hole: Option<Rect>) -> Vec<Rect> {
    let Some(h) = hole else { return vec![r] };
    if h.min.x >= r.max.x || h.max.x <= r.min.x || h.min.y >= r.max.y || h.max.y <= r.min.y {
        return vec![r];
    }
    let (hx0, hx1) = (h.min.x.max(r.min.x), h.max.x.min(r.max.x));
    let (hz0, hz1) = (h.min.y.max(r.min.y), h.max.y.min(r.max.y));
    vec![
        Rect::from_xz(r.min.x, r.min.y, r.max.x, hz0),
        Rect::from_xz(r.min.x, hz1, r.max.x, r.max.y),
        Rect::from_xz(r.min.x, hz0, hx0, hz1),
        Rect::from_xz(hx1, hz0, r.max.x, hz1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rivers_are_water_and_their_banks_too_high_to_step_onto() {
        let city = CityPlan::generate(hw_core::Seed(1), &hw_gen::city::CityParams::default());
        let o = Obstacles::from_city(&city, &[], 0.0, 0.0);
        let river = &city.water.rivers[0];
        let (a, b) = (
            river.pts[river.pts.len() / 2],
            river.pts[river.pts.len() / 2 + 1],
        );
        let dir = (b - a).normalize();
        let side = dir.perp();
        // Mid-river, away from any bridge.
        let mut mid = a.lerp(b, 0.5);
        for k in 0..40 {
            let p = a.lerp(b, k as f64 / 40.0);
            if o.ground_at(p) < 0.0 {
                mid = p;
                break;
            }
        }
        assert_eq!(o.ground_at(mid), hw_gen::water::WATER_LEVEL);
        let bank = mid + side * (river.width * 0.5 + 5.0);
        assert_eq!(o.ground_at(bank), 0.0);
        // Swim towards the bank: the quay wall stops the body.
        let mut body = Body {
            pos: DVec3::new(mid.x, hw_gen::water::WATER_LEVEL, mid.y),
            vel: DVec3::new(side.x, 0.0, side.y) * 5.0,
            grounded: true,
        };
        for _ in 0..200 {
            body.vel = DVec3::new(side.x, body.vel.y - 9.8 / 30.0, side.y) * 1.0;
            body.vel.x *= 5.0;
            body.vel.z *= 5.0;
            o.move_body(&mut body, 1.0 / 30.0);
        }
        assert!(body.pos.y < -3.0, "climbed the quay: {:?}", body.pos);
        let reached = DVec2::new(body.pos.x, body.pos.z).distance(mid);
        assert!(reached < river.width * 0.5 + 1.0, "went {reached} m");
    }

    fn one_box() -> Obstacles {
        let mut o = Obstacles::flat(0.0);
        o.add(Solid {
            origin: DVec2::ZERO,
            along: DVec2::X,
            inward: DVec2::Y,
            min: DVec2::new(0.0, 0.0),
            max: DVec2::new(10.0, 10.0),
            y0: 0.0,
            y1: 12.0,
        });
        o
    }

    #[test]
    fn walls_stop_a_runner() {
        let o = one_box();
        let mut b = Body {
            pos: DVec3::new(-3.0, 0.0, 5.0),
            vel: DVec3::new(7.0, 0.0, 0.0),
            grounded: true,
        };
        for _ in 0..60 {
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert!(b.pos.x <= -BODY_RADIUS + 1e-6, "went through: {}", b.pos);
        assert!(b.vel.x.abs() < 1e-9);
    }

    #[test]
    fn sliding_along_a_wall_keeps_the_sideways_part() {
        let o = one_box();
        let mut b = Body {
            pos: DVec3::new(-1.0, 0.0, 2.0),
            vel: DVec3::new(5.0, 0.0, 3.0),
            grounded: true,
        };
        for _ in 0..10 {
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert!(b.pos.z > 2.5, "no slide: {}", b.pos);
    }

    #[test]
    fn falling_onto_a_roof_lands_on_it() {
        let o = one_box();
        let mut b = Body {
            pos: DVec3::new(5.0, 20.0, 5.0),
            vel: DVec3::new(0.0, -10.0, 0.0),
            grounded: false,
        };
        for _ in 0..60 {
            o.move_body(&mut b, 1.0 / 30.0);
        }
        assert_eq!(b.pos.y, 12.0);
        assert!(b.grounded);
    }

    #[test]
    fn a_fast_body_does_not_tunnel() {
        let mut o = Obstacles::flat(0.0);
        o.add(Solid {
            origin: DVec2::ZERO,
            along: DVec2::X,
            inward: DVec2::Y,
            min: DVec2::new(0.0, -50.0),
            max: DVec2::new(0.3, 50.0),
            y0: 0.0,
            y1: 30.0,
        });
        let mut b = Body {
            pos: DVec3::new(-2.0, 1.0, 0.0),
            vel: DVec3::new(300.0, 0.0, 0.0),
            grounded: false,
        };
        o.move_body(&mut b, 1.0 / 30.0);
        assert!(b.pos.x < 0.0, "tunnelled to {}", b.pos);
    }

    /// What the town draws is what stops a body: every prop's solid parts,
    /// the landmarks' towers and stairs, and the plinths as raised ground
    /// that a runner from the low side cannot walk into.
    #[test]
    fn visible_things_are_solid() {
        use hw_core::Seed;
        use hw_gen::urban::world::TownWorld;
        use hw_gen::urban::TownParams;
        let w = Arc::new(TownWorld::generate(
            Seed(1),
            TownParams::sample(Seed(1), 20_000.0),
        ));
        let ground = w.clone();
        let o = Obstacles::from_town(&w, Terrain(Arc::new(move |p| ground.ground(p))));
        let mut solid = 0;
        for (_, p) in &w.everything().props {
            for f in p.footprints() {
                let c = f.at();
                assert!(
                    o.solid_at(DVec3::new(c.x, p.y + 0.3, c.y)),
                    "{:?} at {:?} can be walked through",
                    p.kind,
                    c
                );
                solid += 1;
            }
        }
        assert!(solid > 1000, "{solid} solid parts");
        let l = w.landmarks.first().expect("a landmark");
        for f in l.footprints() {
            let c = f.at();
            let (y0, _) = f.span();
            assert!(o.solid_at(DVec3::new(c.x, y0 + 0.3, c.y)), "{:?}", l.kind);
        }
        // On the plinth the ground is its top.
        let on = l.pos + l.dir * (l.plaza - 0.5);
        assert_eq!(o.ground_at(on), l.base);
        // A runner at the foot of its highest wall stays outside.
        let (out, drop) = (0..32)
            .map(|k| {
                let d = hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 32.0);
                let p = l.pos + d * (l.plaza + 0.6);
                (p, l.base - w.ground(p))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        if drop > STEP + 0.1 {
            let inward = (l.pos - out).normalize();
            let mut b = Body {
                pos: DVec3::new(out.x, w.ground(out), out.y),
                vel: DVec3::new(inward.x, 0.0, inward.y) * 6.0,
                grounded: true,
            };
            for _ in 0..30 {
                o.move_body(&mut b, 1.0 / 30.0);
            }
            let p = DVec2::new(b.pos.x, b.pos.z);
            assert!(
                p.distance(l.pos) > l.plaza - 0.01 || b.pos.y >= l.base - 0.01,
                "walked into the plinth's side: {:?}",
                b.pos
            );
        }
    }
}
