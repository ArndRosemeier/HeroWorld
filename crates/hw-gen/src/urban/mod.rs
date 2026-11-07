//! Settlements of every kind, from a hamlet at a ford to a megalopolis on a
//! bay, generated the way real towns grow.
//!
//! Layers, coarse to fine (see the plan "HeroWorld cities: back to the
//! planning board"):
//!
//! 1. [`params`]: the settlement's parameters, sampled with realistic odds.
//! 2. [`land`]: terrain, sea, lakes and rivers.
//! 3. [`growth`]: centres, old roads, and how far the town spread, when.
//! 4. [`roads`]: main roads (old roads, rings, quays, avenues) and bridges,
//!    the avenues traced along a direction field ([`tensor`], [`trace`]).
//! 5. [`Town`] pieces: the land between main roads ([`graph`]).
//! 6. [`streets`]: side streets and blocks per piece, in the piece's style.
//!
//! The older [`crate::city`] generator stays in use until this one can
//! replace it.

pub mod graph;
pub mod growth;
pub mod land;
pub mod lines;
pub mod params;
pub mod roads;
pub mod streets;
pub mod tensor;
pub mod trace;

pub use growth::{Centre, CentreKind, Era, Growth};
pub use land::{Cover, Land};
pub use params::{Founding, PlanStyle, Preset, Site, TownParams};
pub use roads::{Network, Road, RoadClass};

use glam::DVec2;
use hw_core::Seed;

/// Tag of the town's outline (the edge of the built-up land) in [`Town::graph`].
pub const OUTLINE: u32 = 100;

/// A piece of town between main roads (or the town's edge): a quarter whose
/// side streets are laid out on their own.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub face: graph::Face,
    pub era: Era,
    /// The centre it grew from.
    pub owner: usize,
}

/// A whole settlement, as far as it is planned.
pub struct Town {
    pub params: TownParams,
    pub land: Land,
    pub growth: Growth,
    pub network: Network,
    /// Main roads and the town's outline as one planar graph; edge tags are
    /// [`RoadClass`] as `u32`, or [`OUTLINE`].
    pub graph: graph::Graph,
    pub pieces: Vec<Piece>,
}

impl Town {
    pub fn generate(seed: Seed, params: TownParams) -> Town {
        let land = Land::generate(seed, &params);
        let growth = Growth::generate(seed, &params, &land);
        let network = Network::generate(seed, &params, &land, &growth);
        let g = &growth.time;
        // The outline: the edge of built land, villages and shores included.
        let mut mask = land::Grid::new(g.half(), g.cell, 1.0f32);
        for j in 0..g.h {
            for i in 0..g.w {
                if growth.built(i, j) && !land.cover.at(i, j).is_water() {
                    mask.set(i, j, 0.0);
                }
            }
        }
        let mut lines: Vec<graph::TaggedLine> = network
            .roads
            .iter()
            .map(|r| graph::TaggedLine {
                pts: r.pts.clone(),
                tag: r.class as u32,
            })
            .collect();
        for ring in lines::contours(&mask, 0.5) {
            let pts = growth::chaikin(&growth::simplify(&ring, g.cell * 0.5), 2);
            lines.push(graph::TaggedLine { pts, tag: OUTLINE });
        }
        let graph = graph::Graph::build(&lines, 1.5);
        let pieces = graph
            .faces(g.cell * g.cell * 0.5)
            .into_iter()
            .filter_map(|face| {
                let c = inner_point(&face.pts)?;
                let (i, j) = g.cell_of(c)?;
                (growth.built(i, j) && !land.cover.at(i, j).is_water()).then(|| Piece {
                    era: growth.era(i, j),
                    owner: *growth.owner.at(i, j) as usize,
                    face,
                })
            })
            .collect();
        Town {
            params,
            land,
            growth,
            network,
            graph,
            pieces,
        }
    }
}

/// A point inside a simple polygon (the centroid, or failing that the middle
/// of the widest horizontal span through it).
pub fn inner_point(pts: &[DVec2]) -> Option<DVec2> {
    let a = graph::signed_area(pts);
    if a.abs() < 1e-9 {
        return None;
    }
    let n = pts.len();
    let mut c = DVec2::ZERO;
    for k in 0..n {
        let (p, q) = (pts[k], pts[(k + 1) % n]);
        c += (p + q) * p.perp_dot(q);
    }
    let c = c / (6.0 * a);
    if contains(pts, c) {
        return Some(c);
    }
    let y = c.y;
    let mut xs: Vec<f64> = (0..n)
        .filter_map(|k| {
            let (p, q) = (pts[k], pts[(k + 1) % n]);
            ((p.y <= y) != (q.y <= y)).then(|| p.x + (y - p.y) / (q.y - p.y) * (q.x - p.x))
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.chunks(2)
        .filter(|w| w.len() == 2)
        .max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0])))
        .map(|w| DVec2::new((w[0] + w[1]) * 0.5, y))
}

/// Even-odd point in polygon.
pub fn contains(pts: &[DVec2], p: DVec2) -> bool {
    let n = pts.len();
    let mut inside = false;
    for k in 0..n {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        if (a.y <= p.y) != (b.y <= p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}
