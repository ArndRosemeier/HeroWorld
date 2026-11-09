//! Rides on the Aether Tube.
//!
//! A hero in a tube station (in its hall or on the walkway by it) picks
//! another station on the network. A capsule glides in along the tube and
//! stops at the platform; the hero gets in, and it runs the way through
//! the tubes ([`TubeNet::path`]), speeding up and slowing down, across
//! junctions, to the other station. There the hero steps out into the hall
//! and the capsule glides on and away.
//!
//! The capsule is a replicated entity ([`hw_net::EntityKind::Capsule`])
//! whose `owner` is the passenger; a riding hero carries
//! [`crate::taxi::Riding`], so the capsule moves it rather than its input.

use crate::{LastChange, Motion, Net, Orientation};
use bevy_ecs::prelude::*;
use glam::{DVec2, DVec3};
use hw_core::NetId;
use hw_gen::urban::deep::TubeNet;
use hw_gen::urban::world::TownWorld;
use hw_net::PlayerId;
use std::sync::Arc;

/// The ride asks for no station: get out now (or send the capsule away).
pub const HOP_OUT: u16 = u16::MAX;
/// Where a riding hero sits, relative to the capsule's floor point.
pub const SEAT: DVec3 = DVec3::new(0.0, 0.25, 0.0);
/// Top speed along the tubes (m/s), and how fast it gets there (m/s²).
const TOP: f64 = 32.0;
const ACCEL: f64 = 6.0;
/// The capsule glides in and out over this distance (m).
const RUN_IN: f64 = 45.0;
/// Seconds the door takes to slide.
pub const DOOR: f64 = 1.0;
/// A hero this near a station's platform, or in its hall, can ride.
const NEAR: f64 = 34.0;

/// Who rides a capsule (replicated as its `owner`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passenger(pub PlayerId);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Gliding in to the platform.
    Coming,
    /// At the platform, door open; the hero gets in.
    Boarding,
    /// Running through the tubes.
    Riding,
    /// At the far platform, door open; the hero steps out.
    Alighting,
    /// Gliding on, empty.
    Leaving,
}

struct Ride {
    owner: PlayerId,
    capsule: Entity,
    from: usize,
    dest: usize,
    stage: Stage,
    /// The way being run now, and its length so far at each point.
    way: Vec<DVec3>,
    lengths: Vec<f64>,
    /// Seconds into the current stage.
    t: f64,
    /// The way the capsule last pointed.
    heading: DVec2,
}

/// The tube network and the capsules running on it.
#[derive(Default)]
pub struct Tubes {
    town: Option<Arc<TownWorld>>,
    rides: Vec<Ride>,
}

impl Tubes {
    pub fn new(town: Arc<TownWorld>) -> Self {
        Self {
            town: Some(town),
            rides: Vec::new(),
        }
    }

    fn net(&self) -> Option<&TubeNet> {
        self.town.as_deref().map(TownWorld::tube)
    }

    /// The station a hero at `p` can ride from, if any.
    pub fn station_at(&self, p: DVec3) -> Option<usize> {
        station_at(self.net()?, p)
    }

    /// Is `owner` riding, or waiting for, a capsule?
    pub fn has_ride(&self, owner: PlayerId) -> bool {
        self.rides
            .iter()
            .any(|r| r.owner == owner && r.stage != Stage::Leaving)
    }

    /// The station `owner`'s capsule is going to.
    pub fn destination(&self, owner: PlayerId) -> Option<usize> {
        self.rides
            .iter()
            .find(|r| r.owner == owner && r.stage != Stage::Leaving)
            .map(|r| r.dest)
    }
}

/// The station a hero at `p` can ride from: near its platform or in its
/// hall, and about level with it.
pub fn station_at(net: &TubeNet, p: DVec3) -> Option<usize> {
    let q = DVec2::new(p.x, p.z);
    net.stations
        .iter()
        .enumerate()
        .filter(|(_, s)| (p.y - s.floor).abs() < 4.0)
        .filter(|(_, s)| s.pos.distance(q) < NEAR || s.hall().covers(q, 0.5))
        .min_by(|a, b| a.1.pos.distance(q).total_cmp(&b.1.pos.distance(q)))
        .map(|(i, _)| i)
}

/// The stations reachable from station `s`, nearest first.
pub fn reachable(net: &TubeNet, s: usize) -> Vec<usize> {
    let at = net.stations[s].pos;
    let mut out: Vec<usize> = (0..net.stations.len())
        .filter(|&b| b != s && net.path(s, b).is_some())
        .collect();
    out.sort_by(|&a, &b| {
        net.stations[a]
            .pos
            .distance(at)
            .total_cmp(&net.stations[b].pos.distance(at))
    });
    out
}

/// A player asks for a capsule to station `dest`, or with [`HOP_OUT`]
/// gets out (or sends a coming capsule away). `id` is the new capsule's.
pub fn call(
    world: &mut World,
    tubes: &mut Tubes,
    id: NetId,
    owner: PlayerId,
    hero: Entity,
    dest: u16,
    tick: u64,
) {
    let Some(at) = world.get::<Motion>(hero).map(|m| m.0.pos) else {
        return;
    };
    if dest == HOP_OUT {
        let net = tubes.town.clone();
        let Some(r) = tubes
            .rides
            .iter_mut()
            .find(|r| r.owner == owner && r.stage != Stage::Leaving)
        else {
            return;
        };
        if matches!(r.stage, Stage::Riding | Stage::Alighting) {
            // Out where the capsule is: on the walkway, beside it.
            let pos = world.get::<Motion>(r.capsule).map_or(at, |m| m.0.pos);
            let out = match (r.stage, net.as_deref()) {
                (Stage::Alighting, Some(t)) => hall_spot(t.tube(), r.dest),
                _ => pos + DVec3::Y * 0.1,
            };
            step_out(world, hero, out);
        }
        leave(world, r, tick);
        return;
    }
    let Some(net) = tubes.net() else { return };
    let dest = dest as usize;
    if tubes.has_ride(owner) || dest >= net.stations.len() {
        return;
    }
    let Some(from) = station_at(net, at) else {
        return;
    };
    if from == dest {
        return;
    }
    let Some(path) = net.path(from, dest) else {
        return;
    };
    // In along the tube from behind the platform (away from the way out).
    let (platform, _) = net.platform(from);
    let out = path
        .iter()
        .find(|p| p.distance(platform) > 2.0)
        .map_or(DVec2::X, |p| {
            (DVec2::new(p.x, p.z) - flat(platform)).normalize_or(DVec2::X)
        });
    let behind = behind(net, from, -out);
    let way = vec![behind, platform];
    let yaw = yaw_of(out);
    let capsule = world
        .spawn((
            Net(id),
            Motion(crate::collision::Body {
                pos: behind,
                vel: DVec3::ZERO,
                grounded: false,
            }),
            Orientation(yaw_quat(yaw)),
            LastChange(tick),
            Passenger(owner),
        ))
        .id();
    let lengths = lengths(&way);
    tubes.rides.push(Ride {
        owner,
        capsule,
        from,
        dest,
        stage: Stage::Coming,
        way,
        lengths,
        t: 0.0,
        heading: out,
    });
}

/// One tick of every capsule.
pub fn step(world: &mut World, tubes: &mut Tubes, tick: u64, dt: f64) {
    let Some(town) = tubes.town.clone() else {
        return;
    };
    let net = town.tube();
    let heroes: Vec<(Entity, PlayerId, DVec3)> = world
        .query::<(Entity, &crate::Hero, &Motion)>()
        .iter(world)
        .map(|(e, h, m)| (e, h.owner, m.0.pos))
        .collect();
    let mut done = Vec::new();
    for (i, r) in tubes.rides.iter_mut().enumerate() {
        r.t += dt;
        let hero = heroes.iter().find(|h| h.1 == r.owner).copied();
        if hero.is_none() && r.stage != Stage::Leaving {
            leave(world, r, tick);
        }
        let total = *r.lengths.last().unwrap_or(&0.0);
        // Gliding in and out is slow; the run through the tubes is quick.
        let top = if r.stage == Stage::Riding { TOP } else { 9.0 };
        let s = travelled(r.t, total, top, ACCEL);
        let (pos, dir) = along(&r.way, &r.lengths, s);
        let s2 = travelled(r.t + dt, total, top, ACCEL);
        let (next, _) = along(&r.way, &r.lengths, s2);
        let vel = (next - pos) / dt;
        let door_open = matches!(r.stage, Stage::Boarding | Stage::Alighting);
        if let Ok(mut e) = world.get_entity_mut(r.capsule) {
            if let Some(mut m) = e.get_mut::<Motion>() {
                m.set_if_neq(Motion(crate::collision::Body {
                    pos,
                    vel,
                    grounded: door_open,
                }));
            }
            if dir.length_squared() > 0.0 {
                r.heading = dir;
                if let Some(mut o) = e.get_mut::<Orientation>() {
                    o.set_if_neq(Orientation(yaw_quat(yaw_of(dir))));
                }
            }
        }
        let arrived = s >= total - 1e-6;
        match r.stage {
            Stage::Coming if arrived => {
                r.stage = Stage::Boarding;
                r.way = vec![pos, pos];
                r.lengths = vec![0.0, 0.0];
                r.t = 0.0;
            }
            Stage::Boarding if r.t >= DOOR * 2.0 => {
                let Some((he, _, hp)) = hero else { continue };
                // Wandered off: the capsule goes without them.
                if station_at(net, hp) != Some(r.from) {
                    leave(world, r, tick);
                    continue;
                }
                let Some(path) = net.path(r.from, r.dest) else {
                    leave(world, r, tick);
                    continue;
                };
                r.lengths = lengths(&path);
                r.way = path;
                r.t = 0.0;
                r.stage = Stage::Riding;
                world.entity_mut(he).insert(crate::taxi::Riding);
                seat(world, he, pos, DVec3::ZERO);
            }
            Stage::Riding => {
                let Some((he, _, _)) = hero else { continue };
                seat(world, he, pos, vel);
                if arrived {
                    r.stage = Stage::Alighting;
                    r.way = vec![pos, pos];
                    r.lengths = vec![0.0, 0.0];
                    r.t = 0.0;
                }
            }
            Stage::Alighting => {
                let Some((he, _, _)) = hero else { continue };
                if r.t >= DOOR * 2.0 {
                    step_out(world, he, hall_spot(net, r.dest));
                    leave(world, r, tick);
                } else {
                    seat(world, he, pos, DVec3::ZERO);
                }
            }
            Stage::Leaving if arrived => done.push(i),
            _ => {}
        }
    }
    for i in done.into_iter().rev() {
        let r = tubes.rides.remove(i);
        world.despawn(r.capsule);
    }
}

/// Every capsule off the lines (on a world switch).
pub fn clear(world: &mut World, tubes: &mut Tubes) {
    for r in tubes.rides.drain(..) {
        world.despawn(r.capsule);
    }
}

/// Is this hero sitting in one of the capsules among `all`?
pub fn riding(hero: &hw_net::EntityState, all: &[hw_net::EntityState]) -> bool {
    let p = DVec3::from_array(hero.pos);
    all.iter().any(|t| {
        t.kind == hw_net::EntityKind::Capsule
            && t.owner == hero.owner
            && (DVec3::from_array(t.pos) + SEAT).distance(p) < 0.6
    })
}

/// Off it goes, on along the tube and out of sight.
fn leave(world: &mut World, r: &mut Ride, tick: u64) {
    let pos = world
        .get::<Motion>(r.capsule)
        .map_or(DVec3::ZERO, |m| m.0.pos);
    let dir = r.heading;
    let end = pos + DVec3::new(dir.x, 0.0, dir.y) * RUN_IN;
    r.way = vec![pos, end];
    r.lengths = lengths(&r.way);
    r.t = 0.0;
    r.stage = Stage::Leaving;
    if let Some(mut c) = world.get_mut::<LastChange>(r.capsule) {
        c.0 = tick;
    }
}

fn step_out(world: &mut World, hero: Entity, at: DVec3) {
    if let Ok(mut e) = world.get_entity_mut(hero) {
        e.remove::<crate::taxi::Riding>();
        if let Some(mut m) = e.get_mut::<Motion>() {
            m.0 = crate::collision::Body {
                pos: at + DVec3::Y * 0.05,
                vel: DVec3::ZERO,
                grounded: true,
            };
        }
    }
}

/// Where a hero steps out at station `s`: in the hall, between the gates.
fn hall_spot(net: &TubeNet, s: usize) -> DVec3 {
    let st = &net.stations[s];
    let c = st.hall_centre();
    DVec3::new(c.x, st.floor, c.y)
}

/// A point `RUN_IN` back along station `s`'s line, the way `dir` points.
fn behind(net: &TubeNet, s: usize, dir: DVec2) -> DVec3 {
    let st = &net.stations[s];
    let l = &net.lines[st.line];
    let (platform, _) = net.platform(s);
    let mut best = platform;
    let mut i = st.at as isize;
    let step = {
        let ahead = l.pts.get(st.at + 1).map(|p| (*p - st.pos).dot(dir) > 0.0);
        if ahead == Some(true) {
            1
        } else {
            -1
        }
    };
    while i >= 0 && (i as usize) < l.pts.len() {
        let p = l.pts[i as usize];
        best = DVec3::new(p.x, l.floors[i as usize], p.y);
        if p.distance(st.pos) >= RUN_IN {
            break;
        }
        i += step;
    }
    best
}

fn lengths(way: &[DVec3]) -> Vec<f64> {
    let mut out = Vec::with_capacity(way.len());
    let mut s = 0.0;
    for (i, p) in way.iter().enumerate() {
        if i > 0 {
            s += p.distance(way[i - 1]);
        }
        out.push(s);
    }
    out
}

/// Distance run after `t` seconds over a way `total` long: speeding up at
/// `accel` to `top`, cruising, and slowing to a stop at the end.
fn travelled(t: f64, total: f64, top: f64, accel: f64) -> f64 {
    let ramp = top / accel;
    let ramp_len = 0.5 * accel * ramp * ramp;
    let (peak, ramp, ramp_len) = if 2.0 * ramp_len > total {
        let v = (accel * total).sqrt();
        (v, v / accel, total * 0.5)
    } else {
        (top, ramp, ramp_len)
    };
    let cruise = (total - 2.0 * ramp_len) / peak;
    let t = t.max(0.0);
    if t < ramp {
        0.5 * accel * t * t
    } else if t < ramp + cruise {
        ramp_len + peak * (t - ramp)
    } else {
        let u = (t - ramp - cruise).min(ramp);
        (ramp_len + peak * cruise + peak * u - 0.5 * accel * u * u).min(total)
    }
}

/// The point `s` along the way, and the way's direction there.
fn along(way: &[DVec3], lengths: &[f64], s: f64) -> (DVec3, DVec2) {
    for i in 1..way.len() {
        if s <= lengths[i] || i + 1 == way.len() {
            let seg = lengths[i] - lengths[i - 1];
            let u = if seg > 1e-9 {
                ((s - lengths[i - 1]) / seg).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let d = flat(way[i] - way[i - 1]);
            let d = if d.length() > 1e-6 {
                d.normalize()
            } else {
                DVec2::ZERO
            };
            return (way[i - 1].lerp(way[i], u), d);
        }
    }
    (way.first().copied().unwrap_or(DVec3::ZERO), DVec2::ZERO)
}

fn seat(world: &mut World, hero: Entity, at: DVec3, vel: DVec3) {
    if let Some(mut m) = world.get_mut::<Motion>(hero) {
        m.set_if_neq(Motion(crate::collision::Body {
            pos: at + SEAT,
            vel,
            grounded: false,
        }));
    }
}

fn flat(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.z)
}

/// Yaw that turns the capsule's nose (+Z) to point along `d`.
fn yaw_of(d: DVec2) -> f64 {
    hw_core::det::atan2(d.x, d.y)
}

fn yaw_quat(yaw: f64) -> [f32; 4] {
    let (s, c) = hw_core::det::sin_cos(yaw * 0.5);
    [0.0, s as f32, 0.0, c as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_speeds_up_cruises_and_stops_at_the_end() {
        for total in [30.0, 400.0] {
            let mut last = 0.0;
            let mut t = 0.0;
            while t < 60.0 {
                let s = travelled(t, total, TOP, ACCEL);
                assert!(s >= last - 1e-9 && s <= total + 1e-9);
                assert!((s - last) / 0.05 <= TOP + 1e-6);
                last = s;
                t += 0.05;
            }
            assert!((last - total).abs() < 1e-6);
        }
    }
}
