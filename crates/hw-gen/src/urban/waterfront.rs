//! The waterfront: stone quay walls where a street meets the water, flights
//! of steps down to the water, and (in [`super::props`]) mooring bollards.
//!
//! A wall runs along a road's outer edge wherever the water starts just
//! beyond it, unless another road paves that side or a bridge leaves there.

use super::roadgrid::RoadGrid;
use super::world::{Local, WATER_LEVEL};
use crate::water::segment_distance;
use glam::DVec2;

/// Spacing of the samples along a wall (m).
pub const WALL_STEP: f64 = 2.0;
/// Width of a flight of steps (m).
pub const STEPS_WIDTH: f64 = 2.4;

/// A quay wall: its top edge, sampled every [`WALL_STEP`].
#[derive(Clone, Debug, PartialEq)]
pub struct QuayWall {
    pub pts: Vec<DVec2>,
    /// Unit vector out over the water, per point.
    pub out: Vec<DVec2>,
    /// Height of the street at the edge, per point.
    pub top: Vec<f64>,
    /// Where the wall's face ends, below the water, per point.
    pub bottom: Vec<f64>,
}

/// A flight of steps from the quay down to a landing just above the water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSteps {
    /// On the wall's top edge, in the middle of the flight.
    pub pos: DVec2,
    /// Out over the water.
    pub out: DVec2,
    pub top: f64,
}

impl WaterSteps {
    /// Height of the landing at the foot of the steps.
    pub fn landing(&self) -> f64 {
        WATER_LEVEL + 0.25
    }
}

/// The quay walls and steps on one unit's ground: walls along every road
/// near it, kept where the unit owns the wall's top edge.
pub(super) fn plan_waterfront(l: &Local) -> (Vec<QuayWall>, Vec<WaterSteps>) {
    let w = l.world;
    let roads = RoadGrid::new(&l.roads, 0.0);
    let wet = |p: DVec2| w.ground(p) < WATER_LEVEL + 0.05;
    let mut walls = Vec::new();
    for (ri, r) in l.roads.iter().enumerate() {
        let total = r.half + r.pavement;
        let samples = resample(&r.pts, WALL_STEP);
        for side in [1.0, -1.0] {
            let mut cur = QuayWall {
                pts: Vec::new(),
                out: Vec::new(),
                top: Vec::new(),
                bottom: Vec::new(),
            };
            let mut flush = |cur: &mut QuayWall| {
                if cur.pts.len() >= 3 {
                    walls.push(std::mem::replace(
                        cur,
                        QuayWall {
                            pts: Vec::new(),
                            out: Vec::new(),
                            top: Vec::new(),
                            bottom: Vec::new(),
                        },
                    ));
                } else {
                    cur.pts.clear();
                    cur.out.clear();
                    cur.top.clear();
                    cur.bottom.clear();
                }
            };
            for &(c, d) in &samples {
                let n = d.perp() * side;
                let e = c + n * total;
                let beyond = e + n * 1.2;
                let ok = l.owner.owns(e)
                    && wet(beyond)
                    // On a bridge the street itself is over the water.
                    && !wet(c)
                    && !roads.touches(e + n * 0.6, 0.2, Some(ri))
                    && !l.bridges.iter().any(|b| {
                        segment_distance(e, b.a, b.b) < b.half + 1.5 && segment_distance(c, b.a, b.b) > 0.5
                    });
                if !ok {
                    flush(&mut cur);
                    continue;
                }
                let floor = w.ground(beyond).min(w.ground(e));
                cur.pts.push(e);
                cur.out.push(n);
                cur.top.push(w.road_height(e));
                cur.bottom.push((floor - 0.3).min(WATER_LEVEL - 1.0));
            }
            flush(&mut cur);
        }
    }
    // Steps: one flight every so often along each long wall, where the water
    // in front is deep enough to step into a boat.
    let mut steps: Vec<WaterSteps> = Vec::new();
    for wall in &walls {
        let n = wall.pts.len();
        if (n as f64) * WALL_STEP < 24.0 {
            continue;
        }
        let mut k = (16.0 / WALL_STEP) as usize;
        while k + 4 < n {
            let (p, o) = (wall.pts[k], wall.out[k]);
            // Straight here: the neighbours face the same way.
            let straight = wall.out[k - 2].dot(o) > 0.97 && wall.out[k + 2].dot(o) > 0.97;
            let deep = w.ground(p + o * 4.0) < WATER_LEVEL - 0.3;
            let lone = steps.iter().all(|s| s.pos.distance(p) > 45.0);
            if straight && deep && lone {
                steps.push(WaterSteps {
                    pos: p,
                    out: o,
                    top: wall.top[k],
                });
                k += (70.0 / WALL_STEP) as usize;
            } else {
                k += 2;
            }
        }
    }
    (walls, steps)
}

/// Points every `step` along a polyline, with the unit direction there.
fn resample(pts: &[DVec2], step: f64) -> Vec<(DVec2, DVec2)> {
    let mut out = Vec::new();
    let mut carry = 0.0;
    for wnd in pts.windows(2) {
        let (a, b) = (wnd[0], wnd[1]);
        let l = a.distance(b);
        if l < 1e-9 {
            continue;
        }
        let d = (b - a) / l;
        let mut s = carry;
        while s < l {
            out.push((a + d * s, d));
            s += step;
        }
        carry = s - l;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::urban::params::TownParams;
    use hw_core::Seed;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn river_towns_get_quay_walls_and_steps() {
        let tw = crate::urban::world::TownWorld::generate(
            Seed(3),
            TownParams::sample(Seed(3), 40_000.0),
        );
        let w = tw.everything();
        assert!(!w.quays.is_empty(), "no quay walls");
        assert!(!w.water_steps.is_empty(), "no steps to the water");
        for q in &w.quays {
            for k in 0..q.pts.len() {
                assert!(q.top[k] > q.bottom[k] + 1.0);
                assert!(tw.ground(q.pts[k] + q.out[k] * 1.2) < WATER_LEVEL + 0.05);
            }
        }
        for s in &w.water_steps {
            assert!(s.top > s.landing());
        }
    }
}
