//! Traffic: golden-age cars on the streets, hover cars and flying taxis in
//! lanes over the main roads.
//!
//! The lanes come from `hw_gen::urban::lanes`: closed loops through the
//! town. Streets near the town's core and main roads are busier.
//!
//! Street cars are a small simulation every machine runs alike, stepped
//! [`STEP`] at a time from the same start: they keep their distance from
//! the car ahead, and where lanes cross in a junction the one that gets
//! there later gives way (whoever has waited long goes first, so nothing
//! jams for good). A friend who joins late is sent the cars as they are
//! ([`Traffic::sync_out`]). A wrecked car stands still and the lane queues
//! behind it.
//!
//! Fliers do not need to give way (their lanes never cross at one height):
//! each is a pure function of the seed, its id and the time.
//!
//! The sky lanes follow the main roads at set heights above them (three
//! layers), so fliers keep clear of the buildings; most fliers are taxis.
//!
//! Buses run on the street lanes that pass bus stops: a few cars of such a
//! lane are buses. A bus pulls up with its middle door at each stop on its
//! lane (on the kerb side), stands there [`DWELL`] seconds while people get
//! off and on (drawn by the client), and drives on; the lane queues behind
//! it as behind any car.

use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::area::Area;
use hw_gen::urban::lanes::{plan_lanes, street_lanes, LaneLoop};
use hw_gen::urban::props::{CarModel, PropKind, CAR_PAINTS};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Heights of the sky lanes above the road under them (m).
pub const SKY_LAYERS: [f64; 3] = [34.0, 46.0, 58.0];
/// Grid cell for finding lanes near a point (m).
const CELL: f64 = 64.0;
/// Closest spacing of cars on one lane at the town's core, and in the
/// outskirts (m); fliers keep further apart.
const GAP_CORE: f64 = 34.0;
const GAP_EDGE: f64 = 140.0;
const SKY_GAP_CORE: f64 = 48.0;
const SKY_GAP_EDGE: f64 = 170.0;
/// Bit set in the ids of fliers.
pub const SKY_BIT: u64 = 1 << 40;
/// Street cars are simulated this many seconds at a time.
pub const STEP: f64 = 0.1;
/// A car's length (m): positions are its middle.
const CAR_LEN: f64 = 4.8;
/// A street car's width and roof height (m), for bumping into.
const CAR_WIDTH: f64 = 2.0;
const CAR_HEIGHT: f64 = 1.45;
/// A bus: its length, width and roof height (m).
pub const BUS_LEN: f64 = 11.2;
pub const BUS_WIDTH: f64 = 2.55;
pub const BUS_HEIGHT: f64 = 3.2;
/// Liveries buses come in.
pub const BUS_LIVERIES: u8 = 4;
/// Seconds a bus stands at a stop.
pub const DWELL: f64 = 16.0;
/// A bus's top speed (m/s).
const BUS_SPEED: f64 = 10.5;
/// About one bus per this much lane (m), on lanes that pass stops.
const BUS_EVERY: f64 = 1600.0;
/// A bus stop serves the kerb lane within this distance of its shelter (m).
const STOP_REACH: f64 = 10.0;
/// Bit set in the `waited` of a bus sent to a friend ([`Traffic::sync_out`])
/// once it has served the stop it stands at.
const SERVED: u32 = 1 << 31;
/// Lanes closer than this where they cross (m) conflict.
const CONFLICT: f64 = 2.4;
/// How far ahead a driver looks for a junction to give way at (m).
const LOOK: f64 = 15.0;
/// After waiting this long (s) a driver goes first; after
/// [`IMPATIENT`] they go whatever happens.
const PATIENT: f64 = 4.0;
const IMPATIENT: f64 = 12.0;
/// Stuck this long (s), a driver drives on through anything but the car
/// ahead: a jam round a block clears.
const STUCK: f64 = 30.0;
/// Stuck this long (s), a car turns off the road (it is not drawn and
/// holds nobody up), and comes back where there is room again.
const PARK: f64 = 60.0;
/// Crossings closer than this (m) are one junction.
const JUNCTION: f64 = 16.0;
/// Longest stretch of lane one crossing takes up (m; they are found within
/// this).
const MAX_ZONE: f64 = 20.0;
/// Traffic lights: one cycle (s). Each way has its green, an all-red
/// moment follows each, and then every way is red while people on foot
/// cross, in every direction at once (a "scramble", as the 1950s
/// introduced them), before an all-red moment again.
pub const CYCLE: f64 = 36.0;
const ALL_RED: f64 = 2.0;
/// The walk phase (s).
pub const WALK: f64 = 8.0;
/// Each way's green (s).
const GREEN: f64 = (CYCLE - 3.0 * ALL_RED - WALK) * 0.5;
/// Cars stop this far before a junction's first crossing: clear of the
/// crosswalk in front of it.
const CROSSWALK: f64 = 4.0;
/// A car stops this far (m) short of someone standing in its lane.
const HOLD_GAP: f64 = 2.5;
/// Cars see people in the road and trouble this far ahead (m).
const HOLD_LOOK: f64 = 60.0;
/// Someone this near a lane's middle (m) is in the way.
const IN_LANE: f64 = 2.3;
/// Junctions nearer each other than this (m) run their lights in step.
const IN_STEP: f64 = 45.0;
/// A red light is seen this far ahead (m).
const RED_LOOK: f64 = 45.0;
/// The last seconds of green show amber.
const AMBER: f64 = 3.0;

/// What a traffic light shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Red,
    Amber,
    Green,
}
/// A town's traffic lights: where each junction is and where it is in its
/// cycle. Cars stop at them; people on foot cross in their walk phase.
#[derive(Clone, Debug, Default)]
pub struct Signals {
    /// Each junction's middle and the axis its phase 0 runs along.
    junctions: Vec<(DVec2, DVec2)>,
    /// Where each junction's lights are in their cycle at time 0.
    offsets: Vec<f64>,
    /// Junctions by grid cell.
    grid: BTreeMap<(i64, i64), Vec<u32>>,
}

impl Signals {
    fn new(junctions: Vec<(DVec2, DVec2)>, offsets: Vec<f64>) -> Self {
        let mut grid: BTreeMap<(i64, i64), Vec<u32>> = BTreeMap::new();
        for (j, (c, _)) in junctions.iter().enumerate() {
            grid.entry(cell_of(*c)).or_default().push(j as u32);
        }
        Self {
            junctions,
            offsets,
            grid,
        }
    }

    /// The junction nearest `at` within `r` (at most [`CELL`]) m.
    pub fn junction_near(&self, at: DVec2, r: f64) -> Option<u32> {
        let c = cell_of(at);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1 {
            for dz in -1..=1 {
                for &j in self.grid.get(&(c.0 + dx, c.1 + dz)).into_iter().flatten() {
                    let d = self.junctions[j as usize].0.distance(at);
                    if d <= r && best.is_none_or(|(b, k)| (d, j) < (b, k)) {
                        best = Some((d, j));
                    }
                }
            }
        }
        best.map(|(_, j)| j)
    }

    /// The junction a traffic light at `at` belongs to, and the phase of
    /// the traffic it faces (it looks along `facing`, at the cars coming
    /// in); none if no junction is near.
    pub fn light_of(&self, at: DVec2, facing: DVec2) -> Option<(u32, u8)> {
        let j = self.junction_near(at, 40.0)?;
        let axis = self.junctions[j as usize].1;
        let phase = u8::from(facing.dot(axis).abs() < std::f64::consts::FRAC_1_SQRT_2);
        Some((j, phase))
    }

    /// What a light of `phase` at junction `j` shows at `time`.
    pub fn signal(&self, j: u32, phase: u8, time: f64) -> Signal {
        if !self.green(j, phase, time) {
            return Signal::Red;
        }
        // Amber for the last few seconds of green.
        if self.green(j, phase, time + AMBER) {
            Signal::Green
        } else {
            Signal::Amber
        }
    }

    /// Where junction `j` is in its cycle at `time` (0 to [`CYCLE`]).
    fn cycle_at(&self, j: u32, time: f64) -> f64 {
        (time + self.offsets[j as usize]).rem_euclid(CYCLE)
    }

    /// Whether `phase` has green at junction `j` at `time`.
    fn green(&self, j: u32, phase: u8, time: f64) -> bool {
        let u = self.cycle_at(j, time);
        if phase == 0 {
            u < GREEN
        } else {
            (GREEN + ALL_RED..GREEN * 2.0 + ALL_RED).contains(&u)
        }
    }

    /// Whether people on foot cross at junction `j` at `time` (or the cars
    /// are about to stop for them).
    pub fn walking(&self, j: u32, time: f64) -> bool {
        let u = self.cycle_at(j, time);
        u >= GREEN * 2.0 + ALL_RED * 1.5
    }

    /// When someone at junction `j`'s kerb at `time` steps out to cross
    /// `len` metres, and by when they must be over: they go in the walk
    /// phase, a moment after the cars have stopped, and only if they can
    /// make it across at a brisk pace before it ends.
    pub fn crossing(&self, j: u32, time: f64, len: f64) -> (f64, f64) {
        const BRISK: f64 = 2.2;
        let walk = GREEN * 2.0 + ALL_RED * 2.0;
        let u = self.cycle_at(j, time);
        let base = time - u;
        let first = walk + 1.0;
        let last = (walk + WALK + ALL_RED * 0.4 - len / BRISK).max(first);
        let start = if u < first {
            base + first
        } else if u <= last {
            time
        } else {
            base + CYCLE + first
        };
        // The walk phase stepped out in.
        let window = start - (start - base).rem_euclid(CYCLE) + walk;
        (start, window + WALK + ALL_RED * 0.4)
    }
}

/// Room (m) between the cars either side for a car to come back.
const ROOM_BACK: f64 = 40.0;
/// Driving: comfortable acceleration and braking (m/s²), time gap kept to
/// the car ahead (s) and distance kept standing (m).
const ACCEL: f64 = 2.2;
const BRAKE: f64 = 3.5;
const HEADWAY: f64 = 1.2;
const STANDSTILL: f64 = 2.0;
/// Further behind than this (s), the cars are put where they would be
/// without junctions instead of being driven there.
const CATCH_UP: f64 = 120.0;

/// What a vehicle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VehicleKind {
    /// A car on the street (model, paint).
    Car(CarModel, u8),
    /// A private hover car in a sky lane (paint).
    SkyCar(u8),
    /// A flying taxi.
    Taxi,
    /// A streamliner bus on a route through the stops (livery).
    Bus(u8),
}

impl VehicleKind {
    /// Its length, width and roof height (m).
    pub fn size(self) -> (f64, f64, f64) {
        match self {
            VehicleKind::Bus(_) => (BUS_LEN, BUS_WIDTH, BUS_HEIGHT),
            _ => (CAR_LEN, CAR_WIDTH, CAR_HEIGHT),
        }
    }
}

/// A vehicle as seen at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vehicle {
    pub id: u64,
    pub kind: VehicleKind,
    /// Centre of its underside, in the town's frame.
    pub pos: DVec3,
    /// The way its nose points (unit; climbs and dips on hills).
    pub dir: DVec3,
    /// Lean into turns (radians, positive to the right), for fliers.
    pub roll: f64,
    /// Speed (m/s).
    pub speed: f64,
    /// Distance driven along its lane (turns the wheels).
    pub odometer: f64,
}

impl Vehicle {
    pub fn flies(&self) -> bool {
        self.id & SKY_BIT != 0
    }

    /// Yaw (radians, 0 = +z) and pitch (radians, positive nose up).
    pub fn yaw_pitch(&self) -> (f64, f64) {
        let flat = DVec2::new(self.dir.x, self.dir.z);
        (
            hw_core::det::atan2(flat.x, flat.y),
            hw_core::det::atan2(self.dir.y, flat.length()),
        )
    }
}

/// One loop of lanes and the vehicles on it.
struct Lane {
    path: LaneLoop,
    /// Vehicles on the loop.
    count: u32,
    speed: f64,
    /// Where vehicle 0 is at time 0.
    phase: f64,
    /// Height above the road (0 on the ground).
    lift: f64,
    /// Which of its cars are buses (empty if none are).
    buses: Vec<bool>,
    /// The bus stops on it: where a bus's middle stands (round the loop,
    /// sorted) and the stop's number in [`Traffic::bus_stops`].
    stops: Vec<(f64, u32)>,
}

impl Lane {
    fn is_bus(&self, k: usize) -> bool {
        self.buses.get(k).copied().unwrap_or(false)
    }

    /// Half the length of car `k`.
    fn half(&self, k: usize) -> f64 {
        if self.is_bus(k) {
            BUS_LEN * 0.5
        } else {
            CAR_LEN * 0.5
        }
    }

    /// Car `k`'s top speed.
    fn top(&self, k: usize) -> f64 {
        if self.is_bus(k) {
            self.speed.min(BUS_SPEED)
        } else {
            self.speed
        }
    }

    /// The next stop a bus at `s` that has served every stop before `left`
    /// pulls up at: where its middle stands (unwrapped near `s`), and the
    /// stop's number.
    fn next_stop(&self, s: f64, left: f64) -> Option<(f64, u32)> {
        if self.stops.is_empty() {
            return None;
        }
        let len = self.path.len();
        // A stop more than a little behind is gone by.
        let from = left.max(s - 1.5);
        let base = from - from.rem_euclid(len);
        let m = from.rem_euclid(len);
        let i = self.stops.partition_point(|st| st.0 < m);
        Some(match self.stops.get(i) {
            Some(&(at, n)) => (base + at, n),
            None => (base + len + self.stops[0].0, self.stops[0].1),
        })
    }
}

/// A bus stop the buses serve: where its shelter stands, the way it faces
/// (to the road) and the height of its foot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusStop {
    pub pos: DVec2,
    pub dir: DVec2,
    pub y: f64,
}

/// A bus standing at a stop (see [`Traffic::bus_at_stop`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtStop {
    /// The stop's number in [`Traffic::bus_stops`].
    pub stop: u32,
    /// Seconds it has stood there (it leaves at [`DWELL`]).
    pub standing: f64,
}

/// A stretch of lane in a grid cell: (sky, lane, from, to).
type Span = (bool, u32, f64, f64);

/// A point on a street lane: (lane, distance along, where, which way).
type LanePoint = (u32, f64, DVec2, DVec2);

/// One street car's state.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Car {
    /// Distance along its lane (grows without wrapping; larger is ahead).
    s: f64,
    v: f64,
    /// How many steps it has stood.
    waited: u32,
    wrecked: bool,
    /// Off the road for now (out of a jam).
    parked: bool,
    /// A bus: it has served every stop before this distance along.
    left: f64,
}

/// Where another lane crosses this one: the stretch of this lane
/// (`from`..`to`, round the loop) and of the other.
#[derive(Clone, Copy, Debug)]
struct Zone {
    from: f64,
    to: f64,
    other: u32,
    other_from: f64,
    other_to: f64,
    /// The junction it is in, this lane's light there and the other's.
    junction: u32,
    phase: u8,
    other_phase: u8,
}

impl Car {
    /// How long it has stood (s).
    fn wait(&self) -> f64 {
        self.waited as f64 * STEP
    }
}

/// Roughly how long a car `d` metres from a crossing, going `v`, takes to
/// get there (from standing, it speeds up).
fn eta(d: f64, v: f64) -> f64 {
    if v > 2.0 {
        d / v
    } else {
        (2.0 * d / ACCEL).sqrt().min(d / v.max(0.1))
    }
}

/// The cars (by number) whose middle is within `w` metres after `from`
/// round a loop of length `len`; `order` is the cars' places round the
/// loop, sorted.
fn in_stretch(
    order: &[(f64, usize)],
    from: f64,
    w: f64,
    len: f64,
) -> impl Iterator<Item = usize> + '_ {
    let a = from.rem_euclid(len);
    let i = order.partition_point(|o| o.0 < a);
    let n = order.len();
    (0..n)
        .map(move |k| order[(i + k) % n])
        .take_while(move |o| ahead(a, o.0, len) <= w)
        .map(|o| o.1)
}

/// Distance from `a` forward to `b` round a loop of length `len`.
fn ahead(a: f64, b: f64, len: f64) -> f64 {
    (b - a).rem_euclid(len)
}

/// The town's traffic.
pub struct Traffic {
    seed: Seed,
    ground: Vec<Lane>,
    sky: Vec<Lane>,
    /// Stretches of lane by grid cell: (sky, lane, from, to).
    grid: BTreeMap<(i64, i64), Vec<Span>>,
    world: Arc<Area>,
    /// Where each street lane is crossed by others, by `from`.
    zones: Vec<Vec<Zone>>,
    /// The junctions and their lights.
    signals: Arc<Signals>,
    /// What street cars stop for, by lane: distances along it (round the
    /// loop) of heroes standing in the road and of trouble ahead (see
    /// [`Self::set_blockers`]).
    holds: Vec<Vec<f64>>,
    /// Spots the road is blocked by something standing on it (a crash's
    /// wrecks): no street car is on the road within them (see
    /// [`Self::set_blocked`]).
    blocked: Vec<(DVec2, f64)>,
    /// The bus stops buses serve.
    bus_stops: Vec<BusStop>,
    /// The street cars now and one step ago, by lane, in order along it.
    cars: Vec<Vec<Car>>,
    prev: Vec<Vec<Car>>,
    /// The time of `cars`.
    time: f64,
}

impl Traffic {
    /// Plans the lanes and how many vehicles each carries (a fraction of a
    /// second for a big town).
    pub fn new(world: Arc<Area>) -> Self {
        let seed = world.seed.child("traffic");
        let core = world.spawn_line().0;
        let reach = world.half().max(200.0) * 0.6;
        // Busy at the core, quiet at the edge (smoothly).
        let busy = |p: DVec2| {
            let t = (p.distance(core) / reach).clamp(0.0, 1.0);
            1.0 - t * t * (3.0 - 2.0 * t)
        };
        // Cars keep to their lane on the right, clear of parked cars; the
        // narrowest streets have no traffic, and none passes a barricade.
        let ground_loops = street_lanes(&world, &world.roads);
        let mains: Vec<usize> = (0..world.roads.len())
            .filter(|&i| world.roads[i].main)
            .collect();
        let sky_loops = plan_lanes(
            &world.roads,
            &mains,
            |r| r.half * 0.4,
            &[],
            seed.child("sky"),
        );
        let make = |path: LaneLoop, sky: bool, i: usize| -> Lane {
            let s = seed
                .child(if sky { "sky lane" } else { "lane" })
                .child(i as u64);
            let mut rng = s.rng();
            // Average busyness along the loop.
            let len = path.len();
            let n = (len / 25.0).ceil().max(1.0) as usize;
            let b = (0..n)
                .map(|k| busy(path.sample(len * k as f64 / n as f64).0))
                .sum::<f64>()
                / n as f64;
            let b = (b * (0.7 + 0.6 * path.main)).min(1.0);
            let (near, far) = if sky {
                (SKY_GAP_CORE, SKY_GAP_EDGE)
            } else {
                (GAP_CORE, GAP_EDGE)
            };
            let gap = far + (near - far) * b;
            let want = len / gap * rng.range(0.7, 1.3);
            let count = want.floor() as u32 + u32::from(rng.unit() < want.fract());
            let (speed, lift) = if sky {
                (
                    rng.range(17.0, 25.0),
                    SKY_LAYERS[rng.index(SKY_LAYERS.len())],
                )
            } else {
                (rng.range(8.0, 12.5) * (1.0 + 0.25 * path.main), 0.0)
            };
            Lane {
                count: count.min(0xFFFF),
                speed,
                phase: rng.range(0.0, len),
                lift,
                path,
                buses: Vec::new(),
                stops: Vec::new(),
            }
        };
        let ground: Vec<Lane> = ground_loops
            .into_iter()
            .enumerate()
            .map(|(i, l)| make(l, false, i))
            .collect();
        let sky: Vec<Lane> = sky_loops
            .into_iter()
            .enumerate()
            .map(|(i, l)| make(l, true, i))
            .collect();
        let mut grid: BTreeMap<(i64, i64), Vec<Span>> = BTreeMap::new();
        for (flies, lanes) in [(false, &ground), (true, &sky)] {
            for (i, l) in lanes.iter().enumerate() {
                if l.count == 0 {
                    continue;
                }
                for (c, a, b) in l.path.spans(CELL) {
                    grid.entry(c).or_default().push((flies, i as u32, a, b));
                }
            }
        }
        Self {
            seed,
            ground,
            sky,
            grid,
            zones: Vec::new(),
            signals: Arc::new(Signals::default()),
            cars: Vec::new(),
            prev: Vec::new(),
            holds: Vec::new(),
            blocked: Vec::new(),
            bus_stops: Vec::new(),
            time: 0.0,
            world,
        }
        .with_zones()
    }

    /// Finds where street lanes cross, and puts the cars at time 0.
    fn with_zones(mut self) -> Self {
        // Points every metre along every lane, in a grid.
        let mut grid: BTreeMap<(i64, i64), Vec<LanePoint>> = BTreeMap::new();
        let key = |p: DVec2| ((p.x / 4.0).floor() as i64, (p.y / 4.0).floor() as i64);
        for (i, l) in self.ground.iter().enumerate() {
            if l.count == 0 {
                continue;
            }
            let len = l.path.len();
            let n = len.ceil() as usize;
            for k in 0..n {
                let a = len * k as f64 / n as f64;
                let (p, d) = l.path.sample(a);
                grid.entry(key(p)).or_default().push((i as u32, a, p, d));
            }
        }
        // Close points of two lanes crossing at an angle, by lane pair.
        let mut hits: BTreeMap<(u32, u32), Vec<(f64, f64)>> = BTreeMap::new();
        for (k, list) in &grid {
            for dx in 0..=1 {
                for dz in -1..=1 {
                    if dx == 0 && dz < 0 {
                        continue;
                    }
                    let Some(other) = grid.get(&(k.0 + dx, k.1 + dz)) else {
                        continue;
                    };
                    let same = dx == 0 && dz == 0;
                    for (ia, &(la, a, pa, da)) in list.iter().enumerate() {
                        for (ib, &(lb, b, pb, db)) in other.iter().enumerate() {
                            if same && ib <= ia {
                                continue;
                            }
                            if pa.distance(pb) >= CONFLICT || da.dot(db).abs() > 0.9 {
                                continue;
                            }
                            if la == lb {
                                let len = self.ground[la as usize].path.len();
                                if ahead(a, b, len).min(ahead(b, a, len)) < 30.0 {
                                    continue;
                                }
                            }
                            let ((la, a), (lb, b)) = if (la, a) <= (lb, b) {
                                ((la, a), (lb, b))
                            } else {
                                ((lb, b), (la, a))
                            };
                            hits.entry((la, lb)).or_default().push((a, b));
                        }
                    }
                }
            }
        }
        let mut zones: Vec<Vec<Zone>> = vec![Vec::new(); self.ground.len()];
        for ((la, lb), mut pts) in hits {
            let (len_a, len_b) = (
                self.ground[la as usize].path.len(),
                self.ground[lb as usize].path.len(),
            );
            pts.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
            // Runs of nearby points are one crossing. The points come in
            // order along lane a, so only runs with a point less than 3 m
            // back can take the next one (a long loop crossing itself
            // has thousands of points: comparing all of them took minutes).
            let near = |q: &(f64, f64), p: &(f64, f64)| {
                ahead(q.0, p.0, len_a).min(ahead(p.0, q.0, len_a)) < 3.0
                    && ahead(q.1, p.1, len_b).min(ahead(p.1, q.1, len_b)) < 3.0
            };
            let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
            let mut open: Vec<usize> = Vec::new();
            for p in pts {
                open.retain(|&r| runs[r].last().is_some_and(|q| q.0 >= p.0 - 3.0));
                let hit = open.iter().copied().find(|&r| {
                    runs[r]
                        .iter()
                        .rev()
                        .take_while(|q| q.0 >= p.0 - 3.0)
                        .any(|q| near(q, &p))
                });
                match hit {
                    Some(r) => runs[r].push(p),
                    None => {
                        open.push(runs.len());
                        runs.push(vec![p]);
                    }
                }
            }
            // A crossing over the loop's start: its two ends are one run.
            let mut k = 0;
            while k < runs.len() {
                if runs[k].first().is_some_and(|q| q.0 < 3.0) {
                    let wraps = (0..runs.len()).filter(|&j| j != k).find(|&j| {
                        runs[j].last().is_some_and(|q| q.0 > len_a - 3.0)
                            && runs[j].iter().any(|q| runs[k].iter().any(|p| near(q, p)))
                    });
                    if let Some(j) = wraps {
                        let moved = std::mem::take(&mut runs[k]);
                        runs[j].extend(moved);
                    }
                }
                k += 1;
            }
            runs.retain(|r| !r.is_empty());
            for r in runs {
                // The stretch on each lane: from the first point to the
                // last (round the loop), a little wider.
                let span = |vals: Vec<f64>, len: f64| {
                    let first = vals[0];
                    let rel: Vec<f64> = vals
                        .iter()
                        .map(|v| {
                            let d = ahead(first, *v, len);
                            if d > len * 0.5 {
                                d - len
                            } else {
                                d
                            }
                        })
                        .collect();
                    let lo = rel.iter().cloned().fold(f64::MAX, f64::min);
                    let hi = rel.iter().cloned().fold(f64::MIN, f64::max);
                    (
                        (first + lo - 1.0).rem_euclid(len),
                        (first + hi + 1.0).rem_euclid(len),
                    )
                };
                let (fa, ta) = span(r.iter().map(|p| p.0).collect(), len_a);
                let (fb, tb) = span(r.iter().map(|p| p.1).collect(), len_b);
                zones[la as usize].push(Zone {
                    from: fa,
                    to: ta,
                    other: lb,
                    other_from: fb,
                    other_to: tb,
                    junction: 0,
                    phase: 0,
                    other_phase: 0,
                });
                zones[lb as usize].push(Zone {
                    from: fb,
                    to: tb,
                    other: la,
                    other_from: fa,
                    other_to: ta,
                    junction: 0,
                    phase: 0,
                    other_phase: 0,
                });
            }
        }
        for z in &mut zones {
            z.sort_by(|a, b| a.from.total_cmp(&b.from).then(a.other.cmp(&b.other)));
        }
        self.zones = zones;
        self.light_junctions();
        self.plan_buses();
        self.reset_at(0.0);
        self
    }

    /// Finds the kerb lane each bus stop serves, and makes a few cars of
    /// every lane with stops buses.
    fn plan_buses(&mut self) {
        let shelters: Vec<BusStop> = (self.world.props.iter())
            .filter(|p| p.kind == PropKind::BusStop)
            .map(|p| BusStop {
                pos: p.pos,
                dir: p.dir.normalize_or(DVec2::Y),
                y: p.y,
            })
            .collect();
        for stop in shelters {
            // The nearest stretch of lane on the road side of the shelter.
            let mut best: Option<(f64, usize, f64)> = None;
            let r = DVec2::splat(STOP_REACH);
            let (c0, c1) = (cell_of(stop.pos - r), cell_of(stop.pos + r));
            for x in c0.0..=c1.0 {
                for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                    for &(flies, li, s0, s1) in list {
                        if flies {
                            continue;
                        }
                        let path = &self.ground[li as usize].path;
                        let mut s = s0;
                        while s <= s1 {
                            let (p, d) = path.sample(s);
                            let off = p - stop.pos;
                            let far = off.length();
                            // In front of it, running along the kerb.
                            if far < STOP_REACH
                                && off.dot(stop.dir) > 1.0
                                && d.dot(stop.dir).abs() < 0.3
                                && best.is_none_or(|b| (far, li as usize, s) < b)
                            {
                                best = Some((far, li as usize, s));
                            }
                            s += 0.5;
                        }
                    }
                }
            }
            let Some((_, li, s)) = best else {
                continue;
            };
            // Not in a junction: a bus standing there would block it.
            let len = self.ground[li].path.len();
            let s = s.rem_euclid(len);
            let clear = BUS_LEN * 0.5 + CROSSWALK + 2.0;
            let in_junction = self.zones[li].iter().any(|z| {
                let zl = ahead(z.from, z.to, len);
                ahead(z.from, s, len) < zl + clear || ahead(s, z.from, len) < clear
            });
            if in_junction {
                continue;
            }
            let n = self.bus_stops.len() as u32;
            self.bus_stops.push(stop);
            self.ground[li].stops.push((s, n));
        }
        for l in &mut self.ground {
            if l.stops.is_empty() || l.count == 0 {
                continue;
            }
            l.stops
                .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let n = l.count as usize;
            let want = ((l.path.len() / BUS_EVERY).round() as usize).clamp(1, n.div_ceil(3));
            l.buses = vec![false; n];
            for i in 0..want {
                l.buses[i * n / want] = true;
            }
        }
    }

    /// The bus stops buses serve (see [`Self::bus_at_stop`]).
    pub fn bus_stops(&self) -> &[BusStop] {
        &self.bus_stops
    }

    /// How many buses run.
    pub fn bus_count(&self) -> usize {
        self.ground
            .iter()
            .map(|l| l.buses.iter().filter(|b| **b).count())
            .sum()
    }

    /// Street car `id` standing at a bus stop, if it is a bus doing so.
    pub fn bus_at_stop(&self, id: u64) -> Option<AtStop> {
        let (li, k) = Self::street_slot(id)?;
        let l = self.ground.get(li)?;
        let c = self.cars.get(li)?.get(k)?;
        if !l.is_bus(k) || c.parked || c.wrecked {
            return None;
        }
        let (at, stop) = l.next_stop(c.s, c.left)?;
        let standing = c.wait();
        ((at - c.s).abs() < 1.5 && c.v < 0.3 && standing < DWELL)
            .then_some(AtStop { stop, standing })
    }

    /// Groups crossings into junctions and gives each junction two-phase
    /// lights: lanes coming in along its main axis go together, the others
    /// together.
    fn light_junctions(&mut self) {
        // Every crossing as (lane, index), where it is and which way the
        // lane runs there.
        let mut all: Vec<(usize, usize, DVec2, DVec2)> = Vec::new();
        for (li, zs) in self.zones.iter().enumerate() {
            for (zi, z) in zs.iter().enumerate() {
                let (p, d) = self.ground[li].path.sample(z.from);
                all.push((li, zi, p, d));
            }
        }
        // Union crossings within a junction's reach of each other.
        let mut parent: Vec<usize> = (0..all.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut grid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        let key = |p: DVec2| {
            (
                (p.x / JUNCTION).floor() as i64,
                (p.y / JUNCTION).floor() as i64,
            )
        };
        for (i, a) in all.iter().enumerate() {
            grid.entry(key(a.2)).or_default().push(i);
        }
        for (i, a) in all.iter().enumerate() {
            let k = key(a.2);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    for &j in grid.get(&(k.0 + dx, k.1 + dz)).into_iter().flatten() {
                        if j > i && a.2.distance(all[j].2) < JUNCTION {
                            let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                            if ri != rj {
                                parent[ri.max(rj)] = ri.min(rj);
                            }
                        }
                    }
                }
            }
        }
        // Junction numbers in order of their first crossing; each one's axis
        // is the way its first lane runs.
        let mut number: BTreeMap<usize, (u32, DVec2)> = BTreeMap::new();
        let mut at: Vec<(u32, u8)> = Vec::with_capacity(all.len());
        for (i, a) in all.iter().enumerate() {
            let r = root(&mut parent, i);
            let n = number.len() as u32;
            let (j, axis) = *number.entry(r).or_insert((n, a.3));
            let phase = u8::from(a.3.dot(axis).abs() < std::f64::consts::FRAC_1_SQRT_2);
            at.push((j, phase));
        }
        let mut lookup: BTreeMap<(usize, usize), (u32, u8)> = BTreeMap::new();
        for (i, a) in all.iter().enumerate() {
            lookup.insert((a.0, a.1), at[i]);
        }
        // This lane's light at each crossing, and the other lane's (from
        // its mirror entry).
        let mut mirror: BTreeMap<(usize, u64, usize, u64), u8> = BTreeMap::new();
        for (li, zs) in self.zones.iter().enumerate() {
            for (zi, z) in zs.iter().enumerate() {
                mirror.insert(
                    (
                        li,
                        z.from.to_bits(),
                        z.other as usize,
                        z.other_from.to_bits(),
                    ),
                    lookup[&(li, zi)].1,
                );
            }
        }
        // Junctions close together run in step (one offset for a group),
        // so their walk phases come at once: whoever crosses between them
        // has every car round them waiting.
        let mut centre = vec![(DVec2::ZERO, 0.0); number.len()];
        for (i, a) in all.iter().enumerate() {
            centre[at[i].0 as usize].0 += a.2;
            centre[at[i].0 as usize].1 += 1.0;
        }
        let centre: Vec<DVec2> = centre.iter().map(|&(p, n)| p / f64::max(n, 1.0)).collect();
        let mut group: Vec<usize> = (0..centre.len()).collect();
        let mut by_cell: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for (j, c) in centre.iter().enumerate() {
            by_cell.entry(cell_of(*c)).or_default().push(j);
        }
        for (j, c) in centre.iter().enumerate() {
            let k = cell_of(*c);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    for &o in by_cell.get(&(k.0 + dx, k.1 + dz)).into_iter().flatten() {
                        if o > j && c.distance(centre[o]) < IN_STEP {
                            let (a, b) = (root(&mut group, j), root(&mut group, o));
                            if a != b {
                                group[a.max(b)] = a.min(b);
                            }
                        }
                    }
                }
            }
        }
        let mut rng = self.seed.child("lights").rng();
        let firsts: Vec<f64> = (0..number.len()).map(|_| rng.range(0.0, CYCLE)).collect();
        let offsets: Vec<f64> = (0..number.len())
            .map(|j| firsts[root(&mut group, j)])
            .collect();
        let mut sum = vec![(DVec2::ZERO, 0.0); number.len()];
        for (i, a) in all.iter().enumerate() {
            sum[at[i].0 as usize].0 += a.2;
            sum[at[i].0 as usize].1 += 1.0;
        }
        let mut axes = vec![DVec2::X; number.len()];
        for &(j, axis) in number.values() {
            axes[j as usize] = axis;
        }
        let junctions = sum
            .iter()
            .zip(axes)
            .map(|(&(p, n), axis)| (p / f64::max(n, 1.0), axis))
            .collect();
        self.signals = Arc::new(Signals::new(junctions, offsets));
        for (li, zs) in self.zones.iter_mut().enumerate() {
            for (zi, z) in zs.iter_mut().enumerate() {
                let (j, ph) = lookup[&(li, zi)];
                z.junction = j;
                z.phase = ph;
                z.other_phase = mirror
                    .get(&(
                        z.other as usize,
                        z.other_from.to_bits(),
                        li,
                        z.from.to_bits(),
                    ))
                    .copied()
                    .unwrap_or(1 - ph);
            }
        }
    }

    /// The junction a traffic light at `at` belongs to, and the phase of
    /// the traffic it faces (it looks along `facing`, at the cars coming
    /// in); none if no junction is near.
    pub fn light_of(&self, at: DVec2, facing: DVec2) -> Option<(u32, u8)> {
        self.signals.light_of(at, facing)
    }

    /// What a light of `phase` at junction `j` shows at `time`.
    pub fn signal(&self, j: u32, phase: u8, time: f64) -> Signal {
        self.signals.signal(j, phase, time)
    }

    /// The town's junctions and their lights (people on foot cross by
    /// them, see [`crate::citizens`]).
    pub fn signals(&self) -> Arc<Signals> {
        self.signals.clone()
    }

    /// Whether `phase` has green at junction `j` at `time`.
    fn green(&self, j: u32, phase: u8, time: f64) -> bool {
        self.signals.green(j, phase, time)
    }

    /// Every street car where it would be at `time` with no junctions to
    /// give way at (wrecks stay where they are).
    pub fn reset_at(&mut self, time: f64) {
        let old = std::mem::take(&mut self.cars);
        self.cars = self
            .ground
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let len = l.path.len();
                (0..l.count as usize)
                    .map(|k| {
                        let s = l.phase + l.speed * time + len * k as f64 / l.count as f64;
                        let mut c = Car {
                            s,
                            v: l.top(k),
                            waited: 0,
                            wrecked: false,
                            parked: false,
                            left: s - 4.0,
                        };
                        if let Some(w) = old.get(i).and_then(|o| o.get(k)).filter(|w| w.wrecked) {
                            c = *w;
                        }
                        c
                    })
                    .collect()
            })
            .collect();
        self.prev = self.cars.clone();
        self.time = time;
    }

    /// The time the street cars have been driven to.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Drives the street cars on to `time` (a step at a time; far behind or
    /// back in time, they are put there instead). The last step reaches
    /// `time` or just past it, so a car drawn at `time` sits between its
    /// last two steps and glides from frame to frame, never snapping from
    /// step to step.
    pub fn advance_to(&mut self, time: f64) {
        if time < self.time - 1.0 || time - self.time > CATCH_UP {
            self.reset_at(time);
            return;
        }
        while self.time < time - 1e-9 {
            self.step();
        }
    }

    /// The next car on the road after `k` (round the loop), and its
    /// distance along from `k`'s position.
    fn next_active(cars: &[Car], k: usize, len: f64) -> Option<(usize, f64)> {
        let n = cars.len();
        (1..n)
            .map(|i| (k + i) % n)
            .find(|&j| !cars[j].parked)
            .map(|j| {
                let s = cars[j].s + if j <= k { len } else { 0.0 };
                (j, s - cars[k].s)
            })
    }

    /// One step of driving.
    fn step(&mut self) {
        let old = &self.cars;
        // Where the cars are round each lane, sorted, for finding those
        // near a crossing.
        let order: Vec<Vec<(f64, usize)>> = old
            .iter()
            .enumerate()
            .map(|(i, cars)| {
                let len = self.ground[i].path.len();
                let mut o: Vec<(f64, usize)> = cars
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| !c.parked)
                    .map(|(k, c)| (c.s.rem_euclid(len), k))
                    .collect();
                o.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                o
            })
            .collect();
        let mut next: Vec<Vec<Car>> = Vec::with_capacity(old.len());
        for (li, cars) in old.iter().enumerate() {
            let l = &self.ground[li];
            let len = l.path.len();
            let n = cars.len();
            let mut out = Vec::with_capacity(n);
            for (k, c) in cars.iter().enumerate() {
                if c.wrecked {
                    out.push(Car { v: 0.0, ..*c });
                    continue;
                }
                if c.parked {
                    out.push(self.come_back(li, k));
                    continue;
                }
                let top = l.top(k);
                let half = l.half(k);
                // The car ahead (round the loop for the last one).
                let mut gap = f64::MAX;
                let mut lead_v = top;
                if let Some((j, d)) = Self::next_active(cars, k, len) {
                    gap = d - half - l.half(j);
                    lead_v = cars[j].v;
                }
                let room = gap;
                // A bus pulls up at its next stop, its middle at the
                // shelter, and stands there a while.
                let mut left = c.left;
                if l.is_bus(k) {
                    if let Some((at, _)) = l.next_stop(c.s, c.left) {
                        let here = (at - c.s).abs() < 1.5 && c.v < 0.3;
                        if here && c.wait() >= DWELL {
                            left = at + 0.5;
                        } else {
                            let stop = at - c.s + STANDSTILL;
                            if stop < gap {
                                gap = stop;
                                lead_v = 0.0;
                            }
                        }
                    }
                }
                // A crossing ahead someone else has: stop before it.
                if let Some(stop) = self.give_way(&order, li as u32, k, c, (gap, lead_v), half) {
                    if stop < gap {
                        gap = stop;
                        lead_v = 0.0;
                    }
                }
                // Someone in the road, or trouble ahead: stop short of it,
                // and never drive on into them.
                let mut room = room;
                if let Some(d) = self.hold_ahead(li, c.s) {
                    let short = d - half - HOLD_GAP;
                    if short < gap {
                        gap = short;
                        lead_v = 0.0;
                    }
                    room = room.min(short.max(0.0));
                }
                // Following the car (or stop line) ahead.
                let want = STANDSTILL
                    + (c.v * HEADWAY + c.v * (c.v - lead_v) / (2.0 * (ACCEL * BRAKE).sqrt()))
                        .max(0.0);
                let free = 1.0 - (c.v / top).powi(4);
                let a = if gap > 0.0 {
                    ACCEL * (free - (want / gap).powi(2))
                } else {
                    -BRAKE * 3.0
                };
                let v = (c.v + a.clamp(-BRAKE * 3.0, ACCEL) * STEP).clamp(0.0, top);
                // Never into the car ahead.
                let moved = ((c.v + v) * 0.5 * STEP).min((room + CAR_LEN * 0.1).max(0.0));
                let waited = if v < 0.3 { c.waited + 1 } else { 0 };
                out.push(Car {
                    s: c.s + moved,
                    v,
                    waited,
                    wrecked: false,
                    parked: waited as f64 * STEP >= PARK,
                    left,
                });
            }
            next.push(out);
        }
        self.prev = std::mem::replace(&mut self.cars, next);
        self.time += STEP;
    }

    /// How far ahead of a car at `s` on lane `li` the nearest hold is.
    fn hold_ahead(&self, li: usize, s: f64) -> Option<f64> {
        let holds = self.holds.get(li)?;
        if holds.is_empty() {
            return None;
        }
        let len = self.ground[li].path.len();
        let at = s.rem_euclid(len);
        let i = holds.partition_point(|h| *h < at);
        let next = holds.get(i).copied().unwrap_or(holds[0]);
        let d = ahead(at, next, len);
        (d <= HOLD_LOOK).then_some(d)
    }

    /// What street cars stop for from now on: heroes standing in the road
    /// (their feet; those in the air or on the pavement are passed), and
    /// trouble (where, and how far round it cars keep away). Every machine
    /// sets this from what it sees, before driving on.
    pub fn set_blockers(&mut self, standing: &[DVec3], trouble: &[(DVec2, f64)]) {
        self.holds = vec![Vec::new(); self.ground.len()];
        for p in standing {
            let q = DVec2::new(p.x, p.z);
            if (p.y - self.world.road_height(q)).abs() < 2.5 {
                self.hold_lanes(q, IN_LANE);
            }
        }
        for &(c, r) in trouble {
            self.hold_lanes(c, r);
        }
        for h in &mut self.holds {
            h.sort_by(f64::total_cmp);
            h.dedup_by(|a, b| (*a - *b).abs() < 3.0);
        }
    }

    /// Spots where something stands on the road from now on (where, and
    /// how far round it): street cars on the road there pull off it (they
    /// are not drawn, as parked cars) and come back only outside them. Pair
    /// it with trouble in [`Self::set_blockers`] so the others stop short.
    /// Every machine sets this from what it sees, before driving on.
    pub fn set_blocked(&mut self, spots: &[(DVec2, f64)]) {
        self.blocked = spots.to_vec();
        for &(c, r) in spots {
            for li in self.street_lanes_near(c, r) {
                let path = &self.ground[li].path;
                for (k, car) in self.cars[li].iter_mut().enumerate() {
                    if car.parked || car.wrecked || path.sample(car.s).0.distance(c) >= r {
                        continue;
                    }
                    car.parked = true;
                    car.waited = 0;
                    car.v = 0.0;
                    if let Some(p) = self.prev.get_mut(li).and_then(|p| p.get_mut(k)) {
                        *p = *car;
                    }
                }
            }
        }
    }

    /// The street lanes that come within `r` of `c`.
    fn street_lanes_near(&self, c: DVec2, r: f64) -> Vec<usize> {
        let (c0, c1) = (cell_of(c - DVec2::splat(r)), cell_of(c + DVec2::splat(r)));
        let mut lanes = Vec::new();
        for x in c0.0..=c1.0 {
            for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                for &(flies, lane, _, _) in list {
                    if !flies {
                        lanes.push(lane as usize);
                    }
                }
            }
        }
        lanes.sort_unstable();
        lanes.dedup();
        lanes
    }

    /// Every street lane coming within `r` of `c`: a hold where it comes in.
    fn hold_lanes(&mut self, c: DVec2, r: f64) {
        let (c0, c1) = (cell_of(c - DVec2::splat(r)), cell_of(c + DVec2::splat(r)));
        let mut found: Vec<(usize, f64)> = Vec::new();
        for x in c0.0..=c1.0 {
            for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                for &(flies, lane, s0, s1) in list {
                    if flies {
                        continue;
                    }
                    let path = &self.ground[lane as usize].path;
                    let len = path.len();
                    let inside = |s: f64| path.sample(s).0.distance(c) < r;
                    let mut was = inside(s0 - 1.0);
                    let mut s = s0;
                    while s <= s1 + 1.0 {
                        let now = inside(s);
                        if now && !was {
                            found.push((lane as usize, s.rem_euclid(len)));
                        }
                        was = now;
                        s += 1.0;
                    }
                }
            }
        }
        for (lane, s) in found {
            self.holds[lane].push(s);
        }
    }

    /// Parked car `k` on lane `li` next step: back on the road halfway
    /// along a long enough gap (if it is the first parked one after the
    /// car behind, and not in a junction), else still parked.
    fn come_back(&self, li: usize, k: usize) -> Car {
        let cars = &self.cars[li];
        let c = cars[k];
        let len = self.ground[li].path.len();
        let n = cars.len();
        let behind = (1..n).map(|i| (k + n - i) % n).find(|&j| !cars[j].parked);
        let Some(b) = behind else {
            // Everyone parked: come back where it is.
            return Car {
                parked: false,
                waited: 0,
                ..c
            };
        };
        // Only the first parked car after the one behind comes back.
        if (b + 1) % n != k {
            return c;
        }
        let gap = Self::next_active(cars, b, len).map_or(len, |(_, d)| d);
        if gap < ROOM_BACK {
            return c;
        }
        let s = cars[b].s + gap * 0.5;
        let sm = s.rem_euclid(len);
        let in_junction = self.zones[li].iter().any(|z| {
            ahead(z.from, sm, len) < ahead(z.from, z.to, len) + CAR_LEN * 2.0
                || ahead(sm, z.from, len) < CAR_LEN * 2.0
        });
        if in_junction {
            return c;
        }
        let at = self.ground[li].path.sample(sm).0;
        if self
            .blocked
            .iter()
            .any(|&(b, r)| at.distance(b) < r + CAR_LEN)
        {
            return c;
        }
        // Unwrapped next to the car behind, so the order holds.
        let s = if (b as i64) < k as i64 { s } else { s - len };
        Car {
            s,
            v: self.ground[li].top(k) * 0.6,
            waited: 0,
            wrecked: false,
            parked: false,
            left: s - 1.5,
        }
    }

    /// The crossings ahead of a car whose front is at `front` on lane
    /// `li`, nearest first, within `look` m: (distance to it, crossing).
    fn zones_ahead(
        &self,
        li: u32,
        front: f64,
        look: f64,
    ) -> impl Iterator<Item = (f64, &Zone)> + '_ {
        let len = self.ground[li as usize].path.len();
        let fm = front.rem_euclid(len);
        let zs = &self.zones[li as usize];
        let i = zs.partition_point(|z| z.from < fm);
        zs[i..]
            .iter()
            .chain(zs[..i].iter())
            .map(move |z| (ahead(fm, z.from, len), z))
            .take_while(move |(d, _)| *d <= look)
    }

    /// If car `k` on lane `li` must give way at a crossing ahead, how far
    /// it may still drive (to just before the crossing). `room`: the free
    /// road past the car's front (to the car ahead), and how fast that car
    /// goes: nobody drives into a crossing they cannot leave again.
    fn give_way(
        &self,
        order: &[Vec<(f64, usize)>],
        li: u32,
        k: usize,
        c: &Car,
        room: (f64, f64),
        half: f64,
    ) -> Option<f64> {
        let len = self.ground[li as usize].path.len();
        let front = c.s + half;
        let long = half * 2.0;
        let mine = c.wait() >= PATIENT;
        // In a junction already: drive on out of it.
        let fm = front.rem_euclid(len);
        let zs = &self.zones[li as usize];
        let i = zs.partition_point(|z| z.from <= fm);
        let inside = (1..=zs.len())
            .map(|b| &zs[(i + zs.len() - b) % zs.len()])
            .take_while(|z| ahead(z.from, fm, len) < MAX_ZONE + long)
            .any(|z| ahead(z.from, fm, len) < ahead(z.from, z.to, len) + long);
        // A red light ahead, seen from afar: stop before the crosswalk,
        // unless too close to stop. Even a car stuck for long waits while
        // people cross.
        let next = self
            .zones_ahead(li, front, RED_LOOK)
            .find(|(d, _)| !(inside && *d < CAR_LEN * 2.0));
        if let Some((d, z)) = next {
            if !inside
                && !self.green(z.junction, z.phase, self.time)
                && d > c.v * c.v / (2.0 * BRAKE) + 0.5
                && (c.wait() < STUCK || self.signals.walking(z.junction, self.time))
            {
                return Some((d - CROSSWALK).max(0.0));
            }
        }
        if c.wait() >= STUCK {
            return None;
        }
        // The junction ahead: its crossings on this lane.
        let mut cluster: Vec<(f64, &Zone)> = Vec::new();
        let mut end = 0.0;
        for (d, z) in self.zones_ahead(li, front, LOOK) {
            if inside && d < CAR_LEN * 2.0 {
                continue;
            }
            if cluster.first().is_some_and(|f| f.1.junction != z.junction) {
                break;
            }
            cluster.push((d, z));
            end = f64::max(end, d + ahead(z.from, z.to, len));
        }
        let (first, _) = *cluster.first()?;
        let stop = Some((first - CROSSWALK).max(0.0));
        // No room past it: wait before it, whatever the others do.
        if room.1 < 3.0 && room.0 + room.1 * 2.0 < end + long + 1.0 {
            return stop;
        }
        if c.wait() >= IMPATIENT {
            return None;
        }
        // Anyone in it, or coming first on green with us, at any of its
        // crossings: wait.
        for &(d, z) in &cluster {
            let olen = self.ground[z.other as usize].path.len();
            let olen_z = ahead(z.other_from, z.other_to, olen);
            let others = &self.cars[z.other as usize];
            let yields = in_stretch(
                &order[z.other as usize],
                z.other_from - LOOK - BUS_LEN,
                LOOK + olen_z + BUS_LEN * 2.0,
                olen,
            )
            .map(|j| (j, &others[j]))
            .any(|(j, o)| {
                // On a lane that crosses itself, only those ahead count
                // (never wait for the cars queueing behind).
                if o.parked || z.other == li && (j == k || ahead(c.s, o.s, len) > len * 0.5) {
                    return false;
                }
                let oh = self.ground[z.other as usize].half(j);
                let of = (o.s + oh).rem_euclid(olen);
                // In the crossing now.
                if ahead(z.other_from, of, olen) < olen_z + oh * 2.0 {
                    return true;
                }
                let od = ahead(of, z.other_from, olen);
                // Too close and too fast to stop for its red: it comes on
                // through, so wait for it.
                if z.other_phase != z.phase && o.v > 2.0 && od < o.v * o.v / (2.0 * BRAKE) + 1.5 {
                    return true;
                }
                if z.other_phase != z.phase || od > LOOK || (o.v < 0.3 && od > 3.0) {
                    return false;
                }
                // Coming: who gets there first goes first, but whoever has
                // waited long goes before those who have not.
                let theirs = (o.wait() >= PATIENT, eta(od, o.v));
                match (theirs.0, mine) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => {
                        let mt = eta(d, c.v);
                        theirs.1 < mt - 0.05
                            || ((theirs.1 - mt).abs() <= 0.05 && (z.other, j) < (li, k))
                    }
                }
            });
            if yields {
                return stop;
            }
        }
        None
    }

    /// The car behind `id` on its lane (the one that queues behind it).
    pub fn behind(&self, id: u64) -> Option<u64> {
        if id & SKY_BIT != 0 {
            return None;
        }
        let lane = (id >> 16) as u32;
        let n = self.cars.get(lane as usize)?.len() as u64;
        let k = id & 0xFFFF;
        (n > 1 && k < n).then(|| id_of(false, lane, (k + n - 1) % n))
    }

    /// Wrecks street car `id`: it stops dead. Returns where along its lane
    /// it stands (for [`Self::place_wreck`] elsewhere).
    pub fn wreck(&mut self, id: u64) -> Option<f32> {
        let (lane, k) = Self::street_slot(id)?;
        let len = self.ground.get(lane)?.path.len();
        let c = self.cars.get_mut(lane)?.get_mut(k)?;
        c.wrecked = true;
        c.parked = false;
        c.v = 0.0;
        let s = c.s;
        if let Some(p) = self.prev.get_mut(lane).and_then(|p| p.get_mut(k)) {
            p.s = s;
            p.v = 0.0;
        }
        Some(s.rem_euclid(len) as f32)
    }

    /// Wreck `id` standing at `at` along its lane (as another machine had it).
    pub fn place_wreck(&mut self, id: u64, at: f32) {
        let Some((lane, k)) = Self::street_slot(id) else {
            return;
        };
        let Some(len) = self.ground.get(lane).map(|l| l.path.len()) else {
            return;
        };
        let Some(c) = self.cars.get_mut(lane).and_then(|c| c.get_mut(k)) else {
            return;
        };
        // The same place, unwrapped near where this machine has the car.
        let mut d = ahead(c.s.rem_euclid(len), at as f64, len);
        if d > len * 0.5 {
            d -= len;
        }
        c.s += d;
        c.v = 0.0;
        c.wrecked = true;
        c.parked = false;
        let c = *c;
        if let Some(p) = self.prev.get_mut(lane).and_then(|p| p.get_mut(k)) {
            *p = c;
        }
    }

    /// Wreck `id` towed away: the car drives on.
    pub fn clear_wreck(&mut self, id: u64) {
        if let Some((lane, k)) = Self::street_slot(id) {
            if let Some(c) = self.cars.get_mut(lane).and_then(|c| c.get_mut(k)) {
                c.wrecked = false;
                c.waited = 0;
            }
        }
    }

    /// Every wreck taken away (a save is loaded).
    pub fn clear_wrecks(&mut self) {
        for c in self.cars.iter_mut().flatten() {
            c.wrecked = false;
        }
    }

    fn street_slot(id: u64) -> Option<(usize, usize)> {
        (id & SKY_BIT == 0).then_some(((id >> 16) as usize, (id & 0xFFFF) as usize))
    }

    /// The street cars as they are, lane by lane in chunks, for a friend who
    /// joins: (time, lane, first car, [(along, speed, waited)]).
    pub fn sync_out(&self) -> Vec<TrafficChunk> {
        let mut out = Vec::new();
        for (i, cars) in self.cars.iter().enumerate() {
            let len = self.ground[i].path.len();
            // Along the lane from a base every car on it is past.
            let base = cars.first().map_or(0.0, |c| (c.s / len).floor() * len);
            for (n, chunk) in cars.chunks(SYNC_CHUNK).enumerate() {
                out.push(TrafficChunk {
                    time: self.time,
                    lane: i as u32,
                    first: (n * SYNC_CHUNK) as u16,
                    cars: chunk
                        .iter()
                        .map(|c| {
                            let waited = if c.parked {
                                u32::MAX
                            } else if c.left > c.s - 1.5 {
                                c.waited.min(SERVED - 1) | SERVED
                            } else {
                                c.waited.min(SERVED - 1)
                            };
                            (c.s - base, c.v, waited)
                        })
                        .collect(),
                });
            }
        }
        out
    }

    /// Takes over street cars from [`Self::sync_out`] on another machine.
    pub fn sync_in(&mut self, chunk: &TrafficChunk) {
        if (chunk.time - self.time).abs() > 1e-6 {
            self.advance_to(chunk.time);
            self.time = chunk.time;
        }
        let Some(cars) = self.cars.get_mut(chunk.lane as usize) else {
            return;
        };
        for (i, &(s, v, wait)) in chunk.cars.iter().enumerate() {
            if let Some(c) = cars.get_mut(chunk.first as usize + i) {
                c.s = s;
                c.v = v;
                c.parked = wait == u32::MAX;
                c.waited = if c.parked { 0 } else { wait & !SERVED };
                // A bus that has served the stop it stands at: past it.
                c.left = if !c.parked && wait & SERVED != 0 {
                    s + 6.0
                } else {
                    s - 1.5
                };
            }
        }
        self.prev[chunk.lane as usize] = self.cars[chunk.lane as usize].clone();
    }

    /// Vehicles on the streets and in the air.
    pub fn count(&self) -> (u64, u64) {
        let n = |l: &[Lane]| l.iter().map(|x| x.count as u64).sum();
        (n(&self.ground), n(&self.sky))
    }

    /// Every vehicle within `radius` (on the ground plan) of `around` at
    /// `time`, nearest first, at most `max`.
    pub fn near(&self, around: DVec2, radius: f64, time: f64, max: usize) -> Vec<Vehicle> {
        let c0 = cell_of(around - DVec2::splat(radius));
        let c1 = cell_of(around + DVec2::splat(radius));
        let mut ids = Vec::new();
        // Street lanes: their stretches in the box first, then each lane's
        // cars once (a long lane crosses many cells; going through all its
        // cars for every stretch took 30 ms round a busy centre).
        let mut streets: BTreeMap<u32, Vec<(f64, f64)>> = BTreeMap::new();
        for x in c0.0..=c1.0 {
            for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                for &(flies, lane, s0, s1) in list {
                    let l = self.lane(flies, lane);
                    let len = l.path.len();
                    if !flies {
                        streets.entry(lane).or_default().push((s0 - 3.0, s1 + 3.0));
                        continue;
                    }
                    let gap = len / l.count as f64;
                    let base = (l.phase + l.speed * time).rem_euclid(len);
                    // Vehicle k sits at base + k * gap (round the loop).
                    let u0 = (s0 - base).rem_euclid(len);
                    let first = (u0 / gap).ceil() as i64;
                    let last = ((u0 + (s1 - s0)) / gap).floor() as i64;
                    for k in first..=last {
                        let k = k.rem_euclid(l.count as i64) as u64;
                        ids.push(id_of(flies, lane, k));
                    }
                }
            }
        }
        for (lane, mut spans) in streets {
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let len = self.ground[lane as usize].path.len();
            for (k, c) in self.cars[lane as usize].iter().enumerate() {
                if c.parked {
                    continue;
                }
                let a = c.s.rem_euclid(len);
                // Inside one of the stretches that start at or before it?
                let i = spans.partition_point(|sp| sp.0 <= a);
                if spans[..i].iter().rev().any(|sp| a <= sp.1) {
                    ids.push(id_of(false, lane, k as u64));
                }
            }
        }
        ids.sort();
        ids.dedup();
        let mut out: Vec<Vehicle> = ids
            .into_iter()
            .filter_map(|id| self.vehicle(id, time))
            .filter(|v| DVec2::new(v.pos.x, v.pos.z).distance(around) <= radius)
            .collect();
        out.sort_by(|a, b| {
            let d = |v: &Vehicle| DVec2::new(v.pos.x, v.pos.z).distance_squared(around);
            d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
        });
        out.truncate(max);
        out
    }

    /// The street lane nearest `at` (within `r`) going about `heading`,
    /// as a stretch of it: points every 2 m from `back` metres before
    /// the nearest point to `ahead` metres after it, with the road's
    /// height; the nearest point is at index `back / 2`. For a vehicle
    /// that joins the traffic for a while (an ambulance pulling out).
    pub fn street_stretch(
        &self,
        at: DVec2,
        heading: DVec2,
        r: f64,
        back: f64,
        ahead: f64,
    ) -> Option<Vec<DVec3>> {
        let (c0, c1) = (cell_of(at - DVec2::splat(r)), cell_of(at + DVec2::splat(r)));
        let mut best: Option<(f64, u32, f64)> = None;
        for x in c0.0..=c1.0 {
            for (_, list) in self.grid.range((x, c0.1)..=(x, c1.1)) {
                for &(flies, lane, s0, s1) in list {
                    if flies {
                        continue;
                    }
                    let path = &self.ground[lane as usize].path;
                    let mut s = s0;
                    while s <= s1 {
                        let (p, d) = path.sample(s);
                        let far = p.distance(at);
                        if far < r && d.dot(heading) > 0.8 && best.is_none_or(|b| far < b.0) {
                            best = Some((far, lane, s));
                        }
                        s += 1.0;
                    }
                }
            }
        }
        let (_, lane, s) = best?;
        let path = &self.ground[lane as usize].path;
        let n = ((back + ahead) / 2.0).ceil() as usize;
        Some(
            (0..=n)
                .map(|k| {
                    let (p, _) = path.sample(s - back + 2.0 * k as f64);
                    DVec3::new(p.x, self.world.road_height(p), p.y)
                })
                .collect(),
        )
    }

    fn lane(&self, flies: bool, i: u32) -> &Lane {
        if flies {
            &self.sky[i as usize]
        } else {
            &self.ground[i as usize]
        }
    }

    /// The street cars within `r` of any of `around` at `time`, as solid
    /// boxes (for heroes to bump into and stand on).
    pub fn car_solids(&self, around: &[DVec3], r: f64, time: f64) -> Vec<crate::collision::Solid> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for p in around {
            for v in self.near(DVec2::new(p.x, p.z), r, time, 64) {
                if v.flies() || !seen.insert(v.id) {
                    continue;
                }
                let fwd = DVec2::new(v.dir.x, v.dir.z).normalize_or(DVec2::Y);
                let (long, wide, tall) = v.kind.size();
                out.push(crate::collision::Solid {
                    origin: DVec2::new(v.pos.x, v.pos.z),
                    along: -fwd.perp(),
                    inward: fwd,
                    min: DVec2::new(-wide * 0.5, -long * 0.5),
                    max: DVec2::new(wide * 0.5, long * 0.5),
                    y0: v.pos.y,
                    y1: v.pos.y + tall,
                });
            }
        }
        out
    }

    /// Vehicle `id` at `time`; `None` if there is no such vehicle.
    pub fn vehicle(&self, id: u64, time: f64) -> Option<Vehicle> {
        let flies = id & SKY_BIT != 0;
        let lane = ((id & !SKY_BIT) >> 16) as u32;
        let k = id & 0xFFFF;
        let lanes = if flies { &self.sky } else { &self.ground };
        let l = lanes.get(lane as usize)?;
        if k >= l.count as u64 {
            return None;
        }
        let len = l.path.len();
        // Street cars: between the last two steps.
        let (s, speed) = if flies {
            (
                l.phase + l.speed * time + len * k as f64 / l.count as f64,
                l.speed,
            )
        } else {
            let (a, b) = (
                self.prev.get(lane as usize)?.get(k as usize)?,
                self.cars.get(lane as usize)?.get(k as usize)?,
            );
            if b.parked {
                return None;
            }
            // Just back on the road: no sliding from where it parked.
            let a = if a.parked { b } else { a };
            let f = ((time - (self.time - STEP)) / STEP).clamp(0.0, 1.0);
            (a.s + (b.s - a.s) * f, a.v + (b.v - a.v) * f)
        };
        let (p, d) = l.path.sample(s);
        let mut rng = self.seed.child(id).rng();
        let kind = if flies {
            if rng.chance(0.65) {
                VehicleKind::Taxi
            } else {
                VehicleKind::SkyCar(rng.range_int(0, CAR_PAINTS as i64) as u8)
            }
        } else if l.is_bus(k as usize) {
            VehicleKind::Bus(rng.range_int(0, BUS_LIVERIES as i64) as u8)
        } else {
            let model = [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover]
                [rng.weighted(&[0.45, 0.35, 0.2])];
            VehicleKind::Car(model, rng.range_int(0, CAR_PAINTS as i64) as u8)
        };
        let road = |q: DVec2| self.world.road_height(q);
        let (pos, dir, roll) = if flies {
            // Height follows the ground smoothly, banking into the turns.
            let h = |s: f64| {
                let mut sum = 0.0;
                for j in -2..=2 {
                    sum += road(l.path.sample(s + j as f64 * 15.0).0);
                }
                sum / 5.0 + l.lift
            };
            let (pa, da) = l.path.sample(s - 8.0);
            let (pb, db) = l.path.sample(s + 8.0);
            let turn = hw_core::det::atan2(da.perp_dot(db), da.dot(db));
            let roll = (turn * l.speed * 0.06).clamp(-0.45, 0.45);
            let climb = (h(s + 8.0) - h(s - 8.0)) / pa.distance(pb).max(1.0);
            let dir = DVec3::new(d.x, climb, d.y).normalize();
            let bob = hw_core::det::sin(time * 1.3 + (id % 97) as f64) * 0.25;
            (DVec3::new(p.x, h(s) + bob, p.y), dir, roll)
        } else {
            // Its axles' points on the road.
            let base = if l.is_bus(k as usize) { 3.05 } else { 1.6 };
            let (pf, _) = l.path.sample(s + base);
            let (pb, _) = l.path.sample(s - base);
            // Wheels on the drawn carriageway, not the bare road height.
            let (yf, yb) = (self.world.surface(pf), self.world.surface(pb));
            let lift = if matches!(kind, VehicleKind::Car(CarModel::Hover, _)) {
                0.25
            } else {
                0.0
            };
            let dir = DVec3::new(pf.x - pb.x, yf - yb, pf.y - pb.y).normalize_or(DVec3::Z);
            (DVec3::new(p.x, (yf + yb) * 0.5 + lift, p.y), dir, 0.0)
        };
        Some(Vehicle {
            id,
            kind,
            pos,
            dir,
            roll,
            speed,
            odometer: s,
        })
    }
}

/// Street cars per [`TrafficChunk`].
pub const SYNC_CHUNK: usize = 48;

/// Some street cars of one lane as they are, for a friend who joins: the
/// time, the lane, the first car's number and each car's (distance along,
/// speed, steps waited, or `u32::MAX` while off the road; a bus that has
/// served the stop it stands at has the top bit set).
#[derive(Clone, Debug, PartialEq)]
pub struct TrafficChunk {
    pub time: f64,
    pub lane: u32,
    pub first: u16,
    pub cars: Vec<(f64, f64, u32)>,
}

fn id_of(flies: bool, lane: u32, k: u64) -> u64 {
    (if flies { SKY_BIT } else { 0 }) | ((lane as u64) << 16) | k
}

fn cell_of(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedTown;

    fn traffic() -> Traffic {
        let t = GeneratedTown::generate(Seed(1), 20_000.0);
        Traffic::new(t.area)
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn streets_and_sky_are_busy_and_vehicles_move_smoothly() {
        let mut t = traffic();
        let (cars, fliers) = t.count();
        assert!(cars > 200, "{cars} cars");
        assert!(fliers > 20, "{fliers} fliers");
        t.advance_to(100.0);
        let p = t.world.spawn_line().0;
        let near = t.near(p, 400.0, 100.0, 1000);
        assert!(near.len() > 20, "{} near the core", near.len());
        assert!(near.iter().any(|v| v.flies()));
        let before: Vec<Vehicle> = near
            .iter()
            .map(|a| t.vehicle(a.id, 100.0).unwrap())
            .collect();
        t.advance_to(100.0 + STEP);
        for a in &before {
            let b = t.vehicle(a.id, 100.0 + STEP).unwrap();
            assert!(a.pos.is_finite());
            // A step's drive, no more (a little extra round tight corners).
            let top = t.lane(a.flies(), ((a.id & !SKY_BIT) >> 16) as u32).speed;
            let step = a.pos.distance(b.pos);
            assert!(step < top * STEP + 0.5, "{} jumped {step} m", a.id);
            if !a.flies() {
                let ground = t.world.road_height(DVec2::new(a.pos.x, a.pos.z));
                assert!((a.pos.y - ground).abs() < 1.5, "car off the road");
            }
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn near_finds_every_street_car_in_reach() {
        let mut t = traffic();
        t.advance_to(60.0);
        let p = t.world.spawn_line().0;
        let r = 500.0;
        let near: std::collections::BTreeSet<u64> = t
            .near(p, r, 60.0, usize::MAX)
            .iter()
            .filter(|v| !v.flies())
            .map(|v| v.id)
            .collect();
        // Every street car, looked at one by one.
        let mut all = std::collections::BTreeSet::new();
        for (lane, cars) in t.cars.iter().enumerate() {
            for k in 0..cars.len() {
                let id = id_of(false, lane as u32, k as u64);
                if let Some(v) = t.vehicle(id, 60.0) {
                    if DVec2::new(v.pos.x, v.pos.z).distance(p) <= r {
                        all.insert(id);
                    }
                }
            }
        }
        assert!(all.len() > 50, "{} cars", all.len());
        assert_eq!(near, all);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn cars_on_one_lane_keep_their_distance() {
        let mut t = traffic();
        for l in t.ground.iter().filter(|l| l.count > 0) {
            let gap = l.path.len() / l.count as f64;
            assert!(gap > GAP_CORE * 0.6, "cars {gap:.1} m apart");
        }
        for l in t.sky.iter().filter(|l| l.count > 0) {
            assert!(l.path.len() / l.count as f64 > SKY_GAP_CORE * 0.6);
        }
        // Driving with junctions to give way at, nobody runs into the car
        // ahead.
        for _ in 0..60 {
            t.advance_to(t.time() + 2.0);
            for (i, cars) in t.cars.iter().enumerate() {
                let l = &t.ground[i];
                let len = l.path.len();
                let on: Vec<(usize, &Car)> =
                    cars.iter().enumerate().filter(|(_, c)| !c.parked).collect();
                // Bumper to bumper, buses as long as they are.
                let room = |a: usize, b: usize| (l.half(a) + l.half(b)) * 0.85;
                for k in 0..on.len().saturating_sub(1) {
                    let (a, b) = (on[k], on[k + 1]);
                    assert!(b.1.s - a.1.s > room(a.0, b.0), "a crash");
                }
                if on.len() > 1 {
                    let (a, b) = (on[on.len() - 1], on[0]);
                    assert!(
                        b.1.s + len - a.1.s > room(a.0, b.0),
                        "a crash round the loop"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn cars_give_way_at_junctions_and_keep_moving() {
        let mut t = traffic();
        let zones: usize = t.zones.iter().map(|z| z.len()).sum();
        assert!(zones > 100, "only {zones} crossings");
        t.advance_to(30.0);
        // Two cars in one crossing from different lanes, counted every step.
        let (mut clashes, mut passes) = (0usize, 0usize);
        let mut moving = 0.0;
        let mut parked: f64 = 0.0;
        let steps = 1200;
        for _ in 0..steps {
            t.advance_to(t.time() + STEP);
            let inside = |lane: usize, from: f64, to: f64| -> Vec<usize> {
                let len = t.ground[lane].path.len();
                let zl = ahead(from, to, len);
                t.cars[lane]
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| !c.parked)
                    .filter(|(_, c)| {
                        ahead(from, (c.s + CAR_LEN * 0.5).rem_euclid(len), len) < zl + CAR_LEN
                    })
                    .map(|(k, _)| k)
                    .collect()
            };
            for (li, zs) in t.zones.iter().enumerate() {
                for z in zs.iter().filter(|z| (z.other as usize) > li) {
                    let mine = inside(li, z.from, z.to);
                    if mine.is_empty() {
                        continue;
                    }
                    passes += 1;
                    if !inside(z.other as usize, z.other_from, z.other_to).is_empty() {
                        clashes += 1;
                    }
                }
            }
            let every = t.cars.iter().flatten().count() as f64;
            let all: Vec<&Car> = t.cars.iter().flatten().filter(|c| !c.parked).collect();
            parked = parked.max(1.0 - all.len() as f64 / every);
            moving += all.iter().filter(|c| c.v > 1.0).count() as f64 / all.len() as f64;
        }
        let rate = clashes as f64 / passes.max(1) as f64;
        assert!(
            rate < 0.005,
            "{clashes} of {passes} crossings shared ({:.1}%)",
            rate * 100.0
        );
        let moving = moving / steps as f64;
        assert!(
            moving > 0.5,
            "only {:.0}% of cars moving: a jam",
            moving * 100.0
        );
        assert!(
            parked < 0.1,
            "{:.0}% of cars turned off the road",
            parked * 100.0
        );
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn traffic_light_props_show_their_junctions_cycle() {
        use hw_gen::urban::props::PropKind;
        let t = traffic();
        let lights: Vec<_> = t
            .world
            .props
            .iter()
            .filter(|p| p.kind == PropKind::TrafficLight)
            .collect();
        assert!(!lights.is_empty(), "the town has traffic lights");
        let found: Vec<(u32, u8)> = lights
            .iter()
            .filter_map(|p| t.light_of(p.pos, p.dir))
            .collect();
        assert!(
            found.len() * 10 >= lights.len() * 8,
            "most lights belong to a junction: {} of {}",
            found.len(),
            lights.len()
        );
        // Each light goes green, amber, red; crossing ways are never both
        // green or amber at once.
        for &(j, phase) in found.iter().take(40) {
            let mut seen = [false; 3];
            let mut time = 0.0;
            while time < CYCLE * 2.0 {
                let s = t.signal(j, phase, time);
                seen[s as usize] = true;
                let other = t.signal(j, 1 - phase, time);
                assert!(
                    s == Signal::Red || other == Signal::Red,
                    "junction {j} shows go both ways at {time}"
                );
                time += 0.25;
            }
            assert_eq!(seen, [true; 3], "junction {j} phase {phase}");
        }
        // Lights at one junction facing crossing roads differ in phase.
        let mut phases: BTreeMap<u32, std::collections::BTreeSet<u8>> = BTreeMap::new();
        for &(j, p) in &found {
            phases.entry(j).or_default().insert(p);
        }
        assert!(phases.values().any(|p| p.len() == 2));
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn cars_stop_for_someone_standing_in_the_road() {
        let mut t = traffic();
        t.advance_to(40.0);
        // Someone steps into a lane ahead of a moving car.
        let (li, k) = t
            .cars
            .iter()
            .enumerate()
            .find_map(|(li, cars)| {
                cars.iter()
                    .position(|c| !c.parked && c.v > 5.0)
                    .map(|k| (li, k))
            })
            .expect("a moving car");
        let len = t.ground[li].path.len();
        let ahead_s = t.cars[li][k].s + 30.0;
        let (p, _) = t.ground[li].path.sample(ahead_s);
        let feet = DVec3::new(p.x, t.world.road_height(p), p.y);
        for _ in 0..200 {
            t.set_blockers(&[feet], &[]);
            t.advance_to(t.time() + STEP);
        }
        let c = t.cars[li][k];
        let gap = ahead(c.s.rem_euclid(len), ahead_s.rem_euclid(len), len);
        assert!(c.v < 0.3, "still driving at {:.1} m/s", c.v);
        assert!(
            gap > CAR_LEN * 0.5 && gap < 15.0,
            "stopped {gap:.1} m short of them"
        );
        // They step out of the way: it drives on (once the lights ahead,
        // if any, let it: within a light cycle and a half).
        let mut moved = false;
        for _ in 0..(CYCLE * 1.5 / STEP) as usize {
            t.set_blockers(&[], &[]);
            t.advance_to(t.time() + STEP);
            if t.cars[li][k].v > 2.0 {
                moved = true;
                break;
            }
        }
        assert!(moved, "the car drives on");
        // Trouble ahead: nobody drives into it.
        let (c, _) = t.ground[li].path.sample(t.cars[li][k].s + 40.0);
        for _ in 0..200 {
            t.set_blockers(&[], &[(c, 20.0)]);
            t.advance_to(t.time() + STEP);
        }
        for (li, cars) in t.cars.iter().enumerate() {
            for (k, car) in cars.iter().enumerate() {
                if car.parked || car.v < 1.0 {
                    continue;
                }
                let v = t
                    .vehicle(id_of(false, li as u32, k as u64), t.time())
                    .unwrap();
                let d = DVec2::new(v.pos.x, v.pos.z).distance(c);
                // Those already inside drive out; none drive in.
                if d < 12.0 {
                    let next = t.ground[li].path.sample(car.s + 2.0).0.distance(c);
                    assert!(next > d - 0.5, "a car drives into the trouble");
                }
            }
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn no_car_stands_where_the_road_is_blocked() {
        let mut t = traffic();
        t.advance_to(40.0);
        // A crash right where a moving car is.
        let (li, k) = t
            .cars
            .iter()
            .enumerate()
            .find_map(|(li, cars)| {
                cars.iter()
                    .position(|c| !c.parked && c.v > 5.0)
                    .map(|k| (li, k))
            })
            .expect("a moving car");
        let c = t.ground[li].path.sample(t.cars[li][k].s).0;
        let on_road_within = |t: &Traffic, r: f64| {
            t.cars.iter().enumerate().any(|(li, cars)| {
                cars.iter().enumerate().any(|(k, _)| {
                    t.vehicle(id_of(false, li as u32, k as u64), t.time())
                        .is_some_and(|v| DVec2::new(v.pos.x, v.pos.z).distance(c) < r)
                })
            })
        };
        assert!(on_road_within(&t, 6.0));
        // A minute and a half of it (parked cars come back after one).
        for _ in 0..(90.0 / STEP) as usize {
            t.set_blockers(&[], &[(c, 9.0)]);
            t.set_blocked(&[(c, 6.0)]);
            t.advance_to(t.time() + STEP);
            assert!(
                !on_road_within(&t, 5.5),
                "a car on the wrecks at {:.1} s",
                t.time()
            );
        }
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn buses_call_at_their_stops_and_drive_on() {
        let mut t = traffic();
        let shelters = (t.world.props.iter())
            .filter(|p| p.kind == PropKind::BusStop)
            .count();
        assert!(shelters > 0, "the town has bus stops");
        assert!(
            t.bus_stops().len() * 10 >= shelters * 8,
            "{} of {shelters} stops served",
            t.bus_stops().len()
        );
        let buses: Vec<u64> = (t.ground.iter().enumerate())
            .flat_map(|(li, l)| {
                (0..l.buses.len())
                    .filter(|&k| l.is_bus(k))
                    .map(move |k| id_of(false, li as u32, k as u64))
            })
            .collect();
        assert!(!buses.is_empty(), "no buses");
        assert_eq!(buses.len(), t.bus_count());
        // Every stop is on the kerb side of its bus: the shelter to the
        // bus's right, a few metres from its middle.
        let mut calls: BTreeMap<u64, std::collections::BTreeSet<u32>> = BTreeMap::new();
        let mut left_again = 0;
        let mut longest: f64 = 0.0;
        let steps = (480.0 / STEP) as usize;
        for _ in 0..steps {
            t.advance_to(t.time() + STEP);
            for &id in &buses {
                let Some(at) = t.bus_at_stop(id) else {
                    continue;
                };
                // Its last moment there: it leaves next step.
                if at.standing + STEP >= DWELL {
                    left_again += 1;
                }
                longest = longest.max(at.standing);
                if at.standing > 3.0 {
                    let v = t.vehicle(id, t.time()).expect("the bus");
                    let stop = t.bus_stops()[at.stop as usize];
                    let mid = DVec2::new(v.pos.x, v.pos.z);
                    let fwd = DVec2::new(v.dir.x, v.dir.z).normalize();
                    let to = stop.pos - mid;
                    assert!(
                        to.length() < STOP_REACH,
                        "{:.1} m from its stop",
                        to.length()
                    );
                    // Its right is -x, the lanes' perp.
                    assert!(to.dot(fwd.perp()) > 1.0, "the stop is on the far side");
                    assert!(
                        to.dot(fwd).abs() < 2.0,
                        "the middle door misses the shelter"
                    );
                    calls.entry(id).or_default().insert(at.stop);
                }
            }
        }
        assert!(
            longest < DWELL + 0.5,
            "a bus stood {longest:.1} s at a stop"
        );
        // With one stop in this square a bus may pass it only now and
        // then: some bus calls, and none stays.
        assert!(!calls.is_empty(), "no bus called at a stop");
        assert!(left_again > 0, "no bus left a stop");
    }

    #[test]
    fn same_seed_same_traffic() {
        let (mut a, mut b) = (traffic(), traffic());
        a.advance_to(7.0);
        b.advance_to(7.0);
        let p = a.world.spawn_line().0;
        assert_eq!(a.near(p, 300.0, 7.0, 400), b.near(p, 300.0, 7.0, 400));
        // A friend who joins later is sent the cars and drives on alike.
        let mut c = traffic();
        for chunk in a.sync_out() {
            c.sync_in(&chunk);
        }
        a.advance_to(20.0);
        c.advance_to(20.0);
        let (na, nc) = (a.near(p, 300.0, 20.0, 400), c.near(p, 300.0, 20.0, 400));
        assert_eq!(na.len(), nc.len());
        for (x, y) in na.iter().zip(&nc) {
            assert!(x.pos.distance(y.pos) < 0.05, "{} drifted", x.id);
        }
    }
}
