//! People in cars: a driver behind the wheel of every car, hover car and
//! flying taxi near the camera, and in some a passenger or a few, seen
//! through the glass. They are agents of the citizens' crowds (one crowd
//! per look, see `crowdview`) held in a seated pose, so they cost no
//! models of their own: far ones are drawn instanced, the nearest become
//! full characters. Who rides in which car follows from the car's id, so
//! the same car always carries the same people.
//!
//! When a hero wrecks a car the people in it sit stunned for a moment,
//! climb out of the door on their side and run off along the nearest
//! pavement, away from the trouble, then stop and look back.

use crate::crowdview::CrowdView;
use engine::prelude::*;
use glam::{DQuat, DVec2, DVec3};
use hw_gen::urban::props::CarModel;
use hw_sim::citizens::{Citizens, Escape, LOOKS};
use hw_sim::traffic::{Vehicle, VehicleKind};
use std::collections::{BTreeMap, BTreeSet};

/// People in cars are shown within this distance of the camera: the
/// cars' far models (from 120 m) have no cabins, and the glass hides them
/// that far off anyway.
pub const RIDER_RADIUS: f64 = 110.0;
/// At most this many riders at once (the nearest cars' first).
pub const MAX_RIDERS: usize = 160;
/// Hero taxis' ids carry this bit (they are server entities, not traffic).
pub const CAB_BIT: u64 = 1 << 62;
/// The poses (see `poses::add_seated_clips`).
pub const DRIVE: &str = "drive";
pub const RIDE: &str = "ride";

/// Seconds a rider sits stunned after the crash (the most; each its own).
const STUNNED: f64 = 1.4;
/// Seconds to climb out of the door.
const CLIMB: f64 = 0.9;
/// Running pace (m/s; the walk played fast).
const RUN: f64 = 2.9;
/// How far they run along the pavement at most (m).
const RUN_MAX: f64 = 45.0;
/// How high a standing person's hips are, roughly: climbing out ends
/// with the hips here over the ground, where the standing clips take over.
const STANDING_HIPS: f64 = 0.92;
/// Runners are let go when this far from the camera, or after the wreck
/// is towed and they are a little way off.
const RUNNER_GONE: f64 = 260.0;
const RUNNER_GONE_TOWED: f64 = 60.0;
/// At most this many runners (the oldest go first).
const MAX_RUNNERS: usize = 48;

/// Who sits in vehicle `v`: (seat index into `carmesh::seats`, look), the
/// driver first. The same car always carries the same people.
///
/// `sits` says whether a look fits in a car seat (see [`fits_seats`]),
/// `None` while it is not built. A seat whose rider's look does not fit
/// takes the next look of its run; one whose look is not built yet stays
/// empty for now (so the car carries the same people once all are built).
pub fn riders(v: &Vehicle, sits: impl Fn(u32) -> Option<bool>) -> Vec<(usize, u32)> {
    let seed = hw_core::Seed(v.id).child("riders");
    let look = |k: u64| {
        for j in 0..8u64 {
            let l = (seed.child(k).child(j).0 % LOOKS as u64) as u32;
            match sits(l) {
                Some(true) => return Some(l),
                Some(false) => continue,
                None => return None,
            }
        }
        None
    };
    let r = seed.child("n").unit();
    let model = model_of(v.kind);
    let seats = crate::carmesh::seats(model).len();
    // Passenger seats in the order they fill: beside the driver, then
    // behind. Taxi fares sit in the back.
    let (order, n): (Vec<usize>, usize) = if v.id & CAB_BIT != 0 {
        (vec![], 0)
    } else if v.kind == VehicleKind::Taxi {
        let n = if r < 0.35 {
            0
        } else if r < 0.8 {
            1
        } else {
            2
        };
        (vec![3, 2], n)
    } else {
        let n = if r < 0.55 {
            0
        } else if r < 0.82 {
            1
        } else if r < 0.94 {
            2
        } else {
            3
        };
        ((1..seats).collect(), n)
    };
    let mut out: Vec<(usize, u32)> = look(0).map(|l| (0, l)).into_iter().collect();
    for (k, &s) in order.iter().take(n).enumerate() {
        if s < seats {
            if let Some(l) = look(1 + k as u64) {
                out.push((s, l));
            }
        }
    }
    out
}

/// Whether a seated citizen's `model` fits every car seat: nothing of it
/// (a long skirt hanging off the knees, say) goes through a car's floor.
/// Worked out once per look, where it is built.
pub fn fits_seats(model: &engine::anim::AnimatedModel) -> bool {
    use crate::carmesh::{floor_under, seats};
    [DRIVE, RIDE].iter().all(|clip| {
        let Some(c) = model.clips.iter().position(|c| c.name == *clip) else {
            return false;
        };
        let skin = engine::anim::sample_joint_matrices(model, c, 0.0);
        model.meshes.iter().all(|mesh| {
            mesh.positions
                .iter()
                .zip(mesh.joints.iter().zip(&mesh.weights))
                .all(|(p, (j, w))| {
                    let mut at = glam::Vec3::ZERO;
                    for k in 0..4 {
                        if w[k] > 0.0 {
                            at += w[k] * skin[j[k] as usize].transform_point3(*p);
                        }
                    }
                    [CarModel::Cruiser, CarModel::Bubble, CarModel::Hover]
                        .iter()
                        .all(|&m| {
                            seats(m).iter().all(|s| {
                                s.hip.y + at.y as f64 > floor_under(m, s.hip.z + at.z as f64)
                            })
                        })
                })
        })
    })
}

/// The body a vehicle is built on.
pub fn model_of(kind: VehicleKind) -> CarModel {
    match kind {
        VehicleKind::Car(m, _) => m,
        VehicleKind::SkyCar(_) | VehicleKind::Taxi => CarModel::Hover,
    }
}

/// Where seat `seat` of `v` puts its sitter's hips, turned with the car.
pub fn seat_place(v: &Vehicle, seat: usize) -> Option<Place> {
    let s = *crate::carmesh::seats(model_of(v.kind)).get(seat)?;
    let (yaw, pitch, roll) = crate::trafficview::angles(v);
    let q =
        DQuat::from_rotation_y(yaw) * DQuat::from_rotation_x(pitch) * DQuat::from_rotation_z(roll);
    let p = v.pos + q * s.hip;
    Some(
        Place::new(p.x as f32, p.y as f32, p.z as f32)
            .with_yaw_deg(yaw.to_degrees() as f32)
            .with_pitch_deg(pitch.to_degrees() as f32)
            .with_roll_deg(roll.to_degrees() as f32),
    )
}

struct Shown {
    look: u32,
    agent: CrowdAgentId,
}

/// Someone who got out of a wreck.
struct Runner {
    look: u32,
    agent: CrowdAgentId,
    /// The wreck it came from.
    car: u64,
    /// Where it sat (hips), and the seat's pose.
    seat: Place,
    pose: &'static str,
    /// Hips just outside its door, at standing height.
    door: DVec3,
    /// The way out of the road and along the pavement.
    escape: Option<Escape>,
    /// Seconds since the crash, and how long it sat stunned.
    t: f64,
    stunned: f64,
    /// Where the trouble was: it looks back there once it stops.
    trouble: DVec2,
    towed: bool,
}

#[derive(Default)]
pub struct RiderView {
    shown: BTreeMap<(u64, usize), Shown>,
    runners: Vec<Runner>,
}

impl RiderView {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many people sit in cars on screen, and how many are running
    /// from wrecks.
    pub fn counts(&self) -> (usize, usize) {
        (self.shown.len(), self.runners.len())
    }

    /// Takes everyone out (another town or square comes).
    pub fn clear(&mut self, world: &mut World, crowd: &CrowdView) {
        for (_, s) in std::mem::take(&mut self.shown) {
            remove(world, crowd, s.look, s.agent);
        }
        for r in std::mem::take(&mut self.runners) {
            remove(world, crowd, r.look, r.agent);
        }
    }

    /// Seats everyone in `cars` (the vehicles drawn near the camera, as
    /// they are now; the nearest first) and moves the runners on.
    pub fn update(
        &mut self,
        world: &mut World,
        crowd: &CrowdView,
        cars: &[Vehicle],
        citizens: &Citizens,
        eye: DVec3,
        dt: f64,
    ) -> EngineResult<()> {
        let mut seen = BTreeSet::new();
        for v in cars {
            if seen.len() >= MAX_RIDERS {
                break;
            }
            if v.pos.distance(eye) > RIDER_RADIUS {
                continue;
            }
            for (seat, look) in riders(v, |l| crowd.sits(l)) {
                let Some(id) = crowd.crowd_of(look) else {
                    continue;
                };
                let Some(place) = seat_place(v, seat) else {
                    continue;
                };
                let key = (v.id, seat);
                match self.shown.get(&key) {
                    Some(s) => world.set_crowd_agent(id, s.agent, place, 0.0)?,
                    None => {
                        let agent = world.add_crowd_agent(id, place)?;
                        let pose = if seat == 0 { DRIVE } else { RIDE };
                        world.set_crowd_agent_pose(id, agent, Some(pose))?;
                        self.shown.insert(key, Shown { look, agent });
                    }
                }
                seen.insert(key);
            }
        }
        let gone: Vec<(u64, usize)> = (self.shown.keys())
            .filter(|k| !seen.contains(k))
            .copied()
            .collect();
        for k in gone {
            if let Some(s) = self.shown.remove(&k) {
                remove(world, crowd, s.look, s.agent);
            }
        }
        self.run(world, crowd, citizens, DVec2::new(eye.x, eye.z), dt)
    }

    /// Car `wreck` (as it stands) was wrecked: by a blow going `blow` (a
    /// yaw) just now, the people in it get out and run; long ago (`None`)
    /// or out of sight (of `eye`), it is simply empty.
    pub fn bail_out(
        &mut self,
        world: &mut World,
        crowd: &CrowdView,
        citizens: &Citizens,
        wreck: &Vehicle,
        blow: Option<f64>,
        eye: DVec3,
    ) -> EngineResult<()> {
        let mine: Vec<(u64, usize)> = (self.shown.keys())
            .filter(|k| k.0 == wreck.id)
            .copied()
            .collect();
        let mut taken = BTreeMap::new();
        for k in mine {
            if let Some(s) = self.shown.remove(&k) {
                taken.insert(k.1, s);
            }
        }
        let blow = match blow {
            Some(b) if !taken.is_empty() || wreck.pos.distance(eye) <= RIDER_RADIUS => b,
            _ => {
                for (_, s) in taken {
                    remove(world, crowd, s.look, s.agent);
                }
                return Ok(());
            }
        };
        let model = model_of(wreck.kind);
        let seats = crate::carmesh::seats(model);
        let at = DVec2::new(wreck.pos.x, wreck.pos.z);
        let trouble = at - DVec2::new(blow.sin(), blow.cos()) * 4.0;
        let (yaw, pitch, roll) = crate::trafficview::angles(wreck);
        let q = DQuat::from_rotation_y(yaw)
            * DQuat::from_rotation_x(pitch)
            * DQuat::from_rotation_z(roll);
        let ground = wreck.pos.y;
        for (seat, look) in riders(wreck, |l| crowd.sits(l)) {
            let Some(id) = crowd.crowd_of(look) else {
                continue;
            };
            let Some(place) = seat_place(wreck, seat) else {
                continue;
            };
            let agent = match taken.remove(&seat) {
                Some(s) => s.agent,
                None => {
                    let a = world.add_crowd_agent(id, place)?;
                    world.set_crowd_agent_pose(
                        id,
                        a,
                        Some(if seat == 0 { DRIVE } else { RIDE }),
                    )?;
                    a
                }
            };
            // Out of the door on its side, or the driver's for the
            // middle of a bench.
            let hip = seats[seat].hip;
            let side = if hip.x >= 0.0 { 1.0 } else { -1.0 };
            let out = DVec3::new(side * (car_half_width(model) + 0.45), 0.0, hip.z);
            let door = wreck.pos + q * out;
            let door = DVec3::new(door.x, ground + STANDING_HIPS, door.z);
            let escape = citizens.escape(DVec2::new(door.x, door.z), trouble, RUN_MAX);
            let r = hw_core::Seed(wreck.id)
                .child("stunned")
                .child(seat as u64)
                .unit();
            self.runners.push(Runner {
                look,
                agent,
                car: wreck.id,
                seat: place,
                pose: if seat == 0 { DRIVE } else { RIDE },
                door,
                escape,
                t: 0.0,
                stunned: STUNNED * (0.35 + 0.65 * r),
                trouble,
                towed: false,
            });
        }
        for (_, s) in taken {
            remove(world, crowd, s.look, s.agent);
        }
        while self.runners.len() > MAX_RUNNERS {
            let r = self.runners.remove(0);
            remove(world, crowd, r.look, r.agent);
        }
        Ok(())
    }

    /// Wreck `car` towed away: its runners go once out of the way.
    pub fn towed(&mut self, car: u64) {
        for r in self.runners.iter_mut().filter(|r| r.car == car) {
            r.towed = true;
        }
    }

    fn run(
        &mut self,
        world: &mut World,
        crowd: &CrowdView,
        citizens: &Citizens,
        eye: DVec2,
        dt: f64,
    ) -> EngineResult<()> {
        let mut keep = Vec::with_capacity(self.runners.len());
        for mut r in std::mem::take(&mut self.runners) {
            r.t += dt;
            let Some(id) = crowd.crowd_of(r.look) else {
                continue;
            };
            let (place, speed, pose) = runner_at(&r, citizens);
            let far = DVec2::new(place.position.x as f64, place.position.z as f64).distance(eye);
            if far > RUNNER_GONE || (r.towed && far > RUNNER_GONE_TOWED) {
                remove(world, crowd, r.look, r.agent);
                continue;
            }
            world.set_crowd_agent_pose(id, r.agent, pose)?;
            world.set_crowd_agent(id, r.agent, place, speed as f32)?;
            keep.push(r);
        }
        self.runners = keep;
        Ok(())
    }
}

/// Where a runner is now: its place, speed and pose.
fn runner_at(r: &Runner, citizens: &Citizens) -> (Place, f64, Option<&'static str>) {
    if r.t < r.stunned {
        return (r.seat, 0.0, Some(r.pose));
    }
    let seat = r.seat.position.as_dvec3();
    let out = DVec2::new(r.door.x - seat.x, r.door.z - seat.z);
    let face = out.normalize_or(DVec2::Y);
    let climb = (r.t - r.stunned) / CLIMB;
    if climb < 1.0 {
        // Still seated, sliding out over the sill and up onto its feet:
        // the hips go from the seat to standing height outside the door.
        let e = climb * climb * (3.0 - 2.0 * climb);
        let p = seat.lerp(r.door, e);
        let yaw = r.seat.yaw_degrees as f64 + 90.0 * e * side_of(r.seat, face);
        let place = Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw as f32);
        return (place, 0.0, Some(r.pose));
    }
    // On its feet: the standing clips' origin is the ground under it.
    let ran = (r.t - r.stunned - CLIMB) * RUN;
    let ground = |p: DVec3| DVec3::new(p.x, p.y - STANDING_HIPS, p.z);
    let (p, dir, speed) = match &r.escape {
        Some(e) if ran < e.len() => {
            let (p, d) = citizens.escape_at(e, ran);
            (p, d, RUN)
        }
        Some(e) => {
            let (p, _) = citizens.escape_at(e, e.len());
            let back = (r.trouble - DVec2::new(p.x, p.z)).normalize_or(face);
            (p, back, 0.0)
        }
        // Nowhere to run: a few steps away from the car, then a look back.
        None => {
            let step = ran.min(4.0);
            let p = ground(r.door) + DVec3::new(face.x, 0.0, face.y) * step;
            let d = if ran < 4.0 { face } else { -face };
            (p, d, if ran < 4.0 { RUN * 0.6 } else { 0.0 })
        }
    };
    let yaw = d_yaw(dir);
    let place = Place::new(p.x as f32, p.y as f32, p.z as f32).with_yaw_deg(yaw as f32);
    (place, speed, None)
}

/// Which way (+1 left, -1 right) a sitter facing `seat` turns to face out
/// of the door the way `out` goes.
fn side_of(seat: Place, out: DVec2) -> f64 {
    let yaw = (seat.yaw_degrees as f64).to_radians();
    let left = DVec2::new(yaw.cos(), -yaw.sin());
    // A larger yaw turns towards the left.
    if out.dot(left) >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// The yaw (degrees) that faces `dir` (x, z).
fn d_yaw(dir: DVec2) -> f64 {
    hw_core::det::atan2(dir.x, dir.y).to_degrees()
}

/// Half a body's width at the seats (m).
fn car_half_width(model: CarModel) -> f64 {
    match model {
        CarModel::Cruiser => 1.0,
        CarModel::Bubble => 0.84,
        CarModel::Hover => 0.95,
    }
}

fn remove(world: &mut World, crowd: &CrowdView, look: u32, agent: CrowdAgentId) {
    if let Some(id) = crowd.crowd_of(look) {
        world.remove_crowd_agent(id, agent);
    }
}
