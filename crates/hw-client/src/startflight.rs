//! The camera's way from the title down to the hero.
//!
//! Two stages. First the camera holds the title's view (turning slowly)
//! while the town around the hero is drawn and put in: the cells, the props
//! and the hero's own model, so nothing pops in or stops a frame on the way.
//! Then it flies a ballistic arc, up and over and down behind the hero,
//! turning its look from the title's view to the hero as it goes.
//!
//! Time comes from Engine's even frame step (`frame.dt`), so the flight
//! moves the same amount every frame the display shows.

use glam::{DVec2, Vec3};

/// Longest the camera waits for the hero's surroundings (s); after that it
/// flies anyway (a slow disk must not keep the player on the title view).
pub const WARM_MAX_SECONDS: f32 = 12.0;
/// Around the hero this far (m), everything is drawn before the flight.
pub const WARM_RADIUS_M: f64 = 450.0;
/// The view turns this fast while it waits (degrees per second).
const WAIT_TURN_DEG_PER_S: f32 = 2.5;

/// Where the flight is.
#[derive(Clone, Debug)]
pub enum Stage {
    /// Holding the title's view while the hero's surroundings load.
    Warming { waited: f32 },
    /// On the arc; `t` seconds of `duration`.
    Flying { t: f32, duration: f32 },
}

/// The camera's flight from the title to the hero.
#[derive(Clone, Debug)]
pub struct StartFlight {
    pub stage: Stage,
    from: Vec3,
    from_yaw: f32,
    from_pitch: f32,
}

/// A camera pose: eye, yaw and pitch in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl StartFlight {
    /// Starts at the title's camera.
    pub fn new(from: Vec3, from_yaw: f32, from_pitch: f32) -> Self {
        Self {
            stage: Stage::Warming { waited: 0.0 },
            from,
            from_yaw,
            from_pitch,
        }
    }

    /// The places the town keeps loaded meanwhile: where the camera is and
    /// where it is going (the hero), so the view it starts from keeps its
    /// buildings while the hero's are drawn.
    pub fn focus(&self, camera: Vec3, hero: Option<Vec3>) -> Vec<DVec2> {
        let flat = |p: Vec3| DVec2::new(p.x as f64, p.z as f64);
        let mut out = vec![flat(camera), flat(self.from)];
        out.extend(hero.map(flat));
        out
    }

    /// Advances by `dt` and gives this frame's camera and how far along the
    /// flight is (0 while waiting, 1 at the end). `ready`: whether the
    /// hero's surroundings are in, `None` while the hero is not known yet
    /// (then it waits however long that takes: there is nowhere to fly). `to`: the game camera's pose behind the hero;
    /// `look_at`: the point it looks at. Returns `None` once the flight is
    /// over (the game camera takes over).
    pub fn step(
        &mut self,
        dt: f32,
        ready: Option<bool>,
        to: Pose,
        look_at: Vec3,
    ) -> Option<(Pose, f32)> {
        let dt = dt.clamp(0.0, 0.1);
        match &mut self.stage {
            Stage::Warming { waited } => {
                *waited += dt;
                let pose = Pose {
                    eye: self.from,
                    yaw: self.from_yaw + WAIT_TURN_DEG_PER_S * *waited,
                    pitch: self.from_pitch,
                };
                if ready.is_some_and(|r| r || *waited >= WARM_MAX_SECONDS) {
                    // Fly from where the wait left the view.
                    self.from_yaw = pose.yaw;
                    let d = horizontal(self.from, to.eye);
                    self.stage = Stage::Flying {
                        t: 0.0,
                        duration: flight_seconds(d),
                    };
                }
                Some((pose, 0.0))
            }
            Stage::Flying { t, duration } => {
                *t += dt;
                let k = (*t / *duration).clamp(0.0, 1.0);
                if k >= 1.0 {
                    return None;
                }
                Some((self.pose_at(k, to, look_at), k))
            }
        }
    }

    /// The camera at `k` (0..1) of the arc.
    pub fn pose_at(&self, k: f32, to: Pose, look_at: Vec3) -> Pose {
        let s = smoother(k);
        let d = horizontal(self.from, to.eye);
        // A parabola over the straight line: up and over, down behind the
        // hero. Higher for longer flights, never lower than both ends.
        let apex = (d * 0.22).clamp(25.0, 260.0);
        let mut eye = self.from.lerp(to.eye, s);
        eye.y += apex * 4.0 * s * (1.0 - s);
        // The look turns from the title's view to the hero over the first
        // two thirds, then stays on the hero while the camera settles.
        let start_look = self.from + dir(self.from_yaw, self.from_pitch) * 120.0;
        let look = start_look.lerp(look_at, smoother((k * 1.5).min(1.0)));
        let (yaw, pitch) = yaw_pitch(look - eye).unwrap_or((to.yaw, to.pitch));
        // The last stretch hands over to the game camera's own angles, so
        // the hand-over has no jump.
        let tail = smoother(((k - 0.85) / 0.15).clamp(0.0, 1.0));
        Pose {
            eye,
            yaw: yaw + wrap(to.yaw - yaw) * tail,
            pitch: pitch + (to.pitch - pitch) * tail,
        }
    }
}

/// How long the arc takes for `d` metres across (s).
pub fn flight_seconds(d: f32) -> f32 {
    (2.6 + d / 700.0).clamp(2.6, 5.0)
}

fn horizontal(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(b.x - a.x, 0.0, b.z - a.z).length()
}

/// Ease in and out with no jerk at either end.
fn smoother(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// Degrees in -180..180.
fn wrap(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

fn dir(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    Vec3::new(y.sin() * p.cos(), p.sin(), y.cos() * p.cos())
}

fn yaw_pitch(v: Vec3) -> Option<(f32, f32)> {
    let v = v.try_normalize()?;
    let yaw = v.x.atan2(v.z).to_degrees();
    let pitch = v.y.clamp(-1.0, 1.0).asin().to_degrees().clamp(-85.0, 85.0);
    Some((yaw, pitch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(eye: Vec3, yaw: f32, pitch: f32) -> Pose {
        Pose { eye, yaw, pitch }
    }

    #[test]
    fn waits_until_ready_then_flies_an_arc_and_ends_on_the_game_camera() {
        let from = Vec3::new(0.0, 120.0, 0.0);
        let mut f = StartFlight::new(from, 30.0, -10.0);
        let to = pose(Vec3::new(800.0, 20.0, 600.0), 200.0, -12.0);
        let look_at = Vec3::new(805.0, 18.0, 606.0);
        let dt = 1.0 / 60.0;
        // Waiting: the camera stays where the title left it.
        for _ in 0..30 {
            let (p, k) = f.step(dt, Some(false), to, look_at).expect("waiting");
            assert_eq!(p.eye, from);
            assert_eq!(k, 0.0);
        }
        // Ready: the arc.
        let mut last = f
            .step(dt, Some(true), to, look_at)
            .expect("flight starts")
            .0;
        let mut top: f32 = 0.0;
        let mut biggest_step: f32 = 0.0;
        let mut steps = Vec::new();
        while let Some((p, _)) = f.step(dt, Some(true), to, look_at) {
            top = top.max(p.eye.y);
            let step = p.eye.distance(last.eye);
            biggest_step = biggest_step.max(step);
            steps.push(step);
            last = p;
        }
        // It rose above both ends.
        assert!(top > from.y.max(to.eye.y) + 20.0, "top {top}");
        // It ends at the game camera, looking the game camera's way.
        assert!(last.eye.distance(to.eye) < 1.0, "ends at {:?}", last.eye);
        assert!(wrap(last.yaw - to.yaw).abs() < 1.0);
        assert!((last.pitch - to.pitch).abs() < 1.0);
        // Smooth: no frame moves much more than its neighbours.
        for w in steps.windows(2) {
            assert!((w[1] - w[0]).abs() < biggest_step * 0.05 + 0.05, "{w:?}");
        }
    }

    #[test]
    fn flies_anyway_after_the_longest_wait() {
        let mut f = StartFlight::new(Vec3::ZERO, 0.0, 0.0);
        let to = pose(Vec3::new(100.0, 0.0, 0.0), 90.0, 0.0);
        let steps = (WARM_MAX_SECONDS * 10.0) as usize + 2;
        // Not while the hero is unknown...
        for _ in 0..steps {
            f.step(0.1, None, to, Vec3::new(110.0, 0.0, 0.0));
        }
        assert!(matches!(f.stage, Stage::Warming { .. }));
        // ...but once it is known.
        f.step(0.1, Some(false), to, Vec3::new(110.0, 0.0, 0.0));
        assert!(matches!(f.stage, Stage::Flying { .. }));
    }
}
