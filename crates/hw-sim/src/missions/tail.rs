//! Tailing a suspect: a courier of the gang walks from a street corner to
//! the hideout, and whoever follows without being seen learns where it is.
//!
//! He walks along the pavements, and now and then he stops and looks
//! about. A hero he can see makes him suspicious, the nearer the faster
//! (and further off while he looks about); out of his sight, or well above
//! him on a roof or in the air, it wears off again. Suspicious enough (or
//! hit), he shouts and runs: caught and beaten he talks, and the case goes
//! on to the hideout all the same, but run far enough he is gone and the
//! case with him. Let too far ahead for too long, his trail is lost. If he
//! reaches the door unseen he goes in, and the hideout does not expect
//! anyone: nobody watches its door.

use super::running::{flat, Running};
use super::*;
use crate::{DT, TICK_HZ};
use glam::DVec2;

/// He sees a hero this near while walking (m).
pub const SEE: f64 = 26.0;
/// Nearer than this he all but walks into them (m).
pub const CLOSE: f64 = 9.0;
/// Looking about he sees this many times as far.
pub const LOOK_FACTOR: f64 = 1.6;
/// Seconds between two looks about, least and most.
pub const LOOK_EVERY: (f64, f64) = (9.0, 16.0);
/// Seconds a look about takes.
pub const LOOK_SECONDS: f64 = 2.5;
/// A hero this far above him is not seen (m): nobody looks up.
pub const ABOVE: f64 = 7.0;
/// Suspicion wearing off, per second.
pub const CALM_DOWN: f64 = 0.05;
/// With every hero further than this the trail goes cold (m) ...
pub const TRAIL: f64 = 75.0;
/// ... and after this many seconds it is lost.
pub const TRAIL_SECONDS: f64 = 20.0;
/// Seconds he runs once he has seen his tail; then he turns and fights.
pub const RUN_SECONDS: f64 = 25.0;
/// Running, he is away with every hero this far behind (m).
pub const AWAY_RUNNING: f64 = 90.0;
/// At a point of his way within this (m).
const AT_POINT: f64 = 2.5;

fn ticks(s: f64) -> u64 {
    (s * TICK_HZ as f64).round() as u64
}

/// How the walk is going.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Walk {
    /// On his way.
    Walking,
    /// Stopped, looking about, until this tick.
    Looking(u64),
    /// He saw his tail: running until this tick.
    Running(u64),
    /// Turned to fight (or beaten).
    Cornered,
    /// Gone in at the hideout's door, his tail unseen.
    Arrived,
    /// Away: the trail is lost.
    Lost,
}

/// A tail: the way, how far along, and how suspicious he is.
#[derive(Clone, Debug)]
pub struct Job {
    way: Vec<DVec3>,
    next: usize,
    pub walk: Walk,
    /// 0 easy, 1 he has seen them.
    pub suspicion: f64,
    look_at: u64,
    cold_since: Option<u64>,
    looks: u32,
    /// The courier and where he was a second ago.
    pub id: NetId,
    pub at: DVec3,
}

/// How fast a hero `d` metres off, in his sight, makes him suspicious
/// (per second), walking or looking about.
pub fn suspicion_rate(d: f64, looking: bool) -> f64 {
    let reach = SEE * if looking { LOOK_FACTOR } else { 1.0 };
    if d >= reach {
        return 0.0;
    }
    if d < CLOSE {
        return 0.6;
    }
    0.06 + 0.24 * (1.0 - (d - CLOSE) / (reach - CLOSE))
}

impl Running {
    /// The courier stands at the corner and sets off.
    pub(super) fn take_tail(&mut self, world: &mut World, tick: u64, heroes: usize) {
        let site = self.site().clone();
        let solids = world.resource::<crate::Solids>().0.clone();
        let from = super::running::open_near(&solids, site.at).unwrap_or(site.at);
        let room = self.group(
            world,
            tick,
            1.min(heroes),
            from,
            site.facing,
            1,
            None,
            false,
        );
        // To the next place's door (a hideout's front door).
        let goal = match self.plan.steps.get(self.step + 1) {
            Some(s) => match s.site.building {
                Some(k) => {
                    let b = &self.floors.buildings[k];
                    b.to_town(b.graph.front())
                }
                None => s.site.at,
            },
            None => site.around.first().copied().unwrap_or(site.at),
        };
        // His way was found with the case (the pavements from the corner
        // to the door); on a bare ground it is a straight line.
        let mut way: Vec<DVec3> = site
            .around
            .iter()
            .copied()
            .filter(|p| combat::room_to_stand(&solids, DVec2::new(p.x, p.z), p.y))
            .collect();
        way.push(goal);
        for mut en in world.query::<&mut combat::Enemy>().iter_mut(world) {
            if en.spot == room.spot {
                en.calm = true;
                en.errand_until = u64::MAX;
                en.home = way[0];
            }
        }
        self.rooms = vec![room];
        let wait = self.look_wait(0);
        self.tail = Some(Job {
            way,
            next: 0,
            walk: Walk::Walking,
            suspicion: 0.0,
            look_at: tick + ticks(wait),
            cold_since: None,
            looks: 0,
            id: NetId(0),
            at: from,
        });
    }

    /// Once a second: he walks on, looks about, grows suspicious, runs.
    pub(super) fn tail_second(&mut self, world: &mut World, tick: u64, heroes: &[Seen]) {
        let Some(mut job) = self.tail.take() else {
            return;
        };
        let Some(spot) = self.rooms.first().map(|r| r.spot) else {
            self.tail = Some(job);
            return;
        };
        let solids = world.resource::<crate::Solids>().0.clone();
        let mut men: Vec<(NetId, Entity, DVec3, bool, bool)> = world
            .query::<(Entity, &Net, &combat::Enemy, &Motion, &Fighter)>()
            .iter(world)
            .filter(|(_, _, en, _, _)| en.spot == spot)
            .map(|(e, n, _, m, f)| (n.0, e, m.0.pos, f.standing(), f.health < f.max_health - 0.5))
            .collect();
        men.sort_by_key(|m| m.0);
        let Some(&(id, _, pos, _, _)) = men.iter().find(|m| m.3) else {
            // Beaten: he talks.
            job.walk = Walk::Cornered;
            self.tail = Some(job);
            return;
        };
        job.id = id;
        job.at = pos;
        let nearest = heroes
            .iter()
            .map(|h| (flat(h.at, pos), h))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let far = nearest.as_ref().map_or(f64::MAX, |n| n.0);
        match job.walk {
            Walk::Walking | Walk::Looking(_) => {
                let looking = matches!(job.walk, Walk::Looking(_));
                // Who he can see, and what it does to him.
                let chest = pos + DVec3::Y * 1.3;
                let rate = heroes
                    .iter()
                    .filter(|h| h.at.y < pos.y + ABOVE)
                    .map(|h| (suspicion_rate(flat(h.at, pos), looking), h))
                    .filter(|(r, h)| {
                        *r > 0.0 && combat::in_sight(&solids, chest, h.at + DVec3::Y * 1.3)
                    })
                    .map(|(r, _)| r)
                    .fold(0.0, f64::max);
                job.suspicion = if rate > 0.0 {
                    job.suspicion + rate
                } else {
                    (job.suspicion - CALM_DOWN).max(0.0)
                };
                let hit = men.iter().any(|m| m.4);
                if job.suspicion >= 1.0 || hit {
                    job.suspicion = 1.0;
                    job.walk = Walk::Running(tick + ticks(RUN_SECONDS));
                    // Away from whoever it is, the way he came if nothing
                    // says otherwise.
                    let from = nearest.as_ref().map_or(pos - DVec3::X, |n| n.1.at);
                    let d = DVec2::new(pos.x - from.x, pos.z - from.z).normalize_or(DVec2::X);
                    let to = DVec2::new(pos.x, pos.z) + d * 160.0;
                    let to = DVec3::new(to.x, solids.ground_at(to).max(crate::GROUND_Y), to.y);
                    if let Some(n) = nearest.as_ref() {
                        let e = combat::event(tick, hw_net::FxKind::Warn, id, n.1.id, pos);
                        world.resource_mut::<Fights>().events.push(e);
                    }
                    self.set_course(world, spot, to, tick + ticks(RUN_SECONDS), false);
                } else {
                    // The trail: cold with everyone far behind.
                    if far > TRAIL {
                        let since = *job.cold_since.get_or_insert(tick);
                        if tick >= since + ticks(TRAIL_SECONDS) {
                            job.walk = Walk::Lost;
                        }
                    } else {
                        job.cold_since = None;
                    }
                    match job.walk {
                        Walk::Looking(until) if tick >= until => {
                            job.walk = Walk::Walking;
                            job.look_at = tick + ticks(self.look_wait(job.looks));
                        }
                        Walk::Walking if tick >= job.look_at => {
                            job.walk = Walk::Looking(tick + ticks(LOOK_SECONDS));
                            job.looks += 1;
                            self.set_course(world, spot, pos, 0, true);
                        }
                        _ => {}
                    }
                    if job.walk == Walk::Walking {
                        while job.next < job.way.len()
                            && flat(pos, job.way[job.next]) < AT_POINT
                            && (pos.y - job.way[job.next].y).abs() < 2.5
                        {
                            job.next += 1;
                        }
                        match job.way.get(job.next) {
                            Some(&to) => self.set_course(world, spot, to, 0, true),
                            None => {
                                // In at the door, none the wiser.
                                for m in &men {
                                    let mut f = world.resource_mut::<Fights>();
                                    f.paths.remove(&m.0);
                                    f.spots[spot].members = f.spots[spot].members.saturating_sub(1);
                                    world.despawn(m.1);
                                }
                                job.walk = Walk::Arrived;
                            }
                        }
                    }
                }
            }
            Walk::Running(until) => {
                if far > AWAY_RUNNING {
                    job.walk = Walk::Lost;
                } else if tick >= until {
                    // Out of breath: he turns on them.
                    job.walk = Walk::Cornered;
                    self.set_course(world, spot, pos, 0, false);
                }
            }
            Walk::Cornered | Walk::Arrived | Walk::Lost => {}
        }
        self.tail = Some(job);
    }

    /// Seconds he walks before his look about number `n` (the case's own
    /// dice: the same for everyone, every time).
    fn look_wait(&self, n: u32) -> f64 {
        let u = Seed(self.number as u64)
            .child(self.gang as u64)
            .child("tail")
            .child(n as u64)
            .unit();
        LOOK_EVERY.0 + (LOOK_EVERY.1 - LOOK_EVERY.0) * u
    }

    /// Everyone of `spot` makes for `to`: fleeing until `flee`, or calmly.
    fn set_course(&self, world: &mut World, spot: usize, to: DVec3, flee: u64, calm: bool) {
        for mut en in world.query::<&mut combat::Enemy>().iter_mut(world) {
            if en.spot == spot {
                en.home = to;
                en.calm = calm;
                en.flee_until = flee;
                en.leaving = flee > 0;
            }
        }
    }

    /// Is the tail over with the hideout known: he went in, or he talked?
    pub(super) fn tail_done(&self, world: &mut World) -> bool {
        match self.tail.as_ref().map(|j| j.walk) {
            Some(Walk::Arrived) => true,
            Some(Walk::Cornered) => self.rooms.iter().all(|r| subdued_spot(world, r.spot)),
            _ => false,
        }
    }

    pub(super) fn tail_lost(&self) -> bool {
        self.tail.as_ref().is_some_and(|j| j.walk == Walk::Lost)
    }

    /// The points of his way still to come.
    pub fn tail_way(&self) -> Vec<DVec3> {
        self.tail
            .as_ref()
            .map_or(Vec::new(), |j| j.way[j.next.min(j.way.len())..].to_vec())
    }

    /// The tail, while this step is one.
    pub fn tail(&self) -> Option<&Job> {
        self.tail.as_ref()
    }

    /// The courier (the first of them on his feet): id and place.
    pub fn courier(&self, world: &mut World) -> Option<(NetId, DVec3)> {
        self.tail.as_ref()?;
        let spot = self.rooms.first()?.spot;
        world
            .query::<(&Net, &combat::Enemy, &Motion, &Fighter)>()
            .iter(world)
            .filter(|(_, en, _, f)| en.spot == spot && f.standing())
            .map(|(n, _, m, _)| (n.0, m.0.pos))
            .min_by_key(|c| c.0)
    }

    /// Seconds until a cold trail is lost, while it is cold.
    pub(super) fn trail_left(&self, tick: u64) -> Option<f64> {
        let since = self.tail.as_ref()?.cold_since?;
        Some((since + ticks(TRAIL_SECONDS)).saturating_sub(tick) as f64 * DT)
    }
}

fn subdued_spot(world: &mut World, spot: usize) -> bool {
    super::running::subdued(world, spot)
}
