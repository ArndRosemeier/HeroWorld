//! Rats in the utility tunnels: they scurry along the walls in short runs,
//! pause, sniff and turn back. Where each one is at any moment is a pure
//! function of the tunnel, the slot and the game time, so nothing is kept
//! between frames but two instanced models (two foot poses, which swap as
//! a rat runs, so its legs move).

use crate::citymesh::{MeshBuilder, Surf};
use crate::undergroundview::{sphere, tube};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_gen::urban::underground::{VoidKind, TUNNEL_HALF};
use hw_gen::urban::world::TownWorld;

/// Rats are drawn within this of the camera (m), at most [`MAX`].
const SHOWN_M: f64 = 40.0;
const MAX: usize = 40;
/// One slot of rat territory every so many metres of tunnel.
const SLOT: f64 = 16.0;
const SPEED: f64 = 2.3;

/// A rat's model standing on y = 0, nose towards +z; `step` 0 or 1 puts its
/// feet in one of two poses.
fn rat_mesh(step: usize) -> Mesh {
    let mut b = MeshBuilder::new();
    let fur = rgb(86, 76, 68);
    let belly = rgb(120, 108, 96);
    let skin = rgb(150, 118, 112);
    b.surf = Surf::Plain;
    let y = 0.065;
    sphere(&mut b, DVec3::new(0.0, y, -0.07), 0.062, fur);
    sphere(&mut b, DVec3::new(0.0, y + 0.004, 0.0), 0.07, fur);
    sphere(&mut b, DVec3::new(0.0, y - 0.02, 0.0), 0.05, belly);
    sphere(&mut b, DVec3::new(0.0, y + 0.012, 0.105), 0.046, fur);
    // Snout, ears, eyes.
    tube(
        &mut b,
        DVec3::new(0.0, y + 0.005, 0.12),
        DVec3::new(0.0, y - 0.005, 0.185),
        0.024,
        fur,
    );
    sphere(&mut b, DVec3::new(0.0, y - 0.004, 0.19), 0.014, skin);
    for s in [-1.0, 1.0] {
        sphere(&mut b, DVec3::new(s * 0.034, y + 0.05, 0.088), 0.021, skin);
        b.surf = Surf::Glow;
        sphere(
            &mut b,
            DVec3::new(s * 0.027, y + 0.03, 0.14),
            0.008,
            rgb(255, 80, 60),
        );
        b.surf = Surf::Plain;
    }
    // A long bare tail, curving.
    let tail = [
        DVec3::new(0.0, y - 0.01, -0.12),
        DVec3::new(0.0, y - 0.03, -0.2),
        DVec3::new(0.02, y - 0.045, -0.29),
        DVec3::new(0.05, y - 0.05, -0.38),
    ];
    for w in tail.windows(2) {
        tube(&mut b, w[0], w[1], 0.011, skin);
    }
    // Feet: diagonal pairs swing forward and back.
    let sw = if step == 0 { 0.05 } else { -0.05 };
    for (sx, z, sign) in [
        (-1.0, 0.06, 1.0),
        (1.0, 0.06, -1.0),
        (-1.0, -0.07, -1.0),
        (1.0, -0.07, 1.0),
    ] {
        let foot = DVec3::new(sx * 0.045, 0.0, z + sw * sign);
        tube(
            &mut b,
            DVec3::new(sx * 0.04, y - 0.02, z),
            foot + DVec3::Y * 0.01,
            0.012,
            belly,
        );
    }
    b.finish()
}

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Where along `a..b` (`length` m) a rat is at `t`, which way it faces and
/// whether it is running: out, a pause, back, a pause.
fn run(length: f64, pause: f64, t: f64) -> (f64, f64, bool) {
    let walk = length / SPEED;
    let period = 2.0 * (walk + pause);
    let t = t.rem_euclid(period);
    if t < walk {
        (t * SPEED, 1.0, true)
    } else if t < walk + pause {
        (length, 1.0, false)
    } else if t < 2.0 * walk + pause {
        (length - (t - walk - pause) * SPEED, -1.0, true)
    } else {
        (0.0, -1.0, false)
    }
}

/// The rats of the tunnels, drawn near the camera while it is below ground.
#[derive(Default)]
pub struct RatView {
    poses: [Option<EntityId>; 2],
}

impl RatView {
    pub fn update(
        &mut self,
        world: &mut World,
        eye: Vec3,
        game_s: f64,
        town: &TownWorld,
    ) -> EngineResult<()> {
        let eye2 = DVec2::new(eye.x as f64, eye.z as f64);
        let below = (eye.y as f64) < town.ground(eye2) - 3.0;
        let mut found: Vec<(f64, Place, usize)> = Vec::new();
        if below {
            let u = town.underground();
            for i in u.near(eye2, SHOWN_M) {
                let v = &u.voids[i];
                if v.kind != VoidKind::Tunnel {
                    continue;
                }
                let len = v.max.x - v.max.y;
                let seed = hash(
                    v.origin.x.round() as i32 as u32,
                    v.origin.y.round() as i32 as u32,
                );
                let slots = (len / SLOT).floor() as u32;
                for k in 0..slots {
                    let h = hash(seed, k);
                    if h % 100 >= 55 {
                        continue;
                    }
                    let length = 4.0 + (h >> 8) as f64 % 9.0;
                    let x0 = k as f64 * SLOT + (h >> 12) as f64 % 3.0;
                    if x0 + length > len {
                        continue;
                    }
                    let side = if (h >> 3) & 1 == 0 { 1.0 } else { -1.0 };
                    let off = side * (0.9 + ((h >> 16) % 8) as f64 * 0.15).min(TUNNEL_HALF - 0.35);
                    let pause = 1.5 + ((h >> 20) % 50) as f64 / 10.0;
                    let t = game_s + (h % 997) as f64;
                    let (d, dir, running) = run(length, pause, t);
                    let p = v.world(DVec2::new(x0 + d, 0.0)) + v.inward() * off;
                    let heading = v.along * dir;
                    let dist = p.distance(eye2);
                    if dist > SHOWN_M {
                        continue;
                    }
                    // A little sway when running, a sniffing turn when still.
                    let sway = if running {
                        (t * 14.0).sin() * 6.0
                    } else {
                        (t * 1.3 + k as f64).sin() * 25.0
                    };
                    let yaw = heading.x.atan2(heading.y).to_degrees() + sway;
                    let step = if running && ((t * 9.0) as i64) % 2 == 0 {
                        0
                    } else {
                        1
                    };
                    let place = Place::new(p.x as f32, v.floor_at(p) as f32 + 0.04, p.y as f32)
                        .with_yaw_deg(yaw as f32);
                    found.push((dist, place, step));
                }
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.truncate(MAX);
        for step in 0..2 {
            let places: Vec<Place> = found.iter().filter(|f| f.2 == step).map(|f| f.1).collect();
            if self.poses[step].is_none() {
                if places.is_empty() {
                    continue;
                }
                self.poses[step] = Some(world.spawn_instanced(rat_mesh(step)));
            }
            if let Some(id) = self.poses[step] {
                world.set_instances(id, &places)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rat_stays_on_its_run_and_turns_back() {
        let mut moved = 0;
        for k in 0..2000 {
            let t = k as f64 * 0.05;
            let (d, dir, running) = run(9.0, 2.0, t);
            assert!((0.0..=9.0 + 1e-9).contains(&d), "off its run at {d}");
            assert!(dir == 1.0 || dir == -1.0);
            moved += running as usize;
        }
        assert!(moved > 500, "it runs for a good part of the time");
    }
}
