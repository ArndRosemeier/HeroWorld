//! The town's landmarks: the Aether Spire on its civic plaza near the old
//! core, a lighthouse on the shore of a seaside town, and the mad
//! scientist's Tesla tower in his quarter.
//!
//! Each stands on a round paved plinth cleared of buildings, in a spot no
//! road crosses. The plinth's top is level; where the ground falls away its
//! side shows as a stone step.

use super::growth::CentreKind;
use super::land::Cover;
use super::props::{key, keys_around, seg_distance, Key};
use super::world::{TownWorld, WATER_LEVEL};
use crate::building::BuildingSite;
use glam::DVec2;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LandmarkKind {
    /// A golden-age tower crowned with Aether light, on the civic plaza.
    AetherSpire,
    /// A striped lighthouse on the shore.
    Lighthouse,
    /// The mad scientist's tower: an iron lattice under a crackling dome,
    /// in the middle of his quarter.
    TeslaTower,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Landmark {
    pub kind: LandmarkKind,
    pub pos: DVec2,
    /// Radius of the paved plinth.
    pub plaza: f64,
    /// Top of the plinth.
    pub base: f64,
    /// Lowest ground under the plinth's edge.
    pub low: f64,
    /// Height of the structure above the plinth.
    pub height: f64,
    /// The way it faces (the spire's entrance, the lighthouse's door).
    pub dir: DVec2,
}

impl Landmark {
    /// Does a circle of radius `r` at `p` reach onto the plinth?
    pub fn covers(&self, p: DVec2, r: f64) -> bool {
        p.distance(self.pos) < self.plaza + r
    }

    /// Does a building's footprint come within `margin` of the plinth?
    pub fn overlaps(&self, site: &BuildingSite, margin: f64) -> bool {
        let d = self.pos - site.origin;
        let (x, z) = (d.dot(site.along), d.dot(site.inward));
        let q = DVec2::new(x.clamp(0.0, site.size.x), z.clamp(0.0, site.size.y));
        DVec2::new(x, z).distance(q) < self.plaza + margin
    }
}

/// Picks the landmarks' places (buildings are cleared off them afterwards).
pub fn plan_landmarks(w: &TownWorld) -> Vec<Landmark> {
    let roads = RoadIndex::new(w);
    let core = w
        .town
        .growth
        .centres
        .iter()
        .find(|c| c.kind == CentreKind::Core)
        .map_or(DVec2::ZERO, |c| c.pos);
    let mut out = Vec::new();
    // Taller in bigger towns: 35 m in a market town, 90 m in a metropolis.
    let pop = w.town.params.population.max(1000.0);
    let spire_h = (25.0 * (hw_core::det::log10(pop) - 2.3)).clamp(30.0, 95.0);
    for plaza in [26.0, 22.0, 17.0, 14.0, 11.0] {
        if let Some(l) = best_spot(w, &roads, core, 420.0, 6.0, plaza, 3.5, |p| {
            -p.distance(core)
        }) {
            out.push(Landmark {
                kind: LandmarkKind::AetherSpire,
                height: spire_h,
                dir: (core - l.0).normalize_or(DVec2::Y),
                ..landmark_at(w, l.0, plaza)
            });
            break;
        }
    }
    if let Some(d) = &w.town.dystopia {
        // As near the quarter's middle as a clear spot allows, inside it.
        let reach = (d.radius * 0.7).min(500.0);
        let tall = (18.0 * (hw_core::det::log10(pop) - 1.0)).clamp(40.0, 75.0);
        for plaza in [15.0, 12.0, 9.0] {
            let spot = best_spot(w, &roads, d.centre, reach, 6.0, plaza, 4.0, |p| {
                if d.contains(p) {
                    -p.distance(d.centre)
                } else {
                    f64::NEG_INFINITY
                }
            });
            if let Some((p, _)) = spot {
                out.push(Landmark {
                    kind: LandmarkKind::TeslaTower,
                    height: tall,
                    dir: (core - p).normalize_or(DVec2::Y),
                    ..landmark_at(w, p, plaza)
                });
                break;
            }
        }
    }
    if w.coastal() {
        let land = &w.town.land;
        // On the shore, where the sea opens widest, not far from the town.
        let reach = (w.half() * 0.8).min(1600.0);
        let open = |p: DVec2| {
            (0..24)
                .filter(|k| {
                    let a = std::f64::consts::TAU * *k as f64 / 24.0;
                    land.cover_at(p + hw_core::det::dir2(a) * 70.0) == Cover::Sea
                })
                .count() as f64
                / 24.0
        };
        let near_sea = |p: DVec2| {
            (0..12).any(|k| {
                let a = std::f64::consts::TAU * k as f64 / 12.0;
                land.cover_at(p + hw_core::det::dir2(a) * 16.0) == Cover::Sea
            })
        };
        let plaza = 7.5;
        let spot = best_spot(w, &roads, core, reach, 8.0, plaza, 2.5, |p| {
            if !near_sea(p) {
                return f64::NEG_INFINITY;
            }
            open(p) - p.distance(core) / reach * 0.35
        });
        if let Some((p, _)) = spot {
            let sea = (0..24)
                .map(|k| hw_core::det::dir2(std::f64::consts::TAU * k as f64 / 24.0))
                .filter(|d| land.cover_at(p + *d * 40.0) == Cover::Sea)
                .sum::<DVec2>();
            out.push(Landmark {
                kind: LandmarkKind::Lighthouse,
                height: 30.0,
                // The door faces the land.
                dir: (-sea).normalize_or(DVec2::Y),
                ..landmark_at(w, p, plaza)
            });
        }
    }
    out
}

fn landmark_at(w: &TownWorld, p: DVec2, plaza: f64) -> Landmark {
    let (lo, hi) = rim(w, p, plaza);
    Landmark {
        kind: LandmarkKind::AetherSpire,
        pos: p,
        plaza,
        base: hi + 0.3,
        low: lo,
        height: 0.0,
        dir: DVec2::Y,
    }
}

/// Lowest and highest ground over a disc.
fn rim(w: &TownWorld, p: DVec2, r: f64) -> (f64, f64) {
    let mut lo = w.ground(p);
    let mut hi = lo;
    for ring in [0.5, 1.0] {
        for k in 0..16 {
            let a = std::f64::consts::TAU * k as f64 / 16.0;
            let g = w.ground(p + hw_core::det::dir2(a) * r * ring);
            lo = lo.min(g);
            hi = hi.max(g);
        }
    }
    (lo, hi)
}

/// The best scoring spot within `reach` of `centre` (on a `step` grid)
/// where a plinth of radius `plaza` stands on dry, fairly level ground clear
/// of every road.
#[allow(clippy::too_many_arguments)]
fn best_spot(
    w: &TownWorld,
    roads: &RoadIndex,
    centre: DVec2,
    reach: f64,
    step: f64,
    plaza: f64,
    slope: f64,
    score: impl Fn(DVec2) -> f64,
) -> Option<(DVec2, f64)> {
    let n = (reach / step) as i64;
    let mut best: Option<(DVec2, f64)> = None;
    for j in -n..=n {
        for i in -n..=n {
            let p = centre + DVec2::new(i as f64, j as f64) * step;
            if p.distance(centre) > reach || p.x.abs() > w.half() || p.y.abs() > w.half() {
                continue;
            }
            let s = score(p);
            if !s.is_finite() || best.is_some_and(|b| s <= b.1) {
                continue;
            }
            let (lo, hi) = rim(w, p, plaza);
            if lo < WATER_LEVEL + 1.2 || hi - lo > slope || roads.near(p, plaza + 0.8) {
                continue;
            }
            best = Some((p, s));
        }
    }
    best
}

/// Road segments by grid cell, to ask quickly whether a disc touches a road.
struct RoadIndex<'a> {
    w: &'a TownWorld,
    cells: BTreeMap<Key, Vec<(usize, usize)>>,
}

impl<'a> RoadIndex<'a> {
    fn new(w: &'a TownWorld) -> Self {
        let mut cells: BTreeMap<Key, Vec<(usize, usize)>> = BTreeMap::new();
        for (ri, r) in w.roads.iter().enumerate() {
            let reach = r.half + r.pavement;
            for (si, s) in r.pts.windows(2).enumerate() {
                let (a, b) = (
                    key(s[0].min(s[1]) - DVec2::splat(reach)),
                    key(s[0].max(s[1]) + DVec2::splat(reach)),
                );
                for x in a.0..=b.0 {
                    for z in a.1..=b.1 {
                        cells.entry((x, z)).or_default().push((ri, si));
                    }
                }
            }
        }
        RoadIndex { w, cells }
    }

    /// Does a disc reach onto any road or its pavements?
    fn near(&self, p: DVec2, r: f64) -> bool {
        keys_around(p, r).any(|k| {
            self.cells.get(&k).is_some_and(|l| {
                l.iter().any(|&(ri, si)| {
                    let road = &self.w.roads[ri];
                    seg_distance(p, road.pts[si], road.pts[si + 1]) < road.half + road.pavement + r
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    #[test]
    fn spire_stands_near_the_core_and_lighthouse_by_the_sea() {
        for s in [1u64, 3, 7] {
            let w = TownWorld::generate(Seed(s), TownParams::sample(Seed(s), 40_000.0));
            let spire = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::AetherSpire)
                .expect("a spire");
            assert!(spire.height >= 30.0);
            for b in &w.buildings {
                for l in &w.landmarks {
                    assert!(!l.overlaps(&b.site, 0.5), "a building on the {:?}", l.kind);
                }
            }
            for p in &w.props {
                for l in &w.landmarks {
                    assert!(!l.covers(p.pos, 0.0), "{:?} on the {:?}", p.kind, l.kind);
                }
            }
            let lh = w
                .landmarks
                .iter()
                .any(|l| l.kind == LandmarkKind::Lighthouse);
            assert_eq!(lh, w.coastal(), "seed {s}: lighthouse {lh}");
            let d = w.town.dystopia.as_ref().expect("a villain quarter");
            let tesla = w
                .landmarks
                .iter()
                .find(|l| l.kind == LandmarkKind::TeslaTower)
                .expect("a Tesla tower");
            assert!(
                d.contains(tesla.pos),
                "seed {s}: the tower outside the quarter"
            );
        }
    }
}
