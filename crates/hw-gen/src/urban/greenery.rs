//! What grows on the town's open ground: which kind of grass, and how thick.
//!
//! The ground between the roads, the buildings and the water is green. How
//! it is kept depends on where it is: mown lawns in the gardens and on the
//! verges, a park's grass a little longer with clover and daisies in it,
//! meadow grass with wildflowers in the open country, and rough, dry tufts
//! in the yards of the works and in the villains' quarter. Sand stays bare
//! by the water, steep banks show earth, and nothing grows on a road, under
//! a building, on a landmark's plinth or in a fountain.
//!
//! Plain data and pure functions of the town: the client asks it where to
//! put grass blades around the camera.

use super::deep::StreetHole;
use super::growth::Era;
use super::land::smooth;
use super::props::{Blossom, Prop, PropKind, Species};
use super::streets::StreetStyle;
use super::world::{Bridge, TownWorld, WorldBuilding, WorldRoad, DECK};
use super::{contains, Town};
use crate::style::Region;
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::{Noise, Seed};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The kinds of grass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sward {
    /// Short mown lawn (gardens, verges, squares' greens).
    Lawn,
    /// A park's grass: mown less often, clover and daisies in it.
    Park,
    /// Long meadow grass with seed heads and wildflowers (open country).
    Meadow,
    /// Dry, patchy tufts and weeds (works' yards, the villains' quarter).
    Rough,
}

impl Sward {
    pub const ALL: [Sward; 4] = [Sward::Lawn, Sward::Park, Sward::Meadow, Sward::Rough];
}

/// Grass at a point: its kind and how thick it grows there (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grass {
    pub sward: Sward,
    pub density: f64,
}

/// Grid cell for the indices (m).
const CELL: f64 = 32.0;
/// Grass stops this far from a road's edge (the kerb).
const KERB: f64 = 0.12;
/// Distances are exact up to this far from roads and buildings (m).
const REACH: f64 = 2.5;
/// Ground below this (m above the water) is sand: the beaches.
const SAND_TOP: f64 = 2.4;
/// Grass thins out to nothing over this band above [`SAND_TOP`].
const SAND_FADE: f64 = 1.0;

type Key = (i64, i64);

fn key(p: DVec2) -> Key {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

fn keys_around(p: DVec2, r: f64) -> impl Iterator<Item = Key> {
    let (a, b) = (key(p - DVec2::splat(r)), key(p + DVec2::splat(r)));
    (a.0..=b.0).flat_map(move |x| (a.1..=b.1).map(move |z| (x, z)))
}

/// How far `p` is outside the strip that segment `si` of road `r` is drawn
/// as (negative inside; exact in sign, near 0 in size). The strip ends
/// square where the road ends, as it is drawn ([`WorldRoad::strip_points`]),
/// not round: a round end would keep grass off ground no road covers.
fn edge_distance(r: &WorldRoad, si: usize, p: DVec2) -> f64 {
    let total = r.half + r.pavement;
    let (a, b) = (r.pts[si], r.pts[si + 1]);
    let mut d = segment_distance(p, a, b) - total;
    let m = r.pts.len();
    // The end segments stop square at the end, as drawn. A true distance to
    // that strip, not the round one: the round field is deep inside right up
    // to the end and positive just past it, so a lawn clipped along the
    // field's zero strayed half a metre from the road's end.
    for (end, next, mine) in [
        (r.pts[0], r.pts[1], si == 0),
        (r.pts[m - 1], r.pts[m - 2], si == m - 2),
    ] {
        if !mine {
            continue;
        }
        let out = (end - next).normalize_or_zero();
        let over = (p - end).dot(out);
        // Not yet past the segment's inner end.
        if -over <= (end - next).length() {
            let side = (p - end).dot(out.perp()).abs();
            d = (side - total).max(over);
        }
    }
    d
}

/// Things grass keeps clear of, besides roads and buildings: a disc.
fn clearing(p: &Prop) -> Option<f64> {
    Some(match p.kind {
        PropKind::Fountain => 3.2,
        PropKind::Kiosk => 1.7,
        PropKind::Statue(_) => 1.3,
        PropKind::AdPillar => 0.75,
        PropKind::BusStop => 2.0,
        PropKind::PhoneBooth => 0.65,
        PropKind::NewsStand => 0.95,
        PropKind::CafeTable(_) => 1.0,
        PropKind::Barricade(_) => 0.6,
        PropKind::Tomb(_) => 2.6,
        PropKind::GraveGate => 2.6,
        _ => return None,
    })
}

/// Side of the tiles the indices are built in (m), one at a time where
/// grass is asked about.
const TILE: f64 = 256.0;
/// How many tiles are kept.
const KEEP_TILES: usize = 96;

/// Where grass grows in a town, with indices to answer quickly. The indices
/// are made tile by tile from the town's streamed quarters, where asked.
pub struct Greenery {
    tiles: Mutex<BTreeMap<Key, (Arc<GreenTile>, u64)>>,
    tick: AtomicU64,
    /// Patches where the grass is thinner or thicker.
    patches: Noise,
    pub region: Region,
}

/// One tile's roads, buildings, districts and clearings.
struct GreenTile {
    roads: Vec<WorldRoad>,
    buildings: Vec<WorldBuilding>,
    bridges: Vec<Bridge>,
    /// Districts: outline and street style.
    districts: Vec<(Vec<DVec2>, StreetStyle)>,
    /// Road segments (road, segment) by cell, filed with their reach.
    road_cells: BTreeMap<Key, Vec<(u32, u32)>>,
    building_cells: BTreeMap<Key, Vec<u32>>,
    /// Discs grass keeps clear of.
    clear: BTreeMap<Key, Vec<(DVec2, f64)>>,
    /// The holes over stairs down, with their paved aprons.
    holes: Vec<StreetHole>,
    /// Plates where road ends meet ([`super::surface::road_joins`]): centre
    /// and pavement radius, by cell.
    joins: BTreeMap<Key, Vec<(DVec2, f64)>>,
}

fn tile_key(p: DVec2) -> Key {
    ((p.x / TILE).floor() as i64, (p.y / TILE).floor() as i64)
}

impl GreenTile {
    fn new(w: &TownWorld, k: Key) -> GreenTile {
        let lo = DVec2::new(k.0 as f64, k.1 as f64) * TILE;
        let hi = lo + DVec2::splat(TILE);
        let m = DVec2::splat(REACH + 4.0);
        let (glo, ghi) = (lo - m, hi + m);
        let roads = w.roads_in(glo, ghi);
        let bridges = w.bridges_in(glo, ghi);
        let mut buildings = Vec::new();
        let mut districts = Vec::new();
        let mut clear: BTreeMap<Key, Vec<(DVec2, f64)>> = BTreeMap::new();
        let mut disc = |p: DVec2, r: f64| {
            for k in keys_around(p, r + REACH) {
                clear.entry(k).or_default().push((p, r));
            }
        };
        for q in w.quarters_in(glo, ghi) {
            buildings.extend(q.buildings.iter().cloned());
            for p in &q.props {
                if let Some(r) = clearing(p) {
                    disc(p.pos, r);
                }
            }
            if let Some(st) = &q.streets {
                districts.push((st.piece.face.pts.clone(), st.plan.style));
            }
        }
        for l in &w.landmarks {
            disc(l.pos, l.plaza + 0.3);
        }
        let mut road_cells: BTreeMap<Key, Vec<(u32, u32)>> = BTreeMap::new();
        for (ri, r) in roads.iter().enumerate() {
            let reach = r.half + r.pavement + REACH;
            for (si, s) in r.pts.windows(2).enumerate() {
                let (a, b) = (
                    key(s[0].min(s[1]) - DVec2::splat(reach)),
                    key(s[0].max(s[1]) + DVec2::splat(reach)),
                );
                for x in a.0..=b.0 {
                    for z in a.1..=b.1 {
                        road_cells
                            .entry((x, z))
                            .or_default()
                            .push((ri as u32, si as u32));
                    }
                }
            }
        }
        let mut joins: BTreeMap<Key, Vec<(DVec2, f64)>> = BTreeMap::new();
        for j in super::surface::road_joins(&roads) {
            for k in keys_around(j.at, j.pavement + REACH) {
                joins.entry(k).or_default().push((j.at, j.pavement));
            }
        }
        let mut building_cells: BTreeMap<Key, Vec<u32>> = BTreeMap::new();
        for (i, b) in buildings.iter().enumerate() {
            let c = b.site.corners();
            let lo = c.iter().fold(DVec2::MAX, |m, p| m.min(*p)) - DVec2::splat(REACH);
            let hi = c.iter().fold(DVec2::MIN, |m, p| m.max(*p)) + DVec2::splat(REACH);
            let (a, z) = (key(lo), key(hi));
            for x in a.0..=z.0 {
                for y in a.1..=z.1 {
                    building_cells.entry((x, y)).or_default().push(i as u32);
                }
            }
        }
        let holes = w.street_holes(glo, ghi);
        GreenTile {
            roads,
            buildings,
            bridges,
            districts,
            road_cells,
            building_cells,
            clear,
            holes,
            joins,
        }
    }
}

impl Greenery {
    pub fn new(w: &TownWorld) -> Greenery {
        Greenery {
            tiles: Mutex::new(BTreeMap::new()),
            tick: AtomicU64::new(0),
            patches: Noise::new(w.seed.child("greenery")),
            region: super::plots::region_of(w.seed),
        }
    }

    /// The tile holding `p`, made if needed.
    fn tile(&self, w: &TownWorld, p: DVec2) -> Arc<GreenTile> {
        let k = tile_key(p);
        let t = self.tick.fetch_add(1, Ordering::Relaxed);
        if let Some(e) = self.tiles.lock().expect("tiles lock").get_mut(&k) {
            e.1 = t;
            return e.0.clone();
        }
        let tile = Arc::new(GreenTile::new(w, k));
        let mut tiles = self.tiles.lock().expect("tiles lock");
        let tile = tiles.entry(k).or_insert((tile, t)).0.clone();
        while tiles.len() > KEEP_TILES {
            let Some(old) = tiles.iter().min_by_key(|e| e.1 .1).map(|e| *e.0) else {
                break;
            };
            tiles.remove(&old);
        }
        tile
    }

    /// Is `p` on a road or pavement (within `r` of its edge)?
    pub fn on_road(&self, w: &TownWorld, p: DVec2, r: f64) -> bool {
        let t = self.tile(w, p);
        let k = key(p);
        t.road_cells.get(&k).is_some_and(|l| {
            l.iter()
                .any(|&(ri, si)| edge_distance(&t.roads[ri as usize], si as usize, p) < r)
        }) || t
            .joins
            .get(&k)
            .is_some_and(|l| l.iter().any(|(q, jr)| q.distance(p) - jr < r))
    }

    /// Is `p` within `r` of a building's footprint?
    pub fn in_building(&self, w: &TownWorld, p: DVec2, r: f64) -> bool {
        let t = self.tile(w, p);
        t.building_cells.get(&key(p)).is_some_and(|l| {
            l.iter().any(|&i| {
                let s = &t.buildings[i as usize].site;
                let d = p - s.origin;
                let (x, z) = (d.dot(s.along), d.dot(s.inward));
                x > -r && x < s.size.x + r && z > -r && z < s.size.y + r
            })
        })
    }

    /// How far `p` is from the nearest thing grass does not grow on (a
    /// road's edge, a building, a fountain, a bridge, the sand), negative
    /// inside it; exact near 0 and capped at [`REACH`]. Ground drawn
    /// where this is positive meets the roads and buildings cleanly.
    pub fn clearance(&self, w: &TownWorld, p: DVec2) -> f64 {
        let (road, other) = self.clearances(w, p);
        road.min(other)
    }

    /// [`Self::clearance`] in two parts: from the roads' edges, and from
    /// everything else (buildings, fountains, plinths, bridges, sand).
    pub fn clearances(&self, w: &TownWorld, p: DVec2) -> (f64, f64) {
        let t = self.tile(w, p);
        let mut road = REACH;
        let k = key(p);
        if let Some(l) = t.road_cells.get(&k) {
            for &(ri, si) in l {
                road = road.min(edge_distance(&t.roads[ri as usize], si as usize, p));
            }
        }
        if let Some(l) = t.joins.get(&k) {
            for (q, r) in l {
                road = road.min(q.distance(p) - r);
            }
        }
        let mut c = REACH;
        if let Some(l) = t.building_cells.get(&k) {
            for &i in l {
                let s = &t.buildings[i as usize].site;
                let d = p - s.origin;
                let (x, z) = (d.dot(s.along), d.dot(s.inward));
                // Signed distance to the footprint rectangle.
                let (hx, hz) = (s.size.x * 0.5, s.size.y * 0.5);
                let q = DVec2::new((x - hx).abs() - hx, (z - hz).abs() - hz);
                let sd = q.max(DVec2::ZERO).length() + q.x.max(q.y).min(0.0);
                c = c.min(sd);
            }
        }
        if let Some(l) = t.clear.get(&k) {
            for (q, r) in l {
                c = c.min(q.distance(p) - r);
            }
        }
        for b in &t.bridges {
            c = c.min(segment_distance(p, b.a, b.b) - b.half - 1.0);
        }
        for h in &t.holes {
            c = c.min(h.distance(p, h.apron));
        }
        // No grass grows through a spring's terraces or a ring's altar.
        let sights = w.wild_sights();
        for s in &sights.springs {
            c = c.min(s.pos.distance(p) - super::sights::TERRACES[0].0 - 0.3);
        }
        for s in &sights.stones {
            c = c.min(s.pos.distance(p) - super::sights::ALTAR.0 - 0.3);
        }
        (road, c.min(w.ground(p) - SAND_TOP))
    }

    fn on_bridge(&self, w: &TownWorld, p: DVec2) -> bool {
        self.tile(w, p)
            .bridges
            .iter()
            .any(|b| segment_distance(p, b.a, b.b) < b.half + 1.0)
    }

    fn cleared(&self, w: &TownWorld, p: DVec2) -> bool {
        let t = self.tile(w, p);
        t.clear
            .get(&key(p))
            .is_some_and(|l| l.iter().any(|(q, r)| q.distance(p) < *r))
            || t.holes.iter().any(|h| h.distance(p, h.apron + 0.1) < 0.0)
    }

    /// The street style of the district `p` lies in.
    fn style_at(&self, w: &TownWorld, p: DVec2) -> Option<StreetStyle> {
        self.tile(w, p)
            .districts
            .iter()
            .find_map(|(pts, style)| contains(pts, p).then_some(*style))
    }

    /// The kind of grass the land around `p` grows, whether or not this
    /// very spot is open (roads and buildings are [`Self::grass`]'s job).
    /// Cheap enough to ask once per few metres.
    pub fn sward_at(&self, w: &TownWorld, p: DVec2) -> Sward {
        if w.town.dystopia.as_ref().is_some_and(|d| d.contains(p)) {
            return Sward::Rough;
        }
        if let Some(style) = self.style_at(w, p) {
            return match style {
                StreetStyle::Park => Sward::Park,
                StreetStyle::Industrial => Sward::Rough,
                _ if built(&w.town, p) => Sward::Lawn,
                _ => Sward::Meadow,
            };
        }
        if built(&w.town, p) {
            Sward::Lawn
        } else {
            Sward::Meadow
        }
    }

    /// Grass at `p` given the land's kind there (from [`Self::sward_at`]),
    /// or `None` where nothing grows.
    pub fn grass(&self, w: &TownWorld, p: DVec2, sward: Sward) -> Option<Grass> {
        let h = w.ground(p);
        if h < SAND_TOP {
            return None;
        }
        if self.on_road(w, p, KERB) || self.in_building(w, p, 0.15) || self.cleared(w, p) {
            return None;
        }
        if self.on_bridge(w, p) {
            return None;
        }
        // Steep banks show earth (Engine's ground shades them so).
        let e = 1.0;
        let gx = (w.ground(p + DVec2::X * e) - w.ground(p - DVec2::X * e)) / (2.0 * e);
        let gz = (w.ground(p + DVec2::Y * e) - w.ground(p - DVec2::Y * e)) / (2.0 * e);
        let slope = (gx * gx + gz * gz).sqrt();
        if slope > 0.7 {
            return None;
        }
        let mut density = 1.0 - ((slope - 0.45) / 0.25).clamp(0.0, 1.0);
        density *= ((h - SAND_TOP) / SAND_FADE).clamp(0.0, 1.0);
        if h < DECK + 0.2 {
            return None;
        }
        let n = 0.5 + 0.5 * self.patches.fbm(p.x * 0.07, p.y * 0.07, 3);
        density *= match sward {
            Sward::Lawn => 0.85 + 0.15 * n,
            Sward::Park => 0.8 + 0.2 * n,
            Sward::Meadow => 0.7 + 0.3 * n,
            // Bare earth between the tufts.
            Sward::Rough => ((n - 0.35) * 2.2).clamp(0.0, 0.8),
        };
        (density > 0.02).then_some(Grass { sward, density })
    }
}

/// Spacing of the grid the woods are scattered on (m).
const WOOD_STEP: f64 = 7.0;

/// Too steep for a tree to stand on?
fn steep(w: &TownWorld, p: DVec2) -> bool {
    let e = 2.0;
    let slope = ((w.ground(p + DVec2::X * e) - w.ground(p - DVec2::X * e)).abs()
        + (w.ground(p + DVec2::Y * e) - w.ground(p - DVec2::Y * e)).abs())
        / (2.0 * e);
    slope > 0.9
}

/// A tree of the open country at `p`, on ground `h` high.
fn wild_tree(species: Species, p: DVec2, h: f64, rng: &mut hw_core::Rng) -> Prop {
    wild_plant(Flora::Tree(species), p, h, rng)
}

/// A tree of the wild: green, or one that stands in blossom.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Flora {
    Tree(Species),
    Bloom(Blossom),
}

fn wild_plant(kind: Flora, p: DVec2, h: f64, rng: &mut hw_core::Rng) -> Prop {
    let kind = match kind {
        Flora::Tree(s) => PropKind::Tree(
            s,
            rng.range_int(0, super::props::TREE_VARIANTS as i64) as u8,
        ),
        Flora::Bloom(b) => PropKind::Bloom(
            b,
            rng.range_int(0, super::props::BLOSSOM_VARIANTS as i64) as u8,
        ),
    };
    let dir = DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalize_or(DVec2::Y);
    Prop {
        kind,
        pos: p,
        y: h - 0.05,
        dir,
        scale: rng.range(0.75, 1.25),
    }
}

/// Where in mesa country a spot lies: what grows there depends on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Habitat {
    /// The valley floor.
    Floor,
    /// On a tableland, any tier.
    Table,
    /// The floor right under a cliff: its fallen rock lies here.
    Foot,
    /// By a river or lake.
    Bank,
}

impl Habitat {
    fn at(w: &TownWorld, planet: &crate::planet::PlanetPatch, p: DVec2, h: f64) -> Habitat {
        if w.outer_rivers().is_some_and(|o| o.canyon(p) < 0.75) {
            return Habitat::Bank;
        }
        if h - planet.lowland(p) > 25.0 {
            return Habitat::Table;
        }
        // A wall within 45 m, on any of six sides?
        let wall = (0..6).any(|k| {
            let q = p + hw_core::det::dir2(k as f64 / 6.0 * std::f64::consts::TAU) * 45.0;
            w.ground(q) - h > 25.0
        });
        if wall {
            Habitat::Foot
        } else {
            Habitat::Floor
        }
    }
}

/// What grows at a spot of mesa country. `grove` (noise, about -0.5..0.5)
/// changes slowly, so stretches of country lean to one kind: a violet
/// grove, a flame grove, plain umbrella thorns, the old woods.
fn mesa_flora(place: Habitat, grove: f64, region: Region, h: f64, rng: &mut hw_core::Rng) -> Flora {
    use Flora::{Bloom, Tree};
    // The grove's own tree, two times in three.
    let band = ((grove + 0.5) * 9.0).rem_euclid(6.0) as usize;
    let own = rng.chance(0.66);
    match place {
        Habitat::Bank => match rng.weighted(&[0.42, 0.26, 0.12, 0.2]) {
            0 => Tree(Species::Willow),
            1 => Bloom(Blossom::Gold),
            2 => Bloom(Blossom::Rose),
            _ => Tree(super::props::wild_species(region, h, rng)),
        },
        Habitat::Table => {
            if own && band == 1 {
                return Bloom(Blossom::Violet);
            }
            if own && band == 4 {
                return Bloom(Blossom::Teal);
            }
            match rng.weighted(&[0.5, 0.16, 0.14, 0.08, 0.06, 0.06]) {
                0 => Tree(Species::Juniper),
                1 => Tree(Species::Pine),
                2 => Tree(Species::Acacia),
                3 => Bloom(Blossom::Flame),
                4 => Bloom(Blossom::Teal),
                _ => Tree(Species::Baobab),
            }
        }
        Habitat::Floor | Habitat::Foot => {
            if own {
                match band {
                    0 => return Tree(Species::Acacia),
                    1 => return Bloom(Blossom::Flame),
                    2 => return Bloom(Blossom::Violet),
                    3 => return Bloom(Blossom::Rose),
                    4 => return Tree(Species::Baobab),
                    _ => {}
                }
            }
            match rng.weighted(&[0.3, 0.08, 0.08, 0.08, 0.06, 0.4]) {
                0 => Tree(Species::Acacia),
                1 => Bloom(Blossom::Flame),
                2 => Bloom(Blossom::Violet),
                3 => Bloom(Blossom::Rose),
                4 => Tree(Species::Baobab),
                _ => Tree(super::props::wild_species(region, h, rng)),
            }
        }
    }
}

/// A flowering bush; `color` (noise) picks its colour, so bushes stand in
/// drifts of one.
fn wild_bush(color: f64, p: DVec2, h: f64, rng: &mut hw_core::Rng) -> Prop {
    let n = super::props::BUSH_COLORS.len();
    let c = ((color + 0.5) * 11.0).rem_euclid(n as f64) as usize % n;
    Prop {
        kind: PropKind::Bush(
            c as u8,
            rng.range_int(0, super::props::BUSH_VARIANTS as i64) as u8,
        ),
        pos: p,
        y: h - 0.03,
        dir: DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalize_or(DVec2::Y),
        scale: rng.range(0.8, 1.5),
    }
}

/// A rock: blocks and boulders fallen under a cliff, slabs and now and then
/// a spire on a table, a lone boulder in the open.
fn wild_rock(place: Habitat, cliff: bool, p: DVec2, h: f64, rng: &mut hw_core::Rng) -> Prop {
    use super::props::Rock;
    let rock = match (place, cliff) {
        (Habitat::Table, false) => {
            [Rock::Slab, Rock::Slab, Rock::Spire, Rock::Boulder][rng.range_int(0, 4) as usize]
        }
        (_, true) | (Habitat::Foot, _) => {
            [Rock::Block, Rock::Block, Rock::Boulder][rng.range_int(0, 3) as usize]
        }
        _ => Rock::Boulder,
    };
    // Mostly a metre or two, a few as big as a house.
    let u = rng.unit();
    let scale = 0.6 + 1.6 * u + 5.0 * u * u * u * u * u * u;
    Prop {
        kind: PropKind::Rock(
            rock,
            rng.range_int(0, super::props::ROCK_VARIANTS as i64) as u8,
        ),
        pos: p,
        // Sunk a little, so it sits in the slope.
        y: h - 0.25 * scale,
        dir: DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalize_or(DVec2::Y),
        scale,
    }
}

impl Greenery {
    /// The woods, copses and lone field trees of the open country whose
    /// foot lies in `lo`..`hi`. Not street props: the client streams them
    /// in tiles around the camera (Engine's world streaming), so the
    /// country costs nothing out of range. Each tree depends only on its
    /// own grid spot, so any tiling gives the same trees.
    pub fn wild_trees(&self, w: &TownWorld, lo: DVec2, hi: DVec2) -> Vec<Prop> {
        let mut out = Vec::new();
        let half = w.half() - 20.0;
        // A town on the planet has country all round it, as far as one goes;
        // one on its own ends at its edge.
        let planet = w.planet();
        let (lo, hi) = if planet.is_some() {
            (lo, hi)
        } else {
            (lo.max(DVec2::splat(-half)), hi.min(DVec2::splat(half)))
        };
        if lo.x >= hi.x || lo.y >= hi.y {
            return out;
        }
        let seed = w.seed.child("wild-trees");
        let noise = Noise::new(seed.child("woods"));
        // Stairs down out in the country (ways up from the tube) keep their
        // hole and apron clear: no trunk stands on a step.
        let holes = w.street_holes(lo - DVec2::splat(8.0), hi + DVec2::splat(8.0));
        // The springs and the waystones stand in clearings.
        let sights = w.wild_sights().clone();
        let (i0, i1) = (
            (lo.x / WOOD_STEP).floor() as i64,
            (hi.x / WOOD_STEP).ceil() as i64,
        );
        let (j0, j1) = (
            (lo.y / WOOD_STEP).floor() as i64,
            (hi.y / WOOD_STEP).ceil() as i64,
        );
        for j in j0..j1 {
            for i in i0..i1 {
                let mut rng = seed.cell(i, j).rng();
                let p = DVec2::new(
                    (i as f64 + rng.range(0.1, 0.9)) * WOOD_STEP,
                    (j as f64 + rng.range(0.1, 0.9)) * WOOD_STEP,
                );
                if p.x < lo.x || p.x >= hi.x || p.y < lo.y || p.y >= hi.y {
                    continue;
                }
                if sights.clearing(p) {
                    continue;
                }
                let wood = noise.fbm(p.x * 0.004, p.y * 0.004, 4);
                let take = if wood > 0.32 {
                    0.7
                } else if wood > 0.22 {
                    0.22
                } else {
                    0.01
                };
                // Beyond the town the planet says how wooded the country is.
                let outside = p.x.abs().max(p.y.abs()) > half;
                if outside {
                    let Some(planet) = planet else { continue };
                    let h = w.ground(p);
                    let cover = planet.cover(p, h);
                    // Closed forest where the land is wooded, the same
                    // copses and lone trees where it is open.
                    let woods = f64::min(take * 1.25, 1.0) * cover.trees
                        + if wood > 0.1 {
                            0.5 * cover.trees * cover.trees
                        } else {
                            0.0
                        };
                    // Not in the water, the sea's or a river pool's.
                    let dry = h - w.water_level(p);
                    if dry < SAND_TOP + 0.8 {
                        continue;
                    }
                    let place = Habitat::at(w, planet, p, h);
                    let cliff = steep(w, p);
                    // Rocks lie under the cliffs and on them, and here and
                    // there on a table.
                    let rocks = match (cliff, place) {
                        (true, _) => 0.1,
                        (false, Habitat::Foot) => 0.3,
                        (false, Habitat::Table) => 0.012,
                        _ => 0.0015,
                    };
                    if rng.unit() < rocks {
                        out.push(wild_rock(place, cliff, p, h, &mut rng));
                        continue;
                    }
                    if cliff || cover.green < 0.15 {
                        continue;
                    }
                    // Mesa country is open: parkland on the floor, sparse
                    // on the tables, a ribbon of trees along the water.
                    let take = match place {
                        Habitat::Floor | Habitat::Foot => 0.5 * woods + 0.022 * cover.trees.sqrt(),
                        Habitat::Table => 0.22 * woods + 0.03 * cover.trees.sqrt(),
                        Habitat::Bank => woods.max(0.3 * cover.trees.sqrt()),
                    };
                    let roll = rng.unit();
                    if roll > take {
                        // Flowering bushes in the open, in drifts of one colour.
                        let drift = noise.fbm(p.x * 0.011 + 40.0, p.y * 0.011 - 7.0, 2);
                        if roll < take + 0.035 * cover.green * smooth(0.0, 0.25, drift) {
                            let color = noise.fbm(p.x * 0.0031 - 90.0, p.y * 0.0031 + 13.0, 2);
                            out.push(wild_bush(color, p, h, &mut rng));
                        }
                        continue;
                    }
                    // Conifers where it is cold, up the mountains.
                    let kind = if cover.temp < 6.0 && rng.chance(0.85) {
                        Flora::Tree(Species::Pine)
                    } else {
                        // Groves: a stretch of country leans to one kind.
                        let grove = noise.fbm(p.x * 0.0023 + 31.0, p.y * 0.0023 + 77.0, 2);
                        mesa_flora(place, grove, self.region, h, &mut rng)
                    };
                    out.push(wild_plant(kind, p, h, &mut rng));
                    continue;
                }
                let roll = rng.unit();
                // On the planet the town's own open land flowers like the
                // country round it: bushes in drifts between the trees.
                let bush = planet.is_some() && roll > take && {
                    let drift = noise.fbm(p.x * 0.011 + 40.0, p.y * 0.011 - 7.0, 2);
                    roll < take + 0.03 * smooth(0.0, 0.25, drift)
                };
                if (roll > take && !bush) || built(&w.town, p) {
                    continue;
                }
                let h = w.ground(p);
                if h < SAND_TOP + 0.8
                    || self.on_road(w, p, 3.0)
                    || self.in_building(w, p, 3.0)
                    || holes.iter().any(|h| h.distance(p, h.apron + 1.0) < 0.0)
                    // The atomic plant's yard is out in the country.
                    || w.landmarks.iter().any(|l| l.covers(p, 3.0))
                {
                    continue;
                }
                if steep(w, p) {
                    continue;
                }
                if bush {
                    let color = noise.fbm(p.x * 0.0031 - 90.0, p.y * 0.0031 + 13.0, 2);
                    out.push(wild_bush(color, p, h, &mut rng));
                } else if planet.is_some() {
                    let grove = noise.fbm(p.x * 0.0023 + 31.0, p.y * 0.0023 + 77.0, 2);
                    let kind = mesa_flora(Habitat::Floor, grove, self.region, h, &mut rng);
                    out.push(wild_plant(kind, p, h, &mut rng));
                } else {
                    let species = super::props::wild_species(self.region, h, &mut rng);
                    out.push(wild_tree(species, p, h, &mut rng));
                }
            }
        }
        out
    }
}

/// Is `p` within the built-up town (or a village)?
pub fn built(t: &Town, p: DVec2) -> bool {
    let g = &t.growth;
    g.time.cell_of(p).is_some_and(|(i, j)| g.built(i, j))
}

/// The era of the quarter at `p` (the newest outside the town).
pub fn era_at(t: &Town, p: DVec2) -> Era {
    let g = &t.growth;
    match g.time.cell_of(p) {
        Some((i, j)) if g.built(i, j) => g.era(i, j),
        _ => Era::Modern,
    }
}

/// How lush the grass is in a region's climate: (green, dry) 0..1.
pub fn climate(region: Region) -> (f64, f64) {
    match region {
        Region::Northern => (1.0, 0.1),
        Region::Continental => (0.8, 0.3),
        Region::Southern => (0.5, 0.65),
    }
}

/// A stable random number 0..1 for a grid spot (placing blades and tufts).
pub fn spot_unit(seed: Seed, x: i64, z: i64, salt: u32) -> f64 {
    seed.cell(x, z).child(salt).unit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::TownParams;

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn grass_grows_off_the_roads_and_out_of_the_buildings() {
        let w = TownWorld::generate(Seed(5), TownParams::sample(Seed(5), 20_000.0));
        let g = Greenery::new(&w);
        let (mut grass, mut tried) = (0, 0);
        let half = w.half() * 0.5;
        let mut rng = Seed(1).rng();
        for _ in 0..4000 {
            let p = DVec2::new(rng.range(-half, half), rng.range(-half, half));
            tried += 1;
            let s = g.sward_at(&w, p);
            if let Some(gr) = g.grass(&w, p, s) {
                grass += 1;
                assert!(gr.density > 0.0 && gr.density <= 1.0);
                assert!(!g.on_road(&w, p, 0.0), "grass on a road at {p}");
                assert!(!g.in_building(&w, p, 0.0), "grass in a building at {p}");
            }
        }
        assert!(grass * 10 > tried, "too little grass: {grass} of {tried}");
        // Every road's centre is bare.
        for r in &w.everything().roads {
            let p = (r.pts[0] + r.pts[1]) * 0.5;
            assert!(g.grass(&w, p, Sward::Lawn).is_none());
        }
    }

    #[test]
    #[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
    fn wild_trees_are_the_same_however_the_land_is_tiled() {
        let w = TownWorld::generate(Seed(5), TownParams::sample(Seed(5), 20_000.0));
        let g = Greenery::new(&w);
        let h = w.half();
        let all = g.wild_trees(&w, DVec2::splat(-h), DVec2::splat(h));
        assert!(!all.is_empty());
        let mut tiled = Vec::new();
        let t = 128.0;
        let n = (h * 2.0 / t).ceil() as i64;
        for j in 0..n {
            for i in 0..n {
                let lo = DVec2::splat(-h) + DVec2::new(i as f64, j as f64) * t;
                tiled.extend(g.wild_trees(&w, lo, lo + DVec2::splat(t)));
            }
        }
        assert_eq!(all.len(), tiled.len());
        for p in &all {
            assert!(!super::built(&w.town, p.pos));
            assert!(!g.on_road(&w, p.pos, 0.0));
        }
    }

    /// Mesa country (Arnd 2026-11-14: fitting, colourful vegetation): the
    /// land beyond the town grows trees in blossom and flowering bushes
    /// beside the green ones, rocks lie about, and the same seed plants the
    /// same country.
    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_country_beyond_the_town_is_in_flower_and_strewn_with_rocks() {
        use crate::planet::{choose_home, PlanetMap, PlanetPatch};
        let seed = Seed(1);
        let map = PlanetMap::build(seed, 96);
        let patch = std::sync::Arc::new(PlanetPatch::new(&map, choose_home(&map)));
        let params = TownParams::on_planet(seed, 20_000.0, &patch);
        let w = TownWorld::generate_on_or_stop(seed, params, None, &hw_core::Cancel::new())
            .expect("not cancelled");
        let g = Greenery::new(&w);
        let h = w.half();
        // A band of country north of the town.
        let (lo, hi) = (
            DVec2::new(-1500.0, h + 100.0),
            DVec2::new(1500.0, h + 1600.0),
        );
        let all = g.wild_trees(&w, lo, hi);
        let count = |f: &dyn Fn(PropKind) -> bool| all.iter().filter(|p| f(p.kind)).count();
        let trees = count(&|k| matches!(k, PropKind::Tree(..)));
        let blooms = count(&|k| matches!(k, PropKind::Bloom(..)));
        let bushes = count(&|k| matches!(k, PropKind::Bush(..)));
        let rocks = count(&|k| matches!(k, PropKind::Rock(..)));
        assert!(trees > 50, "{trees} green trees");
        assert!(blooms * 8 > trees, "{blooms} in blossom of {trees} trees");
        assert!(bushes > 30, "{bushes} flowering bushes");
        assert!(rocks > 20, "{rocks} rocks");
        // More than one colour of blossom, so the country is not one wash.
        let colours: std::collections::BTreeSet<u8> = all
            .iter()
            .filter_map(|p| match p.kind {
                PropKind::Bloom(b, _) => Some(b as u8),
                _ => None,
            })
            .collect();
        assert!(colours.len() >= 2, "{colours:?}");
        // Nothing grows in the water, and every rock is solid.
        for p in &all {
            assert!(
                w.ground(p.pos) > w.water_level(p.pos),
                "{p:?} stands in water"
            );
            if matches!(
                p.kind,
                PropKind::Rock(..) | PropKind::Tree(..) | PropKind::Bloom(..)
            ) {
                assert!(!p.footprints().is_empty(), "{p:?} is not solid");
            }
        }
        // The same seed, the same country.
        let again = g.wild_trees(&w, lo, hi);
        assert_eq!(all.len(), again.len());
        assert!(all
            .iter()
            .zip(&again)
            .all(|(a, b)| a.kind == b.kind && a.pos == b.pos));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn the_same_town_grows_the_same_grass() {
        let w = TownWorld::generate(Seed(9), TownParams::sample(Seed(5), 20_000.0));
        let (a, b) = (Greenery::new(&w), Greenery::new(&w));
        for k in 0..200 {
            let p = DVec2::new(k as f64 * 7.3 - 700.0, k as f64 * 3.1 - 300.0);
            let s = a.sward_at(&w, p);
            assert_eq!(s, b.sward_at(&w, p));
            assert_eq!(a.grass(&w, p, s), b.grass(&w, p, s));
        }
    }
}
