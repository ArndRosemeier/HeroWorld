//! The ground people stand on, as it is drawn: the terrain, and the roads
//! laid over it a few centimetres up (pavements, then the carriageway a
//! little higher, each road at one of four heights so crossing roads never
//! fight).
//!
//! The client draws road strips from [`WorldRoad::strip_points`] and
//! [`WorldRoad::pavement_lift`]/[`WorldRoad::carriage_lift`]; collision,
//! cars and walkers stand on [`TownWorld::surface`], which reads the same
//! strips. One description of the road surface, so feet neither sink into
//! the asphalt nor float above it.

use super::world::{TownWorld, WorldRoad};
use glam::DVec2;

/// Longest stretch of road drawn as one flat piece (m): between its ends
/// the surface follows the ground.
pub const ROAD_STEP: f64 = 3.0;
/// Edge of a square of road surface kept for [`TownWorld::surface`] (m).
pub(super) const SURFACE_TILE: f64 = 64.0;
/// Tiles of road surface kept at once (about 4 km² around the people).
pub(super) const KEEP_SURFACE_TILES: usize = 1024;

impl WorldRoad {
    /// Which of [`LAYERS`] heights the road is drawn at, so roads that meet
    /// never fight ([`assign_layers`]).
    pub fn layer(&self) -> usize {
        self.layer as usize % LAYERS
    }

    /// Height of the pavement's top above [`TownWorld::road_height`].
    pub fn pavement_lift(&self) -> f64 {
        ROAD_BASE + self.layer() as f64 * LAYER_STEP
    }

    /// Height of the carriageway's top above [`TownWorld::road_height`]:
    /// a kerb's step above the pavement.
    pub fn carriage_lift(&self) -> f64 {
        self.pavement_lift() + KERB
    }

    /// The centre line cut into steps of at most [`ROAD_STEP`], each point
    /// with its sideways direction: offset `o` (left negative) lies at
    /// `p + n * o`. At bends `n` is mitred (longer, so the strip keeps its
    /// width), limited at sharp turns; the points before and after a bend
    /// slide along the edges towards the mitre, so for any offset up to
    /// the road's full width the strip never folds back over itself.
    pub fn strip_points(&self) -> Vec<(DVec2, DVec2)> {
        let total = self.half + self.pavement;
        // Bends too tight for the road's width are drawn eased.
        let pts = &ease_bends(&self.pts, total);
        let m = pts.len();
        let mut out = Vec::new();
        if m < 2 {
            return out;
        }
        let dir = |i: usize| (pts[i + 1] - pts[i]).normalize_or_zero();
        // A ring road closes on itself: its two ends meet in a corner
        // like any other bend, not square ends lying over each other.
        let closed = m >= 4 && pts[0].distance(pts[m - 1]) < 0.05;
        let prev = |i: usize| match i {
            0 if closed => Some(dir(m - 2)),
            0 => None,
            _ => Some(dir(i - 1)),
        };
        let next = |i: usize| {
            if i + 2 < m {
                Some(dir(i + 1))
            } else if closed {
                Some(dir(0))
            } else {
                None
            }
        };
        let mitre = |d0: DVec2, d1: DVec2| -> DVec2 {
            let n = (d0 + d1).normalize_or_zero().perp();
            let d = if d1 != DVec2::ZERO { d1 } else { d0 };
            n / n.dot(d.perp()).max(0.5)
        };
        // How far along leg `i` its corners' mitres reach (in units of the
        // leg's direction per metre of offset): the corner at the start
        // and the one at the end.
        let lean = |i: usize| -> (f64, f64) {
            let d = dir(i);
            let start = prev(i).map_or(0.0, |p| mitre(p, d).dot(d));
            let end = next(i).map_or(0.0, |n| mitre(d, n).dot(d));
            (start, end)
        };
        for i in 0..m - 1 {
            let (a, b) = (pts[i], pts[i + 1]);
            let d = dir(i);
            let len = a.distance(b);
            // Near a corner the points slide along the strip's edges
            // towards the mitre, so the inside of a bend never folds back
            // over itself (that left holes and spikes at junctions): the
            // slide starts a little further out than the mitre reaches.
            let (ka, kb) = lean(i);
            let (mut la, mut lb) = (total * ka.abs() * EASE_ROOM, total * kb.abs() * EASE_ROOM);
            if la + lb > len {
                let f = len / (la + lb);
                la *= f;
                lb *= f;
            }
            let steps = (len / ROAD_STEP).ceil().max(1.0) as usize;
            let mut us: Vec<f64> = (0..steps).map(|j| len * j as f64 / steps as f64).collect();
            // Where the slide starts, so the edges keep their exact line.
            for u in [la, len - lb] {
                if u > 1e-3 && u < len - 1e-3 && us.iter().all(|v| (v - u).abs() > 0.25) {
                    us.push(u);
                }
            }
            us.sort_by(f64::total_cmp);
            for u in us {
                let wa = if la > 1e-9 {
                    (1.0 - u / la).max(0.0)
                } else {
                    0.0
                };
                let wb = if lb > 1e-9 {
                    (1.0 - (len - u) / lb).max(0.0)
                } else {
                    0.0
                };
                let n = match prev(i) {
                    Some(p) if u == 0.0 => mitre(p, d),
                    _ => d.perp() + d * (ka * wa + kb * wb),
                };
                out.push((a + d * u, n));
            }
        }
        let last = dir(m - 2);
        out.push((pts[m - 1], mitre(last, next(m - 2).unwrap_or(DVec2::ZERO))));
        out
    }
}

/// How far the mitre at the bend from direction `d0` to `d1` reaches
/// along the road, per metre of offset (0 on a straight).
fn mitre_lean(d0: DVec2, d1: DVec2) -> f64 {
    let n = (d0 + d1).normalize_or_zero().perp();
    (n / n.dot(d1.perp()).max(0.5)).dot(d1).abs()
}

/// A road's centre line as its strip is drawn ([`WorldRoad::strip_points`]):
/// the bends taken out that are too tight for a road `total` metres wide
/// each side: where two bends sit closer than
/// their mitres reach, the short leg between them is drawn in to one
/// point. Without this the inside of the strip folds back over itself
/// (holes and spikes where a street wiggles). Ends stay where they are.
pub fn ease_bends(pts: &[DVec2], total: f64) -> Vec<DVec2> {
    let mut pts: Vec<DVec2> = pts.to_vec();
    pts.dedup_by(|a, b| a.distance(*b) < 0.05);
    for _ in 0..pts.len() {
        let m = pts.len();
        if m < 3 {
            break;
        }
        let dir = |i: usize| (pts[i + 1] - pts[i]).normalize_or_zero();
        let lean = |k: usize| {
            if k == 0 || k + 1 >= m {
                0.0
            } else {
                mitre_lean(dir(k - 1), dir(k))
            }
        };
        let tight = (0..m - 1).find(|&i| {
            let need = total * (lean(i) + lean(i + 1)) * EASE_ROOM;
            need > pts[i].distance(pts[i + 1])
        });
        let Some(i) = tight else { break };
        if i == 0 {
            pts.remove(1);
        } else if i + 2 == m {
            pts.remove(i);
        } else {
            pts[i] = pts[i].lerp(pts[i + 1], 0.5);
            pts.remove(i + 1);
        }
    }
    pts
}

/// The lowest layer's pavement over [`TownWorld::road_height`] (m).
pub const ROAD_BASE: f64 = 0.032;
/// How many heights roads are drawn at.
pub const LAYERS: usize = 5;
/// Between one road height and the next (m): the 1 cm the z-fighting
/// check asks for, and room for strips draped on the ground in different
/// steps to bend apart a little.
pub const LAYER_STEP: f64 = 0.016;
/// The kerb, from pavement up to carriageway (m): a step more than every
/// layer's pavement together, so a plate's carriageway
/// ([`RoadJoin`], a step under the lowest road's) clears every pavement.
pub const KERB: f64 = LAYER_STEP * (LAYERS as f64 + 1.0);

/// The box round `roads`, pavements and all.
pub fn roads_box(roads: &[WorldRoad]) -> (DVec2, DVec2) {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for r in roads {
        let (a, b) = road_box(r);
        lo = lo.min(a);
        hi = hi.max(b);
    }
    (lo, hi)
}

fn road_box(r: &WorldRoad) -> (DVec2, DVec2) {
    let t = DVec2::splat(r.half + r.pavement + 0.5);
    let lo = r
        .pts
        .iter()
        .fold(DVec2::splat(f64::INFINITY), |a, p| a.min(*p));
    let hi = r
        .pts
        .iter()
        .fold(DVec2::splat(f64::NEG_INFINITY), |a, p| a.max(*p));
    (lo - t, hi + t)
}

/// Do the drawn strips of `a` and `b` overlap (by more than a touch along
/// an edge or a flat end)? Points across `a`'s strip about a metre apart
/// are tried inside `b`'s, and the other way round.
pub fn roads_meet(a: &WorldRoad, b: &WorldRoad) -> bool {
    overlap_of(a, b) > 0
}

/// How much the strips of `a` and `b` overlap: the points about a metre
/// apart over either that lie inside the other (about the overlap in m²,
/// counted from both sides).
fn overlap_of(a: &WorldRoad, b: &WorldRoad) -> usize {
    let n = samples(a).filter(|p| inside_strip(b, *p)).count()
        + samples(b).filter(|p| inside_strip(a, *p)).count();
    // Ends meeting at an angle overlap in a wedge the samples may miss:
    // counted as about the corner's size, so it weighs like any overlap.
    match ends_meet(a, b) {
        Some(true) => {
            let w = (a.half + a.pavement).min(b.half + b.pavement);
            n.max((w * w).ceil() as usize)
        }
        // Running straight on into a road of another width: they meet
        // along the end.
        Some(false) if (a.half - b.half).abs() > 1e-6 => n.max(1),
        Some(false) | None => n,
    }
}

/// Does an end of `a` meet an end of `b`: at an angle (`Some(true)`),
/// running straight on (`Some(false)`), or not at all (`None`)?
fn ends_meet(a: &WorldRoad, b: &WorldRoad) -> Option<bool> {
    let ends = |r: &WorldRoad| {
        let m = r.pts.len();
        [
            (r.pts[0], (r.pts[1] - r.pts[0]).normalize_or_zero()),
            (
                r.pts[m - 1],
                (r.pts[m - 2] - r.pts[m - 1]).normalize_or_zero(),
            ),
        ]
    };
    if a.pts.len() < 2 || b.pts.len() < 2 {
        return None;
    }
    let mut found = None;
    for (p, d) in ends(a) {
        for (q, e) in ends(b) {
            if p.distance(q) <= JOIN_SNAP {
                // Even a hair off straight leaves a sliver of overlap.
                let angled = d.dot(e) > -0.99999;
                found = Some(found.unwrap_or(false) || angled);
            }
        }
    }
    found
}

/// Points over `r`'s strip, about a metre apart, its edges and ends
/// included.
fn samples(r: &WorldRoad) -> impl Iterator<Item = DVec2> + '_ {
    let total = r.half + r.pavement;
    let across = (2.0 * total).ceil().max(1.0) as usize;
    r.pts.windows(2).flat_map(move |w| {
        let d = (w[1] - w[0]).normalize_or_zero();
        let n = d.perp();
        let along = w[0].distance(w[1]).ceil().max(1.0) as usize;
        (0..=along).flat_map(move |i| {
            let c = w[0].lerp(w[1], i as f64 / along as f64);
            (0..=across).map(move |k| c + n * (-total + 2.0 * total * k as f64 / across as f64))
        })
    })
}

/// Is `p` well inside `r`'s strip (5 cm in from its edges and its flat
/// ends; bends taken round)?
fn inside_strip(r: &WorldRoad, p: DVec2) -> bool {
    within_strip(r, p, r.half + r.pavement, 0.05)
}

/// Is `p` within `width` of `r`'s centre line, `margin` in from that edge
/// and from the flat ends (bends taken round, which the drawn strip,
/// mitred, covers)?
pub fn within_strip(r: &WorldRoad, p: DVec2, width: f64, margin: f64) -> bool {
    let reach = width - margin;
    let m = r.pts.len();
    r.pts.windows(2).enumerate().any(|(i, w)| {
        let v = w[1] - w[0];
        let len = v.length();
        if len < 1e-9 {
            return false;
        }
        let d = v / len;
        let t = (p - w[0]).dot(d);
        let lo = if i == 0 { margin } else { f64::NEG_INFINITY };
        let hi = if i + 2 == m {
            len - margin
        } else {
            f64::INFINITY
        };
        if t < lo || t > hi {
            return false;
        }
        let q = w[0] + d * t.clamp(0.0, len);
        // Past an inner corner, round about it.
        p.distance(q) < reach
    })
}

/// Gives each of `roads` a height ([`WorldRoad::layer`]) that no road it
/// meets has, among `fixed` and the others: roads that overlap at a
/// junction then never lie in one plane. The most hemmed-in road chooses
/// first (the one meeting the most heights already taken, then the one
/// meeting the most roads); where every height is taken, it takes the one
/// the fewest of them have. Same roads, same heights.
pub fn assign_layers(fixed: &[WorldRoad], roads: &mut [WorldRoad]) {
    let fixed_boxes: Vec<(DVec2, DVec2)> = fixed.iter().map(road_box).collect();
    let boxes: Vec<(DVec2, DVec2)> = roads.iter().map(road_box).collect();
    let touch = |a: (DVec2, DVec2), b: (DVec2, DVec2)| {
        a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
    };
    let n = roads.len();
    // Heights already taken round each road (by the fixed ones), and
    // which of the others it meets.
    let mut taken = vec![[0usize; LAYERS]; n];
    let mut meets: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for i in 0..n {
        for (f, fb) in fixed.iter().zip(&fixed_boxes) {
            if f.id != roads[i].id && touch(boxes[i], *fb) {
                taken[i][f.layer()] += overlap_of(&roads[i], f);
            }
        }
        for j in 0..i {
            if touch(boxes[i], boxes[j]) {
                let o = overlap_of(&roads[i], &roads[j]);
                if o > 0 {
                    meets[i].push((j, o));
                    meets[j].push((i, o));
                }
            }
        }
    }
    let mut done = vec![false; n];
    for _ in 0..n {
        let Some(i) = (0..n).filter(|&i| !done[i]).max_by_key(|&i| {
            let kinds = taken[i].iter().filter(|&&c| c > 0).count();
            (kinds, meets[i].len(), std::cmp::Reverse(i))
        }) else {
            break;
        };
        let best = (0..LAYERS).min_by_key(|&l| (taken[i][l], l)).unwrap_or(0);
        roads[i].layer = best as u8;
        done[i] = true;
        for &(j, o) in &meets[i] {
            taken[j][best] += o;
        }
    }
}

/// Room a bend's points get to slide in, over the mitre's own reach.
const EASE_ROOM: f64 = 1.25;

/// Where road ends meet at an angle (a bend of the main network split into
/// two roads, the arms of a junction): a round plate of pavement and
/// carriageway under the ends fills the wedges their square ends leave.
/// It lies a little below the roads' own surfaces, so they win where they
/// overlap it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoadJoin {
    pub at: DVec2,
    /// Radius of the pavement disc and of the carriageway disc (m).
    pub pavement: f64,
    pub carriage: f64,
    pub pavement_lift: f64,
    pub carriage_lift: f64,
    /// The road whose look the plate takes: the narrowest meeting there,
    /// or at any plate it overlaps (so overlapping plates look alike and
    /// do not fight).
    pub look: super::quarter::DetailId,
}

/// Ends of different roads closer than this meet (m).
const JOIN_SNAP: f64 = 0.3;
/// The plate lies this far under every road's surfaces (m): under the
/// lowest layer's pavement and carriageway, whichever roads cross it.
pub const JOIN_SINK: f64 = LAYER_STEP;

impl RoadJoin {
    /// How far `p` is outside the plate's pavement disc (negative inside).
    pub fn distance(&self, p: DVec2) -> f64 {
        self.at.distance(p) - self.pavement
    }
}

/// The joins among `roads`: every point where ends of two or more roads
/// meet, unless they only continue one another in a straight line (those
/// meet flush). Sorted by place, so any list holding the same roads gives
/// the same joins.
pub fn road_joins(roads: &[WorldRoad]) -> Vec<RoadJoin> {
    // Each end: where, which way the road leaves it, its road.
    let mut ends: Vec<(DVec2, DVec2, usize)> = Vec::new();
    for (i, r) in roads.iter().enumerate() {
        let m = r.pts.len();
        if m < 2 {
            continue;
        }
        ends.push((r.pts[0], (r.pts[1] - r.pts[0]).normalize_or_zero(), i));
        ends.push((
            r.pts[m - 1],
            (r.pts[m - 2] - r.pts[m - 1]).normalize_or_zero(),
            i,
        ));
    }
    ends.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    let mut used = vec![false; ends.len()];
    let mut out = Vec::new();
    for k in 0..ends.len() {
        if used[k] {
            continue;
        }
        let group: Vec<usize> = (k..ends.len())
            .take_while(|&j| ends[j].0.x - ends[k].0.x <= JOIN_SNAP)
            .filter(|&j| !used[j] && ends[j].0.distance(ends[k].0) <= JOIN_SNAP)
            .collect();
        for &j in &group {
            used[j] = true;
        }
        // A road may be listed twice (it reaches into two districts' boxes).
        let mut ids: Vec<usize> = group.iter().map(|&j| ends[j].2).collect();
        ids.sort_unstable_by_key(|&i| roads[i].id.0);
        ids.dedup_by_key(|i| roads[*i].id.0);
        if ids.len() < 2 {
            continue;
        }
        // Two ends running straight on from each other meet flush.
        let dirs: Vec<DVec2> = ids
            .iter()
            .filter_map(|&i| group.iter().find(|&&j| roads[ends[j].2].id == roads[i].id))
            .map(|&j| ends[j].1)
            .collect();
        if dirs.len() == 2 && dirs[0].dot(dirs[1]) < -0.999 {
            continue;
        }
        let rs = ids.iter().map(|&i| &roads[i]);
        let min = |f: &dyn Fn(&WorldRoad) -> f64| rs.clone().map(f).fold(f64::MAX, f64::min);
        out.push(RoadJoin {
            at: ends[k].0,
            pavement: min(&|r| r.half + r.pavement),
            carriage: min(&|r| r.half),
            pavement_lift: ROAD_BASE - JOIN_SINK,
            carriage_lift: ROAD_BASE + KERB - JOIN_SINK,
            look: rs
                .clone()
                .min_by(|a, b| a.half.total_cmp(&b.half).then(a.id.0.cmp(&b.id.0)))
                .map_or_else(Default::default, |r| r.id),
        });
    }
    // Plates that overlap take one look: the narrowest road's of them all.
    let half_of =
        |id: super::quarter::DetailId| roads.iter().find(|r| r.id == id).map_or(0.0, |r| r.half);
    let mut group: Vec<usize> = (0..out.len()).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for i in 0..out.len() {
        for j in i + 1..out.len() {
            if out[i].at.distance(out[j].at) < out[i].pavement + out[j].pavement {
                let (a, b) = (root(&mut group, i), root(&mut group, j));
                group[a.max(b)] = a.min(b);
            }
        }
    }
    let mut look: std::collections::BTreeMap<usize, super::quarter::DetailId> = Default::default();
    for (i, j) in out.iter().enumerate() {
        let g = root(&mut group, i);
        let mine = j.look;
        look.entry(g)
            .and_modify(|l| {
                let (a, b) = ((half_of(mine), mine.0), (half_of(*l), l.0));
                if a.0 < b.0 || (a.0 == b.0 && a.1 < b.1) {
                    *l = mine;
                }
            })
            .or_insert(mine);
    }
    for (i, j) in out.iter_mut().enumerate() {
        let g = root(&mut group, i);
        j.look = look[&g];
    }
    out
}

/// One step of a road's surface: the pavement's four corners, the
/// carriageway's, and their lifts.
#[derive(Clone, Debug)]
pub(super) struct SurfacePiece {
    lo: DVec2,
    hi: DVec2,
    pavement: [DVec2; 4],
    carriage: [DVec2; 4],
    pavement_lift: f64,
    carriage_lift: f64,
}

/// The road pieces over one tile.
#[derive(Debug, Default)]
pub(super) struct SurfaceTile {
    pieces: Vec<SurfacePiece>,
    joins: Vec<RoadJoin>,
}

/// The quad from offset `a` to `b` between two strip points.
fn quad(p: (DVec2, DVec2), q: (DVec2, DVec2), a: f64, b: f64) -> [DVec2; 4] {
    [p.0 + p.1 * a, q.0 + q.1 * a, q.0 + q.1 * b, p.0 + p.1 * b]
}

/// Is `p` inside the four-sided `poly` (any winding)?
fn inside(poly: &[DVec2; 4], p: DVec2) -> bool {
    let mut odd = false;
    for i in 0..4 {
        let (a, b) = (poly[i], poly[(i + 1) % 4]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                odd = !odd;
            }
        }
    }
    odd
}

pub(super) fn tile_key(p: DVec2) -> (i64, i64) {
    (
        (p.x / SURFACE_TILE).floor() as i64,
        (p.y / SURFACE_TILE).floor() as i64,
    )
}

impl SurfaceTile {
    /// The road pieces of `roads` reaching onto tile `k`.
    pub(super) fn new(roads: &[WorldRoad], k: (i64, i64)) -> Self {
        let lo = DVec2::new(k.0 as f64, k.1 as f64) * SURFACE_TILE;
        let hi = lo + DVec2::splat(SURFACE_TILE);
        let mut pieces = Vec::new();
        for r in roads {
            let total = r.half + r.pavement;
            let pts = r.strip_points();
            for w in pts.windows(2) {
                let pavement = quad(w[0], w[1], -total, total);
                let (a, b) = pavement
                    .iter()
                    .fold((DVec2::MAX, DVec2::MIN), |(a, b), c| (a.min(*c), b.max(*c)));
                if a.x > hi.x || b.x < lo.x || a.y > hi.y || b.y < lo.y {
                    continue;
                }
                pieces.push(SurfacePiece {
                    lo: a,
                    hi: b,
                    pavement,
                    carriage: quad(w[0], w[1], -r.half, r.half),
                    pavement_lift: r.pavement_lift(),
                    carriage_lift: r.carriage_lift(),
                });
            }
        }
        let joins = road_joins(roads)
            .into_iter()
            .filter(|j| {
                let r = DVec2::splat(j.pavement);
                !(j.at.x - r.x > hi.x
                    || j.at.x + r.x < lo.x
                    || j.at.y - r.y > hi.y
                    || j.at.y + r.y < lo.y)
            })
            .collect();
        Self { pieces, joins }
    }

    /// The highest road lift at `p`, if `p` is on a road.
    pub(super) fn lift_at(&self, p: DVec2) -> Option<f64> {
        let mut best: Option<f64> = None;
        for s in &self.pieces {
            if p.x < s.lo.x || p.x > s.hi.x || p.y < s.lo.y || p.y > s.hi.y {
                continue;
            }
            let lift = if inside(&s.carriage, p) {
                s.carriage_lift
            } else if inside(&s.pavement, p) {
                s.pavement_lift
            } else {
                continue;
            };
            best = Some(best.map_or(lift, |b: f64| b.max(lift)));
        }
        for j in &self.joins {
            let d = j.at.distance(p);
            let lift = if d < j.carriage {
                j.carriage_lift
            } else if d < j.pavement {
                j.pavement_lift
            } else {
                continue;
            };
            best = Some(best.map_or(lift, |b: f64| b.max(lift)));
        }
        best
    }
}

impl TownWorld {
    /// Height of what people and cars stand on at `p` out in the open: the
    /// top of a road's pavement or carriageway where one is drawn, else the
    /// ground (the river or sea bed under water; the water surface is the
    /// caller's business).
    pub fn surface(&self, p: DVec2) -> f64 {
        let k = tile_key(p);
        let tile = self.surface_tile(k);
        match tile.lift_at(p) {
            Some(lift) => self.road_height(p) + lift,
            None => self.ground(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::quarter::DetailId;

    fn straight() -> WorldRoad {
        WorldRoad {
            id: DetailId(7),
            pts: vec![DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::new(10.0, 10.0)],
            half: 3.0,
            pavement: 2.0,
            main: false,
            class: None,
            style: None,
            layer: 0,
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn strip_points_step_and_mitre() {
        let r = straight();
        let pts = r.strip_points();
        // 10 m in steps of at most 3 m, plus where each bend's slide
        // starts (6.25 m from the corner), plus the last point.
        assert_eq!(pts.len(), 11);
        assert!(pts
            .windows(2)
            .all(|w| w[0].0.distance(w[1].0) <= ROAD_STEP + 1e-9));
        // The corner's mitre reaches sqrt(2) out.
        let corner = pts.iter().find(|p| p.0 == DVec2::new(10.0, 0.0)).unwrap();
        assert!((corner.1.length() - 2f64.sqrt()).abs() < 1e-9);
        // Both edges of the whole width run forward all the way round.
        for w in pts.windows(2) {
            for o in [-5.0, 5.0] {
                let e = (w[1].0 + w[1].1 * o) - (w[0].0 + w[0].1 * o);
                assert!(e.dot(w[1].0 - w[0].0) > 0.0, "folds at {:?}", w[0].0);
            }
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn tight_bends_are_eased() {
        // A staircase of 3 m legs, too tight for a 10 m wide road.
        let pts: Vec<DVec2> = (0..8)
            .map(|k| DVec2::new(((k + 1) / 2) as f64 * 3.0, (k / 2) as f64 * 3.0))
            .collect();
        let eased = ease_bends(&pts, 5.0);
        assert_eq!(eased.first(), pts.first());
        assert_eq!(eased.last(), pts.last());
        assert!(eased.len() < pts.len());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn ends_meeting_at_an_angle_get_a_plate() {
        let a = WorldRoad {
            pts: vec![DVec2::ZERO, DVec2::new(20.0, 0.0)],
            ..straight()
        };
        let b = WorldRoad {
            id: DetailId(8),
            pts: vec![DVec2::new(20.0, 0.0), DVec2::new(30.0, 10.0)],
            ..straight()
        };
        let joins = road_joins(&[a.clone(), b]);
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].at, DVec2::new(20.0, 0.0));
        // Straight on: no plate.
        let c = WorldRoad {
            id: DetailId(9),
            pts: vec![DVec2::new(20.0, 0.0), DVec2::new(40.0, 0.0)],
            ..straight()
        };
        assert!(road_joins(&[a, c]).is_empty());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn carriageway_stands_above_pavement() {
        let r = straight();
        let t = SurfaceTile::new(std::slice::from_ref(&r), (0, 0));
        let on_road = t.lift_at(DVec2::new(5.0, 0.5)).unwrap();
        let on_pavement = t.lift_at(DVec2::new(5.0, 4.0)).unwrap();
        assert!((on_road - r.carriage_lift()).abs() < 1e-12);
        assert!((on_pavement - r.pavement_lift()).abs() < 1e-12);
        // Off the road: 6 m from the first leg, 7 m from the second.
        assert!(t.lift_at(DVec2::new(3.0, 6.0)).is_none());
        // Inside the bend's mitre, on the outer pavement corner.
        assert!(t.lift_at(DVec2::new(14.8, -4.0)).is_some());
    }

    /// A town's road surfaces as drawn: no strip folds back over itself at
    /// a bend (that drew spikes of pavement and left holes in the road),
    /// and no ground near a road is kept bare of grass without a road
    /// drawn on it (flat untextured wedges at junctions, Arnd 2026-11-10).
    fn check_town(seed: u64, people: f64, half: f64) {
        use crate::urban::greenery::Greenery;
        use crate::urban::params::TownParams;
        use hw_core::Seed;
        let w = TownWorld::generate(Seed(seed), TownParams::sample(Seed(seed), people));
        let core = w.core();
        let (lo, hi) = (core - DVec2::splat(half), core + DVec2::splat(half));
        let roads = w.roads_in(lo, hi);
        assert!(!roads.is_empty(), "seed {seed}, {people}: no roads");
        for r in &roads {
            let total = r.half + r.pavement;
            for s in r.strip_points().windows(2) {
                let ((p, n0), (q, n1)) = (s[0], s[1]);
                for o in [-total, -r.half, r.half, total] {
                    assert!(
                        ((q + n1 * o) - (p + n0 * o)).dot(q - p) > 0.0,
                        "seed {seed}, {people}: road {:?} folds back at {p:?}",
                        r.id
                    );
                }
            }
        }
        // Bare ground: grass keeps off (the lawn stops 0.45 m inside a
        // road's edge) but no road is drawn.
        let g = Greenery::new(&w);
        const STEP: f64 = 0.5;
        let mut bare = 0.0;
        let mut at = Vec::new();
        for r in &roads {
            let t = r.half + r.pavement + 1.0;
            for s in r.pts.windows(2) {
                let len = s[0].distance(s[1]);
                let d = (s[1] - s[0]) / len.max(1e-9);
                // Off the grid of the roads' own ends and edges, so no
                // sample sits exactly on a boundary.
                let mut u = -t + 0.13;
                while u < len + t {
                    let mut o = -t + 0.11;
                    while o <= t {
                        let p = s[0] + d * u + d.perp() * o;
                        o += STEP;
                        if p.cmplt(lo).any() || p.cmpgt(hi).any() {
                            continue;
                        }
                        let (road, other) = g.clearances(&w, p);
                        if road + 0.5 <= 0.0 && other > 0.0 && w.surface(p) < w.ground(p) + 0.01 {
                            bare += STEP * STEP;
                            at.push(p);
                        }
                    }
                    u += STEP;
                }
            }
        }
        assert!(
            bare < 3.0,
            "seed {seed}, {people}: {bare} m2 of bare ground beside roads, at {:?}",
            &at[..at.len().min(6)]
        );
    }

    #[test]
    fn a_hamlets_roads_neither_fold_nor_leave_bare_ground() {
        check_town(2, 300.0, 250.0);
    }

    #[test]
    #[ignore = "daily (slow blocker): scripts/check.sh --daily, and CI's first run of the day"]
    fn towns_roads_neither_fold_nor_leave_bare_ground() {
        for seed in 1..=3 {
            for people in [300.0, 5_000.0, 40_000.0, 300_000.0] {
                check_town(seed, people, 300.0);
            }
        }
    }
}
