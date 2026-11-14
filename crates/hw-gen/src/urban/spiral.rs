//! The spiral stair down a shaft.
//!
//! Every way into the underground is a stair you walk: a square spiral
//! round a pillar, going down the shaft's 4.2 m square. Its steps are plain
//! data, so what is drawn is what collides: slabs (a hero steps up and down
//! them, 0.2 m at a time) and the pillar in the middle. There is no lift
//! that floats a body up and down.
//!
//! The ring round the pillar is 1.65 m wide. A turn is 12 steps: a square
//! landing in each corner and two treads along each side between them,
//! 2.4 m of height per turn, so the next turn up clears the head. The top
//! landing is flush with the ground (or the room floor) at a corner of the
//! door side; the foot opens to wherever the shaft leads, and a stair
//! whose foot would be walled in by its own low steps is turned round.

use super::footprint::Footprint;
use super::underground::{Void, VoidKind, SHAFT_HALF};
use glam::DVec2;

/// The riser aimed for (m); the real one is the depth split evenly.
pub const RISE: f64 = 0.2;
/// Thickness of a step slab (m): the space under a step is open.
pub const SLAB: f64 = 0.3;
/// Steps in one turn: four landings and eight treads.
pub const PER_TURN: usize = 12;
/// Half the pillar's side.
pub const NEWEL: f64 = 0.45;
/// Centre line of the ring, and half the width of a step across it.
const MID: f64 = (NEWEL + SHAFT_HALF) / 2.0;
const HALF_WIDTH: f64 = (SHAFT_HALF - NEWEL) / 2.0;
/// Steps along one side of the ring: a landing and two treads.
const SIDE: usize = PER_TURN / 4;
/// The ring's inner and outer edges, from the shaft's centre: the pillar's
/// face and the shaft's wall.
pub const RING: (f64, f64) = (NEWEL, SHAFT_HALF);
/// The balustrade: its rail's height over a step and its thickness (m),
/// standing just inside the ring's outer edge.
pub const RAIL_HEIGHT: f64 = 0.95;
pub const RAIL_T: f64 = 0.04;
/// Headroom a walkway under the stair needs (m).
const HEADROOM: f64 = 2.1;
/// A person's radius, and the ledge height they step onto.
const BODY: f64 = 0.4;
const LEDGE: f64 = 0.45;

/// One step: its centre and half sizes in the stair's own frame (x across
/// the door, z towards it) and the height of its top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub at: DVec2,
    pub half: DVec2,
    pub top: f64,
    pub bottom: f64,
}

/// A spiral stair in a shaft at `c`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spiral {
    pub c: DVec2,
    /// The way the shaft's door faces (unit): local +z. Local x is
    /// `(dir.y, -dir.x)`, which is the shaft void's `along`.
    pub dir: DVec2,
    /// Height of the top landing and of the floor at the foot.
    pub top: f64,
    pub bottom: f64,
    /// Which corner on the door side the top landing is at: 0 on local +x
    /// (the stair then runs along the door side towards -x), 1 on -x.
    pub corner: u8,
}

impl Spiral {
    pub fn new(c: DVec2, dir: DVec2, top: f64, bottom: f64) -> Spiral {
        Spiral {
            c,
            dir: dir.normalize_or(DVec2::Y),
            top,
            bottom,
            corner: 0,
        }
    }

    /// A stair that leaves the foot clear for a way through `passage` (the
    /// void joined to the shaft at the foot, `None` when the shaft opens
    /// into a room all round): the first of the two ways round that does,
    /// `None` when neither does.
    pub fn fit(
        c: DVec2,
        dir: DVec2,
        top: f64,
        bottom: f64,
        passage: Option<&Void>,
    ) -> Option<Spiral> {
        [0u8, 1].into_iter().find_map(|corner| {
            let s = Spiral {
                corner,
                ..Spiral::new(c, dir, top, bottom)
            };
            s.foot_route(passage).map(|_| s)
        })
    }

    /// The shaft's void: a square hole in the frame of the stair. `closed`
    /// gives a roof that height over the foot, `None` leaves it open to the sky.
    pub fn shaft(&self, closed: Option<f64>) -> Void {
        let along = DVec2::new(self.dir.y, -self.dir.x);
        Void {
            kind: VoidKind::Shaft,
            origin: self.c - along * SHAFT_HALF - self.dir * SHAFT_HALF,
            along,
            min: DVec2::ZERO,
            max: DVec2::splat(SHAFT_HALF * 2.0),
            floor: [self.bottom, self.bottom],
            height: closed.unwrap_or(0.0),
            open: closed.is_none(),
        }
    }

    /// Plan position of local `(x, z)`.
    pub fn world(&self, l: DVec2) -> DVec2 {
        self.c + DVec2::new(self.dir.y, -self.dir.x) * l.x + self.dir * l.y
    }

    /// How many steps, from the top landing down to the one just above the
    /// foot.
    pub fn count(&self) -> usize {
        (((self.top - self.bottom) / RISE - 1e-9).ceil() as usize).max(1)
    }

    /// The riser: the depth split evenly over the steps.
    pub fn riser(&self) -> f64 {
        (self.top - self.bottom) / self.count() as f64
    }

    /// The steps, top landing first.
    pub fn steps(&self) -> Vec<Step> {
        let corners = [
            DVec2::new(MID, MID),
            DVec2::new(-MID, MID),
            DVec2::new(-MID, -MID),
            DVec2::new(MID, -MID),
        ];
        let tread = (2.0 * MID - 2.0 * HALF_WIDTH) / (SIDE - 1) as f64;
        let mirror = if self.corner == 1 { -1.0 } else { 1.0 };
        let r = self.riser();
        (0..self.count())
            .map(|i| {
                let q = i % PER_TURN;
                let (ci, j) = (q / SIDE, q % SIDE);
                let a = corners[ci];
                let (at, half) = if j == 0 {
                    (a, DVec2::splat(HALF_WIDTH))
                } else {
                    let b = corners[(ci + 1) % 4];
                    let d = (b - a).normalize();
                    let at = a + d * (HALF_WIDTH + tread * (j as f64 - 0.5));
                    let half = if d.x.abs() > 0.5 {
                        DVec2::new(tread * 0.5, HALF_WIDTH)
                    } else {
                        DVec2::new(HALF_WIDTH, tread * 0.5)
                    };
                    (at, half)
                };
                let top = self.top - r * i as f64;
                Step {
                    at: DVec2::new(at.x * mirror, at.y),
                    half,
                    top,
                    bottom: (top - SLAB).max(self.bottom),
                }
            })
            .collect()
    }

    /// Whether step `s` has the balustrade on its outer edge: from a drop
    /// of a rail's height below the top (so no rail stands up out of the
    /// street) down to where the floor is a metre below.
    pub fn railed(&self, s: &Step) -> bool {
        s.top <= self.top - RAIL_HEIGHT + 1e-9 && s.top > self.bottom + 1.0
    }

    /// The unit normals (local) of the faces of `s` on the ring's outer
    /// edge: one for a tread, two for a landing in a corner.
    pub fn outer_faces(s: &Step) -> Vec<DVec2> {
        [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y]
            .into_iter()
            .filter(|n| ((s.at + s.half * *n).dot(*n) - RING.1).abs() < 1e-6)
            .collect()
    }

    /// The balustrade along the outer edge, as solid parts: a thin wall on
    /// every railed step, as high as the rail (the drawn rail is on it).
    pub fn rails(&self) -> Vec<Footprint> {
        let mut out = Vec::new();
        for s in self.steps().iter().filter(|s| self.railed(s)) {
            for n in Spiral::outer_faces(s) {
                let along = DVec2::new(n.y.abs(), n.x.abs());
                let at = s.at + n * (s.half.dot(n.abs()) - RAIL_T * 0.5 - 0.04);
                out.push(Footprint::Box {
                    at: self.world(at),
                    dir: self.dir,
                    half: along * s.half.dot(along) + n.abs() * (RAIL_T * 0.5 + 0.04),
                    y0: s.top,
                    y1: s.top + RAIL_HEIGHT,
                });
            }
        }
        out
    }

    /// Where a person's feet go walking down: the middle of every step,
    /// top landing first (for bots and tests; reverse it to climb).
    pub fn route(&self) -> Vec<glam::DVec3> {
        self.steps()
            .iter()
            .map(|s| {
                let p = self.world(s.at);
                glam::DVec3::new(p.x, s.top, p.y)
            })
            .collect()
    }

    /// The solid parts: the pillar, every step and the balustrade.
    pub fn footprints(&self) -> Vec<Footprint> {
        let mut out = vec![Footprint::Box {
            at: self.c,
            dir: self.dir,
            half: DVec2::splat(NEWEL),
            y0: self.bottom,
            y1: self.top,
        }];
        out.extend(self.steps().into_iter().map(|s| Footprint::Box {
            at: self.world(s.at),
            dir: self.dir,
            half: s.half,
            y0: s.bottom,
            y1: s.top,
        }));
        out.extend(self.rails());
        out
    }

    /// The rail round the hole in the street on the three sides away from
    /// the door, as solid parts (the drawn rail is on the same line).
    pub fn parapet(&self) -> Vec<Footprint> {
        let k = SHAFT_HALF + 0.1;
        let t = 0.06;
        let wall = |at: DVec2, half: DVec2| Footprint::Box {
            at: self.world(at),
            dir: self.dir,
            half,
            y0: self.top - SLAB,
            y1: self.top + 1.0,
        };
        vec![
            wall(DVec2::new(0.0, -k), DVec2::new(k + t, t)),
            wall(DVec2::new(-k, 0.0), DVec2::new(t, k + t)),
            wall(DVec2::new(k, 0.0), DVec2::new(t, k + t)),
        ]
    }

    /// Everything solid at the shaft: the stair, and the parapet when the
    /// shaft opens on a street.
    pub fn solids(&self, street: bool) -> Vec<Footprint> {
        let mut out = self.footprints();
        if street {
            out.extend(self.parapet());
        }
        out
    }

    /// The way from the lowest step to the passage's mouth along the floor,
    /// under the stair's higher steps, as plan points (`Some` and empty when
    /// there is no passage: the shaft's foot is in a room); `None` when the
    /// stair's own low steps wall the foot in.
    pub fn foot_route(&self, passage: Option<&Void>) -> Option<Vec<DVec2>> {
        let Some(p) = passage else {
            return Some(Vec::new());
        };
        let steps = self.steps();
        // The slabs a person at the floor cannot step onto or walk under.
        let walls: Vec<&Step> = steps
            .iter()
            .filter(|s| s.top > self.bottom + LEDGE && s.bottom < self.bottom + HEADROOM)
            .collect();
        let near = |l: DVec2, s: &Step| {
            let d = (l - s.at).abs() - s.half;
            d.max(DVec2::ZERO).length() < BODY
        };
        let free = |l: DVec2| {
            l.abs().max_element() <= SHAFT_HALF
                && l.abs().max_element() >= NEWEL + BODY
                && !walls.iter().any(|s| near(l, s))
        };
        let cell = 0.2;
        let n = (2.0 * SHAFT_HALF / cell).round() as i32;
        let at = |i: i32, j: i32| {
            DVec2::new(
                -SHAFT_HALF + (i as f64 + 0.5) * cell,
                -SHAFT_HALF + (j as f64 + 0.5) * cell,
            )
        };
        let last = steps.last()?;
        let reaches = |l: DVec2| {
            let d = (l - last.at).abs() - last.half;
            d.max(DVec2::ZERO).length() <= 0.6
        };
        let mouth =
            |l: DVec2| l.abs().max_element() >= SHAFT_HALF - cell && p.covers(self.world(l), -BODY);
        let idx = |i: i32, j: i32| (i * n + j) as usize;
        let mut from: Vec<Option<(i32, i32)>> = vec![None; (n * n) as usize];
        let mut seen = vec![false; (n * n) as usize];
        let mut queue = std::collections::VecDeque::new();
        for i in 0..n {
            for j in 0..n {
                let l = at(i, j);
                if free(l) && reaches(l) {
                    seen[idx(i, j)] = true;
                    queue.push_back((i, j));
                }
            }
        }
        while let Some((i, j)) = queue.pop_front() {
            if mouth(at(i, j)) {
                let mut path = vec![self.world(at(i, j))];
                let mut cur = (i, j);
                while let Some(prev) = from[idx(cur.0, cur.1)] {
                    path.push(self.world(at(prev.0, prev.1)));
                    cur = prev;
                }
                path.reverse();
                // Every third cell is enough to steer by.
                let mut out: Vec<DVec2> = path.iter().step_by(3).copied().collect();
                if let Some(&e) = path.last() {
                    out.push(e);
                }
                return Some(out);
            }
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (a, b) = (i + di, j + dj);
                if a < 0 || b < 0 || a >= n || b >= n || seen[idx(a, b)] {
                    continue;
                }
                if free(at(a, b)) {
                    seen[idx(a, b)] = true;
                    from[idx(a, b)] = Some((i, j));
                    queue.push_back((a, b));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spiral(depth: f64) -> Spiral {
        Spiral::new(DVec2::new(10.0, 20.0), DVec2::new(0.6, 0.8), 0.0, -depth)
    }

    #[test]
    fn the_risers_are_even_and_walkable() {
        for depth in [3.0, 16.3, 36.0, 52.7] {
            let s = spiral(depth);
            let steps = s.steps();
            assert!((steps[0].top - s.top).abs() < 1e-9, "top landing is flush");
            let last = steps.last().unwrap().top;
            let r = s.riser();
            assert!(r <= RISE + 1e-9 && r > 0.15, "riser {r}");
            assert!((last - s.bottom - r).abs() < 1e-9, "foot is one riser down");
        }
    }

    #[test]
    fn each_step_is_next_to_the_one_below_it() {
        // Walking down, the next step starts where this one ends.
        for corner in [0u8, 1] {
            let s = Spiral {
                corner,
                ..spiral(30.0)
            };
            let steps = s.steps();
            for w in steps.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                let gap = ((a.at - b.at).abs() - a.half - b.half).max(DVec2::ZERO);
                assert!(gap.length() < 1e-6, "gap between steps: {gap:?}");
                // They touch along a side, not just at a corner.
                let overlap = (a.half + b.half - (a.at - b.at).abs()).min_element();
                assert!(overlap > -1e-6);
            }
        }
    }

    #[test]
    fn a_turn_clears_the_head() {
        let s = spiral(30.0);
        let steps = s.steps();
        for (i, a) in steps.iter().enumerate() {
            if let Some(b) = steps.get(i + PER_TURN) {
                assert!(a.bottom - b.top > 1.9, "headroom {}", a.bottom - b.top);
            }
        }
    }

    #[test]
    fn steps_stay_in_the_shaft_round_the_pillar() {
        let s = spiral(20.0);
        for st in s.steps() {
            let (lo, hi) = (st.at - st.half, st.at + st.half);
            assert!(lo.min_element() >= -SHAFT_HALF - 1e-9);
            assert!(hi.max_element() <= SHAFT_HALF + 1e-9);
            // Not inside the pillar.
            let inside = lo.x < NEWEL - 1e-9
                && hi.x > -NEWEL + 1e-9
                && lo.y < NEWEL - 1e-9
                && hi.y > -NEWEL + 1e-9;
            assert!(!inside, "step in the pillar: {st:?}");
        }
    }

    #[test]
    fn the_foot_can_be_left_by_a_passage_on_some_side() {
        // From every side a passage can come in, one of the two ways round
        // (or at least one of them for most sides) leaves it open.
        let mut open = 0;
        for k in 0..16 {
            let a = std::f64::consts::TAU * k as f64 / 16.0;
            let d = DVec2::new(a.cos(), a.sin());
            let p = Void {
                kind: VoidKind::Passage,
                origin: DVec2::new(10.0, 20.0),
                along: d,
                min: DVec2::new(0.0, -1.5),
                max: DVec2::new(12.0, 1.5),
                floor: [-30.0, -30.0],
                height: 3.2,
                open: false,
            };
            if Spiral::fit(DVec2::new(10.0, 20.0), DVec2::Y, 0.0, -30.0, Some(&p)).is_some() {
                open += 1;
            }
        }
        assert!(open >= 6, "only {open} of 16 passage directions open");
    }

    #[test]
    fn a_hero_fits_the_ring_with_room_to_turn() {
        // A person's centre keeps 0.4 m from the pillar and from the
        // balustrade: the lane left between is wide enough to walk without
        // scraping (it was 0.16 m when the ring was 1 m wide).
        let lane = (RING.1 - RAIL_T - 0.08) - RING.0 - 2.0 * BODY;
        assert!(lane >= 0.6, "walkable lane {lane:.2} m");
        // And a tread is deep enough to put a foot on.
        let steps = spiral(10.0).steps();
        assert!(steps.iter().all(|s| s.half.min_element() * 2.0 >= 0.4));
    }
}
