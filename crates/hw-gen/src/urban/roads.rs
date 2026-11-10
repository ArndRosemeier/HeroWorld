//! The town's main roads: the old country roads, ring roads where walls
//! stood, quays and promenades along the water, a web of avenues traced
//! along a direction field, and bridges.
//!
//! The main roads are planned for the whole town at once and stay coarse;
//! the side streets inside each piece between them come later, piece by
//! piece.

use super::growth::{chaikin, simplify, CentreKind, Era, Growth};
use super::land::{distance_to, Cover, Land};
use super::lines::{contours, length, split_where};
use super::params::TownParams;
use super::tensor::{Basis, Field};
use super::trace::{trace, TraceSpec};
use glam::DVec2;
use hw_core::{Noise, Seed};

/// Kinds of main road.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoadClass {
    /// An old road out in the country.
    Country,
    /// A grand road: the old roads through a big town, ring roads.
    Boulevard,
    /// The web of main streets between them.
    Avenue,
    /// Along a river or the shore.
    Quay,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Road {
    pub pts: Vec<DVec2>,
    pub class: RoadClass,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub roads: Vec<Road>,
    /// Bridges, bank to bank.
    pub bridges: Vec<[DVec2; 2]>,
}

impl Network {
    pub fn generate(seed: Seed, p: &TownParams, land: &Land, growth: &Growth) -> Network {
        let seed = seed.child("roads");
        let mut rng = seed.rng();
        let g = &growth.time;
        let s = p.scale();
        let built = |q: DVec2| g.cell_of(q).is_some_and(|(i, j)| growth.built(i, j));
        let era = |q: DVec2| g.cell_of(q).map(|(i, j)| growth.era(i, j));
        let big = p.population > 250_000.0;
        let mut roads = Vec::new();

        // --- The old roads.
        for r in &growth.roads {
            let inside = r.iter().filter(|q| built(**q)).count() * 2 > r.len();
            let class = match (inside, big) {
                (false, _) => RoadClass::Country,
                (true, true) => RoadClass::Boulevard,
                (true, false) => RoadClass::Avenue,
            };
            roads.push(Road {
                pts: r.clone(),
                class,
            });
        }

        // --- Ring roads where the walls stood.
        let water_d = distance_to(&land.cover, |c| c.is_water());
        let dry = |q: DVec2| water_d.sample(q) > g.cell * 1.2;
        for &w in &growth.walls {
            let rings = contours(g, w);
            // The ring around the core: the longest line passing near it.
            let Some(ring) = rings
                .into_iter()
                .filter(|r| r.iter().any(|q| q.length() < s * 0.8))
                .max_by(|a, b| length(a).total_cmp(&length(b)))
            else {
                continue;
            };
            for piece in split_where(&ring, dry) {
                // Walls ran smoother than the houses behind them.
                let piece = chaikin(&simplify(&piece, (s * 0.04).max(g.cell)), 4);
                if length(&piece) > g.cell * 6.0 {
                    roads.push(Road {
                        pts: piece,
                        class: RoadClass::Boulevard,
                    });
                }
            }
        }

        // --- Quays and promenades in the older parts.
        let quay_at = g.cell * 0.9;
        for line in contours(&water_d, quay_at as f32) {
            let keep = |q: DVec2| {
                built(q) && matches!(era(q), Some(Era::Old | Era::Extension)) || built(q) && big
            };
            for piece in split_where(&line, keep) {
                let piece = chaikin(&simplify(&piece, g.cell * 0.5), 2);
                if length(&piece) > (s * 0.1).max(150.0) {
                    roads.push(Road {
                        pts: piece,
                        class: RoadClass::Quay,
                    });
                }
            }
        }

        // --- The web of avenues (towns big enough to need one).
        let mut bridges = Vec::new();
        if p.population >= 15_000.0 {
            let d_ext = (s * 0.15).clamp(350.0, 700.0);
            let d_mod = d_ext * 1.6;
            // Radials and rings round the core of a town with a past; the
            // villages are left to their old roads.
            let mut basis = vec![Basis::Radial {
                center: DVec2::ZERO,
                reach: s * 0.5,
                weight: 0.6 + 0.3 * growth.walls.len() as f64,
            }];
            for c in growth
                .centres
                .iter()
                .filter(|c| c.kind == CentreKind::Business)
            {
                basis.push(Basis::Grid {
                    center: c.pos,
                    angle: rng.range(0.0, std::f64::consts::PI),
                    reach: s * 0.2,
                    weight: 1.0,
                });
            }
            for r in &roads {
                basis.push(Basis::Line {
                    pts: simplify(&r.pts, 10.0),
                    reach: d_ext * 0.9,
                    weight: 1.3,
                });
            }
            for r in &land.rivers {
                basis.push(Basis::Line {
                    pts: simplify(&r.pts, 10.0),
                    reach: d_ext,
                    weight: 1.0,
                });
            }
            // Planned quarters: grids lined up with the nearest old road.
            let grids = (p.planned * (2.0 + s / 1500.0)).round() as usize;
            for _ in 0..grids * 4 {
                if basis
                    .iter()
                    .filter(|b| matches!(b, Basis::Grid { .. }))
                    .count()
                    >= grids
                {
                    break;
                }
                let at = hw_core::det::dir2(rng.range(0.0, std::f64::consts::TAU))
                    * s
                    * rng.range(0.2, 1.0);
                if !built(at) || era(at) == Some(Era::Old) {
                    continue;
                }
                let along = nearest_dir(&roads, at).unwrap_or(DVec2::X);
                basis.push(Basis::Grid {
                    center: at,
                    angle: hw_core::det::atan2(along.y, along.x),
                    reach: s * rng.range(0.15, 0.35) + 200.0,
                    weight: 1.5 + 2.0 * p.planned,
                });
            }
            let organic = 1.0 - p.planned;
            let field = Field {
                basis,
                noise: Noise::new(seed.child("wiggle")),
                wiggle_len: d_ext * 3.0,
                wiggle: Box::new(move |_| 0.45 * organic),
            };
            let inside =
                |q: DVec2| built(q) && !land.cover_at(q).is_water() && era(q) != Some(Era::Old);
            let dsep = |q: DVec2, _: bool| match era(q) {
                Some(Era::Modern) => d_mod,
                _ => d_ext,
            };
            let spec = TraceSpec {
                field: &field,
                inside: &inside,
                dsep: &dsep,
                step: d_ext / 8.0,
                max_len: s * 4.0,
                min_len: d_ext * 0.8,
                join: 1.2,
                dead_ends: 0.05,
                random_seeds: 0,
                max_turn_cos: -1.0,
            };
            let given: Vec<Vec<DVec2>> = roads.iter().map(|r| r.pts.clone()).collect();
            // Seeds all over the town, coarse to fine, so no part is missed.
            let mut seeds: Vec<DVec2> = Vec::new();
            let stride = ((d_ext / g.cell).round() as usize).max(1);
            for j in (0..g.h).step_by(stride) {
                for i in (0..g.w).step_by(stride) {
                    if growth.built(i, j) {
                        seeds.push(g.center(i, j));
                    }
                }
            }
            seeds.reverse();
            seeds.extend(growth.centres.iter().map(|c| c.pos));
            let lines = trace(&spec, &given, &seeds, &mut seed.child("trace").rng());
            // Bridges where avenues reach a river and the far bank is town.
            let gap = (d_ext * 1.5).max(p.river_width * 4.0);
            for l in &lines {
                let mut pts = simplify(&l.pts, 4.0);
                for end in [true, false] {
                    let n = pts.len();
                    if n < 2 {
                        break;
                    }
                    let (a, b) = if end {
                        (pts[n - 1], pts[n - 2])
                    } else {
                        (pts[0], pts[1])
                    };
                    let dir = (a - b).normalize_or_zero();
                    if let Some(far) =
                        across(land, a, dir, g.cell, p.river_width * 3.0 + g.cell * 3.0)
                    {
                        let mid = a.lerp(far, 0.5);
                        if built(far)
                            && bridges
                                .iter()
                                .all(|br: &[DVec2; 2]| br[0].lerp(br[1], 0.5).distance(mid) > gap)
                        {
                            bridges.push([a, far]);
                            if end {
                                pts.push(far);
                            } else {
                                pts.insert(0, far);
                            }
                        }
                    }
                }
                roads.push(Road {
                    pts,
                    class: RoadClass::Avenue,
                });
            }
        }
        // Where the old roads cross rivers.
        for r in &growth.roads {
            let mut wet_from: Option<DVec2> = None;
            for w in r.windows(2) {
                let (wa, wb) = (
                    land.cover_at(w[0]).is_water(),
                    land.cover_at(w[1]).is_water(),
                );
                if !wa && wb {
                    wet_from = Some(w[0]);
                }
                if wa && !wb {
                    if let Some(a) = wet_from.take() {
                        if matches!(land.cover_at(a.lerp(w[1], 0.5)), Cover::River(_)) {
                            bridges.push([a, w[1]]);
                        }
                    }
                }
            }
        }
        Network { roads, bridges }
    }
}

/// Direction of the road segment nearest to `p`.
fn nearest_dir(roads: &[Road], p: DVec2) -> Option<DVec2> {
    let mut best: Option<(f64, DVec2)> = None;
    for r in roads {
        for w in r.pts.windows(2) {
            let d = crate::water::segment_distance(p, w[0], w[1]);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, w[1] - w[0]));
            }
        }
    }
    best.map(|b| b.1.normalize_or_zero())
}

/// From `a` heading `dir`: if a river lies just ahead, the first dry point
/// past it (within `max`).
fn across(land: &Land, a: DVec2, dir: DVec2, step: f64, max: f64) -> Option<DVec2> {
    let st = step * 0.25;
    let mut t = st;
    let mut wet = false;
    while t <= max {
        let q = a + dir * t;
        match land.cover_at(q) {
            Cover::River(_) => wet = true,
            Cover::Land if wet => return Some(q + dir * step * 0.5),
            Cover::Land if t > step * 2.0 => return None,
            Cover::Sea | Cover::Lake => return None,
            _ => {}
        }
        t += st;
    }
    None
}
