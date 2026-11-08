//! The town's people on foot.
//!
//! Every citizen is a pure function of the town seed, its id and the time:
//! where it lives (a street), the route it walks from there through a few
//! connected streets, how fast it walks, where it pauses, and its look. No
//! state is kept, so every machine in a co-op game sees the same people in
//! the same places without sending them, and only the citizens near a
//! player are ever worked out.
//!
//! Citizens keep to the pavements: a route runs along one side of each
//! street, crosses at the corners, and turns back at its ends after a pause
//! (looking in a shop window, waiting for someone). A few just stand and
//! chat. More people walk downtown and on main roads than in the outskirts.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::geom::Polyline;
use hw_gen::urban::world::{TownWorld, WorldRoad};
use std::collections::BTreeMap;

/// How many different looks citizens are drawn with (the client builds
/// this many characters; a citizen's look is one of them).
pub const LOOKS: u32 = 24;

/// Walkers per metre of pavement (both sides together) at the town's core
/// and in the outskirts.
const DENSITY_CORE: f64 = 0.22;
const DENSITY_EDGE: f64 = 0.04;
/// Main roads are busier than side streets.
const MAIN_FACTOR: f64 = 1.5;
/// Size of the grid cells that index streets by place.
const CELL: f64 = 64.0;
/// Street ends closer than this to another street are joined to it.
const JOIN: f64 = 4.0;

/// A citizen as seen at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Citizen {
    pub id: u64,
    /// Feet on the pavement, in the town's frame.
    pub pos: DVec3,
    /// The way it faces (unit, in the ground plane).
    pub facing: DVec2,
    /// Walking speed now (0 while it stands).
    pub speed: f64,
    /// Which of the [`LOOKS`] it has.
    pub look: u32,
}

/// One leg of a route: along a street's pavement from one distance to
/// another (either way), or straight across from one pavement to the next.
#[derive(Clone, Copy, Debug)]
enum Leg {
    Street {
        road: u32,
        side: f64,
        from: f64,
        to: f64,
    },
    Cross(DVec2, DVec2),
}

impl Leg {
    fn len(&self) -> f64 {
        match *self {
            Leg::Street { from, to, .. } => (to - from).abs(),
            Leg::Cross(a, b) => a.distance(b),
        }
    }
}

/// A street's pavement centre line, measured.
struct Walk {
    line: Polyline,
    /// Pavement offset from the centre line.
    offset: f64,
    /// Walkers living on this street.
    people: u32,
}

impl Walk {
    fn len(&self) -> f64 {
        self.line.len()
    }

    /// Point and direction at distance `s`, `side` = -1 left or 1 right.
    fn sample(&self, s: f64, side: f64) -> (DVec2, DVec2) {
        let (k, t) = self.line.locate(s);
        let (a, b) = (self.line.pts[k], self.line.pts[k + 1]);
        let dir = (b - a).normalize_or_zero();
        (a.lerp(b, t) + dir.perp() * self.offset * side, dir)
    }

    /// Distance along the line of the point nearest `p` (and how far off).
    fn nearest(&self, p: DVec2) -> (f64, f64) {
        self.line.nearest(p)
    }
}

/// The citizens of one town.
pub struct Citizens {
    seed: Seed,
    walks: Vec<Walk>,
    /// Streets met at each street end: (end 0 or 1) -> (street, distance).
    joins: Vec<[Vec<(u32, f64)>; 2]>,
    /// Streets by grid cell (each street in every cell it passes).
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    world: std::sync::Arc<TownWorld>,
}

impl Citizens {
    /// Plans where the town's people walk (streets, joins, how many per
    /// street). Takes a few milliseconds.
    pub fn new(world: std::sync::Arc<TownWorld>) -> Self {
        let seed = world.seed.child("citizens");
        let core = world.spawn_line().0;
        let reach = world.half().max(200.0) * 0.6;
        let mut walks = Vec::with_capacity(world.roads.len());
        for (i, r) in world.roads.iter().enumerate() {
            walks.push(walk_of(r, core, reach, seed.child(i as u64)));
        }
        let mut grid: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
        for (i, w) in walks.iter().enumerate() {
            let mut cells = Vec::new();
            for p in &w.line.pts {
                cells.push(cell_of(*p));
            }
            let pts = &w.line.pts;
            for k in 0..pts.len().saturating_sub(1) {
                let n = (pts[k].distance(pts[k + 1]) / (CELL * 0.5)).ceil() as usize;
                for j in 1..n {
                    cells.push(cell_of(pts[k].lerp(pts[k + 1], j as f64 / n as f64)));
                }
            }
            cells.sort();
            cells.dedup();
            for c in cells {
                grid.entry(c).or_default().push(i as u32);
            }
        }
        let mut joins = Vec::with_capacity(walks.len());
        for (i, w) in walks.iter().enumerate() {
            let mut ends: [Vec<(u32, f64)>; 2] = [Vec::new(), Vec::new()];
            for (e, p) in [w.line.pts[0], *w.line.pts.last().expect("walk has points")]
                .into_iter()
                .enumerate()
            {
                let mut near = Vec::new();
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let c = cell_of(p);
                        if let Some(list) = grid.get(&(c.0 + dx, c.1 + dy)) {
                            near.extend(list.iter().copied());
                        }
                    }
                }
                near.sort();
                near.dedup();
                for j in near {
                    if j as usize == i {
                        continue;
                    }
                    let (s, d) = walks[j as usize].nearest(p);
                    if d < JOIN + world.roads[j as usize].half {
                        ends[e].push((j, s));
                    }
                }
            }
            joins.push(ends);
        }
        Self {
            seed,
            walks,
            joins,
            grid,
            world,
        }
    }

    /// How many citizens the town has out on its streets.
    pub fn count(&self) -> u64 {
        self.walks.iter().map(|w| w.people as u64).sum()
    }

    /// Every citizen within `radius` of `around` at `time` (seconds of game
    /// time), nearest first, at most `max`.
    pub fn near(&self, around: DVec2, radius: f64, time: f64, max: usize) -> Vec<Citizen> {
        // Routes stay within a few streets of home: look a little further.
        let reach = radius + 400.0;
        let (c0, c1) = (
            cell_of(around - DVec2::splat(reach)),
            cell_of(around + DVec2::splat(reach)),
        );
        let mut homes = Vec::new();
        for x in c0.0..=c1.0 {
            for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                homes.extend(list.iter().copied());
            }
        }
        homes.sort();
        homes.dedup();
        let mut out = Vec::new();
        for road in homes {
            for n in 0..self.walks[road as usize].people {
                let id = ((road as u64) << 20) | n as u64;
                if let Some(c) = self.citizen(id, time) {
                    if c.pos.truncate_xz().distance(around) <= radius {
                        out.push(c);
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            let da = a.pos.truncate_xz().distance_squared(around);
            let db = b.pos.truncate_xz().distance_squared(around);
            da.total_cmp(&db).then(a.id.cmp(&b.id))
        });
        out.truncate(max);
        out
    }

    /// Citizen `id` at `time`; `None` if there is no such citizen.
    pub fn citizen(&self, id: u64, time: f64) -> Option<Citizen> {
        let road = (id >> 20) as usize;
        let n = (id & 0xF_FFFF) as u32;
        let home = self.walks.get(road)?;
        if n >= home.people {
            return None;
        }
        let s = self.seed.child(id);
        let mut rng = s.rng();
        let look = (s.child("look").0 % LOOKS as u64) as u32;
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let start = rng.range(0.0, home.len());
        // Some stand about (chatting, waiting for a taxi).
        if rng.chance(0.1) {
            let (p, d) = home.sample(start, side);
            let turn = rng.range(-2.5, 2.5);
            let facing = DVec2::from_angle(turn).rotate(d);
            return Some(self.at(id, p, facing, 0.0, look));
        }
        let speed = rng.range(1.1, 1.6);
        let pause = rng.range(3.0, 14.0);
        let legs = self.route(road as u32, start, side, &mut rng);
        let length: f64 = legs.iter().map(Leg::len).sum();
        if length < 1.0 {
            let (p, d) = home.sample(start, side);
            return Some(self.at(id, p, d, 0.0, look));
        }
        // There and back with a pause at each end, offset so the town's
        // people are spread over their routes.
        let walk_t = length / speed;
        let period = 2.0 * (walk_t + pause);
        let t = (time + rng.range(0.0, period)).rem_euclid(period);
        let (dist, moving, back) = if t < walk_t {
            (t * speed, true, false)
        } else if t < walk_t + pause {
            (length, false, false)
        } else if t < 2.0 * walk_t + pause {
            (length - (t - walk_t - pause) * speed, true, true)
        } else {
            (0.0, false, true)
        };
        let (p, mut dir) = self.along(&legs, dist);
        if back {
            dir = -dir;
        }
        if !moving {
            // Turned towards the shops while waiting.
            dir = dir.perp() * if back { -side } else { side };
        }
        Some(self.at(id, p, dir, if moving { speed } else { 0.0 }, look))
    }

    fn at(&self, id: u64, p: DVec2, facing: DVec2, speed: f64, look: u32) -> Citizen {
        Citizen {
            id,
            pos: DVec3::new(p.x, self.world.road_height(p) + 0.06, p.y),
            facing: facing.normalize_or(DVec2::Y),
            speed,
            look,
        }
    }

    /// From home along the street, then through up to three more streets
    /// met at the ends (a walk to work, the shops, a friend's).
    fn route(&self, road: u32, start: f64, side: f64, rng: &mut hw_core::seed::Rng) -> Vec<Leg> {
        let mut legs = Vec::new();
        let mut road = road;
        let mut from = start;
        let mut side = side;
        let hops = rng.range_int(1, 4);
        for _ in 0..hops {
            let w = &self.walks[road as usize];
            let end = if rng.chance(0.5) { 0 } else { 1 };
            let to = if end == 0 { 0.0 } else { w.len() };
            legs.push(Leg::Street {
                road,
                side,
                from,
                to,
            });
            let next = &self.joins[road as usize][end];
            if next.is_empty() {
                break;
            }
            let (r, s) = *rng.pick(next);
            // Round the corner onto the nearer pavement of the next street;
            // now and then cross over to the far one.
            let a = w.sample(to, side).0;
            let nw = &self.walks[r as usize];
            let near_side = if nw.sample(s, 1.0).0.distance(a) <= nw.sample(s, -1.0).0.distance(a) {
                1.0
            } else {
                -1.0
            };
            let next_side = if rng.chance(0.2) {
                -near_side
            } else {
                near_side
            };
            let b = nw.sample(s, next_side).0;
            legs.push(Leg::Cross(a, b));
            road = r;
            from = s;
            side = next_side;
        }
        legs
    }

    /// Point and heading `dist` metres along a route.
    fn along(&self, legs: &[Leg], dist: f64) -> (DVec2, DVec2) {
        let mut left = dist;
        for (i, l) in legs.iter().enumerate() {
            let len = l.len();
            if left <= len || i + 1 == legs.len() {
                match *l {
                    Leg::Street {
                        road,
                        side,
                        from,
                        to,
                    } => {
                        let w = &self.walks[road as usize];
                        let forward = to >= from;
                        let s = if forward { from + left } else { from - left };
                        let (p, d) = w.sample(s, side);
                        return (p, if forward { d } else { -d });
                    }
                    Leg::Cross(a, b) => {
                        let t = (left / len.max(1e-9)).clamp(0.0, 1.0);
                        return (a.lerp(b, t), (b - a).normalize_or(DVec2::Y));
                    }
                }
            }
            left -= len;
        }
        (DVec2::ZERO, DVec2::Y)
    }
}

fn cell_of(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

fn walk_of(r: &WorldRoad, core: DVec2, reach: f64, seed: Seed) -> Walk {
    let line = Polyline::new(&r.pts);
    let len = line.len();
    let mid = line.pts[line.pts.len() / 2];
    let t = (mid.distance(core) / reach).clamp(0.0, 1.0);
    // Busy at the core, quiet at the edge (smoothly).
    let k = 1.0 - t * t * (3.0 - 2.0 * t);
    let mut density = DENSITY_EDGE + (DENSITY_CORE - DENSITY_EDGE) * k;
    if r.main {
        density *= MAIN_FACTOR;
    }
    // A road without pavement has no walkers.
    if r.pavement < 0.8 {
        density = 0.0;
    }
    let want = len * density;
    let people = (want.floor() + if seed.unit() < want.fract() { 1.0 } else { 0.0 }) as u32;
    Walk {
        line,
        offset: r.half + r.pavement * 0.5,
        people: people.min(0xF_FFFF),
    }
}

trait Xz {
    fn truncate_xz(self) -> DVec2;
}

impl Xz for DVec3 {
    fn truncate_xz(self) -> DVec2 {
        DVec2::new(self.x, self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedTown;

    fn town() -> Citizens {
        let t = GeneratedTown::generate(Seed(1), 3000.0);
        Citizens::new(t.world)
    }

    #[test]
    fn same_seed_same_people() {
        let (a, b) = (town(), town());
        let p = a.world.spawn_line().0;
        let x = a.near(p, 150.0, 1234.5, 500);
        let y = b.near(p, 150.0, 1234.5, 500);
        assert!(!x.is_empty(), "nobody on the streets");
        assert_eq!(x, y);
    }

    #[test]
    fn citizens_walk_on_pavements_and_move_smoothly() {
        let c = town();
        let p = c.world.spawn_line().0;
        let people = c.near(p, 200.0, 50.0, 300);
        assert!(
            people.len() > 20,
            "only {} people near the core",
            people.len()
        );
        for a in &people {
            let b = c.citizen(a.id, 50.0 + 1.0 / 30.0).unwrap();
            let step = a.pos.distance(b.pos);
            // At most a walking step per tick (plus a corner crossing).
            assert!(step < 0.5, "citizen {} jumped {step} m", a.id);
            assert!(a.pos.is_finite());
            assert!(a.look < LOOKS);
        }
        let walking = people.iter().filter(|c| c.speed > 0.0).count();
        assert!(walking * 2 > people.len(), "most people should be walking");
    }

    #[test]
    fn finding_people_near_a_player_is_quick() {
        let t = GeneratedTown::generate(Seed(1), 40_000.0);
        let c = Citizens::new(t.world);
        let p = c.world.spawn_line().0;
        let start = std::time::Instant::now();
        let near = c.near(p, 260.0, 100.0, 600);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = std::time::Instant::now();
        for x in &near {
            c.citizen(x.id, 100.1);
        }
        let each = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "{} of {} near the core in {ms:.1} ms; moving them {each:.2} ms",
            near.len(),
            c.count()
        );
        assert!(!near.is_empty());
    }
}
