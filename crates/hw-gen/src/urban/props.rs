//! Street furniture, trees and parked cars: what stands along the streets.
//!
//! Lamps line every street, in the style of the quarter's age: cast-iron
//! lanterns in the old town, deco double lamps on the extensions' grand
//! roads, atomic saucer lamps in the newer quarters. Trees line the grand
//! roads and the garden streets (species by region), parks fill with trees
//! around a fountain, front gardens get a tree or a shrub. Benches, bins,
//! hydrants and rocket mailboxes stand on the pavements; traffic lights at
//! the junctions of main roads; golden-age cars, some of them hovering,
//! park along the side streets. Placement follows Scale's street furniture.
//!
//! Everything is plain data, from the town's seed, made after the
//! buildings: nothing stands in a building, on another road or in the water.

use super::footprint::{block, disc, ring, Footprint};
use super::growth::Era;
use super::plots::region_of;
use super::roadgrid::{key, keys_around, Key, RoadGrid};
use super::roads::RoadClass;
use super::streets::StreetStyle;
use super::world::{TownWorld, WorldRoad, CAR_HALF, DECK, PARKED_ROOM, WALKER_HALF};
use crate::building::BuildingUse;
use crate::style::Region;
use crate::water::segment_distance;
use glam::DVec2;
use hw_core::{Rng, Seed};
use std::collections::BTreeMap;

/// Street lamps, by the age of the quarter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LampStyle {
    /// A cast-iron post with a lantern on top (old towns).
    Lantern,
    /// A tall fluted post with two globes (grand roads of the extensions).
    Deco,
    /// A slim mast with a glowing saucer (the golden age).
    Atomic,
}

/// Tree species (the same ten Engine grows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Species {
    Plane,
    Linden,
    Maple,
    Oak,
    Birch,
    Pine,
    Palm,
    Cypress,
    Chestnut,
    Ginkgo,
}

/// Golden-age cars.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CarModel {
    /// A long two-tone sedan with tail fins and chrome.
    Cruiser,
    /// A small rounded car under a glass bubble.
    Bubble,
    /// A flying car, parked hovering.
    Hover,
}

/// Paint schemes for cars (two-tone).
pub const CAR_PAINTS: u8 = 8;
/// Individuals per tree species.
pub const TREE_VARIANTS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PropKind {
    Lamp(LampStyle),
    Tree(Species, u8),
    Shrub(u8),
    Bench,
    Bin,
    Hydrant,
    /// A rocket-shaped mailbox.
    Mailbox,
    TrafficLight,
    Car(CarModel, u8),
    Fountain,
    /// A cast-iron mooring bollard on a quay.
    Bollard,
    /// A round news kiosk with a saucer roof.
    Kiosk,
    /// A round advertising pillar, posters all round.
    AdPillar,
    /// A bronze hero on a stone plinth; the number picks the pose.
    Statue(u8),
    /// A piece of a barricade where a street runs into the villains'
    /// quarter ([`super::edge::barricades`]).
    Barricade(BarricadePiece),
    /// A clipped evergreen hedge, [`HEDGE_LEN`] long along its x (the
    /// number picks the bushes it is grown from).
    Hedge(u8),
}

/// Length of one piece of hedge (m); its height and depth.
pub const HEDGE_LEN: f64 = 2.4;
pub const HEDGE_HEIGHT: f64 = 1.15;
pub const HEDGE_DEPTH: f64 = 0.85;
/// Hedge bush variants.
pub const HEDGE_VARIANTS: u8 = 4;

/// The pieces barricades are made of. Each faces along the street (its
/// +z), its length across it (x).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BarricadePiece {
    /// A concrete jersey barrier, 2 m long.
    Jersey,
    /// A wooden sawhorse with a striped plank, 1.6 m.
    Sawhorse,
    /// A wall of sandbags, 1.2 m.
    Sandbags,
    /// An oil drum with a fire in it.
    Drum,
}

impl BarricadePiece {
    /// The box it fills: half its length (x), half its depth (z), its
    /// height. Bodies cannot pass through it while it stands.
    pub fn block(self) -> (f64, f64, f64) {
        match self {
            BarricadePiece::Jersey => (1.0, 0.32, 0.85),
            BarricadePiece::Sawhorse => (0.8, 0.12, 1.0),
            BarricadePiece::Sandbags => (0.6, 0.3, 0.55),
            BarricadePiece::Drum => (0.3, 0.3, 0.9),
        }
    }
}

/// Poses of [`PropKind::Statue`].
pub const STATUE_POSES: u8 = 3;

/// One thing standing in the town.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prop {
    pub kind: PropKind,
    pub pos: DVec2,
    /// Height of its foot.
    pub y: f64,
    /// Unit vector its front faces (its +z).
    pub dir: DVec2,
    pub scale: f64,
}

impl Prop {
    /// Its solid parts, in the town's frame: what stops a body, what people
    /// on foot step round and what cars keep clear of. Shrubs are soft.
    /// The sizes follow the drawn models (`hw_client::propmesh`).
    pub fn footprints(&self) -> Vec<Footprint> {
        let s = self.scale;
        let local: Vec<Footprint> = match self.kind {
            PropKind::Lamp(style) => vec![disc(
                0.16,
                match style {
                    LampStyle::Lantern => 4.8,
                    LampStyle::Deco => 6.5,
                    LampStyle::Atomic => 7.5,
                },
            )],
            PropKind::TrafficLight => vec![disc(0.16, 4.1)],
            PropKind::Tree(..) => vec![disc((0.32 * s).max(0.15), 3.0)],
            PropKind::Shrub(_) => Vec::new(),
            // A clipped hedge: dense enough to stop a body.
            PropKind::Hedge(_) => vec![block(HEDGE_LEN * 0.5, HEDGE_DEPTH * 0.5, HEDGE_HEIGHT)],
            PropKind::Bench => vec![block(0.97, 0.36, 0.95)],
            PropKind::Bin => vec![disc(0.31, 0.95)],
            PropKind::Hydrant => vec![disc(0.2, 0.7)],
            PropKind::Mailbox => vec![disc(0.36, 1.75)],
            PropKind::Bollard => vec![disc(0.2, 0.75)],
            PropKind::Kiosk => vec![disc(1.28, 3.6)],
            PropKind::AdPillar => vec![disc(0.66, 3.5)],
            PropKind::Statue(_) => vec![block(0.98, 0.98, 4.2)],
            PropKind::Fountain => {
                // The basin's rim, too high to step over, and the column.
                let mut v = ring(3.3, 0.38, 0.66, 20);
                v.push(disc(0.5, 2.8));
                v
            }
            PropKind::Car(model, _) => {
                let (hx, hz, h) = car_block(model);
                vec![block(hx, hz, h)]
            }
            PropKind::Barricade(piece) => {
                let (hx, hz, h) = piece.block();
                vec![Footprint::Box {
                    at: DVec2::ZERO,
                    dir: DVec2::Y,
                    half: DVec2::new(hx, hz),
                    y0: -0.1,
                    y1: h,
                }]
            }
        };
        let dir = self.dir.normalize_or(DVec2::Y);
        local
            .into_iter()
            .map(|f| f.placed(self.pos, dir, self.y))
            .collect()
    }
}

/// Does a car parked at `q` facing `dir` stay in the parking strip along
/// the kerb of `r`, every corner of it?
fn at_the_kerb(r: &WorldRoad, q: DVec2, dir: DVec2, model: CarModel) -> bool {
    let (hx, hz, _) = car_block(model);
    let x = DVec2::new(dir.y, -dir.x);
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .all(|&(a, b)| {
            let c = q + x * hx * a + dir * hz * b;
            let d = r
                .pts
                .windows(2)
                .map(|w| segment_distance(c, w[0], w[1]))
                .fold(f64::MAX, f64::min);
            d >= r.half - PARKED_ROOM - 0.05 && d <= r.half + 0.05
        })
}

/// The box a parked car fills: half its width, half its length, its
/// height (hover cars hang low enough that nobody passes under).
pub fn car_block(model: CarModel) -> (f64, f64, f64) {
    match model {
        CarModel::Cruiser => (1.07, 2.86, 1.6),
        CarModel::Bubble => (0.85, 1.78, 1.37),
        CarModel::Hover => (1.2, 2.4, 1.85),
    }
}

/// Keep props this far from the ends of a road (the junctions).
const END_CLEAR: f64 = 12.0;

/// Places the town's props.
pub fn plan_props(w: &TownWorld) -> Vec<Prop> {
    let mut p = Placer::new(w);
    let seed = w.seed.child("props");
    let region = region_of(w.seed);
    for (ri, r) in w.roads.iter().enumerate() {
        p.road(ri, r, region, seed.child(ri as u32));
    }
    p.quays();
    p.junctions(seed.child("junctions"));
    p.parks(region, seed.child("parks"));
    p.gardens(region, seed.child("gardens"));
    // Last, so the props before keep their numbers in towns without them.
    p.out.extend(barricade_props(w));
    // Greenery added later goes after everything else, for the same reason.
    p.hedges(region, seed.child("hedges"));
    p.back_gardens(region, seed.child("back-gardens"));
    p.old_town_trees(region, seed.child("old-town-trees"));
    p.open_greens(region, seed.child("open-greens"));
    p.out
}

/// The pieces of every barricade: jersey barriers in from both kerbs (one
/// lane left open), a striped sawhorse behind each run, sandbags across the
/// pavements and a burning drum on one of them.
fn barricade_props(w: &TownWorld) -> Vec<Prop> {
    let mut out = Vec::new();
    for b in super::edge::barricades(w) {
        let across = b.along.perp();
        let mut rng = Seed(b.seed).rng();
        let put = |piece: BarricadePiece, x: f64, z: f64, out: &mut Vec<Prop>| {
            let pos = b.at + across * x + b.along * z;
            out.push(Prop {
                kind: PropKind::Barricade(piece),
                pos,
                y: w.road_height(pos),
                dir: b.along,
                scale: 1.0,
            });
        };
        let reach = (b.half - 1.8).max(0.0);
        for sx in [-1.0f64, 1.0] {
            if reach > 0.8 {
                let n = (reach / 2.0).ceil() as usize;
                for k in 0..n {
                    // From the kerb in; the last one ends at the lane.
                    let x = (b.half - 1.0 - 2.0 * k as f64).max(b.half - reach + 1.0);
                    put(BarricadePiece::Jersey, sx * x, 0.0, &mut out);
                }
                // At the lane's edge, behind the barriers.
                let x = (b.half - reach + 0.8).min(b.half - 0.8);
                put(
                    BarricadePiece::Sawhorse,
                    sx * x,
                    1.6 + rng.range(-0.3, 0.3),
                    &mut out,
                );
            }
            let n = ((b.pavement - 0.4) / 1.2).floor() as usize;
            for k in 0..n {
                put(
                    BarricadePiece::Sandbags,
                    sx * (b.half + 0.8 + 1.2 * k as f64),
                    0.4,
                    &mut out,
                );
            }
        }
        let side = if rng.chance(0.5) { -1.0 } else { 1.0 };
        put(
            BarricadePiece::Drum,
            side * (b.half + b.pavement * 0.5),
            -1.4,
            &mut out,
        );
    }
    out
}

/// Positions along a polyline by arc length.
struct Walk<'a> {
    pts: &'a [DVec2],
    at: Vec<f64>,
}

impl<'a> Walk<'a> {
    fn new(pts: &'a [DVec2]) -> Self {
        let mut at = vec![0.0];
        for w in pts.windows(2) {
            at.push(at.last().unwrap() + w[0].distance(w[1]));
        }
        Walk { pts, at }
    }

    fn len(&self) -> f64 {
        *self.at.last().unwrap()
    }

    /// Point and unit direction at arc length `s`.
    fn point(&self, s: f64) -> (DVec2, DVec2) {
        let k = self
            .at
            .partition_point(|&a| a <= s)
            .clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[k - 1], self.pts[k]);
        let l = (self.at[k] - self.at[k - 1]).max(1e-9);
        let t = ((s - self.at[k - 1]) / l).clamp(0.0, 1.0);
        (a.lerp(b, t), (b - a).normalize_or_zero())
    }
}

struct Placer<'a> {
    w: &'a TownWorld,
    roads: RoadGrid<'a>,
    /// The street cars' lanes, segment by segment, by grid cell.
    lanes: BTreeMap<Key, Vec<(DVec2, DVec2)>>,
    buildings: BTreeMap<Key, Vec<usize>>,
    taken: BTreeMap<Key, Vec<(DVec2, f64)>>,
    out: Vec<Prop>,
}

impl<'a> Placer<'a> {
    fn new(w: &'a TownWorld) -> Self {
        let mut buildings: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
        for (i, b) in w.buildings.iter().enumerate() {
            let c = b.site.corners();
            let lo = c.iter().fold(DVec2::MAX, |m, p| m.min(*p));
            let hi = c.iter().fold(DVec2::MIN, |m, p| m.max(*p));
            let (a, z) = (key(lo), key(hi));
            for x in a.0..=z.0 {
                for y in a.1..=z.1 {
                    buildings.entry((x, y)).or_default().push(i);
                }
            }
        }
        let mut lanes: BTreeMap<Key, Vec<(DVec2, DVec2)>> = BTreeMap::new();
        for l in super::lanes::street_lanes(w) {
            for s in l.pts.windows(2) {
                let (a, z) = (key(s[0].min(s[1])), key(s[0].max(s[1])));
                for x in a.0..=z.0 {
                    for y in a.1..=z.1 {
                        lanes.entry((x, y)).or_default().push((s[0], s[1]));
                    }
                }
            }
        }
        Placer {
            w,
            roads: RoadGrid::new(w, 0.0),
            lanes,
            buildings,
            taken: BTreeMap::new(),
            out: Vec::new(),
        }
    }

    /// Does a circle of radius `r` at `p` reach onto a carriageway (of any
    /// road but `own`), or with `pavements` onto any road at all?
    fn on_road(&self, p: DVec2, r: f64, own: Option<usize>, pavements: bool) -> bool {
        self.roads.at(key(p)).iter().any(|&(ri, si)| {
            if Some(ri) == own {
                return false;
            }
            let road = &self.w.roads[ri];
            let reach = road.half + if pavements { road.pavement } else { 0.0 };
            segment_distance(p, road.pts[si], road.pts[si + 1]) < reach + r
        })
    }

    fn in_building(&self, p: DVec2, r: f64) -> bool {
        keys_around(p, r).any(|k| {
            self.buildings.get(&k).is_some_and(|list| {
                list.iter().any(|&i| {
                    let s = &self.w.buildings[i].site;
                    let d = p - s.origin;
                    let (x, z) = (d.dot(s.along), d.dot(s.inward));
                    x > -r && x < s.size.x + r && z > -r && z < s.size.y + r
                })
            })
        })
    }

    /// Does `kind` standing at `q` on `r`'s pavement leave room to walk
    /// past it, on the kerb side or the house side?
    fn passable(&self, r: &WorldRoad, q: DVec2, kind: PropKind, dir: DVec2) -> bool {
        let probe = Prop {
            kind,
            pos: q,
            y: 0.0,
            dir,
            scale: 1.0,
        };
        let line = crate::geom::Polyline::new(&r.pts);
        let (k, d) = line.sample(line.nearest(q).0);
        let side = d.perp()
            * if d.perp().dot(q - k) >= 0.0 {
                1.0
            } else {
                -1.0
            };
        probe.footprints().iter().all(|f| {
            let (lo, hi) = f.extent(k, side);
            let room = (lo - r.half).max(r.half + r.pavement - hi);
            room >= 2.0 * WALKER_HALF + 0.2
        })
    }

    fn crowded(&self, p: DVec2, r: f64) -> bool {
        keys_around(p, r + 6.0).any(|k| {
            self.taken
                .get(&k)
                .is_some_and(|l| l.iter().any(|(q, rq)| q.distance(p) < r + rq))
        })
    }

    fn dry(&self, p: DVec2) -> bool {
        self.w.ground(p) > DECK + 0.2
    }

    /// Does anything of `p` stand where a street car passes (round a
    /// corner, say)?
    fn in_a_lane(&self, p: &Prop) -> bool {
        p.footprints().iter().any(|f| {
            let reach = f.reach() + CAR_HALF + 0.1;
            keys_around(f.at(), reach).any(|k| {
                self.lanes.get(&k).is_some_and(|list| {
                    list.iter().any(|&(a, b)| {
                        segment_distance(f.at(), a, b) < reach && {
                            // Exactly: the nearest point of the lane to it.
                            let ab = b - a;
                            let t = ((f.at() - a).dot(ab) / ab.length_squared().max(1e-9))
                                .clamp(0.0, 1.0);
                            f.distance(a + ab * t) < CAR_HALF + 0.1
                        }
                    })
                })
            })
        })
    }

    /// Places `kind` if the spot is free; true if it did.
    fn put(&mut self, kind: PropKind, pos: DVec2, dir: DVec2, scale: f64, r: f64) -> bool {
        // Bollards stand on the quay's paving, over the water's edge.
        let quay = kind == PropKind::Bollard;
        if !(quay || self.dry(pos))
            || self.in_building(pos, r)
            || self.crowded(pos, r)
            || self.w.landmarks.iter().any(|l| l.covers(pos, r))
        {
            return false;
        }
        let probe = Prop {
            kind,
            pos,
            y: 0.0,
            dir,
            scale,
        };
        if self.in_a_lane(&probe) {
            return false;
        }
        let lift = match kind {
            PropKind::Car(..) => 0.11,
            _ => 0.06,
        };
        self.taken.entry(key(pos)).or_default().push((pos, r));
        self.out.push(Prop {
            kind,
            pos,
            y: if quay {
                self.w.road_height(pos) + lift
            } else {
                self.w.ground(pos) + lift
            },
            dir,
            scale,
        });
        true
    }

    fn era_at(&self, p: DVec2) -> Era {
        let g = &self.w.town.growth;
        match g.time.cell_of(p) {
            Some((i, j)) if g.built(i, j) => g.era(i, j),
            _ => Era::Modern,
        }
    }

    fn road(&mut self, ri: usize, r: &WorldRoad, region: Region, seed: Seed) {
        let walk = Walk::new(&r.pts);
        let len = walk.len();
        if len < END_CLEAR * 2.0 + 4.0 {
            return;
        }
        let mut rng = seed.rng();
        let era = self.era_at(walk.point(len * 0.5).0);
        let style = r.style;
        let class = r.class;
        // Lamps.
        let lamp = match era {
            Era::Old => LampStyle::Lantern,
            Era::Extension if r.main => LampStyle::Deco,
            Era::Extension => LampStyle::Lantern,
            Era::Modern => LampStyle::Atomic,
        };
        let lamp =
            if style == Some(StreetStyle::RetroFuture) || style == Some(StreetStyle::Superblock) {
                LampStyle::Atomic
            } else {
                lamp
            };
        let step = match (r.main, style) {
            (_, Some(StreetStyle::Lanes)) => 24.0,
            (true, _) => 28.0,
            _ => 34.0,
        };
        let both = r.main && r.half >= 4.0;
        let mut s = END_CLEAR + rng.range(0.0, step * 0.5);
        let mut side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        while s < len - END_CLEAR {
            let (p, d) = walk.point(s);
            let n = d.perp();
            let sides = if both { [1.0, -1.0] } else { [side, side] };
            for sd in &sides[..if both { 2 } else { 1 }] {
                let q = p + n * *sd * (r.half + r.kerb_spot());
                if !self.on_road(q, 0.5, Some(ri), true) {
                    self.put(PropKind::Lamp(lamp), q, -n * *sd, 1.0, 0.6);
                }
            }
            side = -side;
            s += step * if both { 1.0 } else { 0.6 };
        }
        // Trees.
        let trees = match (class, style) {
            (Some(RoadClass::Boulevard), _) => 1.0,
            (Some(RoadClass::Avenue), _) => 0.65,
            (Some(RoadClass::Quay), _) => 0.6,
            (Some(RoadClass::Country), _) => 0.25,
            (_, Some(StreetStyle::Garden)) => 0.9,
            (_, Some(StreetStyle::Baroque)) => 0.8,
            (_, Some(StreetStyle::Superblock)) => 0.75,
            (_, Some(StreetStyle::Grid)) => 0.55,
            (_, Some(StreetStyle::RetroFuture)) => 0.6,
            (_, Some(StreetStyle::Organic)) => 0.45,
            (_, Some(StreetStyle::Industrial)) => 0.1,
            _ => 0.0,
        } * if era == Era::Old { 0.2 } else { 1.0 };
        if r.pavement >= 2.2 && rng.chance(trees) {
            let main_species = street_species(region, era, &mut rng);
            let tstep = rng.range(9.0, 13.0);
            // In pits by the kerb, clear of the people walking.
            let off = r.half + r.kerb_spot().max(0.6);
            for sd in [1.0, -1.0] {
                let mut s = END_CLEAR + rng.range(0.0, tstep);
                while s < len - END_CLEAR {
                    let (p, d) = walk.point(s);
                    let q = p + d.perp() * sd * off;
                    let species = if rng.chance(0.85) {
                        main_species
                    } else {
                        street_species(region, era, &mut rng)
                    };
                    let v = rng.range_int(0, TREE_VARIANTS as i64) as u8;
                    if !self.on_road(q, 1.0, Some(ri), true) {
                        let scale = rng.range(0.55, 0.8);
                        self.put(PropKind::Tree(species, v), q, d, scale, 2.2);
                    }
                    s += tstep * rng.range(0.9, 1.15);
                }
            }
        }
        // Parked cars.
        let parking = match (r.main, class, style) {
            (true, Some(RoadClass::Avenue), _) => 0.3,
            (true, _, _) => 0.0,
            (_, _, Some(StreetStyle::Lanes | StreetStyle::Industrial | StreetStyle::Park)) => 0.0,
            (_, _, Some(StreetStyle::Garden)) => 0.3,
            _ => 0.45,
        };
        // Only where a lane each way stays free beside the parked cars.
        if parking > 0.0 && r.parking_fits() {
            for sd in [1.0, -1.0] {
                let mut s = END_CLEAR + rng.range(0.0, 6.0);
                while s < len - END_CLEAR {
                    let (p, d) = walk.point(s);
                    let q = p + d.perp() * sd * (r.half - 1.15);
                    if rng.chance(parking)
                        && !self.on_road(q, 2.6, Some(ri), false)
                        && !self.on_road(q, 1.2, Some(ri), true)
                    {
                        let model = car_model(era, &mut rng);
                        let kind = PropKind::Car(model, rng.range_int(0, CAR_PAINTS as i64) as u8);
                        // Cars face the way their side of the road drives,
                        // and keep to the kerb (not on a bend, where they
                        // would stick out into the lane).
                        if at_the_kerb(r, q, d * sd, model) {
                            self.put(kind, q, d * sd, 1.0, 3.0);
                        }
                    }
                    s += rng.range(5.8, 7.6);
                }
            }
        }
        // News kiosks on wide pavements; advertising pillars in the older
        // quarters.
        if r.pavement >= 2.2 && !matches!(style, Some(StreetStyle::Lanes | StreetStyle::Industrial))
        {
            let mut s = END_CLEAR + rng.range(20.0, 90.0);
            while s < len - END_CLEAR {
                let (p, d) = walk.point(s);
                let sd = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let n = d.perp() * sd;
                let kiosk = r.pavement >= 3.0 && (era == Era::Modern || rng.chance(0.5));
                let (kind, rad) = if kiosk {
                    (PropKind::Kiosk, 1.5)
                } else {
                    (PropKind::AdPillar, 0.7)
                };
                // Pillars belong to the older quarters, and not on every block.
                if !kiosk && (era == Era::Modern || rng.chance(0.55)) {
                    s += rng.range(110.0, 240.0);
                    continue;
                }
                let q = p + n * (r.half + r.pavement * 0.62);
                if self.passable(r, q, kind, -n) && !self.on_road(q, rad * 0.5, Some(ri), false) {
                    self.put(kind, q, -n, 1.0, rad);
                }
                s += rng.range(110.0, 240.0);
            }
        }
        // Benches, bins, hydrants, mailboxes.
        let mut s = END_CLEAR + rng.range(10.0, 30.0);
        while s < len - END_CLEAR {
            let (p, d) = walk.point(s);
            let sd = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let n = d.perp() * sd;
            let garden = style == Some(StreetStyle::Garden);
            let w = [
                1.0,
                1.0,
                if garden || r.pavement < 1.8 { 0.1 } else { 0.9 },
                0.6,
            ];
            let kind = [
                PropKind::Hydrant,
                PropKind::Bin,
                PropKind::Bench,
                PropKind::Mailbox,
            ][rng.weighted(&w)];
            let (off, dir) = match kind {
                PropKind::Bench => (r.pavement * 0.7, -n),
                _ => (0.5, d),
            };
            let q = p + n * (r.half + off);
            if self.passable(r, q, kind, dir) && !self.on_road(q, 0.6, Some(ri), true) {
                self.put(kind, q, dir, 1.0, 0.9);
            }
            s += rng.range(25.0, 65.0);
        }
    }

    /// Traffic lights where three or more main roads meet.
    fn junctions(&mut self, seed: Seed) {
        let mut ends: BTreeMap<Key, Vec<(DVec2, DVec2, usize)>> = BTreeMap::new();
        for (ri, r) in self.w.roads.iter().enumerate().filter(|(_, r)| r.main) {
            let n = r.pts.len();
            if n < 2 {
                continue;
            }
            for (p, q) in [(r.pts[0], r.pts[1]), (r.pts[n - 1], r.pts[n - 2])] {
                let k = ((p.x / 4.0).round() as i64, (p.y / 4.0).round() as i64);
                ends.entry(k)
                    .or_default()
                    .push((p, (q - p).normalize_or_zero(), ri));
            }
        }
        let mut rng = seed.rng();
        for list in ends.values().filter(|l| l.len() >= 3) {
            let clear = list
                .iter()
                .map(|&(_, _, ri)| self.w.roads[ri].half + self.w.roads[ri].pavement)
                .fold(0.0, f64::max)
                + 1.0;
            for &(p, d, ri) in list {
                let r = &self.w.roads[ri];
                // On the right of the traffic coming in.
                let q = p + d * clear + (-d).perp() * -(r.half + 0.6);
                if !self.on_road(q, 0.4, Some(ri), false) && rng.chance(0.9) {
                    self.put(PropKind::TrafficLight, q, d, 1.0, 0.5);
                }
            }
        }
    }

    /// Mooring bollards along the quay walls, clear of the steps.
    fn quays(&mut self) {
        for q in &self.w.quays {
            for k in (2..q.pts.len().saturating_sub(2)).step_by(7) {
                let p = q.pts[k] - q.out[k] * 0.45;
                if self.w.water_steps.iter().any(|s| s.pos.distance(p) < 3.5) {
                    continue;
                }
                self.put(PropKind::Bollard, p, q.out[k], 1.0, 0.3);
            }
        }
    }

    /// Parks: trees in clumps, benches, a fountain in the big ones.
    fn parks(&mut self, region: Region, seed: Seed) {
        for (pi, piece) in self.w.pieces.iter().enumerate() {
            if piece.style != StreetStyle::Park {
                continue;
            }
            for (bi, block) in piece.blocks.iter().enumerate() {
                let mut rng = seed.child(pi as u32).child(bi as u32).rng();
                let area = super::graph::signed_area(block).abs();
                let lo = block.iter().fold(DVec2::MAX, |m, p| m.min(*p));
                let hi = block.iter().fold(DVec2::MIN, |m, p| m.max(*p));
                let era = self.era_at((lo + hi) * 0.5);
                let inside = |p: DVec2| super::contains(block, p);
                if area > 1500.0 {
                    if let Some(c) = super::inner_point(block) {
                        let fountain = area > 2500.0
                            && !self.on_road(c, 6.0, None, true)
                            && self.put(PropKind::Fountain, c, DVec2::Y, 1.0, 5.0);
                        // A hero in bronze, by the fountain or in its place.
                        let d = DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0))
                            .normalize_or(DVec2::Y);
                        let q = if fountain { c + d * 9.0 } else { c };
                        if inside(q) && !self.on_road(q, 2.0, None, true) {
                            let pose = rng.range_int(0, STATUE_POSES as i64) as u8;
                            self.put(PropKind::Statue(pose), q, -d, 1.0, 1.6);
                        }
                    }
                }
                let n = (area / rng.range(110.0, 180.0)) as usize;
                for _ in 0..n.min(400) {
                    let p = DVec2::new(rng.range(lo.x, hi.x), rng.range(lo.y, hi.y));
                    if !inside(p) || self.on_road(p, 2.5, None, true) {
                        continue;
                    }
                    let kind = if rng.chance(0.8) {
                        PropKind::Tree(
                            park_species(region, era, &mut rng),
                            rng.range_int(0, TREE_VARIANTS as i64) as u8,
                        )
                    } else {
                        PropKind::Shrub(rng.range_int(0, 8) as u8)
                    };
                    let dir = DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0))
                        .normalize_or(DVec2::Y);
                    self.put(kind, p, dir, rng.range(0.7, 1.15), 3.0);
                }
                for _ in 0..((area / 700.0) as usize).min(12) {
                    let p = DVec2::new(rng.range(lo.x, hi.x), rng.range(lo.y, hi.y));
                    if inside(p) && !self.on_road(p, 1.5, None, true) {
                        let dir = DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0))
                            .normalize_or(DVec2::Y);
                        self.put(PropKind::Bench, p, dir, 1.0, 1.2);
                    }
                }
            }
        }
    }

    /// Front gardens: a tree or a shrub before most houses.
    fn gardens(&mut self, region: Region, seed: Seed) {
        for i in 0..self.w.buildings.len() {
            let site = &self.w.buildings[i].site;
            if site.usage != BuildingUse::House {
                continue;
            }
            let mut rng = seed.child(i as u32).rng();
            if !rng.chance(0.65) {
                continue;
            }
            let (origin, along, inward, size) = (site.origin, site.along, site.inward, site.size);
            let p = origin + along * size.x * rng.range(0.1, 0.9) - inward * rng.range(2.0, 3.5);
            if self.on_road(p, 1.2, None, true) {
                continue;
            }
            let era = self.era_at(p);
            let kind = if rng.chance(0.55) {
                PropKind::Tree(
                    park_species(region, era, &mut rng),
                    rng.range_int(0, TREE_VARIANTS as i64) as u8,
                )
            } else {
                PropKind::Shrub(rng.range_int(0, 8) as u8)
            };
            let scale = if matches!(kind, PropKind::Tree(..)) {
                rng.range(0.45, 0.7)
            } else {
                rng.range(0.8, 1.2)
            };
            self.put(kind, p, -inward, scale, 1.2);
        }
    }

    /// Hedges: along the front of many gardens (a gap left for the path
    /// to the door) and round some of the parks.
    fn hedges(&mut self, region: Region, seed: Seed) {
        let share = match region {
            Region::Northern => 0.45,
            Region::Continental => 0.4,
            Region::Southern => 0.25,
        };
        for i in 0..self.w.buildings.len() {
            let site = self.w.buildings[i].site.clone();
            if site.usage != BuildingUse::House {
                continue;
            }
            let mut rng = seed.child(i as u32).rng();
            if !rng.chance(share) || site.size.x < HEDGE_LEN * 2.0 {
                continue;
            }
            // How deep the front garden is: out from the facade to the
            // pavement.
            let mid = site.origin + site.along * site.size.x * 0.5;
            let mut depth = 0.0;
            while depth < 12.0 && !self.on_road(mid - site.inward * (depth + 0.5), 0.0, None, true)
            {
                depth += 0.5;
            }
            if !(2.5..12.0).contains(&depth) {
                continue;
            }
            let line = depth - 0.45 - HEDGE_DEPTH * 0.5;
            let v = rng.range_int(0, HEDGE_VARIANTS as i64) as u8;
            let n = (site.size.x / HEDGE_LEN).floor() as usize;
            let start = (site.size.x - n as f64 * HEDGE_LEN) * 0.5;
            let gap = n / 2;
            for k in 0..n {
                if k == gap {
                    continue;
                }
                let x = start + (k as f64 + 0.5) * HEDGE_LEN;
                let p = site.origin + site.along * x - site.inward * line;
                self.put_hedge(p, site.along, v);
            }
        }
        for (pi, piece) in self.w.pieces.iter().enumerate() {
            if piece.style != StreetStyle::Park {
                continue;
            }
            for (bi, block) in piece.blocks.iter().enumerate() {
                let mut rng = seed.child("park").child(pi as u32).child(bi as u32).rng();
                if !rng.chance(0.45) {
                    continue;
                }
                let v = rng.range_int(0, HEDGE_VARIANTS as i64) as u8;
                // Inset from the edge, which runs along the streets.
                let n = block.len();
                let ccw = super::graph::signed_area(block) > 0.0;
                for k in 0..n {
                    let (a, b) = (block[k], block[(k + 1) % n]);
                    let len = a.distance(b);
                    if len < HEDGE_LEN * 3.0 {
                        continue;
                    }
                    let d = (b - a) / len;
                    let inward = if ccw { d.perp() } else { -d.perp() };
                    let segs = (len / HEDGE_LEN) as usize;
                    // Entrances every so often.
                    let every = rng.range_int(5, 9) as usize;
                    for j in 1..segs.saturating_sub(1) {
                        if j % every == every / 2 {
                            continue;
                        }
                        let p = a + d * ((j as f64 + 0.5) * HEDGE_LEN);
                        // Just inside the pavement.
                        let mut off = 0.5;
                        while off < 14.0
                            && self.on_road(p + inward * off, HEDGE_DEPTH * 0.5 + 0.3, None, true)
                        {
                            off += 0.5;
                        }
                        if off >= 14.0 {
                            continue;
                        }
                        self.put_hedge(p + inward * off, d, v);
                    }
                }
            }
        }
    }

    /// One piece of hedge along `along`, if it fits.
    fn put_hedge(&mut self, p: DVec2, along: DVec2, v: u8) -> bool {
        let half = HEDGE_LEN * 0.5;
        let ends = [p - along * half * 0.9, p + along * half * 0.9];
        if !self.dry(p)
            || ends
                .iter()
                .any(|e| self.on_road(*e, 0.2, None, true) || self.in_building(*e, 0.2))
            || self.w.landmarks.iter().any(|l| l.covers(p, 1.0))
            || (self.w.ground(ends[0]) - self.w.ground(ends[1])).abs() > 0.8
        {
            return false;
        }
        // Hedges stand in a row: only the end discs count as taken, so the
        // next piece can follow, but no tree or bench lands in one.
        if self.crowded(p, 0.4) {
            return false;
        }
        for e in ends {
            self.taken.entry(key(e)).or_default().push((e, 0.55));
        }
        // The prop's front (+z) faces across the hedge.
        let dir = DVec2::new(along.y, -along.x);
        self.out.push(Prop {
            kind: PropKind::Hedge(v),
            pos: p,
            y: self.w.ground(p) - 0.05,
            dir,
            scale: 1.0,
        });
        true
    }

    /// A tree in many back gardens, behind the houses.
    fn back_gardens(&mut self, region: Region, seed: Seed) {
        for i in 0..self.w.buildings.len() {
            let site = self.w.buildings[i].site.clone();
            if site.usage != BuildingUse::House {
                continue;
            }
            let mut rng = seed.child(i as u32).rng();
            if !rng.chance(0.55) {
                continue;
            }
            let back = rng.range(3.0, 7.0);
            let p = site.origin
                + site.along * site.size.x * rng.range(0.2, 0.8)
                + site.inward * (site.size.y + back);
            if self.on_road(p, 2.0, None, true) {
                continue;
            }
            let era = self.era_at(p);
            let kind = PropKind::Tree(
                park_species(region, era, &mut rng),
                rng.range_int(0, TREE_VARIANTS as i64) as u8,
            );
            self.put(kind, p, -site.inward, rng.range(0.55, 0.9), 2.5);
        }
    }

    /// The old town keeps a few trees: by its squares and churches, where a
    /// street opens out, along its wider lanes.
    fn old_town_trees(&mut self, region: Region, seed: Seed) {
        for (ri, r) in self.w.roads.iter().enumerate() {
            let mut rng = seed.child(ri as u32).rng();
            let n = r.pts.len();
            if n < 2 {
                continue;
            }
            let mid = r.pts[n / 2];
            if self.era_at(mid) != Era::Old || r.main {
                continue;
            }
            let walk = Walk::new(&r.pts);
            let len = walk.len();
            let mut s = rng.range(6.0, 30.0);
            while s < len {
                let (p, d) = walk.point(s);
                let sd = if rng.chance(0.5) { 1.0 } else { -1.0 };
                // Just beyond the pavement, where there is room.
                let q = p + d.perp() * sd * (r.half + r.pavement + 2.2);
                if !self.on_road(q, 1.6, None, true) && !self.in_building(q, 2.0) {
                    let kind = PropKind::Tree(
                        street_species(region, Era::Old, &mut rng),
                        rng.range_int(0, TREE_VARIANTS as i64) as u8,
                    );
                    self.put(kind, q, d, rng.range(0.55, 0.85), 2.4);
                }
                s += rng.range(18.0, 45.0);
            }
        }
    }

    /// Groups of trees on the larger greens inside the town: the open
    /// middles of blocks, the land between roads and the works.
    fn open_greens(&mut self, region: Region, seed: Seed) {
        let noise = hw_core::Noise::new(seed.child("groves"));
        let half = self.w.half() - 20.0;
        let step = 9.0;
        let n = (half * 2.0 / step) as i64;
        let mut rng = seed.rng();
        for j in 0..n {
            for i in 0..n {
                let p = DVec2::new(
                    -half + (i as f64 + rng.range(0.15, 0.85)) * step,
                    -half + (j as f64 + rng.range(0.15, 0.85)) * step,
                );
                let grove = 0.5 + 0.5 * noise.fbm(p.x * 0.02, p.y * 0.02, 3);
                let take = rng.unit();
                if take > grove * grove * 0.8 || !super::greenery::built(&self.w.town, p) {
                    continue;
                }
                if self.w.ground(p) < 3.0
                    || self.on_road(p, 5.0, None, true)
                    || self.in_building(p, 4.5)
                {
                    continue;
                }
                let era = self.era_at(p);
                let kind = PropKind::Tree(
                    park_species(region, era, &mut rng),
                    rng.range_int(0, TREE_VARIANTS as i64) as u8,
                );
                let dir =
                    DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalize_or(DVec2::Y);
                self.put(kind, p, dir, rng.range(0.6, 1.05), 3.0);
            }
        }
    }
}

fn street_species(region: Region, era: Era, rng: &mut Rng) -> Species {
    use Species::*;
    if era == Era::Modern && rng.chance(0.35) {
        return Ginkgo;
    }
    let (s, w): (&[Species], &[f64]) = match region {
        Region::Northern => (&[Linden, Oak, Birch, Maple], &[0.35, 0.2, 0.2, 0.25]),
        Region::Continental => (&[Plane, Chestnut, Linden, Maple], &[0.35, 0.25, 0.2, 0.2]),
        Region::Southern => (&[Palm, Plane, Cypress, Pine], &[0.4, 0.25, 0.15, 0.2]),
    };
    s[rng.weighted(w)]
}

/// Trees of the woods and fields: conifers higher up.
pub(super) fn wild_species(region: Region, height: f64, rng: &mut Rng) -> Species {
    use Species::*;
    let high = height > 40.0;
    let (s, w): (&[Species], &[f64]) = match (region, high) {
        (Region::Northern, false) => (
            &[Oak, Birch, Pine, Linden, Maple],
            &[0.3, 0.25, 0.2, 0.1, 0.15],
        ),
        (Region::Northern, true) => (&[Pine, Birch], &[0.75, 0.25]),
        (Region::Continental, false) => (
            &[Oak, Chestnut, Linden, Maple, Pine, Birch],
            &[0.3, 0.15, 0.15, 0.15, 0.15, 0.1],
        ),
        (Region::Continental, true) => (&[Pine, Oak, Birch], &[0.6, 0.25, 0.15]),
        (Region::Southern, false) => (&[Pine, Oak, Cypress, Plane], &[0.4, 0.3, 0.2, 0.1]),
        (Region::Southern, true) => (&[Pine, Cypress], &[0.7, 0.3]),
    };
    s[rng.weighted(w)]
}

fn park_species(region: Region, era: Era, rng: &mut Rng) -> Species {
    use Species::*;
    if era == Era::Modern && rng.chance(0.2) {
        return Ginkgo;
    }
    let (s, w): (&[Species], &[f64]) = match region {
        Region::Northern => (
            &[Oak, Birch, Pine, Linden, Maple, Chestnut],
            &[0.25, 0.2, 0.15, 0.15, 0.15, 0.1],
        ),
        Region::Continental => (
            &[Oak, Plane, Chestnut, Linden, Maple, Pine],
            &[0.2, 0.2, 0.2, 0.15, 0.15, 0.1],
        ),
        Region::Southern => (
            &[Pine, Cypress, Palm, Plane, Oak],
            &[0.3, 0.25, 0.2, 0.15, 0.1],
        ),
    };
    s[rng.weighted(w)]
}

fn car_model(era: Era, rng: &mut Rng) -> CarModel {
    let w: [f64; 3] = match era {
        Era::Old => [0.35, 0.55, 0.1],
        Era::Extension => [0.5, 0.35, 0.15],
        Era::Modern => [0.35, 0.3, 0.35],
    };
    [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover][rng.weighted(&w)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::TownParams;

    #[test]
    fn props_stand_clear_of_buildings_and_water() {
        let seed = Seed(1);
        let w = TownWorld::generate(seed, TownParams::sample(seed, 40_000.0));
        let p = Placer::new(&w);
        assert!(w.props.len() > 500, "only {} props", w.props.len());
        let count = |f: fn(&PropKind) -> bool| w.props.iter().filter(|x| f(&x.kind)).count();
        assert!(count(|k| matches!(k, PropKind::Lamp(_))) > 100);
        assert!(count(|k| matches!(k, PropKind::Tree(..))) > 100);
        assert!(count(|k| matches!(k, PropKind::Car(..))) > 50);
        for x in &w.props {
            assert!(!p.in_building(x.pos, 0.2), "{:?} in a building", x.kind);
            assert!(w.ground(x.pos) > DECK, "{:?} in the water", x.kind);
        }
        // Same seed, same props.
        let again = TownWorld::generate(seed, TownParams::sample(seed, 40_000.0));
        assert_eq!(w.props, again.props);
    }
}
