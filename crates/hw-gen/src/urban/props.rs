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

use super::growth::Era;
use super::plots::region_of;
use super::roads::RoadClass;
use super::streets::StreetStyle;
use super::world::{TownWorld, WorldRoad, DECK};
use crate::building::BuildingUse;
use crate::style::Region;
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
    p.out
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

pub(super) type Key = (i64, i64);
const GRID: f64 = 32.0;

pub(super) fn key(p: DVec2) -> Key {
    ((p.x / GRID).floor() as i64, (p.y / GRID).floor() as i64)
}

pub(super) fn keys_around(p: DVec2, r: f64) -> impl Iterator<Item = Key> {
    let (a, b) = (key(p - DVec2::splat(r)), key(p + DVec2::splat(r)));
    (a.0..=b.0).flat_map(move |x| (a.1..=b.1).map(move |z| (x, z)))
}

pub(super) fn seg_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

struct Placer<'a> {
    w: &'a TownWorld,
    /// Road segments by grid cell: (road, segment).
    roads: BTreeMap<Key, Vec<(usize, usize)>>,
    buildings: BTreeMap<Key, Vec<usize>>,
    taken: BTreeMap<Key, Vec<(DVec2, f64)>>,
    out: Vec<Prop>,
}

impl<'a> Placer<'a> {
    fn new(w: &'a TownWorld) -> Self {
        let mut roads: BTreeMap<Key, Vec<(usize, usize)>> = BTreeMap::new();
        for (ri, r) in w.roads.iter().enumerate() {
            let reach = r.half + r.pavement;
            for (si, s) in r.pts.windows(2).enumerate() {
                let lo = s[0].min(s[1]) - DVec2::splat(reach);
                let hi = s[0].max(s[1]) + DVec2::splat(reach);
                let (a, b) = (key(lo), key(hi));
                for x in a.0..=b.0 {
                    for z in a.1..=b.1 {
                        roads.entry((x, z)).or_default().push((ri, si));
                    }
                }
            }
        }
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
        Placer {
            w,
            roads,
            buildings,
            taken: BTreeMap::new(),
            out: Vec::new(),
        }
    }

    /// Does a circle of radius `r` at `p` reach onto a carriageway (of any
    /// road but `own`), or with `pavements` onto any road at all?
    fn on_road(&self, p: DVec2, r: f64, own: Option<usize>, pavements: bool) -> bool {
        let Some(list) = self.roads.get(&key(p)) else {
            return false;
        };
        list.iter().any(|&(ri, si)| {
            if Some(ri) == own {
                return false;
            }
            let road = &self.w.roads[ri];
            let reach = road.half + if pavements { road.pavement } else { 0.0 };
            seg_distance(p, road.pts[si], road.pts[si + 1]) < reach + r
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
                let q = p + n * *sd * (r.half + 0.45);
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
            let off = r.half + r.pavement * 0.62;
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
        if parking > 0.0 && r.half >= 3.0 {
            let sides: &[f64] = if r.half >= 4.5 { &[1.0, -1.0] } else { &[1.0] };
            for &sd in sides {
                let mut s = END_CLEAR + rng.range(0.0, 6.0);
                while s < len - END_CLEAR {
                    let (p, d) = walk.point(s);
                    let q = p + d.perp() * sd * (r.half - 1.15);
                    if rng.chance(parking)
                        && !self.on_road(q, 2.6, Some(ri), false)
                        && !self.on_road(q, 1.2, Some(ri), true)
                    {
                        let kind = PropKind::Car(
                            car_model(era, &mut rng),
                            rng.range_int(0, CAR_PAINTS as i64) as u8,
                        );
                        // Cars face the way their side of the road drives.
                        self.put(kind, q, d * sd, 1.0, 3.0);
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
                let fits = r.pavement * 0.38 > rad * 0.6;
                if fits && !self.on_road(q, rad * 0.5, Some(ri), false) {
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
            if !self.on_road(q, 0.6, Some(ri), true) {
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
