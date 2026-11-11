//! A hero's animation, driven by the movement the server sends: which clip
//! plays and how fast, and how a flier's body tips into its flight.
//!
//! On the ground: idle, walk, jog or sprint by speed, played at the rate
//! that keeps the feet from sliding. In the air: the jump start on the way
//! up, the jump loop while falling, the landing when it touches down after
//! a real fall. Flying: "hover" when slow, "fly" when moving, the body
//! tipped forward by its speed, up for a climb, down for a dive, and banked
//! into turns.

use crate::people::{JOG_MPS, SPRINT_MPS, WALK_MPS};
use glam::{Quat, Vec3};

/// What to show this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    pub clip: &'static str,
    /// Playback rate.
    pub rate: f32,
    /// Play once and hold the end (the jump start, the landing).
    pub once: bool,
    pub yaw: f32,
    /// Degrees, forward positive (the body tips about its middle).
    pub pitch: f32,
    /// Degrees about the body's own head-to-feet axis, before it tips: a
    /// flier lying flat banks into a turn with it (left side down for a
    /// left turn is negative).
    pub bank: f32,
}

impl Drive {
    /// The body's turn: yaw, then the tip forward, then the bank.
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw.to_radians())
            * Quat::from_rotation_x(self.pitch.to_radians())
            * Quat::from_rotation_y(self.bank.to_radians())
    }

    /// [`Drive::rotation`] as an Engine `Place`'s yaw, pitch and roll
    /// (degrees; Place turns yaw, then pitch, then roll about forward).
    pub fn place_angles(&self) -> (f32, f32, f32) {
        let (y, x, z) = self.rotation().to_euler(glam::EulerRot::YXZ);
        (y.to_degrees(), x.to_degrees(), z.to_degrees())
    }
}

/// Height of the body's middle above the feet: a flier tips about it.
pub const PIVOT: f32 = 1.0;

/// Seconds off the ground, not jumping, before the fall loop shows.
pub const AIR_GRACE: f32 = 0.15;

#[derive(Clone, Debug)]
pub struct HeroAnim {
    yaw: f32,
    pitch: f32,
    bank: f32,
    /// Seconds in the air (not flying).
    air: f32,
    /// Seconds left of the landing clip.
    landing: f32,
    /// Seconds since leaving the ground going up (the jump start plays).
    rising: f32,
    /// Last vertical speed and how much it looked like flight (0..1).
    last_vy: Option<f32>,
    flight: f32,
    /// The clip shown last, to restart one-shot clips only on change.
    pub shown: &'static str,
}

impl Default for HeroAnim {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            bank: 0.0,
            air: 0.0,
            landing: 0.0,
            rising: 0.0,
            last_vy: None,
            flight: 0.0,
            shown: "",
        }
    }
}

/// Angles towards a target, smoothly (`k` per second).
fn ease(from: f32, to: f32, k: f32, dt: f32) -> f32 {
    from + (to - from) * (1.0 - (-k * dt).exp())
}

/// Shortest signed difference of two angles in degrees.
fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

impl HeroAnim {
    /// Turns the body towards `yaw` (degrees), quickly: to face a foe.
    pub fn face(&mut self, yaw: f32, dt: f32) {
        self.yaw = ease(self.yaw, self.yaw + angle_delta(self.yaw, yaw), 16.0, dt);
    }

    /// Turns the body towards `yaw` (degrees) at `k` per second: slowly,
    /// to look at someone.
    pub fn turn_to(&mut self, yaw: f32, k: f32, dt: f32) {
        self.yaw = ease(self.yaw, self.yaw + angle_delta(self.yaw, yaw), k, dt);
    }

    /// The way the body faces now (degrees).
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// One frame. `flying`: known for our own hero; `None` for others,
    /// then it is told from the motion (in the air without falling).
    pub fn step(&mut self, vel: Vec3, grounded: bool, flying: Option<bool>, dt: f32) -> Drive {
        let dt = dt.max(1e-4);
        let gravity = hw_sim::GRAVITY as f32;
        // Others' flight: airborne and not accelerating down like a fall.
        let fly_now = match flying {
            Some(f) => f,
            None => {
                let falling = self
                    .last_vy
                    .map(|v| (vel.y - v) / dt < -gravity * 0.5)
                    .unwrap_or(false);
                let target = if grounded || falling { 0.0 } else { 1.0 };
                self.flight = ease(self.flight, target, 6.0, dt);
                self.flight > 0.5
            }
        };
        self.last_vy = Some(vel.y);
        let flat = Vec3::new(vel.x, 0.0, vel.z);
        let speed = flat.length();
        let old_yaw = self.yaw;
        if speed > 0.3 {
            let want = vel.x.atan2(vel.z).to_degrees();
            let k = if fly_now { 5.0 } else { 14.0 };
            self.yaw = old_yaw + angle_delta(old_yaw, want) * (1.0 - (-k * dt).exp());
        }
        let turn_rate = angle_delta(old_yaw, self.yaw) / dt;

        let (clip, rate, once, pitch, bank) = if fly_now {
            self.air = 0.0;
            self.landing = 0.0;
            let s = vel.length();
            if s > 4.0 {
                // Level flight lies almost flat; a climb stands up, a dive
                // goes over head first.
                let climb = (vel.y / s).clamp(-1.0, 1.0).asin().to_degrees();
                let flatness = ((s - 4.0) / 10.0).clamp(0.0, 1.0);
                let pitch = (82.0 - climb) * (0.55 + 0.45 * flatness);
                // Into a turn like a plane: the faster and tighter, the
                // steeper (the turn's sideways pull against gravity).
                let pull = turn_rate.to_radians() * s / hw_sim::GRAVITY as f32;
                let bank = -pull.atan().to_degrees().clamp(-55.0, 55.0) * (0.4 + 0.6 * flatness);
                ("fly", 1.0, false, pitch.clamp(0.0, 150.0), bank)
            } else {
                // Hovering: lean a little into the drift.
                ("hover", 1.0, false, (speed * 4.0).min(15.0), 0.0)
            }
        } else if grounded || (self.air + dt < AIR_GRACE && vel.y <= 1.0) {
            // A moment off the ground without a jump (a missed update, a
            // drop the server steps down) keeps the run going.
            if grounded {
                if self.air > 0.45 && speed < 5.0 {
                    self.landing = 0.55;
                }
                self.air = 0.0;
                self.rising = 0.0;
            } else {
                self.air += dt;
            }
            if self.landing > 0.0 {
                self.landing -= dt;
                ("jump land", 1.0, true, 0.0, 0.0)
            } else if speed < 0.25 {
                ("idle", 1.0, false, 0.0, 0.0)
            } else if speed < 2.4 {
                ("walk", (speed / WALK_MPS).clamp(0.5, 1.6), false, 0.0, 0.0)
            } else if speed < 5.0 {
                ("jog", (speed / JOG_MPS).clamp(0.7, 1.4), false, 0.0, 0.0)
            } else {
                let lean = ((speed - 5.0) * 1.5).min(6.0);
                (
                    "sprint",
                    (speed / SPRINT_MPS).clamp(0.8, 1.3),
                    false,
                    lean,
                    0.0,
                )
            }
        } else {
            self.air += dt;
            if vel.y > 1.0 && self.air < 0.4 {
                self.rising += dt;
                ("jump start", 1.6, true, 0.0, 0.0)
            } else {
                ("jump", 1.0, false, 0.0, 0.0)
            }
        };
        let k = if fly_now { 4.0 } else { 10.0 };
        self.pitch = ease(self.pitch, pitch, k, dt);
        self.bank = ease(self.bank, bank, 3.0, dt);
        Drive {
            clip,
            rate,
            once,
            yaw: self.yaw,
            pitch: self.pitch,
            bank: self.bank,
        }
    }
}

/// Where to put the model's feet so that it tips about its middle.
pub fn tipped_origin(feet: Vec3, d: &Drive) -> Vec3 {
    let mid = Vec3::Y * PIVOT;
    feet + mid - d.rotation() * mid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn running_walking_and_standing() {
        let mut a = HeroAnim::default();
        let d = a.step(Vec3::ZERO, true, Some(false), 0.016);
        assert_eq!(d.clip, "idle");
        let d = a.step(Vec3::new(0.0, 0.0, 1.4), true, Some(false), 0.016);
        assert_eq!(d.clip, "walk");
        let d = a.step(Vec3::new(7.0, 0.0, 0.0), true, Some(false), 0.016);
        assert_eq!(d.clip, "sprint");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_fall_ends_in_a_landing() {
        let mut a = HeroAnim::default();
        for _ in 0..40 {
            a.step(Vec3::new(0.0, -5.0, 0.0), false, Some(false), 0.016);
        }
        assert_eq!(
            a.step(Vec3::ZERO, true, Some(false), 0.016).clip,
            "jump land"
        );
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_moment_off_the_ground_is_no_fall() {
        let mut a = HeroAnim::default();
        let run = Vec3::new(7.0, 0.0, 0.0);
        a.step(run, true, Some(false), 0.016);
        // A blip under the grace: still running.
        for _ in 0..6 {
            let d = a.step(run - Vec3::Y, false, Some(false), 0.016);
            assert_eq!(d.clip, "sprint");
        }
        a.step(run, true, Some(false), 0.016);
        // Off a ledge for real: the fall shows once the grace is over.
        let mut d = a.step(run, false, Some(false), 0.016);
        for _ in 0..20 {
            d = a.step(run - Vec3::Y * 3.0, false, Some(false), 0.016);
        }
        assert_eq!(d.clip, "jump");
        // A jump shows at once.
        let mut b = HeroAnim::default();
        b.step(run, true, Some(false), 0.016);
        let d = b.step(run + Vec3::Y * 8.0, false, Some(false), 0.016);
        assert_eq!(d.clip, "jump start");
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn others_flight_is_told_from_their_motion() {
        let mut a = HeroAnim::default();
        let mut d = a.step(Vec3::ZERO, false, None, 0.016);
        for _ in 0..60 {
            d = a.step(Vec3::new(0.0, 0.0, 20.0), false, None, 0.016);
        }
        assert_eq!(d.clip, "fly");
        assert!(d.pitch > 60.0, "level flight lies flat: {}", d.pitch);
        let mut b = HeroAnim::default();
        let mut vy = 0.0;
        for _ in 0..60 {
            vy -= 20.0 * 0.016;
            d = b.step(Vec3::new(0.0, vy, 0.0), false, None, 0.016);
        }
        assert_eq!(d.clip, "jump", "a fall is not flight");
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn tipping_keeps_the_middle_in_place() {
        let d = Drive {
            clip: "fly",
            rate: 1.0,
            once: false,
            yaw: 30.0,
            pitch: 80.0,
            bank: 10.0,
        };
        let feet = Vec3::new(5.0, 2.0, -3.0);
        let o = tipped_origin(feet, &d);
        let mid = o + d.rotation() * (Vec3::Y * PIVOT);
        assert!((mid - (feet + Vec3::Y * PIVOT)).length() < 1e-4);
        // Engine's Place angles give the same turn.
        let (y, p, r) = d.place_angles();
        let place = Quat::from_rotation_y(y.to_radians())
            * Quat::from_rotation_x(p.to_radians())
            * Quat::from_rotation_z(r.to_radians());
        assert!(place.dot(d.rotation()).abs() > 0.9999);
    }

    #[test]
    #[ignore = "on demand (looks): scripts/check.sh --on-demand"]
    fn a_flier_banks_into_its_turn() {
        // Flying fast along +Z, turning left (towards +X, the body's left).
        let mut a = HeroAnim::default();
        let mut d = a.step(Vec3::new(0.0, 0.0, 20.0), false, Some(true), 0.016);
        let mut heading = 0.0f32;
        for _ in 0..90 {
            heading += 40f32.to_radians() * 0.016;
            let v = Vec3::new(heading.sin(), 0.0, heading.cos()) * 20.0;
            d = a.step(v, false, Some(true), 0.016);
        }
        assert_eq!(d.clip, "fly");
        let r = d.rotation();
        // Lying flat: the head leads, the chest faces the ground ...
        assert!((r * Vec3::Y).y.abs() < 0.4, "head {:?}", r * Vec3::Y);
        // ... and the left side dips into the turn, the right rises.
        let left = r * Vec3::X;
        assert!(left.y < -0.25, "banked: left side {left:?}");
        // Flying straight again levels the wings.
        for _ in 0..200 {
            d = a.step(
                Vec3::new(heading.sin(), 0.0, heading.cos()) * 20.0,
                false,
                Some(true),
                0.016,
            );
        }
        assert!((d.rotation() * Vec3::X).y.abs() < 0.05);
    }
}
