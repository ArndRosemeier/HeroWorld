//! The police: Sky Patrol cruisers and the officers who ride in them.
//!
//! A cruiser is called to a place ([`Police::call`]): a door, and the way
//! it looks out. Over the next ticks open ground near the door is looked
//! for, a candidate or two a tick (never a long search in one tick): room
//! for the whole car with space to walk round it, off every carriageway,
//! level, open to the sky, with a clear walk to the door. Then the cruiser
//! flies in above the roofs, sinks straight down and lands on its skids,
//! the canopy swings up, and two officers step out and walk to posts either
//! side of the door. They stand guard a while, walk back, get in, and the
//! cruiser climbs away. Where no ground is free it hovers over the door
//! for a while instead, and nobody gets out.
//!
//! The cruiser ([`EntityKind::PoliceCar`](hw_net::EntityKind::PoliceCar))
//! and its officers ([`EntityKind::Officer`](hw_net::EntityKind::Officer))
//! are replicated entities. A cruiser standing still is solid
//! ([`cruiser_solid`], the box the model is built in), on the host and for
//! every player alike.

use crate::collision::{Obstacles, Solid};
use crate::{LastChange, Motion, Net, Orientation, DT, TICK_HZ};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::det;
use hw_gen::urban::area::Area;
use hw_gen::water::segment_distance;
use std::sync::Arc;

/// The cruiser's box: half its width and half its length (m).
pub const CRUISER_HALF: DVec2 = DVec2::new(1.4, 2.75);
/// The box's middle lies this far ahead of the cruiser's own position.
pub const CRUISER_MID: f64 = 0.35;
/// The box's height above the skids' feet (m).
pub const CRUISER_HEIGHT: f64 = 1.45;
/// Seconds the canopy takes to swing open or shut.
pub const CANOPY: f64 = 1.2;
/// How many looks officers come in.
pub const LOOKS: u8 = 2;
/// A cruiser slower than this stands (and is solid), m/s.
pub const STANDING: f64 = 0.1;

/// Seconds after the call before the cruiser sets out.
const ANSWER: f64 = 2.0;
/// Speeds (m/s): sinking and climbing, and crossing town.
const SINK: f64 = 7.0;
const CRUISE: f64 = 42.0;
/// It comes in from this far off (m).
const FROM: f64 = 220.0;
/// The officers' pace (m/s).
const WALK: f64 = 1.5;
/// Seconds they stand guard at the door.
const GUARD: f64 = 40.0;
/// Seconds a cruiser with nowhere to land hovers over the door.
const WATCH: f64 = 25.0;
/// How high it hovers then, over the ground (m).
const WATCH_HEIGHT: f64 = 9.0;
/// Room kept free round the landed cruiser (m).
const ROOM: f64 = 0.35;
/// The ground under the cruiser and round it is a plane to within this
/// (m): no kerb, no steps.
const LEVEL: f64 = 0.12;
/// The steepest ground it lands on (rise over run): a steep street.
const SLOPE: f64 = 0.3;
/// Cars keep off the road this far round a cruiser standing in it (the
/// [`AlarmKind::Police`](crate::alarms::AlarmKind::Police) alarm it
/// raises): this near it, and this near their posts by the door, its
/// officers may walk in the road.
const ROAD_HELD: f64 = 6.0;
/// With no hero this near, the police are gone at once (m).
const NOBODY: f64 = 320.0;
/// Candidates looked at per tick while finding a place to land.
const PER_TICK: usize = 2;

/// A Sky Patrol cruiser (replicated as [`hw_net::EntityKind::PoliceCar`];
/// its `grounded` says the canopy is open).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cruiser;

/// A police officer (replicated as [`hw_net::EntityKind::Officer`]).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Officer {
    /// Which of the [`LOOKS`] they have.
    pub look: u8,
}

/// The solid box of a cruiser standing at `pos` (its skids' feet), its
/// nose towards `yaw`: what [`CRUISER_HALF`], [`CRUISER_MID`] and
/// [`CRUISER_HEIGHT`] say, which is what is drawn.
pub fn cruiser_solid(pos: DVec3, yaw: f64) -> Solid {
    let fwd = DVec2::new(det::sin(yaw), det::cos(yaw));
    let mid = DVec2::new(pos.x, pos.z) + fwd * CRUISER_MID;
    Solid {
        origin: mid,
        along: DVec2::new(fwd.y, -fwd.x),
        inward: fwd,
        min: -CRUISER_HALF,
        max: CRUISER_HALF,
        y0: pos.y,
        y1: pos.y + CRUISER_HEIGHT,
    }
}

/// The solid boxes of the cruisers standing still among `all` (for a
/// joined player's hero: the host puts its own into the world).
pub fn solids_of(all: &[hw_net::EntityState]) -> Vec<Solid> {
    all.iter()
        .filter(|e| {
            e.kind == hw_net::EntityKind::PoliceCar
                && glam::Vec3::from_array(e.vel).length() < STANDING as f32
        })
        .map(|e| cruiser_solid(DVec3::from_array(e.pos), crate::robots::rotation_yaw(e.rot)))
        .collect()
}

/// Where a cruiser lands, and where its officers go.
#[derive(Clone, Debug, PartialEq)]
pub struct Landing {
    /// The cruiser's position landed (its skids' feet) and its nose.
    pub at: DVec3,
    pub yaw: f64,
    /// How it stands there: its nose to `yaw`, tipped as the ground lies.
    pub rot: [f32; 4],
    /// Each officer's walk from the cruiser's side to their post.
    pub ways: Vec<Vec<DVec3>>,
    /// The way they face on guard.
    pub out: DVec2,
}

/// One straight, eased stretch of a cruiser's flight.
#[derive(Clone, Copy, Debug)]
struct Leg {
    a: DVec3,
    b: DVec3,
    secs: f64,
}

fn hold(p: DVec3, secs: f64) -> Leg {
    Leg { a: p, b: p, secs }
}

fn leg(a: DVec3, b: DVec3, speed: f64) -> Leg {
    Leg {
        a,
        b,
        secs: a.distance(b) / speed + 1.5,
    }
}

/// Where the legs have got to after `t` seconds, and how fast.
fn fly(legs: &[Leg], t: f64) -> (DVec3, DVec3) {
    let at = |t: f64| {
        let mut left = t.max(0.0);
        for l in legs {
            if left <= l.secs {
                let u = left / l.secs;
                let e = u * u * (3.0 - 2.0 * u);
                return l.a.lerp(l.b, e);
            }
            left -= l.secs;
        }
        legs.last().map_or(DVec3::ZERO, |l| l.b)
    };
    let p = at(t);
    (p, (at(t + DT) - p) / DT)
}

fn yaw_quat(yaw: f64) -> [f32; 4] {
    let (s, c) = det::sin_cos(yaw * 0.5);
    [0.0, s as f32, 0.0, c as f32]
}

fn ticks(seconds: f64) -> u64 {
    (seconds * TICK_HZ as f64).round() as u64
}

/// A carriageway's middle line near a call: from, to, half width.
type RoadPiece = (DVec2, DVec2, f64);

/// A call whose landing place is still being looked for.
struct Search {
    door: DVec3,
    out: DVec2,
    /// The officers' posts by the door.
    posts: Vec<DVec3>,
    roads: Vec<RoadPiece>,
    /// The places to try, best first.
    places: Vec<(DVec2, DVec2)>,
    /// Where people stand about: no cruiser lands on them.
    people: Vec<DVec2>,
    /// The next of them to look at.
    next: usize,
    /// Not before this tick does the cruiser set out.
    answer_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Flying in and sinking.
    Coming,
    /// Landed, the canopy swinging up, since this tick.
    Opening(u64),
    /// The officers out, until this tick.
    Guard(u64),
    /// The officers walking back and getting in.
    Returning,
    /// Hovering over the door until this tick (nowhere to land).
    Watching(u64),
    /// The canopy shut, off again.
    Leaving,
}

/// An officer out of the cruiser.
struct Walker {
    e: Entity,
    /// The whole walk, cruiser to post.
    way: Vec<DVec3>,
    /// The next point of it they make for (walking back: counted down).
    next: usize,
}

struct Patrol {
    car: Entity,
    stage: Stage,
    legs: Vec<Leg>,
    /// Tick the current legs began.
    since: u64,
    yaw: f64,
    landing: Option<Landing>,
    officers: Vec<Walker>,
    /// Where it was called to.
    door: DVec3,
}

/// The cruisers out on calls.
#[derive(Default)]
pub struct Police {
    town: Option<Arc<Area>>,
    searches: Vec<Search>,
    patrols: Vec<Patrol>,
    /// Screenshots: cruisers answer at once and set down from a metre up.
    pub quick: bool,
}

impl Police {
    pub fn new(town: Arc<Area>) -> Self {
        Self {
            town: Some(town),
            ..Self::default()
        }
    }

    /// Another square of the town.
    pub fn set_town(&mut self, town: Arc<Area>) {
        self.town = Some(town);
    }

    /// How many cruisers are out (or about to set out).
    pub fn out(&self) -> usize {
        self.searches.len() + self.patrols.len()
    }

    /// Where the cruisers out on calls land (or `None`: they only hover).
    pub fn landings(&self) -> Vec<Option<Landing>> {
        self.patrols.iter().map(|p| p.landing.clone()).collect()
    }

    /// The cruisers standing still, as solids: heroes, villains and cars
    /// do not pass through them (what is drawn is what collides).
    pub fn solids(&self, world: &World) -> Vec<Solid> {
        self.patrols
            .iter()
            .filter_map(|p| {
                let m = world.get::<Motion>(p.car)?;
                (m.0.vel.length() < STANDING).then(|| cruiser_solid(m.0.pos, p.yaw))
            })
            .collect()
    }

    /// Calls a cruiser to the door at `door` (feet), which looks out
    /// along `out`. One cruiser per place: a second call near is dropped.
    /// It lands on none of `people` (whoever stands about there now).
    pub fn call(
        &mut self,
        solids: &Obstacles,
        door: DVec3,
        out: DVec2,
        people: &[DVec3],
        tick: u64,
    ) {
        let out = out.normalize_or(DVec2::X);
        let near = |p: DVec3| p.distance(door) < 40.0;
        if self.searches.iter().any(|s| near(s.door)) || self.patrols.iter().any(|p| near(p.door)) {
            return;
        }
        let mut s = search(self.town.as_deref(), solids, door, out);
        s.answer_at = tick + if self.quick { 0 } else { ticks(ANSWER) };
        s.people = people
            .iter()
            .filter(|p| p.distance(door) < 60.0)
            .map(|p| DVec2::new(p.x, p.z))
            .collect();
        self.searches.push(s);
    }

    /// Takes every cruiser and officer out of the world.
    pub fn clear(&mut self, world: &mut World) {
        self.searches.clear();
        for p in self.patrols.drain(..) {
            for o in p.officers {
                world.despawn(o.e);
            }
            world.despawn(p.car);
        }
    }
}

/// The height of the open ground at `q`: the police keep to the street
/// (a door may lie up a flight of steps from it).
fn street(solids: &Obstacles, q: DVec2) -> f64 {
    solids.ground_at(q).max(crate::GROUND_Y)
}

/// A search for where to land for a call to `door`, at its start.
fn search(town: Option<&Area>, solids: &Obstacles, door: DVec3, out: DVec2) -> Search {
    let roads = town.map_or_else(Vec::new, |t| {
        let d = DVec2::new(door.x, door.z);
        t.roads
            .iter()
            .flat_map(|r| r.pts.windows(2).map(move |w| (w[0], w[1], r.half)))
            .filter(|&(a, b, half)| segment_distance(d, a, b) < 60.0 + half)
            .collect()
    });
    Search {
        door,
        out,
        posts: posts(solids, door, out),
        places: candidates(door, out, &roads),
        people: Vec::new(),
        roads,
        next: 0,
        answer_at: 0,
    }
}

/// Where a cruiser called to `door` (looking out along `out`) lands: the
/// whole search at once (the game spreads it over ticks; this is for
/// tests and tools).
pub fn find_landing(
    town: Option<&Area>,
    solids: &Obstacles,
    door: DVec3,
    out: DVec2,
) -> Option<Landing> {
    let s = search(town, solids, door, out.normalize_or(DVec2::X));
    if s.posts.len() < 2 {
        return None;
    }
    (0..)
        .map_while(|k| candidate(&s, k))
        .find_map(|(c, fwd)| try_landing(solids, &s, c, fwd).ok())
}

/// Why each place a cruiser might land at for a call to `door` will not
/// do, counted (for tests and tools; "no posts" when nobody could stand
/// by the door at all).
pub fn why_not(
    town: Option<&Area>,
    solids: &Obstacles,
    door: DVec3,
    out: DVec2,
) -> std::collections::BTreeMap<&'static str, usize> {
    let s = search(town, solids, door, out.normalize_or(DVec2::X));
    let mut why = std::collections::BTreeMap::new();
    if s.posts.len() < 2 {
        *why.entry("no posts").or_default() += 1;
    }
    for (c, fwd) in (0..).map_while(|k| candidate(&s, k)) {
        if let Err(e) = try_landing(solids, &s, c, fwd) {
            *why.entry(e).or_default() += 1;
        }
    }
    why
}

/// The posts either side of a door at `door` looking out along `out`:
/// just outside, a step and a half to each side, where there is room to
/// stand. Fewer than two where there is not (then nobody gets out).
fn posts(solids: &Obstacles, door: DVec3, out: DVec2) -> Vec<DVec3> {
    let side = out.perp();
    let flat = DVec2::new(door.x, door.z);
    let free = |q: DVec2| {
        let y = street(solids, q);
        let p = DVec3::new(q.x, y, q.y);
        ((y - door.y).abs() < 4.0
            && solids.inside(p + DVec3::Y * 0.5).is_none()
            && crate::combat::room_to_stand(solids, q, y))
        .then_some(p)
    };
    let mut found = Vec::new();
    for k in [-1.0, 1.0] {
        let post = [1.6, 2.3, 3.0, 3.8, 4.8, 6.0]
            .iter()
            .flat_map(|&d| [1.5, 2.2, 1.0, 3.0].map(|s| flat + out * d + side * (k * s)))
            .find_map(free);
        found.extend(post);
    }
    found
}

/// The middle of the officers' posts by the door (x, z). In the old
/// town's lanes a house's door opens straight onto the road, so the
/// posts stand in it: a second police alarm there keeps the cars off.
fn guard_spot(posts: &[DVec3]) -> DVec2 {
    let n = posts.len().max(1) as f64;
    posts
        .iter()
        .fold(DVec2::ZERO, |a, p| a + DVec2::new(p.x, p.z))
        / n
}

/// The places a cruiser might land for a call, best first: its box's
/// middle and its nose. First in the street by the door, standing along
/// it as a car would (in the near lane, then the middle of the road),
/// nearest the door first; then open ground in front of the door, along
/// the house front and across it.
fn candidates(door: DVec3, out: DVec2, roads: &[RoadPiece]) -> Vec<(DVec2, DVec2)> {
    let d = DVec2::new(door.x, door.z);
    let mut street: Vec<(f64, DVec2, DVec2)> = Vec::new();
    for &(a, b, half) in roads {
        let len = a.distance(b);
        if len < 1e-6 || segment_distance(d, a, b) > half + 16.0 {
            continue;
        }
        let dir = (b - a) / len;
        let along = (d - a).dot(dir);
        // The side of the road the door is on.
        let side = dir.perp() * (d - a).dot(dir.perp()).signum();
        for step in [0.0, 5.0, -5.0, 10.0, -10.0, 15.0, -15.0] {
            let t = along + step;
            if t < CRUISER_HALF.y || t > len - CRUISER_HALF.y {
                continue;
            }
            for lane in [(half * 0.5).min(half - CRUISER_HALF.x - 0.3).max(0.0), 0.0] {
                let c = a + dir * t + side * lane;
                // Its nose the way that lane drives matters to nobody:
                // the way that keeps the door on one side.
                street.push((c.distance(d) + step.abs() * 0.01, c, dir));
            }
        }
    }
    street.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut all: Vec<(DVec2, DVec2)> = street.into_iter().map(|(_, c, f)| (c, f)).collect();
    all.truncate(40);
    let side = out.perp();
    for dist in [6.5, 9.0, 11.5, 14.0, 17.0, 20.0, 24.0] {
        for s in [0.0, 4.0, -4.0, 8.0, -8.0, 12.0, -12.0, 16.0, -16.0] {
            let c = d + out * dist + side * s;
            all.push((c, side));
            all.push((c, out));
        }
    }
    all
}

/// The `k`-th place to try for `s`.
fn candidate(s: &Search, k: usize) -> Option<(DVec2, DVec2)> {
    s.places.get(k).copied()
}

/// Is `q` inside the cruiser's box round `c` (nose `fwd`) grown by `grow`?
fn in_box(q: DVec2, c: DVec2, fwd: DVec2, grow: f64) -> bool {
    let d = q - c;
    d.dot(fwd).abs() < CRUISER_HALF.y + grow && d.dot(fwd.perp()).abs() < CRUISER_HALF.x + grow
}

/// A cruiser landed with its box's middle at `c`, nose along `fwd`, for
/// the call `s`: `None` unless the whole box and room to walk round it is
/// open, level ground off every carriageway under an open sky, and each
/// officer has a clear walk to their post.
fn try_landing(
    solids: &Obstacles,
    s: &Search,
    c: DVec2,
    fwd: DVec2,
) -> Result<Landing, &'static str> {
    let right = DVec2::new(fwd.y, -fwd.x);
    // The ground under the box as a plane: its height in the middle and
    // how it rises to the nose and to the side.
    let (gn, gt) = (
        street(solids, c + fwd * CRUISER_HALF.y),
        street(solids, c - fwd * CRUISER_HALF.y),
    );
    let (gr, gl) = (
        street(solids, c + right * CRUISER_HALF.x),
        street(solids, c - right * CRUISER_HALF.x),
    );
    let y = (gn + gt + gr + gl) * 0.25;
    let rise = DVec2::new(
        (gr - gl) / (2.0 * CRUISER_HALF.x),
        (gn - gt) / (2.0 * CRUISER_HALF.y),
    );
    let plane = |q: DVec2| y + (q - c).dot(right) * rise.x + (q - c).dot(fwd) * rise.y;
    if s.posts.iter().any(|p| (y - p.y).abs() > 3.0) {
        return Err("another height");
    }
    if s.people.iter().any(|&q| in_box(q, c, fwd, 1.0)) {
        return Err("somebody stands there");
    }
    if rise.abs().max_element() > SLOPE {
        return Err("too steep");
    }
    let off_road = |q: DVec2| {
        s.roads
            .iter()
            .all(|&(a, b, half)| segment_distance(q, a, b) > half + 0.15)
    };
    // The box and a little room round it, a point every 0.8 m or so
    // (the officers' walks are looked at each on its own, below).
    let (hx, hz) = (CRUISER_HALF.x + ROOM, CRUISER_HALF.y + ROOM);
    let (nx, nz) = ((hx / 0.4).ceil() as i32, (hz / 0.4).ceil() as i32);
    let points = move || {
        (-nx..=nx).flat_map(move |i| {
            (-nz..=nz).map(move |j| {
                c + right * (hx * i as f64 / nx as f64) + fwd * (hz * j as f64 / nz as f64)
            })
        })
    };
    if !points().all(|q| (street(solids, q) - plane(q)).abs() < LEVEL) {
        return Err("not even");
    }
    if !points().all(|q| {
        solids
            .inside(DVec3::new(q.x, plane(q) + 0.5, q.y))
            .is_none()
    }) {
        return Err("in a building");
    }
    let open = |q: DVec2| {
        [0.3, 0.9, 1.6]
            .iter()
            .all(|h| !solids.solid_at(DVec3::new(q.x, plane(q) + h, q.y)))
    };
    if !points().all(open) {
        return Err("something stands there");
    }
    let mid = DVec3::new(c.x, y, c.y);
    if solids.line_blocked(mid + DVec3::Y * 70.0, mid + DVec3::Y * 2.0) {
        return Err("no open sky");
    }
    // Each officer's walk: out at their side, round the cruiser's end if
    // it stands in the way, and straight to their post.
    // The road is held round the cruiser and round the posts by the door
    // (see `guard_spot`): there the officers may walk in it.
    let by_door = guard_spot(&s.posts);
    let walkable = |a: DVec3, b: DVec3| {
        let n = (a.distance(b) / 0.5).ceil().max(1.0) as usize;
        let mut last = a.y;
        let ground = (0..=n).all(|i| {
            let p = a.lerp(b, i as f64 / n as f64);
            let q = DVec2::new(p.x, p.z);
            let g = street(solids, q);
            let held = q.distance(c) < ROAD_HELD || q.distance(by_door) < ROAD_HELD;
            let ok = (g - last).abs() < 0.35 && (off_road(q) || held);
            last = g;
            ok
        });
        ground
            && [0.4, 1.3]
                .iter()
                .all(|&h| !solids.line_blocked(a + DVec3::Y * h, b + DVec3::Y * h))
    };
    let on_ground = |q: DVec2, _near: f64| DVec3::new(q.x, street(solids, q), q.y);
    let through = |a: DVec3, b: DVec3| {
        let n = (a.distance(b) / 0.25).ceil().max(1.0) as usize;
        (0..=n).any(|i| {
            let p = a.lerp(b, i as f64 / n as f64);
            in_box(DVec2::new(p.x, p.z), c, fwd, 0.35)
        })
    };
    let step_out = CRUISER_HALF.x + 0.6;
    let mut ways = Vec::new();
    for (k, &post) in s.posts.iter().enumerate() {
        let k = if k == 0 { -1.0 } else { 1.0 };
        // The officer nearer the post on that side gets out there.
        let side = if (DVec2::new(post.x, post.z) - c).dot(right) * k >= 0.0 || s.posts.len() == 1 {
            k
        } else {
            -k
        };
        let start = on_ground(c + right * (side * step_out), y);
        // The end of the cruiser nearer the post.
        let end = if (DVec2::new(post.x, post.z) - c).dot(fwd) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let corner = |sd: f64| {
            on_ground(
                c + right * (sd * step_out) + fwd * (end * (CRUISER_HALF.y + 0.6)),
                y,
            )
        };
        let mut routes = vec![
            vec![start, post],
            vec![start, corner(side), post],
            vec![start, corner(side), corner(-side), post],
        ];
        // Round whatever stands between (a lamp, a tree, a parked car):
        // by a point out in front of the door, or one out from the post.
        let door = DVec2::new(s.door.x, s.door.z);
        let flat = DVec2::new(post.x, post.z);
        let mut by: Vec<DVec3> = Vec::new();
        for d in [2.5, 4.5, 7.0] {
            by.push(on_ground(flat + s.out * d, y));
            for t in [0.0, -3.0, 3.0] {
                by.push(on_ground(door + s.out * (d + 1.0) + s.out.perp() * t, y));
            }
        }
        for &p in &by {
            routes.push(vec![start, p, post]);
            routes.push(vec![start, corner(side), p, post]);
            routes.push(vec![start, corner(side), corner(-side), p, post]);
        }
        let way = routes.into_iter().find(|w| {
            w.windows(2)
                .all(|p| !through(p[0], p[1]) && walkable(p[0], p[1]))
        });
        let Some(way) = way else {
            return Err("no walk to the door");
        };
        ways.push(way);
    }
    // Two officers never get out at the same side on top of each other.
    if ways.len() == 2 && ways[0][0].distance(ways[1][0]) < 0.5 {
        let w = &mut ways[1];
        let back = DVec3::new(fwd.x, 0.0, fwd.y) * -1.0;
        w[0] += back;
    }
    // It stands on the slope as the ground lies: nose, side and up.
    let nose = DVec3::new(fwd.x, rise.y, fwd.y).normalize();
    let up = nose.cross(DVec3::new(right.x, rise.x, right.y)).normalize();
    let side = up.cross(nose);
    let rot = glam::DQuat::from_mat3(&glam::DMat3::from_cols(side, up, nose)).normalize();
    let at = mid - nose * CRUISER_MID;
    Ok(Landing {
        at,
        yaw: det::atan2(fwd.x, fwd.y),
        rot: rot.as_quat().to_array(),
        ways,
        out: s.out,
    })
}

/// A height to fly from `a` to `b` at, above the roofs on the way.
fn above_roofs(town: Option<&Area>, a: DVec3, b: DVec3) -> f64 {
    crate::taxi::express_height(town, a, b)
}

fn spawn_cruiser(world: &mut World, tick: u64, at: DVec3, yaw: f64) -> Entity {
    let id = world.resource_mut::<crate::Ids>().0.allocate();
    world
        .spawn((
            Net(id),
            Motion(crate::collision::Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Orientation(yaw_quat(yaw)),
            LastChange(tick),
            Cruiser,
        ))
        .id()
}

fn spawn_officer(world: &mut World, tick: u64, at: DVec3, yaw: f64, look: u8) -> Entity {
    let id = world.resource_mut::<crate::Ids>().0.allocate();
    world
        .spawn((
            Net(id),
            Motion(crate::collision::Body {
                pos: at,
                vel: DVec3::ZERO,
                grounded: true,
            }),
            Orientation(yaw_quat(yaw)),
            LastChange(tick),
            Officer { look },
        ))
        .id()
}

/// The cruiser sets out for `s`: to land at `landing`, or (with `None`)
/// to hover over the door if there is room in the air there.
fn set_out(
    world: &mut World,
    town: Option<&Area>,
    solids: &Obstacles,
    s: &Search,
    landing: Option<Landing>,
    tick: u64,
    quick: bool,
) -> Option<Patrol> {
    let (goal, yaw) = match &landing {
        Some(l) => (l.at, l.yaw),
        None => {
            let side = s.out.perp();
            let yaw = det::atan2(side.x, side.y);
            let c = DVec2::new(s.door.x, s.door.z) + s.out * 7.0;
            let y = street(solids, c) + WATCH_HEIGHT;
            let at = DVec3::new(c.x, y, c.y) - DVec3::new(side.x, 0.0, side.y) * CRUISER_MID;
            let b = cruiser_solid(at, yaw);
            let clear = [-1.0, 0.0, 1.0].iter().all(|&i| {
                [-1.0, 0.0, 1.0].iter().all(|&j| {
                    let q = b.to_world(DVec2::new(CRUISER_HALF.x * i, CRUISER_HALF.y * j));
                    [0.1, 0.8, 1.4]
                        .iter()
                        .all(|h| !solids.solid_at(DVec3::new(q.x, y + h, q.y)))
                })
            });
            let mid = DVec3::new(c.x, y, c.y);
            if !clear || solids.line_blocked(mid + DVec3::Y * 70.0, mid + DVec3::Y * 2.0) {
                return None;
            }
            (at, yaw)
        }
    };
    let fwd = DVec3::new(det::sin(yaw), 0.0, det::cos(yaw));
    let far = goal - fwd * FROM;
    let high = above_roofs(town, far, goal);
    let from = DVec3::new(far.x, high + 30.0, far.z);
    let over = DVec3::new(goal.x, high, goal.z);
    let (from, over) = if quick {
        (goal + DVec3::Y * 1.2, goal + DVec3::Y * 1.0)
    } else {
        (from, over)
    };
    let car = spawn_cruiser(world, tick, from, yaw);
    Some(Patrol {
        car,
        stage: Stage::Coming,
        legs: if quick {
            vec![hold(over, 0.1), leg(over, goal, SINK * 4.0)]
        } else {
            vec![leg(from, over, CRUISE), leg(over, goal, SINK)]
        },
        since: tick,
        yaw,
        landing,
        officers: Vec::new(),
        door: s.door,
    })
}

/// Walks `w` on towards point `to` of its way; true once there.
fn walk(world: &mut World, solids: &Obstacles, w: &Walker, to: DVec3) -> bool {
    let Some(mut m) = world.get_mut::<Motion>(w.e) else {
        return true;
    };
    let pos = m.0.pos;
    let d = DVec2::new(to.x - pos.x, to.z - pos.z);
    let len = d.length();
    let step = WALK * DT;
    if len <= step {
        m.set_if_neq(Motion(crate::collision::Body {
            pos: to,
            vel: DVec3::ZERO,
            grounded: true,
        }));
        return true;
    }
    let dir = d / len;
    let q = DVec2::new(pos.x, pos.z) + dir * step;
    let y = street(solids, q);
    m.0 = crate::collision::Body {
        pos: DVec3::new(q.x, y, q.y),
        vel: DVec3::new(dir.x, 0.0, dir.y) * WALK,
        grounded: true,
    };
    let rot = yaw_quat(det::atan2(dir.x, dir.y));
    if let Some(mut o) = world.get_mut::<Orientation>(w.e) {
        o.set_if_neq(Orientation(rot));
    }
    false
}

/// One tick of the police: looking for places to land, flying, landing,
/// the officers' walks. `heroes` is where every hero is.
pub fn step(
    world: &mut World,
    police: &mut Police,
    solids: &Obstacles,
    alarms: &mut crate::alarms::Alarms,
    heroes: &[DVec3],
    tick: u64,
) {
    let town = police.town.clone();
    // Finding where to land, a little each tick.
    let mut k = 0;
    while k < police.searches.len() {
        let s = &mut police.searches[k];
        let mut found = None;
        let mut over = s.posts.len() < 2;
        for _ in 0..PER_TICK {
            if over {
                break;
            }
            match candidate(s, s.next) {
                Some((c, fwd)) => {
                    s.next += 1;
                    found = try_landing(solids, s, c, fwd).ok();
                    if found.is_some() {
                        break;
                    }
                }
                None => over = true,
            }
        }
        if (found.is_some() || over) && tick >= s.answer_at {
            let s = police.searches.remove(k);
            let quick = police.quick;
            if let Some(p) = set_out(world, town.as_deref(), solids, &s, found, tick, quick) {
                police.patrols.push(p);
            }
        } else if found.is_some() {
            // Found before the cruiser may set out: this one again then.
            s.next -= 1;
            k += 1;
        } else {
            k += 1;
        }
    }

    let mut done = Vec::new();
    for (i, p) in police.patrols.iter_mut().enumerate() {
        let t = (tick - p.since) as f64 * DT;
        let total: f64 = p.legs.iter().map(|l| l.secs).sum();
        let (pos, vel) = fly(&p.legs, t);
        let open = matches!(
            p.stage,
            Stage::Opening(_) | Stage::Guard(_) | Stage::Returning
        );
        if let Ok(mut e) = world.get_entity_mut(p.car) {
            if let Some(mut m) = e.get_mut::<Motion>() {
                m.set_if_neq(Motion(crate::collision::Body {
                    pos,
                    vel,
                    grounded: open,
                }));
            }
            // Level in the air; over the last metres down it tips to lie
            // as the ground does.
            if let (Some(l), Some(mut o)) = (&p.landing, e.get_mut::<Orientation>()) {
                let u = (1.0 - (pos.y - l.at.y) / 4.0).clamp(0.0, 1.0) as f32;
                let level = glam::Quat::from_array(yaw_quat(p.yaw));
                let rot = level.slerp(glam::Quat::from_array(l.rot), u);
                o.set_if_neq(Orientation(rot.to_array()));
            }
        }
        // Coming down into a street, standing in it and lifting off: the
        // cars keep off the road round it and wait.
        if let Some(l) = &p.landing {
            if pos.y - l.at.y < 45.0 && (p.stage != Stage::Leaving || pos.y - l.at.y < 3.0) {
                let mid = cruiser_solid(l.at, l.yaw).to_world(DVec2::ZERO);
                alarms.raise_or_refresh(
                    crate::alarms::AlarmKind::Police,
                    DVec3::new(mid.x, l.at.y, mid.y),
                    tick,
                    TICK_HZ,
                );
                // And round the posts by the door, where they are not
                // the same place.
                let posts: Vec<DVec3> = l.ways.iter().filter_map(|w| w.last().copied()).collect();
                let g = guard_spot(&posts);
                if !posts.is_empty() && g.distance(mid) > ROAD_HELD * 0.5 {
                    alarms.raise_or_refresh(
                        crate::alarms::AlarmKind::Police,
                        DVec3::new(g.x, posts[0].y, g.y),
                        tick,
                        TICK_HZ,
                    );
                }
            }
        }
        // Nobody left to see it: gone.
        if heroes.iter().all(|h| h.distance(p.door) > NOBODY) {
            done.push(i);
            continue;
        }
        let arrived = t >= total;
        match p.stage {
            Stage::Coming if arrived => {
                p.legs = vec![hold(pos, f64::MAX)];
                p.since = tick;
                p.stage = match &p.landing {
                    Some(_) => Stage::Opening(tick),
                    None => Stage::Watching(tick + ticks(WATCH)),
                };
            }
            Stage::Opening(since) if tick >= since + ticks(CANOPY) => {
                // Out they get, each at their side.
                let ways = p.landing.as_ref().map_or_else(Vec::new, |l| l.ways.clone());
                for (k, way) in ways.into_iter().enumerate() {
                    let look = ((world.get::<Net>(p.car).map_or(0, |n| n.0 .0) as usize + k)
                        % LOOKS as usize) as u8;
                    let e = spawn_officer(world, tick, way[0], p.yaw, look);
                    p.officers.push(Walker { e, way, next: 1 });
                }
                p.stage = Stage::Guard(tick + ticks(GUARD));
            }
            Stage::Guard(until) => {
                let out = p.landing.as_ref().map_or(DVec2::X, |l| l.out);
                for w in &mut p.officers {
                    if w.next < w.way.len() && walk(world, solids, w, w.way[w.next]) {
                        w.next += 1;
                        if w.next == w.way.len() {
                            // On their post: facing the street.
                            if let Some(mut o) = world.get_mut::<Orientation>(w.e) {
                                o.0 = yaw_quat(det::atan2(out.x, out.y));
                            }
                        }
                    }
                }
                if tick >= until {
                    for w in &mut p.officers {
                        w.next = w.next.min(w.way.len() - 1).saturating_sub(1);
                    }
                    p.stage = Stage::Returning;
                }
            }
            Stage::Returning => {
                let mut k = 0;
                while k < p.officers.len() {
                    let w = &mut p.officers[k];
                    let to = w.way[w.next];
                    if walk(world, solids, w, to) {
                        if w.next == 0 {
                            // In they get.
                            let w = p.officers.remove(k);
                            world.despawn(w.e);
                            continue;
                        }
                        w.next -= 1;
                    }
                    k += 1;
                }
                if p.officers.is_empty() {
                    leave(p, town.as_deref(), pos, tick, CANOPY + 0.6);
                }
            }
            Stage::Watching(until) if tick >= until => {
                leave(p, town.as_deref(), pos, tick, 0.0);
            }
            Stage::Leaving if arrived => done.push(i),
            _ => {}
        }
    }
    for i in done.into_iter().rev() {
        let p = police.patrols.remove(i);
        for o in p.officers {
            world.despawn(o.e);
        }
        world.despawn(p.car);
    }
}

/// The cruiser at `pos` shuts its canopy (`wait` seconds) and flies off.
fn leave(p: &mut Patrol, town: Option<&Area>, pos: DVec3, tick: u64, wait: f64) {
    let fwd = DVec3::new(det::sin(p.yaw), 0.0, det::cos(p.yaw));
    let far = pos + fwd * FROM;
    let high = above_roofs(town, pos, far);
    let over = DVec3::new(pos.x, high, pos.z);
    p.legs = vec![
        hold(pos, wait),
        leg(pos, over, SINK),
        leg(over, DVec3::new(far.x, high + 30.0, far.z), CRUISE),
    ];
    p.since = tick;
    p.stage = Stage::Leaving;
}
