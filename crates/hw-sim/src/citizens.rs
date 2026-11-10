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

use crate::traffic::{Signals, CYCLE};
use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::building::{BuildingUse, RoomKind};
use hw_gen::geom::Polyline;
use hw_gen::urban::area::Area;
use hw_gen::urban::deep::Folk;
use hw_gen::urban::underground::{molehaven_layout, Underground};
use hw_gen::urban::world::{WorldRoad, WALKER_HALF};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How many different looks citizens are drawn with (the client builds
/// this many characters; a citizen's look is one of them).
pub const LOOKS: u32 = 24;

/// People out on foot come from the buildings along a street: each one
/// sends this many to its stretch of pavement at its busiest hour, by what
/// it is for (and a little more for each floor). Shops draw the most,
/// houses the fewest; a quiet residential street has a handful of people,
/// a shopping street one every seven metres or so.
fn draws(usage: BuildingUse, floors: u32) -> f64 {
    let f = floors as f64;
    match usage {
        BuildingUse::ShopsAndOffices => 0.9 + 0.12 * f,
        BuildingUse::Offices => 0.12 + 0.03 * f,
        BuildingUse::Apartments => 0.06 + 0.018 * f,
        BuildingUse::House => 0.02,
        BuildingUse::Works => 0.03,
        BuildingUse::Pavilion => 0.45,
        BuildingUse::Lab => 0.01,
        BuildingUse::Tenement => 0.05 + 0.01 * f,
    }
}
/// Passers-by per metre on top (main roads see more of them).
const THROUGH: f64 = 0.004;
const THROUGH_MAIN: f64 = 0.008;
/// The share of the people out on foot that are robot street sweepers.
const SWEEPERS: f64 = 0.03;
/// Never more than this many per metre of street (both sides).
const MOST: f64 = 0.15;
/// A building's front is on the street whose pavement is this near.
const FRONTAGE: f64 = 14.0;
/// The hour the town is seen at, unless told (see [`Citizens::set_hour`]).
const DEFAULT_HOUR: f64 = 10.5;
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

/// A run from trouble: out of the road onto the pavement and along it
/// (see [`Citizens::escape`]).
#[derive(Clone, Debug)]
pub struct Escape {
    legs: Vec<Leg>,
}

impl Escape {
    /// Its length in metres.
    pub fn len(&self) -> f64 {
        self.legs.iter().map(Leg::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() < 1e-6
    }
}

/// One leg of a route: along a street's pavement from one distance to
/// another (either way), or straight across from one pavement to the next
/// (waiting for a junction's walk light first, if it crosses traffic).
#[derive(Clone, Copy, Debug)]
enum Leg {
    Street {
        road: u32,
        side: f64,
        from: f64,
        to: f64,
    },
    Cross(DVec2, DVec2, Option<u32>),
}

impl Leg {
    fn len(&self) -> f64 {
        match *self {
            Leg::Street { from, to, .. } => (to - from).abs(),
            Leg::Cross(a, b, _) => a.distance(b),
        }
    }

    /// The same leg walked the other way.
    fn back(self) -> Leg {
        match self {
            Leg::Street {
                road,
                side,
                from,
                to,
            } => Leg::Street {
                road,
                side,
                from: to,
                to: from,
            },
            Leg::Cross(a, b, w) => Leg::Cross(b, a, w),
        }
    }
}

/// Whether a way over a carriageway may be taken.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Wait {
    /// No traffic on it: straight over.
    Free,
    /// Over in the walk phase of this junction's lights.
    Light(u32),
    /// Traffic and no lights: not here.
    Never,
}

/// Where a side street meets a street: its mouth on one pavement, which
/// people walking along cross (at the junction's walk light, if it has
/// traffic).
#[derive(Clone, Copy, Debug)]
struct Mouth {
    s0: f64,
    s1: f64,
    light: Option<u32>,
    /// Not to be crossed: too long, or traffic and no lights. Walking
    /// along stops before it.
    wall: bool,
}

/// The longest way over a carriageway anyone takes (m): one that fits in a
/// walk phase at a brisk pace.
const LONGEST_CROSSING: f64 = 20.0;

/// Turning at a street's end onto another: from each side of this
/// street, where on the other one walking goes on, on which pavement,
/// and whether it may cross to it (the near pavement, round the corner)
/// or over to the far one.
#[derive(Clone, Copy, Debug)]
struct Turn {
    road: u32,
    /// By side (-1, then 1): distance along the next street, its near side,
    /// and how the near and the far pavements are reached.
    sides: [(f64, f64, Wait, Wait); 2],
}

fn side_index(side: f64) -> usize {
    usize::from(side > 0.0)
}

/// A street's pavement centre line, measured.
struct Walk {
    line: Polyline,
    /// Pavement offset from the centre line.
    offset: f64,
    /// At each point, the way to its pavement and how far per metre of
    /// offset (the mitre).
    miters: Vec<DVec2>,
    /// Where the walking line steps round something standing on the
    /// pavement (a kiosk, a bench, a tree), by side (-1, then 1), in order.
    bumps: [Vec<Bump>; 2],
    /// Walkers living on this street at its busiest hour.
    people: u32,
    /// How much of its draw is shops and offices (0 to 1): they are busy
    /// by day, homes in the morning and evening.
    commerce: f64,
}

impl Walk {
    fn len(&self) -> f64 {
        self.line.len()
    }

    /// Point and direction at distance `s`, `side` = -1 left or 1 right.
    /// The pavement keeps its distance from the centre line round bends
    /// too (its corners are mitred), so it never cuts over the road on
    /// the inside of a bend.
    fn sample(&self, s: f64, side: f64) -> (DVec2, DVec2) {
        let (k, t) = self.line.locate(s);
        let (a, b) = (self.line.pts[k], self.line.pts[k + 1]);
        let dir = (b - a).normalize_or_zero();
        let o = (self.offset + self.bump_at(s, side)) * side;
        let (ma, mb) = (self.miters[k], self.miters[k + 1]);
        ((a + ma * o).lerp(b + mb * o, t), dir)
    }

    /// Like [`Self::sample`]'s point, without stepping round anything.
    fn sample_unbumped(&self, s: f64, side: f64) -> DVec2 {
        let (k, t) = self.line.locate(s);
        let (a, b) = (self.line.pts[k], self.line.pts[k + 1]);
        let o = self.offset * side;
        let (ma, mb) = (self.miters[k], self.miters[k + 1]);
        (a + ma * o).lerp(b + mb * o, t)
    }

    /// Distance along the line of the point nearest `p` (and how far off).
    fn nearest(&self, p: DVec2) -> (f64, f64) {
        self.line.nearest(p)
    }

    /// How far the walking line is moved out (+) or in (-) at `s`: the
    /// biggest step round anything there, eased in and out.
    fn bump_at(&self, s: f64, side: f64) -> f64 {
        let mut out: f64 = 0.0;
        for b in &self.bumps[side_index(side)] {
            if b.s0 - EASE > s {
                break;
            }
            let t = ((s - (b.s0 - EASE)) / EASE)
                .min(((b.s1 + EASE) - s) / EASE)
                .clamp(0.0, 1.0);
            let d = b.by * t * t * (3.0 - 2.0 * t);
            if d.abs() > out.abs() {
                out = d;
            }
        }
        out
    }
}

/// A stretch of pavement where walking steps aside, `by` metres from the
/// usual line (+ towards the houses).
#[derive(Clone, Copy, Debug)]
struct Bump {
    s0: f64,
    s1: f64,
    by: f64,
}

/// Over how many metres walking eases out round something and back.
const EASE: f64 = 1.5;

/// The citizens of one town.
pub struct Citizens {
    seed: Seed,
    walks: Vec<Walk>,
    /// Streets met at each street end, as turns onto them.
    joins: Vec<[Vec<Turn>; 2]>,
    /// How far before each end a street's pavement stops (the corner
    /// with the street it meets), by side (-1, then 1).
    ends: Vec<Ends>,
    /// Side streets' mouths along each street, by side (-1, then 1), in
    /// order.
    mouths: Vec<[Vec<Mouth>; 2]>,
    /// The traffic lights people cross by.
    signals: Arc<Signals>,
    /// Streets by grid cell (each street in every cell it passes).
    grid: BTreeMap<(i64, i64), Vec<u32>>,
    world: Arc<Area>,
    /// Molehaven's people: its floor, its middle, and each one's way.
    moles: (f64, DVec2, Vec<MoleWay>),
    /// The hour of day (0 to 24): who is out.
    hour: f64,
}

impl Citizens {
    /// Plans where the town's people walk (streets, joins, how many per
    /// street), crossing by the town's traffic lights (see
    /// [`crate::traffic::Traffic::signals`]). Takes a few milliseconds.
    pub fn new(world: Arc<Area>, signals: Arc<Signals>) -> Self {
        let seed = world.seed.child("citizens");
        let mut walks = Vec::with_capacity(world.roads.len());
        for r in world.roads.iter() {
            walks.push(walk_of(r));
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
        step_round_props(&mut walks, &grid, &world);
        let met = streets_met(&walks, &grid, &world);
        let ways = Carriageways::new(&world);
        let (ends, mut mouths) = pavement_runs(&walks, &ways, &world, &signals);
        stop_at_barricades(&walks, &grid, &world, &mut mouths);
        let joins = turns(&walks, &met, &ends, &ways, &world, &signals);
        people_from_buildings(&mut walks, &grid, &world, seed);
        let moles = plan_moles(world.underground());
        Self {
            seed,
            walks,
            joins,
            grid,
            ends,
            mouths,
            signals,
            world,
            moles,
            hour: DEFAULT_HOUR,
        }
    }

    /// How many citizens the town has out on its streets at this hour.
    pub fn count(&self) -> u64 {
        let out: f64 = self
            .walks
            .iter()
            .map(|w| w.people as f64 * self.share_out(w))
            .sum();
        out.round() as u64 + self.moles.2.len() as u64
    }

    /// The hour of day (0 to 24) the town is seen at: by night the
    /// streets are nearly empty, the shopping streets fill by day and the
    /// homes' streets in the morning and evening.
    pub fn set_hour(&mut self, hour: f64) {
        self.hour = hour.rem_euclid(24.0);
    }

    /// The share of a street's people out at this hour.
    fn share_out(&self, w: &Walk) -> f64 {
        let (shops, homes) = busy_at(self.hour);
        w.commerce * shops + (1.0 - w.commerce) * homes
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
        // Molehaven's people, when the cavern is near.
        let (_, centre, ways) = &self.moles;
        if !ways.is_empty() && centre.distance(around) < radius + 60.0 {
            for n in 0..ways.len() as u64 {
                if let Some(c) = self.citizen((MOLE_HOME << 20) | n, time) {
                    if c.pos.truncate_xz().distance(around) <= radius {
                        out.push(c);
                    }
                }
            }
        }
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

    /// Is citizen `id` one of the town's robot street sweepers? They keep
    /// the citizens' routes and hours (and stand where they would pause),
    /// and are drawn as robots instead of people.
    pub fn is_sweeper(&self, id: u64) -> bool {
        id >> 20 != MOLE_HOME && self.seed.child("sweeper").child(id).unit() < SWEEPERS
    }

    /// Citizen `id` at `time`; `None` if there is no such citizen.
    pub fn citizen(&self, id: u64, time: f64) -> Option<Citizen> {
        // People indoors and along the tube are asked for by their place
        // (`indoors`, `tube_folk`).
        if id & (INDOOR | TUBE_FOLK) != 0 {
            return None;
        }
        if id >> 20 == MOLE_HOME {
            return self.mole(id, time);
        }
        let road = (id >> 20) as usize;
        let n = (id & 0xF_FFFF) as u32;
        let home = self.walks.get(road)?;
        if n >= home.people {
            return None;
        }
        let s = self.seed.child(id);
        // Indoors at this hour.
        if s.child("out").unit() >= self.share_out(home) {
            return None;
        }
        let mut rng = s.rng();
        let look = (s.child("look").0 % LOOKS as u64) as u32;
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let start = self.place_on(road as u32, side, rng.range(0.0, home.len()));
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
        // people are spread over their routes. Time for the usual waits
        // at kerbs is planned in; someone held up longer sets off back
        // later, or out on the next round later.
        let half = length / speed + pause + self.kerbs(&legs) as f64 * CYCLE * 0.5;
        let period = 2.0 * half;
        let off = rng.range(0.0, period);
        let t0 = ((time + off) / period).floor() * period - off;
        let round = |t0: f64, begun: f64| {
            let out = self.walk(&legs, false, begun, speed, f64::INFINITY);
            let turn = (t0 + half).max(out.err().unwrap_or(begun) + pause * 0.5);
            let home = self.walk(&legs, true, turn, speed, f64::INFINITY);
            (turn, home.err().unwrap_or(turn))
        };
        // The round before, as planned, may not be home yet.
        let (turn_before, home_before) = round(t0 - period, t0 - period);
        let begun = t0.max(home_before + 1.0);
        let (back, from) = if time < begun {
            (true, turn_before)
        } else {
            let (turn, _) = round(t0, begun);
            if time < turn {
                (false, begun)
            } else {
                (true, turn)
            }
        };
        if let Ok((p, dir, v)) = self.walk(&legs, back, from, speed, time) {
            return Some(self.at(id, p, dir, v, look));
        }
        // Arrived: turned towards the shops while waiting.
        let (p, dir) = if back {
            let (p, d) = self.leg_at(legs[0], 0.0);
            (p, -d)
        } else {
            let last = legs[legs.len() - 1];
            self.leg_at(last, last.len())
        };
        let dir = dir.perp() * if back { -side } else { side };
        Some(self.at(id, p, dir, 0.0, look))
    }

    /// One of Molehaven's people at `time`.
    fn mole(&self, id: u64, time: f64) -> Option<Citizen> {
        let (floor, _, ways) = &self.moles;
        let way = *ways.get((id & 0xF_FFFF) as usize)?;
        let s = self.seed.child("mole").child(id);
        let mut rng = s.rng();
        let look = LOOKS + (s.child("look").0 % MOLE_LOOKS as u64) as u32;
        let (p, facing, speed) = match way {
            MoleWay::Stand(p, d) => (p, d, 0.0),
            MoleWay::Errand(a, b) => {
                let speed = rng.range(0.8, 1.1);
                let pause = rng.range(6.0, 20.0);
                let period = 2.0 * (a.distance(b).max(0.1) / speed + pause);
                errand(a, b, speed, pause, time + rng.range(0.0, period))
            }
            MoleWay::Stroll { centre, r, sign } => {
                // Evenly spread on their ring, all at one pace.
                let k = (id & 0xF_FFFF) as f64;
                let speed = 0.9;
                let a = k * 2.399 + sign * time * speed / r;
                let (sa, ca) = (hw_core::det::sin(a), hw_core::det::cos(a));
                (
                    centre + DVec2::new(ca, sa) * r,
                    DVec2::new(-sa, ca) * sign,
                    speed,
                )
            }
        };
        Some(Citizen {
            id,
            pos: DVec3::new(p.x, floor + 0.02, p.y),
            facing: facing.normalize_or(DVec2::Y),
            speed,
            look,
        })
    }

    /// The way someone who gets out of a car at `from` runs from trouble at
    /// `danger`: straight over to the nearest pavement (where the way there
    /// is clear of parked cars and street furniture), then along it away
    /// from the trouble, at most `max` metres and never past the corner or
    /// a side street's mouth. `None` far from any street.
    pub fn escape(&self, from: DVec2, danger: DVec2, max: f64) -> Option<Escape> {
        let c = cell_of(from);
        let mut roads: Vec<u32> = (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dz| (c.0 + dx, c.1 + dz)))
            .filter_map(|k| self.grid.get(&k))
            .flatten()
            .copied()
            .collect();
        roads.sort_unstable();
        roads.dedup();
        let (road, s, _) = roads
            .into_iter()
            .map(|r| {
                let (s, d) = self.walks[r as usize].nearest(from);
                (r, s, d)
            })
            .filter(|x| x.2 < 40.0)
            .min_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)))?;
        let w = &self.walks[road as usize];
        let side = [-1.0, 1.0]
            .into_iter()
            .min_by(|a, b| {
                let da = w.sample(s, *a).0.distance(from);
                let db = w.sample(s, *b).0.distance(from);
                da.total_cmp(&db)
            })
            .unwrap_or(1.0);
        let (_, d) = w.sample(s, side);
        let ahead = if (from - danger).dot(d) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        // Over to the pavement where nothing stands in the way: straight
        // across, else a few metres on (or back).
        let near: Vec<_> = (self.world.props.iter())
            .filter(|p| p.pos.distance(from) < 30.0)
            .flat_map(|p| p.footprints())
            .collect();
        let clear = |a: DVec2, b: DVec2| {
            let n = (a.distance(b) / 0.25).ceil().max(1.0) as usize;
            (0..=n).all(|i| {
                let q = a.lerp(b, i as f64 / n as f64);
                near.iter().all(|f| f.distance(q) > WALKER_HALF)
            })
        };
        let start = [0.0, 2.0, -2.0, 4.0, -4.0, 6.0, -6.0, 9.0, -9.0, 12.0]
            .into_iter()
            .map(|k| self.place_on(road, side, s + k * ahead))
            .find(|&x| clear(from, w.sample(x, side).0))
            .unwrap_or_else(|| self.place_on(road, side, s));
        // Along the pavement as far as this stretch of it goes.
        let [e0, e1] = self.ends[road as usize][side_index(side)];
        let mouths = &self.mouths[road as usize][side_index(side)];
        let to = if ahead > 0.0 {
            let stop = (mouths.iter())
                .map(|m| m.s0)
                .filter(|&m| m > start)
                .fold(w.len() - e1, f64::min);
            (start + max).min(stop).max(start)
        } else {
            let stop = (mouths.iter())
                .map(|m| m.s1)
                .filter(|&m| m < start)
                .fold(e0, f64::max);
            (start - max).max(stop).min(start)
        };
        let over = w.sample(start, side).0;
        Some(Escape {
            legs: vec![
                Leg::Cross(from, over, None),
                Leg::Street {
                    road,
                    side,
                    from: start,
                    to,
                },
            ],
        })
    }

    /// Where an escape (see [`Citizens::escape`]) is `x` metres along it:
    /// feet on the ground and the way it runs. Past its end, at the end.
    pub fn escape_at(&self, e: &Escape, x: f64) -> (DVec3, DVec2) {
        let mut x = x.max(0.0);
        let last = e.legs.len() - 1;
        for (k, leg) in e.legs.iter().enumerate() {
            if x <= leg.len() || k == last {
                let (p, d) = self.leg_at(*leg, x.min(leg.len()));
                return (DVec3::new(p.x, self.world.surface(p), p.y), d);
            }
            x -= leg.len();
        }
        unreachable!("an escape has legs")
    }

    fn at(&self, id: u64, p: DVec2, facing: DVec2, speed: f64, look: u32) -> Citizen {
        Citizen {
            id,
            pos: DVec3::new(p.x, self.world.surface(p), p.y),
            facing: facing.normalize_or(DVec2::Y),
            speed,
            look,
        }
    }

    /// `s` along street `road` on `side`, moved onto its pavement: off
    /// the corners and out of side streets' mouths.
    fn place_on(&self, road: u32, side: f64, s: f64) -> f64 {
        let w = &self.walks[road as usize];
        let [e0, e1] = self.ends[road as usize][side_index(side)];
        let s = s.clamp(e0.min(w.len() * 0.5), (w.len() - e1).max(w.len() * 0.5));
        match self.mouths[road as usize][side_index(side)]
            .iter()
            .find(|m| m.s0 < s && s < m.s1)
        {
            Some(m) if s - m.s0 < m.s1 - s => m.s0,
            Some(m) => m.s1,
            None => s,
        }
    }

    /// From home along the street, then through up to three more streets
    /// met at the ends (a walk to work, the shops, a friend's), round the
    /// corners and over at the crossings.
    fn route(&self, road: u32, start: f64, side: f64, rng: &mut hw_core::seed::Rng) -> Vec<Leg> {
        let mut legs = Vec::new();
        let mut road = road;
        let mut from = start;
        let mut side = side;
        let hops = rng.range_int(1, 4);
        for _ in 0..hops {
            let w = &self.walks[road as usize];
            let end = if rng.chance(0.5) { 0 } else { 1 };
            let [e0, e1] = self.ends[road as usize][side_index(side)];
            let to = if end == 0 {
                e0.min(from)
            } else {
                (w.len() - e1).max(from)
            };
            // A stretch that cannot be crossed: walk up to it and back.
            let wall = self.mouths[road as usize][side_index(side)]
                .iter()
                .filter(|m| m.wall)
                .filter_map(|m| {
                    if to > from && m.s1 > from && m.s0 < to {
                        Some(m.s0.max(from))
                    } else if to < from && m.s0 < from && m.s1 > to {
                        Some(m.s1.min(from))
                    } else {
                        None
                    }
                })
                .min_by(|a, b| (a - from).abs().total_cmp(&(b - from).abs()));
            legs.push(Leg::Street {
                road,
                side,
                from,
                to: wall.unwrap_or(to),
            });
            if wall.is_some() {
                break;
            }
            let next = &self.joins[road as usize][end];
            if next.is_empty() {
                break;
            }
            let turn = *rng.pick(next);
            // Round the corner onto the near pavement of the next street;
            // now and then over to the far one (by the lights, where there
            // is traffic; never where there is traffic and no lights).
            let far = rng.chance(0.2);
            let (sb, near, to_near, to_far) = turn.sides[side_index(side)];
            let (next_side, wait) = match (far, to_near, to_far) {
                (true, _, f) if f != Wait::Never => (-near, f),
                (_, n, _) if n != Wait::Never => (near, n),
                (_, _, f) if f != Wait::Never => (-near, f),
                _ => break,
            };
            let light = match wait {
                Wait::Light(j) => Some(j),
                _ => None,
            };
            let a = w.sample(to, side).0;
            let b = self.walks[turn.road as usize].sample(sb, next_side).0;
            legs.push(Leg::Cross(a, b, light));
            road = turn.road;
            from = sb;
            side = next_side;
        }
        legs
    }

    /// Point and heading `x` metres into a leg.
    fn leg_at(&self, leg: Leg, x: f64) -> (DVec2, DVec2) {
        match leg {
            Leg::Street {
                road,
                side,
                from,
                to,
            } => {
                let w = &self.walks[road as usize];
                let forward = to >= from;
                let s = if forward { from + x } else { from - x };
                let (p, d) = w.sample(s, side);
                (p, if forward { d } else { -d })
            }
            Leg::Cross(a, b, _) => {
                let len = a.distance(b);
                let t = (x / len.max(1e-9)).clamp(0.0, 1.0);
                (a.lerp(b, t), (b - a).normalize_or(DVec2::Y))
            }
        }
    }

    /// The pieces of a leg in walking order: (from, to) metres into it,
    /// and the lights to wait for before them (side streets' mouths and
    /// crossings with traffic).
    fn pieces(&self, leg: Leg, out: &mut Vec<(f64, f64, Option<u32>)>) {
        out.clear();
        let len = leg.len();
        let Leg::Street {
            road,
            side,
            from,
            to,
        } = leg
        else {
            if let Leg::Cross(_, _, light) = leg {
                out.push((0.0, len, light));
            }
            return;
        };
        let forward = to >= from;
        let mut at = 0.0;
        let mouths = &self.mouths[road as usize][side_index(side)];
        // Every mouth stepped into on the way, also one the leg ends in
        // (turning into that side street on its far side): its kerb is
        // where the wait is.
        let mut found: Vec<(f64, f64, Option<u32>)> = mouths
            .iter()
            .filter(|m| {
                if forward {
                    m.s0 >= from - 1e-6 && m.s0 < to - 1e-6
                } else {
                    m.s1 <= from + 1e-6 && m.s1 > to + 1e-6
                }
            })
            .map(|m| {
                if forward {
                    (m.s0 - from, m.s1.min(to) - from, m.light)
                } else {
                    (from - m.s1, from - m.s0.max(to), m.light)
                }
            })
            .collect();
        found.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (a, b, light) in found {
            if a < at {
                continue;
            }
            out.push((at, a, None));
            out.push((a, b, light));
            at = b;
        }
        out.push((at, len, None));
    }

    /// How many waits for lights a route has (one way).
    fn kerbs(&self, legs: &[Leg]) -> usize {
        let mut pieces = Vec::new();
        legs.iter()
            .map(|&l| {
                self.pieces(l, &mut pieces);
                pieces.iter().filter(|p| p.2.is_some()).count()
            })
            .sum()
    }

    /// Where someone is at `now` who set off along `legs` (`back`: from
    /// the far end) at `begun`, walking at `speed`, waiting at kerbs for
    /// the walk lights and crossing briskly: (where, heading, speed), or
    /// once arrived, when that was.
    fn walk(
        &self,
        legs: &[Leg],
        back: bool,
        begun: f64,
        speed: f64,
        now: f64,
    ) -> Result<(DVec2, DVec2, f64), f64> {
        let mut t = begun;
        let mut pieces = Vec::new();
        let n = legs.len();
        for i in 0..n {
            let leg = if back {
                legs[n - 1 - i].back()
            } else {
                legs[i]
            };
            self.pieces(leg, &mut pieces);
            for &(a, b, light) in &pieces {
                let len = b - a;
                let (start, v) = match light {
                    Some(j) => {
                        let (start, by) = self.signals.crossing(j, t, len);
                        (start, (len / (by - start).max(0.1)).clamp(speed, 3.0))
                    }
                    None => (t, speed),
                };
                if now < start {
                    // At the kerb, waiting for the walk light.
                    let (p, d) = self.leg_at(leg, a);
                    return Ok((p, d, 0.0));
                }
                let end = start + len / v;
                if now < end {
                    let (p, d) = self.leg_at(leg, a + (now - start) * v);
                    return Ok((p, d, v));
                }
                t = end;
            }
        }
        Err(t)
    }
}

fn cell_of(p: DVec2) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

/// Streets meeting each street end: (end 0 or 1) -> (street, distance
/// along it).
type Met = Vec<[Vec<(u32, f64)>; 2]>;

fn end_point(w: &Walk, e: usize) -> DVec2 {
    if e == 0 {
        w.line.pts[0]
    } else {
        *w.line.pts.last().expect("walk has points")
    }
}

fn streets_met(walks: &[Walk], grid: &BTreeMap<(i64, i64), Vec<u32>>, world: &Area) -> Met {
    let mut met = Vec::with_capacity(walks.len());
    for (i, w) in walks.iter().enumerate() {
        let mut ends: [Vec<(u32, f64)>; 2] = [Vec::new(), Vec::new()];
        for (e, list) in ends.iter_mut().enumerate() {
            let p = end_point(w, e);
            let c = cell_of(p);
            let mut near = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(l) = grid.get(&(c.0 + dx, c.1 + dy)) {
                        near.extend(l.iter().copied());
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
                    list.push((j, s));
                }
            }
        }
        met.push(ends);
    }
    met
}

/// Whether a street carries cars (as [`crate::traffic`] plans them).
/// Distance from `p` to the segment `a`-`b`.
fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// A piece of a street's centre line: (street, from, to).
type Segment = (u32, DVec2, DVec2);

/// How far before each end a street's pavements stop: by side (-1, then
/// 1), at end 0 and end 1.
type Ends = [[f64; 2]; 2];

/// Every street's carriageway, for asking whether a point is on one.
struct Carriageways {
    /// Centre line segments by grid cell: (street, from, to).
    segs: BTreeMap<(i64, i64), Vec<Segment>>,
}

impl Carriageways {
    const CELL: f64 = 16.0;

    fn key(p: DVec2) -> (i64, i64) {
        (
            (p.x / Self::CELL).floor() as i64,
            (p.y / Self::CELL).floor() as i64,
        )
    }

    fn new(world: &Area) -> Self {
        let mut segs: BTreeMap<(i64, i64), Vec<Segment>> = BTreeMap::new();
        for (i, r) in world.roads.iter().enumerate() {
            for s in r.pts.windows(2) {
                let lo = s[0].min(s[1]) - DVec2::splat(r.half + 1.0);
                let hi = s[0].max(s[1]) + DVec2::splat(r.half + 1.0);
                let (k0, k1) = (Self::key(lo), Self::key(hi));
                for x in k0.0..=k1.0 {
                    for y in k0.1..=k1.1 {
                        segs.entry((x, y)).or_default().push((i as u32, s[0], s[1]));
                    }
                }
            }
        }
        Self { segs }
    }

    /// Whether `p` is on a carriageway (within its half width plus
    /// `margin(street)`), and if so whether on one with traffic.
    fn under(&self, p: DVec2, world: &Area, margin: impl Fn(u32) -> f64) -> Option<bool> {
        let mut found: Option<bool> = None;
        for &(j, a, b) in self.segs.get(&Self::key(p)).into_iter().flatten() {
            let r = &world.roads[j as usize];
            if segment_distance(p, a, b) < r.half + margin(j) {
                found = Some(found.unwrap_or(false) || r.has_traffic());
            }
        }
        found
    }
}

/// How walking straight from `a` to `b` may go: if it steps onto a
/// carriageway with traffic, by the lights of the junction there.
fn crossing_wait(a: DVec2, b: DVec2, ways: &Carriageways, world: &Area, signals: &Signals) -> Wait {
    let n = (a.distance(b) / 0.5).ceil().max(2.0) as usize;
    let traffic =
        (1..n).any(|k| ways.under(a.lerp(b, k as f64 / n as f64), world, |_| -0.3) == Some(true));
    if !traffic {
        return Wait::Free;
    }
    if a.distance(b) > LONGEST_CROSSING {
        return Wait::Never;
    }
    signals
        .junction_near(a.lerp(b, 0.5), 30.0)
        .map_or(Wait::Never, Wait::Light)
}

/// The turns at every street end (see [`Turn`]).
fn turns(
    walks: &[Walk],
    met: &Met,
    ends: &[Ends],
    ways: &Carriageways,
    world: &Area,
    signals: &Signals,
) -> Vec<[Vec<Turn>; 2]> {
    let mut out = Vec::with_capacity(walks.len());
    for (i, w) in walks.iter().enumerate() {
        let road = &world.roads[i];
        let reach = road.half + road.pavement * 0.5;
        let mut both: [Vec<Turn>; 2] = [Vec::new(), Vec::new()];
        for (e, list) in met[i].iter().enumerate() {
            let stop = |side: f64| {
                let [e0, e1] = ends[i][side_index(side)];
                if e == 0 {
                    e0
                } else {
                    w.len() - e1
                }
            };
            // The carriageways round the corner.
            for &(j, s) in list {
                let nw = &walks[j as usize];
                let [[a0, a1], [b0, b1]] = ends[j as usize];
                let (lo, hi) = (a0.max(b0), nw.len() - a1.max(b1));
                let (lo, hi) = if lo <= hi {
                    (lo, hi)
                } else {
                    (nw.len() * 0.5, nw.len() * 0.5)
                };
                let r = &world.roads[j as usize];
                let edge = JOIN + r.half;
                // Where walking goes on along the next street: from its
                // corner when it starts here, else either way round.
                let spots: Vec<f64> = if s <= edge {
                    vec![lo]
                } else if s >= nw.len() - edge {
                    vec![hi]
                } else {
                    vec![(s - reach).clamp(lo, hi), (s + reach).clamp(lo, hi)]
                };
                let mut sides = [(0.0, 1.0, Wait::Never, Wait::Never); 2];
                for (k, side) in [-1.0, 1.0].into_iter().enumerate() {
                    let a = w.sample(stop(side), side).0;
                    let mut best = (f64::MAX, 0.0, 1.0);
                    for &sb in &spots {
                        for near in [-1.0, 1.0] {
                            let d = nw.sample(sb, near).0.distance(a);
                            if d < best.0 {
                                best = (d, sb, near);
                            }
                        }
                    }
                    let (_, sb, near) = best;
                    // Where streets meet at odd angles, a pavement's end can
                    // lie on another carriageway: nobody waits there.
                    let on_road = |p: DVec2| ways.under(p, world, |_| -0.3) == Some(true);
                    let wait = |to: f64| {
                        let b = nw.sample(sb, to).0;
                        if on_road(a) || on_road(b) {
                            Wait::Never
                        } else {
                            crossing_wait(a, b, ways, world, signals)
                        }
                    };
                    sides[k] = (sb, near, wait(near), wait(-near));
                }
                both[e].push(Turn { road: j, sides });
            }
        }
        out.push(both);
    }
    out
}

/// Where each street's pavements run over other carriageways: near
/// their ends (where they stop: the corner) and between (side streets'
/// mouths, crossed by the lights if the side street has traffic).
/// Measured along each pavement every metre.
fn pavement_runs(
    walks: &[Walk],
    ways: &Carriageways,
    world: &Area,
    signals: &Signals,
) -> (Vec<Ends>, Vec<[Vec<Mouth>; 2]>) {
    const STEP: f64 = 1.0;
    const NEAR_END: f64 = 12.0;
    let mut ends = Vec::with_capacity(walks.len());
    let mut mouths = Vec::with_capacity(walks.len());
    for (i, w) in walks.iter().enumerate() {
        let len = w.len();
        let mut e = [[0.0; 2]; 2];
        let mut m: [Vec<Mouth>; 2] = [Vec::new(), Vec::new()];
        let n = (len / STEP).ceil().max(1.0) as usize;
        let at = |t: usize| len * t as f64 / n as f64;
        for side in [-1.0, 1.0] {
            let k = side_index(side);
            // Whether each point is on another carriageway, and if so
            // whether one with traffic.
            let under: Vec<Option<bool>> = (0..=n)
                .map(|t| {
                    let p = w.sample(at(t), side).0;
                    // A street that bends back past itself counts too.
                    ways.under(p, world, |j| if j as usize == i { -0.1 } else { 0.3 })
                })
                .collect();
            let mut t = 0;
            while t <= n {
                if under[t].is_none() {
                    t += 1;
                    continue;
                }
                let first = t;
                let mut traffic = false;
                while t <= n && under[t].is_some() {
                    traffic |= under[t] == Some(true);
                    t += 1;
                }
                let last = t - 1;
                let (s0, s1) = (at(first.saturating_sub(1)), at((last + 1).min(n)));
                // Near an end, it is the corner: the pavement stops before.
                if s0 < NEAR_END {
                    e[k][0] = s1;
                } else if s1 > len - NEAR_END {
                    e[k][1] = e[k][1].max(len - s0);
                } else {
                    let mid = w.sample((s0 + s1) * 0.5, side).0;
                    let light = if traffic {
                        signals.junction_near(mid, 30.0)
                    } else {
                        None
                    };
                    let wall = s1 - s0 > LONGEST_CROSSING || (traffic && light.is_none());
                    m[k].push(Mouth {
                        s0,
                        s1,
                        light,
                        wall,
                    });
                }
            }
            // Never more than most of a street.
            let most = len * 0.45;
            e[k] = [e[k][0].min(most), e[k][1].min(most)];
        }
        ends.push(e);
        mouths.push(m);
    }
    (ends, mouths)
}

fn walk_of(r: &WorldRoad) -> Walk {
    let line = Polyline::new(&r.pts);
    let pts = &line.pts;
    let normal = |k: usize| (pts[k + 1] - pts[k]).normalize_or(DVec2::X).perp();
    let last = pts.len() - 1;
    let miters = (0..=last)
        .map(|k| {
            if k == 0 {
                return normal(0);
            }
            if k == last {
                return normal(last - 1);
            }
            let (n0, n1) = (normal(k - 1), normal(k));
            let m = (n0 + n1).normalize_or(n1);
            // Sharp corners are cut off at three times the offset.
            m / m.dot(n1).max(1.0 / 3.0)
        })
        .collect();
    Walk {
        miters,
        line,
        offset: r.walk_offset(),
        bumps: [Vec::new(), Vec::new()],
        people: 0,
        commerce: 0.0,
    }
}

/// Moves each street's walking line round the props standing on its
/// pavements: out towards the houses or in towards the kerb, whichever is
/// nearer and leaves room to pass.
fn step_round_props(walks: &mut [Walk], grid: &BTreeMap<(i64, i64), Vec<u32>>, world: &Area) {
    let clear = WALKER_HALF + 0.05;
    for p in &world.props {
        let parts = p.footprints();
        if parts.is_empty() {
            continue;
        }
        let mut near: Vec<u32> = grid.get(&cell_of(p.pos)).cloned().unwrap_or_default();
        near.sort_unstable();
        near.dedup();
        for i in near {
            let r = &world.roads[i as usize];
            let w = &mut walks[i as usize];
            // Every stretch of the street that passes the prop (a street
            // that bends back, or a loop, passes it more than once).
            for (s, _) in passes(&w.line, p.pos, r.half + r.pavement + 1.0) {
                for side in [-1.0, 1.0] {
                    // Where the walking line itself (mitred round bends)
                    // comes too close: a few metres either way.
                    let (mut s0, mut s1) = (f64::MAX, f64::MIN);
                    let mut x = (s - 6.0).max(0.0);
                    while x <= (s + 6.0).min(w.len()) {
                        let q = w.sample_unbumped(x, side);
                        if parts.iter().any(|f| f.distance(q) <= clear) {
                            (s0, s1) = (s0.min(x), s1.max(x));
                        }
                        x += 0.25;
                    }
                    if s0 > s1 {
                        continue;
                    }
                    let mid = 0.5 * (s0 + s1);
                    let (c, d) = w.line.sample(mid);
                    let n = d.perp() * side;
                    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                    for f in &parts {
                        let (l, h) = f.extent(c, n);
                        (lo, hi) = (lo.min(l), hi.max(h));
                    }
                    if hi < r.half {
                        // In the road: none of the pavement's business.
                        continue;
                    }
                    let line = w.offset;
                    let (kerb, house) = (r.half + WALKER_HALF, r.half + r.pavement - WALKER_HALF);
                    let inner = lo - clear;
                    let outer = hi + clear;
                    let to = match (inner >= kerb, outer <= house) {
                        (true, true) if line - inner <= outer - line => inner,
                        (true, true) => outer,
                        (true, false) => inner,
                        (false, true) => outer,
                        // No room either side: squeeze past on the roomier one.
                        (false, false) if inner - kerb >= house - outer => inner.max(kerb),
                        (false, false) => outer.min(house),
                    };
                    w.bumps[side_index(side)].push(Bump {
                        s0: s0 - 0.45,
                        s1: s1 + 0.45,
                        by: to - line,
                    });
                }
            }
        }
    }
    for w in walks.iter_mut() {
        for list in &mut w.bumps {
            list.sort_by(|x, y| x.s0.total_cmp(&y.s0));
            // A stretch passed twice is found twice: once is enough.
            list.dedup_by(|x, y| {
                (x.s0 - y.s0).abs() < 1e-9 && (x.s1 - y.s1).abs() < 1e-9 && x.by == y.by
            });
        }
    }
}

/// Where `line` passes within `reach` of `p`: (distance along it, how far
/// off) at the nearest point of each stretch that does.
fn passes(line: &hw_gen::geom::Polyline, p: DVec2, reach: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut last_hit = false;
    for k in 0..line.pts.len().saturating_sub(1) {
        let (a, b) = (line.pts[k], line.pts[k + 1]);
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        let d = (a + ab * t).distance(p);
        if d > reach {
            last_hit = false;
            continue;
        }
        let s = line.at[k] + ab.length() * t;
        match out.last_mut() {
            Some(prev) if last_hit => {
                if d < prev.1 {
                    *prev = (s, d);
                }
            }
            _ => out.push((s, d)),
        }
        last_hit = true;
    }
    out
}

/// Barricades close the pavements too: walking turns back before them.
fn stop_at_barricades(
    walks: &[Walk],
    grid: &BTreeMap<(i64, i64), Vec<u32>>,
    world: &Area,
    mouths: &mut [[Vec<Mouth>; 2]],
) {
    for b in hw_gen::urban::edge::barricades(world, &world.roads) {
        let best = grid
            .get(&cell_of(b.at))
            .into_iter()
            .flatten()
            .map(|&i| (i, walks[i as usize].nearest(b.at)))
            .filter(|(i, (_, d))| *d <= world.roads[*i as usize].half)
            .min_by(|x, y| x.1 .1.total_cmp(&y.1 .1));
        let Some((i, (s, _))) = best else { continue };
        for list in &mut mouths[i as usize] {
            list.push(Mouth {
                s0: s - 2.5,
                s1: s + 2.5,
                light: None,
                wall: true,
            });
            list.sort_by(|x, y| x.s0.total_cmp(&y.s0));
        }
    }
}

/// How busy the streets are at `hour` (0 to 1, at their busiest): shops
/// and offices, and homes.
/// Ids of people indoors have this bit set (then the building, the floor
/// and the place: see [`Citizens::indoors`]).
pub const INDOOR: u64 = 1 << 63;
/// Set on the ids of the people along the Aether Tube (see
/// [`Citizens::tube_folk`]).
pub const TUBE_FOLK: u64 = 1 << 62;

/// How long someone indoors stays at one place before the next comes, on
/// average (seconds of game time).
const STAY: f64 = 900.0;

/// The share of a room's places taken at `hour`, by the room's use: homes
/// in the morning and evening (abed at night), offices and labs in working
/// hours, shops while the shopping streets are busy.
pub fn indoor_share(room: RoomKind, hour: f64) -> f64 {
    let h = hour.rem_euclid(24.0);
    let work = if (8.0..18.0).contains(&h) { 1.0 } else { 0.0 };
    let (shops, _) = busy_at(h);
    match room {
        RoomKind::Apartment | RoomKind::Living | RoomKind::Kitchen | RoomKind::Bedroom => {
            if h < 6.0 || h >= 23.5 {
                0.0
            } else if (9.0..16.5).contains(&h) {
                0.25
            } else {
                0.55
            }
        }
        RoomKind::Office | RoomKind::Lab => 0.6 * work,
        RoomKind::Shop => 0.6 * shops * if (7.5..21.0).contains(&h) { 1.0 } else { 0.0 },
        RoomKind::Lobby | RoomKind::Gallery | RoomKind::Hall => 0.3 * shops,
        RoomKind::Core | RoomKind::Corridor | RoomKind::Storage => 0.0,
    }
}

impl Citizens {
    /// Who stands at place `n` of floor `level` of `building` at `time`
    /// (the place is `at`, facing `facing`, in a room of `room`): someone
    /// at home, at work or shopping, by the hour ([`Self::set_hour`]);
    /// `None` while it is empty. People come and go every quarter hour or
    /// so; at home it is the same household.
    #[allow(clippy::too_many_arguments)]
    pub fn indoors(
        &self,
        building: u64,
        level: u32,
        n: u32,
        room: RoomKind,
        at: DVec3,
        facing: DVec2,
        time: f64,
    ) -> Option<Citizen> {
        let id = INDOOR
            | ((building & ((1 << 48) - 1)) << 14)
            | ((level as u64 & 0x3F) << 8)
            | (n as u64 & 0xFF);
        let s = self.seed.child("indoors").child(id);
        let off = s.child("off").unit() * STAY;
        let visit = ((time + off) / STAY).floor() as i64;
        let v = s.child(visit as u64);
        if v.child("here").unit() >= indoor_share(room, self.hour) {
            return None;
        }
        let home = matches!(
            room,
            RoomKind::Apartment | RoomKind::Living | RoomKind::Kitchen | RoomKind::Bedroom
        );
        let who = if home { s } else { v };
        let look = (who.child("look").0 % LOOKS as u64) as u32;
        // Turned a little this way or that, each visit.
        let turn = (v.child("turn").unit() - 0.5) * 1.2;
        Some(Citizen {
            id,
            pos: at,
            facing: DVec2::from_angle(turn).rotate(facing.normalize_or(DVec2::Y)),
            speed: 0.0,
            look,
        })
    }
}

impl Citizens {
    /// Someone who lives or works along the Aether Tube (`folk` number `n`
    /// of deep tile `tile`) at `time`: they stand where the plan puts them,
    /// or walk to their errand and back, pausing at each end. Like the
    /// people indoors, they are asked for by their place, not by id.
    pub fn tube_folk(&self, tile: (i64, i64), n: u32, folk: &Folk, time: f64) -> Citizen {
        let id = TUBE_FOLK
            | (((tile.0 as u64) & 0xF_FFFF) << 28)
            | (((tile.1 as u64) & 0xF_FFFF) << 8)
            | (n as u64 & 0xFF);
        let s = self.seed.child("tube-folk").child(id);
        let look = if folk.mole {
            LOOKS + (s.child("look").0 % MOLE_LOOKS as u64) as u32
        } else {
            (s.child("look").0 % LOOKS as u64) as u32
        };
        let a = DVec2::new(folk.at.x, folk.at.z);
        let (p, facing, speed) = match folk.to {
            None => (a, folk.facing, 0.0),
            Some(b) => {
                let mut rng = s.rng();
                errand(
                    a,
                    b,
                    rng.range(0.8, 1.2),
                    rng.range(5.0, 18.0),
                    time + rng.range(0.0, 60.0),
                )
            }
        };
        Citizen {
            id,
            pos: DVec3::new(p.x, folk.at.y + 0.02, p.y),
            facing: facing.normalize_or(DVec2::Y),
            speed,
            look,
        }
    }
}

/// From `a` to `b` at `speed` and back, pausing `pause` s at each end:
/// where, facing which way, how fast at time `t`.
fn errand(a: DVec2, b: DVec2, speed: f64, pause: f64, t: f64) -> (DVec2, DVec2, f64) {
    let length = a.distance(b).max(0.1);
    let dir = (b - a) / length;
    let walk_t = length / speed;
    let period = 2.0 * (walk_t + pause);
    let t = t.rem_euclid(period);
    if t < walk_t {
        (a + dir * t * speed, dir, speed)
    } else if t < walk_t + pause {
        (b, dir, 0.0)
    } else if t < 2.0 * walk_t + pause {
        (b - dir * (t - walk_t - pause) * speed, -dir, speed)
    } else {
        (a, -dir, 0.0)
    }
}

fn busy_at(hour: f64) -> (f64, f64) {
    // (hour, shops, homes), straight lines between.
    const DAY: [(f64, f64, f64); 11] = [
        (0.0, 0.04, 0.08),
        (5.0, 0.02, 0.04),
        (7.0, 0.25, 0.55),
        (8.5, 0.55, 0.75),
        (10.0, 0.75, 0.45),
        (12.5, 1.0, 0.5),
        (15.0, 0.85, 0.5),
        (17.5, 0.95, 0.85),
        (19.5, 0.5, 0.7),
        (22.0, 0.15, 0.25),
        (24.0, 0.04, 0.08),
    ];
    let h = hour.rem_euclid(24.0);
    for w in DAY.windows(2) {
        let (a, b) = (w[0], w[1]);
        if h <= b.0 {
            let t = (h - a.0) / (b.0 - a.0);
            return (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t);
        }
    }
    (DAY[0].1, DAY[0].2)
}

/// How many people walk each street at its busiest: those its buildings
/// draw (each building counts for the street its front is on) and some
/// passers-by.
fn people_from_buildings(
    walks: &mut [Walk],
    grid: &BTreeMap<(i64, i64), Vec<u32>>,
    world: &Area,
    seed: Seed,
) {
    let mut drawn = vec![(0.0f64, 0.0f64); walks.len()];
    for b in &world.buildings {
        let site = &b.site;
        let front = site.origin + site.along * site.size.x * 0.5;
        let c = cell_of(front);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in grid.get(&(c.0 + dx, c.1 + dy)).into_iter().flatten() {
                    let w = &walks[i as usize];
                    let off = (w.nearest(front).1 - w.offset).abs();
                    if off < FRONTAGE && best.is_none_or(|(d, _)| off < d) {
                        best = Some((off, i));
                    }
                }
            }
        }
        let Some((_, i)) = best else {
            continue;
        };
        let d = draws(site.usage, site.floors);
        let shops = matches!(
            site.usage,
            BuildingUse::ShopsAndOffices | BuildingUse::Offices | BuildingUse::Pavilion
        );
        drawn[i as usize].0 += d;
        if shops {
            drawn[i as usize].1 += d;
        }
    }
    for (i, (w, r)) in walks.iter_mut().zip(&world.roads).enumerate() {
        // A road without pavement has no walkers.
        if r.pavement < 0.8 {
            continue;
        }
        let len = w.len();
        let (all, shops) = drawn[i];
        let through = len * if r.main { THROUGH_MAIN } else { THROUGH };
        let want = (all + through).min(len * MOST);
        let s = seed.child(i as u64);
        w.people = (want.floor() + if s.unit() < want.fract() { 1.0 } else { 0.0 }) as u32;
        w.people = w.people.min(0xF_FFFF);
        w.commerce = if all > 0.0 { shops / all } else { 0.3 };
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

// ---- Molehaven's people -------------------------------------------------

/// Citizens with a look of [`LOOKS`] or more are mole people; there are
/// this many mole looks.
pub const MOLE_LOOKS: u32 = 4;
/// The "street" mole people's ids carry (they live in no street).
const MOLE_HOME: u64 = 0xF_FFFF;
/// Where the market's shoppers walk to, from the well (m): past the stalls
/// (9 m out, their awnings 1.2 m further).
const MARKET_RING: f64 = 11.2;

/// How one of Molehaven's people spends the day.
#[derive(Clone, Copy, Debug)]
enum MoleWay {
    /// Stands (a stallkeeper, a shopper, neighbours chatting).
    Stand(DVec2, DVec2),
    /// From a house door to a market stall and back, pausing at each.
    Errand(DVec2, DVec2),
    /// Round and round the well.
    Stroll { centre: DVec2, r: f64, sign: f64 },
}

/// Molehaven's people: where each spends its day.
fn plan_moles(u: &Underground) -> (f64, DVec2, Vec<MoleWay>) {
    let Some(l) = molehaven_layout(u) else {
        return (0.0, DVec2::ZERO, Vec::new());
    };
    let mut ways = Vec::new();
    let (well, well_r) = l.well;
    for &(at, d) in &l.stalls {
        // The keeper behind the counter, a shopper in front.
        ways.push(MoleWay::Stand(at - d * 1.3, d));
        ways.push(MoleWay::Stand(at + d * 1.5, -d));
    }
    for h in &l.houses {
        match h.door {
            Some(door) => {
                let front = h.at + door * (h.stem * 1.35 + 0.8);
                // To the market ring, just past the stalls' awnings, and
                // back: straight towards the middle, so clear of the stalls
                // and the other houses.
                let out = front - well;
                let market = well + out.normalize_or(DVec2::X) * MARKET_RING;
                // Not if the way runs through a stall.
                let blocked = (0..=20).any(|k| {
                    let p = front.lerp(market, k as f64 / 20.0);
                    l.stalls.iter().any(|&(at, d)| {
                        let q = p - at;
                        let (along, side) = (q.dot(d), q.dot(d.perp()).abs());
                        side < 1.5 && along > -0.7 && along < 1.0
                    })
                });
                if blocked {
                    // A stall stands before the door: nobody waits there.
                } else if out.length() > MARKET_RING + 1.5 {
                    ways.push(MoleWay::Errand(front, market));
                    ways.push(MoleWay::Errand(market, front));
                } else {
                    ways.push(MoleWay::Stand(front, door));
                }
            }
            None => {
                // Neighbours chatting by the house.
                let a = h.at + DVec2::X * (h.stem * 1.35 + 0.7);
                let b = a + DVec2::new(0.4, 1.0);
                ways.push(MoleWay::Stand(a, (b - a).normalize()));
                ways.push(MoleWay::Stand(b, (a - b).normalize()));
            }
        }
    }
    for k in 0..8 {
        ways.push(MoleWay::Stroll {
            centre: well,
            // Inner ring one way, outer the other, evenly spaced.
            r: well_r + 2.6 + (k % 2) as f64 * 0.9,
            sign: if k % 2 == 0 { 1.0 } else { -1.0 },
        });
    }
    (l.floor, well, ways)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedTown;

    fn town() -> Citizens {
        let t = GeneratedTown::generate(Seed(1), 3000.0);
        people(t.area)
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn people_indoors_keep_hours() {
        let mut c = town();
        let place =
            |c: &Citizens, room, n: u32, t: f64| c.indoors(7, 1, n, room, DVec3::ZERO, DVec2::Y, t);
        let count = |c: &Citizens, room| {
            (0..200)
                .filter(|&n| place(c, room, n, 1000.0).is_some())
                .count()
        };
        // Offices empty at night, busy by day; homes abed at 3 and in at 20.
        c.set_hour(3.0);
        assert_eq!(count(&c, RoomKind::Office), 0);
        assert_eq!(count(&c, RoomKind::Apartment), 0);
        c.set_hour(11.0);
        assert!(count(&c, RoomKind::Office) > 80);
        assert!(count(&c, RoomKind::Shop) > 50);
        c.set_hour(20.0);
        assert!(count(&c, RoomKind::Apartment) > 70);
        // Same place, same time, same person; their own ids.
        let a = place(&c, RoomKind::Apartment, 5, 1000.0);
        assert_eq!(
            a.map(|p| (p.id, p.look)),
            place(&c, RoomKind::Apartment, 5, 1000.0).map(|p| (p.id, p.look))
        );
        if let Some(p) = a {
            assert!(p.id & INDOOR != 0);
            assert!(c.citizen(p.id, 1000.0).is_none());
        }
        // People come and go over the hours.
        let at = |t: f64| {
            (0..50)
                .map(|n| place(&c, RoomKind::Apartment, n, t).is_some())
                .collect::<Vec<_>>()
        };
        assert_ne!(at(0.0), at(STAY * 4.0));
    }

    fn people(world: Arc<Area>) -> Citizens {
        let signals = crate::traffic::Traffic::new(world.clone()).signals();
        Citizens::new(world, signals)
    }

    #[test]
    fn molehaven_has_people_who_keep_clear_of_its_furniture() {
        let t = GeneratedTown::generate(Seed(1), 40_000.0);
        let l = molehaven_layout(t.world.underground()).expect("the town has Molehaven");
        let c = people(t.area.clone());
        let (_, centre, ways) = &c.moles;
        assert!(ways.len() >= 30, "{} mole people", ways.len());
        let mut moved = 0;
        for k in 0..240 {
            let time = k as f64 * 5.0;
            let near = c.near(*centre, 80.0, time, 1000);
            let moles: Vec<_> = near.iter().filter(|p| p.look >= LOOKS).collect();
            assert_eq!(
                moles.len(),
                ways.len(),
                "all of them are seen near the cavern"
            );
            for m in moles {
                let p = m.pos.truncate_xz();
                assert!((m.pos.y - l.floor).abs() < 0.1, "on the cavern floor");
                assert!(
                    t.world.underground().at(p).next().is_some(),
                    "{p} is inside the cavern"
                );
                assert!(p.distance(l.well.0) > l.well.1 + 0.5, "{p} in the well");
                for h in &l.houses {
                    assert!(p.distance(h.at) > h.stem * 1.35 + 0.4, "{p} in a house");
                }
                for &(at, d) in &l.stalls {
                    let q = p - at;
                    let (along, side) = (q.dot(d), q.dot(d.perp()).abs());
                    // Behind the counter is the keeper's place.
                    let keeper = (along + 1.3).abs() < 0.01 && side < 0.01;
                    assert!(
                        keeper || !(side < 1.4 && along > -0.6 && along < 0.9),
                        "{p} walks through a stall"
                    );
                }
                if m.speed > 0.0 {
                    moved += 1;
                }
            }
        }
        assert!(moved > 0, "some of them walk about");
    }

    #[test]
    fn people_getting_out_of_cars_run_along_the_pavement_clear_of_things() {
        let t = GeneratedTown::generate(Seed(1), 20_000.0);
        let traffic = crate::traffic::Traffic::new(t.area.clone());
        let c = Citizens::new(t.area.clone(), traffic.signals());
        let p = c.world.spawn_line().0;
        let cars: Vec<_> = (traffic.near(p, 400.0, 100.0, 60).into_iter())
            .filter(|v| !v.flies())
            .collect();
        assert!(cars.len() > 20, "only {} cars", cars.len());
        let things: Vec<_> = c.world.props.iter().flat_map(|p| p.footprints()).collect();
        let mut found = 0;
        for v in &cars {
            let at = v.pos.truncate_xz();
            let blow = at - DVec2::new(v.dir.x, v.dir.z) * 3.0;
            let Some(e) = c.escape(at, blow, 40.0) else {
                continue;
            };
            found += 1;
            assert!(e.len() < 80.0, "a run of {} m", e.len());
            let (end, _) = c.escape_at(&e, e.len());
            let end = end.truncate_xz();
            assert!(
                end.distance(blow) + 0.5 >= at.distance(blow),
                "runs towards trouble"
            );
            // It ends on a pavement: off every carriageway.
            for r in &c.world.roads {
                for w in r.pts.windows(2) {
                    let off = hw_gen::water::segment_distance(end, w[0], w[1]);
                    assert!(off > r.half - 0.1, "{end} ends in the road");
                }
            }
            let mut x = 0.0;
            while x <= e.len() {
                let (q, _) = c.escape_at(&e, x);
                let q = q.truncate_xz();
                for f in things.iter().filter(|f| f.at().distance(q) < 12.0) {
                    assert!(
                        f.distance(q) > WALKER_HALF - 0.05,
                        "{q} runs through a thing"
                    );
                }
                x += 0.5;
            }
        }
        assert!(
            found * 10 >= cars.len() * 9,
            "{found} of {} found a way",
            cars.len()
        );
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
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn citizens_walk_on_pavements_and_move_smoothly() {
        let c = town();
        let p = c.world.spawn_line().0;
        let p_ground = c.world.ground(p);
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
            // Mole people (deep under the core) have their own looks.
            assert!(a.look < LOOKS || a.pos.y < p_ground - 10.0);
        }
        // Over a few minutes, many are walking (the rest pause, chat or
        // wait at the kerb for the lights: about half the time in a town
        // centre).
        let (mut walking, mut all) = (0, 0);
        for k in 0..12 {
            let seen = c.near(p, 300.0, 50.0 + k as f64 * 23.0, 1000);
            walking += seen.iter().filter(|c| c.speed > 0.0).count();
            all += seen.len();
        }
        assert!(walking * 5 > all * 2, "only {walking} of {all} walking");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn people_cross_roads_with_traffic_only_while_the_lights_say_walk() {
        let t = GeneratedTown::generate(Seed(1), 20_000.0);
        let signals = crate::traffic::Traffic::new(t.area.clone()).signals();
        let c = Citizens::new(t.area, signals.clone());
        let p = c.world.spawn_line().0;
        let (mut on_road, mut seen, mut unsafe_) = (0, 0, 0);
        for k in 0..60 {
            let time = 500.0 + k as f64 * 3.7;
            for x in c.near(p, 300.0, time, 3000) {
                // Molehaven is under the streets.
                if x.look >= LOOKS {
                    continue;
                }
                seen += 1;
                let q = x.pos.truncate_xz();
                let cell = cell_of(q);
                let over = c.grid.get(&cell).into_iter().flatten().any(|&i| {
                    let r = &c.world.roads[i as usize];
                    r.has_traffic() && c.walks[i as usize].nearest(q).1 < r.half - 0.6
                });
                if !over {
                    continue;
                }
                on_road += 1;
                let j = signals.junction_near(q, 40.0);
                if !j.is_some_and(|j| signals.walking(j, time)) {
                    unsafe_ += 1;
                }
            }
        }
        assert!(on_road > 0, "nobody crossed in {seen} sightings");
        // A very few odd corners (streets meeting at a sliver of an angle)
        // still let someone step on the road out of turn.
        assert!(
            unsafe_ * 100 <= on_road,
            "{unsafe_} of {on_road} people on the road with no walk light"
        );
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn finding_people_near_a_player_is_quick() {
        let t = GeneratedTown::generate(Seed(1), 40_000.0);
        let c = people(t.area);
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
