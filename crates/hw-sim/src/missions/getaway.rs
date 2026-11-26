//! A gang's flying getaway car, the heist it waits at and the chase
//! after it.
//!
//! **The car** is a hover car in the gang's paint, a target like a
//! villain ([`EnemyKind::Getaway`]) that never fights back. The case
//! moves it: standing at the kerb with its canopy up while the gang
//! loads it, then low along the main streets (their graph,
//! [`crate::streets`]), over the traffic and under the roofs, so a hero
//! on foot can keep up and one who jumps can reach it. Every quarter of
//! its health it loses makes it sputter: it sinks and crawls for a few
//! seconds. Out of health it comes down where it is and stands there, a
//! wreck, and its crew gets out to fight.
//!
//! **A heist** ([`CaseKind::Heist`]): guards in front of the shop's
//! door, lookouts by the car, and two bagmen who carry sack after sack
//! from the door to the car and mind nothing else. [`TRIPS`] sacks and
//! everyone still standing runs for the car, climbs in, and it takes off:
//! the heist is a chase now. Beat the bagmen, or break the car where it
//! stands, and nobody gets away.
//!
//! **A chase** ([`CaseKind::Chase`]): the car is in the air when the
//! heroes get there. Flying for [`ESCAPE_SECONDS`], or with no hero
//! anywhere near for a while, it is gone and the case is lost.

use super::*;
use crate::police::{Landing, LandingSearch};
use crate::streets::Streets;
use crate::Orientation;
use std::collections::VecDeque;

/// Half the car's width and length (m) and its height: the box it is
/// solid in when it stands, which is a parked hover car's
/// ([`hw_gen::urban::props::car_block`]: what is drawn is what collides).
pub const CAR_HALF: DVec2 = DVec2::new(1.2, 2.4);
pub const CAR_HEIGHT: f64 = 1.85;
/// How high over the street it flies (m), and while it sputters.
pub const FLY_HEIGHT: f64 = 6.5;
pub const STALL_HEIGHT: f64 = 2.4;
/// Its speed (m/s): with a hero on its tail, with none near, with none
/// anywhere near, and while it sputters.
pub const SPOOKED: f64 = 12.0;
pub const CRUISE: f64 = 10.0;
pub const DAWDLE: f64 = 8.0;
pub const STALL_SPEED: f64 = 3.0;
/// A hero this near spooks it; with none this near it dawdles (m).
pub const SPOOK_REACH: f64 = 35.0;
pub const DAWDLE_REACH: f64 = 110.0;
/// Seconds it sputters each time it has lost another quarter of its health.
pub const STALL_SECONDS: f64 = 3.0;
/// In the air this long (s), it has got away.
pub const ESCAPE_SECONDS: f64 = 210.0;
/// How fast it turns (rad/s), gathers speed (m/s²), climbs and sinks (m/s).
const TURN: f64 = 2.2;
const ACCEL: f64 = 6.0;
const CLIMB: f64 = 4.0;
/// It sees what stands in its way up to this far over it (m), and passes
/// over it by this much.
const LOOK_UP: f64 = 30.0;
const CLEAR_OVER: f64 = 1.5;
/// It has reached a point of its way this near it (m).
const PASSED: f64 = 7.0;
/// Without streets to follow (a bare test ground) it circles this wide (m).
const CIRCLE: f64 = 60.0;
/// Sacks the bagmen carry to the car before the gang makes off, how many
/// bagmen, and the seconds a bagman takes at the door and at the car.
pub const TRIPS: u32 = 8;
pub const BAGMEN: u32 = 2;
pub const DWELL: f64 = 2.0;
/// A bagman is at the door or the car this near it (m).
const AT_HAND: f64 = 1.8;
/// Seconds the gang has to climb in once the last sack is aboard, and how
/// near the car someone is in it (m).
pub const BOARD_SECONDS: f64 = 8.0;
const BOARD_REACH: f64 = 2.8;
/// A heist or a getaway waits this long (s) for a hero to come near, and
/// a second more for every [`ARRIVE_SPEED`] metres they are away when the
/// case is taken; then the gang is gone.
pub const ARRIVE_SECONDS: f64 = 120.0;
pub const ARRIVE_SPEED: f64 = 7.0;
/// The crew that gets out of a car brought down: at least this many.
pub const CREW_SIZE: u32 = 3;

fn ticks(s: f64) -> u64 {
    (s * TICK_HZ as f64).round() as u64
}

fn flat2(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.z)
}

fn dir_of(yaw: f64) -> DVec2 {
    DVec2::new(hw_core::det::sin(yaw), hw_core::det::cos(yaw))
}

/// What the car is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarState {
    /// Standing at the kerb, its canopy up.
    Parked,
    Flying,
    /// Out of health, coming down.
    Falling,
    /// Down for good.
    Wrecked,
}

/// The solid box of a car standing at `pos` (its feet), nose to `yaw`.
pub fn car_solid(pos: DVec3, yaw: f64) -> crate::collision::Solid {
    let fwd = dir_of(yaw);
    crate::collision::Solid {
        origin: flat2(pos),
        along: DVec2::new(fwd.y, -fwd.x),
        inward: fwd,
        min: -CAR_HALF,
        max: CAR_HALF,
        y0: pos.y,
        y1: pos.y + CAR_HEIGHT,
    }
}

/// The solid boxes of the getaway cars standing among `all` (for a joined
/// player's hero: the host puts its own into the world).
pub fn solids_of(all: &[hw_net::EntityState]) -> Vec<crate::collision::Solid> {
    all.iter()
        .filter(|e| e.kind == hw_net::EntityKind::Getaway && e.grounded)
        .map(|e| car_solid(DVec3::from_array(e.pos), crate::robots::rotation_yaw(e.rot)))
        .collect()
}

/// A getaway car.
#[derive(Clone, Debug)]
pub(crate) struct Car {
    pub id: NetId,
    e: Entity,
    spot: usize,
    pub state: CarState,
    pub pos: DVec3,
    pub yaw: f64,
    speed: f64,
    /// How fast it falls (m/s, down is negative).
    fall: f64,
    /// The street nodes ahead of it (and the one it last passed), and the
    /// points they stand for.
    came: Option<u32>,
    nodes: VecDeque<u32>,
    way: VecDeque<DVec2>,
    /// Turns taken so far (each its own roll) and what the rolls are from.
    hops: u64,
    seed: u64,
    /// Where it started (the middle of its circle without streets).
    origin: DVec2,
    /// Quarters of its health it had when last looked at.
    quarters: u8,
    stall_until: u64,
    pub flying_since: Option<u64>,
}

impl Car {
    /// A car at `pos` (its feet), nose to `yaw`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn(
        world: &mut World,
        tick: u64,
        heroes: usize,
        level: u8,
        pos: DVec3,
        yaw: f64,
        state: CarState,
        seed: u64,
    ) -> Option<Car> {
        let facing = dir_of(yaw);
        let spot = {
            let mut f = world.resource_mut::<Fights>();
            f.spots.push(Spot {
                once: true,
                groups: 1,
                members: 1,
                ..Spot::fight(pos, facing)
            });
            f.spots.len() - 1
        };
        let id = combat::spawn_enemy(world, tick, EnemyKind::Getaway, level, spot, pos, heroes);
        let e = world
            .query::<(Entity, &Net)>()
            .iter(world)
            .find(|(_, n)| n.0 == id)
            .map(|(e, _)| e)?;
        let car = Car {
            id,
            e,
            spot,
            state,
            pos,
            yaw,
            speed: 0.0,
            fall: 0.0,
            came: None,
            nodes: VecDeque::new(),
            way: VecDeque::new(),
            hops: 0,
            seed,
            origin: flat2(pos),
            quarters: 4,
            stall_until: 0,
            flying_since: (state == CarState::Flying).then_some(tick),
        };
        car.write(world);
        Some(car)
    }

    /// Standing (it is solid and its canopy is up)?
    pub(crate) fn stands(&self) -> bool {
        matches!(self.state, CarState::Parked | CarState::Wrecked)
    }

    pub(crate) fn solid(&self) -> crate::collision::Solid {
        car_solid(self.pos, self.yaw)
    }

    /// Where its side is, to stand at (`right` 1 or -1).
    pub(crate) fn beside(&self, right: f64, out: f64) -> DVec3 {
        let f = dir_of(self.yaw);
        let r = DVec2::new(f.y, -f.x) * (right * (CAR_HALF.x + out));
        self.pos + DVec3::new(r.x, 0.0, r.y)
    }

    /// Puts it into the world where the case has it.
    fn write(&self, world: &mut World) {
        let f = dir_of(self.yaw);
        let vel = if self.state == CarState::Flying || self.state == CarState::Falling {
            DVec3::new(f.x * self.speed, self.fall, f.y * self.speed)
        } else {
            DVec3::ZERO
        };
        let Ok(mut e) = world.get_entity_mut(self.e) else {
            return;
        };
        if let Some(mut m) = e.get_mut::<Motion>() {
            let body = crate::collision::Body {
                pos: self.pos,
                vel,
                grounded: self.stands(),
            };
            if m.0 != body {
                m.0 = body;
            }
        }
        let rot = crate::robots::yaw_rotation(self.yaw);
        match e.get_mut::<Orientation>() {
            Some(mut o) => {
                if o.0 != rot {
                    o.0 = rot;
                }
            }
            None => {
                e.insert(Orientation(rot));
            }
        }
    }

    /// Its health as a share of what it had, and whether it still flies
    /// (false once it is beaten, or gone).
    fn health(&self, world: &World) -> (f32, bool) {
        match world.get::<Fighter>(self.e) {
            Some(f) => (
                (f.health / f.max_health.max(1.0)).clamp(0.0, 1.0),
                f.fighting(),
            ),
            None => (0.0, false),
        }
    }

    /// Takes off from where it stands.
    pub(crate) fn take_off(&mut self, tick: u64) {
        if self.state == CarState::Parked {
            self.state = CarState::Flying;
            self.flying_since = Some(tick);
        }
    }

    /// Has it been in the air so long that it is away?
    pub(crate) fn escaped(&self, tick: u64) -> bool {
        self.state == CarState::Flying
            && self
                .flying_since
                .is_some_and(|t| tick >= t + ticks(ESCAPE_SECONDS))
    }

    /// Seconds until it is away (while it flies).
    pub(crate) fn seconds_left(&self, tick: u64) -> Option<f64> {
        let t = self
            .flying_since
            .filter(|_| self.state == CarState::Flying)?;
        Some((t + ticks(ESCAPE_SECONDS)).saturating_sub(tick) as f64 * DT)
    }

    /// More of its way: along the street it is on, now and then round a
    /// corner, back only from a dead end.
    fn look_ahead(&mut self, streets: Option<&Streets>) {
        while self.way.len() < 6 {
            let Some(s) = streets.filter(|s| !s.is_empty()) else {
                // No streets: round and round where it started.
                let a = std::f64::consts::TAU * (self.hops % 16) as f64 / 16.0;
                self.way
                    .push_back(self.origin + hw_core::det::dir2(a) * CIRCLE);
                self.hops += 1;
                continue;
            };
            let Some(&at) = self.nodes.back() else {
                let Some(n) = s.nearest(flat2(self.pos)) else {
                    return;
                };
                self.nodes.push_back(n);
                self.way.push_back(s.node(n));
                continue;
            };
            let prev = if self.nodes.len() >= 2 {
                Some(self.nodes[self.nodes.len() - 2])
            } else {
                self.came
            };
            let here = s.node(at);
            let coming = match prev {
                Some(p) => (here - s.node(p)).normalize_or(dir_of(self.yaw)),
                None => dir_of(self.yaw),
            };
            let mut options: Vec<(f64, u32)> = s
                .links(at)
                .iter()
                .copied()
                .filter(|&m| Some(m) != prev)
                .map(|m| ((s.node(m) - here).normalize_or_zero().dot(coming), m))
                .collect();
            // Straightest first.
            options.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            self.hops += 1;
            let next = match options.len() {
                0 => match prev {
                    Some(p) => p,
                    None => return,
                },
                1 => options[0].1,
                n => {
                    let mut rng = Seed(self.seed).child("turn").child(self.hops).rng();
                    if rng.chance(0.65) {
                        options[0].1
                    } else {
                        options[1 + rng.index(n - 1)].1
                    }
                }
            };
            self.nodes.push_back(next);
            self.way.push_back(s.node(next));
        }
    }

    /// One tick of it in the air: along its way, or down.
    pub(crate) fn fly(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        match self.state {
            CarState::Parked | CarState::Wrecked => {
                // Broken where it stands: a wreck.
                if self.state == CarState::Parked && !self.health(world).1 {
                    self.state = CarState::Wrecked;
                    self.write(world);
                }
                return;
            }
            CarState::Flying => {
                let (share, alive) = self.health(world);
                if !alive {
                    self.state = CarState::Falling;
                    self.fall = 0.0;
                } else {
                    let q = (share * 4.0).ceil().clamp(0.0, 4.0) as u8;
                    if q < self.quarters {
                        self.stall_until = tick + ticks(STALL_SECONDS);
                    }
                    self.quarters = q;
                }
            }
            CarState::Falling => {}
        }
        let solids = world.resource::<crate::Solids>().0.clone();
        let here = flat2(self.pos);
        if self.state == CarState::Falling {
            // It glides on a little and drops, onto whatever is under it.
            self.speed *= 0.96;
            self.fall -= 9.0 * DT;
            let f = dir_of(self.yaw);
            let next = here + f * (self.speed * DT);
            let y = self.pos.y + self.fall * DT;
            let nose = next + f * CAR_HALF.y;
            let blocked = solids.solid_at(DVec3::new(nose.x, y + 0.7, nose.y))
                || solids.inside(DVec3::new(nose.x, y + 0.7, nose.y)).is_some();
            let to = if blocked {
                self.speed = 0.0;
                here
            } else {
                next
            };
            let floor = solids
                .floor_under(to, self.pos.y + 0.3)
                .max(solids.ground_at(to))
                .max(crate::GROUND_Y);
            if y <= floor {
                self.pos = DVec3::new(to.x, floor, to.y);
                self.state = CarState::Wrecked;
                self.speed = 0.0;
                self.fall = 0.0;
            } else {
                self.pos = DVec3::new(to.x, y, to.y);
            }
            self.write(world);
            return;
        }
        let streets = {
            let f = world.resource::<Fights>();
            f.walks.clone().or_else(|| f.streets.clone())
        };
        while self.way.front().is_some_and(|p| p.distance(here) < PASSED) {
            self.way.pop_front();
            if let Some(n) = self.nodes.pop_front() {
                self.came = Some(n);
            }
        }
        self.look_ahead(streets.as_deref());
        let stalled = tick < self.stall_until;
        let nearest = heroes
            .iter()
            .map(|h| h.at.distance(self.pos))
            .fold(f64::MAX, f64::min);
        let mut top = if stalled {
            STALL_SPEED
        } else if nearest < SPOOK_REACH {
            SPOOKED
        } else if nearest > DAWDLE_REACH {
            DAWDLE
        } else {
            CRUISE
        };
        if let Some(&to) = self.way.front() {
            let d = to - here;
            let want = hw_core::det::atan2(d.x, d.y);
            let mut err = (want - self.yaw) % std::f64::consts::TAU;
            if err > std::f64::consts::PI {
                err -= std::f64::consts::TAU;
            } else if err < -std::f64::consts::PI {
                err += std::f64::consts::TAU;
            }
            self.yaw += err.clamp(-TURN * DT, TURN * DT);
            // Slower round a corner.
            top *= 1.0 - 0.5 * (err.abs() / 1.0).min(1.0);
        }
        self.speed += (top - self.speed).clamp(-ACCEL * DT, ACCEL * DT);
        let f = dir_of(self.yaw);
        let side = DVec2::new(f.y, -f.x);
        let mut next = here + f * (self.speed * DT);
        // Over the street, and over whatever stands in its way: it climbs
        // early for what it sees ahead.
        let hover = if stalled { STALL_HEIGHT } else { FLY_HEIGHT };
        let mut over = solids.ground_at(next).max(crate::GROUND_Y) + hover;
        for ahead in [0.0, 7.0, 14.0, 22.0] {
            for s in [-CAR_HALF.x, 0.0, CAR_HALF.x] {
                let p = next + f * ahead + side * s;
                let top = solids.floor_under(p, self.pos.y + LOOK_UP);
                over = over.max(top + CLEAR_OVER);
            }
        }
        let y = self.pos.y + (over - self.pos.y).clamp(-CLIMB * DT, CLIMB * DT);
        // Never into anything: it stops short and climbs until it is clear.
        let hits = |c: DVec2, y: f64| {
            [-1.0, 0.0, 1.0].iter().any(|&i| {
                [-1.0, 0.0, 1.0].iter().any(|&j| {
                    let q = c + side * (CAR_HALF.x * i) + f * (CAR_HALF.y * j);
                    [0.15, 0.9, CAR_HEIGHT]
                        .iter()
                        .any(|h| solids.solid_at(DVec3::new(q.x, y + h, q.y)))
                })
            })
        };
        let mut y = y;
        if hits(next, y) {
            next = here;
            self.speed = 0.0;
            // Straight up where it is, if that is free; else it stays.
            let up = self.pos.y + CLIMB * DT;
            y = if hits(here, up) { self.pos.y } else { up };
        }
        self.pos = DVec3::new(next.x, y, next.y);
        self.write(world);
    }

    /// Takes it out of the world.
    pub(crate) fn remove(&self, world: &mut World) {
        if world.get_entity(self.e).is_ok() {
            world.despawn(self.e);
        }
        if let Some(s) = world.resource_mut::<Fights>().spots.get_mut(self.spot) {
            s.members = 0;
            s.respawn_at = None;
        }
    }
}

/// A bagman: one of the crew's two who carry the loot to the car.
#[derive(Clone, Copy, Debug)]
struct Bagman {
    id: NetId,
    /// On the way to the car with a sack (else to the door for the next).
    loaded: bool,
    /// Standing at the door or the car until this tick.
    until: Option<u64>,
}

/// Where a heist or a chase stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The car is still coming down by the door.
    Arriving,
    /// The bagmen carry the loot to the car.
    Loading,
    /// The last sack is aboard: everyone runs for the car, until this tick.
    Boarding(u64),
    /// The car is in the air.
    Flying,
    /// The car is down, or broken where it stood: whoever is left fights.
    Down,
}

/// A heist's or a chase's own things, beside the case's groups.
#[derive(Clone, Debug)]
pub(crate) struct Job {
    pub car: Option<Car>,
    search: Option<LandingSearch>,
    pub phase: Phase,
    bagmen: Vec<Bagman>,
    /// Sacks in the car so far.
    pub loaded: u32,
    /// Who climbed in before it took off.
    aboard: u32,
    /// It took off (the crew that counts is the one that gets out of it).
    flew: bool,
    /// The shop's door, and the way it looks out.
    door: DVec3,
    out: DVec2,
}

impl Running {
    /// A heist: guards in front of the door now, the car and the rest as
    /// soon as a place for it is found (a couple of places a tick).
    pub(super) fn take_heist(
        &mut self,
        world: &mut World,
        tick: u64,
        heroes: usize,
        town: Option<&hw_gen::urban::area::Area>,
    ) {
        let site = self.site().clone();
        let solids = world.resource::<crate::Solids>().0.clone();
        let out = site.facing.normalize_or(DVec2::X);
        let door = open_near(&solids, site.at).unwrap_or(site.at);
        let extra = heroes.saturating_sub(1) as u32;
        let guards = self.group(
            world,
            tick,
            heroes,
            door + DVec3::new(out.x, 0.0, out.y) * 2.5,
            out,
            ROOM_SIZE + extra,
            None,
            false,
        );
        self.rooms = vec![guards];
        self.way_in = Some(door);
        self.getaway = Some(Job {
            car: None,
            search: Some(LandingSearch::new(town, &solids, door, out)),
            phase: Phase::Arriving,
            bagmen: Vec::new(),
            loaded: 0,
            aboard: 0,
            flew: false,
            door,
            out,
        });
    }

    /// A chase: the car is in the air over the place, its crew in it.
    pub(super) fn take_chase(&mut self, world: &mut World, tick: u64, heroes: usize) {
        let site = self.site().clone();
        let solids = world.resource::<crate::Solids>().0.clone();
        let ground = solids.ground_at(flat2(site.at)).max(crate::GROUND_Y);
        let pos = DVec3::new(site.at.x, ground + FLY_HEIGHT, site.at.z);
        let f = site.facing.normalize_or(DVec2::X);
        let car = Car::spawn(
            world,
            tick,
            heroes,
            self.level(world),
            pos,
            hw_core::det::atan2(f.x, f.y),
            CarState::Flying,
            self.dice(),
        );
        self.getaway = Some(Job {
            car,
            search: None,
            phase: Phase::Flying,
            bagmen: Vec::new(),
            loaded: TRIPS,
            aboard: CREW_SIZE - 1,
            flew: true,
            door: site.at,
            out: f,
        });
    }

    /// The gang's level at this case.
    fn level(&self, world: &World) -> u8 {
        world
            .resource::<Fights>()
            .team_level
            .saturating_add(self.plan.extra)
    }

    /// What this case's own rolls are made from.
    fn dice(&self) -> u64 {
        Seed(self.number as u64)
            .child(self.gang as u64)
            .child(self.step as u64)
            .child("getaway")
            .rng()
            .next_u64()
    }

    /// Every tick: the car comes down by the door, stands, flies or falls.
    pub(super) fn getaway_tick(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let Some(mut job) = self.getaway.take() else {
            return;
        };
        if job.phase == Phase::Arriving {
            let solids = world.resource::<crate::Solids>().0.clone();
            let found = match job.search.as_mut() {
                Some(s) => s.step(&solids),
                None => Some(None),
            };
            if let Some(landing) = found {
                job.search = None;
                self.car_lands(world, tick, heroes.len(), &mut job, landing);
            }
        }
        if let Some(car) = job.car.as_mut() {
            car.fly(world, tick, heroes);
        }
        self.getaway = Some(job);
    }

    /// The car stands by the door (where the search found room, else out
    /// in front of it), the lookouts by it, and the bagmen start.
    fn car_lands(
        &mut self,
        world: &mut World,
        tick: u64,
        heroes: usize,
        job: &mut Job,
        landing: Option<Landing>,
    ) {
        let solids = world.resource::<crate::Solids>().0.clone();
        let out3 = DVec3::new(job.out.x, 0.0, job.out.y);
        let (pos, yaw) = match landing {
            Some(l) => (l.at, l.yaw),
            None => {
                let side = job.out.perp();
                let p = open_near(&solids, job.door + out3 * 9.0).unwrap_or(job.door + out3 * 9.0);
                (p, hw_core::det::atan2(side.x, side.y))
            }
        };
        job.car = Car::spawn(
            world,
            tick,
            heroes,
            self.level(world),
            pos,
            yaw,
            CarState::Parked,
            self.dice(),
        );
        let Some(car) = job.car.as_ref() else {
            job.phase = Phase::Down;
            return;
        };
        // The side of the car the door is on.
        let right = if (flat2(job.door) - flat2(car.pos)).dot(dir_of(car.yaw).perp()) < 0.0 {
            1.0
        } else {
            -1.0
        };
        let by_car = car.beside(right, 1.6);
        let lookouts = open_near(&solids, car.beside(-right, 2.2)).or(open_near(&solids, by_car));
        self.door = lookouts.map(|at| {
            let away = (flat2(at) - flat2(job.door)).normalize_or(job.out);
            self.group(world, tick, heroes, at, away, DOOR_SIZE, None, false)
        });
        let crew = self.group(world, tick, heroes, job.door, job.out, BAGMEN, None, false);
        let mut ids: Vec<NetId> = world
            .query::<(&Net, &combat::Enemy)>()
            .iter(world)
            .filter(|(_, en)| en.spot == crew.spot)
            .map(|(n, _)| n.0)
            .collect();
        ids.sort();
        job.bagmen = ids
            .into_iter()
            .map(|id| Bagman {
                id,
                loaded: false,
                until: Some(tick + ticks(DWELL)),
            })
            .collect();
        self.crew = Some(crew);
        job.phase = Phase::Loading;
    }

    /// Once a second: the bagmen's errands, the run for the car, the crew
    /// that gets out of a car brought down.
    pub(super) fn getaway_second(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let Some(mut job) = self.getaway.take() else {
            return;
        };
        let wrecked = job
            .car
            .as_ref()
            .is_some_and(|c| c.state == CarState::Wrecked);
        match job.phase {
            Phase::Arriving | Phase::Down => {}
            Phase::Loading if wrecked => {
                // No car, no getaway: the bagmen fight like the rest.
                self.stand_and_fight(world, tick);
                job.phase = Phase::Down;
            }
            Phase::Loading => {
                self.carry(world, tick, &mut job);
                if job.loaded >= TRIPS {
                    let until = tick + ticks(BOARD_SECONDS);
                    self.run_for_the_car(world, &job, until);
                    job.phase = Phase::Boarding(until);
                    self.alarms = self.alarms.wrapping_add(1);
                }
            }
            Phase::Boarding(_) if wrecked => {
                self.stand_and_fight(world, tick);
                job.phase = Phase::Down;
            }
            Phase::Boarding(until) => {
                let waiting = self.climb_in(world, &mut job);
                if tick >= until || waiting == 0 {
                    self.stand_and_fight(world, tick);
                    if let Some(car) = job.car.as_mut() {
                        car.take_off(tick);
                    }
                    job.flew = true;
                    job.phase = Phase::Flying;
                }
            }
            Phase::Flying if wrecked => {
                // Down: its crew gets out beside it.
                let extra = heroes.len().saturating_sub(1) as u32;
                let size = (1 + job.aboard).clamp(CREW_SIZE, CREW_SIZE + 1 + extra);
                if let Some(car) = job.car.as_ref() {
                    let solids = world.resource::<crate::Solids>().0.clone();
                    let at = [1.0, -1.0]
                        .into_iter()
                        .find_map(|r| open_near(&solids, car.beside(r, 2.4)))
                        .unwrap_or(car.beside(1.0, 2.4));
                    let facing = heroes
                        .iter()
                        .min_by(|a, b| a.at.distance(at).total_cmp(&b.at.distance(at)))
                        .map_or(dir_of(car.yaw), |h| {
                            (flat2(h.at) - flat2(at)).normalize_or(DVec2::X)
                        });
                    // The stragglers of a heist are no part of this any more.
                    self.rooms.clear();
                    self.door = None;
                    self.crew =
                        Some(self.group(world, tick, heroes.len(), at, facing, size, None, false));
                }
                job.phase = Phase::Down;
            }
            Phase::Flying => {}
        }
        self.getaway = Some(job);
    }

    /// The bagmen go to and fro: a sack from the door, to the car, back.
    /// They mind nobody while they are at it.
    fn carry(&mut self, world: &mut World, tick: u64, job: &mut Job) {
        let Some(car) = job.car.as_ref() else {
            return;
        };
        let right = if (flat2(job.door) - flat2(car.pos)).dot(dir_of(car.yaw).perp()) < 0.0 {
            1.0
        } else {
            -1.0
        };
        let by_car = car.beside(right, 0.9);
        let door = job.door;
        let mut fx = Vec::new();
        let mut loaded = 0;
        for (k, bag) in job.bagmen.iter_mut().enumerate() {
            let found = world
                .query::<(&Net, &mut combat::Enemy, &Motion, &Fighter)>()
                .iter_mut(world)
                .find(|(n, ..)| n.0 == bag.id)
                .map(|(_, en, m, f)| (en, m.0.pos, f.standing()));
            let Some((mut en, pos, standing)) = found else {
                continue;
            };
            if !standing {
                continue;
            }
            // Each to their own place at the door and at the car.
            let aside = DVec3::new(job.out.y, 0.0, -job.out.x) * (k as f64 * 1.4 - 0.7);
            let fwd = dir_of(car.yaw) * (k as f64 * 1.6 - 0.8);
            let goal = if bag.loaded {
                by_car + DVec3::new(fwd.x, 0.0, fwd.y)
            } else {
                door + aside
            };
            en.home = goal;
            en.leaving = true;
            // Head down, hauling: they fight nobody.
            en.flee_until = tick + ticks(5.0);
            let there = flat2(pos).distance(flat2(goal)) < AT_HAND;
            match (there, bag.until) {
                (true, None) => bag.until = Some(tick + ticks(DWELL)),
                (true, Some(t)) if tick >= t => {
                    bag.until = None;
                    let mut l = combat::event(tick, hw_net::FxKind::Loot, bag.id, bag.id, pos);
                    if bag.loaded {
                        loaded += 1;
                        l.amount = hw_net::protocol::loot::NONE;
                    } else {
                        l.amount = hw_net::protocol::loot::SACK;
                    }
                    bag.loaded = !bag.loaded;
                    fx.push(l);
                }
                _ => {}
            }
        }
        job.loaded = (job.loaded + loaded).min(TRIPS);
        world.resource_mut::<Fights>().events.extend(fx);
    }

    /// The fight spots of everyone at a heist: guards, lookouts, bagmen.
    fn heist_spots(&self) -> Vec<usize> {
        self.rooms
            .iter()
            .chain(self.door.iter())
            .chain(self.crew.iter())
            .map(|r| r.spot)
            .collect()
    }

    /// The last sack is aboard: everyone on their feet runs for the car
    /// and fights nobody until `until`.
    fn run_for_the_car(&mut self, world: &mut World, job: &Job, until: u64) {
        let Some(car) = job.car.as_ref() else {
            return;
        };
        let spots = self.heist_spots();
        let sides = [car.beside(1.0, 0.8), car.beside(-1.0, 0.8)];
        let mut k = 0;
        let mut all: Vec<(NetId, Entity)> = world
            .query::<(Entity, &Net, &combat::Enemy, &Fighter)>()
            .iter(world)
            .filter(|(_, _, en, f)| spots.contains(&en.spot) && f.standing())
            .map(|(e, n, ..)| (n.0, e))
            .collect();
        all.sort_by_key(|a| a.0);
        for (_, e) in all {
            if let Some(mut en) = world.get_mut::<combat::Enemy>(e) {
                en.home = sides[k % 2];
                en.leaving = true;
                en.flee_until = until;
                en.windup = None;
                k += 1;
            }
            if let Some(mut f) = world.get_mut::<Fighter>(e) {
                f.target = None;
            }
        }
    }

    /// Whoever of the gang has reached the car is in it (gone from the
    /// street). How many are still on their way.
    fn climb_in(&mut self, world: &mut World, job: &mut Job) -> usize {
        let Some(car) = job.car.as_ref() else {
            return 0;
        };
        let spots = self.heist_spots();
        let mut all: Vec<(NetId, Entity, usize, DVec3)> = world
            .query::<(Entity, &Net, &combat::Enemy, &Motion, &Fighter)>()
            .iter(world)
            .filter(|(_, _, en, _, f)| spots.contains(&en.spot) && f.standing())
            .map(|(e, n, en, m, _)| (n.0, e, en.spot, m.0.pos))
            .collect();
        all.sort_by_key(|a| a.0);
        let mut waiting = 0;
        for (id, e, spot, pos) in all {
            if flat2(pos).distance(flat2(car.pos)) < BOARD_REACH {
                let mut f = world.resource_mut::<Fights>();
                f.paths.remove(&id);
                f.spots[spot].members = f.spots[spot].members.saturating_sub(1);
                world.despawn(e);
                job.aboard += 1;
            } else {
                waiting += 1;
            }
        }
        waiting
    }

    /// Nobody runs or hauls any more: whoever stands, fights where they are.
    fn stand_and_fight(&mut self, world: &mut World, tick: u64) {
        let spots = self.heist_spots();
        for (mut en, m) in world
            .query::<(&mut combat::Enemy, &Motion)>()
            .iter_mut(world)
        {
            if spots.contains(&en.spot) && (en.leaving || en.flee_until > tick) {
                en.leaving = false;
                en.flee_until = 0;
                en.home = m.0.pos;
            }
        }
    }

    /// Is a heist or a chase done: nobody of it left standing, and the
    /// car not in the air?
    pub(super) fn getaway_done(&self, world: &mut World) -> bool {
        let Some(job) = self.getaway.as_ref() else {
            return false;
        };
        let groups = || {
            self.rooms
                .iter()
                .chain(self.door.iter())
                .chain(self.crew.iter())
        };
        match job.phase {
            Phase::Arriving | Phase::Boarding(_) | Phase::Flying => false,
            // The bagmen beaten, the rest too: caught in the act.
            Phase::Loading | Phase::Down => {
                groups().count() > 0 && groups().all(|r| subdued(world, r.spot))
            }
        }
    }

    /// Has the car got away (it flew too long)?
    pub(super) fn getaway_escaped(&self, tick: u64) -> bool {
        self.getaway
            .as_ref()
            .and_then(|j| j.car.as_ref())
            .is_some_and(|c| c.escaped(tick))
    }

    /// Is the car in the air (then heroes far from it lose it)?
    pub(super) fn getaway_flying(&self) -> bool {
        self.getaway
            .as_ref()
            .is_some_and(|j| matches!(j.phase, Phase::Flying))
    }

    /// The car, while this case has one.
    pub(crate) fn car(&self) -> Option<&Car> {
        self.getaway.as_ref().and_then(|j| j.car.as_ref())
    }

    /// Where a heist or a chase stands, if this is one.
    pub fn getaway_phase(&self) -> Option<Phase> {
        self.getaway.as_ref().map(|j| j.phase)
    }

    /// The car's id, where it is and what it does (tests and tools).
    pub fn getaway_car(&self) -> Option<(NetId, DVec3, CarState)> {
        self.car().map(|c| (c.id, c.pos, c.state))
    }

    /// Sacks in the car so far, of [`TRIPS`].
    pub fn loot_loaded(&self) -> u32 {
        self.getaway.as_ref().map_or(0, |j| j.loaded)
    }

    /// The bagmen's fight spot at a heist, or the crew's out of a car
    /// brought down.
    pub fn crew_spot(&self) -> Option<usize> {
        self.crew.as_ref().map(|r| r.spot)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_getaway_car_is_as_solid_as_a_parked_hover_car() {
        let (w, l, h) = hw_gen::urban::props::car_block(hw_gen::urban::props::CarModel::Hover);
        assert_eq!(
            (super::CAR_HALF.x, super::CAR_HALF.y, super::CAR_HEIGHT),
            (w, l, h)
        );
    }
}
