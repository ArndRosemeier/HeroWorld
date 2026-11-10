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
    /// Which of four heights the road is drawn at, so crossing roads never
    /// fight.
    pub fn layer(&self) -> usize {
        ((self.id.0 ^ (self.id.0 >> 32)).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 62) as usize
    }

    /// Height of the pavement's top above [`TownWorld::road_height`].
    pub fn pavement_lift(&self) -> f64 {
        0.02 + self.layer() as f64 * 0.012
    }

    /// Height of the carriageway's top above [`TownWorld::road_height`]:
    /// a kerb's step above the pavement.
    pub fn carriage_lift(&self) -> f64 {
        self.pavement_lift() + 0.05
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
            let start = if i > 0 {
                mitre(dir(i - 1), d).dot(d)
            } else {
                0.0
            };
            let end = if i + 2 < m {
                mitre(d, dir(i + 1)).dot(d)
            } else {
                0.0
            };
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
                let n = if u == 0.0 && i > 0 {
                    mitre(dir(i - 1), d)
                } else {
                    d.perp() + d * (ka * wa + kb * wb)
                };
                out.push((a + d * u, n));
            }
        }
        let last = dir(m - 2);
        out.push((pts[m - 1], mitre(last, DVec2::ZERO)));
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
}

/// Ends of different roads closer than this meet (m).
const JOIN_SNAP: f64 = 0.3;
/// The plate lies this far under the roads' own surfaces (m).
pub const JOIN_SINK: f64 = 0.006;

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
            pavement_lift: min(&|r| r.pavement_lift()) - JOIN_SINK,
            carriage_lift: min(&|r| r.carriage_lift()) - JOIN_SINK,
        });
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
        }
    }

    #[test]
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
    #[ignore = "slow: scripts/check.sh --full and CI run it"]
    fn towns_roads_neither_fold_nor_leave_bare_ground() {
        for seed in 1..=3 {
            for people in [300.0, 5_000.0, 40_000.0, 300_000.0] {
                check_town(seed, people, 300.0);
            }
        }
    }
}
