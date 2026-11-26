//! A gang's getaway car on screen (see [`hw_sim::missions::getaway`]): the
//! hover car in the gangs' black and blood red, where the server has it.
//!
//! At the kerb its canopy stands open while the bagmen load it. In the air
//! the canopy is shut, it leans into its turns and noses down with its
//! speed, and the more it has been hit the more it smokes. Brought down it
//! lies on its belly, tipped over on one side, canopy up, smoking.
//!
//! The model is the street's own hover car ([`crate::carmesh`]); what a
//! hero bumps into is [`hw_sim::missions::getaway::car_solid`], a parked
//! hover car's box.

use crate::carmesh::{car_doors, car_shell_into, Lod};
use crate::citymesh::MeshBuilder;
use engine::particles::EmitterId;
use engine::prelude::*;
use hw_core::NetId;
use hw_gen::urban::props::{CarModel, GANG_PAINT};
use hw_net::{EntityKind, EntityState, Status};
use std::collections::BTreeMap;

/// How far the hull hangs over the street when it flies or stands (m):
/// brought down, it lies this much lower.
const HOVER_GAP: f32 = 0.62;
/// Seconds the canopy takes to swing.
const CANOPY_SECONDS: f32 = 0.9;
/// How far it leans into a turn (degrees per rad/s), and at most.
const LEAN: f32 = 14.0;
const LEAN_MOST: f32 = 24.0;

struct Shown {
    body: EntityId,
    canopy: EntityId,
    open: f32,
    /// Its heading last frame and the lean it has now (degrees).
    yaw: f32,
    lean: f32,
    /// How far down on its belly it is, 0 to 1.
    down: f32,
    smoke: Option<(EmitterId, u8)>,
}

/// The canopy's mesh, hinge, axis and swing (radians).
type Canopy = (Mesh, Vec3, Vec3, f32);

#[derive(Default)]
pub struct GetawayView {
    shown: BTreeMap<NetId, Shown>,
    meshes: Option<(Mesh, Canopy)>,
}

/// The car's two meshes: the hull without its canopy, and the canopy
/// about its hinge. A few thousand triangles, built once (the first time
/// a car is near) in a couple of milliseconds: there is one car to a case.
fn meshes() -> (Mesh, Canopy) {
    let mut b = MeshBuilder::new();
    car_shell_into(&mut b, CarModel::Hover, GANG_PAINT, Lod::Near, false);
    let door = car_doors(CarModel::Hover, GANG_PAINT, Lod::Near).remove(0);
    (
        b.finish(),
        (
            door.mesh,
            door.hinge.as_vec3(),
            door.axis.as_vec3(),
            door.open as f32,
        ),
    )
}

/// Is this car brought down?
pub fn wrecked(e: &EntityState) -> bool {
    e.combat
        .as_ref()
        .is_some_and(|c| c.status == Status::Down || c.health == 0)
}

impl GetawayView {
    /// Draws every getaway car in `all` where the server has it, and
    /// takes away the ones that are gone.
    pub fn update(&mut self, world: &mut World, all: &[EntityState], dt: f32) -> EngineResult<()> {
        let cars: Vec<&EntityState> = all
            .iter()
            .filter(|e| e.kind == EntityKind::Getaway)
            .collect();
        let gone: Vec<NetId> = self
            .shown
            .keys()
            .filter(|id| !cars.iter().any(|c| c.id == **id))
            .copied()
            .collect();
        for id in gone {
            if let Some(s) = self.shown.remove(&id) {
                if let Some((e, _)) = s.smoke {
                    let _ = world.particles_mut().stop(e);
                }
                world.despawn(s.canopy);
                world.despawn(s.body);
            }
        }
        for c in cars {
            let yaw = hw_sim::robots::rotation_yaw(c.rot) as f32;
            let pos = Vec3::new(c.pos[0] as f32, c.pos[1] as f32, c.pos[2] as f32);
            let vel = Vec3::from_array(c.vel);
            if !self.shown.contains_key(&c.id) {
                let (body, canopy) = self.meshes.get_or_insert_with(meshes);
                let hinge = canopy.1;
                let body = world.place(body.clone(), Place::new(pos.x, pos.y, pos.z))?;
                let lid = world.place(canopy.0.clone(), Place::new(0.0, 0.0, 0.0))?;
                world.attach(lid, body, Place::new(hinge.x, hinge.y, hinge.z))?;
                self.shown.insert(
                    c.id,
                    Shown {
                        body,
                        canopy: lid,
                        open: 0.0,
                        yaw,
                        lean: 0.0,
                        down: if wrecked(c) && c.grounded { 1.0 } else { 0.0 },
                        smoke: None,
                    },
                );
            }
            let Some((_, canopy)) = self.meshes.as_ref() else {
                continue;
            };
            let (hinge, axis, swing) = (canopy.1, canopy.2, canopy.3);
            let Some(s) = self.shown.get_mut(&c.id) else {
                continue;
            };
            let wreck = wrecked(c);
            // Leaning into the turn, nose down with its speed.
            let mut turn = yaw - s.yaw;
            turn = (turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            s.yaw = yaw;
            let rate = if dt > 1e-4 { turn / dt } else { 0.0 };
            let flying = !c.grounded;
            let want = if flying && !wreck {
                (-rate * LEAN).clamp(-LEAN_MOST, LEAN_MOST)
            } else {
                0.0
            };
            s.lean += (want - s.lean) * (dt * 3.0).min(1.0);
            let speed = Vec3::new(vel.x, 0.0, vel.z).length();
            let nose = if flying { (speed * 0.5).min(7.0) } else { 0.0 } + vel.y.min(0.0) * -1.5;
            // On its belly once it is down, tipped over to one side.
            let lying = wreck && c.grounded;
            s.down = (s.down + if lying { dt * 3.0 } else { -dt * 3.0 }).clamp(0.0, 1.0);
            let side = if c.id.0 % 2 == 0 { 1.0 } else { -1.0 };
            let place = Place::new(pos.x, pos.y - HOVER_GAP * s.down, pos.z)
                .with_yaw_deg(yaw.to_degrees())
                .with_pitch_deg(nose.min(25.0) + 3.0 * s.down)
                .with_roll_deg(s.lean + side * 9.0 * s.down);
            world.set_place(s.body, place)?;
            // Open at the kerb for the sacks, and where its crew got out.
            let step = dt / CANOPY_SECONDS;
            s.open = if c.grounded {
                (s.open + step).min(1.0)
            } else {
                (s.open - step * 2.0).max(0.0)
            };
            let eased = s.open * s.open * (3.0 - 2.0 * s.open);
            let q = Quat::from_axis_angle(axis.normalize_or(Vec3::X), eased * swing);
            let (y, x, z) = q.to_euler(glam::EulerRot::YXZ);
            world.set_place(
                s.canopy,
                Place::new(hinge.x, hinge.y, hinge.z)
                    .with_yaw_deg(y.to_degrees())
                    .with_pitch_deg(x.to_degrees())
                    .with_roll_deg(z.to_degrees()),
            )?;
            // Smoke from the engine at the back: none while it is whole,
            // thicker with every quarter of its health gone.
            let health = c.combat.as_ref().map_or(255, |k| k.health);
            let level: u8 = match health {
                192..=255 => 0,
                128..=191 => 1,
                64..=127 => 2,
                _ => 3,
            };
            let back = Quat::from_rotation_y(yaw) * Vec3::new(0.0, 1.0 - HOVER_GAP * s.down, -1.9);
            let at = pos + back;
            if s.smoke.is_some_and(|(_, l)| l != level) {
                if let Some((e, _)) = s.smoke.take() {
                    let _ = world.particles_mut().stop(e);
                }
            }
            match (&s.smoke, level) {
                (_, 0) => {}
                (Some((e, _)), _) => {
                    let _ = world.particles_mut().set_emitter_position(*e, at);
                }
                (None, l) => {
                    let puff = ParticleEmitter::smoke(at, 0.35 + 0.3 * l as f32);
                    s.smoke = world.particles_mut().start(puff).ok().map(|e| (e, l));
                }
            }
        }
        Ok(())
    }

    /// Where the first car is and which way it points (for a close-up).
    pub fn first(&self, all: &[EntityState]) -> Option<(Vec3, f32)> {
        all.iter()
            .filter(|e| e.kind == EntityKind::Getaway)
            .min_by_key(|e| e.id)
            .map(|e| {
                (
                    Vec3::new(e.pos[0] as f32, e.pos[1] as f32, e.pos[2] as f32),
                    hw_sim::robots::rotation_yaw(e.rot) as f32,
                )
            })
    }
}
