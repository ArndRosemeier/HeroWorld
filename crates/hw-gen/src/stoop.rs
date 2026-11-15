//! Stoops: where a street door stands higher than the ground in front of it
//! (a building on a slope, on its plinth), a landing at the door and a
//! flight of stairs running parallel to the facade, down to the pavement,
//! with a railing along the open side. A drop too tall for one flight turns
//! back on itself: a platform at the end of the flight, and the next flight
//! returns beside it, and so on down.
//!
//! Pure data from the plan, the building's base height and the ground, so
//! the server's collision and every client's drawing agree on them (what is
//! drawn is what collides). Everything is in the building's frame (x along
//! the front, z inwards), heights absolute.

use crate::building::{BuildingPlan, Door};
use crate::geom::Rect;
use glam::{DVec2, DVec3};

/// A door standing lower than this above the ground in front of it needs no
/// stairs (a kerb's worth: bodies step onto it).
pub const MIN_DROP: f64 = 0.25;
/// Rise and depth of a tread (m).
pub const RISE: f64 = 0.17;
pub const RUN: f64 = 0.32;
/// How far the landing reaches out from the wall line, and the width of a
/// flight, which is as deep (m).
pub const DEPTH: f64 = 1.6;
/// The landing reaches this far beside the doorway on each side (m).
const MARGIN: f64 = 0.7;
/// Height of the railing over the treads and the landing, and the width of
/// its bars (m).
pub const RAIL_H: f64 = 1.0;
pub const RAIL_THICK: f64 = 0.08;
/// A flight never has more treads than this; a drop that needs more turns
/// back on a platform, in the next lane out, up to [`MAX_LANES`] lanes (the
/// last one steeper if it must).
const FLIGHT_TREADS: u32 = 20;
const MAX_LANES: usize = 5;
/// Stone reaches this far down into the ground.
pub const SINK: f64 = 0.6;
/// Treads of a railing section that stands as one box for collision.
const RAIL_CHUNK: u32 = 4;
/// How far a stoop can reach out of the building's footprint, along the
/// wall or away from it (m): what a collider looks that far for.
pub const REACH: f64 = 12.0;

/// A box: a rectangle in the building's frame and its heights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub rect: Rect,
    pub y0: f64,
    pub y1: f64,
}

/// One street door's landing, flights and railing.
#[derive(Clone, Debug, PartialEq)]
pub struct Stoop {
    /// The doorway: its floor-0 centre on the wall line.
    pub at: DVec2,
    /// Out of the building (unit, x or z), and along the wall towards the
    /// first flight (unit).
    pub out: DVec2,
    pub down: DVec2,
    /// The landing at the door, at the building's base.
    pub landing: Block,
    /// One block per tread, in the order one comes down them.
    pub treads: Vec<Block>,
    /// Platforms where a tall flight turns back.
    pub platforms: Vec<Block>,
    /// Railings: top line of the bars, as points (x, height, z); each
    /// `rails[k][i]` to `rails[k][i + 1]`. Posts stand at every point.
    pub rails: Vec<Vec<DVec3>>,
    /// The top of the stoop (the landing).
    pub top: f64,
    /// The ground at the foot of the last flight.
    pub foot_ground: f64,
    /// The point on the ground where the way up begins (building frame).
    pub foot: DVec2,
    /// The way down from the landing's edge to the foot: the middle of
    /// each flight's head and foot and of each platform (building frame).
    pub path: Vec<DVec2>,
}

impl Stoop {
    /// All the solid boxes: landing, platforms, treads and railing sections.
    pub fn blocks(&self) -> Vec<Block> {
        let mut out = vec![self.landing];
        out.extend(self.platforms.iter().copied());
        out.extend(self.treads.iter().copied());
        out.extend(self.rail_blocks());
        out
    }

    /// The railings as boxes for collision: straight runs are one thin box,
    /// sloped runs step down in sections of a few treads, each from the
    /// lowest tread top in it to the bar over its highest.
    pub fn rail_blocks(&self) -> Vec<Block> {
        let mut out = Vec::new();
        for rail in &self.rails {
            for w in rail.windows(2) {
                let (p, q) = (w[0], w[1]);
                let (a, b) = (DVec2::new(p.x, p.z), DVec2::new(q.x, q.z));
                if (p.y - q.y).abs() < 1e-9 {
                    out.push(Block {
                        rect: thin(a, b),
                        y0: p.y - RAIL_H,
                        y1: p.y,
                    });
                    continue;
                }
                let len = a.distance(b);
                let n = ((len / (RUN * RAIL_CHUNK as f64)).ceil() as usize).max(1);
                for k in 0..n {
                    let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
                    let (ya, yb) = (p.y + (q.y - p.y) * t0, p.y + (q.y - p.y) * t1);
                    out.push(Block {
                        rect: thin(a.lerp(b, t0), a.lerp(b, t1)),
                        y0: ya.min(yb) - RAIL_H,
                        y1: ya.max(yb),
                    });
                }
            }
        }
        out
    }

    /// Stone only: the landing, platforms and treads.
    pub fn stone(&self) -> impl Iterator<Item = &Block> {
        std::iter::once(&self.landing)
            .chain(self.platforms.iter())
            .chain(self.treads.iter())
    }

    fn overlaps(&self, r: &Rect) -> bool {
        self.stone().any(|b| b.rect.overlap(r) > 1e-6)
    }

    /// Where the way down ends on the ground: the last tread's middle.
    pub fn flight_end(&self) -> DVec2 {
        self.treads
            .last()
            .map_or(self.foot, |t| (t.rect.min + t.rect.max) * 0.5)
    }
}

/// A thin box along an axis-aligned segment.
fn thin(a: DVec2, b: DVec2) -> Rect {
    let h = RAIL_THICK * 0.5;
    Rect::new(a.min(b) - DVec2::splat(h), a.max(b) + DVec2::splat(h))
}

fn rect_of(a: DVec2, b: DVec2) -> Rect {
    Rect::new(a.min(b), a.max(b))
}

/// What the plan's street doors need: a stoop for every door on the ground
/// floor to the outside that stands higher than [`MIN_DROP`] over the ground
/// in front of it. `base` is the ground floor's height; `ground` the
/// ground's height at a point of the town's frame (x, z).
pub fn stoops(plan: &BuildingPlan, base: f64, ground: &dyn Fn(DVec2) -> f64) -> Vec<Stoop> {
    let Some(floor) = plan.floors.first() else {
        return Vec::new();
    };
    let site = &plan.site;
    let world = |p: DVec2| site.origin + site.along * p.x + site.inward * p.y;
    let g = |p: DVec2| ground(world(p));
    let mid = floor.outline.center();
    let mut out: Vec<Stoop> = Vec::new();
    for door in floor.doors.iter().filter(|d| d.b.is_none()) {
        let wall = if door.along_x {
            (floor.outline.min.x, floor.outline.max.x)
        } else {
            (floor.outline.min.y, floor.outline.max.y)
        };
        if let Some(s) = one(door, wall, mid, base, &g, &out) {
            out.push(s);
        }
    }
    out
}

/// One way down, before it is chosen.
struct Way {
    cost: f64,
    stoop: Stoop,
}

fn one(
    door: &Door,
    wall: (f64, f64),
    mid: DVec2,
    base: f64,
    g: &dyn Fn(DVec2) -> f64,
    earlier: &[Stoop],
) -> Option<Stoop> {
    // Out of the building, and along the wall.
    let (out, along) = if door.along_x {
        (
            if door.at.y < mid.y {
                -DVec2::Y
            } else {
                DVec2::Y
            },
            DVec2::X,
        )
    } else {
        (
            if door.at.x < mid.x {
                -DVec2::X
            } else {
                DVec2::X
            },
            DVec2::Y,
        )
    };
    // The ground a person stands on in front of the landing.
    if base - g(door.at + out * (DEPTH + 0.4)) <= MIN_DROP {
        return None;
    }
    let mut best: Option<Way> = None;
    for sign in [1.0, -1.0] {
        let way = way_down(door, base, g, out, along * sign, wall, earlier);
        if best.as_ref().is_none_or(|b| way.cost < b.cost) {
            best = Some(way);
        }
    }
    best.map(|b| b.stoop)
}

/// The stoop running one way along the wall.
fn way_down(
    door: &Door,
    base: f64,
    g: &dyn Fn(DVec2) -> f64,
    out: DVec2,
    down: DVec2,
    wall: (f64, f64),
    earlier: &[Stoop],
) -> Way {
    let half = door.size.x * 0.5 + MARGIN;
    // How far the wall runs from the door's middle each way (the stairs stay
    // within their own building's front: the next door's landing is there).
    let c = if door.along_x { door.at.x } else { door.at.y };
    let (to_hi, to_lo) = (wall.1 - c - 0.05, c - wall.0 - 0.05);
    let (fwd, back) = if down.x + down.y > 0.0 {
        (to_hi, to_lo)
    } else {
        (to_lo, to_hi)
    };
    // A point `u` along the way from the door's middle and `v` out from the
    // wall line.
    let pt = |u: f64, v: f64| door.at + down * u + out * v;
    let low = |a: DVec2, b: DVec2| {
        let r = rect_of(a, b);
        [
            r.min,
            r.max,
            DVec2::new(r.min.x, r.max.y),
            DVec2::new(r.max.x, r.min.y),
            (r.min + r.max) * 0.5,
        ]
        .iter()
        .map(|p| g(*p))
        .fold(f64::MAX, f64::min)
    };
    let block = |a: DVec2, b: DVec2, top: f64| Block {
        rect: rect_of(a, b),
        y0: low(a, b) - SINK,
        y1: top,
    };
    let landing = block(pt(-half, 0.0), pt(half, DEPTH), base);
    let t = RAIL_THICK * 0.5;
    let post = |at: DVec2, y: f64| DVec3::new(at.x, y + RAIL_H, at.y);
    let mut treads: Vec<Block> = Vec::new();
    let mut platforms: Vec<Block> = Vec::new();
    // Landing railing: round its far end and along its outer edge.
    let mut rails: Vec<Vec<DVec3>> = vec![vec![
        post(pt(-half + t, 0.0), base),
        post(pt(-half + t, DEPTH - t), base),
        post(pt(half, DEPTH - t), base),
    ]];
    let (mut y, mut u, mut dirn, mut lane) = (base, half, 1.0_f64, 0usize);
    let (foot, foot_ground);
    let mut total = 0u32;
    let mut path = vec![pt(half - 0.3, DEPTH * 0.5)];
    loop {
        let (v_mid, v_out) = ((lane as f64 + 0.5) * DEPTH, (lane as f64 + 1.0) * DEPTH - t);
        // How many steps down to the ground, if this flight went all the
        // way (the ground it meets settles with the length).
        let mut steps = ((y - g(pt(u, v_mid))) / RISE).ceil().clamp(1.0, 400.0) as u32;
        let mut ground_end = g(pt(u, v_mid));
        for _ in 0..5 {
            let reach = (steps - 1) as f64 * RUN;
            ground_end = g(pt(u + dirn * (reach + 0.3), v_mid));
            steps = (((y - ground_end) / RISE).ceil().max(1.0) as u32).min(400);
        }
        // Room left along the wall in this flight's direction.
        let room = if dirn > 0.0 { fwd - u } else { u + back }.max(0.0);
        let fits = (room / RUN).floor() as u32;
        let last = steps - 1 <= FLIGHT_TREADS.min(fits) || lane + 1 == MAX_LANES;
        let (n, rise) = if last {
            // (Steeper treads, but never a flight longer than the others.)
            let n = (steps - 1).min(FLIGHT_TREADS).min(fits.max(1));
            (n, (y - ground_end).max(0.05) / (n + 1) as f64)
        } else {
            // Room for a platform after the treads.
            (
                FLIGHT_TREADS.min(((room - DEPTH) / RUN).floor().max(0.0) as u32),
                RISE,
            )
        };
        for k in 1..=n {
            let a = pt(u + dirn * (k - 1) as f64 * RUN, lane as f64 * DEPTH);
            let b = pt(u + dirn * k as f64 * RUN, (lane + 1) as f64 * DEPTH);
            treads.push(block(a, b, y - k as f64 * rise));
        }
        total += n;
        path.push(pt(u, v_mid));
        let u_end = u + dirn * n as f64 * RUN;
        // The railing along the flight's outer edge, from its head to its foot.
        let outer = rails.last_mut().expect("a railing");
        if n > 0 {
            outer.push(post(pt(u, v_out), y));
            outer.push(post(pt(u_end, v_out), y - n as f64 * rise));
        }
        path.push(pt(u_end, v_mid));
        if last {
            foot = pt(u_end + dirn * 0.5, v_mid);
            path.push(foot);
            foot_ground = ground_end;
            break;
        }
        // A platform at the foot, a step lower, and the way back beside it.
        let y_p = y - (n + 1) as f64 * rise;
        let (a, b) = (
            pt(u_end, lane as f64 * DEPTH),
            pt(u_end + dirn * DEPTH, (lane + 2) as f64 * DEPTH),
        );
        platforms.push(block(a, b, y_p));
        path.push(pt(u_end + dirn * DEPTH * 0.5, (lane as f64 + 1.0) * DEPTH));
        let v_far = (lane as f64 + 2.0) * DEPTH - t;
        rails.push(vec![
            post(pt(u_end + dirn * (DEPTH - t), lane as f64 * DEPTH + t), y_p),
            post(pt(u_end + dirn * (DEPTH - t), v_far), y_p),
            post(pt(u_end, v_far), y_p),
        ]);
        y = y_p;
        u = u_end;
        dirn = -dirn;
        lane += 1;
    }
    let stoop = Stoop {
        at: door.at,
        out,
        down,
        landing,
        treads,
        platforms,
        rails,
        top: base,
        foot_ground,
        foot,
        path,
    };
    let blocked = earlier
        .iter()
        .any(|e| stoop.stone().any(|b| e.overlaps(&b.rect)));
    Way {
        cost: total as f64 + lane as f64 * 10.0 + if blocked { 1e6 } else { 0.0 },
        stoop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::{CityParams, CityPlan};

    fn plans() -> Vec<BuildingPlan> {
        let city = CityPlan::generate(hw_core::Seed(3), &CityParams::default());
        city.buildings
            .iter()
            .take(80)
            .map(|(_, s)| BuildingPlan::generate(s))
            .collect()
    }

    #[test]
    fn doors_on_the_ground_need_no_stoop() {
        for p in plans() {
            assert!(stoops(&p, 0.0, &|_| 0.0).is_empty());
            assert!(stoops(&p, MIN_DROP, &|_| 0.0).is_empty());
        }
    }

    #[test]
    fn a_raised_door_gets_a_landing_and_treads_down_to_the_ground() {
        let mut seen = 0;
        for p in plans() {
            let outside = p.floors[0].doors.iter().filter(|d| d.b.is_none()).count();
            let s = stoops(&p, 2.0, &|_| 0.0);
            // Every street door has its own.
            assert_eq!(s.len(), outside);
            for st in &s {
                seen += 1;
                assert_eq!(st.top, 2.0);
                // The landing is at the door's wall and as high as the floor.
                assert_eq!(st.landing.y1, 2.0);
                assert!(st.landing.rect.contains_point(st.at + st.out * 0.1));
                // Treads fall a rise each, to within a rise of the ground.
                let mut y = 2.0;
                for t in &st.treads {
                    // (After a platform, two: it stands a step lower than the
                    // tread before it.)
                    assert!(y - t.y1 <= 2.0 * RISE + 1e-9 && y - t.y1 > 0.0);
                    y = t.y1;
                }
                assert!(y <= RISE + 1e-9, "the last tread is {y} m up");
                // The flight runs along the wall, not out from it.
                assert!(st.down.dot(st.out).abs() < 1e-9);
                // The railing stands on the open side, as high as the landing.
                assert!(st.rails[0].len() >= 3);
                assert!((st.rails[0][0].y - (2.0 + RAIL_H)).abs() < 1e-9);
            }
        }
        assert!(seen > 20, "only {seen} stoops");
    }

    #[test]
    fn a_tall_drop_turns_back_and_stays_within_reach() {
        let mut turned = 0;
        for p in plans() {
            let size = p.site.size;
            for drop in [6.0, 12.0, 25.0] {
                for st in stoops(&p, drop, &|_| 0.0) {
                    if !st.platforms.is_empty() {
                        turned += 1;
                    }
                    // Down to the ground, a rise or two at a time (a
                    // platform is a step lower than the tread before it).
                    let mut y = drop;
                    for t in &st.treads {
                        assert!(t.y1 < y && y - t.y1 <= 2.0 * RISE + 1e-9 || drop > 15.0);
                        y = t.y1;
                    }
                    // Nothing reaches out of the building's footprint by
                    // more than colliders look for.
                    for b in st.blocks() {
                        assert!(b.rect.min.x > -REACH && b.rect.min.y > -REACH);
                        assert!(b.rect.max.x < size.x + REACH && b.rect.max.y < size.y + REACH);
                    }
                    // Every platform has a flight on both sides: more treads
                    // than platforms.
                    assert!(st.treads.len() > st.platforms.len());
                }
            }
        }
        assert!(turned > 20, "{turned} stoops turned back");
    }

    #[test]
    fn the_flight_goes_down_the_slope() {
        // The ground rises towards -x along the front: the flight of a door
        // in a front wall runs that way, to meet the ground sooner.
        let mut checked = 0;
        for p in plans() {
            let site = &p.site;
            let fall = |q: DVec2| 1.0 - (q - site.origin).dot(site.along) * 0.1;
            let all = stoops(&p, 2.0, &fall);
            // (With more than one door the flights also keep out of each
            // other's way.)
            if all.len() != 1 {
                continue;
            }
            for st in all {
                if st.out.x.abs() < 0.5 && !st.treads.is_empty() {
                    assert!(st.down.x < 0.0, "the flight runs away from the ground: door {:?} out {:?} down {:?} treads {}", st.at, st.out, st.down, st.treads.len());
                    checked += 1;
                }
            }
        }
        assert!(checked > 10, "{checked} checked");
    }
}

#[cfg(test)]
mod town {
    use super::*;
    use crate::urban::world::TownWorld;
    use crate::urban::TownParams;

    #[ignore = "daily (slow blocker): scripts/check.sh --daily"]
    /// In a real town, on slopes: every stoop reaches the ground it stands
    /// on, stays within what colliders look for, and keeps to the wall.
    #[test]
    fn stoops_in_a_town_reach_the_ground_and_stay_near_their_building() {
        for seed in [2u64, 5] {
            let sd = hw_core::Seed(seed);
            let tw = TownWorld::generate(sd, TownParams::sample(sd, 20_000.0));
            let (mut n, mut lanes) = (0, 0);
            for (_, b) in tw.everything().buildings.iter() {
                let plan = BuildingPlan::generate(&b.site);
                for st in stoops(&plan, b.base, &|q| tw.ground(q)) {
                    n += 1;
                    lanes += !st.platforms.is_empty() as u32;
                    // The last tread is within a step of the ground there.
                    if let Some(t) = st.treads.last() {
                        assert!(
                            t.y1 - st.foot_ground <= 0.45,
                            "seed {seed}: last tread {:.2} m over the ground",
                            t.y1 - st.foot_ground
                        );
                    } else {
                        assert!(st.top - st.foot_ground < 0.45 + RISE);
                    }
                    for blk in st.blocks() {
                        assert!(blk.rect.min.x > -REACH && blk.rect.min.y > -REACH);
                        let size = b.site.size;
                        assert!(blk.rect.max.x < size.x + REACH && blk.rect.max.y < size.y + REACH);
                    }
                    // Within its own front along the wall (but a landing
                    // by a corner).
                    let size = b.site.size;
                    let (lo, hi) = if st.out.x.abs() > 0.5 {
                        (0.0, size.y)
                    } else {
                        (0.0, size.x)
                    };
                    for t in &st.treads {
                        let (a, z) = if st.out.x.abs() > 0.5 {
                            (t.rect.min.y, t.rect.max.y)
                        } else {
                            (t.rect.min.x, t.rect.max.x)
                        };
                        assert!(
                            a >= lo - 0.2 && z <= hi + 0.2,
                            "a tread past its building's wall"
                        );
                    }
                }
            }
            eprintln!("seed {seed}: {n} stoops, {lanes} turn back");
            assert!(n > 100);
        }
    }
}
