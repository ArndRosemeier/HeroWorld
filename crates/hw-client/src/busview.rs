//! People at the bus stops: a few townsfolk wait under each shelter near
//! the camera, more stroll up along the pavement now and then, and when a
//! bus pulls in (see `hw_sim::traffic`, buses stand [`DWELL`] seconds at a
//! stop) some passengers step down from its middle door and walk off along
//! the pavement while those waiting climb in at the front.
//!
//! Like the riders in the cars (`riderview`) they are agents of the
//! citizens' crowds, one crowd per look, so they cost no models of their
//! own. They are only a show for the camera: nothing of it is on the
//! server, and who waits where follows from the stop and the time.

use crate::crowdview::CrowdView;
use engine::prelude::*;
use glam::{DQuat, DVec2, DVec3};
use hw_core::Seed;
use hw_sim::citizens::{Citizens, Escape, LOOKS};
use hw_sim::traffic::{BusStop, Traffic, Vehicle, VehicleKind, BUS_WIDTH, DWELL};
use std::collections::{BTreeMap, BTreeSet};

/// People wait at stops within this distance of the camera.
const STOP_RADIUS: f64 = 90.0;
/// ...and are let go when the camera is further than this.
const STOP_GONE: f64 = 115.0;
/// At most this many waiting at one stop.
const MAX_WAITING: usize = 5;
/// Walking pace (m/s).
const WALK: f64 = 1.35;
/// Someone strolls up to a stop this often (s), on average.
const ARRIVALS: f64 = 35.0;
/// How far off they come from (m along the pavement).
const COME_FROM: f64 = 28.0;
/// Those who got off walk this far before they stop (m)...
const WALK_OFF: f64 = 45.0;
/// ...and are let go this far from the camera, or after this long.
const WALKER_GONE: f64 = 130.0;
const WALKER_LIFE: f64 = 90.0;
/// At most this many walking to or from buses.
const MAX_WALKERS: usize = 48;
/// When, after the bus stops, the doors are open (s).
const DOORS_OPEN: f64 = 1.6;

/// Where people wait at a shelter, in its frame (x along it, z out to the
/// road): under the roof in front of the bench, and beside it.
const SLOTS: [(f64, f64); MAX_WAITING] = [
    (0.1, 0.35),
    (-1.0, 0.3),
    (1.15, 0.25),
    (2.35, 0.9),
    (0.6, 1.3),
];

/// Someone at a stop.
struct Waiter {
    look: u32,
    agent: CrowdAgentId,
    slot: usize,
    /// Walking up along the pavement: the way (walked backwards, from its
    /// end to the slot) and how far along.
    coming: Option<(Escape, f64)>,
}

/// One stop near the camera.
struct Stop {
    waiting: Vec<Waiter>,
    /// When the next person comes along (game time).
    next: f64,
    /// How many have come so far (for their looks).
    came: u64,
}

/// What someone walking to or from a bus does.
enum Goal {
    /// To the bus's door, then in.
    Board { to: DVec3 },
    /// Down from the door to the pavement, then off along it.
    Alight {
        from: DVec3,
        to: DVec3,
        away: Option<Escape>,
    },
}

struct Walker {
    look: u32,
    agent: CrowdAgentId,
    at: DVec3,
    /// Seconds until it sets off (they go one after the other).
    wait: f64,
    goal: Goal,
    t: f64,
}

#[derive(Default)]
pub struct BusView {
    stops: BTreeMap<u32, Stop>,
    walkers: Vec<Walker>,
    /// Buses standing at stops now: (bus, stop), handled once each stay.
    visits: BTreeSet<(u64, u32)>,
}

impl BusView {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many wait at stops, and how many walk to or from buses.
    pub fn counts(&self) -> (usize, usize) {
        let waiting = self.stops.values().map(|s| s.waiting.len()).sum();
        (waiting, self.walkers.len())
    }

    /// Takes everyone away (another town or square comes).
    pub fn clear(&mut self, world: &mut World, crowd: &CrowdView) {
        for (_, s) in std::mem::take(&mut self.stops) {
            for w in s.waiting {
                remove(world, crowd, w.look, w.agent);
            }
        }
        for w in std::mem::take(&mut self.walkers) {
            remove(world, crowd, w.look, w.agent);
        }
        self.visits.clear();
    }

    /// Fills the stops near `eye`, lets the buses standing at them (among
    /// `cars`, the vehicles drawn near the camera) take people on and let
    /// people off, and moves everyone on by `dt`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        crowd: &CrowdView,
        traffic: &Traffic,
        citizens: &Citizens,
        cars: &[Vehicle],
        eye: DVec2,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        let stops = traffic.bus_stops();
        // Stops that come near get their waiting people; far ones let them go.
        let gone: Vec<u32> = (self.stops.keys())
            .filter(|&&n| {
                stops
                    .get(n as usize)
                    .is_none_or(|s| s.pos.distance(eye) > STOP_GONE)
            })
            .copied()
            .collect();
        for n in gone {
            if let Some(s) = self.stops.remove(&n) {
                for w in s.waiting {
                    remove(world, crowd, w.look, w.agent);
                }
            }
        }
        for (n, stop) in stops.iter().enumerate() {
            let n = n as u32;
            if self.stops.contains_key(&n) || stop.pos.distance(eye) > STOP_RADIUS {
                continue;
            }
            // Already waiting when the camera comes: from none to three.
            let seed = Seed(n as u64)
                .child("bus stop")
                .child((time / 600.0) as u64);
            let count = (seed.child("n").unit() * 3.6) as usize;
            let mut s = Stop {
                waiting: Vec::new(),
                next: time + ARRIVALS * (0.3 + seed.child("next").unit()),
                came: seed.0 & 0xFFFF,
            };
            for slot in 0..count.min(MAX_WAITING) {
                s.came += 1;
                let look = look_of(n, s.came);
                let Some(id) = crowd.crowd_of(look) else {
                    continue;
                };
                let agent = world.add_crowd_agent(id, slot_place(stop, slot, n))?;
                s.waiting.push(Waiter {
                    look,
                    agent,
                    slot,
                    coming: None,
                });
            }
            self.stops.insert(n, s);
        }
        // Buses at stops: the doors open, people get off and on.
        let mut now = BTreeSet::new();
        for v in cars {
            if !matches!(v.kind, VehicleKind::Bus(_)) {
                continue;
            }
            let Some(at) = traffic.bus_at_stop(v.id) else {
                continue;
            };
            now.insert((v.id, at.stop));
            if at.standing < DOORS_OPEN || self.visits.contains(&(v.id, at.stop)) {
                continue;
            }
            self.visits.insert((v.id, at.stop));
            if v.pos.distance(DVec3::new(eye.x, v.pos.y, eye.y)) > STOP_RADIUS {
                continue;
            }
            let Some(stop) = stops.get(at.stop as usize).copied() else {
                continue;
            };
            self.call_at(world, crowd, citizens, v, &stop, at.stop, at.standing)?;
        }
        self.visits.retain(|k| now.contains(k));
        // Now and then someone strolls up.
        for (&n, s) in self.stops.iter_mut() {
            if time < s.next {
                continue;
            }
            let seed = Seed(n as u64).child("arrival").child(s.came);
            s.next = time + ARRIVALS * (0.4 + 1.2 * seed.child("gap").unit());
            let stop = stops[n as usize];
            let Some(slot) = (0..MAX_WAITING).find(|k| s.waiting.iter().all(|w| w.slot != *k))
            else {
                continue;
            };
            s.came += 1;
            let look = look_of(n, s.came);
            let Some(id) = crowd.crowd_of(look) else {
                continue;
            };
            // Along the pavement from one side or the other.
            let along = DVec2::new(stop.dir.y, -stop.dir.x);
            let side = if seed.child("side").unit() < 0.5 {
                1.0
            } else {
                -1.0
            };
            let spot = to_world(&stop, SLOTS[slot]);
            let Some(way) = citizens.escape(spot, spot - along * side * 10.0, COME_FROM) else {
                continue;
            };
            let (p, _) = citizens.escape_at(&way, way.len());
            let agent =
                world.add_crowd_agent(id, Place::new(p.x as f32, p.y as f32, p.z as f32))?;
            s.waiting.push(Waiter {
                look,
                agent,
                slot,
                coming: Some((way, 0.0)),
            });
        }
        // Everyone at the stops: walking up, or waiting.
        for (&n, s) in self.stops.iter_mut() {
            let stop = stops[n as usize];
            for w in &mut s.waiting {
                let Some(id) = crowd.crowd_of(w.look) else {
                    continue;
                };
                match &mut w.coming {
                    Some((way, t)) => {
                        *t += dt * WALK;
                        let left = way.len() - *t;
                        if left <= 0.0 {
                            w.coming = None;
                            world.set_crowd_agent(
                                id,
                                w.agent,
                                slot_place(&stop, w.slot, n),
                                0.0,
                            )?;
                            continue;
                        }
                        let (p, d) = citizens.escape_at(way, left);
                        let place = Place::new(p.x as f32, p.y as f32, p.z as f32)
                            .with_yaw_deg(d_yaw(-d) as f32);
                        world.set_crowd_agent(id, w.agent, place, WALK as f32)?;
                    }
                    None => {
                        world.set_crowd_agent(id, w.agent, slot_place(&stop, w.slot, n), 0.0)?;
                    }
                }
            }
        }
        self.walk(world, crowd, citizens, eye, dt)
    }

    /// Bus `v` stands at stop `n` with its doors open: who rides on gets
    /// off at the middle door, those waiting get on at the front.
    #[allow(clippy::too_many_arguments)]
    fn call_at(
        &mut self,
        world: &mut World,
        crowd: &CrowdView,
        citizens: &Citizens,
        v: &Vehicle,
        stop: &BusStop,
        n: u32,
        standing: f64,
    ) -> EngineResult<()> {
        let (yaw, _, _) = crate::trafficview::angles(v);
        let q = DQuat::from_rotation_y(yaw);
        let ground = stop.y;
        let at = |x: f64, z: f64| {
            let p = v.pos + q * DVec3::new(x, 0.0, z);
            DVec3::new(p.x, ground, p.z)
        };
        let [front, middle] = crate::carmesh::BUS_DOORS;
        let outside = -(BUS_WIDTH * 0.5 + 0.45);
        // Time left before the doors shut, for those getting on.
        let left = DWELL - 1.4 - standing;
        // Getting off: none to three, one after the other.
        let seed = Seed(v.id).child("alight").child(n as u64);
        let off = (seed.child("n").unit() * 3.7) as usize;
        for k in 0..off {
            if self.walkers.len() >= MAX_WALKERS {
                break;
            }
            let look = (seed.child(k as u64).0 % LOOKS as u64) as u32;
            let Some(id) = crowd.crowd_of(look) else {
                continue;
            };
            let from = at(-0.75, middle.0 + 0.25 * (k as f64 - 1.0));
            let from = DVec3::new(from.x, v.pos.y + crate::carmesh::BUS_FLOOR - 0.2, from.z);
            let to = at(outside - 0.4, middle.0 + 0.4 * (k as f64 - 1.0));
            let away_dir = q
                * DVec3::Z
                * if seed.child("way").child(k as u64).unit() < 0.5 {
                    1.0
                } else {
                    -1.0
                };
            let danger = DVec2::new(to.x - away_dir.x * 8.0, to.z - away_dir.z * 8.0);
            let away = citizens.escape(DVec2::new(to.x, to.z), danger, WALK_OFF);
            let agent = world.add_crowd_agent(id, place_at(from, yaw.to_degrees()))?;
            self.walkers.push(Walker {
                look,
                agent,
                at: from,
                wait: k as f64 * 1.3,
                goal: Goal::Alight { from, to, away },
                t: 0.0,
            });
        }
        // Getting on: everyone waiting who can make it, after those
        // getting off.
        let door = at(outside, front.0);
        let Some(s) = self.stops.get_mut(&n) else {
            return Ok(());
        };
        let mut waiting = std::mem::take(&mut s.waiting);
        waiting.sort_by(|a, b| {
            let pa = to_world(stop, SLOTS[a.slot]).distance(DVec2::new(door.x, door.z));
            let pb = to_world(stop, SLOTS[b.slot]).distance(DVec2::new(door.x, door.z));
            pa.total_cmp(&pb)
        });
        let mut stay = Vec::new();
        for (k, w) in waiting.into_iter().enumerate() {
            let start = 1.0 + off as f64 * 0.8 + k as f64 * 1.4;
            let spot = to_world(stop, SLOTS[w.slot]);
            let walk = spot.distance(DVec2::new(door.x, door.z)) / WALK;
            if w.coming.is_some() || start + walk > left || self.walkers.len() >= MAX_WALKERS {
                stay.push(w);
                continue;
            }
            self.walkers.push(Walker {
                look: w.look,
                agent: w.agent,
                at: DVec3::new(spot.x, ground, spot.y),
                wait: start,
                goal: Goal::Board { to: door },
                t: 0.0,
            });
        }
        if let Some(s) = self.stops.get_mut(&n) {
            s.waiting = stay;
        }
        Ok(())
    }

    /// Moves those walking to and from buses on.
    fn walk(
        &mut self,
        world: &mut World,
        crowd: &CrowdView,
        citizens: &Citizens,
        eye: DVec2,
        dt: f64,
    ) -> EngineResult<()> {
        let mut keep = Vec::with_capacity(self.walkers.len());
        for mut w in std::mem::take(&mut self.walkers) {
            let Some(id) = crowd.crowd_of(w.look) else {
                continue;
            };
            if w.wait > 0.0 {
                w.wait -= dt;
                keep.push(w);
                continue;
            }
            w.t += dt;
            let (place, speed, done) = match &w.goal {
                Goal::Board { to } => {
                    let d = DVec2::new(to.x - w.at.x, to.z - w.at.z);
                    let step = WALK * dt;
                    if d.length() <= step {
                        // In at the door.
                        (place_at(*to, d_yaw(d)), 0.0, true)
                    } else {
                        let dir = d.normalize();
                        w.at += DVec3::new(dir.x, 0.0, dir.y) * step;
                        (place_at(w.at, d_yaw(dir)), WALK, false)
                    }
                }
                Goal::Alight { from, to, away } => {
                    // Down the steps, then off along the pavement.
                    const STEP_DOWN: f64 = 1.3;
                    if w.t < STEP_DOWN {
                        let e = w.t / STEP_DOWN;
                        let p = from.lerp(*to, e);
                        let d = DVec2::new(to.x - from.x, to.z - from.z);
                        (place_at(p, d_yaw(d)), WALK * 0.7, false)
                    } else {
                        let gone = (w.t - STEP_DOWN) * WALK;
                        match away {
                            Some(e) if gone < e.len() => {
                                let (p, d) = citizens.escape_at(e, gone);
                                (place_at(p, d_yaw(d)), WALK, false)
                            }
                            Some(e) => {
                                let (p, d) = citizens.escape_at(e, e.len());
                                (place_at(p, d_yaw(d)), 0.0, w.t > WALKER_LIFE)
                            }
                            None => (place_at(*to, 0.0), 0.0, w.t > WALKER_LIFE * 0.3),
                        }
                    }
                }
            };
            let far = DVec2::new(place.position.x as f64, place.position.z as f64).distance(eye);
            if done || far > WALKER_GONE {
                remove(world, crowd, w.look, w.agent);
                continue;
            }
            world.set_crowd_agent(id, w.agent, place, speed as f32)?;
            keep.push(w);
        }
        self.walkers = keep;
        Ok(())
    }
}

/// The look of the `k`th person to come to stop `n`.
fn look_of(n: u32, k: u64) -> u32 {
    (Seed(n as u64).child("look").child(k).0 % LOOKS as u64) as u32
}

/// A point in a shelter's frame, on the town's plan.
fn to_world(stop: &BusStop, (x, z): (f64, f64)) -> DVec2 {
    let across = DVec2::new(stop.dir.y, -stop.dir.x);
    stop.pos + across * x + stop.dir * z
}

/// Where someone waiting in slot `slot` of stop `n` stands: facing the
/// road, each turned a little their own way.
fn slot_place(stop: &BusStop, slot: usize, n: u32) -> Place {
    let p = to_world(stop, SLOTS[slot]);
    let turn = (Seed(n as u64).child("turn").child(slot as u64).unit() - 0.5) * 50.0;
    Place::new(p.x as f32, stop.y as f32, p.y as f32).with_yaw_deg((d_yaw(stop.dir) + turn) as f32)
}

fn place_at(p: DVec3, yaw: f64) -> Place {
    Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw as f32)
}

/// The yaw (degrees) that faces `dir` (x, z).
fn d_yaw(dir: DVec2) -> f64 {
    hw_core::det::atan2(dir.x, dir.y).to_degrees()
}

fn remove(world: &mut World, crowd: &CrowdView, look: u32, agent: CrowdAgentId) {
    if let Some(id) = crowd.crowd_of(look) {
        world.remove_crowd_agent(id, agent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_slots_keep_clear_of_the_shelters_solid_parts() {
        use hw_gen::urban::props::{Prop, PropKind};
        let prop = Prop {
            kind: PropKind::BusStop,
            pos: DVec2::new(10.0, 20.0),
            y: 0.0,
            dir: DVec2::new(0.6, 0.8),
            scale: 1.0,
        };
        let stop = BusStop {
            pos: prop.pos,
            dir: prop.dir,
            y: 0.0,
        };
        let solid: Vec<_> = prop
            .footprints()
            .into_iter()
            .filter(|f| f.span().0 < 1.0)
            .collect();
        for slot in SLOTS {
            let p = to_world(&stop, slot);
            for f in &solid {
                assert!(f.distance(p) > 0.25, "slot {slot:?} in the shelter");
            }
        }
    }
}
