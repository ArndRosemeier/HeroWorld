//! Robot street sweepers: little golden-age robots that trundle along the
//! pavements with two spinning brushes. Which citizens are sweepers, and
//! where they are, is `hw_sim::citizens` (they walk the citizens' routes);
//! this draws them, instanced, with three levels of detail.
//!
//! The models are built on a worker thread the first time they are needed.

use crate::carmesh::{lathe, slab, tube, Lod};
use crate::citymesh::{MeshBuilder, Surf};
use engine::prelude::*;
use glam::{DVec2, DVec3};
use hw_sim::citizens::Citizens;
use std::f64::consts::{PI, TAU};
use std::sync::mpsc::{channel, Receiver};

/// Sweepers are drawn within this distance of the camera...
const REACH: f64 = 140.0;
/// ...and at most this many.
const MOST: usize = 48;
/// Brushes turn this fast (radians a second).
const SPIN: f64 = 7.0;
/// Where the brushes sit on the robot: across, up, forward (m).
const BRUSH: (f64, f64, f64) = (0.3, 0.03, 0.34);

type Models = (Vec<LodLevel>, Vec<LodLevel>);

#[derive(Default)]
pub struct SweeperView {
    body: Option<LodId>,
    brush: Option<LodId>,
    pending: Option<Receiver<Models>>,
    near: Vec<u64>,
    refresh: f64,
}

impl SweeperView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the sweepers near `camera` to `time`.
    pub fn update(
        &mut self,
        world: &mut World,
        citizens: &Citizens,
        camera: DVec2,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        if self.body.is_none() {
            if self.pending.is_none() {
                let (tx, rx) = channel();
                std::thread::Builder::new()
                    .name("hw-sweepers".into())
                    .spawn(move || {
                        engine::parallel::background_thread();
                        let _ = tx.send(models());
                    })
                    .map_err(|e| EngineError::InvalidValue(e.to_string()))?;
                self.pending = Some(rx);
            }
            let Some(Ok((body, brush))) = self.pending.as_ref().map(|r| r.try_recv()) else {
                return Ok(());
            };
            self.pending = None;
            self.body = Some(world.spawn_lod_at(body, &[])?);
            self.brush = Some(world.spawn_lod_at(brush, &[])?);
        }
        let (Some(body), Some(brush)) = (self.body, self.brush) else {
            return Ok(());
        };
        self.refresh -= dt;
        if self.refresh <= 0.0 {
            self.refresh = 0.5;
            self.near = citizens
                .near(camera, REACH, time, crate::crowdview::MAX_SHOWN)
                .iter()
                .map(|c| c.id)
                .filter(|id| citizens.is_sweeper(*id))
                .take(MOST)
                .collect();
        }
        let mut bodies = Vec::with_capacity(self.near.len());
        let mut brushes = Vec::with_capacity(self.near.len() * 2);
        for &id in &self.near {
            let Some(c) = citizens.citizen(id, time) else {
                continue;
            };
            let f = c.facing.normalize_or(DVec2::Y);
            let yaw = f.x.atan2(f.y);
            bodies.push(
                Place::new(c.pos.x as f32, c.pos.y as f32, c.pos.z as f32)
                    .with_yaw_deg(yaw.to_degrees() as f32),
            );
            // Local +x is (f.y, -f.x) in the ground plane.
            let side = DVec2::new(f.y, -f.x);
            let turn = time * SPIN + (id % 7) as f64;
            for s in [-1.0, 1.0] {
                let q = DVec2::new(c.pos.x, c.pos.z) + side * (BRUSH.0 * s) + f * BRUSH.2;
                // The brushes turn inwards, sweeping dirt to the middle.
                let a = yaw + turn * s;
                brushes.push(
                    Place::new(q.x as f32, (c.pos.y + BRUSH.1) as f32, q.y as f32)
                        .with_yaw_deg(a.to_degrees() as f32),
                );
            }
        }
        world.set_lod_places(body, &bodies)?;
        world.set_lod_places(brush, &brushes)?;
        Ok(())
    }

    pub fn clear(&mut self, world: &mut World) {
        for id in [self.body, self.brush].into_iter().flatten() {
            let _ = world.set_lod_places(id, &[]);
        }
        self.near.clear();
        self.refresh = 0.0;
    }
}

fn models() -> Models {
    let three = |f: &dyn Fn(Lod) -> Mesh, d: [f32; 3]| {
        vec![
            LodLevel::new(f(Lod::Near), d[0]),
            LodLevel::new(f(Lod::Mid), d[1]),
            LodLevel::new(f(Lod::Far), d[2]),
        ]
    };
    (
        three(&sweeper, [25.0, 70.0, REACH as f32 + 10.0]),
        three(&brush, [20.0, 50.0, REACH as f32 + 10.0]),
    )
}

fn sides(lod: Lod, near: usize) -> usize {
    lod.pick(near, (near / 2).max(8), 6)
}

/// The robot, facing +z, on y = 0: a cream drum on a red skirt, a chrome
/// waist band, a glass bubble head with a glowing eye, an aerial with a
/// beacon, a hopper on its back and arms down to the brushes.
pub fn sweeper(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let cream = rgb(240, 232, 212);
    let red = rgb(204, 52, 44);
    let chrome = rgb(214, 218, 224);
    let seg = sides(lod, 32);
    let paint = |c| Some((Surf::Plain, c));
    let metal = |c| Some((Surf::Metal, c));
    let glow = |c| Some((Surf::Glow, c));
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.04),
            (0.38, 0.04),
            (0.42, 0.08),
            (0.42, 0.18),
            (0.38, 0.22),
            (0.36, 0.24),
            (0.37, 0.27),
            (0.37, 0.31),
            (0.35, 0.33),
            (0.34, 0.6),
            (0.3, 0.7),
            (0.22, 0.76),
            (0.16, 0.78),
            (0.0, 0.79),
        ],
        seg,
        |k| match k {
            0..=4 => paint(red),
            5..=7 => metal(chrome),
            _ => paint(cream),
        },
    );
    // The glass head with an eye looking ahead.
    let head: Vec<(f64, f64)> = (0..=10)
        .map(|k| {
            let a = PI * k as f64 / 10.0;
            (0.2 * a.sin(), 0.76 + 0.2 - 0.2 * a.cos())
        })
        .collect();
    lathe(&mut b, DVec3::ZERO, DVec3::Y, &head, sides(lod, 24), |_| {
        Some((Surf::Glass, rgb(150, 200, 220)))
    });
    lathe(
        &mut b,
        DVec3::new(0.0, 0.93, 0.185),
        DVec3::Z,
        &[(0.0, 0.0), (0.085, 0.0), (0.075, 0.03), (0.0, 0.04)],
        sides(lod, 16),
        |k| {
            if k == 0 {
                metal(chrome)
            } else {
                glow(rgb(120, 236, 255))
            }
        },
    );
    tube(
        &mut b,
        &[DVec3::Y * 1.14, DVec3::Y * 1.42],
        (0.012, 0.012),
        sides(lod, 8),
        metal(chrome),
    );
    lathe(
        &mut b,
        DVec3::Y * 1.42,
        DVec3::Y,
        &(0..=6)
            .map(|k| {
                let a = PI * k as f64 / 6.0;
                (0.045 * a.sin(), 0.045 - 0.045 * a.cos())
            })
            .collect::<Vec<_>>(),
        sides(lod, 12),
        |_| glow(rgb(255, 170, 60)),
    );
    if lod != Lod::Far {
        // The hopper on its back, the dust in it.
        slab(
            &mut b,
            DVec3::new(0.0, 0.45, -0.38),
            [DVec3::X, DVec3::Y],
            DVec3::new(0.2, 0.17, 0.1),
            red,
        );
        // Arms down to the brushes, and a stripe of chrome dots.
        for s in [-1.0, 1.0] {
            tube(
                &mut b,
                &[
                    DVec3::new(0.24 * s, 0.3, 0.26),
                    DVec3::new(BRUSH.0 * s, 0.2, BRUSH.2 - 0.02),
                    DVec3::new(BRUSH.0 * s, BRUSH.1 + 0.1, BRUSH.2),
                ],
                (0.022, 0.022),
                sides(lod, 8),
                metal(chrome),
            );
        }
        for k in 0..lod.pick(5, 3, 0) {
            let a = PI / 2.0 + (k as f64 - 2.0) * 0.32;
            let p = DVec3::new(0.345 * a.cos(), 0.48, 0.345 * a.sin());
            lathe(
                &mut b,
                p,
                DVec3::new(a.cos(), 0.0, a.sin()),
                &[(0.0, 0.0), (0.03, 0.0), (0.0, 0.025)],
                8,
                |_| {
                    if k == 2 {
                        glow(rgb(255, 90, 70))
                    } else {
                        metal(chrome)
                    }
                },
            );
        }
    }
    b.finish()
}

/// One brush: a red hub and bristle tufts splayed out flat over the
/// ground, turning about its own y.
pub fn brush(lod: Lod) -> Mesh {
    let mut b = MeshBuilder::new();
    let hub = rgb(204, 52, 44);
    let bristle = rgb(206, 176, 110);
    lathe(
        &mut b,
        DVec3::ZERO,
        DVec3::Y,
        &[
            (0.0, 0.02),
            (0.07, 0.02),
            (0.08, 0.06),
            (0.05, 0.12),
            (0.0, 0.13),
        ],
        sides(lod, 14),
        |_| Some((Surf::Plain, hub)),
    );
    let tufts = lod.pick(14, 8, 5);
    for k in 0..tufts {
        let a = TAU * k as f64 / tufts as f64;
        let d = DVec3::new(a.cos(), 0.0, a.sin());
        let side = DVec3::new(-a.sin(), 0.0, a.cos());
        slab(
            &mut b,
            d * 0.13 + DVec3::Y * 0.025 + side * 0.0,
            [d, DVec3::new(0.0, 1.0, 0.0)],
            DVec3::new(0.08, 0.02, 0.012),
            bristle,
        );
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_has_faces() {
        for lod in [Lod::Near, Lod::Mid, Lod::Far] {
            assert!(sweeper(lod).face_count() > 0);
            assert!(brush(lod).face_count() > 0);
        }
    }
}
