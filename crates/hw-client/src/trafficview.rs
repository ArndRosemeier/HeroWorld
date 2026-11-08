//! Traffic on screen: the vehicles near the camera (worked out from the town
//! seed and the game time, see `hw_sim::traffic`), one instanced LOD set per
//! model and paint, moved every frame. Car wheels are their own instances,
//! turning with the distance driven.

use engine::prelude::*;
use glam::{DQuat, DVec2, DVec3};
use hw_gen::urban::props::CarModel;
use hw_sim::traffic::{Traffic, Vehicle, VehicleKind};
use std::collections::BTreeMap;

/// Vehicles are shown within this distance of the camera...
pub const SHOW_RADIUS: f64 = 700.0;
/// ...and at most this many at once (the nearest).
pub const MAX_SHOWN: usize = 700;
/// Wheels are turned only on cars this close.
const WHEEL_RADIUS: f64 = 120.0;

#[derive(Default)]
pub struct TrafficView {
    bodies: BTreeMap<VehicleKind, LodId>,
    wheels: BTreeMap<CarModel, LodId>,
    /// Who is near enough to show (looked up twice a second, placed every
    /// frame).
    near: Vec<u64>,
    refresh: f64,
    /// Cars wrecked by heroes: as they stand, when it happened, and the
    /// smoke rising from them (wrecked while someone watched).
    wrecks: BTreeMap<u64, (Vehicle, f64, Option<EmitterId>)>,
}

impl TrafficView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes every vehicle off the streets (another town comes).
    pub fn clear(&mut self, world: &mut World) {
        for (_, id) in std::mem::take(&mut self.bodies) {
            world.despawn_lod(id);
        }
        for (_, id) in std::mem::take(&mut self.wheels) {
            world.despawn_lod(id);
        }
        self.near.clear();
        self.refresh = 0.0;
        for (_, (_, _, smoke)) in std::mem::take(&mut self.wrecks) {
            if let Some(e) = smoke {
                let _ = world.particles_mut().stop(e);
            }
        }
    }

    /// Car `id` wrecked at `tick` by a blow going `fell` (a yaw byte): it
    /// stops dented and askew, its lane stands still behind it; `live`:
    /// sparks fly and it smokes.
    pub fn wreck(
        &mut self,
        world: &mut World,
        traffic: &mut Traffic,
        id: u64,
        tick: u64,
        fell: u8,
        live: bool,
    ) -> EngineResult<()> {
        let time = tick as f64 * hw_sim::DT;
        let Some(mut v) = traffic.vehicle(id, time) else {
            return Ok(());
        };
        // Knocked round a little, away from the blow, and leaning.
        let blow = hw_sim::props::byte_to_yaw(fell);
        let (yaw, _) = v.yaw_pitch();
        let side = (blow - yaw).sin().signum();
        let turn = side * (0.25 + (id % 7) as f64 * 0.04);
        let (s, c) = (turn.sin(), turn.cos());
        v.dir = DVec3::new(
            v.dir.x * c + v.dir.z * s,
            v.dir.y,
            -v.dir.x * s + v.dir.z * c,
        );
        v.roll = side * 0.12;
        v.speed = 0.0;
        let mut smoke = None;
        if live {
            let at = v.pos.as_vec3() + Vec3::Y * 0.9;
            let dir = Vec3::new(blow.sin() as f32, 0.6, blow.cos() as f32) * 6.0;
            let p = world.particles_mut();
            p.burst(ParticleEmitter::sparks(at, dir, 60))?;
            smoke = p.start(ParticleEmitter::smoke(at, 1.4)).ok();
        }
        if let Some((_, _, Some(old))) = self.wrecks.insert(id, (v, time, smoke)) {
            let _ = world.particles_mut().stop(old);
        }
        traffic.set_stops(self.wrecks.iter().map(|(id, w)| (*id, w.1)));
        self.refresh = 0.0;
        Ok(())
    }

    /// Wrecks towed away: their lanes drive on.
    pub fn clear_wreck(&mut self, world: &mut World, traffic: &mut Traffic, id: u64) {
        if let Some((_, _, Some(e))) = self.wrecks.remove(&id) {
            let _ = world.particles_mut().stop(e);
        }
        traffic.set_stops(self.wrecks.iter().map(|(id, w)| (*id, w.1)));
        self.refresh = 0.0;
    }

    /// How many vehicles are on screen.
    pub fn shown(&self) -> usize {
        self.near.len()
    }

    pub fn update(
        &mut self,
        world: &mut World,
        traffic: &Traffic,
        eye: DVec2,
        time: f64,
        dt: f64,
    ) -> EngineResult<()> {
        self.refresh -= dt;
        if self.refresh <= 0.0 {
            self.refresh = 0.5;
            // A little further than shown: they drive in during the half second.
            self.near = traffic
                .near(eye, SHOW_RADIUS + 15.0, time, MAX_SHOWN)
                .into_iter()
                .map(|v| v.id)
                .collect();
        }
        let mut bodies: BTreeMap<VehicleKind, Vec<Place>> = BTreeMap::new();
        let mut wheels: BTreeMap<CarModel, Vec<Place>> = BTreeMap::new();
        for (v, _, _) in self.wrecks.values() {
            if DVec2::new(v.pos.x, v.pos.z).distance(eye) > SHOW_RADIUS {
                continue;
            }
            let body = body_place(v);
            bodies.entry(v.kind).or_default().push(Place {
                stretch: Vec3::new(1.03, 0.86, 0.94),
                tint: Color::rgb(92, 86, 82),
                ..body
            });
            if let VehicleKind::Car(model, _) = v.kind {
                wheel_places(v, model, wheels.entry(model).or_default());
            }
        }
        for &id in &self.near {
            if self.wrecks.contains_key(&id) {
                continue;
            }
            let Some(v) = traffic.vehicle(id, time) else {
                continue;
            };
            bodies.entry(v.kind).or_default().push(body_place(&v));
            if let VehicleKind::Car(model, _) = v.kind {
                if DVec2::new(v.pos.x, v.pos.z).distance(eye) < WHEEL_RADIUS {
                    wheel_places(&v, model, wheels.entry(model).or_default());
                }
            }
        }
        // Every set gets its places, an empty list when none is near.
        for kind in bodies.keys() {
            if !self.bodies.contains_key(kind) {
                let levels = match *kind {
                    VehicleKind::Car(m, paint) => crate::propview::moving_car_levels(m, paint),
                    VehicleKind::SkyCar(paint) => crate::propview::sky_car_levels(paint),
                    VehicleKind::Taxi => crate::propview::taxi_levels(),
                };
                let id = world.spawn_lod(levels)?;
                self.bodies.insert(*kind, id);
            }
        }
        for model in wheels.keys() {
            if !self.wheels.contains_key(model) {
                if let Some(levels) = crate::propview::wheel_levels(*model) {
                    let id = world.spawn_lod(levels)?;
                    self.wheels.insert(*model, id);
                }
            }
        }
        for (kind, id) in &self.bodies {
            let places = bodies.get(kind).map_or(&[][..], |v| &v[..]);
            world.set_lod_places(*id, places)?;
        }
        for (model, id) in &self.wheels {
            let places = wheels.get(model).map_or(&[][..], |v| &v[..]);
            world.set_lod_places(*id, places)?;
        }
        Ok(())
    }
}

/// Engine's turn for a vehicle: yaw, then pitch (positive noses down), then
/// roll (positive lifts the left side, the car's +x).
fn angles(v: &Vehicle) -> (f64, f64, f64) {
    let (yaw, pitch) = v.yaw_pitch();
    (yaw, -pitch, v.roll)
}

pub fn body_place(v: &Vehicle) -> Place {
    let (yaw, pitch, roll) = angles(v);
    Place::new(v.pos.x as f32, v.pos.y as f32, v.pos.z as f32)
        .with_yaw_deg(yaw.to_degrees() as f32)
        .with_pitch_deg(pitch.to_degrees() as f32)
        .with_roll_deg(roll.to_degrees() as f32)
}

fn wheel_places(v: &Vehicle, model: CarModel, out: &mut Vec<Place>) {
    let Some((x, zs, r)) = crate::propview::wheel_layout(model) else {
        return;
    };
    let (yaw, pitch, roll) = angles(v);
    let q =
        DQuat::from_rotation_y(yaw) * DQuat::from_rotation_x(pitch) * DQuat::from_rotation_z(roll);
    // Rolling forward turns the top of the wheel forward.
    let spin = (v.odometer / r).rem_euclid(std::f64::consts::TAU);
    for z in zs {
        for sx in [-x, x] {
            let p = v.pos + q * DVec3::new(sx, r, z);
            out.push(
                Place::new(p.x as f32, p.y as f32, p.z as f32)
                    .with_yaw_deg(yaw.to_degrees() as f32)
                    .with_pitch_deg((pitch + spin).to_degrees() as f32)
                    .with_roll_deg(roll.to_degrees() as f32),
            );
        }
    }
}
