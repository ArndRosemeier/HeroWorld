//! The town's animals: this is another planet, and its streets show it.
//!
//! - **Wibbets**: big-eyed climbers the size of a cat, not quite monkeys.
//!   A troop of a few keeps to one stretch of pavement: they scamper
//!   along it, sit and watch the people, hop onto bins, hydrants and
//!   benches, and run up lamp posts and advertising pillars to cling there.
//! - **Drakelets**: pet dragons. A few citizens fly one on a lead; it
//!   flits ahead of them over the heads of the people on the pavement and
//!   circles them when they stand.
//! - **Driftbells**: sky jellies that float along the wide streets above
//!   the lamps, a few together, glowing by night.
//!
//! Like the citizens, every animal is a pure function of the town seed,
//! its id and the time: nothing is kept or sent, and every machine in a
//! co-op game sees the same animals in the same places.
//!
//! Nothing walks or flies through anything solid. Wibbets keep to a lane
//! on the pavement that swings onto the walkers' own line wherever
//! something stands near (and that line is clear by construction), and
//! leave it only for the short clear hop to a perch. A drakelet flies in
//! the way its owner walks and is pushed clear of everything standing on
//! the pavement. Driftbells stay over the carriageway, higher than the
//! lamps, on streets no bridge crosses.

use super::*;
use hw_core::det;
use hw_gen::urban::dystopia::Zone;
use hw_gen::urban::footprint::Footprint;
use hw_gen::urban::props::Perch;
use std::f64::consts::TAU;

/// The kinds of animal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Species {
    Wibbet,
    Drakelet,
    Driftbell,
}

impl Species {
    pub const ALL: [Species; 3] = [Species::Wibbet, Species::Drakelet, Species::Driftbell];

    /// How many coats (colourings) it comes in.
    pub fn coats(self) -> u8 {
        3
    }

    fn tag(self) -> u64 {
        match self {
            Species::Wibbet => 1,
            Species::Drakelet => 2,
            Species::Driftbell => 3,
        }
    }

    /// The species of animal `id`.
    pub fn of(id: u64) -> Option<Species> {
        Species::ALL.into_iter().find(|s| s.tag() == id >> TAG)
    }
}

/// What an animal is doing, as far as its drawing cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    /// Under way (running, climbing, flying, drifting).
    Go,
    /// Still, on all fours (or hovering, or clinging to a post).
    Stay,
    /// Sat up on its haunches (or landed).
    Sit,
}

/// An animal as seen at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Critter {
    pub id: u64,
    pub species: Species,
    /// Which of its species' colourings it has.
    pub coat: u8,
    /// Its feet (a flier's too: the lowest point of it), in the town's frame.
    pub pos: DVec3,
    /// The way it faces (unit, in the ground plane).
    pub facing: DVec2,
    /// Nose up (+) or down (-), degrees: 90 running up a post it faces,
    /// -90 coming down it head first (facing away, its belly to the post).
    pub pitch: f64,
    /// How fast it moves (m/s).
    pub speed: f64,
    pub pose: Pose,
    /// How big it is (1: as modelled).
    pub size: f64,
    /// The citizen holding its lead.
    pub owner: Option<u64>,
    /// Standing or sitting on the pavement (not on a perch, in the air or
    /// under way).
    pub on_ground: bool,
}

/// Where the species tag sits in an id.
const TAG: u32 = 56;

/// The share of the people out walking who fly a drakelet on a lead.
const PETS: f64 = 0.014;
/// How long a lead is (m): a drakelet keeps within it of its owner's hand.
pub const LEAD: f64 = 2.1;
/// How high a drakelet's feet fly: over the heads of the people.
const FLY_LOW: f64 = 1.95;
const FLY_HIGH: f64 = 2.15;
/// How far everything on a pavement is kept from a drakelet's middle
/// (half its wings' span and a hand's breadth).
const WING_ROOM: f64 = 0.5;
/// How big a drakelet is drawn (as modelled, its wings span a metre).
const DRAKELET_SIZE: f64 = 0.85;

/// Of a hundred rounds, this many end on the pavement itself: they would
/// rather be up something.
const GROUND_ROUNDS: f64 = 0.1;
/// Troops of wibbets per metre of pavement (one side of a street).
const TROOPS_PER_M: f64 = 1.0 / 45.0;
/// A troop's stretch of pavement reaches this far either way from its home.
const TROOP_REACH: f64 = 16.0;
/// How far from the walkers' line the wibbets' lane runs, towards the kerb.
const LANE: f64 = 0.55;
/// Half a wibbet's width, and how near it passes to anything standing.
const WIBBET_HALF: f64 = 0.17;
/// A wibbet's round: it picks a place, goes there and rests, a few seconds
/// of it (s). They are restless.
const ROUND: f64 = 24.0;
/// Two places up things this near each other (m, in plan) are got between
/// by a leap, when nothing stands in the way.
const LEAP: f64 = 6.0;
/// How fast a wibbet flies through a leap (m/s), and how long it crouches
/// before it (s).
const LEAP_SPEED: f64 = 5.5;
const CROUCH: f64 = 0.25;
/// How fast a wibbet climbs a post (m/s) and how long a hop takes (s).
const CLIMB: f64 = 2.0;
const HOP: f64 = 0.45;

/// Driftbells float this high over the street, at least and at most:
/// over the lamps and the street trees, between the upper storeys.
const DRIFT_LOW: f64 = 14.0;
const DRIFT_HIGH: f64 = 19.0;
/// How fast they drift (m/s).
const DRIFT: f64 = 0.55;

/// Size of the cells the standing things are indexed by.
const PROP_CELL: f64 = 8.0;
/// How far round a point the index answers for.
const PROP_REACH: f64 = 2.0;

/// Everything standing in the town, by place.
struct PropGrid {
    cells: BTreeMap<(i64, i64), Vec<Footprint>>,
}

impl PropGrid {
    fn new(world: &Area) -> Self {
        let mut cells: BTreeMap<(i64, i64), Vec<Footprint>> = BTreeMap::new();
        let key = |p: DVec2| {
            (
                (p.x / PROP_CELL).floor() as i64,
                (p.y / PROP_CELL).floor() as i64,
            )
        };
        for p in &world.props {
            for f in p.footprints() {
                let reach = DVec2::splat(f.reach() + PROP_REACH);
                let (lo, hi) = (key(f.at() - reach), key(f.at() + reach));
                for x in lo.0..=hi.0 {
                    for z in lo.1..=hi.1 {
                        cells.entry((x, z)).or_default().push(f);
                    }
                }
            }
        }
        Self { cells }
    }

    fn at(&self, p: DVec2) -> &[Footprint] {
        let k = (
            (p.x / PROP_CELL).floor() as i64,
            (p.y / PROP_CELL).floor() as i64,
        );
        self.cells.get(&k).map_or(&[], Vec::as_slice)
    }

    /// How far `p` is from the nearest thing standing (at most
    /// [`PROP_REACH`]).
    fn clearance(&self, p: DVec2) -> f64 {
        self.at(p)
            .iter()
            .map(|f| f.distance(p))
            .fold(PROP_REACH, f64::min)
    }

    /// `p` moved until everything standing is `need` away (as far as two
    /// pushes get it).
    fn pushed_clear(&self, mut p: DVec2, need: f64) -> DVec2 {
        for _ in 0..2 {
            let mut by = DVec2::ZERO;
            for f in self.at(p) {
                let d = f.distance(p);
                if d < need {
                    let away = (p - f.at()).normalize_or(DVec2::X);
                    by += away * (need - d);
                }
            }
            if by == DVec2::ZERO {
                break;
            }
            p += by;
        }
        p
    }

    /// Is the straight way from `a` to `b` at least `need` clear of
    /// everything standing?
    fn way_clear(&self, a: DVec2, b: DVec2, need: f64) -> bool {
        let n = (a.distance(b) / 0.1).ceil().max(1.0) as usize;
        (0..=n).all(|i| self.clearance(a.lerp(b, i as f64 / n as f64)) >= need)
    }
}

/// Can a wibbet leap from stop `a` to stop `b`: both up something, near
/// each other, and nothing standing between (but the two things they are
/// on, near their ends)?
fn leapable(props: &PropGrid, a: &Stop, b: &Stop) -> bool {
    if a.rest == Rest::Ground || b.rest == Rest::Ground {
        return false;
    }
    let d = a.at.distance(b.at);
    if !(0.8..=LEAP).contains(&d) {
        return false;
    }
    let n = (d / 0.1).ceil() as usize;
    (0..=n).all(|i| {
        let u = i as f64 / n as f64;
        let near_end = u * d < 0.7 || (1.0 - u) * d < 0.7;
        near_end || props.clearance(a.at.lerp(b.at, u)) >= WIBBET_HALF
    })
}

/// How a wibbet rests at a stop.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rest {
    /// On the pavement.
    Ground,
    /// On top of something: hopped up from the foot.
    Top,
    /// Clinging to a post: run up from the foot.
    Cling,
}

/// A place a troop's wibbets rest at.
#[derive(Clone, Copy, Debug)]
struct Stop {
    /// Where along the street its lane is left for it.
    s: f64,
    /// On the ground beside it (a place on the lane itself, for a rest
    /// on the pavement).
    foot: DVec2,
    /// Where it rests, and how high over the ground there.
    at: DVec2,
    height: f64,
    rest: Rest,
    /// The way it looks while resting on top or on the ground, or the way
    /// to the post it clings to.
    face: DVec2,
}

/// A troop of wibbets and its stretch of pavement.
struct Troop {
    road: u32,
    side: f64,
    /// Its places, in order along the street.
    stops: Vec<Stop>,
    /// The ones up a post or on top of something (indices into `stops`).
    high: Vec<usize>,
    /// `leaps[i][j]`: stop `i` to stop `j` is one leap.
    leaps: Vec<Vec<bool>>,
    size: u32,
    seed: Seed,
}

/// A street the driftbells float along.
struct Bloom {
    road: u32,
    bells: u32,
    seed: Seed,
}

/// Where the town's animals live: worked out once, on first ask.
pub(super) struct Plan {
    props: PropGrid,
    troops: Vec<Troop>,
    /// Troops by the street they live on.
    by_road: BTreeMap<u32, Vec<u32>>,
    blooms: BTreeMap<u32, Bloom>,
}

/// Smooth step from 0 at `a` to 1 at `b`.
fn ease(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Citizens {
    fn critter_plan(&self) -> &Plan {
        self.critters.get_or_init(|| self.plan_critters())
    }

    fn plan_critters(&self) -> Plan {
        let props = PropGrid::new(&self.world);
        let seed = self.seed.child("critters");
        // The props wibbets climb, by the street and side they stand on.
        let mut perches: BTreeMap<(u32, bool), Vec<(f64, usize)>> = BTreeMap::new();
        for (k, p) in self.world.props.iter().enumerate() {
            if p.perch().is_none() {
                continue;
            }
            let c = cell_of(p.pos);
            let mut best: Option<(f64, u32, f64)> = None;
            for &i in self.grid.get(&c).into_iter().flatten() {
                let (s, d) = self.walks[i as usize].nearest(p.pos);
                let r = &self.world.roads[i as usize];
                if d < r.half + r.pavement + 0.5 && best.is_none_or(|b| d < b.0) {
                    best = Some((d, i, s));
                }
            }
            let Some((_, road, s)) = best else {
                continue;
            };
            let w = &self.walks[road as usize];
            let right = w.sample(s, 1.0).0.distance(p.pos) < w.sample(s, -1.0).0.distance(p.pos);
            perches.entry((road, right)).or_default().push((s, k));
        }
        let dystopia = self.world.town.dystopia.as_ref();
        let utopia = |p: DVec2| dystopia.is_none_or(|d| d.zone(p) == Zone::Utopia);
        let mut troops = Vec::new();
        let mut by_road: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut blooms = BTreeMap::new();
        for (i, w) in self.walks.iter().enumerate() {
            let road = i as u32;
            let r = &self.world.roads[i];
            let len = w.len();
            if len < 20.0 || !utopia(w.line.point(len * 0.5)) {
                continue;
            }
            for side in [-1.0, 1.0] {
                let s = seed
                    .child("troops")
                    .child(road as u64)
                    .child(side_index(side));
                let mut rng = s.rng();
                let n = (len * TROOPS_PER_M + rng.unit()).floor() as usize;
                for k in 0..n {
                    let near = perches.get(&(road, side > 0.0));
                    // A troop lives where there is something to climb.
                    let Some(&(at, _)) = near.and_then(|n| n.get(rng.index(n.len().max(1)))) else {
                        continue;
                    };
                    let home = (at + rng.range(-4.0, 4.0)).clamp(0.0, len);
                    let ts = s.child(k);
                    if let Some(t) = self.troop(&props, road, side, home, near, ts) {
                        by_road.entry(road).or_default().push(troops.len() as u32);
                        troops.push(t);
                    }
                }
            }
            // Driftbells: over wide streets that no bridge crosses.
            let s = seed.child("bloom").child(road as u64);
            let wide = r.half >= 3.4 && len >= 140.0;
            let bridged = self.world.main_bridges.iter().any(|b| {
                (r.pts.windows(2)).any(|w| {
                    segment_distance(b.a, w[0], w[1]) < 30.0
                        || segment_distance(b.b, w[0], w[1]) < 30.0
                        || segment_distance(w[0], b.a, b.b) < 30.0
                })
            });
            if wide && !bridged && s.unit() < (len / 1400.0).min(0.5) {
                blooms.insert(
                    road,
                    Bloom {
                        road,
                        bells: 1 + (s.child("bells").0 % 3) as u32,
                        seed: s,
                    },
                );
            }
        }
        Plan {
            props,
            troops,
            by_road,
            blooms,
        }
    }

    /// The wibbets' lane at `s` along a street's pavement: beside the
    /// walkers' line towards the kerb, and on that line wherever something
    /// stands near (the walkers' line is clear of everything by more than
    /// a wibbet's width). Its point, the way the street runs, and how much
    /// of the lane's offset it has there (0: on the walkers' line).
    fn lane(&self, props: &PropGrid, road: u32, side: f64, s: f64) -> (DVec2, DVec2, f64) {
        let w = &self.walks[road as usize];
        let r = &self.world.roads[road as usize];
        let (p, dir) = w.sample(s, side);
        let room = w.offset + w.bump_at(s, side) - (r.half + WIBBET_HALF + 0.1);
        let off = LANE.min(room).max(0.0);
        let kerbward = -dir.perp() * side;
        let q = p + kerbward * off;
        let f = ease(0.3, 0.9, props.clearance(q));
        (p + kerbward * off * f, dir, f * off / LANE)
    }

    /// Plans a troop round `home` on one pavement of a street: its
    /// stretch (between the corners and the side streets' mouths), places
    /// to sit along the lane and the perches standing by it.
    fn troop(
        &self,
        props: &PropGrid,
        road: u32,
        side: f64,
        home: f64,
        perches: Option<&Vec<(f64, usize)>>,
        seed: Seed,
    ) -> Option<Troop> {
        let w = &self.walks[road as usize];
        let r = &self.world.roads[road as usize];
        if r.pavement < 1.3 {
            return None;
        }
        let [e0, e1] = self.ends[road as usize][side_index(side)];
        let (mut lo, mut hi) = (e0 + 1.0, w.len() - e1 - 1.0);
        for m in &self.mouths[road as usize][side_index(side)] {
            if m.s0 - 1.0 < home && home < m.s1 + 1.0 {
                return None;
            }
            if m.s1 < home {
                lo = lo.max(m.s1 + 1.0);
            } else {
                hi = hi.min(m.s0 - 1.0);
            }
        }
        let (lo, hi) = (lo.max(home - TROOP_REACH), hi.min(home + TROOP_REACH));
        if hi - lo < 10.0 || home < lo || home > hi {
            return None;
        }
        let mut rng = seed.rng();
        let mut stops = Vec::new();
        // Places on the pavement, where the lane runs clear of the walkers.
        let mut s = lo + rng.range(0.3, 1.5);
        while s < hi {
            let (p, dir, f) = self.lane(props, road, side, s);
            if f > 0.85 {
                let out = dir.perp() * side;
                let turn = rng.range(-0.9, 0.9);
                stops.push(Stop {
                    s,
                    foot: p,
                    at: p,
                    height: 0.0,
                    rest: Rest::Ground,
                    face: DVec2::from_angle(turn).rotate(out),
                });
            }
            s += rng.range(2.2, 3.4);
        }
        // Perches: reached straight from the lane with nothing in the way.
        for &(ps, k) in perches.into_iter().flatten() {
            if ps < lo || ps > hi {
                continue;
            }
            let prop = &self.world.props[k];
            let Some(perch) = prop.perch() else {
                continue;
            };
            let (from, _, _) = self.lane(props, road, side, ps);
            let to_lane = (from - prop.pos).normalize_or(DVec2::X);
            let ground = self.world.surface(prop.pos);
            let lift = prop.y - ground;
            let x = DVec2::new(prop.dir.y, -prop.dir.x);
            let stop = match perch {
                Perch::Cling { radius, low, high } => Stop {
                    s: ps,
                    foot: prop.pos + to_lane * radius,
                    at: prop.pos + to_lane * radius,
                    height: lift + rng.range(low, high),
                    rest: Rest::Cling,
                    face: -to_lane,
                },
                Perch::Top {
                    x: px,
                    z,
                    height,
                    from: off,
                } => {
                    let sx = if rng.chance(0.5) { px } else { -px };
                    let at = prop.pos + x * sx + prop.dir * z;
                    // Benches are got onto from the front.
                    let way = if px != 0.0 { prop.dir } else { to_lane };
                    Stop {
                        s: ps,
                        foot: at + way * off,
                        at,
                        height: lift + height,
                        rest: Rest::Top,
                        face: way,
                    }
                }
            };
            // The foot on the pavement, the lane near, nothing between
            // (but the prop itself, at its foot).
            let d = w.nearest(stop.foot).1;
            let on_pavement = d > r.half + 0.2 && d < r.half + r.pavement + 0.2;
            let near = stop.foot.distance(from);
            let free = match stop.rest {
                // Up to the post itself (its solid shape is a little
                // wider than the shaft).
                Rest::Cling => {
                    let reach = prop.footprints().first().map_or(0.0, Footprint::reach);
                    let off = prop.pos + to_lane * (reach + WIBBET_HALF + 0.03);
                    props.way_clear(from, off, WIBBET_HALF)
                }
                _ => props.way_clear(from, stop.foot, WIBBET_HALF),
            };
            if on_pavement && near < 2.6 && free {
                stops.push(stop);
            }
        }
        stops.sort_by(|a, b| a.s.total_cmp(&b.s));
        let high: Vec<usize> = (0..stops.len())
            .filter(|&i| stops[i].rest != Rest::Ground)
            .collect();
        // Only where there are two or more places up things to go to.
        if high.len() < 2 {
            return None;
        }
        let size = (rng.range_int(2, 5) as usize).min(high.len()) as u32;
        let leaps = (0..stops.len())
            .map(|i| {
                (0..stops.len())
                    .map(|j| i != j && leapable(props, &stops[i], &stops[j]))
                    .collect()
            })
            .collect();
        Some(Troop {
            road,
            side,
            stops,
            high,
            leaps,
            size,
            seed,
        })
    }

    /// The stop wibbet `m` of a troop makes for in round `k`: the troop
    /// picks a place, and each of them takes one of the places nearest it
    /// (no two the same).
    fn stop_of(&self, t: &Troop, m: u32, k: i64) -> usize {
        let rk = t.seed.child("focus").child(k as u64);
        // Nearly always one of the places up things; now and then the pavement.
        let up = rk.child("up").unit() >= GROUND_ROUNDS;
        let n = if up { t.high.len() } else { t.stops.len() };
        let size = (t.size as usize).min(n);
        let focus = (rk.0 % n as u64) as usize;
        let first = focus.saturating_sub(size / 2).min(n - size);
        let i = first + (m as usize + k.rem_euclid(size as i64) as usize) % size;
        if up {
            t.high[i]
        } else {
            i
        }
    }

    /// One of a troop's wibbets at `time`.
    fn wibbet(&self, plan: &Plan, id: u64, time: f64) -> Option<Critter> {
        let troop = plan.troops.get(((id >> 8) & 0xFFFF_FFFF) as usize)?;
        let m = (id & 0xFF) as u32;
        if m >= troop.size {
            return None;
        }
        let seed = troop.seed.child("wibbet").child(m);
        let mut rng = seed.rng();
        let coat = if rng.chance(0.7) {
            (troop.seed.child("coat").0 % 3) as u8
        } else {
            rng.index(3) as u8
        };
        let size = rng.range(0.82, 1.08);
        let run = rng.range(2.6, 3.6);
        let t = time + rng.range(0.0, ROUND);
        let k = (t / ROUND).floor() as i64;
        let since = t - k as f64 * ROUND;
        let (ia, ib) = (self.stop_of(troop, m, k - 1), self.stop_of(troop, m, k));
        let (a, b) = (troop.stops[ia], troop.stops[ib]);
        let ks = seed.child(k as u64);
        let wait = ks.child("wait").unit() * 2.5;
        let sits = ks.child("sits").unit() < 0.65;
        let ground = |p: DVec2| self.world.surface(p);
        let make = |p: DVec2, y: f64, facing: DVec2, pitch: f64, speed: f64, pose: Pose| {
            Some(Critter {
                id,
                species: Species::Wibbet,
                coat,
                pos: DVec3::new(p.x, ground(p) + y, p.y),
                facing: facing.normalize_or(DVec2::Y),
                pitch,
                speed,
                pose,
                size,
                owner: None,
                // On the pavement and not on a post: where a hero can meet it.
                on_ground: y == 0.0 && pitch == 0.0,
            })
        };
        let resting = |s: &Stop, sits: bool| match s.rest {
            Rest::Cling => make(s.at, s.height, s.face, 90.0, 0.0, Pose::Stay),
            Rest::Top => make(s.at, s.height, s.face, 0.0, 0.0, Pose::Sit),
            Rest::Ground if sits => make(s.at, 0.0, s.face, 0.0, 0.0, Pose::Sit),
            Rest::Ground => make(s.at, 0.0, s.face, 0.0, 0.0, Pose::Stay),
        };
        let mut x = since - wait;
        if x < 0.0 || a.s == b.s {
            // Not off yet (sat as it was the round before), or staying.
            let before = seed.child((k - 1) as u64).child("sits").unit() < 0.65;
            return resting(&a, if a.s == b.s { before } else { sits && before });
        }
        // Straight across to a place near by: crouch, leap, land.
        if troop.leaps[ia][ib] {
            let (from, to) = (a.at, b.at);
            let way = (to - from).normalize_or(DVec2::Y);
            let air = (from.distance(to) / LEAP_SPEED).max(0.45);
            // How it hangs on a post before it goes.
            let start = if a.rest == Rest::Cling { 90.0 } else { 0.0 };
            if x < CROUCH {
                return make(
                    from,
                    a.height,
                    way,
                    start * (1.0 - x / CROUCH),
                    0.0,
                    Pose::Stay,
                );
            }
            let u = (x - CROUCH) / air;
            if u < 1.0 {
                let arc = 0.3 + 0.08 * from.distance(to);
                let y = a.height + (b.height - a.height) * u + arc * 4.0 * u * (1.0 - u);
                // Nose up, then down, and up the post it lands on.
                let land = if b.rest == Rest::Cling { 90.0 } else { 0.0 };
                let pitch = 25.0 * (1.0 - 2.0 * u) * (1.0 - u) + land * u * u * u;
                return make(from.lerp(to, u), y, way, pitch, LEAP_SPEED, Pose::Go);
            }
            return resting(&b, sits);
        }
        // Down from where it was.
        let down = match a.rest {
            Rest::Ground => 0.0,
            Rest::Top => HOP,
            Rest::Cling => a.height / CLIMB,
        };
        if x < down {
            return match a.rest {
                // Head first, its belly to the post still.
                Rest::Cling => make(a.at, a.height - x * CLIMB, -a.face, -90.0, CLIMB, Pose::Go),
                _ => {
                    let u = x / HOP;
                    let p = a.at.lerp(a.foot, u);
                    let y = a.height * (1.0 - u) + 0.25 * (u * (1.0 - u) * 4.0);
                    make(p, y, a.foot - a.at, 0.0, run, Pose::Go)
                }
            };
        }
        x -= down;
        // Over to the lane, along it, and over to the next place.
        let props = &plan.props;
        let (la, _, _) = self.lane(props, troop.road, troop.side, a.s);
        let (lb, _, _) = self.lane(props, troop.road, troop.side, b.s);
        let (d0, dl, d1) = (a.foot.distance(la), (b.s - a.s).abs(), lb.distance(b.foot));
        let mut d = x * run;
        if d < d0 {
            return make(
                a.foot.lerp(la, d / d0),
                0.0,
                la - a.foot,
                0.0,
                run,
                Pose::Go,
            );
        }
        d -= d0;
        if d < dl {
            let on = b.s >= a.s;
            let s = if on { a.s + d } else { a.s - d };
            let (p, dir, _) = self.lane(props, troop.road, troop.side, s);
            // The lane swings round things: face the way it really goes.
            let ahead = if on { s + 0.2 } else { s - 0.2 };
            let (q, _, _) = self.lane(props, troop.road, troop.side, ahead);
            let facing = (q - p).normalize_or(if on { dir } else { -dir });
            return make(p, 0.0, facing, 0.0, run, Pose::Go);
        }
        d -= dl;
        if d < d1 {
            return make(
                lb.lerp(b.foot, d / d1),
                0.0,
                b.foot - lb,
                0.0,
                run,
                Pose::Go,
            );
        }
        // Up.
        let x = (d - d1) / run;
        match b.rest {
            Rest::Cling if x < b.height / CLIMB => {
                make(b.at, x * CLIMB, b.face, 90.0, CLIMB, Pose::Go)
            }
            Rest::Top if x < HOP => {
                let u = x / HOP;
                let p = b.foot.lerp(b.at, u);
                let y = b.height * u + 0.25 * (u * (1.0 - u) * 4.0);
                make(p, y, b.at - b.foot, 0.0, run, Pose::Go)
            }
            _ => resting(&b, sits),
        }
    }

    /// Does citizen `id` fly a drakelet on a lead? Its coat, if so.
    pub fn pet_of(&self, id: u64) -> Option<u8> {
        if id & (INDOOR | TUBE_FOLK) != 0 || id >> 20 == MOLE_HOME || self.is_sweeper(id) {
            return None;
        }
        let s = self.seed.child("pet").child(id);
        (s.unit() < PETS).then(|| (s.child("coat").0 % 3) as u8)
    }

    /// Where the drakelet of `owner` flies at `time`: its feet.
    fn drakelet_at(&self, plan: &Plan, owner: u64, time: f64) -> Option<DVec3> {
        let c = self.citizen(owner, time)?;
        let s = self.seed.child("drakelet").child(owner);
        let mut rng = s.rng();
        let (p1, p2, p3) = (
            rng.range(0.0, TAU),
            rng.range(0.0, TAU),
            rng.range(0.0, TAU),
        );
        let at = c.pos.truncate_xz();
        // Ahead: where its owner will be in a second or so.
        let lead = 0.85 + 0.4 * det::sin(time * 0.37 + p1);
        let ahead = self
            .citizen(owner, time + lead)
            .map_or(at, |n| n.pos.truncate_xz());
        let ahead = at + (ahead - at).clamp_length_max(1.5);
        let side = c.facing.perp() * 0.2 * det::sin(time * 0.9 + p2);
        // Standing: round and round its owner's head.
        let a = time * 1.15 + p3;
        let circle = at + DVec2::new(det::cos(a), det::sin(a)) * 0.5;
        let going = ease(0.25, 0.9, ahead.distance(at));
        let p = circle.lerp(ahead + side, going);
        let p = plan.props.pushed_clear(p, WING_ROOM);
        // Never further off than the lead lets it.
        let p = at + (p - at).clamp_length_max(LEAD - 0.6);
        let h = FLY_LOW + (FLY_HIGH - FLY_LOW) * (0.5 + 0.5 * det::sin(time * 0.6 + p1));
        Some(DVec3::new(p.x, c.pos.y + h, p.y))
    }

    fn drakelet(&self, plan: &Plan, id: u64, time: f64) -> Option<Critter> {
        let owner = id & ((1 << TAG) - 1);
        let coat = self.pet_of(owner)?;
        let pos = self.drakelet_at(plan, owner, time)?;
        let next = self.drakelet_at(plan, owner, time + 0.1).unwrap_or(pos);
        let v = (next - pos).truncate_xz() / 0.1;
        let c = self.citizen(owner, time)?;
        // Flying, it faces the way it goes; on the spot, the way it circles.
        let speed = v.length();
        let facing = if speed > 0.15 { v / speed } else { c.facing };
        Some(Critter {
            id,
            species: Species::Drakelet,
            coat,
            pos,
            facing,
            pitch: 0.0,
            speed,
            pose: if speed > 0.9 { Pose::Go } else { Pose::Stay },
            size: DRAKELET_SIZE,
            owner: Some(owner),
            on_ground: false,
        })
    }

    fn driftbell(&self, plan: &Plan, id: u64, time: f64) -> Option<Critter> {
        let road = ((id >> 8) & 0xFFFF_FFFF) as u32;
        let k = (id & 0xFF) as u32;
        let bloom = plan.blooms.get(&road).filter(|b| k < b.bells)?;
        let w = &self.walks[bloom.road as usize];
        let r = &self.world.roads[bloom.road as usize];
        let s = bloom.seed.child(k);
        let mut rng = s.rng();
        // Up and down the street, the bloom's bells a few metres apart.
        let (lo, hi) = (14.0, w.len() - 14.0);
        let span = hi - lo;
        let mut bloom_rng = bloom.seed.rng();
        let x = (time * DRIFT + bloom_rng.range(0.0, 2.0 * span) + k as f64 * 5.5)
            .rem_euclid(2.0 * span);
        let (along, on) = if x < span {
            (lo + x, 1.0)
        } else {
            (hi - (x - span), -1.0)
        };
        let (c, dir) = w.line.sample(along);
        let (p1, p2) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
        let sway = (r.half - 2.8).clamp(0.0, 3.0) * det::sin(time * 0.045 + p1);
        let p = c + dir.perp() * sway;
        let height = rng.range(DRIFT_LOW, DRIFT_HIGH - 1.0) + 0.9 * det::sin(time * 0.21 + p2);
        Some(Critter {
            id,
            species: Species::Driftbell,
            coat: (bloom.seed.child("coat").0 % 3) as u8,
            pos: DVec3::new(p.x, self.world.surface(p) + height, p.y),
            facing: dir * on,
            pitch: 0.0,
            speed: DRIFT,
            pose: Pose::Go,
            size: rng.range(1.3, 2.1),
            owner: None,
            on_ground: false,
        })
    }

    /// Animal `id` at `time`; `None` if there is none such (or its owner
    /// is indoors).
    pub fn critter(&self, id: u64, time: f64) -> Option<Critter> {
        let plan = self.critter_plan();
        match Species::of(id)? {
            Species::Wibbet => self.wibbet(plan, id, time),
            Species::Drakelet => self.drakelet(plan, id, time),
            Species::Driftbell => self.driftbell(plan, id, time),
        }
    }

    /// Every animal within `radius` of `around` at `time`, nearest first,
    /// at most `max`. The first ask plans the town's animals (a moment's
    /// work: ask from a worker thread).
    pub fn critters_near(&self, around: DVec2, radius: f64, time: f64, max: usize) -> Vec<Critter> {
        let plan = self.critter_plan();
        let mut out = Vec::new();
        let near = |c: &Critter| c.pos.truncate_xz().distance(around) <= radius;
        for road in self.roads_near(around, radius + 60.0) {
            for &t in plan.by_road.get(&road).into_iter().flatten() {
                for m in 0..plan.troops[t as usize].size {
                    let id = (Species::Wibbet.tag() << TAG) | ((t as u64) << 8) | m as u64;
                    out.extend(self.wibbet(plan, id, time).filter(near));
                }
            }
            if let Some(b) = plan.blooms.get(&road) {
                for k in 0..b.bells {
                    let id = (Species::Driftbell.tag() << TAG) | ((road as u64) << 8) | k as u64;
                    out.extend(self.driftbell(plan, id, time).filter(near));
                }
            }
        }
        // Pets: their owners' routes stay within a few streets of home.
        for road in self.roads_near(around, radius + 400.0) {
            for n in 0..self.walks[road as usize].people {
                let owner = ((road as u64) << 20) | n as u64;
                if self.pet_of(owner).is_some() {
                    let id = (Species::Drakelet.tag() << TAG) | owner;
                    out.extend(self.drakelet(plan, id, time).filter(near));
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

    /// May a wibbet standing at `from` on a pavement come straight over to
    /// `to` (to look at a hero)? Only along a pavement, with nothing
    /// standing in the way.
    pub fn critter_way_clear(&self, from: DVec2, to: DVec2) -> bool {
        let plan = self.critter_plan();
        let on_pavement = |p: DVec2| {
            self.grid.get(&cell_of(p)).into_iter().flatten().any(|&i| {
                let r = &self.world.roads[i as usize];
                let d = self.walks[i as usize].nearest(p).1;
                d > r.half + WIBBET_HALF + 0.1 && d < r.half + r.pavement - WIBBET_HALF - 0.1
            })
        };
        let n = (from.distance(to) / 0.5).ceil().max(1.0) as usize;
        (0..=n).all(|i| on_pavement(from.lerp(to, i as f64 / n as f64)))
            && plan.props.way_clear(from, to, WIBBET_HALF + 0.05)
    }

    /// How near to `p` anything stands on the streets (at most two
    /// metres): what the animals keep clear of.
    pub fn standing_clearance(&self, p: DVec2) -> f64 {
        self.critter_plan().props.clearance(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratedTown;

    fn town() -> Citizens {
        let t = GeneratedTown::generate(Seed(1), 20_000.0);
        let signals = crate::traffic::Traffic::new(t.area.clone()).signals();
        Citizens::new(t.area, signals)
    }

    fn flat(p: DVec3) -> DVec2 {
        p.truncate_xz()
    }

    #[test]
    fn same_seed_same_animals() {
        let (a, b) = (town(), town());
        let p = a.world.spawn_line().0;
        let x = a.critters_near(p, 400.0, 1234.5, 500);
        assert_eq!(x, b.critters_near(p, 400.0, 1234.5, 500));
        for s in Species::ALL {
            let n = x.iter().filter(|c| c.species == s).count();
            assert!(n > 0, "no {s:?} within 400 m of the start");
            assert!(x.iter().all(|c| c.coat < c.species.coats()));
        }
        // Not too many: a handful of dragons, wibbets in a few troops.
        let dragons = x.iter().filter(|c| c.species == Species::Drakelet).count();
        let people = a.near(p, 400.0, 1234.5, 100_000).len();
        assert!(
            dragons * 30 < people,
            "{dragons} dragons to {people} people"
        );
        // Every one is found again by its id.
        for c in &x {
            assert_eq!(a.critter(c.id, 1234.5), Some(*c));
        }
    }

    #[test]
    fn animals_keep_clear_of_everything_standing() {
        let c = town();
        let plan = c.critter_plan();
        let p = c.world.spawn_line().0;
        let ids: Vec<u64> = (0..4)
            .flat_map(|k| c.critters_near(p, 450.0, 300.0 + 97.0 * k as f64, 2000))
            .map(|x| x.id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let (mut ran, mut climbed, mut flew, mut drifted) = (0, 0, 0, 0);
        let dt = 1.0 / 20.0;
        for &id in &ids {
            let mut last: Option<Critter> = None;
            for i in 0..(2.4 * ROUND / dt) as usize {
                let t = 300.0 + i as f64 * dt;
                let Some(a) = c.critter(id, t) else {
                    last = None;
                    continue;
                };
                assert!(a.pos.is_finite() && a.facing.is_finite());
                let q = flat(a.pos);
                let ground = c.world.surface(q);
                let clear = plan.props.clearance(q);
                if let Some(b) = last {
                    // No jumps: at most a quick dart per step (a drakelet
                    // swinging round to the other side of its owner).
                    let step = a.pos.distance(b.pos);
                    assert!(step < 12.0 * dt, "{:?} {id} jumped {step} m", a.species);
                }
                match a.species {
                    Species::Wibbet => {
                        let troop = &plan.troops[((id >> 8) & 0xFFFF_FFFF) as usize];
                        let r = &c.world.roads[troop.road as usize];
                        let off = c.walks[troop.road as usize].nearest(q).1;
                        assert!(
                            off > r.half && off < r.half + r.pavement + 0.3,
                            "wibbet {id} is {off} m from its street's middle"
                        );
                        let up = a.pos.y - ground;
                        assert!((-0.01..8.0).contains(&up), "wibbet {id} is {up} m up");
                        if up > 0.05 {
                            climbed += 1;
                        } else if a.speed > 0.0 {
                            ran += 1;
                            // On the ground it is clear of everything but
                            // the perch it is at the foot of.
                            let at_perch = troop
                                .stops
                                .iter()
                                .any(|s| s.rest != Rest::Ground && s.foot.distance(q) < 0.9);
                            assert!(
                                clear >= WIBBET_HALF - 0.02 || at_perch,
                                "wibbet {id} runs {clear} m from something at {q}"
                            );
                        }
                    }
                    Species::Drakelet => {
                        flew += 1;
                        assert!(
                            clear >= WING_ROOM * DRAKELET_SIZE - 0.03,
                            "drakelet {id} flies {clear} m from something at {q}"
                        );
                        let owner = c.citizen(a.owner.unwrap(), t).expect("it has an owner");
                        let hand = owner.pos + DVec3::Y * 0.9;
                        assert!(a.pos.distance(hand) < LEAD, "the lead is too short");
                        assert!(a.pos.y - owner.pos.y >= FLY_LOW - 0.01);
                    }
                    Species::Driftbell => {
                        drifted += 1;
                        assert!(a.pos.y - ground >= DRIFT_LOW - 1.0);
                        let road = ((id >> 8) & 0xFFFF_FFFF) as usize;
                        let off = c.walks[road].nearest(q).1;
                        assert!(off <= c.world.roads[road].half - 2.5, "not over the middle");
                    }
                }
                last = Some(a);
            }
        }
        assert!(ran > 500 && climbed > 500, "{ran} ran, {climbed} climbed");
        assert!(
            flew > 500 && drifted > 500,
            "{flew} flew, {drifted} drifted"
        );
    }

    #[test]
    fn wibbets_rest_on_what_is_drawn() {
        // Each perch is where its prop stands, as high as the prop's model.
        let c = town();
        let plan = c.critter_plan();
        let (mut cling, mut top) = (0, 0);
        for t in &plan.troops {
            assert!(t.size as usize <= t.stops.len() && t.size >= 2);
            assert!(t.high.iter().all(|&i| t.stops[i].rest != Rest::Ground));
            for s in &t.stops {
                match s.rest {
                    Rest::Ground => assert_eq!(s.height, 0.0),
                    Rest::Cling => {
                        cling += 1;
                        assert!(plan.props.clearance(s.at) < 0.12, "clings to thin air");
                        assert!(s.height > 0.5 && s.height < 5.0);
                    }
                    Rest::Top => {
                        top += 1;
                        assert_eq!(plan.props.clearance(s.at), 0.0, "sits on thin air");
                    }
                }
            }
        }
        let leaps: usize = plan
            .troops
            .iter()
            .map(|t| t.leaps.iter().flatten().filter(|&&l| l).count())
            .sum();
        println!("{} troops, {leaps} leaps", plan.troops.len());
        assert!(leaps > 20, "{leaps} places to leap between");
        assert!(cling > 10 && top > 3, "{cling} posts, {top} tops");
    }

    #[test]
    fn wibbets_are_up_things_and_on_the_move() {
        // Over a long time most of them are up a post or on something, or
        // under way; they rarely just stand on the pavement.
        let c = town();
        let p = c.world.spawn_line().0;
        let ids: Vec<u64> = c
            .critters_near(p, 450.0, 300.0, 2000)
            .into_iter()
            .filter(|x| x.species == Species::Wibbet)
            .map(|x| x.id)
            .collect();
        println!(
            "{} wibbets near the start, {} troops in all",
            ids.len(),
            c.critter_plan().troops.len()
        );
        assert!(ids.len() > 20, "{} wibbets", ids.len());
        let (mut still, mut up, mut going, mut all) = (0, 0, 0, 0);
        for &id in &ids {
            for i in 0..(6.0 * ROUND / 0.5) as usize {
                let Some(a) = c.critter(id, 300.0 + i as f64 * 0.5) else {
                    continue;
                };
                all += 1;
                let high = a.pos.y - c.world.surface(flat(a.pos)) > 0.05;
                if a.speed > 0.0 {
                    going += 1;
                } else if high {
                    up += 1;
                } else {
                    still += 1;
                }
            }
        }
        let f = |n: i32| n as f64 / all as f64;
        println!(
            "wibbets: {:.0}% still on the pavement, {:.0}% up something, {:.0}% under way",
            f(still) * 100.0,
            f(up) * 100.0,
            f(going) * 100.0
        );
        assert!(f(still) < 0.15, "{:.2} stand on the pavement", f(still));
        assert!(f(going) > 0.1, "{:.2} under way", f(going));
    }
}
