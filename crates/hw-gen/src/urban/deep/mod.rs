//! The deep underground: everything below the utility tunnels and
//! Molehaven, planned tile by tile when something comes near.
//!
//! Layers, from the top down (depths are below the lowest ground round
//! about, [`datum`]):
//!
//! - **The Aether Tube** (about 16 m down): a pneumatic capsule line under
//!   every main road, with stations a few hundred metres apart. Each station
//!   is a vaulted hall beside the tube with boarding gates, and a lift up to
//!   a Googie pavilion on the pavement.
//! - **The Doctor's lab** (30 m): a villain lair under the dystopia quarter,
//!   reached from a newspaper kiosk that is really a lift.
//! - **The buried city** (34 m): ruins of an older, Aether-lit civilisation:
//!   colonnaded streets, halls and domed vaults.
//! - **The Deeps** (52 m): natural caves under the whole land, galleries
//!   from tile to tile and grottos (crystal, mushroom, glow lake, fossil),
//!   where the mole folk camp.
//!
//! Lifts join the layers: from tube stations, ruins, the lab and Molehaven
//! down to the Deeps.
//!
//! Everything is made of [`Void`]s like the shallow underground, so carving,
//! collision and drawing come from the same numbers, and of [`Fixture`]s
//! (columns, vats, crystals...) whose solid parts are [`Footprint`]s.
//!
//! A tile is a pure function of the town's macro plan, the shallow
//! underground, the tube network and its own key: tiles never read each
//! other, so any order of planning gives the same world.

mod caves;
mod lair;
mod nooks;
mod ruins;
mod tube;

pub use nooks::{Folk, Livery, Nook, NookKind, SECTION, SLOT, STYLES};
pub use tube::{TubeLine, TubeNet, TubeStation, TUBE_HALF, TUBE_HEIGHT};

use super::footprint::Footprint;
use super::spiral::Spiral;
use super::underground::{Underground, Void, VoidKind};
use super::world::TownWorld;
use glam::{DVec2, DVec3};
use hw_core::Seed;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Side of a deep tile (m).
pub const TILE: f64 = 256.0;
/// Depths below [`datum`] of each layer's floor.
pub const LAIR_DEPTH: f64 = 30.0;
pub const RUIN_DEPTH: f64 = 34.0;
pub const DEEP_DEPTH: f64 = 52.0;
/// How far below the lowest ground of a voxel column the rock must be
/// generated to hold the deepest caves (their floor, its carving and the
/// ground's rise above the datum).
pub const VOXEL_DEPTH: f64 = DEEP_DEPTH + 34.0;
/// Tiles kept planned at once.
const CACHE: usize = 384;
/// Grid cells for finding a tile's voids.
const CELL: f64 = 8.0;
/// Coarse cells for the datum.
const DATUM_CELL: f64 = 192.0;

/// A tile's key.
pub type TileKey = (i64, i64);

/// The tile holding `p`.
pub fn tile_of(p: DVec2) -> TileKey {
    ((p.x / TILE).floor() as i64, (p.y / TILE).floor() as i64)
}

/// A tile's centre.
pub fn tile_centre(k: TileKey) -> DVec2 {
    DVec2::new((k.0 as f64 + 0.5) * TILE, (k.1 as f64 + 0.5) * TILE)
}

/// The lowest ground over a coarse cell and half a cell round it.
fn cell_low(w: &TownWorld, i: i64, j: i64) -> f64 {
    let lo = DVec2::new((i as f64 - 0.5) * DATUM_CELL, (j as f64 - 0.5) * DATUM_CELL);
    let mut low = f64::MAX;
    for a in 0..7 {
        for b in 0..7 {
            let p = lo + DVec2::new(a as f64, b as f64) * (DATUM_CELL * 2.0 / 6.0);
            low = low.min(w.ground(p));
        }
    }
    low
}

/// The reference the deep layers hang from: the lowest ground round about,
/// smooth (bilinear between coarse cells), never above the ground a couple
/// of hundred metres round.
pub fn datum(w: &TownWorld, p: DVec2) -> f64 {
    let q = p / DATUM_CELL - DVec2::splat(0.5);
    let (i, j) = (q.x.floor() as i64, q.y.floor() as i64);
    let (tx, ty) = (q.x - i as f64, q.y - j as f64);
    let a = cell_low(w, i, j) * (1.0 - tx) + cell_low(w, i + 1, j) * tx;
    let b = cell_low(w, i, j + 1) * (1.0 - tx) + cell_low(w, i + 1, j + 1) * tx;
    a * (1.0 - ty) + b * ty
}

/// Floor of the Deeps at `p`.
pub fn deep_floor(w: &TownWorld, p: DVec2) -> f64 {
    datum(w, p) - DEEP_DEPTH
}

/// What a fixture is, for drawing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FixtureKind {
    // Tube stations.
    Bench,
    Planter,
    DestinationBoard,
    // The buried city.
    Column,
    BrokenColumn,
    FallenColumn,
    Rubble,
    GiantHead,
    VaultDoor,
    Brazier,
    // The lab.
    TeslaCoil,
    Vat,
    Console,
    Reactor,
    CellBars,
    RayCannon,
    ScreenWall,
    Crate,
    // The Deeps.
    Crystal,
    GiantMushroom,
    Stalagmite,
    Skeleton,
    GlowPool,
    Tent,
    Campfire,
    /// The newspaper kiosk over the lab's lift (drawn with the entrance;
    /// only its walls here).
    Kiosk,
    // Along the tube: the mole folk's homes, market, shrine and burrows,
    // the tube workers' shop, the way-up signs.
    /// A round wooden door set in the rock (size: radius, height).
    RoundDoor,
    Barrel,
    /// A pot of small glowing mushrooms.
    MushroomPot,
    /// A crooked iron lamp post (size: radius, height).
    Lantern,
    /// Washing on a line overhead (size.x: half length).
    Washing,
    /// A woven rug on the floor (size: half length, -, half width).
    Rug,
    /// A market stall with a striped awning (half width, height, half depth).
    Stall,
    /// Strings of lamps across a cave (half length, -, half width).
    LampString,
    Workbench,
    /// A riveted pump with gauges and pipes (radius, height).
    Pump,
    /// A mine cart (half width, height, half length).
    Cart,
    /// A carved mole idol (radius, height).
    Idol,
    Candles,
    /// Mole folk's painted glyphs on a wall (half width, half height).
    Glyphs,
    /// A lit sign over a way up, read from both ways along the tube.
    ExitSign,
}

/// A thing standing in the deep underground: drawn from these numbers,
/// solid where its footprints say.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixture {
    pub kind: FixtureKind,
    /// Foot centre (y = the floor it stands on).
    pub at: DVec3,
    /// The way it faces (unit, plan).
    pub dir: DVec2,
    /// Its size: what the numbers mean depends on the kind (radius and
    /// height for round things, half width, half depth and height for
    /// boxes).
    pub size: DVec3,
    /// A variant number for looks (colour, damage).
    pub look: u32,
}

impl Fixture {
    /// The solid parts (none for things you can walk over or through).
    pub fn footprints(&self) -> Vec<Footprint> {
        use FixtureKind::*;
        let p = DVec2::new(self.at.x, self.at.z);
        let y = self.at.y;
        let disc = |r: f64, h: f64| Footprint::Disc {
            at: p,
            r,
            y0: y,
            y1: y + h,
        };
        let block = |hx: f64, hz: f64, h: f64| Footprint::Box {
            at: p,
            dir: self.dir,
            half: DVec2::new(hx, hz),
            y0: y,
            y1: y + h,
        };
        let s = self.size;
        match self.kind {
            GlowPool | Campfire | DestinationBoard | ScreenWall | VaultDoor | RoundDoor
            | Washing | Rug | LampString | Candles | Glyphs | ExitSign => Vec::new(),
            Barrel | Lantern | Pump | Idol => vec![disc(s.x, s.y)],
            MushroomPot => vec![disc(s.x, s.y)],
            Stall | Workbench | Cart => vec![block(s.x, s.z, s.y)],
            Kiosk => {
                // Back and side walls, and the front either side of the door.
                let h = s.x;
                let t = 0.06;
                let wall = |at: DVec2, d: DVec2, hx: f64| {
                    Footprint::Box {
                        at,
                        dir: d,
                        half: DVec2::new(hx, t),
                        y0: y,
                        y1: y + s.y,
                    }
                    .placed(p, self.dir, 0.0)
                };
                let (fx, fz) = (DVec2::X, DVec2::Y);
                vec![
                    wall(DVec2::new(0.0, -h), fz, h),
                    wall(DVec2::new(-h, 0.0), fx, h),
                    wall(DVec2::new(h, 0.0), fx, h),
                    wall(DVec2::new(-(h + 0.6) * 0.5, h), fz, (h - 0.6) * 0.5),
                    wall(DVec2::new((h + 0.6) * 0.5, h), fz, (h - 0.6) * 0.5),
                ]
            }
            Column | BrokenColumn | Stalagmite | TeslaCoil | Vat | Reactor | RayCannon
            | Brazier => vec![disc(s.x, s.y)],
            GiantMushroom => vec![disc(s.x * 0.45, s.y)],
            Crystal => vec![disc(s.x * 0.6, s.y)],
            GiantHead => vec![disc(s.x * 0.9, s.y)],
            Bench | Planter | Console | Crate | Rubble | Tent | Skeleton | FallenColumn => {
                vec![block(s.x, s.z, s.y)]
            }
            CellBars => vec![block(s.x, 0.08, s.y)],
        }
    }

    pub fn plan(&self) -> DVec2 {
        DVec2::new(self.at.x, self.at.z)
    }
}

/// A light in the deep underground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeepLamp {
    pub pos: DVec3,
    /// Linear-ish colour 0..1 and strength, range.
    pub color: [f32; 3],
    pub strength: f32,
    pub range: f32,
}

/// What a stair up to the street is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntranceKind {
    /// A tube station's Googie pavilion.
    Tube,
    /// The newspaper kiosk over the Doctor's lab.
    Kiosk,
    /// A way up from the tube between stations: a little green pavilion.
    Exit,
}

/// A stair from the street down.
#[derive(Clone, Debug, PartialEq)]
pub struct DeepEntrance {
    pub kind: EntranceKind,
    pub name: String,
    pub pos: DVec2,
    /// The way its door faces.
    pub door: DVec2,
    pub top: f64,
    pub bottom: f64,
    /// The spiral stair down the shaft.
    pub spiral: Spiral,
}

/// A stair between two deep layers (a closed shaft).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InnerLift {
    pub pos: DVec2,
    pub bottom: f64,
    pub top: f64,
    pub spiral: Spiral,
    /// Its top opens in a floor (Molehaven's side chamber): a rail round
    /// the hole on the three sides away from the door.
    pub rail: bool,
}

/// A tube station as planned in its tile.
#[derive(Clone, Debug, PartialEq)]
pub struct StationHall {
    /// Index in [`TubeNet::stations`].
    pub station: usize,
    /// The hall's void (in the tile).
    pub hall: usize,
    /// Boarding gates: the point in the tube where a capsule stops, and
    /// the way from the tube into the hall.
    pub gates: Vec<(DVec2, DVec2)>,
}

/// What kind of grotto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GrottoKind {
    Crystal,
    Mushroom,
    GlowLake,
    Fossil,
}

/// A cave chamber of the Deeps.
#[derive(Clone, Debug, PartialEq)]
pub struct Grotto {
    pub kind: GrottoKind,
    pub void: usize,
    pub centre: DVec2,
    pub floor: f64,
}

/// A buried ruin.
#[derive(Clone, Debug, PartialEq)]
pub struct Ruin {
    pub name: String,
    /// The colonnaded street, halls and the domed vault (void indices).
    pub street: usize,
    pub halls: Vec<usize>,
    pub rotunda: usize,
    pub floor: f64,
}

/// The villain's lab.
#[derive(Clone, Debug, PartialEq)]
pub struct Lair {
    pub name: String,
    /// The main lab hall, the reactor, the cells and the control room.
    pub hall: usize,
    pub reactor: usize,
    pub cells: usize,
    pub control: usize,
    pub floor: f64,
}

/// What a spot is for (the factions put their people there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpotKind {
    /// Mole folk camping in a grotto.
    MoleCamp,
    /// The Doctor's guards in the lab.
    LairGuard,
    /// Scavengers in the buried city.
    RuinProwler,
}

/// A place for a group of three to five: open floor at least 5 m round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeepSpot {
    pub kind: SpotKind,
    /// Feet position (y = floor).
    pub at: DVec3,
    /// Where the group looks (unit).
    pub facing: DVec2,
}

/// Everything planned for one tile.
#[derive(Debug, Default)]
pub struct DeepTile {
    pub key: TileKey,
    pub voids: Vec<Void>,
    pub fixtures: Vec<Fixture>,
    pub lamps: Vec<DeepLamp>,
    pub entrances: Vec<DeepEntrance>,
    pub lifts: Vec<InnerLift>,
    pub stations: Vec<StationHall>,
    pub grottos: Vec<Grotto>,
    pub ruins: Vec<Ruin>,
    pub lairs: Vec<Lair>,
    pub spots: Vec<DeepSpot>,
    /// Nooks along the tube, and the people who live and work in them.
    pub nooks: Vec<Nook>,
    pub folk: Vec<Folk>,
    /// Tube stretch voids: (void, line, point).
    pub tube_parts: Vec<(u32, u32, u32)>,
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    solids: Vec<Footprint>,
    solid_grid: BTreeMap<(i64, i64), Vec<u32>>,
    /// Plan bounds of everything in the tile.
    pub lo: DVec2,
    pub hi: DVec2,
}

fn cell(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

fn bounds(v: &Void) -> (DVec2, DVec2) {
    let c = v.corners();
    let lo = c.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
    let hi = c.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
    (lo, hi)
}

impl DeepTile {
    fn index(&mut self) {
        self.grid.clear();
        self.lo = DVec2::splat(f64::MAX);
        self.hi = DVec2::splat(f64::MIN);
        for (i, v) in self.voids.iter().enumerate() {
            let (lo, hi) = bounds(v);
            self.lo = self.lo.min(lo);
            self.hi = self.hi.max(hi);
            let (a, b) = (cell(lo), cell(hi));
            for x in a.0..=b.0 {
                for y in a.1..=b.1 {
                    self.grid.entry((x, y)).or_default().push(i as u32);
                }
            }
        }
        // Fixtures, and the stairs down the shafts (with a rail round the
        // hole where it opens on the street).
        self.solids = self.fixtures.iter().flat_map(|f| f.footprints()).collect();
        self.solids
            .extend(self.entrances.iter().flat_map(|e| e.spiral.solids(true)));
        self.solids
            .extend(self.lifts.iter().flat_map(|l| l.spiral.solids(l.rail)));
        self.solid_grid.clear();
        for (i, f) in self.solids.iter().enumerate() {
            let r = f.reach();
            let (a, b) = (
                cell(f.at() - DVec2::splat(r)),
                cell(f.at() + DVec2::splat(r)),
            );
            for x in a.0..=b.0 {
                for y in a.1..=b.1 {
                    self.solid_grid.entry((x, y)).or_default().push(i as u32);
                }
            }
        }
        for e in &self.entrances {
            self.lo = self.lo.min(e.pos - DVec2::splat(8.0));
            self.hi = self.hi.max(e.pos + DVec2::splat(8.0));
        }
    }

    /// Does anything of the tile reach within `r` of `p` (plan)?
    pub fn reaches(&self, p: DVec2, r: f64) -> bool {
        p.x >= self.lo.x - r && p.x <= self.hi.x + r && p.y >= self.lo.y - r && p.y <= self.hi.y + r
    }

    /// Voids whose rectangle covers `p`.
    pub fn at(&self, p: DVec2) -> impl Iterator<Item = (usize, &Void)> + '_ {
        self.grid
            .get(&cell(p))
            .into_iter()
            .flatten()
            .map(|&i| (i as usize, &self.voids[i as usize]))
            .filter(move |(_, v)| v.covers(p, 0.0))
    }

    /// Voids filed within `r` of `p`.
    pub fn near(&self, p: DVec2, r: f64) -> Vec<usize> {
        let (a, b) = (cell(p - DVec2::splat(r)), cell(p + DVec2::splat(r)));
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for (_, list) in self.grid.range((x, a.1)..=(x, b.1)) {
                out.extend(list.iter().map(|&i| i as usize));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Solid parts of fixtures within `r` of `p`.
    pub fn solids_near(&self, p: DVec2, r: f64) -> Vec<Footprint> {
        let (a, b) = (cell(p - DVec2::splat(r)), cell(p + DVec2::splat(r)));
        let mut idx = Vec::new();
        for x in a.0..=b.0 {
            for (_, list) in self.solid_grid.range((x, a.1)..=(x, b.1)) {
                idx.extend(list.iter().copied());
            }
        }
        idx.sort_unstable();
        idx.dedup();
        idx.into_iter().map(|i| self.solids[i as usize]).collect()
    }

    /// All the tile's solid parts.
    pub fn solids(&self) -> &[Footprint] {
        &self.solids
    }
}

/// The deep underground of a town: the tube network, the lab's site, and
/// tiles planned on demand.
pub struct Deep {
    id: u64,
    net: OnceLock<TubeNet>,
    lair: OnceLock<Option<lair::LairSite>>,
    cache: Mutex<Cache>,
}

#[derive(Default)]
struct Cache {
    tiles: BTreeMap<TileKey, (Arc<DeepTile>, u64)>,
    tick: u64,
}

impl std::fmt::Debug for Deep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deep").field("id", &self.id).finish()
    }
}

impl Default for Deep {
    fn default() -> Self {
        Self::new()
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Deep {
    pub fn new() -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            net: OnceLock::new(),
            lair: OnceLock::new(),
            cache: Mutex::new(Cache::default()),
        }
    }

    /// How many tiles are planned now (bounded).
    pub fn planned(&self) -> usize {
        self.cache.lock().expect("deep cache").tiles.len()
    }
}

thread_local! {
    /// The last few tiles this thread asked for, to spare the lock on hot
    /// paths (the voxel ground asks for millions of points).
    static RECENT: std::cell::RefCell<Vec<(u64, TileKey, Arc<DeepTile>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A hole in the street over a stair down: what grass, lawns and
/// pavements leave open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreetHole {
    pub pos: DVec2,
    /// The way the door faces (unit): the square's own +z.
    pub door: DVec2,
    /// Half the side of the open shaft, and of the paved apron round it.
    pub hole: f64,
    pub apron: f64,
}

impl StreetHole {
    /// Signed distance from `p` to the square of half side `half` round
    /// the hole (negative inside).
    pub fn distance(&self, p: DVec2, half: f64) -> f64 {
        let d = p - self.pos;
        let l = DVec2::new(d.dot(self.door.perp()), d.dot(self.door)).abs() - DVec2::splat(half);
        l.max(DVec2::ZERO).length() + l.x.max(l.y).min(0.0)
    }
}

impl TownWorld {
    /// The holes in the street over the stairs down whose shafts lie in
    /// `lo`..`hi`: the shallow underground's, and the deep one's (tube
    /// stations, ways up, the lab's kiosk) where the tube or the lab is.
    pub fn street_holes(&self, lo: DVec2, hi: DVec2) -> Vec<StreetHole> {
        use super::underground::{APRON, SHAFT_HALF};
        let inside = |p: DVec2| p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y;
        let mut out: Vec<StreetHole> = self
            .underground()
            .entrances
            .iter()
            .filter(|e| inside(e.pos))
            .map(|e| StreetHole {
                pos: e.pos,
                door: e.door,
                hole: SHAFT_HALF,
                apron: SHAFT_HALF + APRON,
            })
            .collect();
        // A way up can stand a little outside the tile that plans it.
        let m = DVec2::splat(40.0);
        let (a, b) = (tile_of(lo - m), tile_of(hi + m));
        let net = self.tube();
        let lair = self.lair_centre();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                let k = (x, y);
                let c = tile_centre(k);
                let deep = self.deep_in_land(k)
                    && (net.stretches_in(k).is_some_and(|s| !s.is_empty())
                        || !net.stations_in(k).is_empty()
                        || lair.is_some_and(|l| (l - c).abs().max_element() < TILE));
                if !deep {
                    continue;
                }
                for e in self.deep_tile(k).entrances.iter().filter(|e| inside(e.pos)) {
                    let apron = match e.kind {
                        EntranceKind::Kiosk => SHAFT_HALF + 0.3,
                        EntranceKind::Tube | EntranceKind::Exit => SHAFT_HALF + APRON,
                    };
                    out.push(StreetHole {
                        pos: e.pos,
                        door: e.door,
                        hole: SHAFT_HALF,
                        apron,
                    });
                }
            }
        }
        out
    }
}

impl TownWorld {
    /// The tube network (planned once, from the main roads).
    pub fn tube(&self) -> &TubeNet {
        self.deep_state().net.get_or_init(|| TubeNet::plan(self))
    }

    /// Where the villain's lab lies, if the town has one.
    pub fn lair_centre(&self) -> Option<DVec2> {
        self.lair_site().map(|s| s.centre)
    }

    pub(crate) fn lair_site(&self) -> Option<&lair::LairSite> {
        self.deep_state()
            .lair
            .get_or_init(|| lair::LairSite::plan(self))
            .as_ref()
    }

    /// The deep tile `k`, planned if it is not yet.
    pub fn deep_tile(&self, k: TileKey) -> Arc<DeepTile> {
        let id = self.deep_state().id;
        if let Some(t) = RECENT.with(|r| {
            r.borrow()
                .iter()
                .find(|(i, key, _)| *i == id && *key == k)
                .map(|x| x.2.clone())
        }) {
            return t;
        }
        let t = {
            let mut c = self.deep_state().cache.lock().expect("deep cache");
            c.tick += 1;
            let tick = c.tick;
            c.tiles.get_mut(&k).map(|e| {
                e.1 = tick;
                e.0.clone()
            })
        };
        let t = t.unwrap_or_else(|| {
            // Planned outside the lock: two threads may plan the same tile,
            // and get the same result.
            let t = Arc::new(self.plan_deep_tile(k));
            let mut c = self.deep_state().cache.lock().expect("deep cache");
            c.tick += 1;
            let tick = c.tick;
            let t = c.tiles.entry(k).or_insert((t, tick)).0.clone();
            while c.tiles.len() > CACHE {
                let old = c
                    .tiles
                    .iter()
                    .min_by_key(|(_, v)| v.1)
                    .map(|(k, _)| *k)
                    .expect("not empty");
                c.tiles.remove(&old);
            }
            t
        });
        RECENT.with(|r| {
            let mut r = r.borrow_mut();
            r.retain(|(i, key, _)| !(*i == id && *key == k));
            r.insert(0, (id, k, t.clone()));
            r.truncate(12);
        });
        t
    }

    /// The deep tile `k` if it is planned already (plans nothing: for the
    /// main thread, where the deep view's worker plans the tiles round the
    /// camera).
    pub fn deep_planned(&self, k: TileKey) -> Option<Arc<DeepTile>> {
        let c = self.deep_state().cache.lock().expect("deep cache");
        c.tiles.get(&k).map(|e| e.0.clone())
    }

    /// The planned deep tiles whose plan reaches within `r` of `p`.
    pub fn deep_planned_near(&self, p: DVec2, r: f64) -> Vec<Arc<DeepTile>> {
        let (a, b) = (
            tile_of(p - DVec2::splat(r + TILE * 0.5)),
            tile_of(p + DVec2::splat(r + TILE * 0.5)),
        );
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                if let Some(t) = self.deep_planned((x, y)).filter(|t| t.reaches(p, r)) {
                    out.push(t);
                }
            }
        }
        out
    }

    /// Can the deep underground matter for something at height `y` over
    /// `p`? Only below the ground, or by a street lift's opening (near a
    /// station or the lab). Cheap, and plans nothing: ask it before the
    /// other `deep_` queries on hot paths.
    pub fn deep_matters(&self, p: DVec2, y: f64) -> bool {
        y < self.ground(p) - 0.5
            || self.tube().station_within(p, 95.0)
            || self
                .lair_site()
                .is_some_and(|l| l.centre.distance(p) < 110.0)
    }

    /// The deep tiles that reach within `r` of `p` (plan).
    pub fn deep_near(&self, p: DVec2, r: f64) -> Vec<Arc<DeepTile>> {
        let (a, b) = (
            tile_of(p - DVec2::splat(r + TILE * 0.5)),
            tile_of(p + DVec2::splat(r + TILE * 0.5)),
        );
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                if !self.deep_in_land((x, y)) {
                    continue;
                }
                let t = self.deep_tile((x, y));
                if t.reaches(p, r) {
                    out.push(t);
                }
            }
        }
        out
    }

    /// Does tile `k` lie on the planned land?
    pub fn deep_in_land(&self, k: TileKey) -> bool {
        let c = tile_centre(k);
        let h = self.half();
        c.x.abs() < h && c.y.abs() < h
    }

    /// The floor for something at height `y` over `p` in the deep
    /// underground, if it is in a deep void there.
    pub fn deep_floor_for(&self, p: DVec2, y: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for t in self.deep_near(p, 0.0) {
            if let Some(f) = floor_for_in(t.at(p).map(|x| x.1), p, y) {
                best = Some(best.map_or(f, |b: f64| b.max(f)));
            }
        }
        best
    }

    /// The lowest deep ceiling above `y` at `p`.
    pub fn deep_ceiling_for(&self, p: DVec2, y: f64) -> Option<f64> {
        self.deep_near(p, 0.0)
            .iter()
            .filter_map(|t| ceiling_for_in(&t.at(p).map(|x| x.1).collect::<Vec<_>>(), p, y))
            .reduce(f64::min)
    }

    /// Is `p` (3D) in a deep void?
    pub fn deep_open_at(&self, p: DVec3) -> bool {
        let q = DVec2::new(p.x, p.z);
        self.deep_near(q, 0.0)
            .iter()
            .any(|t| t.at(q).any(|(_, v)| v.contains(p)))
    }

    /// Is any deep void within `r` of `p`?
    pub fn deep_voids_near(&self, p: DVec2, r: f64) -> bool {
        self.deep_near(p, r)
            .iter()
            .any(|t| !t.near(p, r).is_empty())
    }

    /// Solid fixture parts within `r` of `p`.
    pub fn deep_solids_near(&self, p: DVec2, r: f64) -> Vec<Footprint> {
        self.deep_near(p, r)
            .iter()
            .flat_map(|t| t.solids_near(p, r))
            .collect()
    }

    /// Places for faction groups in the deep tiles over `lo..hi`
    /// (plans the tiles).
    pub fn deep_spots_in(&self, lo: DVec2, hi: DVec2) -> Vec<DeepSpot> {
        let (a, b) = (tile_of(lo), tile_of(hi));
        let mut out = Vec::new();
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                if self.deep_in_land((x, y)) {
                    out.extend(self.deep_tile((x, y)).spots.iter().copied());
                }
            }
        }
        out
    }

    fn plan_deep_tile(&self, k: TileKey) -> DeepTile {
        let mut t = DeepTile {
            key: k,
            ..DeepTile::default()
        };
        if !self.deep_in_land(k) {
            t.index();
            return t;
        }
        let seed = self.seed.child("deep").cell(k.0, k.1);
        let mut plan = Plan {
            w: self,
            under: self.core_underground(),
            seed,
            t: &mut t,
            bottoms: Vec::new(),
        };
        tube::plan_tile(&mut plan);
        nooks::plan_tile(&mut plan);
        lair::plan_tile(&mut plan);
        ruins::plan_tile(&mut plan);
        caves::plan_tile(&mut plan);
        t.index();
        t
    }
}

/// A tile being planned.
pub(crate) struct Plan<'a> {
    pub w: &'a TownWorld,
    pub under: &'a Underground,
    pub seed: Seed,
    pub t: &'a mut DeepTile,
    /// Lift bottoms in the Deeps that the caves must reach.
    pub bottoms: Vec<DVec2>,
}

impl Plan<'_> {
    pub fn key(&self) -> TileKey {
        self.t.key
    }

    /// Does `v` keep `margin` clear of the shallow underground (tunnels,
    /// lifts, Molehaven)?
    pub fn clear_of_shallow(&self, v: &Void, margin: f64) -> bool {
        let (lo, hi) = bounds(v);
        let c = (lo + hi) * 0.5;
        let r = (hi - lo).length() * 0.5 + margin;
        self.under
            .near(c, r)
            .into_iter()
            .all(|i| !overlaps(v, &self.under.voids[i], margin))
    }

    /// Is the void roofed by at least `cover` of rock?
    pub fn roofed(&self, v: &Void, cover: f64) -> bool {
        roofed(self.w, v, cover)
    }

    /// A closed stair shaft from `bottom` up to `top` at `c`.
    pub fn inner_lift(&mut self, c: DVec2, dir: DVec2, bottom: f64, top: f64, rail: bool) {
        let spiral = Spiral::new(c, dir, top, bottom);
        self.t.voids.push(spiral.shaft(Some(top - bottom + 4.0)));
        self.t.lifts.push(InnerLift {
            pos: c,
            bottom,
            top,
            spiral,
            rail,
        });
        self.t.lamps.push(DeepLamp {
            pos: DVec3::new(c.x, bottom + 2.6, c.y),
            color: [0.5, 0.9, 1.0],
            strength: 5.0,
            range: 9.0,
        });
        self.t.lamps.push(DeepLamp {
            pos: DVec3::new(c.x, top + 2.6, c.y),
            color: [0.5, 0.9, 1.0],
            strength: 5.0,
            range: 9.0,
        });
    }

    /// A stair from the deep layer above down to the Deeps at `c` (the caves
    /// connect its foot to the tile's galleries).
    pub fn lift_to_deeps(&mut self, c: DVec2, top: f64) {
        let bottom = deep_floor(self.w, c);
        self.inner_lift(c, DVec2::Y, bottom, top, false);
        self.bottoms.push(c);
    }
}

/// A straight run of open space from `a` (floor `fa`) to `b` (floor
/// `fb`), `half` wide each side, its ends reaching `half` past the points
/// (so runs joined end to end overlap) with the floor carried on.
pub(crate) fn run(
    kind: VoidKind,
    a: DVec2,
    b: DVec2,
    half: f64,
    fa: f64,
    fb: f64,
    height: f64,
) -> Option<Void> {
    let len = a.distance(b);
    if len < 0.5 {
        return None;
    }
    let slope = (fb - fa) / len;
    Some(Void {
        kind,
        origin: a,
        along: (b - a) / len,
        min: DVec2::new(-half, -half),
        max: DVec2::new(len + half, half),
        floor: [fa - slope * half, fb + slope * half],
        height,
        open: false,
    })
}

/// A box room: centre `c`, local x along `along`, half size, floor, height.
pub(crate) fn room(
    kind: VoidKind,
    c: DVec2,
    along: DVec2,
    half: DVec2,
    floor: f64,
    height: f64,
) -> Void {
    Void {
        kind,
        origin: c,
        along,
        min: -half,
        max: half,
        floor: [floor, floor],
        height,
        open: false,
    }
}

/// Is the void under at least `cover` of rock everywhere?
pub(crate) fn roofed(w: &TownWorld, v: &Void, cover: f64) -> bool {
    if v.open {
        return true;
    }
    let size = v.max - v.min;
    let (nx, ny) = (
        (size.x / 6.0).ceil().max(1.0) as usize,
        (size.y / 6.0).ceil().max(1.0) as usize,
    );
    (0..=nx).all(|i| {
        (0..=ny).all(|j| {
            let l =
                v.min + DVec2::new(size.x * i as f64 / nx as f64, size.y * j as f64 / ny as f64);
            let p = v.world(l);
            v.ceiling_at(p).is_none_or(|c| c < w.ground(p) - cover)
        })
    })
}

/// Do two voids come within `margin` of each other (plan rectangles and
/// height spans, open shafts reaching the sky)? Sloping voids are taken a
/// few metres at a time.
pub(crate) fn overlaps(a: &Void, b: &Void, margin: f64) -> bool {
    let sa = slices(a);
    let sb = slices(b);
    sa.iter()
        .any(|x| sb.iter().any(|y| overlaps_flat(x, y, margin)))
}

/// A void cut into pieces short enough that its floor hardly changes.
fn slices(v: &Void) -> Vec<Void> {
    let rise = (v.floor[1] - v.floor[0]).abs();
    let len = v.max.x - v.min.x;
    let n = ((rise / 0.5).ceil() as usize).clamp(1, (len / 2.0).ceil().max(1.0) as usize);
    if n == 1 {
        return vec![v.clone()];
    }
    (0..n)
        .map(|k| {
            let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
            let f = |t: f64| v.floor[0] + (v.floor[1] - v.floor[0]) * t;
            Void {
                min: DVec2::new(v.min.x + len * t0, v.min.y),
                max: DVec2::new(v.min.x + len * t1, v.max.y),
                floor: [f(t0), f(t1)],
                ..v.clone()
            }
        })
        .collect()
}

fn overlaps_flat(a: &Void, b: &Void, margin: f64) -> bool {
    let span = |v: &Void| {
        let lo = v.floor[0].min(v.floor[1]);
        let hi = if v.open {
            f64::MAX
        } else {
            v.floor[0].max(v.floor[1]) + v.height
        };
        (lo, hi)
    };
    let (sa, sb) = (span(a), span(b));
    if sa.1 + margin < sb.0 || sb.1 + margin < sa.0 {
        return false;
    }
    // Separating axes of two rectangles.
    let ca = a.corners();
    let cb = b.corners();
    for axis in [a.along, a.inward(), b.along, b.inward()] {
        let pa = ca.iter().map(|p| p.dot(axis));
        let pb = cb.iter().map(|p| p.dot(axis));
        let (a0, a1) = pa.fold((f64::MAX, f64::MIN), |m, x| (m.0.min(x), m.1.max(x)));
        let (b0, b1) = pb.fold((f64::MAX, f64::MIN), |m, x| (m.0.min(x), m.1.max(x)));
        if a1 + margin < b0 || b1 + margin < a0 {
            return false;
        }
    }
    true
}

/// The floor under `p` for something at `y`, among `voids` covering `p`:
/// the highest floor at most a step above `y` whose space reaches `y`.
/// Inside a stair shaft the floor is the shaft's own, far below: the floors
/// of the rooms it passes through are cut away over it (the steps, being
/// solids, hold a body up).
pub fn floor_for_in<'a>(voids: impl Iterator<Item = &'a Void>, p: DVec2, y: f64) -> Option<f64> {
    let voids: Vec<&Void> = voids.collect();
    let shaft = voids.iter().find(|v| {
        v.kind == VoidKind::Shaft
            && y >= v.floor_at(p) - 0.5
            && v.ceiling_at(p).is_none_or(|c| y <= c)
    });
    if let Some(v) = shaft {
        return Some(v.floor_at(p));
    }
    let mut best: Option<f64> = None;
    for v in voids {
        let f = v.floor_at(p);
        let reach = v.ceiling_at(p).unwrap_or(f64::MAX);
        if y >= f - 1.0 && y <= reach + 0.45 && best.is_none_or(|b| f > b) {
            best = Some(f);
        }
    }
    best
}

/// The lowest ceiling above `y` at `p` among `voids` covering `p` that `y`
/// is in (none if one of them is open to the sky).
pub fn ceiling_for_in(voids: &[&Void], p: DVec2, y: f64) -> Option<f64> {
    if voids.iter().any(|v| v.open && y >= v.floor_at(p) - 1.0) {
        return None;
    }
    voids
        .iter()
        .filter(|v| y >= v.floor_at(p) - 1.0)
        .filter_map(|v| v.ceiling_at(p))
        .filter(|c| *c >= y)
        .reduce(f64::min)
}

/// A name from two word lists, picked by `seed`.
pub(crate) fn name(seed: Seed, first: &[&str], second: &[&str]) -> String {
    let mut r = seed.rng();
    format!("{} {}", r.pick(first), r.pick(second))
}

#[cfg(test)]
mod tests;
