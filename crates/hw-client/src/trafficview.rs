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
    /// Cars wrecked by heroes: as they stand, and the smoke rising from
    /// them (wrecked while someone watched).
    wrecks: BTreeMap<u64, (Vehicle, Option<EmitterId>)>,
    /// Wrecked cars are drawn without their doors, which hang open on
    /// their own: shells per model and paint, and each door's set with its
    /// hinge, axis and how far it opens.
    shells: BTreeMap<VehicleKind, LodId>,
    doors: BTreeMap<VehicleKind, Vec<(LodId, DVec3, DVec3, f64)>>,
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
        for (_, id) in std::mem::take(&mut self.shells) {
            world.despawn_lod(id);
        }
        for (_, doors) in std::mem::take(&mut self.doors) {
            for (id, ..) in doors {
                world.despawn_lod(id);
            }
        }
        self.near.clear();
        self.refresh = 0.0;
        for (_, (_, smoke)) in std::mem::take(&mut self.wrecks) {
            if let Some(e) = smoke {
                let _ = world.particles_mut().stop(e);
            }
        }
    }

    /// Car `id` wrecked by a blow going `fell` (a yaw byte), standing
    /// `along` its lane: it stops dented and askew, the lane queues behind
    /// it; `live`: sparks fly and it smokes.
    pub fn wreck(
        &mut self,
        world: &mut World,
        traffic: &mut Traffic,
        id: u64,
        fell: u8,
        along: f32,
        live: bool,
    ) -> EngineResult<()> {
        traffic.place_wreck(id, along);
        let Some(mut v) = traffic.vehicle(id, traffic.time()) else {
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
        if let Some((_, Some(old))) = self.wrecks.insert(id, (v, smoke)) {
            let _ = world.particles_mut().stop(old);
        }
        self.refresh = 0.0;
        Ok(())
    }

    /// Wrecks towed away: their lanes drive on.
    pub fn clear_wreck(&mut self, world: &mut World, traffic: &mut Traffic, id: u64) {
        if let Some((_, Some(e))) = self.wrecks.remove(&id) {
            let _ = world.particles_mut().stop(e);
        }
        traffic.clear_wreck(id);
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
        let mut shells: BTreeMap<VehicleKind, Vec<Place>> = BTreeMap::new();
        let mut doors: BTreeMap<(VehicleKind, usize), Vec<Place>> = BTreeMap::new();
        for (&id, (v, _)) in &self.wrecks {
            if DVec2::new(v.pos.x, v.pos.z).distance(eye) > SHOW_RADIUS {
                continue;
            }
            let crumpled = |p: Place| Place {
                stretch: WRECK_STRETCH,
                tint: Color::rgb(92, 86, 82),
                ..p
            };
            let body = crumpled(body_place(v));
            let VehicleKind::Car(model, paint) = v.kind else {
                bodies.entry(v.kind).or_default().push(body);
                continue;
            };
            wheel_places(v, model, wheels.entry(model).or_default());
            shells.entry(v.kind).or_default().push(body);
            // The doors sprung by the crash: hanging part open, each wreck
            // its own way.
            if let std::collections::btree_map::Entry::Vacant(e) = self.doors.entry(v.kind) {
                let mut sets = Vec::new();
                for (levels, hinge, axis, open) in crate::propview::car_door_levels(model, paint) {
                    sets.push((world.spawn_lod(levels)?, hinge, axis, open));
                }
                e.insert(sets);
            }
            let (yaw, pitch, roll) = angles(v);
            let q = DQuat::from_rotation_y(yaw)
                * DQuat::from_rotation_x(pitch)
                * DQuat::from_rotation_z(roll);
            for (k, &(_, hinge, axis, open)) in self.doors[&v.kind].iter().enumerate() {
                let ajar = open * (0.25 + ((id >> (k * 3)) % 5) as f64 * 0.12);
                let at = v.pos + q * (hinge * WRECK_STRETCH.as_dvec3());
                let turn = q * DQuat::from_axis_angle(axis.normalize(), ajar);
                let (y, x, z) = turn.to_euler(glam::EulerRot::YXZ);
                doors.entry((v.kind, k)).or_default().push(crumpled(
                    Place::new(at.x as f32, at.y as f32, at.z as f32)
                        .with_yaw_deg(y.to_degrees() as f32)
                        .with_pitch_deg(x.to_degrees() as f32)
                        .with_roll_deg(z.to_degrees() as f32),
                ));
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
        for kind in shells.keys() {
            if let (false, VehicleKind::Car(model, paint)) = (self.shells.contains_key(kind), *kind)
            {
                let id = world.spawn_lod(shell_levels(model, paint))?;
                self.shells.insert(*kind, id);
            }
        }
        for (kind, id) in &self.shells {
            let places = shells.get(kind).map_or(&[][..], |v| &v[..]);
            world.set_lod_places(*id, places)?;
        }
        for (kind, sets) in &self.doors {
            for (k, (id, ..)) in sets.iter().enumerate() {
                let places = doors.get(&(*kind, k)).map_or(&[][..], |v| &v[..]);
                world.set_lod_places(*id, places)?;
            }
        }
        Ok(())
    }
}

/// A wreck is squashed this much (x, y, z).
const WRECK_STRETCH: Vec3 = Vec3::new(1.03, 0.86, 0.94);

/// A car without its doors and wheels (both drawn on their own).
fn shell_levels(model: CarModel, paint: u8) -> Vec<LodLevel> {
    use crate::carmesh::Lod;
    let make = |lod: Lod| {
        let mut b = crate::citymesh::MeshBuilder::new();
        crate::carmesh::car_shell_into(&mut b, model, paint, lod, false);
        b.finish()
    };
    vec![
        LodLevel::new(make(Lod::Near), 35.0),
        LodLevel::new(make(Lod::Mid), 140.0),
        LodLevel::new(make(Lod::Far), 650.0),
    ]
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
