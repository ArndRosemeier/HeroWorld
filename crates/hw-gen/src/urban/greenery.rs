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

use super::growth::Era;
use super::props::{Prop, PropKind};
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
        GreenTile {
            roads,
            buildings,
            bridges,
            districts,
            road_cells,
            building_cells,
            clear,
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
        (road, c.min(w.ground(p) - SAND_TOP))
    }

    fn on_bridge(&self, w: &TownWorld, p: DVec2) -> bool {
        self.tile(w, p)
            .bridges
            .iter()
            .any(|b| segment_distance(p, b.a, b.b) < b.half + 1.0)
    }

    fn cleared(&self, w: &TownWorld, p: DVec2) -> bool {
        self.tile(w, p)
            .clear
            .get(&key(p))
            .is_some_and(|l| l.iter().any(|(q, r)| q.distance(p) < *r))
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

impl Greenery {
    /// The woods, copses and lone field trees of the open country whose
    /// foot lies in `lo`..`hi`. Not street props: the client streams them
    /// in tiles around the camera (Engine's world streaming), so the
    /// country costs nothing out of range. Each tree depends only on its
    /// own grid spot, so any tiling gives the same trees.
    pub fn wild_trees(&self, w: &TownWorld, lo: DVec2, hi: DVec2) -> Vec<Prop> {
        let mut out = Vec::new();
        let half = w.half() - 20.0;
        let (lo, hi) = (lo.max(DVec2::splat(-half)), hi.min(DVec2::splat(half)));
        if lo.x >= hi.x || lo.y >= hi.y {
            return out;
        }
        let seed = w.seed.child("wild-trees");
        let noise = Noise::new(seed.child("woods"));
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
                let wood = noise.fbm(p.x * 0.004, p.y * 0.004, 4);
                let take = if wood > 0.32 {
                    0.7
                } else if wood > 0.22 {
                    0.22
                } else {
                    0.01
                };
                if rng.unit() > take || built(&w.town, p) {
                    continue;
                }
                let h = w.ground(p);
                if h < SAND_TOP + 0.8
                    || self.on_road(w, p, 3.0)
                    || self.in_building(w, p, 3.0)
                    // The atomic plant's yard is out in the country.
                    || w.landmarks.iter().any(|l| l.covers(p, 3.0))
                {
                    continue;
                }
                let e = 2.0;
                let slope = ((w.ground(p + DVec2::X * e) - w.ground(p - DVec2::X * e)).abs()
                    + (w.ground(p + DVec2::Y * e) - w.ground(p - DVec2::Y * e)).abs())
                    / (2.0 * e);
                if slope > 0.9 {
                    continue;
                }
                let species = super::props::wild_species(self.region, h, &mut rng);
                let v = rng.range_int(0, super::props::TREE_VARIANTS as i64) as u8;
                let dir =
                    DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalize_or(DVec2::Y);
                out.push(Prop {
                    kind: PropKind::Tree(species, v),
                    pos: p,
                    y: h - 0.05,
                    dir,
                    scale: rng.range(0.75, 1.25),
                });
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
