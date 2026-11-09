//! The town's detail, made only where someone is.
//!
//! The big picture ([`super::Town`]: land, water, growth, main roads and the
//! pieces between them) is planned up front; it takes a second or two even
//! for a megacity. Everything finer is planned one *unit* at a time, on
//! demand, and forgotten again when nobody is near:
//!
//! 1. A piece wider than [`super::PIECE_MAX`] is split into districts by
//!    district roads ([`super::districts`]), so no unit is too big.
//! 2. A district's side streets and blocks ([`Streets`]).
//! 3. A district's buildings, props, quay walls and graffiti ([`Quarter`]).
//!
//! Outside every piece (country roads, the shore beyond the town) the units
//! are square tiles of [`VERGE_TILE`], holding the props and quay walls of
//! the main roads there.
//!
//! Every step reads only the big picture, its own unit and its neighbours'
//! *streets* (never their buildings or props), so it comes out the same
//! whichever unit is planned first, on every machine. Everything is owned by
//! exactly one unit: the one whose ground holds its anchor point (a prop's
//! foot, a building's site, a wall's sample). Things are named by
//! [`DetailId`]: their unit and their number in it, stable for the seed.

use super::graph::signed_area;
use super::props::Prop;
use super::streets::{plan_piece, PieceStreets};
use super::waterfront::{QuayWall, WaterSteps};
use super::world::{Bridge, WorldBuilding, WorldRoad};
use super::Piece;
use glam::DVec2;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Side of the square units outside the pieces (m).
pub const VERGE_TILE: f64 = 512.0;
/// The district number that names a piece's own district roads.
pub const PIECE_ROADS: u32 = 1023;

/// One unit of detail: a district of a piece, or a verge tile outside them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unit(pub u32);

/// What a [`Unit`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    /// District `district` of piece `piece` (index into the town's pieces).
    District { piece: usize, district: usize },
    /// The verge tile at (x, y), in [`VERGE_TILE`]s from the town's centre.
    Verge { x: i64, y: i64 },
    /// The main roads themselves (only as part of road ids).
    Main,
}

const VERGE_BIT: u32 = 1 << 31;
const TILE_OFFSET: i64 = 1 << 14;

impl Unit {
    /// The main roads, as the unit part of their ids.
    pub const MAIN: Unit = Unit(u32::MAX);

    pub fn district(piece: usize, district: usize) -> Unit {
        debug_assert!(piece < 1 << 21 && district <= PIECE_ROADS as usize);
        Unit(((piece as u32) << 10) | district as u32)
    }

    pub fn verge(x: i64, y: i64) -> Unit {
        let (x, y) = ((x + TILE_OFFSET) as u32, (y + TILE_OFFSET) as u32);
        Unit(VERGE_BIT | ((x & 0x7FFF) << 15) | (y & 0x7FFF))
    }

    pub fn kind(self) -> UnitKind {
        if self == Unit::MAIN {
            UnitKind::Main
        } else if self.0 & VERGE_BIT != 0 {
            UnitKind::Verge {
                x: ((self.0 >> 15) & 0x7FFF) as i64 - TILE_OFFSET,
                y: (self.0 & 0x7FFF) as i64 - TILE_OFFSET,
            }
        } else {
            UnitKind::District {
                piece: (self.0 >> 10) as usize,
                district: (self.0 & 0x3FF) as usize,
            }
        }
    }

    /// The verge tile holding `p`.
    pub fn verge_at(p: DVec2) -> Unit {
        Unit::verge(
            (p.x / VERGE_TILE).floor() as i64,
            (p.y / VERGE_TILE).floor() as i64,
        )
    }
}

/// A building, prop or road: its unit and its number there. Stable for the
/// seed, whichever units were planned before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DetailId(pub u64);

impl DetailId {
    pub fn new(unit: Unit, n: usize) -> Self {
        DetailId(((unit.0 as u64) << 32) | n as u64)
    }

    pub fn unit(self) -> Unit {
        Unit((self.0 >> 32) as u32)
    }

    pub fn index(self) -> usize {
        (self.0 & 0xFFFF_FFFF) as usize
    }
}

/// A piece cut into districts, and the district roads between them.
#[derive(Debug)]
pub struct Split {
    pub districts: Vec<Piece>,
    /// Bounding boxes of the districts.
    pub boxes: Vec<(DVec2, DVec2)>,
    /// The district roads (main roads in all but name: avenues).
    pub roads: Vec<WorldRoad>,
    /// Where they cross water.
    pub bridges: Vec<Bridge>,
}

/// A district's side streets.
#[derive(Debug)]
pub struct Streets {
    pub unit: Unit,
    /// The district's ground.
    pub piece: Piece,
    pub plan: PieceStreets,
    /// The side streets, ready to draw (ids in this unit).
    pub roads: Vec<WorldRoad>,
    /// Where they cross water.
    pub bridges: Vec<Bridge>,
    pub lo: DVec2,
    pub hi: DVec2,
}

/// Everything standing in one unit.
#[derive(Debug)]
pub struct Quarter {
    pub unit: Unit,
    /// The ground it covers (a verge tile's square, or a district's box).
    pub lo: DVec2,
    pub hi: DVec2,
    /// A district's streets (none in a verge tile).
    pub streets: Option<Arc<Streets>>,
    /// Ids: [`DetailId::new`]`(unit, index)`.
    pub buildings: Vec<WorldBuilding>,
    pub props: Vec<Prop>,
    pub quays: Vec<QuayWall>,
    pub water_steps: Vec<WaterSteps>,
}

impl Quarter {
    /// Its side streets (empty in a verge tile).
    pub fn roads(&self) -> &[WorldRoad] {
        self.streets.as_ref().map_or(&[], |s| s.roads.as_slice())
    }

    /// Its side streets' bridges.
    pub fn bridges(&self) -> &[Bridge] {
        self.streets.as_ref().map_or(&[], |s| s.bridges.as_slice())
    }

    /// Buildings with their ids.
    pub fn buildings_with_ids(&self) -> impl Iterator<Item = (DetailId, &WorldBuilding)> {
        let u = self.unit;
        self.buildings
            .iter()
            .enumerate()
            .map(move |(i, b)| (DetailId::new(u, i), b))
    }

    /// Props with their ids.
    pub fn props_with_ids(&self) -> impl Iterator<Item = (DetailId, &Prop)> {
        let u = self.unit;
        self.props
            .iter()
            .enumerate()
            .map(move |(i, p)| (DetailId::new(u, i), p))
    }

    /// Does the box come near anything of this unit?
    pub fn touches(&self, lo: DVec2, hi: DVec2) -> bool {
        lo.x <= self.hi.x && hi.x >= self.lo.x && lo.y <= self.hi.y && hi.y >= self.lo.y
    }
}

/// A small least-recently-used store of things made on demand. Values are
/// shared: whoever still holds one keeps it alive after it leaves here.
pub(super) struct Cache<K, V> {
    map: Mutex<BTreeMap<K, (Arc<V>, u64)>>,
    tick: AtomicU64,
    cap: usize,
}

impl<K: Ord + Copy, V> Cache<K, V> {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            map: Mutex::new(BTreeMap::new()),
            tick: AtomicU64::new(0),
            cap,
        }
    }

    /// The value for `k`, made with `make` if it is not here. Making runs
    /// unlocked, so other threads keep working (two may make the same value
    /// at once; both come out the same).
    pub(super) fn get(&self, k: K, make: impl FnOnce() -> V) -> Arc<V> {
        let t = self.tick.fetch_add(1, Ordering::Relaxed);
        if let Some(e) = self.map.lock().expect("cache lock").get_mut(&k) {
            e.1 = t;
            return e.0.clone();
        }
        let v = Arc::new(make());
        let mut map = self.map.lock().expect("cache lock");
        let v = map.entry(k).or_insert((v, t)).0.clone();
        while map.len() > self.cap {
            let Some(old) = map.iter().min_by_key(|(_, e)| e.1).map(|(k, _)| *k) else {
                break;
            };
            map.remove(&old);
        }
        v
    }

    /// Is `k` here now?
    pub(super) fn contains(&self, k: K) -> bool {
        self.map.lock().expect("cache lock").contains_key(&k)
    }

    /// How many values are here now.
    pub(super) fn len(&self) -> usize {
        self.map.lock().expect("cache lock").len()
    }
}

/// Bounding box of points.
pub(super) fn bounds(pts: &[DVec2]) -> (DVec2, DVec2) {
    pts.iter().fold((DVec2::MAX, DVec2::MIN), |(lo, hi), p| {
        (lo.min(*p), hi.max(*p))
    })
}

/// Do two boxes overlap (with `margin`)?
pub(super) fn overlap(a: (DVec2, DVec2), b: (DVec2, DVec2), margin: f64) -> bool {
    a.0.x <= b.1.x + margin
        && a.1.x + margin >= b.0.x
        && a.0.y <= b.1.y + margin
        && a.1.y + margin >= b.0.y
}

/// Plans a district's side streets.
pub(super) fn plan_streets(
    seed: hw_core::Seed,
    town: &super::Town,
    unit: Unit,
    piece: &Piece,
    roads: impl Fn(Vec<Vec<DVec2>>, &PieceStreets) -> (Vec<WorldRoad>, Vec<Bridge>),
) -> Streets {
    let plan = plan_piece(seed, town, piece);
    let (roads, bridges) = roads(plan.streets.clone(), &plan);
    let (lo, hi) = bounds(&piece.face.pts);
    Streets {
        unit,
        piece: piece.clone(),
        plan,
        roads,
        bridges,
        lo,
        hi,
    }
}

/// Total footprint of a district's blocks.
pub fn block_area(s: &Streets) -> f64 {
    s.plan.blocks.iter().map(|b| signed_area(b)).sum()
}
