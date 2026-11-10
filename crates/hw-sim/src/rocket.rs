//! The rocket port's flights, as the server keeps them: every
//! [`PERIOD`] seconds of game clock the rocket vents, its arms swing away,
//! it counts down from ten, lights, climbs out of sight on a column of fire
//! and smoke, and some minutes later comes back down on its tail onto the
//! stand. Everything is a pure function of the game clock and the world
//! seed, so every player sees the same launch at the same moment without
//! anything sent over the wire.
//!
//! A launch can be scrubbed before the engine lights ([`Schedule::scrub`]):
//! the hook for a saboteur mission. The pad's weak spots are
//! [`hw_gen::urban::gantry::weak_spots`].

use crate::collision::Solid;
use glam::{DVec2, DVec3};
use hw_core::Seed;
use hw_gen::urban::gantry::{
    ARM_DEPTH, ARM_HALF_W, ARM_HEIGHTS, ARM_HINGE, ARM_LEN, FIN_ANGLES, FIN_REACH, ROCKET_H,
    ROCKET_R, STAND_H,
};
use hw_gen::urban::landmarks::{Landmark, LandmarkKind};
use hw_gen::urban::world::TownWorld;
use std::collections::BTreeMap;

/// Seconds of game clock from one launch to the next.
pub const PERIOD: f64 = 480.0;
/// The first launch's countdown starts between these many seconds into a
/// session (by the seed): soon, so a new player can catch one.
pub const FIRST: (f64, f64) = (45.0, 120.0);

/// A launch's timeline, in seconds from the start of its cycle.
/// Venting (white vapour off the hull, the klaxon) starts the cycle.
pub const VENT: f64 = 0.0;
/// The arms swing away over these seconds.
pub const ARMS_OUT: (f64, f64) = (8.0, 16.0);
/// The last ten seconds counted on the board: ten at `COUNT`.
pub const COUNT: f64 = 20.0;
/// The engine lights at zero and roars on the stand ...
pub const IGNITION: f64 = 30.0;
/// ... until the rocket lifts off and climbs ...
pub const LIFTOFF: f64 = 33.0;
/// ... at this acceleration (m/s²) ...
pub const CLIMB: f64 = 3.0;
/// ... out of sight at this height above the stand.
pub const GONE: f64 = 2600.0;
/// It comes back: the engine lights again this high up ...
pub const RETURN: f64 = 260.0;
pub const RETURN_FROM: f64 = 1600.0;
/// ... and it brakes all the way down, touching down at `LANDED`.
pub const LANDED: f64 = 300.0;
/// Vapour while it cools, then the arms swing back in.
pub const ARMS_IN: (f64, f64) = (312.0, 322.0);
/// After a scrub the arms swing back this long.
const SCRUB_ARMS: f64 = 10.0;

/// The seconds the climb takes from lift-off to out of sight.
pub fn climb_time() -> f64 {
    (2.0 * GONE / CLIMB).sqrt()
}

/// What the rocket is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// On the stand, the arms in, waiting.
    Rest,
    /// Venting before the count (and its arms swinging away).
    Venting,
    /// The last ten seconds: seconds left (10 to 1).
    Countdown(u32),
    /// The engine lit, still on the stand.
    Ignition,
    /// Climbing away.
    Ascent,
    /// Out of sight.
    Away,
    /// Coming back down on its engine.
    Descent,
    /// Down again, cooling (vapour), the arms swinging back.
    Landed,
    /// The launch was called off before the engine lit.
    Scrubbed,
}

/// The rocket at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Now {
    pub phase: Phase,
    /// The nozzle's lip above the stand's top (m).
    pub height: f64,
    /// Up speed (m/s; down is negative).
    pub speed: f64,
    /// How hard the engine burns (0 off, 1 full).
    pub thrust: f64,
    /// The arms: 0 at the hull, 1 swung away against the tower.
    pub arms: f64,
    /// Is it in sight (false while away)?
    pub shown: bool,
    /// Seconds to the next ignition (0 while lit or flying).
    pub to_launch: f64,
    /// Which launch this is (counts from 0).
    pub cycle: u64,
    /// Seconds into this launch's cycle.
    pub local: f64,
}

/// When the launches happen.
#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    /// The first cycle's start (game seconds).
    pub first: f64,
    pub period: f64,
    /// Launches called off: cycle, and the seconds into it when.
    scrubbed: BTreeMap<u64, f64>,
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Schedule {
    pub fn new(seed: Seed) -> Self {
        let u = seed.child("rocket launches").unit();
        Self {
            first: FIRST.0 + (FIRST.1 - FIRST.0) * u,
            period: PERIOD,
            scrubbed: BTreeMap::new(),
        }
    }

    /// The rocket at game second `t`.
    pub fn at(&self, t: f64) -> Now {
        let rest = |to_launch: f64, cycle: u64, local: f64| Now {
            phase: Phase::Rest,
            height: 0.0,
            speed: 0.0,
            thrust: 0.0,
            arms: 0.0,
            shown: true,
            to_launch,
            cycle,
            local,
        };
        if t < self.first {
            return rest(self.first - t + IGNITION, 0, 0.0);
        }
        let since = t - self.first;
        let cycle = (since / self.period).floor() as u64;
        let s = since - cycle as f64 * self.period;
        let to_next = self.period - s + IGNITION;
        if let Some(&at) = self.scrubbed.get(&cycle) {
            if s >= at {
                // Called off: the arms swing back from wherever they were.
                let was = self.arms_at(at);
                let back = (s - at) / SCRUB_ARMS;
                return Now {
                    phase: Phase::Scrubbed,
                    arms: was * (1.0 - smooth(back)),
                    to_launch: to_next,
                    ..rest(to_next, cycle, s)
                };
            }
        }
        let arms = self.arms_at(s);
        let base = Now {
            arms,
            to_launch: (IGNITION - s).max(0.0),
            ..rest(0.0, cycle, s)
        };
        if s < COUNT {
            return Now {
                phase: Phase::Venting,
                ..base
            };
        }
        if s < IGNITION {
            let left = (IGNITION - s).ceil().clamp(1.0, 10.0) as u32;
            return Now {
                phase: Phase::Countdown(left),
                ..base
            };
        }
        if s < LIFTOFF {
            // The engine comes up to full on the stand.
            return Now {
                phase: Phase::Ignition,
                thrust: smooth((s - IGNITION) / 1.2),
                ..base
            };
        }
        let up = s - LIFTOFF;
        if up < climb_time() {
            return Now {
                phase: Phase::Ascent,
                height: 0.5 * CLIMB * up * up,
                speed: CLIMB * up,
                thrust: 1.0,
                ..base
            };
        }
        if s < RETURN {
            return Now {
                phase: Phase::Away,
                height: GONE,
                shown: false,
                ..base
            };
        }
        if s < LANDED {
            // Braking evenly from RETURN_FROM to a standstill on the stand.
            let span = LANDED - RETURN;
            let k = 1.0 - (s - RETURN) / span;
            return Now {
                phase: Phase::Descent,
                height: RETURN_FROM * k * k,
                speed: -2.0 * RETURN_FROM * k / span,
                // Lighter as it slows, never quite out.
                thrust: 0.55 + 0.45 * smooth(1.0 - k * 1.5),
                ..base
            };
        }
        if s < ARMS_IN.1 + 30.0 {
            return Now {
                phase: Phase::Landed,
                to_launch: to_next,
                ..base
            };
        }
        rest(to_next, cycle, s)
    }

    /// The arms' swing at `s` seconds into a cycle (no scrub).
    fn arms_at(&self, s: f64) -> f64 {
        if s < ARMS_IN.0 {
            smooth((s - ARMS_OUT.0) / (ARMS_OUT.1 - ARMS_OUT.0))
        } else {
            1.0 - smooth((s - ARMS_IN.0) / (ARMS_IN.1 - ARMS_IN.0))
        }
    }

    /// Calls off the launch under way at game second `t`, if its engine
    /// has not lit yet (a saboteur's work, or heroes stopping one). True if
    /// it was called off. The next cycle goes ahead as usual.
    pub fn scrub(&mut self, t: f64) -> bool {
        let now = self.at(t);
        let can = matches!(now.phase, Phase::Venting | Phase::Countdown(_));
        if can {
            self.scrubbed.insert(now.cycle, now.local);
        }
        can
    }
}

/// A rocket port: where it stands, and its flights.
#[derive(Clone, Debug)]
pub struct Pad {
    pub landmark: Landmark,
    pub schedule: Schedule,
}

/// The town's rocket ports (one at most today).
pub fn pads(world: &TownWorld) -> Vec<Pad> {
    world
        .landmarks
        .iter()
        .filter(|l| l.kind == LandmarkKind::RocketGantry)
        .map(|l| Pad {
            landmark: l.clone(),
            schedule: Schedule::new(world.seed.child("rocket port")),
        })
        .collect()
}

impl Pad {
    /// A local point and height (the pad's frame) in the town.
    pub fn world_at(&self, local: DVec2, y: f64) -> DVec3 {
        let p = self.landmark.to_plan(local);
        DVec3::new(p.x, self.landmark.base + y, p.y)
    }

    /// The rocket's and its arms' solid parts at game second `t`, if
    /// anyone in `near` is within `r` of the pad (none while away).
    pub fn solids(&self, t: f64, near: &[DVec3], r: f64) -> Vec<Solid> {
        let l = &self.landmark;
        // The rocket only ever stands over its pad, however high.
        let reach = l.plaza + r;
        if !near
            .iter()
            .any(|p| DVec2::new(p.x, p.z).distance(l.pos) < reach)
        {
            return Vec::new();
        }
        let now = self.schedule.at(t);
        let x = DVec2::new(l.dir.y, -l.dir.x);
        let z = l.dir;
        let mut out = Vec::new();
        let boxed = |at: DVec2, inward: DVec2, half: DVec2, y0: f64, y1: f64| Solid {
            origin: l.to_plan(at),
            along: DVec2::new(inward.y, -inward.x),
            inward,
            min: -half,
            max: half,
            y0: l.base + y0,
            y1: l.base + y1,
        };
        // Turns a local direction into the town's frame.
        let turn = |d: DVec2| (x * d.x + z * d.y).normalize_or(z);
        if now.shown {
            let foot = STAND_H + now.height;
            // The hull: two crossed boxes (a fat octagon), the nose a
            // narrower pair above.
            for a in [0.0f64, 45.0] {
                let d = turn(hw_core::det::dir2(a.to_radians()));
                let body = DVec2::splat(ROCKET_R * 0.86);
                let nose = DVec2::splat(ROCKET_R * 0.5);
                out.push(Solid {
                    inward: d,
                    along: DVec2::new(d.y, -d.x),
                    ..boxed(DVec2::ZERO, z, body, foot, foot + ROCKET_H * 0.7)
                });
                out.push(Solid {
                    inward: d,
                    along: DVec2::new(d.y, -d.x),
                    ..boxed(
                        DVec2::ZERO,
                        z,
                        nose,
                        foot + ROCKET_H * 0.7,
                        foot + ROCKET_H * 0.93,
                    )
                });
            }
            // The fins, thin slabs out to their feet.
            for a in FIN_ANGLES {
                let d = hw_core::det::dir2(a.to_radians());
                let mid = d * (ROCKET_R + FIN_REACH) * 0.5;
                let half = DVec2::new(0.12, (FIN_REACH - ROCKET_R) * 0.5);
                let mut s = boxed(mid, z, half, foot, foot + 7.0);
                s.inward = turn(d);
                s.along = DVec2::new(s.inward.y, -s.inward.x);
                out.push(s);
            }
        }
        // The arms' decks, wherever they have swung to.
        for (from, to) in arm_spans(now.arms) {
            let d = (to - from).normalize();
            for (k, h) in ARM_HEIGHTS.iter().enumerate() {
                let mut s = boxed(
                    (from + to) * 0.5,
                    z,
                    DVec2::new(ARM_HALF_W, ARM_LEN * 0.5),
                    h - 0.35,
                    h + if k == 1 { ARM_DEPTH } else { 0.9 },
                );
                s.inward = turn(d);
                s.along = DVec2::new(s.inward.y, -s.inward.x);
                out.push(s);
            }
        }
        out
    }
}

/// The arms' deck from the hinge to its far end (local), swung `arms` of
/// the way (0 at the hull, 1 folded against the tower towards +z).
pub fn arm_spans(arms: f64) -> [(DVec2, DVec2); 1] {
    let a = arm_angle(arms);
    let d = DVec2::new(-a.cos(), a.sin());
    [(ARM_HINGE, ARM_HINGE + d * ARM_LEN)]
}

/// The arms' turn about their hinge (radians; 0 pointing -x at the hull,
/// a right angle folded towards +z).
pub fn arm_angle(arms: f64) -> f64 {
    arms.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2
}

/// Every pad's solids near the heroes (see [`Pad::solids`]).
pub fn solids(pads: &[Pad], t: f64, near: &[DVec3], r: f64) -> Vec<Solid> {
    pads.iter().flat_map(|p| p.solids(t, near, r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> Schedule {
        Schedule::new(Seed(1))
    }

    #[test]
    fn a_launch_goes_up_and_comes_back() {
        let s = sched();
        assert!((FIRST.0..FIRST.1).contains(&s.first));
        let c = s.first + PERIOD * 2.0;
        assert_eq!(s.at(c - 1.0).phase, Phase::Rest);
        assert_eq!(s.at(c + 1.0).phase, Phase::Venting);
        assert_eq!(s.at(c + COUNT + 0.1).phase, Phase::Countdown(10));
        assert_eq!(s.at(c + IGNITION - 0.5).phase, Phase::Countdown(1));
        assert_eq!(s.at(c + IGNITION + 0.5).phase, Phase::Ignition);
        let up = s.at(c + LIFTOFF + 10.0);
        assert_eq!(up.phase, Phase::Ascent);
        assert!(up.height > 100.0 && up.speed > 0.0 && up.shown);
        assert!(LIFTOFF + climb_time() < RETURN);
        let away = s.at(c + (LIFTOFF + climb_time() + RETURN) * 0.5);
        assert_eq!(away.phase, Phase::Away);
        assert!(!away.shown);
        // Coming down, slower and slower, onto the stand.
        let mut last = f64::MAX;
        let mut t = c + RETURN;
        while t < c + LANDED {
            let n = s.at(t);
            assert_eq!(n.phase, Phase::Descent);
            assert!(n.height < last && n.speed <= 0.0 && n.thrust > 0.0);
            last = n.height;
            t += 0.5;
        }
        assert!(s.at(c + LANDED - 0.05).speed.abs() < 1.0, "a soft landing");
        let down = s.at(c + LANDED + 1.0);
        assert_eq!(down.phase, Phase::Landed);
        assert_eq!(down.height, 0.0);
        assert!(down.arms > 0.9);
        assert!(s.at(c + ARMS_IN.1 + 1.0).arms < 1e-9);
        // The board's count runs down to the next ignition.
        assert!((s.at(c + 5.0).to_launch - (IGNITION - 5.0)).abs() < 1e-9);
        assert!(s.at(c + 400.0).to_launch > 60.0);
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn the_flight_is_smooth() {
        let s = sched();
        let c = s.first;
        let mut prev = s.at(c);
        let mut t = c;
        while t < c + PERIOD {
            t += 0.05;
            let n = s.at(t);
            if n.shown && prev.shown {
                assert!(
                    (n.height - prev.height).abs() < 10.0,
                    "a jump at {}: {} to {}",
                    t - c,
                    prev.height,
                    n.height
                );
            }
            assert!(
                (n.arms - prev.arms).abs() < 0.05,
                "the arms jump at {}",
                t - c
            );
            prev = n;
        }
    }

    #[test]
    fn a_scrub_stops_the_count_but_not_the_next() {
        let mut s = sched();
        let c = s.first + PERIOD;
        assert!(!s.scrub(c - 5.0), "nothing to call off at rest");
        assert!(s.scrub(c + COUNT + 3.0));
        let n = s.at(c + COUNT + 4.0);
        assert_eq!(n.phase, Phase::Scrubbed);
        assert!(n.arms > 0.5, "the arms start back from out");
        assert_eq!(s.at(c + LIFTOFF + 10.0).height, 0.0, "it stays down");
        assert!(s.at(c + LIFTOFF + 10.0).arms < 1e-9);
        assert_eq!(s.at(c + PERIOD + COUNT + 1.0).phase, Phase::Countdown(10));
        // Once lit, it goes.
        assert!(!s.scrub(c + PERIOD + IGNITION + 1.0));
    }

    #[test]
    fn the_rocket_is_solid_on_the_pad_and_gone_when_away() {
        let l = Landmark {
            kind: LandmarkKind::RocketGantry,
            pos: DVec2::new(100.0, 50.0),
            plaza: 26.0,
            base: 4.0,
            low: 3.0,
            height: 40.0,
            dir: DVec2::new(0.6, 0.8),
            stairs: Vec::new(),
        };
        let pad = Pad {
            landmark: l.clone(),
            schedule: sched(),
        };
        let hero = [DVec3::new(100.0, 4.0, 60.0)];
        let t0 = pad.schedule.first - 10.0;
        let s = pad.solids(t0, &hero, 25.0);
        // Hull: something solid on the axis at mid height, nothing beside.
        let hit = |p: DVec3| {
            s.iter().any(|b| {
                let q = DVec2::new(p.x, p.z) - b.origin;
                let lq = DVec2::new(q.dot(b.along), q.dot(b.inward));
                lq.cmpge(b.min).all() && lq.cmple(b.max).all() && p.y >= b.y0 && p.y <= b.y1
            })
        };
        let mid = DVec3::new(l.pos.x, l.base + STAND_H + ROCKET_H * 0.4, l.pos.y);
        assert!(hit(mid));
        assert!(!hit(mid + DVec3::X * 6.0));
        // The crew arm at the hull's side, before the launch.
        let arm = pad.world_at(
            ARM_HINGE + DVec2::NEG_X * (ARM_LEN * 0.5),
            ARM_HEIGHTS[1] + 0.5,
        );
        assert!(hit(arm));
        // Away: no hull; the arms folded.
        let away = pad.schedule.first + RETURN - 20.0;
        let s2 = pad.solids(away, &hero, 25.0);
        assert!(s2
            .iter()
            .all(|b| b.y1 < l.base + ARM_HEIGHTS[1] + ARM_DEPTH + 0.1));
        assert_eq!(s2.len(), 2);
        // Far from everyone: nothing.
        assert!(pad
            .solids(t0, &[DVec3::new(9000.0, 0.0, 9000.0)], 25.0)
            .is_empty());
        // Every solid stays within the movers' reach of its origin.
        for b in s.iter().chain(&s2) {
            assert!(b.min.length() <= 3.5 && b.max.length() <= 3.5, "{b:?}");
        }
    }
}
