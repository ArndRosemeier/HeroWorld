//! The town's graveyards: where they lie, and which block each takes.
//!
//! Every settlement buries its dead, so every one has at least one; bigger
//! towns have more, the way real ones grew:
//!
//! - an **old burial ground** in the old town, small and crowded, its
//!   stones leaning with age;
//! - the **cemeteries** laid out when the town spread, once on its edge,
//!   now inside it: walled, with an avenue of cypresses from the gate,
//!   mausoleums along it;
//! - the golden age's own **gardens of rest** in the newest quarters: rows
//!   of streamlined markers on open lawn, rocket monuments and saucer
//!   tombs.
//!
//! The sites are picked with the town's big picture ([`super::Town`]); a
//! district that holds one gives it a block of its own when its streets are
//! planned ([`super::streets::plan_piece`]), so the graveyard is planned
//! with the district, on demand, like everything else there. No building
//! stands on that block; its stones, walls and trees are props
//! ([`super::props`]).

use super::dystopia::Dystopia;
use super::graph::signed_area;
use super::{contains, inner_point, CentreKind, Era, Growth, Piece};
use glam::DVec2;
use hw_core::Seed;

/// What kind of graveyard it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GraveKind {
    /// The old town's burial ground: small, crowded, old stones.
    BurialGround,
    /// A walled cemetery with an avenue, mausoleums and many stones.
    Cemetery,
    /// A golden-age garden of rest: low streamlined markers on open lawn.
    GardenOfRest,
}

/// Where one graveyard lies, as picked with the big picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraveSite {
    /// A point inside it (the block holding it, or the nearest, becomes
    /// the graveyard).
    pub at: DVec2,
    pub kind: GraveKind,
    /// The town piece it lies in.
    pub piece: usize,
}

/// One district's graveyard: the block it lies on and its ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Graveyard {
    pub kind: GraveKind,
    /// Index into the district's blocks; nothing is built on its ground
    /// ([`Self::outline`]).
    pub block: usize,
    /// Its ground, along street centre lines where streets bound it (the
    /// props keep off the pavements), cut down to [`max_area`] in a big
    /// block.
    pub outline: Vec<DVec2>,
    /// The side its gate faces: index of the outline's edge `k`..`k + 1`.
    pub gate_edge: usize,
    /// Its own seed.
    pub seed: Seed,
}

impl Graveyard {
    /// Is `p` on its ground?
    pub fn contains(&self, p: DVec2) -> bool {
        contains(&self.outline, p)
    }

    /// Does a building on `site` come within `margin` of its ground?
    pub fn covers(&self, site: &crate::building::BuildingSite, margin: f64) -> bool {
        let c = site.corners();
        let mid = c.iter().sum::<DVec2>() / c.len() as f64;
        c.iter()
            .chain(std::iter::once(&mid))
            .any(|p| self.contains(*p))
            || self.outline.iter().any(|p| inside_site(site, *p, margin))
            || c.iter().any(|p| self.distance(*p) < margin)
    }

    /// Distance from `p` to its edge.
    pub fn distance(&self, p: DVec2) -> f64 {
        edge_distance(&self.outline, p)
    }

    /// The gate side's direction (along the edge) and the way in (unit).
    pub fn frame(&self) -> (DVec2, DVec2) {
        let n = self.outline.len();
        let (a, b) = (
            self.outline[self.gate_edge],
            self.outline[(self.gate_edge + 1) % n],
        );
        let along = (b - a).normalize_or(DVec2::X);
        let ccw = signed_area(&self.outline) > 0.0;
        let inward = if ccw { along.perp() } else { -along.perp() };
        (along, inward)
    }
}

/// How many graveyards a town of `population` has: one for a hamlet, a
/// handful for a city, a dozen for a megalopolis.
pub fn count_for(population: f64) -> usize {
    (1.0 + (population.max(0.0) / 25_000.0).sqrt())
        .round()
        .clamp(1.0, 12.0) as usize
}

/// The most ground one graveyard takes (m²): a big block is cut down to
/// about this much around its middle.
pub fn max_area(kind: GraveKind, population: f64) -> f64 {
    match kind {
        GraveKind::BurialGround => (1_500.0 + population * 0.6).min(4_500.0),
        GraveKind::Cemetery => (14_000.0 + population / 80.0).min(48_000.0),
        GraveKind::GardenOfRest => (12_000.0 + population / 120.0).min(32_000.0),
    }
}

/// Smallest piece a graveyard is put in (m²).
const MIN_PIECE: f64 = 2_500.0;

/// Picks the town's graveyard sites: the old burial ground in the old town,
/// then cemeteries in the 19th-century extensions (out towards where their
/// edge was) and gardens of rest in the newest quarters, spread out over
/// the town. Never in the villains' quarter.
pub fn plan(
    seed: Seed,
    pieces: &[Piece],
    growth: &Growth,
    dystopia: Option<&Dystopia>,
    population: f64,
) -> Vec<GraveSite> {
    let core = growth
        .centres
        .iter()
        .find(|c| c.kind == CentreKind::Core)
        .map_or(DVec2::ZERO, |c| c.pos);
    let centres: Vec<Option<DVec2>> = pieces.iter().map(|p| inner_point(&p.face.pts)).collect();
    let usable = |i: usize, least: f64| {
        centres[i].is_some_and(|c| {
            pieces[i].face.area >= least
                && !dystopia.is_some_and(|d| d.pieces.contains(&i) || d.contains(c))
        })
    };
    let far_of = |era: Era| {
        (0..pieces.len())
            .filter(|&i| pieces[i].era == era)
            .filter_map(|i| centres[i].map(|c| c.distance(core)))
            .fold(1.0, f64::max)
    };
    let far_ext = far_of(Era::Extension);
    let mut rng = seed.child("graveyards").rng();
    let mut out: Vec<GraveSite> = Vec::new();
    let n = count_for(population);
    for k in 0..n {
        let kind = match k {
            0 => GraveKind::BurialGround,
            k if k % 3 == 2 => GraveKind::GardenOfRest,
            _ => GraveKind::Cemetery,
        };
        // Where each kind belongs, in order of preference.
        let eras: &[Era] = match kind {
            GraveKind::BurialGround => &[Era::Old, Era::Extension, Era::Modern],
            GraveKind::Cemetery => &[Era::Extension, Era::Modern, Era::Old],
            GraveKind::GardenOfRest => &[Era::Modern, Era::Extension],
        };
        let taken = |i: usize| out.iter().any(|g| g.piece == i);
        // Room for it, and a little more round it.
        let least = match kind {
            GraveKind::BurialGround => MIN_PIECE,
            _ => max_area(kind, population) * 1.3,
        };
        let mut pick = None;
        for &era in eras {
            let cands: Vec<usize> = (0..pieces.len())
                .filter(|&i| pieces[i].era == era && usable(i, least) && !taken(i))
                .collect();
            if cands.is_empty() {
                continue;
            }
            // Spread out: far from the graveyards before; a cemetery also
            // out towards the extensions' old edge, the old burial ground
            // near the core.
            let score = |i: usize, r: f64| {
                let c = centres[i].unwrap_or(core);
                let apart = out
                    .iter()
                    .map(|g| g.at.distance(c))
                    .fold(f64::MAX, f64::min)
                    .min(5_000.0);
                let d = c.distance(core);
                let place = match kind {
                    GraveKind::BurialGround => 1.0 / (1.0 + d / 150.0),
                    GraveKind::Cemetery if era == Era::Extension => {
                        0.4 + (d / far_ext).clamp(0.0, 1.0)
                    }
                    _ => 1.0,
                };
                (if out.is_empty() { 1.0 } else { apart }) * place * r
            };
            pick = cands
                .iter()
                .map(|&i| (i, score(i, rng.range(0.75, 1.0))))
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
                .map(|(i, _)| i);
            if pick.is_some() {
                break;
            }
        }
        // A town too small for a piece of that size still buries its dead:
        // the biggest piece there is.
        let pick = pick.or_else(|| {
            (0..pieces.len())
                .filter(|&i| centres[i].is_some() && !taken(i))
                .filter(|&i| !dystopia.is_some_and(|d| d.pieces.contains(&i)))
                .max_by(|&a, &b| pieces[a].face.area.total_cmp(&pieces[b].face.area))
        });
        let Some(i) = pick else { break };
        out.push(GraveSite {
            at: centres[i].expect("usable pieces have a centre"),
            kind,
            piece: i,
        });
    }
    out
}

/// The ground a cemetery or garden of rest keeps free of side streets in
/// its district, so that it gets a block of its size: a rectangle round its
/// site, lined up with the district's longest side. `None` for the old
/// burial ground (it takes a block of the old town as it is), in a park,
/// and in a district too small to leave room around it.
pub fn reserve(
    site: &GraveSite,
    district: &[DVec2],
    district_area: f64,
    population: f64,
) -> Option<Vec<DVec2>> {
    let max = max_area(site.kind, population);
    if site.kind == GraveKind::BurialGround || district_area < max * 1.6 {
        return None;
    }
    let k = longest_edge(district);
    let along = (district[(k + 1) % district.len()] - district[k]).normalize_or(DVec2::X);
    let across = along.perp();
    let rect = |c: DVec2, f: f64| {
        let (hx, hz) = ((max * 1.3).sqrt() * 0.5 * f, (max / 1.3).sqrt() * 0.5 * f);
        vec![
            c - along * hx - across * hz,
            c + along * hx - across * hz,
            c + along * hx + across * hz,
            c - along * hx + across * hz,
        ]
    };
    // Wholly inside the district, clear of the roads round it: the spot
    // nearest the site where it fits, a little smaller if it must be.
    let fits = |r: &[DVec2]| {
        (0..4).all(|i| {
            let (a, b) = (r[i], r[(i + 1) % 4]);
            let n = (a.distance(b) / 8.0).ceil().max(1.0) as usize;
            (0..n).all(|j| {
                let p = a.lerp(b, j as f64 / n as f64);
                contains(district, p) && edge_distance(district, p) > RESERVE_MARGIN
            })
        })
    };
    let (lo, hi) = district
        .iter()
        .fold((DVec2::MAX, DVec2::MIN), |(l, h), p| (l.min(*p), h.max(*p)));
    let step = 16.0;
    let mut spots: Vec<DVec2> = Vec::new();
    let mut y = lo.y;
    while y <= hi.y {
        let mut x = lo.x;
        while x <= hi.x {
            let p = DVec2::new(x, y);
            if contains(district, p) {
                spots.push(p);
            }
            x += step;
        }
        y += step;
    }
    spots.sort_by(|a, b| {
        a.distance(site.at)
            .total_cmp(&b.distance(site.at))
            .then(a.x.total_cmp(&b.x))
            .then(a.y.total_cmp(&b.y))
    });
    for f in [1.0, 0.85, 0.7] {
        if let Some(c) = std::iter::once(site.at)
            .chain(spots.iter().copied())
            .find(|&c| fits(&rect(c, f)))
        {
            return Some(rect(c, f));
        }
    }
    None
}

/// How far a reserve keeps inside its district's edge (the main roads
/// round it and their pavements), m.
const RESERVE_MARGIN: f64 = 14.0;

/// Distance from `p` to a closed outline.
fn edge_distance(pts: &[DVec2], p: DVec2) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|k| crate::water::segment_distance(p, pts[k], pts[(k + 1) % n]))
        .fold(f64::MAX, f64::min)
}

/// The graveyard of a district with outline `district` and these blocks,
/// if one of the town's `sites` lies in it; `reserve` is what
/// [`reserve`] kept free of streets there.
pub fn in_district(
    sites: &[GraveSite],
    district: &[DVec2],
    blocks: &[Vec<DVec2>],
    reserve: Option<&[DVec2]>,
    population: f64,
    seed: Seed,
) -> Option<Graveyard> {
    let site = sites.iter().find(|s| contains(district, s.at))?;
    // A block near the site (the reserve's middle) with room for it: the
    // bigger the better, up to what it takes, the nearer the better.
    let max = max_area(site.kind, population);
    let at = reserve.map_or(site.at, |r| r.iter().sum::<DVec2>() / r.len() as f64);
    let block = blocks
        .iter()
        .enumerate()
        .filter_map(|(k, b)| {
            let c = inner_point(b)?;
            let d = if contains(b, at) { 0.0 } else { c.distance(at) };
            let room = signed_area(b).abs().min(max);
            Some((k, room / (1.0 + d / 250.0)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(k, _)| k)?;
    let pts = &blocks[block];
    let area = signed_area(pts).abs();
    let square = match reserve {
        Some(r) => Some(r.to_vec()),
        None if area > max * 1.15 => {
            // A square of the most it takes, lined up with the block's
            // longest street, round the middle of the block.
            let longest = longest_edge(pts);
            let along = (pts[(longest + 1) % pts.len()] - pts[longest]).normalize_or(DVec2::X);
            let c = inner_point(pts).unwrap_or(at);
            let h = max.sqrt() * 0.5;
            let across = along.perp();
            Some(vec![
                c - along * h - across * h,
                c + along * h - across * h,
                c + along * h + across * h,
                c - along * h + across * h,
            ])
        }
        None => None,
    };
    let outline = match square {
        Some(sq) => {
            let cut = clip(pts, &sq);
            if cut.len() < 3 || signed_area(&cut).abs() < 400.0 {
                pts.clone()
            } else {
                cut
            }
        }
        None => pts.clone(),
    };
    // The gate on the longest side that runs along a street of the block
    // (a cut side is open ground).
    let on_block = |a: DVec2, b: DVec2| {
        let m = (a + b) * 0.5;
        pts.iter()
            .zip(pts.iter().cycle().skip(1))
            .any(|(p, q)| crate::water::segment_distance(m, *p, *q) < 0.5)
    };
    let n = outline.len();
    let gate_edge = (0..n)
        .filter(|&k| on_block(outline[k], outline[(k + 1) % n]))
        .max_by(|&a, &b| {
            let l = |k: usize| outline[k].distance(outline[(k + 1) % n]);
            l(a).total_cmp(&l(b)).then(b.cmp(&a))
        })
        .unwrap_or_else(|| longest_edge(&outline));
    Some(Graveyard {
        kind: site.kind,
        block,
        outline,
        gate_edge,
        seed: seed
            .child("graveyard")
            .child(site.at.x.round() as i64)
            .child(site.at.y.round() as i64),
    })
}

/// Is `p` within `margin` of a building site's rectangle?
fn inside_site(site: &crate::building::BuildingSite, p: DVec2, margin: f64) -> bool {
    let d = p - site.origin;
    let (x, z) = (d.dot(site.along), d.dot(site.inward));
    x > -margin && x < site.size.x + margin && z > -margin && z < site.size.y + margin
}

fn longest_edge(pts: &[DVec2]) -> usize {
    let n = pts.len();
    (0..n)
        .max_by(|&a, &b| {
            let l = |k: usize| pts[k].distance(pts[(k + 1) % n]);
            l(a).total_cmp(&l(b)).then(b.cmp(&a))
        })
        .unwrap_or(0)
}

/// `subject` cut by the convex polygon `clip` (Sutherland-Hodgman); both
/// counter-clockwise or both clockwise does not matter.
fn clip(subject: &[DVec2], clip: &[DVec2]) -> Vec<DVec2> {
    let ccw = signed_area(clip) > 0.0;
    let mut out = subject.to_vec();
    let n = clip.len();
    for k in 0..n {
        let (a, b) = (clip[k], clip[(k + 1) % n]);
        let inside = |p: DVec2| {
            let s = (b - a).perp_dot(p - a);
            if ccw {
                s >= 0.0
            } else {
                s <= 0.0
            }
        };
        let cross = |p: DVec2, q: DVec2| {
            let (sp, sq) = ((b - a).perp_dot(p - a), (b - a).perp_dot(q - a));
            p + (q - p) * (sp / (sp - sq))
        };
        let input = std::mem::take(&mut out);
        let m = input.len();
        for i in 0..m {
            let (p, q) = (input[i], input[(i + 1) % m]);
            match (inside(p), inside(q)) {
                (true, true) => out.push(q),
                (true, false) => out.push(cross(p, q)),
                (false, true) => {
                    out.push(cross(p, q));
                    out.push(q);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_people_more_graveyards() {
        assert_eq!(count_for(150.0), 1);
        assert_eq!(count_for(2_000.0), 1);
        assert!(count_for(100_000.0) >= 3);
        assert!(count_for(1_000_000.0) >= 6);
        assert_eq!(count_for(1e9), 12);
        let mut last = 0;
        for p in [100.0, 1e3, 1e4, 1e5, 1e6, 1e7] {
            assert!(count_for(p) >= last);
            last = count_for(p);
        }
    }

    #[test]
    fn a_big_block_is_cut_to_a_square_inside_it() {
        let block = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(400.0, 0.0),
            DVec2::new(400.0, 300.0),
            DVec2::new(0.0, 300.0),
        ];
        let square = vec![
            DVec2::new(100.0, 100.0),
            DVec2::new(500.0, 100.0),
            DVec2::new(500.0, 200.0),
            DVec2::new(100.0, 200.0),
        ];
        let cut = clip(&block, &square);
        assert!((signed_area(&cut).abs() - 300.0 * 100.0).abs() < 1e-6);
        assert!(cut
            .iter()
            .all(|p| contains(&block, *p - (*p - DVec2::new(200.0, 150.0)) * 1e-6)));
    }
}
