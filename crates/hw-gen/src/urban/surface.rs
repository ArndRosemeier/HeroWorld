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
    /// width), limited at sharp turns.
    pub fn strip_points(&self) -> Vec<(DVec2, DVec2)> {
        let pts = &self.pts;
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
        for i in 0..m - 1 {
            let (a, b) = (pts[i], pts[i + 1]);
            let d = dir(i);
            let steps = (a.distance(b) / ROAD_STEP).ceil().max(1.0) as usize;
            for j in 0..steps {
                let n = if j == 0 {
                    let before = if i > 0 { dir(i - 1) } else { DVec2::ZERO };
                    mitre(before, d)
                } else {
                    d.perp()
                };
                out.push((a.lerp(b, j as f64 / steps as f64), n));
            }
        }
        let last = dir(m - 2);
        out.push((pts[m - 1], mitre(last, DVec2::ZERO)));
        out
    }
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
        Self { pieces }
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
        // 10 m in steps of at most 3 m: 4 steps a leg, plus the last point.
        assert_eq!(pts.len(), 9);
        assert!(pts
            .windows(2)
            .all(|w| w[0].0.distance(w[1].0) <= ROAD_STEP + 1e-9));
        // The corner's mitre reaches sqrt(2) out.
        let corner = pts[4];
        assert_eq!(corner.0, DVec2::new(10.0, 0.0));
        assert!((corner.1.length() - 2f64.sqrt()).abs() < 1e-9);
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
}
