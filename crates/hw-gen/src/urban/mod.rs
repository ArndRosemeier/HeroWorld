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
//! 7. [`plots`]: buildings along the streets of each block.
//!
//! [`world`] puts it all on the ground for the game.
//!
//! The older [`crate::city`] generator stays in use until this one can
//! replace it.

pub mod area;
pub mod dystopia;
pub mod edge;
pub mod footprint;
pub mod graph;
pub mod greenery;
pub mod growth;
pub mod land;
pub mod landmarks;
pub mod lanes;
pub mod lines;
pub mod params;
pub mod plots;
pub mod props;
pub mod quarter;
mod roadgrid;
pub mod roads;
pub mod streets;
pub mod tensor;
pub mod trace;
pub mod underground;
pub mod waterfront;
pub mod world;

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
    /// The mad scientist's quarter (none in a small town).
    pub dystopia: Option<dystopia::Dystopia>,
}

/// No piece is wider than this (m): bigger ones get district roads
/// through them, as real cities have an arterial every kilometre or so.
/// It also bounds the work of planning one piece's streets and buildings,
/// which happens only when someone comes near (see [`world`]).
pub const PIECE_MAX: f64 = 1100.0;
/// Spacing of the district roads cut through a big piece (m).
const DISTRICT_GAP: f64 = 650.0;

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
        let pieces = pieces_of(&graph, &land, &growth);
        let dystopia =
            dystopia::Dystopia::plan(seed, &pieces, &growth, params.dystopia, params.population);
        Town {
            dystopia,
            params,
            land,
            growth,
            network,
            graph,
            pieces,
        }
    }
}

/// The built faces of the main road graph.
fn pieces_of(graph: &graph::Graph, land: &Land, growth: &Growth) -> Vec<Piece> {
    let g = &growth.time;
    graph
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
        .collect()
}

/// A piece split by district roads into districts no wider than
/// [`PIECE_MAX`] (the piece itself if it is small enough), and the
/// district roads. Pure: from the town and the piece alone.
pub fn districts(seed: Seed, town: &Town, piece: &Piece) -> (Vec<Piece>, Vec<Road>) {
    let mut parts = vec![piece.clone()];
    let mut roads: Vec<Road> = Vec::new();
    let g = &town.growth.time;
    for round in 0..4u32 {
        let extra: Vec<Road> = parts
            .iter()
            .flat_map(|pc| {
                district_roads(
                    seed.child("district").child(round),
                    pc,
                    &town.land,
                    &town.params,
                )
            })
            .collect();
        if extra.is_empty() {
            break;
        }
        roads.extend(extra);
        let mut lines = vec![graph::TaggedLine {
            pts: piece
                .face
                .pts
                .iter()
                .copied()
                .chain([piece.face.pts[0]])
                .collect(),
            tag: OUTLINE,
        }];
        lines.extend(roads.iter().map(|r| graph::TaggedLine {
            pts: r.pts.clone(),
            tag: r.class as u32,
        }));
        let graph = graph::Graph::build(&lines, 1.5);
        parts = graph
            .faces(g.cell * g.cell * 0.05)
            .into_iter()
            .filter_map(|face| {
                let c = inner_point(&face.pts)?;
                if !contains(&piece.face.pts, c) {
                    return None;
                }
                let (era, owner) = match g.cell_of(c) {
                    Some((i, j)) if town.growth.built(i, j) => {
                        (town.growth.era(i, j), *town.growth.owner.at(i, j) as usize)
                    }
                    _ => (piece.era, piece.owner),
                };
                Some(Piece { face, era, owner })
            })
            .collect();
    }
    (parts, roads)
}

/// Lowest and highest corner of a piece's outline in the frame of `dir`.
fn extent_along(pts: &[DVec2], dir: DVec2) -> (f64, f64) {
    pts.iter()
        .map(|p| p.dot(dir))
        .fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
}

/// District roads across a piece wider than [`PIECE_MAX`]: a loose grid
/// lined up with its longest side, gently curving in organic towns, cut
/// off where the piece ends or the water starts.
fn district_roads(seed: Seed, piece: &Piece, land: &Land, params: &TownParams) -> Vec<Road> {
    let pts = &piece.face.pts;
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let longest = (0..n)
        .map(|k| pts[(k + 1) % n] - pts[k])
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap_or(DVec2::X)
        .normalize_or(DVec2::X);
    let axes = [longest, longest.perp()];
    if axes
        .iter()
        .all(|d| extent_along(pts, *d).1 - extent_along(pts, *d).0 <= PIECE_MAX)
    {
        return Vec::new();
    }
    let c = inner_point(pts).unwrap_or(pts[0]);
    let mut rng = seed
        .child(c.x.round() as i64)
        .child(c.y.round() as i64)
        .rng();
    let wiggle = 40.0 * (1.0 - params.planned).clamp(0.0, 1.0);
    let mut out = Vec::new();
    for (k, &u) in axes.iter().enumerate() {
        let v = axes[1 - k];
        let (u0, u1) = extent_along(pts, u);
        let (v0, v1) = extent_along(pts, v);
        let span = u1 - u0;
        if span <= PIECE_MAX {
            continue;
        }
        let cuts = (span / DISTRICT_GAP).ceil() as usize;
        let gap = span / cuts as f64;
        for i in 1..cuts {
            let at = u0 + gap * i as f64 + rng.range(-0.08, 0.08) * gap;
            let phase = rng.range(0.0, std::f64::consts::TAU);
            let wave = rng.range(260.0, 420.0);
            let point = |t: f64| {
                let off = wiggle * hw_core::det::sin(t / wave + phase);
                u * (at + off) + v * t
            };
            // Sample across, keep the runs inside the piece and on land.
            let ok = |p: DVec2| contains(pts, p) && !land.cover_at(p).is_water();
            let step = 12.0;
            let steps = ((v1 - v0) / step).ceil() as usize + 2;
            let mut run: Vec<DVec2> = Vec::new();
            let mut prev: Option<(f64, bool)> = None;
            let mut flush = |run: &mut Vec<DVec2>| {
                let len: f64 = run.windows(2).map(|w| w[0].distance(w[1])).sum();
                if run.len() >= 2 && len > 20.0 {
                    out.push(Road {
                        pts: std::mem::take(run),
                        class: RoadClass::Avenue,
                    });
                } else {
                    run.clear();
                }
            };
            for s in 0..=steps {
                let t = v0 - step + s as f64 * step;
                let inside = ok(point(t));
                if let Some((pt, was)) = prev {
                    if was != inside {
                        // The edge, to a few centimetres.
                        let (mut a, mut b) = (pt, t);
                        for _ in 0..20 {
                            let m = (a + b) * 0.5;
                            if ok(point(m)) == was {
                                a = m;
                            } else {
                                b = m;
                            }
                        }
                        let edge = point(if was { a } else { b });
                        if was {
                            run.push(edge);
                            flush(&mut run);
                        } else {
                            run.push(edge);
                        }
                    } else if inside {
                        run.push(point(t));
                    }
                }
                prev = Some((t, inside));
            }
            flush(&mut run);
        }
    }
    out
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
